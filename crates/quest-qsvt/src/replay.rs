//! Lazy immutable matching-oracle replay; only one primitive is live at a time.
use crate::{
	Error, MatchingEncoding, NumericalPolicy, OracleFragment, Result,
	matching::{admit, bit},
};
use quest_compile::{Angle, Control, ControlState, Gate, QuantumRegionBuilder};
use std::ops::{Neg, Sub};
/// Replay primitive family. Phase means a conditional scalar phase.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum ReplayKind {
	/// Hadamard on one target.
	H,
	/// X on one target.
	X,
	/// Real flag rotation in radians.
	Ry(f64),
	/// Scalar e^(i angle) on the selected subspace.
	Phase(f64),
}
/// A bounded primitive with signed computational controls.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ReplayGate {
	/// Primitive family and optional angle.
	pub kind: ReplayKind,
	/// Target bit, absent only for scalar phases.
	pub target: Option<usize>,
	/// Bits tested by the control predicate.
	pub control_mask: usize,
	/// Required values on the control mask.
	pub control_value: usize,
}
impl ReplayGate {
	pub(super) fn mapped(
		self,
		targets: &[usize],
		outer_mask: usize,
		outer_value: usize,
	) -> Result<Self> {
		let map = |value: usize| -> Result<usize> {
			let mut mapped = 0_usize;
			for (local, &physical) in targets.iter().enumerate() {
				if value & bit(local)? != 0 {
					mapped |= bit(physical)?;
				}
			}
			Ok(mapped)
		};
		Ok(Self {
			kind: self.kind,
			target: self
				.target
				.map(|t| {
					targets
						.get(t)
						.copied()
						.ok_or(Error::Encoding("matching mapped target"))
				})
				.transpose()?,
			control_mask: map(self.control_mask)? | outer_mask,
			control_value: map(self.control_value)? | outer_value,
		})
	}
}
impl MatchingEncoding {
	/// Replay forward or adjoint primitives using bounded temporary state.
	/// A visitor error stops replay immediately. No expanded instruction list is retained.
	///
	/// # Errors
	/// Propagates visitor errors and checked bit/index overflow.
	pub fn visit_gates(
		&self,
		adjoint: bool,
		mut visitor: impl FnMut(ReplayGate) -> Result<()>,
	) -> Result<()> {
		visit_hadamards(self, adjoint, &mut visitor)?;
		for ordinal in 0..self.num_colors() {
			let color = if adjoint {
				self.num_colors()
					.checked_sub(ordinal)
					.and_then(|n| n.checked_sub(1))
					.ok_or(Error::Budget("matching reverse color"))?
			} else {
				ordinal
			};
			visit_color(self, color, adjoint, &mut visitor)?;
		}
		visit_hadamards(self, adjoint, &mut visitor)
	}

