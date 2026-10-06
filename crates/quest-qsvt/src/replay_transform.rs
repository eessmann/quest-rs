//! Owned QSVT schedules over general replayable encodings, without expanded circuits.
use crate::{Complex64, Error, MatchingEncoding, NumericalPolicy, Result, StandardConvention};
use quest_qsp::{PhaseSequence, WxSymmetric};
use std::ops::{Mul, Neg};

/// One semantic operation interpreted against the separately prepared matching source.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum TransformStep {
	/// Unconditional Hadamard on the response bit.
	Hadamard,
	/// Unconditional Z rotation on the response bit.
	ResponseRotation(f64),
	/// Coherent exp(i angle (2 Pi - I)) under a signed response control.
	Projector {
		left: bool,
		angle: f64,
		response: bool,
	},
	/// The same whole source U or U adjoint, under a signed response control.
	Oracle { adjoint: bool, response: bool },
}
impl TransformStep {
	const fn adjoint(self) -> Self {
		match self {
			Self::Hadamard => Self::Hadamard,
			Self::ResponseRotation(angle) => Self::ResponseRotation(-angle),
			Self::Projector {
				left,
				angle,
				response,
			} => Self::Projector {
				left,
				angle: -angle,
				response,
			},
			Self::Oracle { adjoint, response } => Self::Oracle {
				adjoint: !adjoint,
				response,
			},
		}
	}
}
/// Compact immutable QSVT phase schedule, independent of the full sparse source.
///
/// Only a compact encoding descriptor and the actual projector phases are retained.
/// Coherent oracle steps are generated lazily, including the adjoint orientation.
#[derive(Clone, Debug)]
pub struct TransformSchedule {
	descriptor: crate::EncodingDescriptor,
	phases: std::sync::Arc<Vec<f64>>,
	readout: f64,
	conversion_roundoff: f64,
	projector_response_bound: Option<f64>,
}
impl TransformSchedule {
	/// Convert Wx phases using the shared convention algorithm, owning no source records.
	/// # Errors
	/// Rejects descriptor, input/conversion/output capacity and response-width budgets.
	pub fn from_phase_sequence(
		descriptor: crate::EncodingDescriptor,
		sequence: PhaseSequence<WxSymmetric>,
		policy: NumericalPolicy,
	) -> Result<Self> {
		descriptor.validate()?;
		crate::matching::bit(
			descriptor
				.layout
				.num_qubits
				.checked_add(1)
				.ok_or(Error::Budget("QSVT response width"))?,
		)?;
		admit_phase_conversion(&sequence, 0, 0, size_of::<Self>(), policy)?;
		let degree = sequence.degree();
		let converted = WxSymmetric::projector_phases(&sequence);
		let live = sequence
			.retained_bytes()?
			.checked_add(converted.retained_bytes()?)
			.and_then(|n| n.checked_add(sequence.values().len().checked_mul(size_of::<f64>())?))
			.and_then(|n| n.checked_add(size_of::<Self>()))
			.and_then(|n| n.checked_add(size_of::<Vec<f64>>()))
			.and_then(|n| n.checked_add(size_of::<[usize; 2]>()))
			.ok_or(Error::Budget("QSVT live converted capacity"))?;
		crate::matching::admit(live, policy)?;
		drop(sequence);
		let readout = f64::from(
			u32::try_from(degree.saturating_sub(1) % 4)
				.map_err(|_| Error::Budget("readout phase"))?,
		)
		.mul(std::f64::consts::PI)
		.neg();
		let mut phases = crate::matching::reserve(converted.values().len())?;
		let peak = phases
			.capacity()
			.checked_mul(size_of::<f64>())
			.and_then(|n| n.checked_add(converted.retained_bytes().ok()?))
			.and_then(|n| n.checked_add(size_of::<Self>()))
			.and_then(|n| n.checked_add(size_of::<Vec<f64>>()))
			.and_then(|n| n.checked_add(size_of::<[usize; 2]>()))
			.ok_or(Error::Budget("QSVT converted phase capacity"))?;
		crate::matching::admit(peak, policy)?;
		phases.extend_from_slice(converted.values());
		let mut result = Self::from_parts(descriptor, phases, readout, policy)?;
		result.conversion_roundoff = converted.roundoff_estimate();
		Ok(result)
	}
	/// Copy the exact independently certified projector payload and scalar response attestation.
	/// The caller retains the full certificate/provenance separately.
	/// # Errors
	/// Rejects certificate evidence, source contract and concurrent storage admission.
	#[cfg(feature = "certification")]
	pub fn from_certified_projector(
		descriptor: crate::EncodingDescriptor,
		certificate: &quest_qsp::certification::CertifiedProjectorPhases,
		policy: NumericalPolicy,
	) -> Result<Self> {
		descriptor.validate()?;
		crate::matching::bit(
			descriptor
				.layout
				.num_qubits
				.checked_add(1)
				.ok_or(Error::Budget("QSVT response width"))?,
		)?;
		let envelope = certificate
			.attempts()
			.iter()
			.map(quest_qsp::certification::CertificationAttempt::modeled_peak_bytes)
			.max()
			.ok_or(Error::Encoding(
				"projector certificate lacks accounting evidence",
			))?;
		let planned = certificate
			.values()
			.len()
			.checked_mul(size_of::<f64>())
			.and_then(|n| n.checked_add(envelope))
			.and_then(|n| n.checked_add(size_of::<Self>()))
			.and_then(|n| n.checked_add(size_of::<Vec<f64>>()))
			.and_then(|n| n.checked_add(size_of::<[usize; 2]>()))
			.ok_or(Error::Budget("QSVT certified phase capacity"))?;
		crate::matching::admit(planned, policy)?;
		let mut phases = crate::matching::reserve(certificate.values().len())?;
		let peak = phases
			.capacity()
			.checked_mul(size_of::<f64>())
			.and_then(|n| n.checked_add(envelope))
			.and_then(|n| n.checked_add(size_of::<Self>()))
			.and_then(|n| n.checked_add(size_of::<Vec<f64>>()))
			.and_then(|n| n.checked_add(size_of::<[usize; 2]>()))
			.ok_or(Error::Budget("QSVT certified phase capacity"))?;
		crate::matching::admit(peak, policy)?;
		phases.extend_from_slice(certificate.values());
		let mut result = Self::from_parts(descriptor, phases, certificate.readout_phase(), policy)?;
		result.projector_response_bound = Some(certificate.response_bound().upper_f64());
		Ok(result)
	}
	/// Freeze an explicitly supplied rounded projector phase payload.
	/// This constructor carries no independent response certificate.
	///
	/// # Errors
	/// Rejects header/phase/width invariants, retained capacities and storage limits.
	pub fn from_parts(
		descriptor: crate::EncodingDescriptor,
		phases: Vec<f64>,
		readout: f64,
		policy: NumericalPolicy,
	) -> Result<Self> {
		descriptor.validate()?;
		if phases.is_empty() || phases.iter().any(|p| !p.is_finite()) || !readout.is_finite() {
			return Err(Error::Encoding("invalid QSVT projector payload"));
		}
		crate::matching::bit(
			descriptor
				.layout
				.num_qubits
				.checked_add(1)
				.ok_or(Error::Budget("QSVT response width"))?,
		)?;
		phases
			.len()
			.checked_sub(1)
			.and_then(|degree| degree.checked_mul(4))
			.and_then(|count| count.checked_add(5))
			.ok_or(Error::Budget("QSVT schedule count"))?;
		let bytes = phases
			.capacity()
			.checked_mul(size_of::<f64>())
			.and_then(|n| n.checked_add(size_of::<Self>()))
			.and_then(|n| n.checked_add(size_of::<Vec<f64>>()))
			.and_then(|n| n.checked_add(2_usize.checked_mul(size_of::<usize>())?))
			.ok_or(Error::Budget("QSVT phase storage"))?;
		crate::matching::admit(bytes, policy)?;
		Ok(Self {
			descriptor,
			phases: std::sync::Arc::new(phases),
			readout,
			conversion_roundoff: 0.0,
			projector_response_bound: None,
		})
	}
	/// Scalar source metadata; no coefficient or completed permutation table is retained.
	#[must_use]
	pub const fn descriptor(&self) -> &crate::EncodingDescriptor {
		&self.descriptor
	}
	/// Actual rounded projector phases in product order.
	#[must_use]
	pub fn values(&self) -> &[f64] {
		&self.phases
	}
	/// Actual response readout angle.
	#[must_use]
	pub const fn readout_phase(&self) -> f64 {
		self.readout
	}
	/// Polynomial query degree.
	#[must_use]
	pub fn degree(&self) -> usize {
		self.phases.len().saturating_sub(1)
	}
	/// Response bit is above the complete matching register.
	/// # Errors
	/// Retains checked scalar header width failures.
	pub fn num_qubits(&self) -> Result<usize> {
		self.descriptor
			.layout
			.num_qubits
			.checked_add(1)
			.ok_or(Error::Budget("QSVT response width"))
	}
	/// Diagnostic conversion estimate; zero when phases were supplied directly.
	#[must_use]
	pub const fn conversion_roundoff_estimate(&self) -> f64 {
		self.conversion_roundoff
	}
	/// Uniform response-bound attestation carried from the exact owned certificate.
	/// This scalar attestation does not retain the full certificate or its source polynomial.
	#[must_use]
	pub const fn projector_response_bound(&self) -> Option<f64> {
		self.projector_response_bound
	}
	/// Exact retained phase capacity plus schedule/Arc metadata, excluding allocator bookkeeping.
	/// # Errors
	/// Rejects integer accounting overflow.
	pub fn retained_bytes(&self) -> Result<usize> {
		self.phases
			.capacity()
			.checked_mul(size_of::<f64>())
			.and_then(|n| n.checked_add(size_of::<Self>()))
			.and_then(|n| n.checked_add(size_of::<Vec<f64>>()))
			.and_then(|n| n.checked_add(2_usize.checked_mul(size_of::<usize>())?))
			.ok_or(Error::Budget("QSVT phase storage"))
	}
	/// Lazily replay the admitted phase schedule, preserving the visitor's error type.
	///
	/// # Errors
	/// Propagates visitor errors and checked index failures through `E::from`.
	pub fn visit_steps<E: From<Error>>(
		&self,
		adjoint: bool,
		mut visitor: impl FnMut(TransformStep) -> std::result::Result<(), E>,
	) -> std::result::Result<(), E> {
		let count = self
			.degree()
			.checked_mul(4)
			.and_then(|n| n.checked_add(5))
			.ok_or(Error::Budget("QSVT schedule count"))
			.map_err(E::from)?;
		for ordinal in 0..count {
			let index = if adjoint {
				count
					.checked_sub(ordinal)
					.and_then(|n| n.checked_sub(1))
					.ok_or(Error::Budget("QSVT reverse schedule"))
					.map_err(E::from)?
			} else {
				ordinal
			};
			let step = self.step_at(index).map_err(E::from)?;
			visitor(if adjoint { step.adjoint() } else { step })?;
		}
		Ok(())
	}
	fn step_at(&self, index: usize) -> Result<TransformStep> {
		if index == 0 {
			return Ok(TransformStep::Hadamard);
		}
		let degree = self.degree();
		let branch = degree
			.checked_mul(2)
			.and_then(|n| n.checked_add(1))
			.ok_or(Error::Budget("QSVT branch count"))?;
		let relative = index
			.checked_sub(1)
			.ok_or(Error::Budget("QSVT schedule index"))?;
		let branches = branch
			.checked_mul(2)
			.ok_or(Error::Budget("QSVT branches"))?;
		if relative == branches {
			return Ok(TransformStep::ResponseRotation(self.readout));
		}
		if relative
			== branches
				.checked_add(1)
				.ok_or(Error::Budget("QSVT schedule endpoint"))?
		{
			return Ok(TransformStep::Hadamard);
		}
		if relative >= branches {
			return Err(Error::Encoding("QSVT schedule index"));
		}
		let response = relative >= branch;
		let position = if response {
			relative
				.checked_sub(branch)
				.ok_or(Error::Budget("QSVT branch index"))?
		} else {
			relative
		};
		self.branch_step(position, response)
	}
	fn branch_step(&self, position: usize, response: bool) -> Result<TransformStep> {
		let degree = self.degree();
		let phase = |i: usize| -> Result<f64> {
			let value = *self
				.phases
				.get(i)
				.ok_or(Error::Encoding("QSVT phase index"))?;
			Ok(if response { value.neg() } else { value })
		};
		let rounds = degree
			.checked_div(2)
			.and_then(|n| n.checked_mul(4))
			.ok_or(Error::Budget("QSVT round count"))?;
		if position < rounds {
			let k = degree
				.checked_sub(
					position
						.checked_div(4)
						.and_then(|n| n.checked_mul(2))
						.ok_or(Error::Budget("QSVT round index"))?,
				)
				.ok_or(Error::Budget("QSVT phase index"))?;
			Ok(match position % 4 {
				0 => TransformStep::Projector {
					left: false,
					angle: phase(k)?,
					response,
				},
				1 => TransformStep::Oracle {
					adjoint: false,
					response,
				},
				2 => TransformStep::Projector {
					left: true,
					angle: phase(k.checked_sub(1).ok_or(Error::Budget("QSVT phase index"))?)?,
					response,
				},
				_ => TransformStep::Oracle {
					adjoint: true,
					response,
				},
			})
		} else if degree.is_multiple_of(2) {
			Ok(TransformStep::Projector {
				left: false,
				angle: phase(0)?,
				response,
			})
		} else {
			Ok(
				match position
					.checked_sub(rounds)
					.ok_or(Error::Budget("QSVT terminal index"))?
				{
					0 => TransformStep::Projector {
						left: false,
						angle: phase(1)?,
						response,
					},
					1 => TransformStep::Oracle {
						adjoint: false,
						response,
					},
					_ => TransformStep::Projector {
						left: true,
						angle: phase(0)?,
						response,
					},
				},
			)
		}
	}
}
/// Compatibility schedule over the matching manifest and shared general iteration.
#[derive(Clone, Debug)]
pub struct MatchingSchedule {
	header: crate::MatchingHeader,
	inner: TransformSchedule,
}
impl MatchingSchedule {
	/// Freeze an explicitly supplied rounded projector payload.
	/// # Errors
	/// Rejects invalid manifest/phases, widths and retained storage budgets.
	pub fn from_parts(
		header: crate::MatchingHeader,
		phases: Vec<f64>,
		readout: f64,
		policy: NumericalPolicy,
	) -> Result<Self> {
		let inner = TransformSchedule::from_parts(
			crate::EncodingDescriptor::from_matching_header(header)?,
			phases,
			readout,
			policy,
		)?;
		let result = Self { header, inner };
		crate::matching::admit(result.retained_bytes()?, policy)?;
		Ok(result)
	}
	#[must_use]
	pub const fn header(&self) -> crate::MatchingHeader {
		self.header
	}
	#[must_use]
	pub fn values(&self) -> &[f64] {
		self.inner.values()
	}
	#[must_use]
	pub const fn readout_phase(&self) -> f64 {
		self.inner.readout_phase()
	}
	#[must_use]
	pub fn degree(&self) -> usize {
		self.inner.degree()
	}
	/// # Errors
	/// Rejects response width overflow.
	pub fn num_qubits(&self) -> Result<usize> {
		self.inner.num_qubits()
	}
	#[must_use]
	pub const fn conversion_roundoff_estimate(&self) -> f64 {
		self.inner.conversion_roundoff_estimate()
	}
	#[must_use]
	pub const fn projector_response_bound(&self) -> Option<f64> {
		self.inner.projector_response_bound()
	}
	/// General descriptor and phase schedule, sharing the immutable phase payload.
	#[must_use]
	pub const fn general_schedule(&self) -> &TransformSchedule {
		&self.inner
	}
	/// # Errors
	/// Rejects retained storage accounting overflow.
	pub fn retained_bytes(&self) -> Result<usize> {
		self.inner
			.retained_bytes()?
			.checked_add(size_of::<crate::MatchingHeader>())
			.ok_or(Error::Budget("matching schedule bytes"))
	}
	/// Replay without retaining expanded operations.
	/// # Errors
	/// Propagates visitor and checked index failures.
	pub fn visit_steps<E: From<Error>>(
		&self,
		adjoint: bool,
		visitor: impl FnMut(TransformStep) -> std::result::Result<(), E>,
	) -> std::result::Result<(), E> {
		self.inner.visit_steps(adjoint, visitor)
	}
}
// The caller's imported Vec remains retained while conversion allocates its
// output. Also reserve a concurrent frozen phase copy and its owning metadata.
fn admit_phase_conversion(
	sequence: &PhaseSequence<WxSymmetric>,
	source_bytes: usize,
	owner_bytes: usize,
	schedule_bytes: usize,
	policy: NumericalPolicy,
) -> Result<()> {
	let input_bytes = sequence.retained_bytes()?;
	let bytes = sequence
		.values()
		.len()
		.checked_mul(size_of::<f64>())
		.and_then(|n| n.checked_mul(2))
		.and_then(|n| n.checked_add(input_bytes))
		.and_then(|n| n.checked_add(source_bytes))
		.and_then(|n| n.checked_add(owner_bytes))
		.and_then(|n| n.checked_add(schedule_bytes))
		.and_then(|n| n.checked_add(size_of::<quest_qsp::ConvertedProjectorPhases>()))
		.and_then(|n| n.checked_add(size_of::<Vec<f64>>()))
		.and_then(|n| n.checked_add(2_usize.checked_mul(size_of::<usize>())?))
		.ok_or(Error::Budget("QSVT conversion bytes"))?;
	crate::matching::admit(bytes, policy)
}
/// Owning root/reference QSVT execution source and its independent compact phase schedule.
#[derive(Clone, Debug)]
pub struct MatchingTransform {
	encoding: MatchingEncoding,
	schedule: MatchingSchedule,
	certificate_bytes: usize,
	#[cfg(feature = "certification")]
	projector_certificate:
		Option<std::sync::Arc<quest_qsp::certification::CertifiedProjectorPhases>>,
}
impl MatchingTransform {
	/// Convert supplied Wx phases once into a bounded replay schedule.
	/// # Errors
	/// Rejects retained/temporary phase/source storage and checked response widths.
	pub fn new(
		encoding: MatchingEncoding,
		sequence: PhaseSequence<WxSymmetric>,
		policy: NumericalPolicy,
	) -> Result<Self> {
		admit_phase_conversion(
			&sequence,
			encoding.resources().retained_bytes,
			size_of::<Self>(),
			size_of::<MatchingSchedule>(),
			policy,
		)?;
		let header = crate::MatchingHeader::from_encoding(&encoding)?;
		let mut phase_policy = policy;
		phase_policy.max_bytes = policy
			.max_bytes
			.checked_sub(
				encoding
					.resources()
					.retained_bytes
					.checked_add(size_of::<Self>())
					.ok_or(Error::Budget("QSVT source bytes"))?,
			)
			.ok_or(Error::Budget("QSVT source bytes"))?;
		let inner = TransformSchedule::from_phase_sequence(
			crate::EncodingDescriptor::from_matching_header(header)?,
			sequence,
			phase_policy,
		)?;
		Ok(Self {
			encoding,
			schedule: MatchingSchedule { header, inner },
			certificate_bytes: 0,
			#[cfg(feature = "certification")]
			projector_certificate: None,
		})
	}
	/// Retain and execute the exact independently certified rounded projector phases/readout.
	/// # Errors
	/// Rejects source/schedule/certificate admission and width/index overflow.
	#[cfg(feature = "certification")]
	pub fn from_certified_projector(
		encoding: MatchingEncoding,
		certificate: quest_qsp::certification::CertifiedProjectorPhases,
		policy: NumericalPolicy,
	) -> Result<Self> {
		let certificate_bytes = certificate
			.attempts()
			.iter()
			.map(quest_qsp::certification::CertificationAttempt::modeled_peak_bytes)
			.max()
			.ok_or(Error::Encoding(
				"projector certificate lacks accounting evidence",
			))?;
		let peak = certificate
			.values()
			.len()
			.checked_mul(size_of::<f64>())
			.and_then(|n| n.checked_mul(2))
			.and_then(|n| n.checked_add(encoding.resources().retained_bytes))
			.and_then(|n| n.checked_add(size_of::<Self>()))
			.and_then(|n| n.checked_add(size_of::<MatchingSchedule>()))
			.and_then(|n| n.checked_add(certificate_bytes))
			.ok_or(Error::Budget("QSVT schedule bytes"))?;
		crate::matching::admit(peak, policy)?;
		let header = crate::MatchingHeader::from_encoding(&encoding)?;
		let mut phase_policy = policy;
		phase_policy.max_bytes = policy
			.max_bytes
			.checked_sub(
				encoding
					.resources()
					.retained_bytes
					.checked_add(size_of::<Self>())
					.ok_or(Error::Budget("QSVT source bytes"))?,
			)
			.ok_or(Error::Budget("QSVT source bytes"))?;
		let inner = TransformSchedule::from_certified_projector(
			crate::EncodingDescriptor::from_matching_header(header)?,
			&certificate,
			phase_policy,
		)?;
		Ok(Self {
			encoding,
			schedule: MatchingSchedule { header, inner },
			certificate_bytes,
			projector_certificate: Some(std::sync::Arc::new(certificate)),
		})
	}
	/// Export independently owned phase/header replay, retaining no sparse source or full certificate.
	/// # Errors
	/// Rejects exported retained phase storage beyond policy.
	pub fn schedule(&self, policy: NumericalPolicy) -> Result<MatchingSchedule> {
		crate::matching::admit(self.schedule.retained_bytes()?, policy)?;
		Ok(self.schedule.clone())
	}
	/// Retained source, compact phase schedule and conservative certificate allowance.
	/// # Errors
	/// Rejects integer accounting overflow.
	pub fn retained_bytes(&self) -> Result<usize> {
		self.schedule
			.retained_bytes()?
			.checked_add(self.encoding.resources().retained_bytes)
			.and_then(|n| n.checked_add(size_of::<Self>()))
			.and_then(|n| n.checked_add(self.certificate_bytes))
			.ok_or(Error::Budget("QSVT retained bytes"))
	}
	/// Exact owned independent projector certificate, when supplied.
	#[cfg(feature = "certification")]
	#[must_use]
	pub fn projector_certificate(
		&self,
	) -> Option<&quest_qsp::certification::CertifiedProjectorPhases> {
		self.projector_certificate.as_deref()
	}
	#[must_use]
	pub const fn encoding(&self) -> &MatchingEncoding {
		&self.encoding
	}
	#[must_use]
	pub fn degree(&self) -> usize {
		self.schedule.degree()
	}
	#[must_use]
	pub fn num_qubits(&self) -> usize {
		self.encoding.num_qubits().saturating_add(1)
	}
	#[must_use]
	pub const fn conversion_roundoff_estimate(&self) -> f64 {
		self.schedule.inner.conversion_roundoff
	}
	/// Replay either orientation without expanded semantic/gate instruction storage.
	/// # Errors
	/// Propagates visitor and checked-index failures.
	pub fn visit_steps(
		&self,
		adjoint: bool,
		visitor: impl FnMut(TransformStep) -> Result<()>,
	) -> Result<()> {
		self.schedule.visit_steps(adjoint, visitor)
	}
	/// Whole-register reference execution, including unsuccessful sectors.
	/// This is an explicitly bounded simulator, never a large-problem fallback.
	/// # Errors
	/// Rejects full-state size/byte limits, nonfinite state, and source failures.
	#[allow(
		clippy::arithmetic_side_effects,
		reason = "Finite binary64 unitary kernels validate generated amplitudes before reporting success"
	)]
	pub fn apply_reference(
		&self,
		state: &mut [Complex64],
		adjoint: bool,
		policy: NumericalPolicy,
	) -> Result<()> {
		let half = 1usize
			.checked_shl(
				u32::try_from(self.encoding.num_qubits())
					.map_err(|_| Error::Budget("QSVT state width"))?,
			)
			.ok_or(Error::Budget("QSVT state width"))?;
		let length = half
			.checked_mul(2)
			.ok_or(Error::Budget("QSVT state length"))?;
		let bytes = length
			.checked_add(half)
			.and_then(|n| n.checked_mul(size_of::<Complex64>()))
			.and_then(|n| n.checked_add(self.retained_bytes().ok()?))
			.ok_or(Error::Budget("QSVT reference bytes"))?;
		if length != state.len() {
			return Err(Error::Encoding("QSVT replay state shape"));
		}
		if bytes > policy.max_bytes {
			return Err(Error::Budget("QSVT reference state"));
		}
		if state.iter().any(|z| !z.re.is_finite() || !z.im.is_finite()) {
			return Err(Error::NonFinite);
		}
		self.visit_steps(adjoint, |step| {
			match step {
				TransformStep::Hadamard => {
					let (low, high) = state.split_at_mut(half);
					for (a, b) in low.iter_mut().zip(high) {
						let x = *a;
						let y = *b;
						*a = (x + y) * std::f64::consts::FRAC_1_SQRT_2;
						*b = (x - y) * std::f64::consts::FRAC_1_SQRT_2;
					}
				}
				TransformStep::ResponseRotation(angle) => {
					let (low, high) = state.split_at_mut(half);
					for z in low {
						*z *= Complex64::from_polar(1.0, -0.5 * angle);
					}
					for z in high {
						*z *= Complex64::from_polar(1.0, 0.5 * angle);
					}
				}
				TransformStep::Projector {
					left,
					angle,
					response,
				} => {
					let block = if response {
						state
							.get_mut(half..)
							.ok_or(Error::Encoding("QSVT response range"))?
					} else {
						state
							.get_mut(..half)
							.ok_or(Error::Encoding("QSVT response range"))?
					};
					let logical = if left {
						self.encoding.rows()
					} else {
						self.encoding.cols()
					};
					let positive = Complex64::from_polar(1.0, angle);
					let negative = positive.conj();
					for (i, z) in block.iter_mut().enumerate() {
						*z *= if i % 2 == 0 && i / 2 < logical {
							positive
						} else {
							negative
						};
					}
				}
				TransformStep::Oracle { adjoint, response } => {
					let block = if response {
						state
							.get_mut(half..)
							.ok_or(Error::Encoding("QSVT response range"))?
					} else {
						state
							.get_mut(..half)
							.ok_or(Error::Encoding("QSVT response range"))?
					};
					self.encoding.apply_reference(block, adjoint, policy)?;
				}
			}
			Ok(())
		})?;
		if state.iter().any(|z| !z.re.is_finite() || !z.im.is_finite()) {
			return Err(Error::NonFinite);
		}
		Ok(())
	}

	/// Prepare a normalized RHS only in the right successful logical subspace.
	/// # Errors
	/// Rejects zero/nonfinite RHS, shape mismatch, or state allocation budget.
	#[allow(
		clippy::arithmetic_side_effects,
		reason = "Finite RHS normalization checks all inputs and its resulting norm"
	)]
	pub fn prepare_rhs(
		&self,
		rhs: &[Complex64],
		policy: NumericalPolicy,
	) -> Result<(Vec<Complex64>, f64)> {
		if rhs.len() != self.encoding.cols() {
			return Err(Error::Encoding("QSVT RHS dimension"));
		}
		let norm = rhs.iter().fold(0.0_f64, |n, z| n.hypot(z.norm()));
		if !norm.is_finite() || norm <= 0.0 {
			return Err(Error::Encoding("QSVT RHS norm"));
		}
		let length = 1usize
			.checked_shl(
				u32::try_from(self.num_qubits()).map_err(|_| Error::Budget("QSVT state width"))?,
			)
			.ok_or(Error::Budget("QSVT state width"))?;
		if length
			.checked_mul(size_of::<Complex64>())
			.and_then(|n| n.checked_add(self.retained_bytes().ok()?))
			.is_none_or(|n| n > policy.max_bytes)
		{
			return Err(Error::Budget("QSVT RHS preparation"));
		}
		let mut state = Vec::new();
		state
			.try_reserve_exact(length)
			.map_err(|_| Error::Budget("QSVT RHS allocation"))?;
		state.resize(length, Complex64::new(0.0, 0.0));
		for (i, z) in rhs.iter().enumerate() {
			*state
				.get_mut(i.checked_mul(2).ok_or(Error::Budget("QSVT RHS index"))?)
				.ok_or(Error::Encoding("QSVT RHS index"))? = *z / norm;
		}
		Ok((state, norm))
	}
	/// Decode unnormalized successful amplitudes; success mass is not discarded.
	/// # Errors
	/// Rejects incorrectly sized state or logical allocation failure.
	pub fn successful_amplitudes(&self, state: &[Complex64]) -> Result<Vec<Complex64>> {
		let length = 1usize
			.checked_shl(
				u32::try_from(self.num_qubits()).map_err(|_| Error::Budget("QSVT state width"))?,
			)
			.ok_or(Error::Budget("QSVT state width"))?;
		if length != state.len() {
			return Err(Error::Encoding("QSVT output state shape"));
		}
		let count = if self.degree().is_multiple_of(2) {
			self.encoding.cols()
		} else {
			self.encoding.rows()
		};
		let mut output = Vec::new();
		output
			.try_reserve_exact(count)
			.map_err(|_| Error::Budget("QSVT output allocation"))?;
		for i in 0..count {
			output.push(
				*state
					.get(i.checked_mul(2).ok_or(Error::Budget("QSVT output index"))?)
					.ok_or(Error::Encoding("QSVT output index"))?,
			);
		}
		Ok(output)
	}
}

