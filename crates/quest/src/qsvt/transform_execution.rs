//! Shared descriptor-aware local projector execution; never owns source records.
use crate::{Error, QubitCount, Register, StateVector, error::BackendResult};
use quest_qsvt::{
	CompactProjector, EncodingDescriptor, ReplayGate, ReplayKind,
	replay_transform::{TransformSchedule, TransformStep},
};
const CHUNK: usize = 256;
pub(super) const SCRATCH_BYTES: usize = CHUNK * size_of::<quest_sys::QuestComplex>() + 16384;
fn add(a: usize, b: usize) -> crate::Result<usize> {
	a.checked_add(b).ok_or(Error::Overflow)
}
fn mul(a: usize, b: usize) -> crate::Result<usize> {
	a.checked_mul(b).ok_or(Error::Overflow)
}
fn bit(i: usize) -> crate::Result<usize> {
	super::matching::bit(i)
}
struct Projector {
	mask: usize,
	value: usize,
	range: std::ops::Range<usize>,
	packed: Vec<(usize, usize)>,
}
impl Projector {
	fn new(
		p: &CompactProjector,
		targets: &[usize],
		admit: &mut impl FnMut(usize) -> crate::Result<()>,
	) -> crate::Result<Self> {
		let mut mask = 0;
		let mut value = 0;
		let mut packed = crate::values::reserve_vec(targets.len())?;
		admit(mul(packed.capacity(), size_of::<(usize, usize)>())?)?;
		for (logical, &physical) in targets.iter().enumerate() {
			let l = bit(logical)?;
			let b = bit(physical)?;
			if p.fixed_mask & l != 0 {
				mask |= b;
				if p.fixed_value & l != 0 {
					value |= b;
				}
			} else {
				packed.push((b, bit(packed.len())?));
			}
		}
		Ok(Self {
			mask,
			value,
			range: p.logical_range.clone(),
			packed,
		})
	}
	fn contains(&self, basis: usize) -> bool {
		basis & self.mask == self.value
			&& self
				.range
				.contains(&self.packed.iter().fold(0, |index, &(physical, logical)| {
					if basis & physical == 0 {
						index
					} else {
						index | logical
					}
				}))
	}
	fn bytes(&self) -> crate::Result<usize> {
		mul(self.packed.capacity(), size_of::<(usize, usize)>())
	}
}
pub(super) struct TransformLayout {
	pub count: QubitCount,
	pub response: usize,
	pub response_mask: usize,
	active: usize,
	left: Projector,
	right: Projector,
}
impl TransformLayout {
	pub fn planned_bytes(schedule: &TransformSchedule, owner: usize) -> super::Result<usize> {
		Ok(add(
			add(
				schedule.retained_bytes()?,
				mul(
					schedule.descriptor().layout.num_qubits,
					size_of::<[(usize, usize); 2]>(),
				)?,
			)?,
			add(add(size_of::<Self>(), owner)?, SCRATCH_BYTES)?,
		)?)
	}
	pub fn new(
		descriptor: &EncodingDescriptor,
		count: QubitCount,
		targets: &[usize],
		response: usize,
		mut admit: impl FnMut(usize) -> crate::Result<()>,
	) -> super::Result<Self> {
		descriptor.validate()?;
		if targets.len() != descriptor.layout.num_qubits || response >= count.get() {
			return Err(Error::Value("transform response/source width").into());
		}
		let mut active = bit(response)?;
		for &target in targets {
			if target >= count.get() || active & bit(target)? != 0 {
				return Err(Error::Value("transform response/target overlap").into());
			}
			active |= bit(target)?;
		}
		Ok(Self {
			count,
			response,
			response_mask: bit(response)?,
			active,
			left: Projector::new(&descriptor.left, targets, &mut admit)?,
			right: Projector::new(&descriptor.right, targets, &mut admit)?,
		})
	}
	pub fn bytes(&self, schedule: &TransformSchedule, owner: usize) -> super::Result<usize> {
		Ok(add(
			add(
				schedule.retained_bytes()?,
				add(self.left.bytes()?, self.right.bytes()?)?,
			)?,
			add(add(size_of::<Self>(), owner)?, SCRATCH_BYTES)?,
		)?)
	}
	pub const fn controls(&self, mask: usize, value: usize) -> crate::Result<()> {
		if value & !mask != 0 || mask >= self.count.dimension() || mask & self.active != 0 {
			return Err(Error::Value("transform outer controls"));
		}
		Ok(())
	}
	pub const fn query_controls(
		&self,
		response: bool,
		mask: usize,
		value: usize,
	) -> (usize, usize) {
		(
			mask | self.response_mask,
			value | if response { self.response_mask } else { 0 },
		)
	}
	pub fn response_gates(
		&self,
		step: TransformStep,
		mask: usize,
		value: usize,
		mut visit: impl FnMut(ReplayGate) -> super::Result<()>,
	) -> super::Result<()> {
		match step {
			TransformStep::Hadamard => visit(ReplayGate {
				kind: ReplayKind::H,
				target: Some(self.response),
				control_mask: mask,
				control_value: value,
			}),
			TransformStep::ResponseRotation(angle) => {
				visit(ReplayGate {
					kind: ReplayKind::Phase(-0.5 * angle),
					target: None,
					control_mask: mask,
					control_value: value,
				})?;
				visit(ReplayGate {
					kind: ReplayKind::Phase(angle),
					target: None,
					control_mask: mask | self.response_mask,
					control_value: value | self.response_mask,
				})
			}
			_ => Err(Error::Value("transform response step").into()),
		}
	}
	#[allow(
		clippy::arithmetic_side_effects,
		reason = "Finite unit-modulus phase factors update checked bounded native chunks"
	)]
	pub fn phase(
		&self,
		register: &mut Register<'_, StateVector>,
		left: bool,
		angle: f64,
		response: bool,
		mask: usize,
		value: usize,
	) -> crate::Result<()> {
		let (mask, value) = self.query_controls(response, mask, value);
		let projector = if left { &self.left } else { &self.right };
		let positive = crate::Complex64::from_polar(1., angle);
		let negative = positive.conj();
		let local = register.deployment().local_amplitudes();
		let base = mul(register.deployment().rank(), local)?;
		let mut buffer = [quest_sys::QuestComplex { re: 0., im: 0. }; CHUNK];
		for start in (0..local).step_by(CHUNK) {
			let len = local.checked_sub(start).ok_or(Error::Overflow)?.min(CHUNK);
			let chunk = buffer.get_mut(..len).ok_or(Error::Overflow)?;
			quest_sys::read_local_qureg_amps(
				&register.native,
				i64::try_from(start).map_err(|_| Error::Overflow)?,
				chunk,
			)
			.context("reading transform projector")?;
			for (offset, z) in chunk.iter_mut().enumerate() {
				let basis = add(add(base, start)?, offset)?;
				if basis & mask != value {
					continue;
				}
				let f = if projector.contains(basis) {
					positive
				} else {
					negative
				};
				let out = crate::Complex64::new(z.re, z.im) * f;
				*z = quest_sys::QuestComplex {
					re: out.re,
					im: out.im,
				};
			}
			quest_sys::write_local_qureg_amps(
				register.pin(),
				i64::try_from(start).map_err(|_| Error::Overflow)?,
				chunk,
			)
			.context("writing transform projector")?;
		}
		Ok(())
	}
	/// Preserve legacy matching response dispatch while sharing the complete projector predicate.
	pub fn legacy_step(
		&self,
		register: &mut Register<'_, StateVector>,
		step: TransformStep,
	) -> crate::Result<()> {
		match step {
			TransformStep::Hadamard => register.h(self.response),
			TransformStep::ResponseRotation(angle) => quest_sys::apply_rotate_z(
				register.pin(),
				i32::try_from(self.response).map_err(|_| Error::Overflow)?,
				angle,
			)
			.context("applying QSVT response rotation"),
			TransformStep::Projector {
				left,
				angle,
				response,
			} => self.phase(register, left, angle, response, 0, 0),
			TransformStep::Oracle { .. } => Err(Error::Value("oracle requires matching resource")),
		}
	}
	pub fn phase_cost(&self, local: usize) -> crate::Result<(usize, usize)> {
		Ok((
			mul(local, mul(64, add(self.count.get(), 1)?)?)?,
			mul(2, local.div_ceil(CHUNK))?,
		))
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	#[test]
	#[allow(
		clippy::panic_in_result_fn,
		reason = "Independent fixed basis enumeration asserts compact projector semantics and returned capacities"
	)]
	fn compact_projectors_preserve_offset_fixed_value_and_logical_bit_order()
	-> super::super::Result<()> {
		let d = EncodingDescriptor {
			rows: 2,
			cols: 2,
			normalization: 1.,
			layout: quest_qsvt::EncodingLayout {
				num_qubits: 3,
				system_mask: 3,
				workspace_mask: 4,
				clean_workspace_mask: 4,
				clean_workspace_value: 4,
			},
			left: CompactProjector {
				fixed_mask: 4,
				fixed_value: 4,
				logical_range: 1..3,
			},
			right: CompactProjector {
				fixed_mask: 5,
				fixed_value: 5,
				logical_range: 0..2,
			},
			errors: quest_qsvt::EncodingErrors {
				preparation: None,
				encoding: None,
				binary64_parameters: true,
			},
			source_identity: 1,
			construction_identity: 2,
		};
		let mut vector_bytes = 0;
		let layout = TransformLayout::new(&d, QubitCount::new(6)?, &[3, 0, 2], 4, |n| {
			vector_bytes = add(vector_bytes, n)?;
			Ok(())
		})?;
		assert_eq!(
			vector_bytes,
			add(layout.left.bytes()?, layout.right.bytes()?)?
		);
		for basis in 0..64 {
			let logical = ((basis >> 3) & 1) | ((basis & 1) << 1) | (((basis >> 2) & 1) << 2);
			assert_eq!(
				layout.left.contains(basis),
				logical & 4 == 4 && (1..3).contains(&(logical & 3))
			);
			assert_eq!(layout.right.contains(basis), logical & 5 == 5);
		}
		let schedule = TransformSchedule::from_parts(
			d,
			vec![0.1],
			0.,
			quest_qsvt::NumericalPolicy::default(),
		)?;
		assert!(layout.bytes(&schedule, 0)? >= add(schedule.retained_bytes()?, vector_bytes)?);
		assert!(layout.controls(1 << 5, 0).is_ok());
		assert!(layout.controls(1 << 3, 0).is_err());
		Ok(())
	}
}