	/// Replay on caller-selected target bits with additional signed outer controls.
	/// Target order maps local bit zero first. Outer controls must be disjoint.
	///
	/// # Errors
	/// Rejects repeated targets, overlapping/invalid controls and visitor errors.
	pub fn visit_mapped_gates(
		&self,
		targets: &[usize],
		outer_mask: usize,
		outer_value: usize,
		adjoint: bool,
		mut visitor: impl FnMut(ReplayGate) -> Result<()>,
	) -> Result<()> {
		if targets.len() != self.num_qubits() || outer_value & !outer_mask != 0 {
			return Err(Error::Encoding("matching replay mapping"));
		}
		let mut occupied = 0_usize;
		for &target in targets {
			let mask = bit(target)?;
			if occupied & mask != 0 || outer_mask & mask != 0 {
				return Err(Error::Encoding("matching replay operand overlap"));
			}
			occupied |= mask;
		}
		self.visit_gates(adjoint, |gate| {
			visitor(gate.mapped(targets, outer_mask, outer_value)?)
		})
	}
	/// Convert lazy replay into the conventional owning oracle under an explicit storage budget.
	/// This small-instance compatibility route admits circuit storage without dense unitary construction.
	///
	/// # Errors
	/// Rejects expanded-circuit storage, bit width and compiler operand/angle admission failures.
	pub fn to_oracle(&self, policy: NumericalPolicy) -> Result<OracleFragment> {
		oracle_from_replay(
			self.num_qubits(),
			self.resources().replay_gates,
			self.resources().retained_bytes,
			policy,
			|visitor| self.visit_gates(false, visitor),
		)
	}
}
pub fn oracle_from_replay(
	num_qubits: usize,
	gate_count: usize,
	retained_bytes: usize,
	policy: NumericalPolicy,
	visit: impl FnOnce(&mut dyn FnMut(ReplayGate) -> Result<()>) -> Result<()>,
) -> Result<OracleFragment> {
	// Includes semantic and bound instruction copies, operands and angle payloads.
	let per_gate = num_qubits
		.checked_mul(128)
		.and_then(|b| b.checked_add(2048))
		.ok_or(Error::Budget("matching circuit size"))?;
	let bytes = gate_count
		.checked_mul(per_gate)
		.and_then(|b| b.checked_add(retained_bytes))
		.ok_or(Error::Budget("matching circuit size"))?;
	admit(bytes, policy)?;
	let mut builder = QuantumRegionBuilder::new(num_qubits, 0)?;
	visit(&mut |primitive| {
		let mut controls = crate::matching::reserve(
			usize::try_from(primitive.control_mask.count_ones())
				.map_err(|_| Error::Budget("matching controls"))?,
		)?;
		for index in 0..num_qubits {
			let mask = bit(index)?;
			if primitive.control_mask & mask != 0 {
				controls.push(Control::new(
					builder.qubit(index)?,
					if primitive.control_value & mask != 0 {
						ControlState::One
					} else {
						ControlState::Zero
					},
				));
			}
		}
		if let ReplayKind::Phase(angle) = primitive.kind {
			builder.global_phase(Angle::radians(angle)?, &controls)?;
		} else {
			let target = builder.qubit(
				primitive
					.target
					.ok_or(Error::Encoding("matching gate target"))?,
			)?;
			let gate = match primitive.kind {
				ReplayKind::H => Gate::H,
				ReplayKind::X => Gate::X,
				ReplayKind::Ry(angle) => Gate::Ry(Angle::radians(angle)?),
				ReplayKind::Phase(_) => return Err(Error::Encoding("matching scalar phase")),
			};
			builder.gate(gate, &[target], &controls)?;
		}
		Ok(())
	})?;
	Ok(OracleFragment::builder(builder.finish()?.bind(&[])?)
		.matrix_tolerance(1e-12)?
		.matrix_policy(policy.matrix_policy())
		.build()?)
}
fn visit_hadamards(
	encoding: &MatchingEncoding,
	adjoint: bool,
	visitor: &mut dyn FnMut(ReplayGate) -> Result<()>,
) -> Result<()> {
	let width = encoding
		.system_qubits()
		.checked_add(1)
		.ok_or(Error::Budget("matching replay width"))?;
	for ordinal in 0..encoding.color_qubits() {
		let color_bit = if adjoint {
			encoding
				.color_qubits()
				.checked_sub(ordinal)
				.and_then(|n| n.checked_sub(1))
				.ok_or(Error::Budget("matching reverse H"))?
		} else {
			ordinal
		};
		visitor(ReplayGate {
			kind: ReplayKind::H,
			target: Some(
				width
					.checked_add(color_bit)
					.ok_or(Error::Budget("matching H target"))?,
			),
			control_mask: 0,
			control_value: 0,
		})?;
	}
	Ok(())
}
fn visit_color(
	encoding: &MatchingEncoding,
	color: usize,
	adjoint: bool,
	visitor: &mut dyn FnMut(ReplayGate) -> Result<()>,
) -> Result<()> {
	let width = encoding
		.system_qubits()
		.checked_add(1)
		.ok_or(Error::Budget("matching replay width"))?;
	let color_mask = bit(encoding.num_qubits())?
		.checked_sub(1)
		.ok_or(Error::Budget("matching replay mask"))?
		& !bit(width)?
			.checked_sub(1)
			.ok_or(Error::Budget("matching replay mask"))?;
	let color_value = color
		.checked_shl(u32::try_from(width).map_err(|_| Error::Budget("matching color value"))?)
		.ok_or(Error::Budget("matching color value"))?;
	let default = ReplayGate {
		kind: ReplayKind::Ry(if adjoint {
			std::f64::consts::PI.neg()
		} else {
			std::f64::consts::PI
		}),
		target: Some(0),
		control_mask: color_mask,
		control_value: color_value,
	};
	if adjoint {
		visit_permutations(encoding, color, color_mask, color_value, true, visitor)?;
		visit_corrections(encoding, color, color_mask, color_value, true, visitor)?;
		visitor(default)?;
	} else {
		visitor(default)?;
		visit_corrections(encoding, color, color_mask, color_value, false, visitor)?;
		visit_permutations(encoding, color, color_mask, color_value, false, visitor)?;
	}
	Ok(())
}
fn visit_permutations(
	encoding: &MatchingEncoding,
	color: usize,
	color_mask: usize,
	color_value: usize,
	adjoint: bool,
	visitor: &mut dyn FnMut(ReplayGate) -> Result<()>,
) -> Result<()> {
	if let Some(matching) = encoding.matchings().get(color) {
		for ordinal in 0..matching.cycles().len() {
			let position = if adjoint {
				matching
					.cycles()
					.len()
					.checked_sub(ordinal)
					.and_then(|n| n.checked_sub(1))
					.ok_or(Error::Budget("matching reverse cycle"))?
			} else {
				ordinal
			};
			let cycle = matching
				.cycles()
				.get(position)
				.ok_or(Error::Encoding("matching cycle replay"))?;
			let pivot = *cycle
				.first()
				.ok_or(Error::Encoding("empty matching cycle"))?;
			for edge_ordinal in 1..cycle.len() {
				let offset = if adjoint {
					cycle
						.len()
						.checked_sub(edge_ordinal)
						.ok_or(Error::Budget("matching reverse transposition"))?
				} else {
					edge_ordinal
				};
				transposition(
					pivot,
					*cycle
						.get(offset)
						.ok_or(Error::Encoding("matching cycle replay"))?,
					encoding.system_qubits(),
					color_mask,
					color_value,
					visitor,
				)?;
			}
		}
	}
	Ok(())
}
fn visit_corrections(
	encoding: &MatchingEncoding,
	color: usize,
	color_mask: usize,
	color_value: usize,
	adjoint: bool,
	visitor: &mut dyn FnMut(ReplayGate) -> Result<()>,
) -> Result<()> {
	if let Some(matching) = encoding.matchings().get(color) {
		let system_mask = bit(encoding.system_qubits())?
			.checked_sub(1)
			.and_then(|n| n.checked_mul(2))
			.ok_or(Error::Budget("matching system predicate"))?;
		for ordinal in 0..matching.edges().len() {
			let position = if adjoint {
				matching
					.edges()
					.len()
					.checked_sub(ordinal)
					.and_then(|n| n.checked_sub(1))
					.ok_or(Error::Budget("matching reverse edge"))?
			} else {
				ordinal
			};
			let edge = matching
				.edges()
				.get(position)
				.ok_or(Error::Encoding("matching edge replay"))?;
			let mask = color_mask | system_mask;
			let value = color_value
				| edge
					.col
					.checked_mul(2)
					.ok_or(Error::Budget("matching edge predicate"))?;
			let phase = ReplayGate {
				kind: ReplayKind::Phase(if adjoint {
					edge.phase.neg()
				} else {
					edge.phase
				}),
				target: None,
				control_mask: mask | 1,
				control_value: value,
			};
			let correction = ReplayGate {
				kind: ReplayKind::Ry(if adjoint {
					std::f64::consts::PI.sub(edge.theta)
				} else {
					edge.theta.sub(std::f64::consts::PI)
				}),
				target: Some(0),
				control_mask: mask,
				control_value: value,
			};
			if adjoint {
				visitor(phase)?;
				visitor(correction)?;
			} else {
				visitor(correction)?;
				visitor(phase)?;
			}
		}
	}
	Ok(())
}
fn transposition(
	a: usize,
	b: usize,
	system_qubits: usize,
	color_mask: usize,
	color_value: usize,
	visitor: &mut dyn FnMut(ReplayGate) -> Result<()>,
) -> Result<()> {
	let difference = a ^ b;
	if difference == 0 {
		return Ok(());
	}
	let system_mask = bit(system_qubits)?
		.checked_sub(1)
		.and_then(|n| n.checked_mul(2))
		.ok_or(Error::Budget("matching swap mask"))?;
	let mut current = a;
	let mut last_bit = None;
	for local in 0..system_qubits {
		let changed = bit(local)?;
		if difference & changed == 0 {
			continue;
		}
		let target = local
			.checked_add(1)
			.ok_or(Error::Budget("matching swap target"))?;
		let target_mask = bit(target)?;
		visitor(ReplayGate {
			kind: ReplayKind::X,
			target: Some(target),
			control_mask: color_mask | (system_mask & !target_mask),
			control_value: color_value
				| (current
					.checked_mul(2)
					.ok_or(Error::Budget("matching swap value"))?
					& !target_mask),
		})?;
		current ^= changed;
		last_bit = Some(local);
	}
	let last = last_bit.ok_or(Error::Encoding("matching transposition difference"))?;
	current ^= bit(last)?;
	for local in (0..last).rev() {
		let changed = bit(local)?;
		if difference & changed == 0 {
			continue;
		}
		let target = local
			.checked_add(1)
			.ok_or(Error::Budget("matching swap target"))?;
		let target_mask = bit(target)?;
		visitor(ReplayGate {
			kind: ReplayKind::X,
			target: Some(target),
			control_mask: color_mask | (system_mask & !target_mask),
			control_value: color_value
				| (current
					.checked_mul(2)
					.ok_or(Error::Budget("matching swap value"))?
					& !target_mask),
		})?;
		current ^= changed;
	}
	Ok(())
}
