//! Weighted whole-unitary SELECT and tensor calculus; no dense PREP isometry.
#![allow(
	clippy::arithmetic_side_effects,
	clippy::indexing_slicing,
	reason = "Widths, simultaneous buffers and complete streamed counts are admitted before arithmetic/indexing"
)]
use super::{
	LcuPlan, LcuPlanLimits, LcuStep, PortfolioLimits, PortfolioResources, add, admit, bits,
	count_with_work, full_projectors, hash, mul, targets, word,
};
use crate::{
	Complex64, EncodingDescriptor, EncodingErrors, Error, ReplayEncoding, ReplayGate, ReplayKind,
	Result,
};
use std::sync::Arc;
#[derive(Debug)]
struct LcuData<E> {
	terms: Vec<E>,
	plan: LcuPlan,
	resources: PortfolioResources,
}
/// PREP-SELECT-UNPREP of equally laid-out projected encodings.
///
/// The nominal normalization is `sum |w_t| alpha_t`. Padded labels select identity;
/// their preparation amplitudes are zero. Arbitrary failure sectors remain unitary.
#[derive(Clone, Debug)]
pub struct WeightedLcu<E: ReplayEncoding>(Arc<LcuData<E>>);
impl<E: ReplayEncoding> WeightedLcu<E> {
	/// # Errors
	/// Rejects empty/zero sums, nonfinite weights, differing layouts/projectors and budgets.
	#[allow(
		clippy::too_many_lines,
		reason = "One staged ownership transition shares all metadata, child replay and selector budgets"
	)]
	pub fn new(terms: Vec<(Complex64, E)>, limits: PortfolioLimits) -> Result<Self> {
		if terms.is_empty() {
			return Err(Error::Encoding("empty weighted LCU"));
		}
		let input_bytes = mul(terms.capacity(), size_of::<(Complex64, E)>())?;
		if input_bytes > limits.max_bytes
			|| terms.len() > limits.max_table_entries
			|| super::lcu_plan::metadata_work_floor(terms.len())? > limits.max_compile_work
		{
			return Err(Error::Budget("LCU input/work"));
		}
		let mut children_bytes = 0;
		for (_, child) in &terms {
			children_bytes = add(children_bytes, child.retained_bytes()?)?;
			if add(input_bytes, children_bytes)? > limits.max_bytes {
				return Err(Error::Budget("LCU child storage"));
			}
		}
		let owner_bytes = add(size_of::<LcuData<E>>(), 64)?;
		let frozen_allowance = mul(terms.len(), size_of::<E>())?;
		// This conservative overlap retains input storage, all children, future
		// frozen owner capacity and the complete selector-construction envelope.
		let external_peak = add(
			add(input_bytes, children_bytes)?,
			add(owner_bytes, frozen_allowance)?,
		)?;
		let available = limits
			.max_bytes
			.checked_sub(external_peak)
			.ok_or(Error::Budget("LCU construction owners"))?;
		if mul(terms.len(), size_of::<(Complex64, EncodingDescriptor)>())? > available {
			return Err(Error::Budget("LCU descriptor input"));
		}
		let mut metadata = crate::matching::reserve(terms.len())?;
		if mul(
			metadata.capacity(),
			size_of::<(Complex64, EncodingDescriptor)>(),
		)? > available
		{
			return Err(Error::Budget("LCU descriptor capacity"));
		}
		for (weight, child) in &terms {
			metadata.push((*weight, child.descriptor()?));
		}
		let plan = LcuPlan::new(
			metadata,
			LcuPlanLimits {
				max_terms: limits.max_table_entries,
				max_bytes: available,
				max_compile_work: limits.max_compile_work,
				max_primitives: limits.max_gates,
				preparation: limits.preparation,
			},
		)?;
		let pr = plan.resources();
		let peak = add(external_peak, pr.construction_peak_bytes)?;
		let mut frozen = crate::matching::reserve(pr.selected_terms)?;
		let frozen_bytes = mul(frozen.capacity(), size_of::<E>())?;
		let peak = add(peak, frozen_bytes.saturating_sub(frozen_allowance))?;
		if peak > limits.max_bytes {
			return Err(Error::Budget("LCU frozen source capacity"));
		}
		let mut compile_work = pr.compile_work;
		let mut elementary_gates = pr.primitive_gates;
		let mut retained_children = 0;
		for &index in plan.surviving_indices() {
			let child = &terms
				.get(index)
				.ok_or(Error::Encoding("LCU surviving source"))?
				.1;
			let mut child_limits = limits;
			child_limits.max_gates = limits
				.max_gates
				.checked_sub(elementary_gates)
				.ok_or(Error::Budget("LCU aggregate gates"))?;
			elementary_gates = add(
				elementary_gates,
				count_with_work(child, child_limits, &mut compile_work)?,
			)?;
			retained_children = add(retained_children, child.retained_bytes()?)?;
		}
		let mut surviving = plan.surviving_indices().iter().copied().peekable();
		for (index, (_, child)) in terms.into_iter().enumerate() {
			if surviving.peek() == Some(&index) {
				surviving.next();
				frozen.push(child);
			}
		}
		let resources = PortfolioResources {
			elementary_gates,
			oracle_queries: pr.selected_terms,
			table_entries: pr.selected_terms,
			workspace_qubits: usize::try_from(plan.descriptor().layout.workspace_mask.count_ones())
				.map_err(|_| Error::Budget("LCU workspace"))?,
			preparation_gates: pr.preparation_gates,
			preparation_compile_work: pr.preparation_compile_work,
			compile_work,
			retained_bytes: add(
				add(owner_bytes, frozen_bytes)?,
				add(retained_children, plan.retained_bytes()?)?,
			)?,
			construction_peak_bytes: peak,
			normalization_roundoff: pr.normalization_roundoff,
			..PortfolioResources::default()
		};
		admit(resources, limits)?;
		Ok(Self(Arc::new(LcuData {
			terms: frozen,
			plan,
			resources,
		})))
	}
	#[must_use]
	pub fn resources(&self) -> PortfolioResources {
		self.0.resources
	}
}
impl<E: ReplayEncoding> ReplayEncoding for WeightedLcu<E> {
	fn descriptor(&self) -> Result<EncodingDescriptor> {
		Ok(self.0.plan.descriptor().clone())
	}
	fn retained_bytes(&self) -> Result<usize> {
		Ok(self.0.resources.retained_bytes)
	}
	fn visit_replay(
		&self,
		adjoint: bool,
		v: &mut dyn FnMut(ReplayGate) -> Result<()>,
	) -> Result<()> {
		let width = self.0.plan.descriptor().layout.num_qubits;
		let mapping = targets(0, width)?;
		let mapping = mapping
			.get(..width)
			.ok_or(Error::Encoding("LCU whole mapping"))?;
		let child_targets = mapping
			.get(..self.0.plan.child_width())
			.ok_or(Error::Encoding("LCU child mapping"))?;
		self.0
			.plan
			.visit_mapped_steps(mapping, 0, 0, adjoint, |step| match step {
				LcuStep::Gate(gate) => v(gate),
				LcuStep::Child {
					index,
					adjoint,
					control_mask,
					control_value,
				} => self
					.0
					.terms
					.get(index)
					.ok_or(Error::Encoding("LCU selected source"))?
					.visit_mapped_replay(child_targets, control_mask, control_value, adjoint, v),
			})
	}
}
fn product_descriptor(
	a: &EncodingDescriptor,
	b: &EncodingDescriptor,
) -> Result<EncodingDescriptor> {
	a.validate()?;
	b.validate()?;
	full_projectors(a)?;
	full_projectors(b)?;
	let low = a.layout.num_qubits;
	let width = add(low, b.layout.num_qubits)?;
	bits(width)?;
	let mut d = a.clone();
	d.rows = mul(a.rows, b.rows)?;
	d.cols = mul(a.cols, b.cols)?;
	d.normalization = a.normalization * b.normalization;
	d.layout.num_qubits = width;
	d.layout.system_mask |= b.layout.system_mask << low;
	d.layout.workspace_mask |= b.layout.workspace_mask << low;
	d.layout.clean_workspace_mask |= b.layout.clean_workspace_mask << low;
	d.layout.clean_workspace_value |= b.layout.clean_workspace_value << low;
	d.left.fixed_mask |= b.left.fixed_mask << low;
	d.left.fixed_value |= b.left.fixed_value << low;
	d.left.logical_range = 0..d.rows;
	d.right.fixed_mask |= b.right.fixed_mask << low;
	d.right.fixed_value |= b.right.fixed_value << low;
	d.right.logical_range = 0..d.cols;
	d.errors = EncodingErrors {
		preparation: None,
		encoding: None,
		binary64_parameters: a.errors.binary64_parameters || b.errors.binary64_parameters,
	};
	d.source_identity = hash([0x5445_4e53_4f52_5031, a.source_identity, b.source_identity]);
	d.construction_identity = hash([
		0x5445_4e53_554e_4931,
		a.construction_identity,
		b.construction_identity,
	]);
	d.validate()?;
	Ok(d)
}
#[derive(Debug)]
struct TensorData<A, B> {
	a: A,
	b: B,
	descriptor: EncodingDescriptor,
	resources: PortfolioResources,
}
/// Ordered tensor product with the first factor's system coordinates varying fastest.
/// Full power-of-two system projectors are required by the compact range contract.
#[derive(Clone, Debug)]
pub struct TensorProduct<A: ReplayEncoding, B: ReplayEncoding>(Arc<TensorData<A, B>>);
impl<A: ReplayEncoding, B: ReplayEncoding> TensorProduct<A, B> {
	/// # Errors
	/// Rejects incompatible compact embeddings, normalization overflow and resources.
	pub fn new(a: A, b: B, l: PortfolioLimits) -> Result<Self> {
		let descriptor = product_descriptor(&a.descriptor()?, &b.descriptor()?)?;
		let mut compile_work = 0;
		let elementary_gates = add(
			count_with_work(&a, l, &mut compile_work)?,
			count_with_work(&b, l, &mut compile_work)?,
		)?;
		let resources = PortfolioResources {
			elementary_gates,
			oracle_queries: 2,
			workspace_qubits: usize::try_from(descriptor.layout.workspace_mask.count_ones())
				.map_err(|_| Error::Budget("tensor workspace"))?,
			retained_bytes: add(
				add(a.retained_bytes()?, b.retained_bytes()?)?,
				size_of::<TensorData<A, B>>() + 64,
			)?,
			..PortfolioResources::default()
		};
		let resources = PortfolioResources {
			compile_work,
			construction_peak_bytes: add(resources.retained_bytes, 4096)?,
			..resources
		};
		admit(resources, l)?;
		Ok(Self(Arc::new(TensorData {
			a,
			b,
			descriptor,
			resources,
		})))
	}
	#[must_use]
	pub fn resources(&self) -> PortfolioResources {
		self.0.resources
	}
}
impl<A: ReplayEncoding, B: ReplayEncoding> ReplayEncoding for TensorProduct<A, B> {
	fn descriptor(&self) -> Result<EncodingDescriptor> {
		Ok(self.0.descriptor.clone())
	}
	fn retained_bytes(&self) -> Result<usize> {
		Ok(self.0.resources.retained_bytes)
	}
	fn visit_replay(
		&self,
		adjoint: bool,
		v: &mut dyn FnMut(ReplayGate) -> Result<()>,
	) -> Result<()> {
		let aw = self.0.a.descriptor()?.layout.num_qubits;
		let bw = self.0.b.descriptor()?.layout.num_qubits;
		let a = targets(0, aw)?;
		let b = targets(aw, bw)?;
		if adjoint {
			self.0.b.visit_mapped_replay(&b[..bw], 0, 0, true, v)?;
			self.0.a.visit_mapped_replay(&a[..aw], 0, 0, true, v)
		} else {
			self.0.a.visit_mapped_replay(&a[..aw], 0, 0, false, v)?;
			self.0.b.visit_mapped_replay(&b[..bw], 0, 0, false, v)
		}
	}
}
#[derive(Clone, Debug)]
enum Lifted<A: ReplayEncoding, B: ReplayEncoding> {
	A {
		source: A,
		bridge: usize,
		descriptor: EncodingDescriptor,
	},
	B {
		source: B,
		bridge: usize,
		start: usize,
		descriptor: EncodingDescriptor,
	},
}
impl<A: ReplayEncoding, B: ReplayEncoding> ReplayEncoding for Lifted<A, B> {
	fn descriptor(&self) -> Result<EncodingDescriptor> {
		Ok(match self {
			Self::A { descriptor, .. } | Self::B { descriptor, .. } => descriptor.clone(),
		})
	}
	fn retained_bytes(&self) -> Result<usize> {
		add(
			size_of::<Self>(),
			match self {
				Self::A { source, .. } => source.retained_bytes()?,
				Self::B { source, .. } => source.retained_bytes()?,
			},
		)
	}
	fn visit_replay(
		&self,
		adjoint: bool,
		v: &mut dyn FnMut(ReplayGate) -> Result<()>,
	) -> Result<()> {
		let bridge = match self {
			Self::A { bridge, .. } | Self::B { bridge, .. } => *bridge,
		};
		if adjoint {
			visit_bridge(bridge, true, v)?;
		}
		match self {
			Self::A { source, .. } => source.visit_replay(adjoint, v)?,
			Self::B { source, start, .. } => {
				let width = source.descriptor()?.layout.num_qubits;
				let map = targets(*start, width)?;
				source.visit_mapped_replay(&map[..width], 0, 0, adjoint, v)?;
			}
		}
		if !adjoint {
			visit_bridge(bridge, false, v)?;
		}
		Ok(())
	}
}
// Full fixed workspace projectors admit a unitary XOR bridge from the right
// embedding to the left embedding, leaving every inactive system coordinate alone.
fn visit_bridge(
	mask: usize,
	adjoint: bool,
	v: &mut dyn FnMut(ReplayGate) -> Result<()>,
) -> Result<()> {
	for ordinal in 0..usize::BITS {
		let bit = if adjoint {
			usize::BITS - 1 - ordinal
		} else {
			ordinal
		};
		if mask & (1 << bit) != 0 {
			v(ReplayGate {
				kind: ReplayKind::X,
				target: Some(
					usize::try_from(bit).map_err(|_| Error::Budget("identity bridge width"))?,
				),
				control_mask: 0,
				control_value: 0,
			})?;
		}
	}
	Ok(())
}
/// A acting on the fast factor plus B acting on the slow factor, with independent work bits.
#[derive(Clone, Debug)]
pub struct KroneckerSum<A: ReplayEncoding, B: ReplayEncoding>(WeightedLcu<Lifted<A, B>>);
impl<A: ReplayEncoding, B: ReplayEncoding> KroneckerSum<A, B> {
	/// # Errors
	/// Rejects non-square/full-power embeddings and the composition resource limits.
	pub fn new(a: A, b: B, l: PortfolioLimits) -> Result<Self> {
		let ad = a.descriptor()?;
		let bd = b.descriptor()?;
		let mut da = product_descriptor(&ad, &bd)?;
		let mut db = da.clone();
		let a_bridge = ad.left.fixed_value ^ ad.right.fixed_value;
		let b_bridge = (bd.left.fixed_value ^ bd.right.fixed_value) << ad.layout.num_qubits;
		da.normalization = ad.normalization;
		db.normalization = bd.normalization;
		da.source_identity = hash([ad.source_identity, word(bd.rows)?, 0x4c49_4654_4131]);
		db.source_identity = hash([bd.source_identity, word(ad.rows)?, 0x4c49_4654_4231]);
		da.construction_identity = hash([
			ad.construction_identity,
			word(bd.layout.num_qubits)?,
			word(b_bridge)?,
			0x4252_4944_4745,
		]);
		db.construction_identity = hash([
			bd.construction_identity,
			word(ad.layout.num_qubits)?,
			word(a_bridge)?,
			0x4252_4944_4745,
		]);
		Ok(Self(WeightedLcu::new(
			vec![
				(
					Complex64::new(1.0, 0.0),
					Lifted::A {
						source: a,
						bridge: b_bridge,
						descriptor: da,
					},
				),
				(
					Complex64::new(1.0, 0.0),
					Lifted::B {
						source: b,
						bridge: a_bridge,
						start: ad.layout.num_qubits,
						descriptor: db,
					},
				),
			],
			l,
		)?))
	}
	#[must_use]
	pub fn resources(&self) -> PortfolioResources {
		self.0.resources()
	}
}
impl<A: ReplayEncoding, B: ReplayEncoding> ReplayEncoding for KroneckerSum<A, B> {
	fn descriptor(&self) -> Result<EncodingDescriptor> {
		self.0.descriptor()
	}
	fn retained_bytes(&self) -> Result<usize> {
		self.0.retained_bytes()
	}
	fn visit_replay(
		&self,
		adjoint: bool,
		v: &mut dyn FnMut(ReplayGate) -> Result<()>,
	) -> Result<()> {
		self.0.visit_replay(adjoint, v)
	}
}