/// Owns an arbitrary replay encoding and its bound compact QSVT phase schedule.
#[derive(Clone, Debug)]
pub struct ReplayTransform<E: crate::ReplayEncoding> {
	encoding: E,
	schedule: TransformSchedule,
	certificate_bytes: usize,
	#[cfg(feature = "certification")]
	projector_certificate:
		Option<std::sync::Arc<quest_qsp::certification::CertifiedProjectorPhases>>,
}
impl<E: crate::ReplayEncoding> ReplayTransform<E> {
	/// Convert Wx phases once and bind them to the complete source descriptor.
	/// # Errors
	/// Rejects conversion/source storage, descriptor and response-width failures.
	pub fn new(
		encoding: E,
		sequence: PhaseSequence<WxSymmetric>,
		policy: NumericalPolicy,
	) -> Result<Self> {
		admit_phase_conversion(
			&sequence,
			encoding.retained_bytes()?,
			size_of::<Self>(),
			size_of::<TransformSchedule>(),
			policy,
		)?;
		let mut phase_policy = policy;
		phase_policy.max_bytes = policy
			.max_bytes
			.checked_sub(
				encoding
					.retained_bytes()?
					.checked_add(size_of::<Self>())
					.ok_or(Error::Budget("QSVT source bytes"))?,
			)
			.ok_or(Error::Budget("QSVT source bytes"))?;
		let schedule =
			TransformSchedule::from_phase_sequence(encoding.descriptor()?, sequence, phase_policy)?;
		Self::from_schedule(encoding, schedule, policy)
	}
	/// Bind an independently owned phase schedule to its exact source contract.
	/// # Errors
	/// Rejects source, layout, construction, normalization or projector mismatches and storage budgets.
	pub fn from_schedule(
		encoding: E,
		schedule: TransformSchedule,
		policy: NumericalPolicy,
	) -> Result<Self> {
		let descriptor = encoding.descriptor()?;
		descriptor.validate()?;
		if descriptor != schedule.descriptor {
			return Err(Error::Encoding("QSVT source/layout/construction mismatch"));
		}
		let result = Self {
			encoding,
			schedule,
			certificate_bytes: 0,
			#[cfg(feature = "certification")]
			projector_certificate: None,
		};
		crate::matching::admit(result.retained_bytes()?, policy)?;
		Ok(result)
	}
	/// Retain exact independently certified rounded phases and readout.
	/// # Errors
	/// Rejects certificate accounting, descriptor and source/storage budgets.
	#[cfg(feature = "certification")]
	pub fn from_certified_projector(
		encoding: E,
		certificate: quest_qsp::certification::CertifiedProjectorPhases,
		policy: NumericalPolicy,
	) -> Result<Self> {
		let certificate_bytes = certificate
			.attempts()
			.iter()
			.map(quest_qsp::certification::CertificationAttempt::modeled_peak_bytes)
			.max()
			.ok_or(Error::Encoding(
				"projector certificate lacks accounting evidence",
			))?;
		let peak = certificate
			.values()
			.len()
			.checked_mul(size_of::<f64>())
			.and_then(|n| n.checked_add(certificate_bytes))
			.and_then(|n| n.checked_add(encoding.retained_bytes().ok()?))
			.and_then(|n| n.checked_add(size_of::<Self>()))
			.and_then(|n| n.checked_add(size_of::<TransformSchedule>()))
			.ok_or(Error::Budget("QSVT certified schedule bytes"))?;
		crate::matching::admit(peak, policy)?;
		let mut phase_policy = policy;
		phase_policy.max_bytes = policy
			.max_bytes
			.checked_sub(
				encoding
					.retained_bytes()?
					.checked_add(size_of::<Self>())
					.ok_or(Error::Budget("QSVT source bytes"))?,
			)
			.ok_or(Error::Budget("QSVT source bytes"))?;
		let schedule = TransformSchedule::from_certified_projector(
			encoding.descriptor()?,
			&certificate,
			phase_policy,
		)?;
		let mut result = Self::from_schedule(encoding, schedule, policy)?;
		result.certificate_bytes = certificate_bytes;
		result.projector_certificate = Some(std::sync::Arc::new(certificate));
		crate::matching::admit(result.retained_bytes()?, policy)?;
		Ok(result)
	}
	#[cfg(feature = "certification")]
	#[must_use]
	pub fn projector_certificate(
		&self,
	) -> Option<&quest_qsp::certification::CertifiedProjectorPhases> {
		self.projector_certificate.as_deref()
	}
	#[must_use]
	pub const fn encoding(&self) -> &E {
		&self.encoding
	}
	#[must_use]
	pub const fn descriptor(&self) -> &crate::EncodingDescriptor {
		self.schedule.descriptor()
	}
	#[must_use]
	pub fn degree(&self) -> usize {
		self.schedule.degree()
	}
	/// # Errors
	/// Rejects response width overflow.
	pub fn num_qubits(&self) -> Result<usize> {
		self.schedule.num_qubits()
	}
	/// Export an independently owning schedule, retaining no source recipes.
	/// # Errors
	/// Rejects retained schedule storage beyond policy.
	pub fn schedule(&self, policy: NumericalPolicy) -> Result<TransformSchedule> {
		crate::matching::admit(self.schedule.retained_bytes()?, policy)?;
		Ok(self.schedule.clone())
	}
	/// Fallible conservative source, phase and certificate storage accounting.
	/// # Errors
	/// Rejects integer overflow.
	pub fn retained_bytes(&self) -> Result<usize> {
		self.schedule
			.retained_bytes()?
			.checked_add(self.encoding.retained_bytes()?)
			.and_then(|n| n.checked_add(size_of::<Self>()))
			.and_then(|n| n.checked_add(self.certificate_bytes))
			.ok_or(Error::Budget("QSVT owned transform bytes"))
	}
	/// Replay the single shared semantic iteration in either orientation.
	/// # Errors
	/// Propagates visitor and checked iteration failures.
	pub fn visit_steps<VisitorError: From<Error>>(
		&self,
		adjoint: bool,
		visitor: impl FnMut(TransformStep) -> std::result::Result<(), VisitorError>,
	) -> std::result::Result<(), VisitorError> {
		self.schedule.visit_steps(adjoint, visitor)
	}
	/// Lower steps lazily to whole-unitary primitives; compact projectors remain bit cubes.
	/// # Errors
	/// Propagates source/visitor and checked control failures.
	pub fn visit_gates(
		&self,
		adjoint: bool,
		mut visitor: impl FnMut(crate::ReplayGate) -> Result<()>,
	) -> Result<()> {
		let width = self.descriptor().layout.num_qubits;
		let response_mask = crate::matching::bit(width)?;
		self.visit_steps(adjoint, |step| {
			match step {
				TransformStep::Hadamard => visitor(crate::ReplayGate {
					kind: crate::ReplayKind::H,
					target: Some(width),
					control_mask: 0,
					control_value: 0,
				})?,
				TransformStep::ResponseRotation(angle) => {
					visitor(crate::ReplayGate {
						kind: crate::ReplayKind::Phase(angle.mul(-0.5)),
						target: None,
						control_mask: 0,
						control_value: 0,
					})?;
					visitor(crate::ReplayGate {
						kind: crate::ReplayKind::Phase(angle),
						target: None,
						control_mask: response_mask,
						control_value: response_mask,
					})?;
				}
				TransformStep::Projector {
					left,
					angle,
					response,
				} => {
					let value = if response { response_mask } else { 0 };
					visitor(crate::ReplayGate {
						kind: crate::ReplayKind::Phase(angle.neg()),
						target: None,
						control_mask: response_mask,
						control_value: value,
					})?;
					let projector = if left {
						&self.descriptor().left
					} else {
						&self.descriptor().right
					};
					projector.visit_cubes(width, |mask, fixed| {
						// Two equal phases preserve every finite input angle without
						// overflowing an intermediate doubled binary64 parameter.
						let gate = crate::ReplayGate {
							kind: crate::ReplayKind::Phase(angle),
							target: None,
							control_mask: mask | response_mask,
							control_value: fixed | value,
						};
						visitor(gate)?;
						visitor(gate)
					})?;
				}
				TransformStep::Oracle { adjoint, response } => {
					self.encoding.visit_replay(adjoint, &mut |mut gate| {
						gate.control_mask |= response_mask;
						if response {
							gate.control_value |= response_mask;
						}
						visitor(gate)
					})?;
				}
			}
			Ok(())
		})
	}
	/// Replay on remapped complete source/response operands under signed outer controls.
	/// # Errors
	/// Rejects operand/control overlap and propagates visitor failures.
	pub fn visit_mapped_gates(
		&self,
		targets: &[usize],
		outer_mask: usize,
		outer_value: usize,
		adjoint: bool,
		mut visitor: impl FnMut(crate::ReplayGate) -> Result<()>,
	) -> Result<()> {
		crate::owned_replay::validate_mapping(
			self.num_qubits()?,
			targets,
			outer_mask,
			outer_value,
		)?;
		self.visit_gates(adjoint, |gate| {
			visitor(gate.mapped(targets, outer_mask, outer_value)?)
		})
	}
	/// Small-instance whole-register reference, including failure and padded sectors.
	/// # Errors
	/// Rejects full-state shape, nonfinite values and storage admission failures.
	pub fn apply_reference(
		&self,
		state: &mut [Complex64],
		adjoint: bool,
		policy: NumericalPolicy,
	) -> Result<()> {
		crate::owned_replay::admit_state(
			state,
			self.num_qubits()?,
			self.retained_bytes()?,
			policy,
		)?;
		self.visit_gates(adjoint, |gate| crate::owned_replay::apply_gate(state, gate))?;
		if state.iter().any(|z| !z.re.is_finite() || !z.im.is_finite()) {
			return Err(Error::NonFinite);
		}
		Ok(())
	}
	/// Small-instance reference preserving controls, remapped operands and spectators.
	/// # Errors
	/// Rejects malformed mappings, nonfinite states and full-state storage budgets.
	pub fn apply_mapped_reference(
		&self,
		state: &mut [Complex64],
		targets: &[usize],
		outer_mask: usize,
		outer_value: usize,
		adjoint: bool,
		policy: NumericalPolicy,
	) -> Result<()> {
		if state.is_empty() || !state.len().is_power_of_two() {
			return Err(Error::Encoding("owned replay state shape"));
		}
		let width = usize::try_from(state.len().ilog2())
			.map_err(|_| Error::Budget("owned replay width"))?;
		crate::owned_replay::admit_state(state, width, self.retained_bytes()?, policy)?;
		crate::owned_replay::validate_mapping(
			self.num_qubits()?,
			targets,
			outer_mask,
			outer_value,
		)?;
		if outer_mask >= state.len() || targets.iter().any(|&t| t >= width) {
			return Err(Error::Encoding("owned replay physical width"));
		}
		self.visit_mapped_gates(targets, outer_mask, outer_value, adjoint, |gate| {
			crate::owned_replay::apply_gate(state, gate)
		})?;
		if state.iter().any(|z| !z.re.is_finite() || !z.im.is_finite()) {
			return Err(Error::NonFinite);
		}
		Ok(())
	}
	/// Expand bounded conventional instructions without a dense unitary snapshot.
	/// # Errors
	/// Rejects gate/storage count overflow and circuit admission failures.
	pub fn to_oracle(&self, policy: NumericalPolicy) -> Result<crate::OracleFragment> {
		let mut count = 0usize;
		self.visit_gates(false, |_| {
			count = count
				.checked_add(1)
				.ok_or(Error::Budget("QSVT replay gate count"))?;
			Ok(())
		})?;
		crate::replay::oracle_from_replay(
			self.num_qubits()?,
			count,
			self.retained_bytes()?,
			policy,
			|visitor| self.visit_gates(false, visitor),
		)
	}
}
