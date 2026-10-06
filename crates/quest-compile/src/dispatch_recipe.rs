//! Pure description of the native dispatch and numerical preparation used by the facade.
//! Counts describe API calls and payloads, not backend kernels or allocator peaks.
use crate::{
	BoundGate, Control, ControlState, Error, NumericalOperator, Operation, OracleFragment,
	RegionPlan, Result,
};
use num_complex::Complex64;
use std::collections::BTreeSet;

/// A primitive gate with a single native dispatch.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum PrimitiveGate {
	H,
	X,
	Y,
	Z,
	Swap,
	Rx(f64),
	Ry(f64),
	Rz(f64),
}

/// Each phase step applies X to negative controls before and after the native call.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum DispatchStep {
	Native(PrimitiveGate),
	PhaseGate(f64),
	ScalarPhase(f64),
}

/// At most four ordered logical steps, so constructing a recipe cannot allocate.
#[derive(Debug, Clone, Copy)]
pub struct GateRecipe {
	steps: [Option<DispatchStep>; 4],
	len: usize,
	zero_controls: usize,
	native_calls: usize,
}
impl GateRecipe {
	pub fn steps(self) -> impl Iterator<Item = DispatchStep> {
		self.steps.into_iter().take(self.len).flatten()
	}
	#[must_use]
	pub const fn zero_controls(self) -> usize {
		self.zero_controls
	}
	#[must_use]
	pub const fn native_calls(self) -> usize {
		self.native_calls
	}
	/// One-sided logical state passes. Density global phase without controls
	/// retains its native API call but has no state pass.
	#[must_use]
	pub fn logical_state_passes(self, density: bool, has_controls: bool) -> usize {
		if density && !has_controls {
			self.native_calls.saturating_sub(
				self.steps()
					.filter(|step| matches!(step, DispatchStep::ScalarPhase(_)))
					.count(),
			)
		} else {
			self.native_calls
		}
	}
}

fn recipe(steps: &[DispatchStep], zero_controls: usize) -> Result<GateRecipe> {
	let phase_calls = zero_controls
		.checked_mul(2)
		.and_then(|x| x.checked_add(1))
		.ok_or(Error::Budget("dispatch count overflow"))?;
	let native_calls = steps.iter().try_fold(0usize, |sum, step| {
		sum.checked_add(match step {
			DispatchStep::Native(_) => 1,
			DispatchStep::PhaseGate(_) | DispatchStep::ScalarPhase(_) => phase_calls,
		})
		.ok_or(Error::Budget("dispatch count overflow"))
	})?;
	let mut result = [None; 4];
	for (out, step) in result.iter_mut().zip(steps) {
		*out = Some(*step);
	}
	Ok(GateRecipe {
		steps: result,
		len: steps.len(),
		zero_controls,
		native_calls,
	})
}

/// Describe the exact ordered lowering of a bound gate.
/// # Errors
/// Rejects dispatch-count overflow.
pub fn gate_recipe(gate: &BoundGate, zero_controls: usize) -> Result<GateRecipe> {
	use DispatchStep::{Native, PhaseGate as Phase, ScalarPhase as Scalar};
	use PrimitiveGate as P;
	use std::f64::consts::{FRAC_PI_2, FRAC_PI_4};
	match gate {
		BoundGate::Id => recipe(&[], zero_controls),
		BoundGate::H => recipe(&[Native(P::H)], zero_controls),
		BoundGate::X => recipe(&[Native(P::X)], zero_controls),
		BoundGate::Y => recipe(&[Native(P::Y)], zero_controls),
		BoundGate::Z => recipe(&[Native(P::Z)], zero_controls),
		BoundGate::Swap => recipe(&[Native(P::Swap)], zero_controls),
		BoundGate::Rx(x) => recipe(&[Native(P::Rx(*x))], zero_controls),
		BoundGate::Ry(x) => recipe(&[Native(P::Ry(*x))], zero_controls),
		BoundGate::Rz(x) => recipe(&[Native(P::Rz(*x))], zero_controls),
		BoundGate::S => recipe(&[Phase(FRAC_PI_2)], zero_controls),
		BoundGate::Sdg => recipe(&[Phase(-FRAC_PI_2)], zero_controls),
		BoundGate::T => recipe(&[Phase(FRAC_PI_4)], zero_controls),
		BoundGate::Tdg => recipe(&[Phase(-FRAC_PI_4)], zero_controls),
		BoundGate::Phase(x) => recipe(&[Phase(*x)], zero_controls),
		BoundGate::Sx => recipe(
			&[Native(P::Rx(FRAC_PI_2)), Scalar(FRAC_PI_4)],
			zero_controls,
		),
		BoundGate::Sxdg => recipe(
			&[Native(P::Rx(-FRAC_PI_2)), Scalar(-FRAC_PI_4)],
			zero_controls,
		),
		BoundGate::U { theta, phi, lambda } => recipe(
			&[
				Phase(*lambda),
				Native(P::Ry(*theta)),
				Phase(*phi),
				Scalar(theta / 2.0),
			],
			zero_controls,
		),
	}
}

