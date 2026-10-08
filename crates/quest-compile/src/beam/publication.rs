//! Input routing and publication of accepted execution snapshots.
#[cfg(feature = "workers")]
use super::WorkerContext;
use super::{
	BeamOptions, BeamReport, BeamRun, BoundRegion, BudgetCategory, BudgetLedger, CompilerResult,
	CostComparison, Error, ExactOptions, ExactPasses, OptimizationEvidence, OptimizationOptions,
	OptimizerInput, RoundingStatus, StopReason, TerminalStatus, bound_as_ideal, run_region,
	score_candidate, structured_error,
};

pub(super) fn run_inner(
	input: OptimizerInput,
	config: &OptimizationOptions,
	ledger: &BudgetLedger,
	beam: BeamOptions,
	#[cfg(feature = "workers")] worker: Option<WorkerContext<'_>>,
) -> CompilerResult<BeamRun> {
	let report = BeamReport {
		input_snapshot: input.snapshot_id(),
		rounds: 0,
		generated: 0,
		admitted: 0,
		maximum_frontier_operations: 0,
		unscorable_comparisons: 0,
		generator_budget_exhaustions: 0,
		structured: None,
		#[cfg(feature = "workers")]
		worker_requests: 0,
		#[cfg(feature = "workers")]
		exact_regions: Vec::new(),
		#[cfg(feature = "workers")]
		retained_exact_history: None,
		#[cfg(feature = "workers")]
		local_rotations: Vec::new(),
		#[cfg(feature = "workers")]
		cumulative_error: None,
		#[cfg(feature = "workers")]
		retained_approx_history: None,
		#[cfg(feature = "workers")]
		worker_failures: Vec::new(),
		#[cfg(feature = "workers")]
		worker_declines: 0,
		#[cfg(feature = "workers")]
		exact_mitm_statuses: Vec::new(),
		#[cfg(feature = "workers")]
		approx_mitm_statuses: Vec::new(),
		#[cfg(feature = "workers")]
		approx_regions: Vec::new(),
	};
	match input {
		OptimizerInput::Region { source, bound } => run_region(
			source,
			bound,
			config,
			ledger,
			beam,
			report,
			#[cfg(feature = "workers")]
			worker,
		),
		OptimizerInput::Bound(bound) => run_bound(
			bound,
			config,
			ledger,
			beam,
			report,
			#[cfg(feature = "workers")]
			worker,
		),
		OptimizerInput::VerifiedStructured(program) => {
			let outcome = program
				.optimize_structured(config, ledger)
				.map_err(structured_error)?;
			let mut report = report;
			report.rounds = 1;
			report.generated = 1;
			report.admitted = outcome
				.report()
				.exact_report()
				.map_or(0, |exact| exact.rewrites.len());
			let rounding = if outcome
				.report()
				.terminal_report()
				.is_some_and(crate::StructuredTerminalReport::rounding_changed)
			{
				RoundingStatus::NumericalFusionChanged
			} else {
				RoundingStatus::Unchanged
			};
			let stop_reason = match outcome.report().status() {
				TerminalStatus::WorkLimit => StopReason::WorkLimit,
				TerminalStatus::StorageLimit => StopReason::StorageLimit,
				TerminalStatus::Unscorable => StopReason::Unscorable,
				TerminalStatus::Complete => StopReason::Complete,
			};
			let evidence = if outcome.report().exact_report().is_some() {
				OptimizationEvidence::ExactVerified
			} else {
				OptimizationEvidence::OriginalInput
			};
			let (program, structured, allowances) = outcome.into_parts();
			report.structured = Some(structured);
			Ok(BeamRun {
				input: OptimizerInput::VerifiedStructured(program),
				stop_reason,
				evidence,
				rounding,
				report,
				allowances,
			})
		}
	}
}

