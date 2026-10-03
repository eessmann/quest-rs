//! Coherent QSVT on a borrowed MPI subgroup.
//!
//! Every operation is called in the same order on all participating ranks. Results expose masses and collective
//! conditioning; they never gather or export a distributed state.
use super::hadamard::{AdmittedOverlap, MissingVector, SuppliedVector};
use super::{
	AdmittedTransform, Error, MassObservation, MissingTransform, Result, SuppliedTransform,
	ValidatedTransform,
};
use crate::collective::{CollectiveEnvironment, CollectiveRegister, equal};
use crate::{Complex64, error::BackendResult};
use quest_sys::mpi::MpiCollectiveLane;
use std::io::Write as _;
mod payload;

/// Supply an admitted transform before requesting native preparation.
/// ```compile_fail
/// # fn f(env: &quest::collective::CollectiveEnvironment<'_, '_>) {
/// env.qsvt().prepare();
/// # }
/// ```
pub struct PreparationBuilder<'env, 'comm, 'runtime, T = MissingTransform> {
	environment: &'env CollectiveEnvironment<'comm, 'runtime>,
	transform: T,
}
impl<'comm, 'runtime> CollectiveEnvironment<'comm, 'runtime> {
	#[must_use]
	pub const fn qsvt(&self) -> PreparationBuilder<'_, 'comm, 'runtime> {
		PreparationBuilder {
			environment: self,
			transform: MissingTransform,
		}
	}
}
impl<'env, 'comm, 'runtime> PreparationBuilder<'env, 'comm, 'runtime> {
	#[must_use]
	pub const fn transform(
		self,
		transform: ValidatedTransform,
	) -> PreparationBuilder<'env, 'comm, 'runtime, SuppliedTransform> {
		PreparationBuilder {
			environment: self.environment,
			transform: SuppliedTransform(transform),
		}
	}
}
impl<'env, 'comm, 'runtime> PreparationBuilder<'env, 'comm, 'runtime, SuppliedTransform> {
	/// Compare complete transform values and allocation topology, then admit all
	/// programs and projectors before any rank enters native materialization.
	/// # Errors
	/// Rejects differing transforms, insufficient per-rank dense-operation capacity,
	/// or any rank's admission failure. Add idle operand qubits to accommodate
	/// dense operations that exceed the native communication buffer.
	pub fn prepare(self) -> Result<PreparedTransform<'env, 'comm, 'runtime>> {
		let env = self.environment;
		let id = env.identifier();
		let mut lane = env.begin(20, id, 0, 0)?;
		compare(&mut lane, env, &self.transform.0, None)?;
		agree(
			&mut lane,
			capacity(&self.transform.0, env.size()?).map_err(Error::from),
		)?;
		let admission = agree(
			&mut lane,
			AdmittedTransform::new(&env.resources, self.transform.0),
		)?;
		let inner = fatal(|| admission.materialize());
		Ok(PreparedTransform {
			environment: env,
			inner,
			id,
		})
	}
	#[must_use]
	pub fn overlap(self) -> OverlapBuilder<'env, 'comm, 'runtime> {
		OverlapBuilder {
			environment: self.environment,
			transform: self.transform.0,
			input: MissingVector,
			reference: MissingVector,
		}
	}
}
/// Native resources borrow the collective environment and remain thread bound.
/// ```compile_fail
/// # fn f(comm: &quest::collective::MpiCommunicator<'_>, transform: quest::qsvt::model::ValidatedTransform) -> Result<(), Box<dyn std::error::Error>> {
/// let prepared = {
///     let env = quest::collective::CollectiveEnvironment::builder(comm)?.build()?;
///     env.qsvt().transform(transform).prepare()?
/// };
/// let _ = prepared.transform();
/// # Ok(()) }
/// ```
pub struct PreparedTransform<'env, 'comm, 'runtime> {
	environment: &'env CollectiveEnvironment<'comm, 'runtime>,
	inner: super::PreparedTransform<'env>,
	id: u64,
}
impl<'env, 'comm, 'runtime> PreparedTransform<'env, 'comm, 'runtime> {
	#[must_use]
	pub const fn transform(&self) -> &ValidatedTransform {
		self.inner.transform()
	}
	/// # Errors
	/// Rejects inconsistent invocation identities or register admission on any rank.
	pub fn run<'prepared, 'register>(
		&'prepared mut self,
		register: &'register mut CollectiveRegister<'env, 'comm, 'runtime>,
	) -> Result<ExecutionResult<'prepared, 'register, 'env, 'comm, 'runtime>> {
		let mut lane = self.environment.begin(21, self.id, register.id, 0)?;
		let admission = agree(
			&mut lane,
			self.inner.admit_run(&register.inner).map_err(Error::from),
		)?;
		let mass = fatal(|| {
			let result = self.inner.run_admitted(&mut register.inner, admission)?;
			let mass = result.mass();
			let _ = result.release();
			Ok(mass)
		});
		Ok(ExecutionResult {
			register,
			prepared: self,
			mass,
		})
	}
}
/// Exclusive capability tied to this register and prepared transform. Releasing
/// it returns the collective register wrapper, never a local native register.
/// ```compile_fail
/// # fn f<'e,'c,'r>(p: &mut quest::qsvt::collective::PreparedTransform<'e,'c,'r>, r: &mut quest::collective::CollectiveRegister<'e,'c,'r>) {
/// let result = p.run(r).unwrap();
/// r.init_zero().unwrap();
/// result.condition().unwrap();
/// # }
/// ```
pub struct ExecutionResult<'prepared, 'register, 'env, 'comm, 'runtime> {
	register: &'register mut CollectiveRegister<'env, 'comm, 'runtime>,
	prepared: &'prepared PreparedTransform<'env, 'comm, 'runtime>,
	mass: MassObservation,
}
impl<'prepared, 'register, 'env, 'comm, 'runtime>
	ExecutionResult<'prepared, 'register, 'env, 'comm, 'runtime>
{
	#[must_use]
	pub const fn mass(&self) -> MassObservation {
		self.mass
	}
	#[must_use]
	pub const fn release(self) -> &'register mut CollectiveRegister<'env, 'comm, 'runtime> {
		self.register
	}
	/// # Errors
	/// Rejects mismatched operation order and zero or nonfinite retained mass.
	pub fn condition(
		self,
	) -> Result<ConditionedResult<'prepared, 'register, 'env, 'comm, 'runtime>> {
		let mut lane =
			self.prepared
				.environment
				.begin(22, self.prepared.id, self.register.id, 0)?;
		agree(
			&mut lane,
			if self.mass.retained() > 0.0 && self.mass.retained().is_finite() {
				Ok(())
			} else {
				Err(Error::ZeroSuccess)
			},
		)?;
		fatal(|| {
			Ok(
				quest_sys::set_qureg_to_renormalized(self.register.inner.pin())
					.context("conditioning collective QSVT result")?,
			)
		});
		Ok(ConditionedResult { inner: self })
	}
}
pub struct ConditionedResult<'prepared, 'register, 'env, 'comm, 'runtime> {
	inner: ExecutionResult<'prepared, 'register, 'env, 'comm, 'runtime>,
}
impl<'register, 'env, 'comm, 'runtime> ConditionedResult<'_, 'register, 'env, 'comm, 'runtime> {
	#[must_use]
	pub const fn mass(&self) -> MassObservation {
		self.inner.mass
	}
	#[must_use]
	pub const fn register(&self) -> &CollectiveRegister<'env, 'comm, 'runtime> {
		self.inner.register
	}
	#[must_use]
	pub const fn release(self) -> &'register mut CollectiveRegister<'env, 'comm, 'runtime> {
		self.inner.register
	}
}
pub struct OverlapBuilder<'env, 'comm, 'runtime, I = MissingVector, R = MissingVector> {
	environment: &'env CollectiveEnvironment<'comm, 'runtime>,
	transform: ValidatedTransform,
	input: I,
	reference: R,
}
impl<'env, 'comm, 'runtime, R> OverlapBuilder<'env, 'comm, 'runtime, MissingVector, R> {
	#[must_use]
	pub fn input(
		self,
		input: Vec<Complex64>,
	) -> OverlapBuilder<'env, 'comm, 'runtime, SuppliedVector, R> {
		OverlapBuilder {
			environment: self.environment,
			transform: self.transform,
			input: SuppliedVector(input),
			reference: self.reference,
		}
	}
}
impl<'env, 'comm, 'runtime, I> OverlapBuilder<'env, 'comm, 'runtime, I, MissingVector> {
	#[must_use]
	pub fn reference(
		self,
		reference: Vec<Complex64>,
	) -> OverlapBuilder<'env, 'comm, 'runtime, I, SuppliedVector> {
		OverlapBuilder {
			environment: self.environment,
			transform: self.transform,
			input: self.input,
			reference: SuppliedVector(reference),
		}
	}
}
impl<'env, 'comm, 'runtime> OverlapBuilder<'env, 'comm, 'runtime, SuppliedVector, SuppliedVector> {
	/// # Errors
	/// Rejects inconsistent transforms/vectors, invalid unit vectors, widths or budgets on any rank.
	/// The three internal registers include `log2(ranks)` idle high qubits so
	/// controlled dense matrices fit each rank's native communication buffer.
	/// Their complete storage is included in aggregate admission.
	pub fn prepare(self) -> Result<PreparedOverlap<'env, 'comm, 'runtime>> {
		let env = self.environment;
		let id = env.identifier();
		let mut lane = env.begin(23, id, 0, 0)?;
		compare(
			&mut lane,
			env,
			&self.transform,
			Some((&self.input.0, &self.reference.0)),
		)?;
		let admission = (|| {
			AdmittedOverlap::with_padding(
				&env.resources,
				self.transform,
				&self.input.0,
				&self.reference.0,
				rank_bits(env.size()?)?,
			)
		})();
		let admission = agree(&mut lane, admission)?;
		Ok(PreparedOverlap {
			environment: env,
			id,
			inner: fatal(|| admission.materialize()),
		})
	}
}
pub struct PreparedOverlap<'env, 'comm, 'runtime> {
	environment: &'env CollectiveEnvironment<'comm, 'runtime>,
	id: u64,
	inner: super::PreparedOverlap<'env>,
}
impl PreparedOverlap<'_, '_, '_> {
	#[must_use]
	pub const fn transform(&self) -> &ValidatedTransform {
		self.inner.transform()
	}
	/// # Errors
	/// Rejects inconsistent invocation or changed admission before native entry.
	pub fn run(&mut self) -> Result<super::OverlapObservation> {
		let mut lane = self.environment.begin(24, self.id, 0, 0)?;
		let admission = agree(&mut lane, self.inner.admit_run().map_err(Error::from))?;
		Ok(fatal(|| self.inner.run_admitted(admission)))
	}
}
fn compare(
	lane: &mut MpiCollectiveLane<'_>,
	env: &CollectiveEnvironment<'_, '_>,
	transform: &ValidatedTransform,
	vectors: Option<(&[Complex64], &[Complex64])>,
) -> Result<()> {
	let limit = env
		.resources
		.memory_budget()
		.bytes()
		.saturating_sub(env.resources.allocated_bytes());
	let bytes = agree(
		lane,
		payload::encode(
			transform,
			vectors,
			&env.resources,
			limit,
			if vectors.is_some() {
				rank_bits(env.size()?)?
			} else {
				0
			},
		)
		.map_err(Error::from),
	)?;
	let _storage = agree(
		lane,
		env.resources.reserve(bytes.len()).map_err(Error::from),
	)?;
	Ok(equal(lane, &bytes)?)
}
fn agree<T>(lane: &mut MpiCollectiveLane<'_>, value: Result<T>) -> Result<T> {
	if !lane
		.all_agree(value.is_ok())
		.context("agreeing QSVT collective admission")?
	{
		return Err(Error::Runtime(crate::Error::Value(
			"collective QSVT admission rejected on one or more ranks",
		)));
	}
	value
}
fn fatal<T>(operation: impl FnOnce() -> Result<T>) -> T {
	std::panic::catch_unwind(std::panic::AssertUnwindSafe(operation))
		.unwrap_or_else(|_| quest_sys::mpi::abort_job())
		.unwrap_or_else(|error| {
			let _ = writeln!(
				std::io::stderr().lock(),
				"collective QSVT native failure: {error}"
			);
			quest_sys::mpi::abort_job()
		})
}

