//! Weighted arithmetic tensor-shift stencils without CSR or basis enumeration.
use crate::{
	Complex64, EncodingBuilder, Error, ExplicitUnitaryPremise, Left, LogicalSpace, Normalization,
	NumericalPolicy, OracleFragment, ProjectedEncoding, ReplayGate, ReplayKind, Result, Right,
	TensorShiftEncoding,
	matching::{admit, bit},
};
use std::ops::{Mul, Neg};
use std::sync::Arc;
/// Immutable coefficient and reversible arithmetic source recipe.
#[derive(Clone, Debug)]
pub struct StructuredStencilTerm {
	weight: Complex64,
	shift: TensorShiftEncoding,
	theta: f64,
	phase: f64,
}
impl StructuredStencilTerm {
	/// Frozen complex coefficient.
	#[must_use]
	pub const fn weight(&self) -> Complex64 {
		self.weight
	}
	/// Arithmetic system permutation.
	#[must_use]
	pub const fn shift(&self) -> &TensorShiftEncoding {
		&self.shift
	}
}
/// Coherent encoding of a weighted sum of arithmetic tensor-shift permutations.
///
/// Layout is flag bit zero, system bits next, and padded color labels highest.
/// The projected block is the sum divided by K beta; no coefficient PREP identity
/// or exact symbolic floating-parameter certificate is asserted.
#[derive(Clone, Debug)]
pub struct StructuredStencilEncoding {
	system_qubits: usize,
	color_qubits: usize,
	num_colors: usize,
	beta: f64,
	normalization: Normalization,
	terms: Arc<[StructuredStencilTerm]>,
	gate_count: usize,
	source_identity: u64,
	identity_work: usize,
}
impl StructuredStencilEncoding {
	/// Freeze coefficients and arithmetic recipes in supplied deterministic order.
	/// Zero coefficients are removed; empty sums have K=beta=alpha=1.
	/// # Errors
	/// Rejects nonfinite coefficients, mismatched widths, normalization and storage overflow.
	/// Storage admission includes the fixed record-digest stack with live recipe owners.
	pub fn new(
		system_qubits: usize,
		mut terms: Vec<(Complex64, TensorShiftEncoding)>,
		policy: NumericalPolicy,
	) -> Result<Self> {
		bit(system_qubits
			.checked_add(1)
			.ok_or(Error::Budget("stencil width"))?)?;
		let input_bytes = terms
			.capacity()
			.checked_mul(size_of::<(Complex64, TensorShiftEncoding)>())
			.ok_or(Error::Budget("stencil input storage"))?;
		let mut recipe_bytes = 0_usize;
		let mut beta = 0.0_f64;
		for (weight, shift) in &terms {
			if !weight.re.is_finite() || !weight.im.is_finite() {
				return Err(Error::NonFinite);
			}
			if shift.num_qubits() != system_qubits {
				return Err(Error::Encoding("stencil system width"));
			}
			recipe_bytes = recipe_bytes
				.checked_add(shift.retained_bytes()?)
				.ok_or(Error::Budget("stencil recipe storage"))?;
			beta = beta.max(weight.norm());
		}
		if !beta.is_finite() {
			return Err(Error::NonFinite);
		}
		let peak = input_bytes
			.checked_add(recipe_bytes)
			.and_then(|n| {
				n.checked_add(
					terms
						.len()
						.checked_mul(size_of::<StructuredStencilTerm>())?
						.checked_mul(2)?,
				)
			})
			.and_then(|n| n.checked_add(size_of::<Self>()))
			.and_then(|n| n.checked_add(crate::RECORD_FINGERPRINT_SCRATCH_BYTES))
			.ok_or(Error::Budget("stencil construction storage"))?;
		admit(peak, policy)?;
		terms.retain(|(w, _)| w.re != 0.0 || w.im != 0.0);
		let num_colors = terms
			.len()
			.max(1)
			.checked_next_power_of_two()
			.ok_or(Error::Budget("stencil colors"))?;
		let color_qubits = usize::try_from(num_colors.trailing_zeros())
			.map_err(|_| Error::Budget("stencil color width"))?;
		bit(system_qubits
			.checked_add(color_qubits)
			.and_then(|n| n.checked_add(1))
			.ok_or(Error::Budget("stencil width"))?)?;
		if terms.is_empty() {
			beta = 1.0;
		}
		let alpha = beta.mul(f64::from(
			u32::try_from(num_colors).map_err(|_| Error::Budget("stencil color normalization"))?,
		));
		let mut frozen = crate::matching::reserve(terms.len())?;
		for (weight, shift) in terms {
			frozen.push(StructuredStencilTerm {
				weight,
				shift,
				theta: coefficient_angle(weight, beta),
				phase: weight.arg(),
			});
		}
		let identity_work = frozen
			.len()
			.checked_mul(crate::record_fingerprint_work(3)?)
			.ok_or(Error::Budget("stencil identity work"))?;
		let mut result = Self {
			system_qubits,
			color_qubits,
			num_colors,
			beta,
			normalization: Normalization::new(alpha)?,
			terms: Arc::from(frozen),
			gate_count: 0,
			source_identity: 0,
			identity_work,
		};
		result.source_identity = crate::owned_replay::stencil_source_identity(&result)?;
		let mut count = 0_usize;
		result.visit_gates(false, |_| result_count(&mut count))?;
		result.gate_count = count;
		admit(result.retained_bytes()?, policy)?;
		Ok(result)
	}
	/// Width of system bits, excluding flag and colors.
	#[must_use]
	pub const fn system_qubits(&self) -> usize {
		self.system_qubits
	}
	/// Width of padded color labels.
	#[must_use]
	pub const fn color_qubits(&self) -> usize {
		self.color_qubits
	}
	/// Modeled SHA record work performed once for this stencil's own records.
	/// Previously constructed child shifts have separate source work. Clones
	/// retain this original cost; byte-only policy does not impose a work ceiling.
	#[must_use]
	pub const fn source_fingerprint_work(&self) -> usize {
		self.identity_work
	}
	pub(crate) const fn cached_source_identity(&self) -> u64 {
		self.source_identity
	}
	/// Padded number of terms.
	#[must_use]
	pub const fn num_colors(&self) -> usize {
		self.num_colors
	}
	/// Maximum coefficient magnitude, one for a zero sum.
	#[must_use]
	pub const fn beta(&self) -> f64 {
		self.beta
	}
	/// Positive alpha=K beta.
	#[must_use]
	pub const fn normalization(&self) -> Normalization {
		self.normalization
	}
	/// Physical flag/system/color width.
	#[must_use]
	pub const fn num_qubits(&self) -> usize {
		self.system_qubits
			.saturating_add(self.color_qubits)
			.saturating_add(1)
	}
	/// Nonzero weighted recipes; no basis-index table is retained.
	#[must_use]
	pub fn terms(&self) -> &[StructuredStencilTerm] {
		&self.terms
	}
	/// Retained terms, arithmetic recipes and metadata, excluding allocator bookkeeping.
	/// # Errors
	/// Rejects accounting overflow.
	pub fn retained_bytes(&self) -> Result<usize> {
		let mut bytes = self
			.terms
			.len()
			.checked_mul(size_of::<StructuredStencilTerm>())
			.and_then(|n| n.checked_add(size_of::<Self>()))
			.and_then(|n| n.checked_add(2_usize.checked_mul(size_of::<usize>())?))
			.ok_or(Error::Budget("stencil storage"))?;
		for term in &*self.terms {
			bytes = bytes
				.checked_add(term.shift.retained_bytes()?)
				.ok_or(Error::Budget("stencil recipe storage"))?;
		}
		Ok(bytes)
	}
	/// Stream coherent gates, or their exact reversed adjoint order.
	/// # Errors
	/// Propagates visitor failure and checked control arithmetic.
	pub fn visit_gates(
		&self,
		adjoint: bool,
		mut visitor: impl FnMut(ReplayGate) -> Result<()>,
	) -> Result<()> {
		self.hadamards(adjoint, &mut visitor)?;
		for ordinal in 0..self.num_colors {
			let color = if adjoint {
				self.num_colors
					.checked_sub(ordinal)
					.and_then(|n| n.checked_sub(1))
					.ok_or(Error::Budget("stencil reverse color"))?
			} else {
				ordinal
			};
			self.color_gates(color, adjoint, &mut visitor)?;
		}
		self.hadamards(adjoint, &mut visitor)
	}
	fn hadamards(
		&self,
		adjoint: bool,
		visitor: &mut dyn FnMut(ReplayGate) -> Result<()>,
	) -> Result<()> {
		for ordinal in 0..self.color_qubits {
			let color = if adjoint {
				self.color_qubits
					.checked_sub(ordinal)
					.and_then(|n| n.checked_sub(1))
					.ok_or(Error::Budget("stencil reverse H"))?
			} else {
				ordinal
			};
			visitor(ReplayGate {
				kind: ReplayKind::H,
				target: Some(
					self.system_qubits
						.checked_add(1)
						.and_then(|n| n.checked_add(color))
						.ok_or(Error::Budget("stencil H bit"))?,
				),
				control_mask: 0,
				control_value: 0,
			})?;
		}
		Ok(())
	}
	fn color_gates(
		&self,
		color: usize,
		adjoint: bool,
		visitor: &mut dyn FnMut(ReplayGate) -> Result<()>,
	) -> Result<()> {
		let width = self
			.system_qubits
			.checked_add(1)
			.ok_or(Error::Budget("stencil color position"))?;
		let mask = bit(self.num_qubits())?
			.checked_sub(1)
			.ok_or(Error::Budget("stencil color mask"))?
			& !bit(width)?
				.checked_sub(1)
				.ok_or(Error::Budget("stencil color mask"))?;
		let value = color
			.checked_shl(u32::try_from(width).map_err(|_| Error::Budget("stencil color value"))?)
			.ok_or(Error::Budget("stencil color value"))?;
		let term = self.terms.get(color);
		let theta = term.map_or(std::f64::consts::PI, |t| t.theta);
		let rotation = ReplayGate {
			kind: ReplayKind::Ry(if adjoint { theta.neg() } else { theta }),
			target: Some(0),
			control_mask: mask,
			control_value: value,
		};
		let phase = ReplayGate {
			kind: ReplayKind::Phase(
				term.map_or(0.0, |t| if adjoint { t.phase.neg() } else { t.phase }),
			),
			target: None,
			control_mask: mask | 1,
			control_value: value,
		};
		let permutation = |visitor: &mut dyn FnMut(ReplayGate) -> Result<()>| -> Result<()> {
			if let Some(t) = term {
				t.shift.visit_gates(adjoint, |gate| {
					visitor(ReplayGate {
						kind: gate.kind,
						target: gate
							.target
							.map(|n| {
								n.checked_add(1)
									.ok_or(Error::Budget("stencil shift target"))
							})
							.transpose()?,
						control_mask: gate
							.control_mask
							.checked_mul(2)
							.ok_or(Error::Budget("stencil shift control"))?
							| mask,
						control_value: gate
							.control_value
							.checked_mul(2)
							.ok_or(Error::Budget("stencil shift value"))?
							| value,
					})
				})?;
			}
			Ok(())
		};
		if adjoint {
			permutation(visitor)?;
			visitor(phase)?;
			visitor(rotation)?;
		} else {
			visitor(rotation)?;
			visitor(phase)?;
			permutation(visitor)?;
		}
		Ok(())
	}
	/// Replay under signed controls on remapped caller-selected targets.
	/// # Errors
	/// Rejects repeated/overlapping targets, invalid controls and visitor errors.
	pub fn visit_mapped_gates(
		&self,
		targets: &[usize],
		outer_mask: usize,
		outer_value: usize,
		adjoint: bool,
		mut visitor: impl FnMut(ReplayGate) -> Result<()>,
	) -> Result<()> {
		if targets.len() != self.num_qubits() || outer_value & !outer_mask != 0 {
			return Err(Error::Encoding("stencil mapping"));
		}
		let mut occupied = outer_mask;
		for &target in targets {
			let mask = bit(target)?;
			if occupied & mask != 0 {
				return Err(Error::Encoding("stencil target overlap"));
			}
			occupied |= mask;
		}
		self.visit_gates(adjoint, |gate| {
			visitor(gate.mapped(targets, outer_mask, outer_value)?)
		})
	}
	/// Materialize only bounded compiler instructions for small compatibility callers.
	/// # Errors
	/// Rejects circuit storage and compiler failures.
	pub fn to_oracle(&self, policy: NumericalPolicy) -> Result<OracleFragment> {
		crate::replay::oracle_from_replay(
			self.num_qubits(),
			self.gate_count,
			self.retained_bytes()?,
			policy,
			|visitor| self.visit_gates(false, visitor),
		)
	}
	/// Expose the weighted stencil with fixed flag/color zero and free system bits.
	/// # Errors
	/// Rejects logical-space or instruction storage admission.
	pub fn projected_encoding(&self, policy: NumericalPolicy) -> Result<ProjectedEncoding> {
		let dimension = bit(self.num_qubits())?;
		let system_dimension = bit(self.system_qubits)?;
		let system_mask = system_dimension
			.checked_sub(1)
			.and_then(|n| n.checked_mul(2))
			.ok_or(Error::Budget("stencil projection"))?;
		let mask = dimension
			.checked_sub(1)
			.ok_or(Error::Budget("stencil projection"))?
			& !system_mask;
		EncodingBuilder::new().oracle(self.to_oracle(policy)?)
   .left(LogicalSpace::<Left>::constrained_range(dimension,mask,0,0..system_dimension,policy)?)
   .right(LogicalSpace::<Right>::constrained_range(dimension,mask,0,0..system_dimension,policy)?)
   .normalization(self.normalization.get())?.policy(policy)
   .unitarity_assumption(ExplicitUnitaryPremise::new("Weighted arithmetic stencil oracle composes coherent controlled H, Ry, phase and reversible modular shifts; binary64 parameters are not exact-symbolic certificates")?).build()
	}
}
// Numerical flag rotation from a finite normalized coefficient; roundoff is not an exact certificate.
#[allow(clippy::arithmetic_side_effects)]
fn coefficient_angle(weight: Complex64, beta: f64) -> f64 {
	2.0 * (weight.norm() / beta).clamp(0.0, 1.0).acos()
}
fn result_count(count: &mut usize) -> Result<()> {
	*count = count
		.checked_add(1)
		.ok_or(Error::Budget("stencil gate count"))?;
	Ok(())
}