#[expect(
	clippy::too_many_lines,
	reason = "Bound authority, exact adapter fallback, and terminal publication share one transaction"
)]
pub(super) fn run_bound(
	bound: BoundRegion,
	config: &OptimizationOptions,
	ledger: &BudgetLedger,
	beam: BeamOptions,
	mut report: BeamReport,
	#[cfg(feature = "workers")] worker: Option<WorkerContext<'_>>,
) -> CompilerResult<BeamRun> {
	let preparation_bytes = bound
		.retained_bytes()?
		.checked_mul(4)
		.and_then(|amount| amount.checked_add(1024 * 1024))
		.ok_or(Error::Budget("beam bound adapter storage"))?;
	let preparation_work = bound
		.instructions()
		.len()
		.checked_mul(8192)
		.ok_or(Error::Budget("beam bound adapter work"))?;
	let preparation = ledger.reserve(
		BudgetCategory::Verification,
		u64::try_from(preparation_work).map_err(|_| Error::Budget("beam bound adapter work"))?,
		u64::try_from(preparation_bytes)
			.map_err(|_| Error::Budget("beam bound adapter storage"))?,
	);
	if let Ok(preparation) = preparation {
		let adapter = bound_as_ideal(&bound)?;
		drop(preparation);
		if let Some(ideal) = adapter {
			let original_bindings = bound.binding_storage().clone();
			let original_source = bound.source_snapshot_id();
			let mut bound = bound;
			bound.retain_source(ideal.snapshot_id(), bound.binding_storage().clone())?;
			let mut result = run_region(
				Box::new(ideal),
				bound,
				config,
				ledger,
				beam,
				report,
				#[cfg(feature = "workers")]
				worker,
			)?;
			if let OptimizerInput::Region { mut bound, .. } = result.input {
				bound.retain_source(original_source, original_bindings)?;
				result.input = OptimizerInput::Bound(bound);
			}
			return Ok(result);
		}
	}
	let (original_score, published, mut rounding, mut allowances, original_status) =
		match score_candidate(&bound, config, ledger) {
			Ok(value) => value,
			Err(Error::Budget(_)) => {
				return Ok(BeamRun {
					input: OptimizerInput::Bound(bound),
					stop_reason: StopReason::WorkLimit,
					evidence: OptimizationEvidence::OriginalInput,
					rounding: RoundingStatus::Unchanged,
					report,
					allowances: Vec::new(),
				});
			}
			Err(error) => return Err(error.into()),
		};
	report.rounds = 1;
	report.generated = 1;
	let mut evidence = OptimizationEvidence::OriginalInput;
	let mut stop_reason = if original_status == TerminalStatus::Unscorable {
		StopReason::Unscorable
	} else {
		StopReason::Complete
	};
	let mut selected = published.map_or_else(
		|| bound.clone(),
		|published| {
			if published.snapshot_id() != bound.snapshot_id() {
				report.admitted = 1;
			}
			published
		},
	);
	let candidate_bytes = bound
		.retained_bytes()?
		.checked_mul(4)
		.and_then(|n| n.checked_add(1024 * 1024))
		.ok_or(Error::Budget("beam bound candidate storage"))?;
	let candidate_lease = match ledger.reserve(
		BudgetCategory::Candidate,
		0,
		u64::try_from(candidate_bytes)
			.map_err(|_| Error::Budget("beam bound candidate storage"))?,
	) {
		Ok(value) => Some(value),
		Err(Error::Budget(_)) => {
			stop_reason = StopReason::StorageLimit;
			None
		}
		Err(error) => return Err(error.into()),
	};
	if let Some(candidate_lease) = candidate_lease {
		let preparation = bound
			.instructions()
			.len()
			.checked_mul(8192)
			.ok_or(Error::Budget("beam bound adapter work"))?;
		let preparation =
			u64::try_from(preparation).map_err(|_| Error::Budget("beam bound adapter work"))?;
		if ledger
			.reserve(BudgetCategory::Verification, preparation, 0)
			.is_err()
		{
			stop_reason = StopReason::WorkLimit;
		} else if let Some(ideal) = bound_as_ideal(&bound)? {
			let (remaining, _) = ledger.remaining()?;
			let maximum = remaining.min(4_000_000);
			if maximum == 0 {
				stop_reason = StopReason::WorkLimit;
			} else {
				let work = ledger.reserve_work_allowance(maximum)?;
				let pass = ideal.optimize_exact_with_options(ExactOptions {
					max_work: usize::try_from(maximum).map_err(|_| Error::Budget("beam work"))?,
					max_bytes: 64 * 1024 * 1024,
				});
				report.generated = report
					.generated
					.checked_add(1)
					.ok_or(Error::Budget("beam candidates"))?;
				match pass {
					Ok((candidate, pass)) => {
						work.commit(
							u64::try_from(pass.work).map_err(|_| Error::Budget("beam work"))?,
						)?;
						if !pass.rewrites.is_empty() {
							let binding_work = candidate.binding_work_estimate()?;
							let allowance = ledger.reserve_work_allowance(binding_work)?;
							let mut rebound = candidate.bind(&[])?;
							allowance.commit(binding_work)?;
							rebound.retain_source(
								bound.source_snapshot_id(),
								bound.binding_storage().clone(),
							)?;
							let (
								cost,
								terminal,
								candidate_rounding,
								mut candidate_allowances,
								candidate_status,
							) = score_candidate(&rebound, config, ledger)?;
							if candidate_status == TerminalStatus::Unscorable {
								stop_reason = StopReason::Unscorable;
							}
							report.admitted = report
								.admitted
								.checked_add(1)
								.ok_or(Error::Budget("beam candidates"))?;
							report.maximum_frontier_operations = report
								.maximum_frontier_operations
								.max(rebound.instructions().len());
							match config.target().compare(&cost, &original_score)? {
								CostComparison::Better => {
									selected = terminal.unwrap_or(rebound);
									rounding = candidate_rounding;
									evidence = OptimizationEvidence::ExactVerified;
									allowances.clear();
									allowances.append(&mut candidate_allowances);
									allowances.push(candidate_lease);
								}
								CostComparison::Unscorable => {
									report.unscorable_comparisons = report
										.unscorable_comparisons
										.checked_add(1)
										.ok_or(Error::Budget("beam comparisons"))?;
									stop_reason = StopReason::Unscorable;
								}
								CostComparison::Equal | CostComparison::Worse => (),
							}
						}
					}
					Err(Error::Budget(_)) => stop_reason = StopReason::WorkLimit,
					Err(error) => return Err(error.into()),
				}
			}
		}
	}
	Ok(BeamRun {
		input: OptimizerInput::Bound(selected),
		stop_reason,
		evidence,
		rounding,
		report,
		allowances,
	})
}
