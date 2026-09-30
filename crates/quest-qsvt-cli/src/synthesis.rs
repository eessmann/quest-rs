use crate::{
    CatalogCommand, Context, Error, FamilySelection, Result, Stage, SynthesisArgs, SynthesisMode,
    read_qsp,
};
use quest_polynomial::{Chebyshev, Laurent, Polynomial};
use quest_qsp::{
    AdmittedTarget, CompletedPolynomial, FrozenCandidate, Policy, RealParityWx, SynthesisAlgorithm,
    SynthesisBuilder, UnitCircleResponse,
};
use quest_qsvt_io::{CatalogFamily, IoPolicy, QspInput};
use serde_json::{Value, json};
use std::time::Instant;

enum Candidate {
    RealParityWx(FrozenCandidate<RealParityWx>),
    UnitCircleResponse(FrozenCandidate<UnitCircleResponse>),
    #[cfg(feature = "certification")]
    CertifiedWx(quest_qsp::certification::Certified<RealParityWx>),
    #[cfg(feature = "certification")]
    CertifiedCircle(quest_qsp::certification::Certified<UnitCircleResponse>),
}
impl Candidate {
    fn payload(&self) -> QspInput {
        match self {
            #[cfg(feature = "certification")]
            Self::CertifiedWx(value) => QspInput::Symmetric(value.candidate().phase_sequence()),
            #[cfg(feature = "certification")]
            Self::CertifiedCircle(value) => {
                QspInput::GeneralizedMatrices(value.candidate().control_sequence())
            }
            Self::RealParityWx(value) => QspInput::Symmetric(value.phase_sequence()),
            Self::UnitCircleResponse(value) => {
                QspInput::GeneralizedMatrices(value.control_sequence())
            }
        }
    }
    fn export(&self, format: crate::ExportFormat) -> Result<Vec<u8>> {
        if format == crate::ExportFormat::Sequence {
            return Ok(quest_qsvt_io::write_qsp_json(&self.payload())?.into_bytes());
        }
        #[cfg(feature = "certification")]
        {
            use quest_qsp::artifact::{ArtifactLimits, export_certified, export_compiled};
            Ok(match self {
                Self::RealParityWx(c) => export_compiled(c, ArtifactLimits::default())?,
                Self::UnitCircleResponse(c) => export_compiled(c, ArtifactLimits::default())?,
                Self::CertifiedWx(c) => export_certified(c, ArtifactLimits::default())?,
                Self::CertifiedCircle(c) => export_certified(c, ArtifactLimits::default())?,
            })
        }
        #[cfg(not(feature = "certification"))]
        {
            Err(Error::Feature("certification (compiled artifacts)"))
        }
    }
    #[cfg(feature = "native")]
    fn into_execution(self) -> Result<QspInput> {
        #[cfg(feature = "certification")]
        {
            use quest_qsp::artifact::{ArtifactLimits, LoadedCertified};
            let certified = match self {
                Self::CertifiedWx(c) => LoadedCertified::RealParityWx(c),
                Self::CertifiedCircle(c) => LoadedCertified::UnitCircleResponse(c),
                other => return Ok(other.payload()),
            };
            Ok(QspInput::Compiled(
                quest_qsvt_io::CompiledInput::from_certified(certified, ArtifactLimits::default())?,
            ))
        }
        #[cfg(not(feature = "certification"))]
        {
            Ok(self.payload())
        }
    }
    fn report(&self) -> Result<Value> {
        fn diagnostics<M>(value: &FrozenCandidate<M>) -> Value {
            json!({"completion_residual":value.completion_residual(),
                "reconstruction_residual":value.reconstruction_residual(),
                "completion_grid":value.completion_grid()})
        }
        let (degree, mut report) = match self {
            #[cfg(feature = "certification")]
            Self::CertifiedWx(value) => (
                value.candidate().phase_sequence().degree(),
                diagnostics(value.candidate()),
            ),
            #[cfg(feature = "certification")]
            Self::CertifiedCircle(value) => (
                value.candidate().control_sequence().degree(),
                diagnostics(value.candidate()),
            ),
            Self::RealParityWx(value) => (value.phase_sequence().degree(), diagnostics(value)),
            Self::UnitCircleResponse(value) => {
                (value.control_sequence().degree(), diagnostics(value))
            }
        };
        crate::set(&mut report, "degree", json!(degree))?;
        Ok(report)
    }
}
fn policy(tolerance: f64, algorithm: SynthesisAlgorithm) -> Result<Policy> {
    if !tolerance.is_finite() || tolerance <= 0.0 {
        return Err(Error::Input("positive finite tolerance required"));
    }
    Ok(Policy {
        algorithm,
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
fn real_parity_wx(
    target: &Polynomial<Chebyshev>,
    tolerance: f64,
    algorithm: SynthesisAlgorithm,
    context: &mut Context<'_>,
) -> Result<Candidate> {
    let policy = policy(tolerance, algorithm)?;
    let admitted = context.measure("admission", Stage::Construction, || {
        Ok(SynthesisBuilder::new()
            .policy(policy)
            .real_parity_wx(target)?
            .admit()?)
    })?;
    let completed = complete(admitted, context)?;
    let execution = context.execution;
    context.measure("synthesis", Stage::Synthesis, || {
        Ok(Candidate::RealParityWx(
            completed.synthesize_with(execution)?,
        ))
    })
}
fn unit_circle_response(
    target: &Polynomial<Laurent>,
    tolerance: f64,
    algorithm: SynthesisAlgorithm,
    context: &mut Context<'_>,
) -> Result<Candidate> {
    let policy = policy(tolerance, algorithm)?;
    let admitted = context.measure("admission", Stage::Construction, || {
        Ok(SynthesisBuilder::new()
            .policy(policy)
            .unit_circle_response(target)?
            .admit()?)
    })?;
    let completed = complete(admitted, context)?;
    let execution = context.execution;
    context.measure("synthesis", Stage::Synthesis, || {
        Ok(Candidate::UnitCircleResponse(
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
        ) -> Result<(quest_qsp::certification::Certified<M>, Value)> {
            let certified = CertificationBuilder::new()
                .candidate(value)
                .policy(certificate_policy(tolerance))?
                .certify()?;
            let report = certificate_report(certified.report());
            Ok((certified, report))
        }
        context.measure("certification", Stage::Certification, || match candidate {
            Candidate::CertifiedWx(c) => {
                let r = certificate_report(c.report());
                Ok((Candidate::CertifiedWx(c), r))
            }
            Candidate::CertifiedCircle(c) => {
                let r = certificate_report(c.report());
                Ok((Candidate::CertifiedCircle(c), r))
            }
            Candidate::RealParityWx(value) => {
                check(value, tolerance).map(|(c, r)| (Candidate::CertifiedWx(c), r))
            }
            Candidate::UnitCircleResponse(value) => {
                check(value, tolerance).map(|(c, r)| (Candidate::CertifiedCircle(c), r))
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
    if args.certify && args.export == crate::ExportFormat::Sequence {
        return Err(Error::Input(
            "--certify requires --export compiled to retain evidence",
        ));
    }
    #[cfg(not(feature = "certification"))]
    if args.export == crate::ExportFormat::Compiled {
        return Err(Error::Feature("certification (compiled artifacts)"));
    }
    let input = context.measure("read", Stage::Construction, || read_qsp(&args.input))?;
    let QspInput::Polynomial(input) = input else {
        return Err(Error::Input("synthesis requires a polynomial target"));
    };
    let mut conversion = 0.0;
    let (candidate, mut report) = match args.mode {
        SynthesisMode::RealParityWx => {
            let converted = context.measure("basis_conversion", Stage::Approximation, || {
                Ok(input.to_chebyshev()?)
            })?;
            conversion = converted.coefficient_error_bound();
            if offline {
                offline_canonical(
                    converted.polynomial(),
                    args.tolerance,
                    args.algorithm.solver(),
                    context,
                )?
            } else {
                let candidate = real_parity_wx(
                    converted.polynomial(),
                    args.tolerance,
                    args.algorithm.solver(),
                    context,
                )?;
                finish(candidate, args.tolerance, args.certify, context)?
            }
        }
        SynthesisMode::UnitCircleResponse => {
            let target = input.laurent().ok_or(Error::Input(
                "generalized synthesis requires an explicit nonnegative Laurent target",
            ))?;
            if offline {
                offline_generalized(target, args.tolerance, args.algorithm.solver(), context)?
            } else {
                let candidate =
                    unit_circle_response(target, args.tolerance, args.algorithm.solver(), context)?;
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
        std::fs::write(&args.output, candidate.export(args.export)?)?;
        Ok(())
    })?;
    crate::set(&mut report, "algorithm", json!(args.algorithm.name()))?;
    crate::set(
        &mut report,
        "export",
        json!(if args.export == crate::ExportFormat::Compiled {
            "compiled"
        } else {
            "sequence"
        }),
    )?;
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
            policy(tolerance, SynthesisAlgorithm::default())?;
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
        let candidate = real_parity_wx(
            &polynomial,
            tolerance,
            SynthesisAlgorithm::default(),
            &mut context,
        )?;
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
    wrap: impl FnOnce(quest_qsp::certification::Certified<M>) -> Candidate,
) -> Result<(Candidate, Value)> {
    let mut report = offline_report(&solution);
    let candidate = wrap(solution.into_certified());
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
    algorithm: SynthesisAlgorithm,
    context: &mut Context<'_>,
) -> Result<(Candidate, Value)> {
    #[cfg(feature = "offline-synthesis")]
    {
        context.measure(
            "offline_including_all_certification_attempts",
            Stage::Offline,
            || {
                let policy = quest_qsp::offline::OfflinePolicy {
                    algorithm,
                    certification: certificate_policy(tolerance),
                    ..quest_qsp::offline::OfflinePolicy::default()
                };
                let solution = quest_qsp::offline::OfflineBuilder::new()
                    .real_parity_wx(target)?
                    .policy(policy)?
                    .solve()?;
                finish_offline(solution, Candidate::CertifiedWx)
            },
        )
    }
    #[cfg(not(feature = "offline-synthesis"))]
    {
        let _ = (target, tolerance, algorithm);
        context.measure("offline_unavailable", Stage::Offline, || {
            Err(Error::Feature("offline-synthesis"))
        })
    }
}
fn offline_generalized(
    target: &Polynomial<Laurent>,
    tolerance: f64,
    algorithm: SynthesisAlgorithm,
    context: &mut Context<'_>,
) -> Result<(Candidate, Value)> {
    #[cfg(feature = "offline-synthesis")]
    {
        context.measure(
            "offline_including_all_certification_attempts",
            Stage::Offline,
            || {
                let policy = quest_qsp::offline::OfflinePolicy {
                    algorithm,
                    certification: certificate_policy(tolerance),
                    ..quest_qsp::offline::OfflinePolicy::default()
                };
                let solution = quest_qsp::offline::OfflineBuilder::new()
                    .unit_circle_response(target)?
                    .policy(policy)?
                    .solve()?;
                finish_offline(solution, Candidate::CertifiedCircle)
            },
        )
    }
    #[cfg(not(feature = "offline-synthesis"))]
    {
        let _ = (target, tolerance, algorithm);
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
    let input = context.measure("qsp_read", Stage::Construction, || {
        crate::read_qsp_with_tolerance(&args.qsp, args.input_tolerance)
    })?;
    #[cfg(feature = "certification")]
    if let QspInput::Compiled(compiled) = input {
        if args.synthesize_input {
            return Err(Error::Input("compiled payload must not be resynthesized"));
        }
        let certificate = match compiled.certified() {
            quest_qsp::artifact::LoadedCertified::RealParityWx(c) => certificate_report(c.report()),
            quest_qsp::artifact::LoadedCertified::UnitCircleResponse(c) => {
                certificate_report(c.report())
            }
        };
        return Ok((
            QspInput::Compiled(compiled),
            json!({"construction":"compiled-recertified","certified":true,"certificate":certificate}),
        ));
    }
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
            real_parity_wx(
                converted.polynomial(),
                args.input_tolerance,
                args.algorithm.solver(),
                context,
            )?,
            converted.coefficient_error_bound(),
        )
    } else {
        (
            unit_circle_response(
                polynomial.laurent().ok_or(Error::Input(
                    "generalized synthesis requires a nonnegative Laurent target",
                ))?,
                args.input_tolerance,
                args.algorithm.solver(),
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
    Ok((candidate.into_execution()?, report))
}
