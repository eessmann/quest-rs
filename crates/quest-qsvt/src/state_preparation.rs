//! Coherent amplitude-tree preparation through uniformly controlled rotations.
//!
//! Möttönen et al., quant-ph/0407010, supplies the Gray-code multiplexor identity.
//! This bounded stored-table baseline charges compilation and elementary gates;
//! binary64 angle construction is not an independent error certificate. It never
//! constructs a dense preparation isometry or a complete gate stream.
#![allow(
	clippy::arithmetic_side_effects,
	clippy::indexing_slicing,
	clippy::suboptimal_flops,
	reason = "Power-of-two dimensions and complete simultaneous storage/work are admitted before indexed tree construction"
)]
use crate::{Complex64, Error, ReplayGate, ReplayKind, Result};
use std::sync::Arc;

#[derive(Clone, Copy, Debug)]
pub struct PreparationLimits {
	pub max_dimension: usize,
	pub max_bytes: usize,
	pub max_compile_work: usize,
	pub max_gates: usize,
}
impl Default for PreparationLimits {
	fn default() -> Self {
		Self {
			max_dimension: 1_048_576,
			max_bytes: 268_435_456,
			max_compile_work: 67_108_864,
			max_gates: 8_388_608,
		}
	}
}
#[derive(Clone, Copy, Debug)]
pub struct PreparationResources {
	pub padded_dimension: usize,
	pub coefficients: usize,
	pub elementary_gates: usize,
	pub compile_work: usize,
	pub retained_bytes: usize,
	pub construction_peak_bytes: usize,
}
#[derive(Debug)]
struct Data {
	qubits: usize,
	norm: f64,
	phase: f64,
	y: Vec<f64>,
	z: Vec<f64>,
	resources: PreparationResources,
}
/// Immutable compiled tree. Clones share coefficient storage.
#[derive(Clone, Debug)]
pub struct AmplitudePreparation(Arc<Data>);
impl AmplitudePreparation {
	/// Compile a padded amplitude tree and its Gray-code Walsh coefficients.
	///
	/// The physical input norm is retained. Zero inputs have no normalized quantum
	/// state and are rejected. Caller storage beyond this borrowed slice and opaque
	/// allocator bookkeeping are external to the explicit application-byte model.
	/// # Errors
	/// Rejects zero/nonfinite norms, dimension/precision overflow and resource limits.
	pub fn new(amplitudes: &[Complex64], limits: PreparationLimits) -> Result<Self> {
		if amplitudes.is_empty() {
			return Err(Error::Encoding("empty preparation input"));
		}
		let dimension = amplitudes
			.len()
			.checked_next_power_of_two()
			.ok_or(Error::Budget("preparation dimension"))?;
		let qubits =
			usize::try_from(dimension.ilog2()).map_err(|_| Error::Budget("preparation width"))?;
		let internal = dimension
			.checked_sub(1)
			.ok_or(Error::Budget("preparation nodes"))?;
		let coefficients = internal
			.checked_mul(2)
			.and_then(|n| n.checked_add(1))
			.ok_or(Error::Budget("preparation coefficients"))?;
		let gates = if dimension == 1 {
			1
		} else {
			dimension
				.checked_mul(5)
				.and_then(|n| n.checked_sub(6))
				.ok_or(Error::Budget("preparation gates"))?
		};
		let compile_work = qubits
			.checked_mul(2)
			.and_then(|n| n.checked_add(8))
			.and_then(|n| n.checked_mul(dimension))
			.ok_or(Error::Budget("preparation work"))?;
		let retained = coefficients
			.checked_mul(size_of::<f64>())
			.and_then(|n| n.checked_add(size_of::<Data>() + 64))
			.ok_or(Error::Budget("preparation storage"))?;
		let peak = dimension
			.checked_mul(4 * size_of::<f64>())
			.and_then(|n| n.checked_add(retained))
			.and_then(|n| n.checked_add(amplitudes.len().checked_mul(size_of::<Complex64>())?))
			.ok_or(Error::Budget("preparation construction storage"))?;
		if dimension > limits.max_dimension
			|| peak > limits.max_bytes
			|| compile_work > limits.max_compile_work
			|| gates > limits.max_gates
		{
			return Err(Error::Budget("amplitude preparation admission"));
		}
		let scale = input_scale(amplitudes)?;
		let tree_size = dimension
			.checked_mul(2)
			.ok_or(Error::Budget("preparation tree"))?;
		let mut mass = zeroes(tree_size)?;
		let mut phases = zeroes(tree_size)?;
		let mut y = zeroes(internal)?;
		let mut z = zeroes(internal)?;
		let (retained, peak) = admit_construction_capacities(
			[
				mass.capacity(),
				phases.capacity(),
				y.capacity(),
				z.capacity(),
			],
			amplitudes.len(),
			limits.max_bytes,
		)?;
		for (index, value) in amplitudes.iter().enumerate() {
			mass[dimension + index] = (value.re / scale).hypot(value.im / scale);
			phases[dimension + index] = if value.re == 0.0 && value.im == 0.0 {
				0.0
			} else {
				value.im.atan2(value.re)
			};
		}
		for node in (1..dimension).rev() {
			let (left, right) = (mass[2 * node], mass[2 * node + 1]);
			mass[node] = left.hypot(right);
			y[node - 1] = 2.0 * right.atan2(left);
			z[node - 1] = phases[2 * node + 1] - phases[2 * node];
			phases[node] = 0.5 * phases[2 * node] + 0.5 * phases[2 * node + 1];
		}
		let norm = scale * mass[1];
		if !norm.is_finite() || norm <= 0.0 {
			return Err(Error::Encoding("unrepresentable preparation norm"));
		}
		transform_coefficients(qubits, &mut y, &mut z);
		let phase = phases[1];
		let resources = PreparationResources {
			padded_dimension: dimension,
			coefficients,
			elementary_gates: gates,
			compile_work,
			retained_bytes: retained,
			construction_peak_bytes: peak,
		};
		Ok(Self(Arc::new(Data {
			qubits,
			norm,
			phase,
			y,
			z,
			resources,
		})))
	}
	#[must_use]
	pub fn qubits(&self) -> usize {
		self.0.qubits
	}
	#[must_use]
	pub fn norm(&self) -> f64 {
		self.0.norm
	}
	#[must_use]
	pub fn resources(&self) -> PreparationResources {
		self.0.resources
	}
	/// Actual coefficient Vec capacities plus the existing scalar/Arc metadata allowance.
	/// Excludes allocator bookkeeping and the caller's retained input storage.
	/// # Errors
	/// Rejects integer byte-accounting overflow.
	pub fn retained_bytes(&self) -> Result<usize> {
		coefficient_bytes(self.0.y.capacity(), self.0.z.capacity())
	}
	/// Independent preparation error evidence is not supplied by binary64 synthesis.
	#[must_use]
	pub const fn certified_error_bound(&self) -> Option<f64> {
		None
	}
	/// Apply the gate stream to an explicitly bounded classical reference state.
	/// # Errors
	/// Rejects state/operand/byte/work admission before mutation. Numerical failure
	/// after mutation is reported without promising transactional state recovery.
	pub fn apply_reference(
		&self,
		state: &mut [Complex64],
		targets: &[usize],
		adjoint: bool,
		policy: crate::NumericalPolicy,
		max_state_gate_work: usize,
	) -> Result<()> {
		crate::owned_replay::validate_mapping(self.qubits(), targets, 0, 0)?;
		let width = usize::try_from(
			state
				.len()
				.checked_ilog2()
				.ok_or(Error::Encoding("empty preparation state"))?,
		)
		.map_err(|_| Error::Budget("preparation state width"))?;
		if !state.len().is_power_of_two()
			|| targets.iter().any(|target| *target >= width)
			|| state
				.len()
				.checked_mul(self.resources().elementary_gates)
				.is_none_or(|n| n > max_state_gate_work)
			|| state
				.len()
				.checked_mul(size_of::<Complex64>())
				.and_then(|n| n.checked_add(self.resources().retained_bytes))
				.is_none_or(|n| n > policy.max_bytes)
		{
			return Err(Error::Budget("preparation reference execution"));
		}
		if state
			.iter()
			.any(|value| !value.re.is_finite() || !value.im.is_finite())
		{
			return Err(Error::NonFinite);
		}
		self.visit_mapped_gates(targets, 0, 0, adjoint, &mut |gate| {
			crate::owned_replay::apply_gate(state, gate)
		})?;
		if state
			.iter()
			.any(|value| !value.re.is_finite() || !value.im.is_finite())
		{
			return Err(Error::NonFinite);
		}
		Ok(())
	}
	/// Stream elementary single-qubit rotations, phases and CNOT gates.
	/// # Errors
	/// Stops immediately on visitor failure; no gate list is retained.
	pub fn visit_gates(
		&self,
		adjoint: bool,
		visitor: &mut dyn FnMut(ReplayGate) -> Result<()>,
	) -> Result<()> {
		visit_preparation_gates(
			self.0.qubits,
			self.0.phase,
			adjoint,
			&mut |depth, is_z, gray| {
				let index = (1usize << depth) - 1 + gray;
				Ok(if is_z {
					self.0.z[index]
				} else {
					self.0.y[index]
				})
			},
			visitor,
		)
	}