/// Describe a standalone scalar phase, including its signed-control toggles.
/// # Errors
/// Rejects dispatch-count overflow.
pub fn scalar_phase_recipe(angle: f64, zero_controls: usize) -> Result<GateRecipe> {
	recipe(&[DispatchStep::ScalarPhase(angle)], zero_controls)
}

/// The facade's exact signed-control embedding and two native matrix variants.
#[derive(Debug, Clone, Copy)]
pub struct MatrixRecipe<'a> {
	source: &'a NumericalOperator,
	controls: &'a [bool],
	dimension: usize,
	active: usize,
}
impl<'a> MatrixRecipe<'a> {
	/// # Errors
	/// Rejects dimensions that cannot be represented or sized.
	pub fn new(source: &'a NumericalOperator, controls: &'a [bool]) -> Result<Self> {
		let factor = 1usize
			.checked_shl(u32::try_from(controls.len()).map_err(|_| Error::MatrixDimension)?)
			.ok_or(Error::MatrixDimension)?;
		let dimension = source
			.dimension()
			.checked_mul(factor)
			.ok_or(Error::MatrixDimension)?
			.max(2);
		// The native bridge represents dimensions and indices with signed 64-bit integers.
		i64::try_from(dimension).map_err(|_| Error::MatrixDimension)?;
		let active = controls
			.iter()
			.enumerate()
			.fold(0usize, |mask, (bit, enabled)| {
				mask | (usize::from(*enabled) << bit)
			});
		Ok(Self {
			source,
			controls,
			dimension,
			active,
		})
	}
	#[must_use]
	pub const fn dimension(self) -> usize {
		self.dimension
	}
	#[must_use]
	pub const fn width(self) -> u32 {
		self.dimension.ilog2()
	}
	#[must_use]
	pub const fn is_diagonal(self) -> bool {
		self.source.is_diagonal()
	}
	/// Native dense control dispatch is authorized only by this operator's evidence.
	#[must_use]
	pub const fn uses_native_controls(self) -> bool {
		self.source.unitary_evidence().is_some() && !self.source.is_diagonal()
	}
	#[must_use]
	pub fn native_dimension(self) -> usize {
		if self.uses_native_controls() {
			self.source.dimension()
		} else {
			self.dimension
		}
	}
	#[must_use]
	pub const fn native_variant_count(self) -> usize {
		2
	}
	#[must_use]
	pub const fn native_apply_calls(self, density: bool) -> usize {
		if density && !self.uses_native_controls() {
			2
		} else {
			1
		}
	}
	#[must_use]
	pub const fn signed_profile(self) -> &'a [bool] {
		self.controls
	}
	#[must_use]
	pub fn storage_identity(self) -> usize {
		self.source.view().as_ptr().addr()
	}
	/// Bytes of native forward and adjoint entries, excluding padding and GPU mirrors.
	/// # Errors
	/// Rejects byte-count overflow.
	pub fn payload_bytes(self) -> Result<usize> {
		let entries = if self.is_diagonal() {
			self.native_dimension()
		} else {
			self.native_dimension()
				.checked_mul(self.native_dimension())
				.ok_or(Error::Budget("matrix payload overflow"))?
		};
		entries
			.checked_mul(size_of::<Complex64>())
			.and_then(|n| n.checked_mul(2))
			.ok_or(Error::Budget("matrix payload overflow"))
	}
	/// Forward entry of the complete logical signed-control operator.
	#[must_use]
	#[expect(
		clippy::arithmetic_side_effects,
		reason = "source matrix admission guarantees a nonzero dimension"
	)]
	pub fn value(self, row: usize, col: usize) -> Complex64 {
		let local = self.source.dimension();
		if local == 1 && self.controls.is_empty() {
			if row == col {
				self.source.view()[(0, 0)]
			} else {
				Complex64::new(0.0, 0.0)
			}
		} else if row / local == self.active && col / local == self.active {
			self.source.view()[(row % local, col % local)]
		} else if row == col {
			Complex64::new(1.0, 0.0)
		} else {
			Complex64::new(0.0, 0.0)
		}
	}
	/// Adjoint entry of the complete logical signed-control operator.
	#[must_use]
	pub fn adjoint_value(self, row: usize, col: usize) -> Complex64 {
		self.value(col, row).conj()
	}
}

