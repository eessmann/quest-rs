use criterion::Criterion;
use quest_polynomial::{Chebyshev, Limits, Polynomial};
use quest_qsp::{Complex64, FrozenCandidate, RealParityWx, SynthesisBuilder};
use std::hint::black_box;
#[expect(
    clippy::panic,
    reason = "A benchmark failure must abort measurement instead of timing an error path"
)]
fn require_success<T, E: std::fmt::Display>(result: Result<T, E>) -> T {
    match result {
        Ok(value) => value,
        Err(error) => {
            panic!("benchmark stage failed: {error}");
        }
    }
}
fn fixture() -> Result<Polynomial<Chebyshev>, Box<dyn std::error::Error>> {
    Ok(Polynomial::new(
        Chebyshev,
        vec![Complex64::new(0.0, 0.0), Complex64::new(0.6, 0.0)],
        Limits::default(),
    )?)
}
fn synthesize(
    target: &Polynomial<Chebyshev>,
) -> Result<FrozenCandidate<RealParityWx>, quest_qsp::Error> {
    SynthesisBuilder::new()
        .real_parity_wx(target)?
        .admit()?
        .complete()?
        .synthesize()
}
fn pipeline(criterion: &mut Criterion) -> Result<(), Box<dyn std::error::Error>> {
    let target = fixture()?;
    let candidate = synthesize(&target)?;
    candidate.response(0.3)?;
    criterion.bench_function("binary64/synthesis_degree1", |bench| {
        bench.iter(|| black_box(require_success(synthesize(black_box(&target)))));
    });
    criterion.bench_function("frozen/response_degree1", |bench| {
        bench.iter(|| black_box(require_success(candidate.response(black_box(0.3)))));
    });
    #[cfg(feature = "certification")]
    {
        use quest_qsp::certification::{CertificationBuilder, CertificationPolicy};
        CertificationBuilder::new()
            .candidate(candidate.clone())
            .policy(CertificationPolicy::default())?
            .certify()?;
        criterion.bench_function("cold/certification_degree1", |bench| {
            bench.iter(|| {
                black_box(require_success(
                    CertificationBuilder::new()
                        .candidate(black_box(candidate.clone()))
                        .policy(CertificationPolicy::default())
                        .and_then(CertificationBuilder::certify),
                ))
            });
        });
    }
    #[cfg(feature = "offline-synthesis")]
    {
        use quest_qsp::offline::{OfflineBuilder, OfflinePolicy};
        OfflineBuilder::new()
            .real_parity_wx(&target)?
            .policy(OfflinePolicy::default())?
            .solve()?;
        criterion.bench_function("cold/offline_including_certification_degree1", |bench| {
            bench.iter(|| {
                black_box(require_success(
                    OfflineBuilder::new()
                        .real_parity_wx(black_box(&target))
                        .and_then(|builder| builder.policy(OfflinePolicy::default()))
                        .and_then(quest_qsp::offline::OfflineBuilder::solve),
                ))
            });
        });
    }
    #[cfg(feature = "rayon")]
    parallel_pipeline(criterion)?;
    Ok(())
}
#[cfg(feature = "rayon")]
fn parallel_pipeline(criterion: &mut Criterion) -> Result<(), Box<dyn std::error::Error>> {
    use quest_numerics::ExecutionPolicy;
    let target = Polynomial::new(
        quest_polynomial::Laurent::new(0),
        (0_i32..257)
            .map(|k| {
                Complex64::new(
                    f64::from(k.rem_euclid(3).saturating_sub(1)) / 1024.0,
                    f64::from(k.rem_euclid(5).saturating_sub(2)) / 2048.0,
                )
            })
            .collect(),
        Limits::default(),
    )?;
    let admitted = SynthesisBuilder::new()
        .unit_circle_response(&target)?
        .admit()?;
    criterion.bench_function(
        "binary64/complete_synthesize_degree256/sequential",
        |bench| {
            bench.iter(|| {
                black_box(require_success(admitted.clone().complete().and_then(
                    quest_qsp::CompletedPolynomial::<quest_qsp::UnitCircleResponse>::synthesize,
                )))
            });
        },
    );
    for workers in [1, 2, 4] {
        let pool = rayon::ThreadPoolBuilder::new()
            .num_threads(workers)
            .build()?;
        let execution = ExecutionPolicy::Rayon(&pool);
        admitted
            .clone()
            .complete_with(execution)?
            .synthesize_with(execution)?;
        criterion.bench_function(
            &format!("binary64/complete_synthesize_degree256/workers{workers}"),
            |bench| {
                bench.iter(|| {
                    black_box(require_success(
                        admitted
                            .clone()
                            .complete_with(execution)
                            .and_then(|completed| completed.synthesize_with(execution)),
                    ))
                });
            },
        );
    }
    Ok(())
}
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut criterion = Criterion::default().configure_from_args();
    pipeline(&mut criterion)?;
    criterion.final_summary();
    drop(criterion);
    Ok(())
}