	/// Map data bits and add signed outer controls, preserving global phase.
	/// # Errors
	/// Rejects overlapping/invalid operands or propagates replay failure.
	pub fn visit_mapped_gates(
		&self,
		targets: &[usize],
		mask: usize,
		value: usize,
		adjoint: bool,
		visitor: &mut dyn FnMut(ReplayGate) -> Result<()>,
	) -> Result<()> {
		crate::owned_replay::validate_mapping(self.qubits(), targets, mask, value)?;
		self.visit_gates(adjoint, &mut |gate| {
			visitor(gate.mapped(targets, mask, value)?)
		})
	}
}
/// Replay the canonical amplitude-tree unitary from an immutable coefficient source.
///
/// The callback supplies the normalized Walsh coefficient at `(depth, is_z,
/// gray_index)`. Source ownership, coefficient validation and total replay work
/// must be admitted by the caller before execution. This helper allocates no
/// tables or gate stream and uses the same order as `AmplitudePreparation`.
/// A callback or visitor failure can occur after earlier gates were emitted.
/// # Errors
/// Rejects nonfinite phase, index/count overflow or nonfinite coefficients, and
/// propagates callback/visitor failures without rollback.
pub fn visit_preparation_gates(
	qubits: usize,
	phase: f64,
	adjoint: bool,
	coefficient: &mut dyn FnMut(usize, bool, usize) -> Result<f64>,
	visitor: &mut dyn FnMut(ReplayGate) -> Result<()>,
) -> Result<()> {
	let dimension = 1usize
		.checked_shl(u32::try_from(qubits).map_err(|_| Error::Budget("preparation width"))?)
		.ok_or(Error::Budget("preparation width"))?;
	if !phase.is_finite() {
		return Err(Error::NonFinite);
	}
	if dimension > 1
		&& dimension
			.checked_mul(5)
			.and_then(|d| d.checked_sub(6))
			.is_none()
	{
		return Err(Error::Budget("preparation gate count"));
	}
	if adjoint {
		visitor(phase_gate(-phase, 0, 0))?;
	}
	for family in 0..2 {
		let is_z = if adjoint { family == 0 } else { family == 1 };
		for ordinal in 0..qubits {
			let depth = if adjoint {
				qubits - 1 - ordinal
			} else {
				ordinal
			};
			multiplexor(qubits, coefficient, depth, is_z, adjoint, visitor)?;
		}
	}
	if !adjoint {
		visitor(phase_gate(phase, 0, 0))?;
	}
	Ok(())
}
fn multiplexor(
	qubits: usize,
	coefficient: &mut dyn FnMut(usize, bool, usize) -> Result<f64>,
	depth: usize,
	is_z: bool,
	adjoint: bool,
	visitor: &mut dyn FnMut(ReplayGate) -> Result<()>,
) -> Result<()> {
	let count = 1usize << depth;
	let target = qubits - 1 - depth;
	for ordinal in 0..count {
		let index = if adjoint {
			count - 1 - ordinal
		} else {
			ordinal
		};
		let gray = index ^ (index >> 1);
		let angle = coefficient(depth, is_z, gray)?;
		if !angle.is_finite() {
			return Err(Error::NonFinite);
		}
		let cnot = || {
			let next = (index + 1) % count;
			let change = gray ^ (next ^ (next >> 1));
			let control =
				target + 1 + usize::try_from(change.trailing_zeros()).unwrap_or(usize::MAX);
			ReplayGate {
				kind: ReplayKind::X,
				target: Some(target),
				control_mask: 1usize << control,
				control_value: 1usize << control,
			}
		};
		if adjoint && count > 1 {
			visitor(cnot())?;
		}
		let angle = if adjoint { -angle } else { angle };
		if is_z {
			let base = phase_gate(-0.5 * angle, 0, 0);
			let conditional = phase_gate(angle, 1usize << target, 1usize << target);
			if adjoint {
				visitor(conditional)?;
				visitor(base)?;
			} else {
				visitor(base)?;
				visitor(conditional)?;
			}
		} else {
			visitor(ReplayGate {
				kind: ReplayKind::Ry(angle),
				target: Some(target),
				control_mask: 0,
				control_value: 0,
			})?;
		}
		if !adjoint && count > 1 {
			visitor(cnot())?;
		}
	}
	Ok(())
}
const fn phase_gate(angle: f64, mask: usize, value: usize) -> ReplayGate {
	ReplayGate {
		kind: ReplayKind::Phase(angle),
		target: None,
		control_mask: mask,
		control_value: value,
	}
}
fn zeroes(count: usize) -> Result<Vec<f64>> {
	let mut values = Vec::new();
	values
		.try_reserve_exact(count)
		.map_err(|_| Error::Budget("preparation allocation"))?;
	values.resize(count, 0.0);
	Ok(values)
}
fn coefficient_bytes(y: usize, z: usize) -> Result<usize> {
	y.checked_add(z)
		.and_then(|n| n.checked_add(1))
		.and_then(|n| n.checked_mul(size_of::<f64>()))
		.and_then(|n| n.checked_add(size_of::<Data>() + 64))
		.ok_or(Error::Budget("preparation coefficient capacities"))
}
fn construction_bytes(mass: usize, phases: usize, retained: usize, input: usize) -> Result<usize> {
	mass.checked_add(phases)
		.and_then(|n| n.checked_mul(size_of::<f64>()))
		.and_then(|n| n.checked_add(retained))
		.and_then(|n| n.checked_add(input.checked_mul(size_of::<Complex64>())?))
		.ok_or(Error::Budget("preparation actual capacities"))
}
fn admit_construction_capacities(
	capacities: [usize; 4],
	input: usize,
	limit: usize,
) -> Result<(usize, usize)> {
	let [mass, phases, y, z] = capacities;
	let retained = coefficient_bytes(y, z)?;
	let peak = construction_bytes(mass, phases, retained, input)?;
	if peak > limit {
		return Err(Error::Budget("preparation actual capacities"));
	}
	Ok((retained, peak))
}
fn walsh(values: &mut [f64]) {
	let mut stride = 1;
	while stride < values.len() {
		for base in (0..values.len()).step_by(2 * stride) {
			for offset in 0..stride {
				let (a, b) = (values[base + offset], values[base + offset + stride]);
				values[base + offset] = 0.5 * a + 0.5 * b;
				values[base + offset + stride] = 0.5 * a - 0.5 * b;
			}
		}
		stride *= 2;
	}
}
fn transform_coefficients(qubits: usize, y: &mut [f64], z: &mut [f64]) {
	for depth in 0..qubits {
		let count = 1usize << depth;
		walsh(&mut y[count - 1..2 * count - 1]);
		walsh(&mut z[count - 1..2 * count - 1]);
	}
}