/// Finite ceilings applied before recursively expanding oracle work or publishing payloads.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RecipeLimits {
	work: usize,
	profiles: usize,
	storage_bytes: usize,
}
impl Default for RecipeLimits {
	fn default() -> Self {
		Self {
			work: 10_000_000,
			profiles: 1_000_000,
			storage_bytes: 256 * 1024 * 1024,
		}
	}
}
impl RecipeLimits {
	/// # Errors
	/// Rejects ceilings above the shared task-wide hard bounds.
	pub fn new(work: usize, profiles: usize, storage_bytes: usize) -> Result<Self> {
		let default = Self::default();
		if work > default.work
			|| profiles > default.profiles
			|| storage_bytes > default.storage_bytes
		{
			return Err(Error::Budget("dispatch recipe limits"));
		}
		Ok(Self {
			work,
			profiles,
			storage_bytes,
		})
	}
	#[must_use]
	pub const fn with_storage_cap(mut self, bytes: usize) -> Self {
		if bytes < self.storage_bytes {
			self.storage_bytes = bytes;
		}
		self
	}
	#[must_use]
	pub const fn work(self) -> usize {
		self.work
	}
	#[must_use]
	pub const fn profiles(self) -> usize {
		self.profiles
	}
	#[must_use]
	pub const fn storage_bytes(self) -> usize {
		self.storage_bytes
	}
}

