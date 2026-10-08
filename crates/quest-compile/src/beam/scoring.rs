//! Cost comparison and terminal scoring under explicit budget leases.
use super::{
	ApproximationMode, BoundGate, BoundRegion, BudgetCategory, BudgetLease, BudgetLedger,
	CliffordCost, CommunicationCost, CostComparison, CostComponents, CostProfile, Error,
	NativeCost, Operation, OptimizationOptions, OptimizationTarget, Result, RoundingStatus,
	TerminalOptions, TerminalPasses, TerminalStatus,
};

pub(super) fn clifford_cost(bound: &BoundRegion, ledger: &BudgetLedger) -> Result<CostComponents> {
	let count = bound
		.instructions()
		.len()
		.checked_add(bound.dependencies().len())
		.and_then(|n| n.checked_mul(64))
		.ok_or(Error::Budget("beam cost work"))?;
	let _work = ledger.reserve(
		BudgetCategory::Verification,
		u64::try_from(count).map_err(|_| Error::Budget("beam cost work"))?,
		0,
	)?;
	let mut t = 0u64;
	let mut two = 0u64;
	for instruction in bound.instructions() {
		if let Operation::Gate {
			gate,
			targets,
			controls,
		} = instruction.operation()
		{
			if matches!(gate, BoundGate::T | BoundGate::Tdg) {
				t = t.checked_add(1).ok_or(Error::Budget("beam cost"))?;
			}
			if targets
				.len()
				.checked_add(controls.len())
				.ok_or(Error::Budget("beam cost"))?
				>= 2
			{
				two = two.checked_add(1).ok_or(Error::Budget("beam cost"))?;
			}
		}
	}
	let operations =
		u64::try_from(bound.instructions().len()).map_err(|_| Error::Budget("beam cost"))?;
	let depth = u64::try_from(bound.dependency_depth()).map_err(|_| Error::Budget("beam cost"))?;
	let native = NativeCost::new(0, 0, 0, 0, 0, CommunicationCost::known(0));
	Ok(CostComponents::new(
		native,
		CliffordCost::new(t, two, depth, operations),
	))
}

pub fn score_bound(
	bound: &BoundRegion,
	target: &OptimizationTarget,
	ledger: &BudgetLedger,
) -> Result<CostComponents> {
	if target.profile() == CostProfile::CliffordTV1 {
		return clifford_cost(bound, ledger);
	}
	let retained = bound.retained_bytes()?;
	let _copy = ledger.reserve(
		BudgetCategory::Candidate,
		0,
		u64::try_from(retained).map_err(|_| Error::Budget("beam score storage"))?,
	)?;
	let setup = bound
		.instructions()
		.len()
		.checked_add(bound.dependencies().len())
		.and_then(|n| n.checked_mul(64))
		.ok_or(Error::Budget("beam score work"))?;
	let _setup = ledger.reserve(
		BudgetCategory::Verification,
		u64::try_from(setup).map_err(|_| Error::Budget("beam score work"))?,
		0,
	)?;
	let plan = bound.clone().plan()?;
	let native = NativeCost::from_embedded_plan(&plan, target.deployment(), ledger, 0)?;
	Ok(CostComponents::new(native, CliffordCost::new(0, 0, 0, 0)))
}

pub(super) fn precedes(
	a: &CostComponents,
	b: &CostComponents,
	target: &OptimizationTarget,
	ledger: &BudgetLedger,
	unscorable: &mut usize,
) -> Result<bool> {
	let _work = ledger.reserve(BudgetCategory::Verification, 4096, 0)?;
	match target.compare(a, b)? {
		CostComparison::Better => Ok(true),
		CostComparison::Unscorable => {
			*unscorable = unscorable
				.checked_add(1)
				.ok_or(Error::Budget("beam comparisons"))?;
			Ok(false)
		}
		CostComparison::Equal | CostComparison::Worse => Ok(false),
	}
}

pub(super) type ScoredPublication = (
	CostComponents,
	Option<BoundRegion>,
	RoundingStatus,
	Vec<BudgetLease>,
	TerminalStatus,
);

pub(super) fn score_candidate(
	bound: &BoundRegion,
	config: &OptimizationOptions,
	ledger: &BudgetLedger,
) -> Result<ScoredPublication> {
	if config.target().profile() != CostProfile::NativeV1
		|| matches!(config.approximation(), ApproximationMode::Global(_))
	{
		return Ok((
			score_bound(bound, config.target(), ledger)?,
			None,
			RoundingStatus::Unchanged,
			Vec::new(),
			TerminalStatus::Complete,
		));
	}
	let retained = bound.retained_bytes()?;
	let _copy = ledger.reserve(
		BudgetCategory::Candidate,
		0,
		u64::try_from(retained).map_err(|_| Error::Budget("beam terminal storage"))?,
	)?;
	let terminal =
		bound
			.clone()
			.schedule_and_fuse(TerminalOptions::default(), config.target(), ledger)?;
	if terminal.report().status() == TerminalStatus::Complete
		&& let Some(score) = terminal.report().published_cost().cloned()
	{
		let rounding = if terminal.report().rounding_changed() {
			RoundingStatus::NumericalFusionChanged
		} else {
			RoundingStatus::Unchanged
		};
		let (program, allowances) = terminal.into_parts();
		return Ok((
			score,
			Some(program),
			rounding,
			allowances,
			TerminalStatus::Complete,
		));
	}
	let status = terminal.report().status();
	Ok((
		score_bound(bound, config.target(), ledger)?,
		None,
		RoundingStatus::Unchanged,
		Vec::new(),
		status,
	))
}