fn rank_bits(ranks: i32) -> crate::Result<usize> {
	usize::try_from(
		u32::try_from(ranks)
			.map_err(|_| crate::Error::Overflow)?
			.ilog2(),
	)
	.map_err(|_| crate::Error::Overflow)
}
fn capacity(transform: &ValidatedTransform, ranks: i32) -> crate::Result<()> {
	let local = transform
		.operands()
		.num_qubits()
		.checked_sub(rank_bits(ranks)?)
		.ok_or(crate::Error::Value(
			"QSVT state has fewer amplitudes than ranks",
		))?;
	for p in [Some(transform.main()), transform.continuation()]
		.into_iter()
		.flatten()
	{
		for instruction in super::plan(p)?.instructions() {
			check_operation(instruction.operation(), 0, local, 0)?;
		}
	}
	for projection in [
		Some(transform.input()),
		transform.bridge(),
		Some(transform.output()),
	]
	.into_iter()
	.flatten()
	{
		// Coordinates always use a projector/diagonal. An admitted isometry uses
		// a dense canonical VV† even if its entries happen to be sparse.
		let dense = match projection.space() {
			quest_qsvt::ProjectionSpace::Left(s) => {
				!matches!(s.kind(), quest_qsvt::ProjectorKind::Coordinates(_))
			}
			quest_qsvt::ProjectionSpace::Right(s) => {
				!matches!(s.kind(), quest_qsvt::ProjectorKind::Coordinates(_))
			}
			quest_qsvt::ProjectionSpace::Joint { left, right } => {
				!matches!(left.kind(), quest_qsvt::ProjectorKind::Coordinates(_))
					|| !matches!(right.kind(), quest_qsvt::ProjectorKind::Coordinates(_))
			}
		};
		if dense && projection.targets().len().max(1) > local {
			return Err(crate::Error::Value(
				"dense QSVT projection exceeds per-rank communication capacity; supply idle operand qubits",
			));
		}
	}
	Ok(())
}
fn check_operation(
	operation: &quest_compile::Operation,
	inherited: usize,
	local: usize,
	depth: usize,
) -> crate::Result<()> {
	if depth > 64 {
		return Err(crate::Error::Value("QSVT oracle nesting limit"));
	}
	match operation {
		quest_compile::Operation::Numerical {
			matrix,
			targets,
			controls,
		} if !matrix.is_diagonal() => {
			let count = targets
				.len()
				.checked_add(controls.len())
				.and_then(|n| n.checked_add(inherited))
				.ok_or(crate::Error::Overflow)?
				.max(1);
			if count > local {
				return Err(crate::Error::Value(
					"dense QSVT operation exceeds per-rank communication capacity; supply idle operand qubits",
				));
			}
		}
		quest_compile::Operation::Oracle {
			fragment, controls, ..
		} => {
			let inherited = inherited
				.checked_add(controls.len())
				.ok_or(crate::Error::Overflow)?;
			for operation in fragment.operations() {
				check_operation(operation, inherited, local, depth.saturating_add(1))?;
			}
		}
		_ => {}
	}
	Ok(())
}