/// Pure payload and dispatch inventory. Identity matches the facade's matrix cache:
/// matrix storage address plus the complete ordered signed-control profile.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PreparedRecipeInventory {
	density: bool,
	unique_matrices: usize,
	native_variants: usize,
	payload_bytes: usize,
	native_apply_calls: usize,
	native_dispatches: usize,
	logical_state_passes: usize,
	work: usize,
	retained_bytes: usize,
}
impl PreparedRecipeInventory {
	/// Analyze a coherent executable, expanding nested oracle occurrences.
	/// # Errors
	/// Rejects dynamic effects, excessive nesting and checked arithmetic overflow.
	pub fn from_plan(plan: &RegionPlan, density: bool) -> Result<Self> {
		Self::from_plan_with_limits(plan, density, RecipeLimits::default())
	}
	/// Analyze under caller-reserved ceilings from the shared optimization ledger.
	/// # Errors
	/// Rejects exhausted work, profile or retained storage allowance.
	pub fn from_plan_with_limits(
		plan: &RegionPlan,
		density: bool,
		limits: RecipeLimits,
	) -> Result<Self> {
		let mut walk = InventoryWalker {
			seen: BTreeSet::new(),
			result: Self {
				density,
				unique_matrices: 0,
				native_variants: 0,
				payload_bytes: 0,
				native_apply_calls: 0,
				native_dispatches: 0,
				logical_state_passes: 0,
				work: 0,
				retained_bytes: 0,
			},
			density,
			width: plan.num_qubits(),
			limits,
			work: 0,
			storage_bytes: 0,
		};
		for instruction in plan.instructions() {
			walk.operation(instruction.operation(), &[], 0, 0)?;
		}
		walk.result.work = walk.work;
		walk.result.retained_bytes = walk.storage_bytes;
		Ok(walk.result)
	}
	#[must_use]
	pub const fn unique_matrices(self) -> usize {
		self.unique_matrices
	}
	#[must_use]
	pub const fn density(self) -> bool {
		self.density
	}
	#[must_use]
	pub const fn native_variants(self) -> usize {
		self.native_variants
	}
	#[must_use]
	pub const fn payload_bytes(self) -> usize {
		self.payload_bytes
	}
	#[must_use]
	pub const fn native_apply_calls(self) -> usize {
		self.native_apply_calls
	}
	#[must_use]
	pub const fn native_dispatches(self) -> usize {
		self.native_dispatches
	}
	#[must_use]
	pub const fn logical_state_passes(self) -> usize {
		self.logical_state_passes
	}
	#[must_use]
	pub const fn work(self) -> usize {
		self.work
	}
	#[must_use]
	pub const fn retained_bytes(self) -> usize {
		self.retained_bytes
	}
}
struct InventoryWalker {
	seen: BTreeSet<(usize, Vec<bool>, [u64; 3])>,
	result: PreparedRecipeInventory,
	density: bool,
	width: usize,
	limits: RecipeLimits,
	work: usize,
	storage_bytes: usize,
}
impl InventoryWalker {
	#[expect(
		clippy::too_many_lines,
		reason = "One exhaustive operation walk preserves native dispatch and payload accounting together"
	)]
	#[expect(
		clippy::set_contains_or_insert,
		reason = "Budget admission must precede publication of a cache key"
	)]
	fn operation(
		&mut self,
		op: &Operation,
		inherited: &[bool],
		depth: usize,
		scratch_live: usize,
	) -> Result<()> {
		self.work = self
			.work
			.checked_add(1)
			.ok_or(Error::Budget("dispatch recipe work"))?;
		if self.work > self.limits.work {
			return Err(Error::Budget("dispatch recipe work"));
		}
		if depth > 64 {
			return Err(Error::Budget("oracle nesting"));
		}
		match op {
			Operation::Gate { gate, controls, .. } => {
				self.charge_work(
					inherited
						.len()
						.checked_add(controls.len())
						.ok_or(Error::Budget("dispatch recipe work"))?,
				)?;
				let zeros = inherited
					.iter()
					.filter(|&&state| !state)
					.count()
					.checked_add(
						controls
							.iter()
							.filter(|c| c.state() == ControlState::Zero)
							.count(),
					)
					.ok_or(Error::Budget("dispatch count overflow"))?;
				let recipe = gate_recipe(gate, zeros)?;
				self.add_dispatches(recipe.native_calls())?;
				self.add_passes(recipe.logical_state_passes(
					self.density,
					!inherited.is_empty() || !controls.is_empty(),
				))?;
			}
			Operation::GlobalPhase { controls, radians } => {
				self.charge_work(
					inherited
						.len()
						.checked_add(controls.len())
						.ok_or(Error::Budget("dispatch recipe work"))?,
				)?;
				let zeros = inherited
					.iter()
					.filter(|&&state| !state)
					.count()
					.checked_add(
						controls
							.iter()
							.filter(|c| c.state() == ControlState::Zero)
							.count(),
					)
					.ok_or(Error::Budget("dispatch count overflow"))?;
				let recipe = scalar_phase_recipe(*radians, zeros)?;
				self.add_dispatches(recipe.native_calls())?;
				self.add_passes(recipe.logical_state_passes(
					self.density,
					!inherited.is_empty() || !controls.is_empty(),
				))?;
			}
			Operation::Numerical {
				matrix, controls, ..
			} => {
				let profile_len = inherited
					.len()
					.checked_add(controls.len())
					.ok_or(Error::Budget("control profile"))?;
				self.charge_work(profile_len)?;
				let scratch = scratch_live
					.checked_add(
						profile_len
							.checked_mul(2)
							.ok_or(Error::Budget("dispatch recipe storage"))?,
					)
					.ok_or(Error::Budget("dispatch recipe storage"))?;
				self.check_scratch(scratch)?;
				let profile = extended_profile(inherited, controls, self.width)?;
				let recipe = MatrixRecipe::new(matrix, &profile)?;
				let calls = recipe.native_apply_calls(self.density);
				self.result.native_apply_calls = self
					.result
					.native_apply_calls
					.checked_add(calls)
					.ok_or(Error::Budget("dispatch count overflow"))?;
				self.add_dispatches(calls)?;
				self.add_passes(1)?;
				let key = (
					recipe.storage_identity(),
					profile.clone(),
					matrix.evidence_identity(),
				);
				let comparisons = usize::try_from(
					self.seen
						.len()
						.checked_add(1)
						.ok_or(Error::Budget("dispatch recipe work"))?
						.ilog2(),
				)
				.map_err(|_| Error::Budget("dispatch recipe work"))?;
				let comparisons = comparisons
					.checked_add(2)
					.and_then(|n| n.checked_mul(profile_len.checked_add(1)?))
					.and_then(|n| n.checked_mul(2))
					.ok_or(Error::Budget("dispatch recipe work"))?;
				self.charge_work(comparisons)?;
				// Admission must precede publication of a new cache key.
				if !self.seen.contains(&key) {
					let next_profiles = self
						.result
						.unique_matrices
						.checked_add(1)
						.ok_or(Error::Budget("dispatch recipe profiles"))?;
					if next_profiles > self.limits.profiles {
						return Err(Error::Budget("dispatch recipe profiles"));
					}
					let payload = recipe.payload_bytes()?;
					let retained = size_of::<(usize, Vec<bool>, [u64; 3])>()
						.checked_add(profile.len())
						.and_then(|n| n.checked_add(64))
						.and_then(|n| n.checked_add(payload))
						.ok_or(Error::Budget("dispatch recipe storage"))?;
					self.storage_bytes = self
						.storage_bytes
						.checked_add(retained)
						.ok_or(Error::Budget("dispatch recipe storage"))?;
					self.check_scratch(scratch)?;
					self.seen.insert(key);
					self.result.unique_matrices = self
						.result
						.unique_matrices
						.checked_add(1)
						.ok_or(Error::Budget("matrix count overflow"))?;
					self.result.native_variants = self
						.result
						.native_variants
						.checked_add(recipe.native_variant_count())
						.ok_or(Error::Budget("matrix count overflow"))?;
					self.result.payload_bytes = self
						.result
						.payload_bytes
						.checked_add(recipe.payload_bytes()?)
						.ok_or(Error::Budget("matrix payload overflow"))?;
				}
			}
			Operation::Oracle {
				fragment, controls, ..
			} => {
				let profile_len = inherited
					.len()
					.checked_add(controls.len())
					.ok_or(Error::Budget("control profile"))?;
				self.charge_work(profile_len)?;
				let next_scratch = scratch_live
					.checked_add(profile_len)
					.ok_or(Error::Budget("dispatch recipe storage"))?;
				self.check_scratch(next_scratch)?;
				let profile = extended_profile(inherited, controls, self.width)?;
				if profile
					.len()
					.checked_add(fragment.num_qubits())
					.ok_or(Error::Budget("oracle width"))?
					> self.width
				{
					return Err(Error::Budget("oracle width"));
				}
				for nested in fragment.operations() {
					self.operation(
						nested,
						&profile,
						depth
							.checked_add(1)
							.ok_or(Error::Budget("oracle nesting"))?,
						next_scratch,
					)?;
				}
			}
			Operation::Barrier { .. } => {}
			Operation::Conditional { .. }
			| Operation::Measure { .. }
			| Operation::Reset { .. }
			| Operation::Channel { .. } => {
				return Err(Error::Unsupported(
					"dynamic or stochastic dispatch inventory",
				));
			}
		}
		Ok(())
	}
	fn add_dispatches(&mut self, calls: usize) -> Result<()> {
		self.result.native_dispatches = self
			.result
			.native_dispatches
			.checked_add(calls)
			.ok_or(Error::Budget("dispatch count overflow"))?;
		Ok(())
	}
	fn add_passes(&mut self, passes: usize) -> Result<()> {
		self.result.logical_state_passes = self
			.result
			.logical_state_passes
			.checked_add(passes)
			.ok_or(Error::Budget("state pass count overflow"))?;
		Ok(())
	}
	fn charge_work(&mut self, count: usize) -> Result<()> {
		self.work = self
			.work
			.checked_add(count)
			.ok_or(Error::Budget("dispatch recipe work"))?;
		if self.work > self.limits.work {
			return Err(Error::Budget("dispatch recipe work"));
		}
		Ok(())
	}
	fn check_scratch(&self, scratch: usize) -> Result<()> {
		if self
			.storage_bytes
			.checked_add(scratch)
			.ok_or(Error::Budget("dispatch recipe storage"))?
			> self.limits.storage_bytes
		{
			return Err(Error::Budget("dispatch recipe storage"));
		}
		Ok(())
	}
}
fn extended_profile(inherited: &[bool], controls: &[Control], width: usize) -> Result<Vec<bool>> {
	let count = inherited
		.len()
		.checked_add(controls.len())
		.ok_or(Error::Budget("control profile"))?;
	if count > width {
		return Err(Error::Budget("control profile"));
	}
	let mut profile = Vec::new();
	profile
		.try_reserve_exact(count)
		.map_err(|_| Error::Budget("control profile allocation"))?;
	profile.extend(
		inherited
			.iter()
			.copied()
			.chain(controls.iter().map(|c| c.state() == ControlState::One)),
	);
	Ok(profile)
}

