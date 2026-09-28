use crate::{
    CatalogCommand, Context, Error, FamilySelection, Result, Stage, SynthesisArgs, SynthesisMode,
    read_qsp,
};
use quest_polynomial::{Chebyshev, Laurent, Polynomial};
use quest_qsp::{
    AdmittedTarget, Canonical, CompletedPolynomial, FrozenCandidate, Generalized, Policy,
    SynthesisBuilder,
};
use quest_qsvt_io::{CatalogFamily, IoPolicy, QspInput};
use serde_json::{Value, json};
use std::time::Instant;

enum Candidate {
    Canonical(FrozenCandidate<Canonical>),
    Generalized(FrozenCandidate<Generalized>),
}
impl Candidate {
    fn payload(&self) -> QspInput {
        match self {
            Self::Canonical(value) => QspInput::Symmetric(value.phase_sequence()),
            Self::Generalized(value) => QspInput::GeneralizedMatrices(value.control_sequence()),
        }
    }
    fn report(&self) -> Result<Value> {
        fn diagnostics<M>(value: &FrozenCandidate<M>) -> Value {
            json!({"completion_residual":value.completion_residual(),
                "reconstruction_residual":value.reconstruction_residual(),
                "completion_grid":value.completion_grid()})
        }
        let (degree, mut report) = match self {
            Self::Canonical(value) => (value.phase_sequence().degree(), diagnostics(value)),
            Self::Generalized(value) => (value.control_sequence().degree(), diagnostics(value)),
        };
        crate::set(&mut report, "degree", json!(degree))?;
        Ok(report)
    }
}
fn policy(tolerance: f64) -> Result<Policy> {
    if !tolerance.is_finite() || tolerance <= 0.0 {
        return Err(Error::Input("positive finite tolerance required"));
    }
    Ok(Policy {
        response_tolerance: tolerance,
        ..Policy::default()
    })
}
fn complete<M>(
    admitted: AdmittedTarget<M>,
    context: &mut Context<'_>,
) -> Result<CompletedPolynomial<M>> {
    let execution = context.execution;
    context.measure("completion", Stage::Construction, || {
        Ok(admitted.complete_with(execution)?)
    })
}
fn canonical(
    target: &Polynomial<Chebyshev>,
    tolerance: f64,
    context: &mut Context<'_>,
) -> Result<Candidate> {
    let policy = policy(tolerance)?;
    let admitted = context.measure("admission", Stage::Construction, || {
        Ok(SynthesisBuilder::new()
            .policy(policy)
            .canonical(target)?
            .admit()?)
    })?;
    let completed = complete(admitted, context)?;
    let execution = context.execution;
    context.measure("synthesis", Stage::Synthesis, || {
        Ok(Candidate::Canonical(completed.synthesize_with(execution)?))
    })
}
fn generalized(
    target: &Polynomial<Laurent>,
    tolerance: f64,
    context: &mut Context<'_>,
) -> Result<Candidate> {
    let policy = policy(tolerance)?;
    let admitted = context.measure("admission", Stage::Construction, || {
        Ok(SynthesisBuilder::new()
            .policy(policy)
            .generalized(target)?
            .admit()?)
    })?;
    let completed = complete(admitted, context)?;
    let execution = context.execution;
    context.measure("synthesis", Stage::Synthesis, || {
        Ok(Candidate::Generalized(
            completed.synthesize_with(execution)?,
        ))
    })
}
#[cfg(feature = "certification")]
fn certificate_policy(tolerance: f64) -> quest_qsp::certification::CertificationPolicy {
    quest_qsp::certification::CertificationPolicy {
        response_tolerance: tolerance,
        completion_tolerance: tolerance,
        conversion_tolerance: tolerance,
        reconstruction_tolerance: tolerance,
        unitarity_tolerance: tolerance,
        ..quest_qsp::certification::CertificationPolicy::default()
    }
}
#[cfg(feature = "certification")]
fn certificate_report(report: &quest_qsp::certification::CertificationReport) -> Value {
    fn bound(value: &quest_qsp::certification::Bound) -> Value {
        json!({"lower":value.lower_f64(), "upper":value.upper_f64()})
    }
    json!({"response":bound(report.response()), "completion":bound(report.completion()),
        "conversion":bound(report.conversion()), "reconstruction":bound(report.reconstruction()),
        "unitarity":bound(report.unitarity()), "entries":report.entries().iter().map(bound).collect::<Vec<_>>(),
        "attempts":report.attempts().iter().map(|a| json!({"precision":a.precision(),
            "work_units":a.work_units(), "modeled_peak_bytes":a.modeled_peak_bytes(),
            "elapsed_seconds":a.elapsed().as_secs_f64()})).collect::<Vec<_>>()})
}
fn certify(
    candidate: Candidate,
    tolerance: f64,
    context: &mut Context<'_>,
) -> Result<(Candidate, Value)> {
    #[cfg(feature = "certification")]
    {
        use quest_qsp::certification::{CertificationBuilder, CertificationMode};
        fn check<M: CertificationMode>(
            value: FrozenCandidate<M>,
            tolerance: f64,
        ) -> Result<(FrozenCandidate<M>, Value)> {
            let certified = CertificationBuilder::new()
                .candidate(value)
                .policy(certificate_policy(tolerance))?
                .certify()?;
            let report = certificate_report(certified.report());
            Ok((certified.into_candidate(), report))
        }
        context.measure("certification", Stage::Certification, || match candidate {
            Candidate::Canonical(value) => {
                check(value, tolerance).map(|(c, r)| (Candidate::Canonical(c), r))
            }
            Candidate::Generalized(value) => {
                check(value, tolerance).map(|(c, r)| (Candidate::Generalized(c), r))
            }
        })
    }
    #[cfg(not(feature = "certification"))]
    {
        let _ = (candidate, tolerance, context);
        Err(Error::Feature("certification"))
    }
}
fn finish(
    candidate: Candidate,
    tolerance: f64,
    requested: bool,
    context: &mut Context<'_>,
) -> Result<(Candidate, Value)> {
    let (candidate, certificate) = if requested {
        certify(candidate, tolerance, context)?
    } else {
        (candidate, Value::Null)
    };
    let mut report = candidate.report()?;
    crate::set(&mut report, "construction", json!("binary64"))?;
    crate::set(&mut report, "certified", json!(requested))?;
    crate::set(&mut report, "certificate", certificate)?;
    Ok((candidate, report))
}
pub fn run(args: &SynthesisArgs, context: &mut Context<'_>, offline: bool) -> Result<Value> {
    let input = context.measure("read", Stage::Construction, || read_qsp(&args.input))?;
    let QspInput::Polynomial(input) = input else {
        return Err(Error::Input("synthesis requires a polynomial target"));
    };
    let mut conversion = 0.0;
    let (candidate, mut report) = match args.mode {
        SynthesisMode::Canonical => {
            let converted = context.measure("basis_conversion", Stage::Approximation, || {
                Ok(input.to_chebyshev()?)
            })?;
            conversion = converted.coefficient_error_bound;
            if offline {
                offline_canonical(&converted.polynomial, args.tolerance, context)?
            } else {
                let candidate = canonical(&converted.polynomial, args.tolerance, context)?;
                finish(candidate, args.tolerance, args.certify, context)?
            }
        }
        SynthesisMode::Generalized => {
            let target = input.laurent().ok_or(Error::Input(
                "generalized synthesis requires an explicit nonnegative Laurent target",
            ))?;
            if offline {
                offline_generalized(target, args.tolerance, context)?
            } else {
                let candidate = generalized(target, args.tolerance, context)?;
                finish(candidate, args.tolerance, args.certify, context)?
            }
        }
    };
    crate::set(
        &mut report,
        "input_basis_conversion_bound",
        json!(conversion),
    )?;
    crate::set(
        &mut report,
        "certificate_scope",
        json!(
            "frozen export against the supplied synthesis target; input basis conversion is reported separately"
        ),
    )?;
    context.measure("write", Stage::Construction, || {
        std::fs::write(
            &args.output,
            quest_qsvt_io::write_qsp_json(&candidate.payload())?,
        )?;
        Ok(())
    })?;
    Ok(report)
}
fn selected(selection: &FamilySelection) -> Result<Vec<&'static CatalogFamily>> {
    match (selection.kappa, selection.epsilon) {
        (None, None) => Ok(quest_qsvt_io::catalog_families().iter().collect()),
        (Some(kappa), Some(epsilon)) => Ok(vec![
            quest_qsvt_io::find_catalog_family(kappa, epsilon).ok_or(Error::Input(
                "no exact catalogue family matches kappa and epsilon",
            ))?,
        ]),
        _ => Err(Error::Input("kappa and epsilon must be supplied together")),
    }
}
fn family_report(family: &CatalogFamily) -> Value {
    json!({"kappa":family.kappa(), "epsilon_label":family.epsilon_label(), "degree":family.degree(),
        "reciprocal_scale":family.reciprocal_scale(), "source_revision":family.source_revision(),
        "label_is_certificate":false})
}
pub fn catalog(command: CatalogCommand, context: &mut Context<'_>) -> Result<Value> {
    match command {
        CatalogCommand::List(selection) => Ok(
            json!({"families":selected(&selection)?.into_iter().map(family_report).collect::<Vec<_>>() }),
        ),
        CatalogCommand::Check {
            family,
            certify: requested,
            tolerance,
        } => {
            policy(tolerance)?;
            check_families(&selected(&family)?, requested, tolerance, context)
        }
    }
}
struct FamilyJob {
    report: Value,
    timings: serde_json::Map<String, Value>,
    trace: quest_numerics::observer::TraceObserver,
}
fn family_job(
    family: &CatalogFamily,
    requested: bool,
    tolerance: f64,
    clock: &quest_numerics::observer::MonotonicClock,
) -> Result<FamilyJob> {
    let start = Instant::now();
    let mut trace = quest_numerics::observer::TraceObserver::new(128);
    let mut context = Context {
        timings: serde_json::Map::new(),
        trace: &mut trace,
        clock,
        output_rank: true,
        execution: quest_numerics::ExecutionPolicy::Sequential,
        #[cfg(feature = "rayon")]
        pool: None,
    };
    let mut report = family_report(family);
    let result = (|| {
        let polynomial = family.polynomial(IoPolicy::default())?;
        let candidate = canonical(&polynomial, tolerance, &mut context)?;
        finish(candidate, tolerance, requested, &mut context).map(|(_, report)| report)
    })();
    match result {
        Ok(value) => {
            crate::set(&mut report, "status", json!("passed"))?;
            crate::set(&mut report, "synthesis", value)?;
        }
        Err(error) => {
            crate::set(&mut report, "status", json!("failed"))?;
            crate::set(&mut report, "error", json!(error.to_string()))?;
        }
    }
    let timings = context.timings;
    crate::set(
        &mut report,
        "total_seconds",
        json!(start.elapsed().as_secs_f64()),
    )?;
    crate::set(
        &mut report,
        "timings_seconds",
        Value::Object(timings.clone()),
    )?;
    Ok(FamilyJob {
        report,
        timings,
        trace,
    })
}
fn check_families(
    families: &[&CatalogFamily],
    requested: bool,
    tolerance: f64,
    context: &mut Context<'_>,
) -> Result<Value> {
    use quest_numerics::observer::Observer;
    let clock = context.clock;
    let run = |family: &&CatalogFamily| family_job(family, requested, tolerance, clock);
    #[cfg(feature = "rayon")]
    let jobs: Vec<_> = context.pool.map_or_else(
        || families.iter().map(run).collect(),
        |pool| {
            use rayon::prelude::*;
            pool.install(|| families.par_iter().map(run).collect())
        },
    );
    #[cfg(not(feature = "rayon"))]
    let jobs: Vec<_> = families.iter().map(run).collect();
    let mut reports = Vec::new();
    let mut failed = 0_usize;
    for job in jobs {
        let job = job?;
        if job.report.get("status").and_then(Value::as_str) == Some("failed") {
            failed = failed.saturating_add(1);
        }
        for (name, value) in job.timings {
            let previous = context
                .timings
                .get(&name)
                .and_then(Value::as_f64)
                .unwrap_or(0.0);
            context
                .timings
                .insert(name, json!(previous + value.as_f64().unwrap_or(0.0)));
        }
        for event in job.trace.events() {
            context.trace.record(*event);
        }
        reports.push(job.report);
    }
    Ok(json!({"families":reports,"failed":failed,"stage_times_are_summed_worker_wall_times":true}))
}
#[cfg(feature = "offline-synthesis")]
fn offline_report<M>(solution: &quest_qsp::offline::OfflineSolution<M>) -> Value {
    json!({"construction":"offline-arbitrary-precision", "certified":true,
        "certificate":certificate_report(solution.certified().report()),
        "offline_attempts":solution.report().attempts().iter().map(|attempt|json!({
            "precision":attempt.precision(),"completion_grid":attempt.completion_grid(),"work_units":attempt.work_units(),
            "computation_seconds":attempt.computation_elapsed().as_secs_f64(),
            "certification_seconds":attempt.certification_elapsed().as_secs_f64()
        })).collect::<Vec<_>>()})
}
#[cfg(feature = "offline-synthesis")]
fn finish_offline<M>(
    solution: quest_qsp::offline::OfflineSolution<M>,
    wrap: impl FnOnce(FrozenCandidate<M>) -> Candidate,
) -> Result<(Candidate, Value)> {
    let mut report = offline_report(&solution);
    let candidate = wrap(solution.into_certified().into_candidate());
    crate::set(
        &mut report,
        "degree",
        candidate
            .report()?
            .get("degree")
            .cloned()
            .ok_or(Error::Input("candidate report degree"))?,
    )?;
    Ok((candidate, report))
}
fn offline_canonical(
    target: &Polynomial<Chebyshev>,
    tolerance: f64,
    context: &mut Context<'_>,
) -> Result<(Candidate, Value)> {
    #[cfg(feature = "offline-synthesis")]
    {
        context.measure(
            "offline_including_all_certification_attempts",
            Stage::Offline,
            || {
                let policy = quest_qsp::offline::OfflinePolicy {
                    certification: certificate_policy(tolerance),
                    ..quest_qsp::offline::OfflinePolicy::default()
                };
                let solution = quest_qsp::offline::OfflineBuilder::new()
                    .canonical(target)?
                    .policy(policy)?
                    .solve()?;
                finish_offline(solution, Candidate::Canonical)
            },
        )
    }
    #[cfg(not(feature = "offline-synthesis"))]
    {
        let _ = (target, tolerance);
        context.measure("offline_unavailable", Stage::Offline, || {
            Err(Error::Feature("offline-synthesis"))
        })
    }
}
fn offline_generalized(
    target: &Polynomial<Laurent>,
    tolerance: f64,
    context: &mut Context<'_>,
) -> Result<(Candidate, Value)> {
    #[cfg(feature = "offline-synthesis")]
    {
        context.measure(
            "offline_including_all_certification_attempts",
            Stage::Offline,
            || {
                let policy = quest_qsp::offline::OfflinePolicy {
                    certification: certificate_policy(tolerance),
                    ..quest_qsp::offline::OfflinePolicy::default()
                };
                let solution = quest_qsp::offline::OfflineBuilder::new()
                    .generalized(target)?
                    .policy(policy)?
                    .solve()?;
                finish_offline(solution, Candidate::Generalized)
            },
        )
    }
    #[cfg(not(feature = "offline-synthesis"))]
    {
        let _ = (target, tolerance);
        context.measure("offline_unavailable", Stage::Offline, || {
            Err(Error::Feature("offline-synthesis"))
        })
    }
}

