//! Explicit generalized catalog acceptance. Requires the certification feature.
//! Every original Chebyshev shard is converted with checked exact binary64 halves,
//! then synthesized through generalized controls and independently certified.
use quest_polynomial::{Laurent, Limits, Polynomial};
use quest_qsp::certification::{CertificationBuilder, CertificationPolicy};
use quest_qsp::{Complex64, SynthesisBuilder};
use std::{
    ops::{Add, Mul},
    time::Instant,
};
#[derive(Clone, Copy)]
struct Family {
    name: &'static str,
    bytes: &'static [u8],
}
const FAMILIES: &[Family] = &[
    Family {
        name: "coeffs_kappa_5_eps_0p001",
        bytes: include_bytes!("../data/inverse/coeffs_kappa_5_eps_0p001.bin"),
    },
    Family {
        name: "coeffs_kappa_5_eps_0p01",
        bytes: include_bytes!("../data/inverse/coeffs_kappa_5_eps_0p01.bin"),
    },
    Family {
        name: "coeffs_kappa_5_eps_0p1",
        bytes: include_bytes!("../data/inverse/coeffs_kappa_5_eps_0p1.bin"),
    },
    Family {
        name: "coeffs_kappa_50_eps_0p001",
        bytes: include_bytes!("../data/inverse/coeffs_kappa_50_eps_0p001.bin"),
    },
    Family {
        name: "coeffs_kappa_50_eps_0p01",
        bytes: include_bytes!("../data/inverse/coeffs_kappa_50_eps_0p01.bin"),
    },
    Family {
        name: "coeffs_kappa_50_eps_0p1",
        bytes: include_bytes!("../data/inverse/coeffs_kappa_50_eps_0p1.bin"),
    },
    Family {
        name: "coeffs_kappa_100_eps_0p001",
        bytes: include_bytes!("../data/inverse/coeffs_kappa_100_eps_0p001.bin"),
    },
    Family {
        name: "coeffs_kappa_100_eps_0p01",
        bytes: include_bytes!("../data/inverse/coeffs_kappa_100_eps_0p01.bin"),
    },
    Family {
        name: "coeffs_kappa_100_eps_0p1",
        bytes: include_bytes!("../data/inverse/coeffs_kappa_100_eps_0p1.bin"),
    },
    Family {
        name: "coeffs_kappa_250_eps_0p001",
        bytes: include_bytes!("../data/inverse/coeffs_kappa_250_eps_0p001.bin"),
    },
    Family {
        name: "coeffs_kappa_250_eps_0p01",
        bytes: include_bytes!("../data/inverse/coeffs_kappa_250_eps_0p01.bin"),
    },
    Family {
        name: "coeffs_kappa_250_eps_0p1",
        bytes: include_bytes!("../data/inverse/coeffs_kappa_250_eps_0p1.bin"),
    },
    Family {
        name: "coeffs_kappa_500_eps_0p001",
        bytes: include_bytes!("../data/inverse/coeffs_kappa_500_eps_0p001.bin"),
    },
    Family {
        name: "coeffs_kappa_500_eps_0p01",
        bytes: include_bytes!("../data/inverse/coeffs_kappa_500_eps_0p01.bin"),
    },
    Family {
        name: "coeffs_kappa_500_eps_0p1",
        bytes: include_bytes!("../data/inverse/coeffs_kappa_500_eps_0p1.bin"),
    },
    Family {
        name: "coeffs_kappa_1000_eps_0p001",
        bytes: include_bytes!("../data/inverse/coeffs_kappa_1000_eps_0p001.bin"),
    },
    Family {
        name: "coeffs_kappa_1000_eps_0p01",
        bytes: include_bytes!("../data/inverse/coeffs_kappa_1000_eps_0p01.bin"),
    },
    Family {
        name: "coeffs_kappa_1000_eps_0p1",
        bytes: include_bytes!("../data/inverse/coeffs_kappa_1000_eps_0p1.bin"),
    },
    Family {
        name: "coeffs_kappa_1500_eps_0p001",
        bytes: include_bytes!("../data/inverse/coeffs_kappa_1500_eps_0p001.bin"),
    },
    Family {
        name: "coeffs_kappa_1500_eps_0p01",
        bytes: include_bytes!("../data/inverse/coeffs_kappa_1500_eps_0p01.bin"),
    },
    Family {
        name: "coeffs_kappa_1500_eps_0p1",
        bytes: include_bytes!("../data/inverse/coeffs_kappa_1500_eps_0p1.bin"),
    },
];
#[derive(Debug, thiserror::Error)]
#[error("{stage}: {source}")]
struct Failure {
    stage: &'static str,
    source: Box<dyn std::error::Error>,
}
impl Failure {
    fn new(stage: &'static str, error: impl Into<Box<dyn std::error::Error>>) -> Self {
        Self {
            stage,
            source: error.into(),
        }
    }
}
fn target(family: Family, complex: bool) -> Result<Polynomial<Laurent>, Failure> {
    let (chunks, tail) = family.bytes.as_chunks::<8>();
    if !tail.is_empty() || chunks.is_empty() {
        return Err(Failure::new("source", "invalid binary64 shard"));
    }
    let source: Vec<_> = chunks
        .iter()
        .map(|bytes| f64::from_le_bytes(*bytes))
        .collect();
    let degree = source.iter().rposition(|value| *value != 0.0).unwrap_or(0);
    let count = degree
        .checked_add(1)
        .ok_or_else(|| Failure::new("source", "degree overflow"))?;
    let mut target = vec![Complex64::new(0.0, 0.0); count];
    for (index, value) in source.iter().enumerate() {
        if !value.is_finite() || (index % 2 != degree % 2 && *value != 0.0) {
            return Err(Failure::new(
                "source",
                "invalid original parity or coefficient",
            ));
        }
        if *value == 0.0 {
            continue;
        }
        let half = value * 0.5;
        if (half * 2.0).to_bits() != value.to_bits() {
            return Err(Failure::new("conversion", "inexact binary64 halving"));
        }
        let high = degree
            .checked_add(index)
            .ok_or_else(|| Failure::new("conversion", "support overflow"))?
            / 2;
        let low = degree
            .checked_sub(index)
            .ok_or_else(|| Failure::new("conversion", "support overflow"))?
            / 2;
        for position in [high, low] {
            let entry = target
                .get_mut(position)
                .ok_or_else(|| Failure::new("conversion", "support"))?;
            *entry = entry.add(Complex64::new(half, 0.0));
        }
    }
    if complex {
        // This is a separately defined binary64 complex target. It is not an
        // assertion that rounded multiplication preserves an exact global phase.
        for value in &mut target {
            *value = value.mul(Complex64::new(0.6, 0.8));
        }
    }
    Polynomial::new(Laurent::new(0), target, Limits::default())
        .map_err(|error| Failure::new("source", error))
}
fn run(family: Family, complex: bool) -> Result<(), Failure> {
    let total = Instant::now();
    let target = target(family, complex)?;
    let converted = total.elapsed().as_secs_f64();
    let started = Instant::now();
    let admitted = SynthesisBuilder::new()
        .unit_circle_response(&target)
        .and_then(SynthesisBuilder::admit)
        .map_err(|error| Failure::new("admission", error))?;
    let admission = started.elapsed().as_secs_f64();
    let started = Instant::now();
    let completed = admitted
        .complete()
        .map_err(|error| Failure::new("completion", error))?;
    let grid = completed.completion_grid();
    let completion = started.elapsed().as_secs_f64();
    let started = Instant::now();
    let candidate = completed
        .synthesize()
        .map_err(|error| Failure::new("synthesis", error))?;
    let synthesis = started.elapsed().as_secs_f64();
    let bytes: Vec<_> = candidate
        .controls()
        .iter()
        .flatten()
        .flatten()
        .flat_map(|value| [value.re.to_bits(), value.im.to_bits()])
        .collect();
    let started = Instant::now();
    let certified = CertificationBuilder::new()
        .candidate(candidate)
        .policy(CertificationPolicy::default())
        .and_then(CertificationBuilder::certify)
        .map_err(|error| Failure::new("certification", error))?;
    let certification = started.elapsed().as_secs_f64();
    if !bytes.iter().copied().eq(certified
        .candidate()
        .controls()
        .iter()
        .flatten()
        .flatten()
        .flat_map(|value| [value.re.to_bits(), value.im.to_bits()]))
    {
        return Err(Failure::new(
            "immutability",
            "verifier changed exported bits",
        ));
    }
    let report = certified.report();
    let mode = if complex {
        "complex_rotation_binary64"
    } else {
        "real_canonical_family"
    };
    println!(
        "{{\"family\":\"{}\",\"mode\":\"{mode}\",\"degree\":{},\"status\":\"certified\",\"completion_grid\":{grid},\"response_upper\":{:.17e},\"completion_upper\":{:.17e},\"conversion_upper\":{:.17e},\"reconstruction_upper\":{:.17e},\"unitarity_upper\":{:.17e},\"certification_attempts\":{},\"conversion_seconds\":{converted},\"admission_seconds\":{admission},\"completion_seconds\":{completion},\"synthesis_seconds\":{synthesis},\"certification_seconds\":{certification},\"total_seconds\":{}}}",
        family.name,
        target
            .degree()
            .map_or_else(|| "null".to_owned(), |n| n.to_string()),
        report.response().upper_f64(),
        report.completion().upper_f64(),
        report.conversion().upper_f64(),
        report.reconstruction().upper_f64(),
        report.unitarity().upper_f64(),
        report.attempts().len(),
        total.elapsed().as_secs_f64()
    );
    Ok(())
}
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut failures = 0_usize;
    for &family in FAMILIES {
        if let Err(error) = run(family, false) {
            eprintln!("{}: {error}", family.name);
            println!(
                "{{\"family\":\"{}\",\"mode\":\"real_canonical_family\",\"status\":\"failed\",\"stage\":\"{}\"}}",
                family.name, error.stage
            );
            failures = failures.saturating_add(1);
        }
    }
    let largest = FAMILIES
        .iter()
        .max_by_key(|family| family.bytes.len())
        .ok_or("empty catalog")?;
    if let Err(error) = run(*largest, true) {
        eprintln!("complex {}: {error}", largest.name);
        println!(
            "{{\"family\":\"{}\",\"mode\":\"complex_rotation_binary64\",\"status\":\"failed\",\"stage\":\"{}\"}}",
            largest.name, error.stage
        );
        failures = failures.saturating_add(1);
    }
    if failures > 0 {
        return Err(format!("{failures} generalized catalog cases failed without fallback").into());
    }
    Ok(())
}