/// A body and full inherited signed profile requiring native preparation.
#[derive(Debug, Clone)]
pub struct OracleProfile {
	fragment: OracleFragment,
	signed_controls: Vec<bool>,
	depth: usize,
}

/// Conservative live bytes of the temporary discovery result, including Vec growth.
/// # Errors
/// Rejects byte-count overflow.
pub fn oracle_profile_storage_bytes(profiles: &[OracleProfile]) -> Result<usize> {
	profiles.iter().try_fold(0usize, |total, profile| {
		total
			.checked_add(256)
			.and_then(|n| n.checked_add(profile.signed_controls.len()))
			.ok_or(Error::Budget("oracle profile storage"))
	})
}
impl OracleProfile {
	#[must_use]
	pub const fn fragment(&self) -> &OracleFragment {
		&self.fragment
	}
	#[must_use]
	pub fn signed_controls(&self) -> &[bool] {
		&self.signed_controls
	}
	#[must_use]
	pub const fn depth(&self) -> usize {
		self.depth
	}
}

/// Discover unique body/profile pairs across nested calls. The facade consumes
/// this same discovery for its native oracle cache.
/// # Errors
/// Rejects invalid width, excessive depth and profile arithmetic overflow.
pub fn discover_oracle_profiles(
	fragment: &OracleFragment,
	controls: &[bool],
	depth: usize,
	max_qubits: usize,
) -> Result<Vec<OracleProfile>> {
	discover_oracle_profiles_with_limits(
		fragment,
		controls,
		depth,
		max_qubits,
		RecipeLimits::default(),
	)
}