#[cfg(all(test, feature = "rayon"))]
mod tests {
    use super::*;
    use googletest::prelude::*;
    #[gtest]
    fn catalogue_pool_preserves_family_order_and_failure_order() -> googletest::Result<()> {
        let families: Vec<_> = quest_qsvt_io::catalog_families()
            .iter()
            .take(3)
            .rev()
            .collect();
        let pool = rayon::ThreadPoolBuilder::new().num_threads(3).build()?;
        let clock = quest_numerics::observer::MonotonicClock::new();
        for tolerance in [1e-11, 1e-18] {
            let run = |pool| -> crate::Result<Value> {
                let mut trace = quest_numerics::observer::TraceObserver::new(128);
                let mut context = Context {
                    timings: serde_json::Map::new(),
                    trace: &mut trace,
                    clock: &clock,
                    output_rank: true,
                    execution: quest_numerics::ExecutionPolicy::Sequential,
                    pool,
                };
                let mut report = check_families(&families, false, tolerance, &mut context)?;
                if let Some(families) = report.get_mut("families").and_then(Value::as_array_mut) {
                    for family in families {
                        if let Some(object) = family.as_object_mut() {
                            object.remove("timings_seconds");
                            object.remove("total_seconds");
                        }
                    }
                }
                Ok(report)
            };
            expect_eq!(run(None)?, run(Some(&pool))?);
        }
        Ok(())
    }
}