fn input_scale(amplitudes: &[Complex64]) -> Result<f64> {
	let mut scale = 0.0_f64;
	for value in amplitudes {
		if !value.re.is_finite() || !value.im.is_finite() {
			return Err(Error::Encoding("nonfinite preparation input"));
		}
		scale = scale.max(value.re.abs()).max(value.im.abs());
	}
	if scale == 0.0 {
		return Err(Error::Encoding("zero preparation norm"));
	}
	Ok(scale)
}

/// Map a logical preparation primitive and attach signed outer controls.
///
/// The source must already have admitted its primitive sequence. This checks
/// the operand map and local mask shape, including for scalar phases.
/// # Errors
/// Rejects overlapping maps, out-of-width logical controls, invalid control
/// values and unrepresentable bits; propagates missing logical target errors.
pub fn map_preparation_gate(
	gate: ReplayGate,
	targets: &[usize],
	mask: usize,
	value: usize,
) -> Result<ReplayGate> {
	crate::owned_replay::validate_mapping(targets.len(), targets, mask, value)?;
	let allowed = 1usize
		.checked_shl(
			u32::try_from(targets.len()).map_err(|_| Error::Budget("preparation map width"))?,
		)
		.and_then(|n| n.checked_sub(1))
		.ok_or(Error::Budget("preparation map width"))?;
	if gate.control_mask & !allowed != 0 || gate.control_value & !gate.control_mask != 0 {
		return Err(Error::Encoding("preparation logical control shape"));
	}
	if let Some(t) = gate.target {
		let bit = 1usize
			.checked_shl(u32::try_from(t).map_err(|_| Error::Budget("preparation target"))?)
			.ok_or(Error::Budget("preparation target"))?;
		if gate.control_mask & bit != 0 {
			return Err(Error::Encoding("preparation target/control overlap"));
		}
	}
	gate.mapped(targets, mask, value)
}

#[cfg(test)]
mod capacity_tests {
	use super::{AmplitudePreparation, Data, PreparationResources};
	use std::sync::Arc;

	#[googletest::gtest]
	fn retained_payload_counts_actual_coefficient_capacities() -> googletest::Result<()> {
		let mut y = Vec::with_capacity(64);
		y.push(0.0);
		let mut z = Vec::with_capacity(32);
		z.push(0.0);
		let expected = y
			.capacity()
			.checked_add(z.capacity())
			.and_then(|n| n.checked_add(1))
			.and_then(|n| n.checked_mul(size_of::<f64>()))
			.and_then(|n| n.checked_add(size_of::<Data>() + 64))
			.ok_or(crate::Error::Budget("test coefficient bytes"))?;
		let source = AmplitudePreparation(Arc::new(Data {
			qubits: 1,
			norm: 1.0,
			phase: 0.0,
			y,
			z,
			resources: PreparationResources {
				padded_dimension: 2,
				coefficients: 3,
				elementary_gates: 4,
				compile_work: 20,
				retained_bytes: 0,
				construction_peak_bytes: 0,
			},
		}));
		googletest::expect_eq!(source.retained_bytes()?, expected);
		Ok(())
	}
}