/// Discover under caller-reserved work, profile and retained storage ceilings.
/// # Errors
/// Rejects exhausted allowance before expanding another body/profile pair.
#[expect(
	clippy::too_many_lines,
	reason = "Bounded recursive discovery keeps profile admission and traversal together"
)]
pub fn discover_oracle_profiles_with_limits(
	fragment: &OracleFragment,
	controls: &[bool],
	depth: usize,
	max_qubits: usize,
	limits: RecipeLimits,
) -> Result<Vec<OracleProfile>> {
	#[expect(
		clippy::too_many_arguments,
		reason = "Recursive discovery carries the caller's three independent finite ceilings"
	)]
	fn include(
		result: &mut Vec<OracleProfile>,
		fragment: &OracleFragment,
		controls: &[bool],
		depth: usize,
		max_qubits: usize,
		limits: RecipeLimits,
		work: &mut usize,
		storage_bytes: &mut usize,
		scratch_live: usize,
	) -> Result<()> {
		*work = work
			.checked_add(1)
			.ok_or(Error::Budget("oracle profile work"))?;
		if *work > limits.work {
			return Err(Error::Budget("oracle profile work"));
		}
		if depth > 64
			|| controls
				.len()
				.checked_add(fragment.num_qubits())
				.ok_or(Error::Budget("oracle interface"))?
				> max_qubits
		{
			return Err(Error::Budget("oracle interface"));
		}
		let comparisons = result
			.len()
			.checked_mul(
				controls
					.len()
					.checked_add(1)
					.ok_or(Error::Budget("oracle profile work"))?,
			)
			.ok_or(Error::Budget("oracle profile work"))?;
		*work = work
			.checked_add(comparisons)
			.ok_or(Error::Budget("oracle profile work"))?;
		if *work > limits.work {
			return Err(Error::Budget("oracle profile work"));
		}
		if let Some(previous) = result
			.iter_mut()
			.find(|p| p.fragment.shares_storage_with(fragment) && p.signed_controls == controls)
		{
			if previous.depth >= depth {
				return Ok(());
			}
			previous.depth = depth;
		} else {
			if result.len() >= limits.profiles {
				return Err(Error::Budget("oracle profile count"));
			}
			let retained = 256usize
				.checked_add(controls.len())
				.ok_or(Error::Budget("oracle profile storage"))?;
			*storage_bytes = storage_bytes
				.checked_add(retained)
				.ok_or(Error::Budget("oracle profile storage"))?;
			if storage_bytes
				.checked_add(scratch_live)
				.ok_or(Error::Budget("oracle profile storage"))?
				> limits.storage_bytes
			{
				return Err(Error::Budget("oracle profile storage"));
			}
			result
				.try_reserve(1)
				.map_err(|_| Error::Budget("oracle profile allocation"))?;
			result.push(OracleProfile {
				fragment: fragment.clone(),
				signed_controls: controls.to_vec(),
				depth,
			});
		}
		for operation in fragment.operations() {
			*work = work
				.checked_add(1)
				.ok_or(Error::Budget("oracle profile work"))?;
			if *work > limits.work {
				return Err(Error::Budget("oracle profile work"));
			}
			if let Operation::Oracle {
				fragment: nested,
				controls: local,
				..
			} = operation
			{
				let len = controls
					.len()
					.checked_add(local.len())
					.ok_or(Error::Budget("oracle profile storage"))?;
				*work = work
					.checked_add(len)
					.ok_or(Error::Budget("oracle profile work"))?;
				if *work > limits.work {
					return Err(Error::Budget("oracle profile work"));
				}
				let next_scratch = scratch_live
					.checked_add(len)
					.ok_or(Error::Budget("oracle profile storage"))?;
				if storage_bytes
					.checked_add(next_scratch)
					.ok_or(Error::Budget("oracle profile storage"))?
					> limits.storage_bytes
				{
					return Err(Error::Budget("oracle profile storage"));
				}
				let mut profile = Vec::new();
				profile
					.try_reserve_exact(len)
					.map_err(|_| Error::Budget("oracle profile allocation"))?;
				profile.extend(
					controls
						.iter()
						.copied()
						.chain(local.iter().map(|c| c.state() == ControlState::One)),
				);
				let next_scratch = scratch_live
					.checked_add(profile.capacity())
					.ok_or(Error::Budget("oracle profile storage"))?;
				if storage_bytes
					.checked_add(next_scratch)
					.ok_or(Error::Budget("oracle profile storage"))?
					> limits.storage_bytes
				{
					return Err(Error::Budget("oracle profile storage"));
				}
				include(
					result,
					nested,
					&profile,
					depth
						.checked_add(1)
						.ok_or(Error::Budget("oracle nesting"))?,
					max_qubits,
					limits,
					work,
					storage_bytes,
					next_scratch,
				)?;
			}
		}
		Ok(())
	}
	let mut result = Vec::new();
	let mut work = 0;
	let mut storage_bytes = 0;
	include(
		&mut result,
		fragment,
		controls,
		depth,
		max_qubits,
		limits,
		&mut work,
		&mut storage_bytes,
		0,
	)?;
	Ok(result)
}