/// Read or explicitly construct a frozen payload once during cold application setup.
#[cfg(feature = "native")]
pub fn freeze_input(
    args: &crate::TransformArgs,
    context: &mut Context<'_>,
) -> Result<(QspInput, Value)> {
    let input = context.measure("qsp_read", Stage::Construction, || read_qsp(&args.qsp))?;
    let QspInput::Polynomial(polynomial) = input else {
        if args.synthesize_input || args.certify_input {
            return Err(Error::Input("input synthesis requires a polynomial target"));
        }
        return Ok((
            input,
            json!({"construction":"imported-frozen", "certified":false}),
        ));
    };
    if !args.synthesize_input {
        return Err(Error::Input(
            "polynomial execution requires explicit --synthesize-input",
        ));
    }
    let (candidate, conversion) = if matches!(args.route, crate::TransformRoute::Standard) {
        let converted = context.measure("basis_conversion", Stage::Approximation, || {
            Ok(polynomial.to_chebyshev()?)
        })?;
        (
            canonical(&converted.polynomial, args.input_tolerance, context)?,
            converted.coefficient_error_bound,
        )
    } else {
        (
            generalized(
                polynomial.laurent().ok_or(Error::Input(
                    "generalized synthesis requires a nonnegative Laurent target",
                ))?,
                args.input_tolerance,
                context,
            )?,
            0.0,
        )
    };
    let (candidate, mut report) =
        finish(candidate, args.input_tolerance, args.certify_input, context)?;
    crate::set(
        &mut report,
        "input_basis_conversion_bound",
        json!(conversion),
    )?;
    crate::set(
        &mut report,
        "certificate_scope",
        json!(
            "frozen export against the synthesis target; input basis conversion is reported separately"
        ),
    )?;
    Ok((candidate.payload(), report))
}
