//! Hadamard observations with a reusable reference, working state and scratch.
use super::{
	PreparationBuilder, Result, SuppliedTransform,
	continuation::{AdmittedContinuation, PreparedContinuation},
	projection::PreparedProjection,
};
use crate::execution::PreparedRegion;
use crate::{Complex64, Environment, QubitCount, Register, StateVector};
use crate::{
	environment::{Reservation, RuntimeResources},
	error::BackendResult,
	values::{bytes_for, reserve_vec},
};
use quest_compile::{Control, ControlState, OracleFragment, QuantumRegionBuilder};
use quest_qsvt::ValidatedTransform;
use std::ops::{Add, Div, Mul, Sub};

pub struct MissingVector;
pub struct SuppliedVector(pub(super) Vec<Complex64>);
/// Supply both ordered logical vectors before preparing the experiment.
pub struct OverlapBuilder<'env, I = MissingVector, R = MissingVector> {
	environment: &'env Environment,
	transform: ValidatedTransform,
	input: I,
	reference: R,
}
impl<'env> PreparationBuilder<'env, SuppliedTransform> {
	#[must_use]
	pub fn overlap(self) -> OverlapBuilder<'env> {
		OverlapBuilder {
			environment: self.environment,
			transform: self.transform.0,
			input: MissingVector,
			reference: MissingVector,
		}
	}
}
impl<'env, R> OverlapBuilder<'env, MissingVector, R> {
	#[must_use]
	pub fn input(self, input: Vec<Complex64>) -> OverlapBuilder<'env, SuppliedVector, R> {
		OverlapBuilder {
			environment: self.environment,
			transform: self.transform,
			input: SuppliedVector(input),
			reference: self.reference,
		}
	}
}
impl<'env, I> OverlapBuilder<'env, I, MissingVector> {
	#[must_use]
	pub fn reference(self, reference: Vec<Complex64>) -> OverlapBuilder<'env, I, SuppliedVector> {
		OverlapBuilder {
			environment: self.environment,
			transform: self.transform,
			input: self.input,
			reference: SuppliedVector(reference),
		}
	}
}
impl<'env> OverlapBuilder<'env, SuppliedVector, SuppliedVector> {
	/// Lower controlled circuits, pack input data and reserve the complete budget.
	/// No native matrix or register is allocated by this stage.
	/// # Errors
	/// Rejects vector dimensions/norms, unsupported instructions and resource limits.
	pub fn admit(self) -> Result<AdmittedOverlap<'env>> {
		AdmittedOverlap::new(
			&self.environment.resources,
			self.transform,
			&self.input.0,
			&self.reference.0,
		)
	}
	/// Freeze all input data and prepare controlled main/continuation circuits.
	/// # Errors
	/// Rejects vector dimensions/norms, budgets, native admission and transfer failures.
	pub fn prepare(self) -> Result<PreparedOverlap<'env>> {
		self.admit()?.prepare()
	}
}
/// Packed binary64 input, lowered controlled circuits and aggregate reservations.
/// Native allocation is deferred until this owning stage is consumed.
pub struct AdmittedOverlap<'env> {
	initial: Reservation<'env>,
	working: Reservation<'env>,
	scratch: Reservation<'env>,
	main: crate::execution::AdmittedPlan<'env>,
	continuation: AdmittedContinuation<'env>,
	input: super::projection::AdmittedProjection<'env>,
	output: super::projection::AdmittedProjection<'env>,
	outer: i32,
	transform: ValidatedTransform,
	dispatches: super::NativeDispatchReport,
	reservation: Reservation<'env>,
	amplitudes: Vec<quest_sys::QuestComplex>,
	count: QubitCount,
}
impl<'env> AdmittedOverlap<'env> {
	/// Materialize native matrices and the three reusable register workspaces.
	/// # Errors
	/// Reports native failures; every successful earlier allocation is released.
	pub fn prepare(self) -> Result<PreparedOverlap<'env>> {
		self.materialize()
	}
	pub(super) fn new(
		resources: &'env RuntimeResources,
		transform: ValidatedTransform,
		input_values: &[Complex64],
		reference_values: &[Complex64],
	) -> Result<Self> {
		Self::with_padding(resources, transform, input_values, reference_values, 0)
	}
	pub(super) fn with_padding(
		resources: &'env RuntimeResources,
		transform: ValidatedTransform,
		input_values: &[Complex64],
		reference_values: &[Complex64],
		padding: usize,
	) -> Result<Self> {
		check_vector(input_values, transform.input().logical_dimension())?;
		check_vector(reference_values, transform.output().logical_dimension())?;
		let width = transform.operands().num_qubits();
		let outer = i32::try_from(width).map_err(|_| crate::Error::Overflow)?;
		let count = QubitCount::new(
			width
				.checked_add(1)
				.and_then(|n| n.checked_add(padding))
				.ok_or(crate::Error::Overflow)?,
		)?;
		let dimension = count.dimension();
		let local = super::physical_bit(width)?;
		let branches = local.checked_mul(2).ok_or(crate::Error::Overflow)?;
		let coordinate_inputs = transform.input().space().is_coordinate_space()
			&& transform.output().space().is_coordinate_space();
		let preparation_bytes = if coordinate_inputs {
			// Only the unpacked and packed state buffers coexist; no isometry.
			bytes_for(dimension, 2)?
		} else {
			bytes_for(
				dimension
					.checked_mul(
						transform
							.input()
							.logical_dimension()
							.checked_add(transform.output().logical_dimension())
							.ok_or(crate::Error::Overflow)?,
					)
					.ok_or(crate::Error::Overflow)?,
				4,
			)?
		};
		let reservation = resources.reserve(preparation_bytes)?;
		let policy = quest_qsvt::NumericalPolicy {
			max_bytes: preparation_bytes,
		};
		let mut amplitudes = reserve_vec(dimension)?;
		amplitudes.resize(dimension, Complex64::new(0.0, 0.0));
		pack_branch(
			amplitudes.get_mut(..local).ok_or(crate::Error::Overflow)?,
			transform.output(),
			reference_values,
			width,
			policy,
		)?;
		pack_branch(
			amplitudes
				.get_mut(local..branches)
				.ok_or(crate::Error::Overflow)?,
			transform.input(),
			input_values,
			width,
			policy,
		)?;
		let amplitudes = crate::register::pack(amplitudes.into_iter(), dimension)?;
		let initial = Register::<StateVector>::admit_allocation(resources, count)?;
		let working = Register::<StateVector>::admit_allocation(resources, count)?;
		let scratch = Register::<StateVector>::admit_allocation(resources, count)?;
		let main = resources.admit_plan(controlled_plan_padded(
			resources,
			transform.main(),
			width,
			padding,
		)?)?;
		let continuation = AdmittedContinuation::new(resources, &transform, |p| {
			controlled_plan_padded(resources, p, width, padding)
		})?;
		let input = super::projection::AdmittedProjection::new(resources, transform.input())?;
		let output = super::projection::AdmittedProjection::new(resources, transform.output())?;
		let dispatches = super::dispatch_schedule(&main, &continuation, &input, &output, true)?;
		Ok(Self {
			initial,
			working,
			scratch,
			main,
			continuation,
			input,
			output,
			outer,
			transform,
			dispatches,
			reservation,
			amplitudes,
			count,
		})
	}
	pub(super) fn materialize(self) -> Result<PreparedOverlap<'env>> {
		let mut initial = Register::allocate_admitted(self.initial, self.count)?;
		quest_sys::init_arbitrary_pure_state(initial.pin(), &self.amplitudes)
			.context("initializing Hadamard reference")?;
		let working = Register::allocate_admitted(self.working, self.count)?;
		let scratch = Register::allocate_admitted(self.scratch, self.count)?;
		let result = PreparedOverlap {
			initial,
			working,
			scratch,
			main: self.main.materialize()?,
			continuation: self.continuation.materialize()?,
			input: self.input.materialize()?,
			output: self.output.materialize()?,
			outer: self.outer,
			transform: self.transform,
			dispatches: self.dispatches,
		};
		drop(self.amplitudes);
		drop(self.reservation);
		Ok(result)
	}
}
// Scatter exact coordinate embeddings; numerical embeddings retain their admitted
// dense definition. Caller-owned overlap vectors and the state buffer are explicit.
fn pack_branch(
	amplitudes: &mut [Complex64],
	projection: &quest_qsvt::Projection,
	coefficients: &[Complex64],
	width: usize,
	policy: quest_qsvt::NumericalPolicy,
) -> Result<()> {
	if amplitudes.len() != super::physical_bit(width)? {
		return Err(crate::Error::Value("overlap preparation branch width").into());
	}
	let scale = std::f64::consts::FRAC_1_SQRT_2;
	if projection.space().is_coordinate_space() {
		for (logical, &coefficient) in coefficients.iter().enumerate() {
			let physical = projection
				.coordinate_at(logical)?
				.ok_or(crate::Error::Value("overlap preparation coordinate"))?;
			*amplitudes
				.get_mut(physical)
				.ok_or(crate::Error::Value("overlap preparation register width"))? =
				coefficient.mul(scale);
		}
	} else {
		let basis = projection.materialize_isometry(width, policy)?;
		for (row, amplitude) in amplitudes.iter_mut().enumerate() {
			let mut value = Complex64::new(0.0, 0.0);
			for (col, &coefficient) in coefficients.iter().enumerate() {
				value = value.add(basis[(row, col)].mul(coefficient));
			}
			*amplitude = value.mul(scale);
		}
	}
	Ok(())
}
fn check_vector(vector: &[Complex64], dimension: usize) -> Result<()> {
	if vector.len() != dimension {
		return Err(crate::Error::Value("logical overlap vector dimension").into());
	}
	let norm = vector
		.iter()
		.fold(0.0_f64, |norm, value| norm.hypot(value.re).hypot(value.im));
	if !norm.is_finite() || norm.sub(1.0).abs() > 1e-12 {
		return Err(crate::Error::Value("overlap vectors must have unit norm within 1e-12").into());
	}
	Ok(())
}
pub(super) fn controlled_plan_padded(
	resources: &RuntimeResources,
	program: &quest_compile::BoundRegion,
	width: usize,
	padding: usize,
) -> crate::Result<quest_compile::RegionPlan> {
	let build = || -> quest_compile::Result<_> {
		let fragment = OracleFragment::builder(program.clone())
			.matrix_tolerance(1e-10)?
			.matrix_policy(quest_compile::MatrixPolicy {
				max_bytes: resources.memory_budget().bytes(),
			})
			.build()?;
		let mut builder = QuantumRegionBuilder::new(
			width
				.checked_add(1)
				.and_then(|n| n.checked_add(padding))
				.ok_or(quest_compile::Error::Budget("Hadamard width"))?,
			0,
		)?;
		let targets = (0..width)
			.map(|q| builder.qubit(q))
			.collect::<quest_compile::Result<Vec<_>>>()?;
		builder.oracle(
			&fragment,
			&targets,
			&[Control::new(builder.qubit(width)?, ControlState::One)],
		)?;
		builder.finish()?.bind(&[])?.plan()
	};
	Ok(build()?)
}
/// Three reusable registers and prepared operations.
///
/// Projection of the active
/// branch uses a native scratch state and exact linear recombination; the
/// reference branch survives intermediate postselection. Native weighted-sum
/// calls may allocate small QuEST-internal argument vectors.
pub struct PreparedOverlap<'env> {
	initial: Register<'env, StateVector>,
	working: Register<'env, StateVector>,
	scratch: Register<'env, StateVector>,
	main: PreparedRegion<'env>,
	continuation: PreparedContinuation<'env>,
	input: PreparedProjection<'env>,
	output: PreparedProjection<'env>,
	outer: i32,
	transform: ValidatedTransform,
	dispatches: super::NativeDispatchReport,
}
impl PreparedOverlap<'_> {
	#[must_use]
	pub const fn transform(&self) -> &ValidatedTransform {
		&self.transform
	}
	/// Observe real and imaginary interference from the same transformed state.
	/// # Errors
	/// Reports preflight and completed-prefix native circuit failures.
	pub fn run(&mut self) -> Result<OverlapObservation> {
		let admission = self.admit_run()?;
		self.run_admitted(admission)
	}
	pub(super) fn admit_run(&self) -> crate::Result<super::RunAdmission> {
		Ok((
			self.main.admit_run(&self.working)?,
			self.continuation
				.program()
				.map(|p| p.admit_run(&self.working))
				.transpose()?,
		))
	}
	pub(super) fn run_admitted(
		&mut self,
		(bits, continuation_bits): super::RunAdmission,
	) -> Result<OverlapObservation> {
		let mut completed = 0usize;
		quest_sys::set_qureg_to_clone(self.working.pin(), &self.initial.native)
			.context("restoring Hadamard state")
			.map_err(|source| super::at(super::Stage::InputProjection, completed, source))?;
		conditional_projection(
			&self.input,
			&mut self.working,
			&mut self.scratch,
			self.outer,
		)
		.map_err(|source| super::at(super::Stage::InputProjection, completed, source))?;
		completed = completed.saturating_add(1);
		self.main
			.run_admitted(&mut self.working, bits)
			.map_err(|source| super::at(super::Stage::Main, completed, source))?;
		completed = completed.saturating_add(1);
		if let Some(bridge) = self.continuation.bridge() {
			conditional_projection(bridge, &mut self.working, &mut self.scratch, self.outer)
				.map_err(|source| super::at(super::Stage::BridgeProjection, completed, source))?;
			completed = completed.saturating_add(1);
		}
		if let Some((continuation, bits)) = self.continuation.program_mut().zip(continuation_bits) {
			continuation
				.run_admitted(&mut self.working, bits)
				.map_err(|source| super::at(super::Stage::Continuation, completed, source))?;
			completed = completed.saturating_add(1);
		}
		conditional_projection(
			&self.output,
			&mut self.working,
			&mut self.scratch,
			self.outer,
		)
		.map_err(|source| super::at(super::Stage::OutputProjection, completed, source))?;
		completed = completed.saturating_add(1);
		let readout = (|| -> crate::Result<_> {
			let retained_mass = self.working.total_probability()?;
			let active_mass =
				quest_sys::calc_prob_of_qubit_outcome(&self.working.native, self.outer, 1)
					.context("reading active Hadamard mass")?;
			quest_sys::set_qureg_to_clone(self.scratch.pin(), &self.working.native)
				.context("copying Hadamard readout")?;
			quest_sys::apply_hadamard(self.scratch.pin(), self.outer)
				.context("real Hadamard readout")?;
			let real_zero =
				quest_sys::calc_prob_of_qubit_outcome(&self.scratch.native, self.outer, 0)
					.context("reading real Hadamard probability")?;
			quest_sys::apply_phase_shift(
				self.working.pin(),
				self.outer,
				-std::f64::consts::FRAC_PI_2,
			)
			.context("imaginary Hadamard phase")?;
			quest_sys::apply_hadamard(self.working.pin(), self.outer)
				.context("imaginary Hadamard readout")?;
			let imaginary_zero =
				quest_sys::calc_prob_of_qubit_outcome(&self.working.native, self.outer, 0)
					.context("reading imaginary Hadamard probability")?;
			for value in [retained_mass, active_mass, real_zero, imaginary_zero] {
				if !value.is_finite() || value < 0.0 {
					return Err(crate::Error::Value("nonfinite or negative Hadamard mass"));
				}
			}
			Ok(OverlapObservation {
				dispatches: self.dispatches,
				retained_mass,
				active_mass,
				real_zero,
				imaginary_zero,
				overlap: Complex64::new(
					real_zero.mul(2.0).sub(retained_mass),
					imaginary_zero.mul(2.0).sub(retained_mass),
				),
			})
		})();
		readout.map_err(|source| super::at(super::Stage::Readout, completed, source))
	}
}
fn conditional_projection(
	projection: &PreparedProjection<'_>,
	working: &mut Register<'_, StateVector>,
	scratch: &mut Register<'_, StateVector>,
	outer: i32,
) -> crate::Result<()> {
	quest_sys::set_qureg_to_clone(scratch.pin(), &working.native)
		.context("copying conditional projection scratch")?;
	quest_sys::apply_qubit_projector(scratch.pin(), outer, 1)
		.context("selecting active Hadamard branch")?;
	projection.apply(scratch)?;
	quest_sys::apply_qubit_projector(working.pin(), outer, 0)
		.context("preserving reference Hadamard branch")?;
	quest_sys::add_qureg(working.pin(), &scratch.native)
		.context("recombining Hadamard branches")?;
	Ok(())
}
#[derive(Debug, Clone, Copy)]
pub struct OverlapObservation {
	dispatches: super::NativeDispatchReport,
	retained_mass: f64,
	active_mass: f64,
	real_zero: f64,
	imaginary_zero: f64,
	overlap: Complex64,
}
impl OverlapObservation {
	/// Native API calls in this successful run, per process/rank.
	#[must_use]
	pub const fn native_dispatches(self) -> super::NativeDispatchReport {
		self.dispatches
	}
	#[must_use]
	pub const fn overlap(self) -> Complex64 {
		self.overlap
	}
	#[must_use]
	pub const fn retained_mass(self) -> f64 {
		self.retained_mass
	}
	#[must_use]
	pub const fn active_mass(self) -> f64 {
		self.active_mass
	}
	#[must_use]
	pub const fn real_zero_mass(self) -> f64 {
		self.real_zero
	}
	#[must_use]
	pub const fn imaginary_zero_mass(self) -> f64 {
		self.imaginary_zero
	}
	#[must_use]
	pub fn transformed_norm(self) -> f64 {
		self.active_mass.mul(2.0).sqrt()
	}
	#[must_use]
	pub fn normalized_overlap(self) -> Option<Complex64> {
		let norm = self.transformed_norm();
		(norm > 0.0).then(|| self.overlap.div(norm))
	}
	#[must_use]
	pub fn real_zero_conditioned(self) -> Option<f64> {
		(self.retained_mass > 0.0).then(|| self.real_zero.div(self.retained_mass))
	}
	#[must_use]
	pub fn imaginary_zero_conditioned(self) -> Option<f64> {
		(self.retained_mass > 0.0).then(|| self.imaginary_zero.div(self.retained_mass))
	}
}
