//! Prepared QSVT execution with explicit projection and consuming conditioning.
//!
//! Preparation freezes binary64 native resources. It performs no certification
//! or multiprecision calculation. Bounds on the mathematical transform remain
//! separate from floating point execution observations.
//!
//! A result exclusively borrows its register. Copying its mass observations
//! cannot authorize conditioning after the register has been released.
use crate::execution::PreparedRegion;
#[cfg(all(feature = "mpi", quest_native_mpi))]
pub mod amplitude_preparation;
#[cfg(all(feature = "mpi", quest_native_mpi))]
pub mod collective;
mod continuation;
mod hadamard;
pub mod matching;
pub mod matching_lcu;
pub mod matching_lcu_transform;
pub mod matching_transform;
#[cfg(all(feature = "qsvt-io", feature = "mpi", quest_native_mpi))]
pub mod persisted_matching;
mod projection;
pub mod replay_native;
mod reporting;
mod transform_execution;
use crate::{Complex64, Environment, Register, StateVector};
use crate::{environment::Reservation, error::BackendResult, values::bytes_for};
use continuation::{AdmittedContinuation, PreparedContinuation};
use faer::Mat;
pub use hadamard::{AdmittedOverlap, OverlapBuilder, OverlapObservation, PreparedOverlap};
use projection::PreparedProjection;
pub use quest_qsvt as model;
use quest_qsvt::{Projection, ValidatedTransform};
pub use reporting::NativeDispatchReport;
use std::ops::Div;

