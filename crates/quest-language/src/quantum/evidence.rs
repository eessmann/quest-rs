//! Typed historical certificate records. Data retention grants no proof authority.
use super::{Control, OccurrenceId, ProvenanceId, QubitId, RegionSnapshotId};
use crate::ssa;
use dashu_base::BitTest;
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum EvidenceScope {
	HistoricalLocalCertificate,
}
#[derive(Debug, Clone)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serde", serde(deny_unknown_fields))]
pub enum EvidenceLocation {
	Finite {
		occurrence: OccurrenceId,
		targets: Vec<QubitId>,
		controls: Vec<Control>,
		outputs: Vec<OccurrenceId>,
		input: RegionSnapshotId,
		output: ProvenanceId,
	},
	Structured {
		block: ssa::BlockId,
		instruction: usize,
		memory: ssa::ValueId,
		interface: Vec<ssa::Place>,
		outputs: Vec<ssa::ValueId>,
		input: ssa::SnapshotId,
		output: ssa::SnapshotId,
	},
}
#[derive(Debug, Clone)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serde", serde(deny_unknown_fields))]
pub struct CompilationEvidence {
	pub version: u32,
	pub scope: EvidenceScope,
	pub location: EvidenceLocation,
	pub algorithm: String,
	pub seed: u64,
	pub target: quest_math::Target,
	pub candidate: quest_math::Sequence,
	pub epsilon_bits: u64,
	pub limits: quest_math::Limits,
}
impl CompilationEvidence {
	/// Conservative owned storage bound, including large exact target coefficients.
	#[must_use]
	pub fn retained_bytes(&self) -> usize {
		let location = match &self.location {
			EvidenceLocation::Finite {
				targets,
				controls,
				outputs,
				..
			} => targets
				.capacity()
				.saturating_mul(size_of::<QubitId>())
				.saturating_add(controls.capacity().saturating_mul(size_of::<Control>()))
				.saturating_add(outputs.capacity().saturating_mul(size_of::<OccurrenceId>())),
			EvidenceLocation::Structured {
				interface, outputs, ..
			} => interface
				.iter()
				.fold(
					interface.capacity().saturating_mul(size_of::<ssa::Place>()),
					|bytes, place| {
						bytes.saturating_add(
							place
								.indices
								.capacity()
								.saturating_mul(size_of::<ssa::ValueId>()),
						)
					},
				)
				.saturating_add(outputs.capacity().saturating_mul(size_of::<ssa::ValueId>())),
		};
		size_of::<Self>()
			.saturating_add(self.algorithm.capacity())
			.saturating_add(location)
			.saturating_add(
				self.candidate.operations.iter().fold(
					self.candidate
						.operations
						.capacity()
						.saturating_mul(size_of::<quest_math::Operation>()),
					|n, op| {
						n.saturating_add(op.targets.capacity().saturating_mul(size_of::<usize>()))
							.saturating_add(
								op.controls
									.capacity()
									.saturating_mul(size_of::<quest_math::Control>()),
							)
					},
				),
			)
			.saturating_add(target_bytes(&self.target.angle))
	}
}

fn target_bytes(angle: &quest_math::AngleTarget) -> usize {
	fn integer(n: &dashu_int::IBig) -> usize {
		usize::try_from(u64::try_from(n.bit_len()).unwrap_or(u64::MAX).div_ceil(64))
			.unwrap_or(usize::MAX)
			.saturating_mul(8)
	}
	match angle {
		quest_math::AngleTarget::DyadicRadians { .. } => 0,
		quest_math::AngleTarget::RationalPi {
			numerator,
			denominator,
		} => integer(numerator).saturating_add(integer(denominator)),
		quest_math::AngleTarget::AffinePi {
			radians_numerator,
			radians_denominator,
			pi_numerator,
			pi_denominator,
		} => integer(radians_numerator)
			.saturating_add(integer(radians_denominator))
			.saturating_add(integer(pi_numerator))
			.saturating_add(integer(pi_denominator)),
	}
}
