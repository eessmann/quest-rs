//! Worker admission, bounded hints, and retained independent certificates.
use super::{
	Arc, BeamMitmStatus, BitTest, BoundRegion, BudgetLease, BudgetLedger, Error, Gate, MitmResult,
	ParameterId, QuantumRegion, RBig, Result, SemanticOperation, StopReason, ZxPasses,
};

#[cfg(feature = "workers")]
pub(super) fn mitm_status<T>(result: &MitmResult<T>, window: (usize, usize)) -> BeamMitmStatus {
	match result {
		MitmResult::Candidate(_) => BeamMitmStatus::Candidate { window },
		MitmResult::NoCandidate { explored } => BeamMitmStatus::NoCandidate {
			explored: *explored,
		},
		MitmResult::Incomplete { reason, explored } => BeamMitmStatus::Incomplete {
			reason: reason.chars().take(256).collect(),
			explored: *explored,
		},
		MitmResult::Exhausted { explored } => BeamMitmStatus::Exhausted {
			explored: *explored,
		},
		MitmResult::Unresolved {
			precision_bits,
			explored,
		} => BeamMitmStatus::Unresolved {
			precision_bits: *precision_bits,
			explored: *explored,
		},
	}
}

#[cfg(feature = "workers")]
#[derive(Clone, Copy)]
pub(super) struct WorkerContext<'a> {
	pub(super) client: &'a quest_optimizer_client::Client,
	pub(super) seed: u64,
	pub(super) limits: quest_math::Limits,
}

#[cfg(feature = "workers")]
#[derive(Debug)]
pub(super) struct ExactHistory {
	pub(super) parent: Option<Arc<Self>>,
	pub(super) certificate: Arc<crate::ExactRegionCertificate>,
	pub(super) _proof_allowance: Arc<BudgetLease>,
}
#[cfg(feature = "workers")]
#[derive(Debug)]
pub(super) struct ApproxHistory {
	pub(super) parent: Option<Arc<Self>>,
	pub(super) certificate: ApproxEvidence,
	pub(super) _proof_allowance: Arc<BudgetLease>,
}

#[cfg(feature = "workers")]
#[derive(Debug)]
pub(super) enum ApproxEvidence {
	Synthesis(Arc<crate::RotationCertificate>),
	Mitm(Arc<crate::ApproxRegionCertificate>),
}

#[cfg(feature = "workers")]
pub(super) fn zx_generation(
	source: &QuantumRegion,
	offset: usize,
	expanded: bool,
	worker: WorkerContext<'_>,
	seed: u64,
	max_output: usize,
) -> std::result::Result<(QuantumRegion, crate::ZxReport), crate::WorkerError> {
	if expanded {
		source.clone().zx_expanded_candidate_from(
			offset,
			worker.client,
			seed,
			worker.limits,
			1,
			max_output,
		)
	} else {
		source
			.clone()
			.zx_candidate_from(offset, worker.client, seed, worker.limits, 1, max_output)
	}
}

#[cfg(feature = "workers")]
pub(super) fn rotation_count(source: &QuantumRegion) -> usize {
	source
		.occurrences()
		.iter()
		.filter(|item| {
			matches!(
				&item.operation,
				SemanticOperation::Gate {
					gate: Gate::Rx(_) | Gate::Ry(_) | Gate::Rz(_),
					..
				}
			)
		})
		.count()
}

#[cfg(feature = "workers")]
pub(super) fn exact_mitm_hint(source: &QuantumRegion) -> bool {
	source.occurrences().iter().any(|item| {
		matches!(
			&item.operation,
			SemanticOperation::Gate {
				gate: Gate::H
					| Gate::X
					| Gate::Y
					| Gate::Z
					| Gate::S
					| Gate::Sdg
					| Gate::T
					| Gate::Tdg
					| Gate::Swap
					| Gate::Phase(_),
				..
			} | SemanticOperation::GlobalPhase { .. }
		)
	})
}

#[cfg(feature = "workers")]
pub(super) fn admitted_epsilon(
	budget: &RBig,
	spent: &RBig,
	count: usize,
	limits: quest_math::Limits,
) -> Result<Option<(f64, RBig)>> {
	if count == 0 {
		return Ok(None);
	}
	let remaining = std::ops::Sub::sub(budget.clone(), spent.clone());
	if remaining <= RBig::ZERO {
		return Ok(None);
	}
	let per = std::ops::Div::div(remaining, RBig::from(dashu_int::IBig::from(count)));
	// numerator >= 2^(nbits-1), denominator < 2^dbits; this power of two
	// lies strictly below the rational allowance without floating conversion.
	let exponent = i64::try_from(per.numerator().bit_len())
		.and_then(|numerator| {
			i64::try_from(per.denominator().bit_len())
				.map(|denominator| numerator.saturating_sub(denominator).saturating_sub(2))
		})
		.map_err(|_| Error::Budget("beam approximation precision"))?;
	let bits = if exponent >= -1 {
		0.5f64.to_bits()
	} else if exponent < -1074 {
		return Ok(None);
	} else if exponent >= -1022 {
		u64::try_from(
			exponent
				.checked_add(1023)
				.ok_or(Error::Budget("beam approximation precision"))?,
		)
		.map_err(|_| Error::Budget("beam approximation precision"))?
		.checked_shl(52)
		.ok_or(Error::Budget("beam approximation precision"))?
	} else {
		1u64.checked_shl(
			u32::try_from(
				exponent
					.checked_add(1074)
					.ok_or(Error::Budget("beam approximation precision"))?,
			)
			.map_err(|_| Error::Budget("beam approximation precision"))?,
		)
		.ok_or(Error::Budget("beam approximation precision"))?
	};
	let epsilon = f64::from_bits(bits);
	let exact = quest_math::dyadic_from_bits(bits, limits)
		.map_err(|_| Error::Budget("beam approximation precision"))?;
	if exact > per {
		return Err(Error::Budget("beam approximation rounding"));
	}
	Ok(Some((epsilon, exact)))
}

#[cfg(feature = "workers")]
pub(super) fn bind_candidate(
	candidate: &QuantumRegion,
	pairs: &[(ParameterId, f64)],
	parent: &BoundRegion,
	ledger: &BudgetLedger,
) -> Result<BoundRegion> {
	let binding_work = candidate.binding_work_estimate()?;
	let binding_allowance = ledger.reserve_work_allowance(binding_work)?;
	let mut bound = candidate.clone().bind(pairs)?;
	binding_allowance.commit(binding_work)?;
	bound.retain_source(
		parent.source_snapshot_id(),
		parent.binding_storage().clone(),
	)?;
	Ok(bound)
}

#[cfg(feature = "workers")]
pub(super) fn budget_stop(reason: &str) -> StopReason {
	if reason.contains("byte") || reason.contains("storage") || reason.contains("allocation") {
		StopReason::StorageLimit
	} else {
		StopReason::WorkLimit
	}
}