pub type Result<T> = std::result::Result<T, Error>;
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum Error {
	#[error(transparent)]
	Runtime(#[from] crate::Error),
	#[error(transparent)]
	Model(#[from] quest_qsvt::Error),
	#[error("QSVT {stage:?} failed after {completed_stages} completed stages: {source}")]
	Execution {
		stage: Stage,
		completed_stages: usize,
		#[source]
		source: crate::Error,
	},
	#[error("conditioning requires strictly positive finite retained mass")]
	ZeroSuccess,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Stage {
	InputProjection,
	Main,
	BridgeProjection,
	Continuation,
	OutputProjection,
	Readout,
}
#[derive(Debug)]
pub struct MissingTransform;
#[derive(Debug)]
pub struct SuppliedTransform(ValidatedTransform);
/// Required transform and native environment precede preparation.
/// ```compile_fail
/// # fn f(env: &quest::Environment) {
/// env.qsvt().prepare();
/// # }
/// ```
pub struct PreparationBuilder<'env, T = MissingTransform> {
	environment: &'env Environment,
	transform: T,
}
impl Environment {
	#[must_use]
	pub const fn qsvt(&self) -> PreparationBuilder<'_> {
		PreparationBuilder {
			environment: self,
			transform: MissingTransform,
		}
	}
}
impl<'env> PreparationBuilder<'env> {
	#[must_use]
	pub const fn transform(
		self,
		transform: ValidatedTransform,
	) -> PreparationBuilder<'env, SuppliedTransform> {
		PreparationBuilder {
			environment: self.environment,
			transform: SuppliedTransform(transform),
		}
	}
}
impl<'env> PreparationBuilder<'env, SuppliedTransform> {
	/// Lower the circuits, validate projections and reserve the aggregate budget.
	/// Native matrices are allocated only by the returned stage's `prepare`.
	/// # Errors
	/// Rejects unsupported instructions, invalid projections and resource limits.
	pub fn admit(self) -> Result<AdmittedTransform<'env>> {
		AdmittedTransform::new(&self.environment.resources, self.transform.0)
	}
	/// Allocate transactionally: failure drops every already transferred native object.
	/// # Errors
	/// Rejects numerical configuration, memory budgets and native allocation/transfer failures.
	pub fn prepare(self) -> Result<PreparedTransform<'env>> {
		self.admit()?.prepare()
	}
}
/// Lowered plans and admitted projections with an environment budget reservation.
/// This stage owns no native matrix. Dropping it releases its reservation.
///
/// ```compile_fail
/// # fn f(stage: quest::qsvt::AdmittedTransform<'_>) {
/// stage.run();
/// # }
/// ```
pub struct AdmittedTransform<'env> {
	main: crate::execution::AdmittedPlan<'env>,
	continuation: AdmittedContinuation<'env>,
	input: projection::AdmittedProjection<'env>,
	output: projection::AdmittedProjection<'env>,
	transform: ValidatedTransform,
	dispatches: NativeDispatchReport,
}
impl<'env> AdmittedTransform<'env> {
	/// Transfer the admitted binary64 data to native storage transactionally.
	/// # Errors
	/// Reports native allocation or transfer failures; earlier transfers are dropped.
	pub fn prepare(self) -> Result<PreparedTransform<'env>> {
		self.materialize()
	}
	fn new(
		resources: &'env crate::environment::RuntimeResources,
		transform: ValidatedTransform,
	) -> Result<Self> {
		let main = resources.admit_plan(plan(transform.main())?)?;
		let continuation = AdmittedContinuation::new(resources, &transform, plan)?;
		let input = projection::AdmittedProjection::new(resources, transform.input())?;
		let output = projection::AdmittedProjection::new(resources, transform.output())?;
		let dispatches = dispatch_schedule(&main, &continuation, &input, &output, false)?;
		Ok(Self {
			main,
			continuation,
			input,
			output,
			transform,
			dispatches,
		})
	}
	fn materialize(self) -> Result<PreparedTransform<'env>> {
		Ok(PreparedTransform {
			main: self.main.materialize()?,
			continuation: self.continuation.materialize()?,
			input: self.input.materialize()?,
			output: self.output.materialize()?,
			transform: self.transform,
			dispatches: self.dispatches,
		})
	}
}
fn dispatch_schedule(
	main: &crate::execution::AdmittedPlan<'_>,
	continuation: &AdmittedContinuation<'_>,
	input: &projection::AdmittedProjection<'_>,
	output: &projection::AdmittedProjection<'_>,
	overlap: bool,
) -> crate::Result<NativeDispatchReport> {
	let count = |program: &crate::execution::AdmittedPlan<'_>| {
		let _scratch = program.dispatch_scratch()?;
		reporting::circuit(program.plan())
	};
	let circuit = count(main)?
		.checked_add(continuation.program().map(count).transpose()?.unwrap_or(0))
		.ok_or(crate::Error::Overflow)?;
	let projections = input
		.native_dispatches()
		.checked_add(output.native_dispatches())
		.and_then(|n| {
			n.checked_add(
				continuation
					.bridge()
					.map_or(0, projection::AdmittedProjection::native_dispatches),
			)
		})
		.ok_or(crate::Error::Overflow)?;
	NativeDispatchReport::new(
		circuit,
		projections,
		overlap,
		continuation.bridge().is_some(),
	)
}
fn plan(program: &quest_compile::BoundRegion) -> crate::Result<quest_compile::RegionPlan> {
	Ok(program.clone().plan()?)
}
type RunAdmission = (Vec<bool>, Option<Vec<bool>>);
/// Environment-bound native matrices and scratch, reusable for successive states.
///
/// ```compile_fail
/// # fn f(transform: quest::qsvt::model::ValidatedTransform) -> Result<(), Box<dyn std::error::Error>> {
/// let prepared = {
///     let env = quest::Environment::builder().build()?;
///     env.qsvt().transform(transform).prepare()?
/// };
/// # Ok(()) }
/// ```
pub struct PreparedTransform<'env> {
	main: PreparedRegion<'env>,
	continuation: PreparedContinuation<'env>,
	input: PreparedProjection<'env>,
	output: PreparedProjection<'env>,
	transform: ValidatedTransform,
	dispatches: NativeDispatchReport,
}
impl<'env> PreparedTransform<'env> {
	#[must_use]
	pub const fn transform(&self) -> &ValidatedTransform {
		&self.transform
	}
	/// Run projection → main → optional bridge/continuation → output projection.
	/// Input need not be normalized; recorded masses are absolute squared norms.
	/// No numerical payload or native matrix is constructed during this method.
	/// # Errors
	/// Reports preflight rejection, or the completed stage prefix and native instruction failure.
	pub fn run<'prepared, 'register>(
		&'prepared mut self,
		register: &'register mut Register<'env, StateVector>,
	) -> Result<ExecutionResult<'prepared, 'register, 'env>> {
		let admission = self.admit_run(register)?;
		self.run_admitted(register, admission)
	}
	fn admit_run(&self, register: &Register<'_, StateVector>) -> crate::Result<RunAdmission> {
		Ok((
			self.main.admit_run(register)?,
			self.continuation
				.program()
				.map(|p| p.admit_run(register))
				.transpose()?,
		))
	}
	fn run_admitted<'prepared, 'register>(
		&'prepared mut self,
		register: &'register mut Register<'env, StateVector>,
		(main_bits, continuation_bits): RunAdmission,
	) -> Result<ExecutionResult<'prepared, 'register, 'env>> {
		let initial = valid_mass(register.total_probability()?)?;
		let mut completed = 0usize;
		self.input
			.apply(register)
			.map_err(|source| at(Stage::InputProjection, completed, source))?;
		let input = valid_mass(
			register
				.total_probability()
				.map_err(|source| at(Stage::InputProjection, completed, source))?,
		)?;
		completed = completed.saturating_add(1);
		self.main
			.run_admitted(register, main_bits)
			.map_err(|source| at(Stage::Main, completed, source))?;
		completed = completed.saturating_add(1);
		let bridge = if let Some(projection) = self.continuation.bridge() {
			projection
				.apply(register)
				.map_err(|source| at(Stage::BridgeProjection, completed, source))?;
			let mass = valid_mass(
				register
					.total_probability()
					.map_err(|source| at(Stage::BridgeProjection, completed, source))?,
			)?;
			completed = completed.saturating_add(1);
			Some(mass)
		} else {
			None
		};
		if let Some((program, bits)) = self.continuation.program_mut().zip(continuation_bits) {
			program
				.run_admitted(register, bits)
				.map_err(|source| at(Stage::Continuation, completed, source))?;
			completed = completed.saturating_add(1);
		}
		self.output
			.apply(register)
			.map_err(|source| at(Stage::OutputProjection, completed, source))?;
		let retained = valid_mass(
			register
				.total_probability()
				.map_err(|source| at(Stage::OutputProjection, completed, source))?,
		)?;
		Ok(ExecutionResult {
			register,
			output: self.transform.output(),
			mass: MassObservation {
				initial,
				input,
				bridge,
				retained,
				dispatches: self.dispatches,
			},
		})
	}
}
const fn at(stage: Stage, completed_stages: usize, source: crate::Error) -> Error {
	Error::Execution {
		stage,
		completed_stages,
		source,
	}
}
fn valid_mass(value: f64) -> Result<f64> {
	if value.is_finite() && value >= 0.0 {
		Ok(value)
	} else {
		Err(crate::Error::Value("nonfinite or negative squared norm").into())
	}
}
/// Numerical observations, never a mathematical error certificate or permission to condition.
#[derive(Debug, Clone, Copy)]
pub struct MassObservation {
	initial: f64,
	input: f64,
	bridge: Option<f64>,
	retained: f64,
	dispatches: NativeDispatchReport,
}
impl MassObservation {
	/// Native API calls in the successful run; see [`NativeDispatchReport`] for scope.
	#[must_use]
	pub const fn native_dispatches(self) -> NativeDispatchReport {
		self.dispatches
	}
	#[must_use]
	pub const fn initial(self) -> f64 {
		self.initial
	}
	#[must_use]
	pub const fn input(self) -> f64 {
		self.input
	}
	#[must_use]
	pub const fn bridge(self) -> Option<f64> {
		self.bridge
	}
	#[must_use]
	pub const fn retained(self) -> f64 {
		self.retained
	}
	#[must_use]
	pub fn relative_success(self) -> Option<f64> {
		(self.initial > 0.0).then(|| self.retained.div(self.initial))
	}
}
/// Exclusive capability for this exact postselection result.
/// ```compile_fail
/// # fn f<'env>(prepared: &mut quest::qsvt::PreparedTransform<'env>, register: &mut quest::Register<'env, quest::StateVector>) -> Result<(), Box<dyn std::error::Error>> {
/// let result = prepared.run(register)?;
/// register.init_zero()?;
/// result.condition()?;
/// # Ok(()) }
/// ```
/// ```compile_fail
/// # fn f(result: quest::qsvt::ExecutionResult<'_, '_, '_>) -> Result<(), Box<dyn std::error::Error>> {
/// let mass = result.mass();
/// result.release();
/// mass.condition()?;
/// # Ok(()) }
/// ```
pub struct ExecutionResult<'prepared, 'register, 'env> {
	register: &'register mut Register<'env, StateVector>,
	output: &'prepared Projection,
	mass: MassObservation,
}
impl<'prepared, 'register, 'env> ExecutionResult<'prepared, 'register, 'env> {
	#[must_use]
	pub const fn mass(&self) -> MassObservation {
		self.mass
	}
	/// Read an independent owned logical vector; native storage remains subnormalized.
	/// # Errors
	/// Rejects allocation budgets, invalid native state or failed state transfer.
	pub fn logical_snapshot(&self) -> Result<Mat<Complex64>> {
		decode(self.register, self.output)
	}
	/// Read the full postselected physical register before conditioning.
	/// # Errors
	/// Rejects allocation budgets, invalid native state or failed state transfer.
	pub fn physical_snapshot(&self) -> Result<Mat<Complex64>> {
		Ok(self.register.snapshot()?)
	}
	/// Release without normalization, preserving every subnormalized amplitude.
	#[must_use]
	pub const fn release(self) -> &'register mut Register<'env, StateVector> {
		self.register
	}
	/// Consume the exclusive result and normalize the still-borrowed state.
	/// # Errors
	/// Rejects zero success or native renormalization failure.
	pub fn condition(self) -> Result<ConditionedResult<'prepared, 'register, 'env>> {
		if self.mass.retained <= 0.0 || !self.mass.retained.is_finite() {
			return Err(Error::ZeroSuccess);
		}
		quest_sys::set_qureg_to_renormalized(self.register.pin())
			.context("conditioning QSVT result")?;
		Ok(ConditionedResult {
			register: self.register,
			output: self.output,
			mass: self.mass,
		})
	}
}
pub struct ConditionedResult<'prepared, 'register, 'env> {
	register: &'register mut Register<'env, StateVector>,
	output: &'prepared Projection,
	mass: MassObservation,
}
impl<'register, 'env> ConditionedResult<'_, 'register, 'env> {
	#[must_use]
	pub const fn mass(&self) -> MassObservation {
		self.mass
	}
	#[must_use]
	pub const fn register(&self) -> &Register<'env, StateVector> {
		self.register
	}
	/// # Errors
	/// Rejects allocation budgets and native transfer failure.
	pub fn logical_snapshot(&self) -> Result<Mat<Complex64>> {
		decode(self.register, self.output)
	}
	#[must_use]
	pub const fn release(self) -> &'register mut Register<'env, StateVector> {
		self.register
	}
}
fn decode(register: &Register<'_, StateVector>, projection: &Projection) -> Result<Mat<Complex64>> {
	if projection.space().is_coordinate_space() {
		let dimension = projection.logical_dimension();
		let _reservation = register.resources().reserve(bytes_for(dimension, 3)?)?;
		let mut result = crate::register::matrix(dimension, 1)?;
		let mut base = 0usize;
		for control in projection.controls() {
			if control.value {
				base |= physical_bit(control.qubit)?;
			}
		}
		for row in 0..dimension {
			let local = projection
				.space()
				.coordinate_at(row)
				.ok_or(crate::Error::Value("projection coordinate"))?;
			let mut physical = base;
			for (bit, &target) in projection.targets().iter().enumerate() {
				if local & physical_bit(bit)? != 0 {
					physical |= physical_bit(target)?;
				}
			}
			result[(row, 0)] = register.amplitude(physical)?;
		}
		return Ok(result);
	}
	let entries = register
		.dimension()
		.checked_mul(projection.logical_dimension())
		.ok_or(crate::Error::Overflow)?;
	let bytes = bytes_for(
		entries
			.checked_add(register.dimension())
			.ok_or(crate::Error::Overflow)?,
		3,
	)?;
	let _reservation: Reservation<'_> = register.resources().reserve(bytes)?;
	let policy = model::NumericalPolicy { max_bytes: bytes };
	// Export is an explicit cold operation. Neither preparation nor execution
	// needs a full-register isometry or full-state host snapshot.
	let basis = projection.materialize_isometry(register.num_qubits().get(), policy)?;
	let state = register.snapshot()?;
	let mut result = crate::register::matrix(basis.ncols(), 1)?;
	faer::linalg::matmul::matmul(
		result.as_mut(),
		faer::Accum::Replace,
		basis.adjoint(),
		state.as_ref(),
		Complex64::new(1.0, 0.0),
		faer::Par::Seq,
	);
	Ok(result)
}

fn physical_bit(position: usize) -> Result<usize> {
	1usize
		.checked_shl(u32::try_from(position).map_err(|_| crate::Error::Overflow)?)
		.ok_or(Error::Runtime(crate::Error::Overflow))
}
