//! Matched QSP stage and end-to-end observations across immutable source variants.
mod allocator;
use quest_polynomial::{Laurent, Limits, Polynomial};
use quest_qsp::{
    Complex64, SynthesisBuilder,
    offline::{OfflineBuilder, OfflinePolicy},
};
use serde_json::{Value, json};
use std::hint::black_box;
type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;
fn target(degree: i32) -> Result<Polynomial<Laurent>> {
    let denominator = f64::from(degree) * 4.0;
    Ok(Polynomial::new(
        Laurent::new(0),
        (0..=degree)
            .map(|k| {
                Complex64::new(
                    f64::from(k.rem_euclid(3) - 1) / denominator,
                    f64::from(k.rem_euclid(5) - 2) / (2.0 * denominator),
                )
            })
            .collect(),
        Limits::default(),
    )?)
}
fn fingerprint(candidate: &quest_qsp::FrozenCandidate<quest_qsp::UnitCircleResponse>) -> u64 {
    candidate
        .controls()
        .iter()
        .flatten()
        .flatten()
        .flat_map(|v| [v.re.to_bits(), v.im.to_bits()])
        .fold(0xcbf29ce484222325, |state, bits| {
            state.wrapping_mul(0x100000001b3) ^ bits
        })
}
fn check(candidate: &quest_qsp::FrozenCandidate<quest_qsp::UnitCircleResponse>) -> Result<()> {
    for theta in [-2.4, -0.3, 0.0, 1.7] {
        let signal = Complex64::from_polar(1.0, theta);
        let expected = candidate
            .target()
            .iter()
            .rev()
            .fold(Complex64::new(0.0, 0.0), |v, c| v * signal + c);
        let [[actual, _], _] = candidate.evaluate(signal)?;
        if (actual - expected).norm() > 2e-10 {
            return Err("response mismatch".into());
        }
    }
    Ok(())
}
fn binary64(degree: i32) -> Result<Value> {
    let polynomial = target(degree)?;
    let admitted = SynthesisBuilder::new()
        .unit_circle_response(&polynomial)?
        .admit()?;
    let frozen = admitted.clone().complete()?.synthesize()?;
    check(&frozen)?;
    let expected = fingerprint(&frozen);
    let iterations = if degree == 256 { 12 } else { 4 };
    let (completion_ns, completion_allocations, completion_peak) =
        allocator::sample(iterations, || {
            black_box(admitted.clone().complete()?);
            Ok(())
        })?;
    let (ns, allocations, peak) = allocator::sample(iterations, || {
        let c = admitted.clone().complete()?.synthesize()?;
        check(&c)?;
        if fingerprint(&c) != expected {
            return Err("nondeterministic export".into());
        }
        black_box(c);
        Ok(())
    })?;
    Ok(
        json!({"kind":"binary64","degree":degree,"bits":53,"iterations":iterations,
        "nanoseconds":ns,"allocations":allocations,"peak_extra_live_bytes":peak,
        "completion_nanoseconds":completion_ns,"completion_allocations":completion_allocations,
        "completion_peak_extra_live_bytes":completion_peak,
        "completion_work_units":quest_qsp::benchmark_completion_work(),
        "synthesis_work_units":quest_qsp::benchmark_synthesis_work(),
        "work_units":quest_qsp::benchmark_completion_work().checked_add(quest_qsp::benchmark_synthesis_work()).ok_or("work count overflow")?,
        "grid":frozen.completion_grid(),"completion_residual":frozen.completion_residual(),
        "reconstruction_residual":frozen.reconstruction_residual(),"export_fingerprint":format!("{expected:016x}")}),
    )
}
fn offline(degree: i32, bits: u32) -> Result<Value> {
    let polynomial = target(degree)?;
    let mut policy = OfflinePolicy {
        initial_precision: bits,
        max_precision: bits,
        ..OfflinePolicy::default()
    };
    policy.certification.initial_precision = bits;
    policy.certification.max_precision = bits;
    let run = || {
        OfflineBuilder::new()
            .unit_circle_response(&polynomial)?
            .policy(policy)?
            .solve()
    };
    let warm = run()?;
    check(warm.certified().candidate())?;
    let expected = fingerprint(warm.certified().candidate());
    let mut completion_ns = 0_u64;
    let mut work = 0;
    let mut completion_work = 0;
    let mut completion_allocations = 0;
    let mut completion_peak = 0;
    let (ns, allocations, peak) = allocator::sample(1, || {
        let solved = run()?;
        if solved.report().attempts().len() != 1 {
            return Err("unexpected precision retries".into());
        }
        if solved.certified().report().response().upper_f64() > 1e-11
            || solved.certified().report().reconstruction().upper_f64() > 1e-11
        {
            return Err("certificate failed".into());
        }
        check(solved.certified().candidate())?;
        if fingerprint(solved.certified().candidate()) != expected {
            return Err("nondeterministic offline export".into());
        }
        (completion_ns, completion_work) = quest_qsp::offline::benchmark_completion_observation();
        (completion_allocations, completion_peak) = allocator::phase_observation();
        work = solved
            .report()
            .attempts()
            .iter()
            .map(|a| a.work_units())
            .sum::<usize>();
        black_box(solved);
        Ok(())
    })?;
    Ok(
        json!({"kind":"offline","degree":degree,"bits":bits,"iterations":1,
        "nanoseconds":ns,"allocations":allocations,"peak_extra_live_bytes":peak,
        "completion_nanoseconds":completion_ns,"completion_work_units":completion_work,
        "completion_allocations":completion_allocations,"completion_peak_extra_live_bytes":completion_peak,
        "work_units":work,"grid":warm.certified().candidate().completion_grid(),
        "response_bound":warm.certified().report().response().upper_f64(),
        "reconstruction_bound":warm.certified().report().reconstruction().upper_f64(),
        "export_fingerprint":format!("{expected:016x}")}),
    )
}
fn main() {
    quest_qsp::offline::benchmark_register_completion_hooks(
        allocator::phase_begin,
        allocator::phase_end,
    );
    let mut failed = false;
    for (kind, degree, bits) in [
        ("binary64", 256, 53),
        ("binary64", 1024, 53),
        ("offline", 16, 128),
        ("offline", 16, 256),
        ("offline", 256, 128),
        ("offline", 256, 256),
    ] {
        let result = if kind == "binary64" {
            binary64(degree)
        } else {
            offline(degree, bits)
        };
        match result {
            Ok(mut value) => {
                value["status"] = json!("ok");
                println!("{value}");
            }
            Err(e) => {
                failed = true;
                println!(
                    "{}",
                    json!({"kind":kind,"degree":degree,"bits":bits,"status":"error","error":e.to_string()})
                );
            }
        }
    }
    if failed {
        std::process::exit(1);
    }
}
