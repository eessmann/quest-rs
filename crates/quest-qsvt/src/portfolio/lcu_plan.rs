//! Shared bounded selector semantics for portable and native weighted oracles.
use super::{add, bits, charge_work, hash, mul, word};
use crate::{
	Complex64, EncodingDescriptor, EncodingErrors, Error, ReplayGate, ReplayKind, Result,
	state_preparation::{AmplitudePreparation, PreparationLimits},
};
use std::{
	ops::{Add, Mul, Sub},
	sync::Arc,
};

pub(super) fn metadata_work_floor(input_terms: usize) -> Result<usize> {
	mul(input_terms, add(128, crate::record_fingerprint_work(3)?)?)
}

/// Independent finite replicated-metadata and selector compilation limits.
#[derive(Clone, Copy, Debug)]
pub struct LcuPlanLimits {
	pub max_terms: usize,
	pub max_bytes: usize,
	/// This plan’s descriptor/fingerprint and preparation work allowance.
	/// Previously constructed child sources and child replay remain separate.
	pub max_compile_work: usize,
	/// PREP, scalar weight phases and UNPREP only; excludes child primitives.
	pub max_primitives: usize,
	pub preparation: PreparationLimits,
}
impl Default for LcuPlanLimits {
	fn default() -> Self {
		Self {
			max_terms: 1_048_576,
			max_bytes: 67_108_864,
			max_compile_work: 67_108_864,
			max_primitives: 8_388_608,
			preparation: PreparationLimits::default(),
		}
	}
}
/// Selector resources only. Child source ownership, replay and transport are separate.
#[derive(Clone, Copy, Debug)]
pub struct LcuPlanResources {
	pub input_terms: usize,
	pub selected_terms: usize,
	pub selector_qubits: usize,
	pub primitive_gates: usize,
	/// Combined PREP and UNPREP primitive count in one replay.
	pub preparation_gates: usize,
	pub metadata_compile_work: usize,
	pub preparation_compile_work: usize,
	pub compile_work: usize,
	/// Actual owned Vec capacities plus explicit scalar/Arc metadata allowances.
	pub retained_bytes: usize,
	/// Input capacity, owned plan and simultaneous preparation buffers plus a
	/// 4096-byte allowance, including the fixed record-digest scratch.
	pub construction_peak_bytes: usize,
	/// Binary64 implemented-minus-nominal normalization diagnostic; not a certified bound.
	pub normalization_roundoff: f64,
}
/// Lazy application-order event. Child indices address dense surviving owners.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum LcuStep {
	Gate(ReplayGate),
	Child {
		index: usize,
		adjoint: bool,
		control_mask: usize,
		control_value: usize,
	},
}
#[derive(Debug)]
struct Data {
	indices: Vec<usize>,
	phases: Vec<f64>,
	prepare: AmplitudePreparation,
	descriptor: EncodingDescriptor,
	child_width: usize,
	resources: LcuPlanResources,
}
/// Immutable finite selector plan; never owns child coefficients or child circuits.
///
/// Every input descriptor is validated and contributes provenance, including exact-zero
/// weights. Zero weights are omitted from SELECT; nonzero underflow branches remain.
#[derive(Clone, Debug)]
pub struct LcuPlan(Arc<Data>);
impl LcuPlan {
	/// # Errors
	/// Rejects malformed/incompatible descriptors, zero sums, nonfinite values and budgets.
	#[allow(
		clippy::too_many_lines,
		reason = "One staged transaction validates all metadata and admits simultaneous selector owners"
	)]
	pub fn new(terms: Vec<(Complex64, EncodingDescriptor)>, limits: LcuPlanLimits) -> Result<Self> {
		let common = terms
			.first()
			.ok_or(Error::Encoding("empty weighted LCU"))?
			.1
			.clone();
		let input_bytes = mul(
			terms.capacity(),
			size_of::<(Complex64, EncodingDescriptor)>(),
		)?;
		if input_bytes > limits.max_bytes
			|| terms.len() > limits.max_terms
			|| metadata_work_floor(terms.len())? > limits.max_compile_work
			|| add(input_bytes, add(add(size_of::<Data>(), 64)?, 4096)?)? > limits.max_bytes
		{
			return Err(Error::Budget("LCU input/work"));
		}
		let mut work = 0;
		let mut nominal = 0.0;
		let mut selected = 0;
		let mut source = 0_u64;
		let mut construction = hash([0x4c43_5550_5245_5031]);
		for (weight, d) in &terms {
			// Explicit scalar slots for validation, byte-wise identities and rounded weights.
			charge_work(
				&mut work,
				add(
					add(128, crate::record_fingerprint_work(3)?)?,
					mul(4, d.layout.num_qubits)?,
				)?,
				limits.max_compile_work,
			)?;
			d.validate()?;
			if d.rows != common.rows
				|| d.cols != common.cols
				|| d.layout != common.layout
				|| d.left != common.left
				|| d.right != common.right
			{
				return Err(Error::Encoding("LCU child projector/layout mismatch"));
			}
			if !weight.re.is_finite() || !weight.im.is_finite() {
				return Err(Error::NonFinite);
			}
			let mass = weight.norm().mul(d.normalization);
			if !mass.is_finite() {
				return Err(Error::NonFinite);
			}
			nominal = nominal.add(mass);
			source = source.wrapping_add(crate::record_fingerprint(
				0x4c43_5552_4543_5632,
				[d.source_identity, weight.re.to_bits(), weight.im.to_bits()],
			));
			construction = hash([
				construction,
				d.construction_identity,
				d.normalization.to_bits(),
				weight.re.to_bits(),
				weight.im.to_bits(),
			]);
			if weight.re != 0.0 || weight.im != 0.0 {
				selected = add(selected, 1)?;
			}
		}
		if !nominal.is_finite() || nominal <= 0.0 {
			return Err(Error::Encoding("zero/unrepresentable LCU normalization"));
		}
		let base = add(size_of::<Data>(), 64)?;
		let table_bytes = mul(selected, add(size_of::<usize>(), size_of::<f64>())?)?;
		let buffers = add(
			add(input_bytes, base)?,
			add(table_bytes, mul(selected, size_of::<Complex64>())?)?,
		)?;
		if add(buffers, 4096)? > limits.max_bytes {
			return Err(Error::Budget("LCU selector buffers"));
		}
		let mut indices = crate::matching::reserve(selected)?;
		let mut phases = crate::matching::reserve(selected)?;
		let mut amplitudes = crate::matching::reserve(selected)?;
		let frozen_bytes = add(
			base,
			add(
				mul(indices.capacity(), size_of::<usize>())?,
				mul(phases.capacity(), size_of::<f64>())?,
			)?,
		)?;
		let live_base = add(add(input_bytes, frozen_bytes)?, 4096)?;
		if add(
			live_base,
			mul(amplitudes.capacity(), size_of::<Complex64>())?,
		)? > limits.max_bytes
		{
			return Err(Error::Budget("LCU selector actual capacities"));
		}
		for (index, (weight, d)) in terms.iter().enumerate() {
			if weight.re != 0.0 || weight.im != 0.0 {
				indices.push(index);
				phases.push(weight.arg());
				amplitudes.push(Complex64::new(
					weight.norm().mul(d.normalization).sqrt(),
					0.0,
				));
			}
		}
		let available = limits
			.max_bytes
			.checked_sub(live_base)
			.ok_or(Error::Budget("LCU selector capacities"))?;
		let mut pl = limits.preparation;
		pl.max_bytes = pl.max_bytes.min(available);
		pl.max_compile_work = pl.max_compile_work.min(
			limits
				.max_compile_work
				.checked_sub(work)
				.ok_or(Error::Budget("LCU compile work"))?,
		);
		pl.max_gates = pl.max_gates.min(limits.max_primitives);
		// The preparation constructor counts its borrowed input length. Admit excess
		// amplitude capacity in addition so no retained capacity disappears at this boundary.
		let excess = mul(
			amplitudes.capacity().saturating_sub(amplitudes.len()),
			size_of::<Complex64>(),
		)?;
		pl.max_bytes = pl
			.max_bytes
			.checked_sub(excess)
			.ok_or(Error::Budget("LCU amplitude capacity"))?;
		let prepare = AmplitudePreparation::new(&amplitudes, pl)?;
		let pr = prepare.resources();
		let normalization = prepare.norm().mul(prepare.norm());
		if !normalization.is_finite() || normalization <= 0.0 {
			return Err(Error::NonFinite);
		}
		let child_width = common.layout.num_qubits;
		let width = add(child_width, prepare.qubits())?;
		let full = bits(width)?
			.checked_sub(1)
			.ok_or(Error::Budget("LCU width"))?;
		let labels = full
			& !bits(child_width)?
				.checked_sub(1)
				.ok_or(Error::Budget("LCU child width"))?;
		let mut descriptor = common;
		descriptor.normalization = normalization;
		descriptor.layout.num_qubits = width;
		descriptor.layout.workspace_mask |= labels;
		descriptor.layout.clean_workspace_mask |= labels;
		descriptor.left.fixed_mask |= labels;
		descriptor.right.fixed_mask |= labels;
		descriptor.errors = EncodingErrors {
			preparation: None,
			encoding: None,
			binary64_parameters: true,
		};
		descriptor.source_identity = hash([0x4c43_5553_4f55_5232, word(terms.len())?, source]);
		descriptor.construction_identity = hash([
			construction,
			normalization.to_bits(),
			word(prepare.qubits())?,
		]);
		descriptor.validate()?;
		let resources = LcuPlanResources {
			input_terms: terms.len(),
			selected_terms: selected,
			selector_qubits: prepare.qubits(),
			primitive_gates: add(mul(pr.elementary_gates, 2)?, selected)?,
			preparation_gates: mul(pr.elementary_gates, 2)?,
			metadata_compile_work: work,
			preparation_compile_work: pr.compile_work,
			compile_work: add(work, pr.compile_work)?,
			retained_bytes: add(frozen_bytes, prepare.retained_bytes()?)?,
			construction_peak_bytes: add(add(live_base, excess)?, pr.construction_peak_bytes)?,
			normalization_roundoff: normalization.sub(nominal).abs(),
		};
		if resources.primitive_gates > limits.max_primitives
			|| resources.compile_work > limits.max_compile_work
			|| resources.construction_peak_bytes > limits.max_bytes
		{
			return Err(Error::Budget("LCU selector resources"));
		}
		drop(terms);
		Ok(Self(Arc::new(Data {
			indices,
			phases,
			prepare,
			descriptor,
			child_width,
			resources,
		})))
	}
	#[must_use]
	pub fn descriptor(&self) -> &EncodingDescriptor {
		&self.0.descriptor
	}
	#[must_use]
	pub fn child_width(&self) -> usize {
		self.0.child_width
	}
	#[must_use]
	pub fn selector_qubits(&self) -> usize {
		self.0.prepare.qubits()
	}
	#[must_use]
	pub fn surviving_indices(&self) -> &[usize] {
		&self.0.indices
	}
	#[must_use]
	pub fn preparation(&self) -> &AmplitudePreparation {
		&self.0.prepare
	}
	#[must_use]
	pub fn resources(&self) -> LcuPlanResources {
		self.0.resources
	}
	/// # Errors
	/// Rejects checked byte-accounting overflow; excludes children and caller input owners.
	pub fn retained_bytes(&self) -> Result<usize> {
		add(
			add(
				add(size_of::<Data>(), 64)?,
				mul(self.0.indices.capacity(), size_of::<usize>())?,
			)?,
			add(
				mul(self.0.phases.capacity(), size_of::<f64>())?,
				self.0.prepare.retained_bytes()?,
			)?,
		)
	}
	/// Stream primitives and selected-child calls in whole-unitary application order.
	///
	/// Child indices address `surviving_indices`, not original inputs; their target
	/// mapping is the supplied prefix `targets[..child_width]`. All events carry
	/// the same signed outer controls. No child source is visited by this method.
	/// # Errors
	/// Rejects mapping before emission and preserves the visitor's own error type.
	pub fn visit_mapped_steps<VisitorError: From<Error>>(
		&self,
		targets: &[usize],
		mask: usize,
		value: usize,
		adjoint: bool,
		mut visitor: impl FnMut(LcuStep) -> std::result::Result<(), VisitorError>,
	) -> std::result::Result<(), VisitorError> {
		crate::owned_replay::validate_mapping(
			self.0.descriptor.layout.num_qubits,
			targets,
			mask,
			value,
		)?;
		let labels = targets
			.get(self.0.child_width..)
			.ok_or(Error::Encoding("LCU selector mapping"))?;
		self.visit_preparation(labels, mask, value, false, &mut visitor)?;
		let logical_mask = bits(self.0.descriptor.layout.num_qubits)?
			.checked_sub(1)
			.ok_or(Error::Budget("LCU width"))?
			& !bits(self.0.child_width)?
				.checked_sub(1)
				.ok_or(Error::Budget("LCU width"))?;
		for ordinal in 0..self.0.indices.len() {
			let index = if adjoint {
				self.0
					.indices
					.len()
					.checked_sub(add(ordinal, 1)?)
					.ok_or(Error::Budget("LCU term ordinal"))?
			} else {
				ordinal
			};
			let phase = *self
				.0
				.phases
				.get(index)
				.ok_or(Error::Encoding("LCU term phase"))?;
			let logical_value = index
				.checked_shl(
					u32::try_from(self.0.child_width)
						.map_err(|_| Error::Budget("LCU child width"))?,
				)
				.ok_or(Error::Budget("LCU selector value"))?;
			let gate = ReplayGate {
				kind: ReplayKind::Phase(if adjoint { -phase } else { phase }),
				target: None,
				control_mask: logical_mask,
				control_value: logical_value,
			}
			.mapped(targets, mask, value)?;
			if !adjoint {
				visitor(LcuStep::Gate(gate))?;
			}
			visitor(LcuStep::Child {
				index,
				adjoint,
				control_mask: gate.control_mask,
				control_value: gate.control_value,
			})?;
			if adjoint {
				visitor(LcuStep::Gate(gate))?;
			}
		}
		self.visit_preparation(labels, mask, value, true, &mut visitor)
	}
	fn visit_preparation<VisitorError: From<Error>>(
		&self,
		targets: &[usize],
		mask: usize,
		value: usize,
		adjoint: bool,
		visitor: &mut impl FnMut(LcuStep) -> std::result::Result<(), VisitorError>,
	) -> std::result::Result<(), VisitorError> {
		let mut failure = None;
		let result =
			self.0
				.prepare
				.visit_mapped_gates(targets, mask, value, adjoint, &mut |gate| {
					visitor(LcuStep::Gate(gate)).map_err(|error| {
						failure = Some(error);
						Error::Encoding("LCU preparation visitor failed")
					})
				});
		failure.map_or_else(|| result.map_err(Into::into), Err)
	}
}
