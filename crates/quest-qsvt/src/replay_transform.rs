//! Owned QSVT schedules over replayable matching oracles, without expanded circuits.
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
/// Only scalar matching metadata and the actual projector phases are retained.
/// Coherent oracle steps are generated lazily, including the adjoint orientation.
#[derive(Clone, Debug)]
pub struct MatchingSchedule {
	header: crate::MatchingHeader,
	phases: std::sync::Arc<Vec<f64>>,
	readout: f64,
	conversion_roundoff: f64,
	projector_response_bound: Option<f64>,
}
impl MatchingSchedule {
	/// Freeze an explicitly supplied rounded projector phase payload.
	/// This constructor carries no independent response certificate.
	///
	/// # Errors
	/// Rejects header/phase/width invariants, retained capacities and storage limits.
	pub fn from_parts(
		header: crate::MatchingHeader,
		phases: Vec<f64>,
		readout: f64,
		policy: NumericalPolicy,
	) -> Result<Self> {
		header.validate()?;
		if phases.is_empty() || phases.iter().any(|p| !p.is_finite()) || !readout.is_finite() {
			return Err(Error::Encoding("invalid QSVT projector payload"));
		}
		crate::matching::bit(
			header
				.num_qubits()?
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
			header,
			phases: std::sync::Arc::new(phases),
			readout,
			conversion_roundoff: 0.0,
			projector_response_bound: None,
		})
	}
	/// Scalar source metadata; no coefficient or completed permutation table is retained.
	#[must_use]
	pub const fn header(&self) -> crate::MatchingHeader {
		self.header
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
		self.header
			.num_qubits()?
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
		let phase_bytes = sequence
			.values()
			.len()
			.checked_mul(size_of::<f64>())
			.and_then(|n| n.checked_mul(3))
			.and_then(|n| n.checked_add(encoding.resources().retained_bytes))
			.and_then(|n| n.checked_add(size_of::<Self>()))
			.ok_or(Error::Budget("QSVT conversion bytes"))?;
		crate::matching::admit(phase_bytes, policy)?;
		let degree = sequence.degree();
		let converted = WxSymmetric::projector_phases(&sequence);
		drop(sequence);
		let reduced = degree.saturating_sub(1) % 4;
		let readout =
			f64::from(u32::try_from(reduced).map_err(|_| Error::Budget("readout phase"))?)
				.mul(std::f64::consts::PI)
				.neg();
		Self::from_phases(
			encoding,
			converted.values(),
			readout,
			converted.roundoff_estimate(),
			0,
			policy,
		)
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
		let mut result = Self::from_phases(
			encoding,
			certificate.values(),
			certificate.readout_phase(),
			0.0,
			certificate_bytes,
			policy,
		)?;
		result.schedule.projector_response_bound = Some(certificate.response_bound().upper_f64());
		result.projector_certificate = Some(std::sync::Arc::new(certificate));
		Ok(result)
	}
	fn from_phases(
		encoding: MatchingEncoding,
		phases: &[f64],
		readout: f64,
		conversion_roundoff: f64,
		certificate_bytes: usize,
		policy: NumericalPolicy,
	) -> Result<Self> {
		let bytes = phases
			.len()
			.checked_mul(size_of::<f64>())
			.and_then(|n| n.checked_mul(2))
			.and_then(|n| n.checked_add(encoding.resources().retained_bytes))
			.and_then(|n| n.checked_add(size_of::<Self>()))
			.and_then(|n| n.checked_add(size_of::<MatchingSchedule>()))
			.and_then(|n| n.checked_add(certificate_bytes))
			.ok_or(Error::Budget("QSVT schedule bytes"))?;
		crate::matching::admit(bytes, policy)?;
		let header = crate::MatchingHeader::from_encoding(&encoding)?;
		let mut values = crate::matching::reserve(phases.len())?;
		values.extend_from_slice(phases);
		let mut schedule = MatchingSchedule::from_parts(header, values, readout, policy)?;
		schedule.conversion_roundoff = conversion_roundoff;
		Ok(Self {
			encoding,
			schedule,
			certificate_bytes,
			#[cfg(feature = "certification")]
			projector_certificate: None,
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
		self.schedule.conversion_roundoff
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
