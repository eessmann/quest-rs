//! Complete solver comparisons; build identical source against each checkout.
use quest_numerics::arithmetic::{Backend, ExactConstant, MpBackend, MpIntervalBackend, Precision};
use quest_polynomial::{
    Accuracy, DynamicShape, ExactDomain, Laurent, Limits, MpHouseholder, Polynomial, RemezOptions,
    RemezRequest,
};
use quest_qsp::{
    Complex64,
    offline::{OfflineBuilder, OfflinePolicy},
};
use std::hint::black_box;
mod allocator;

fn remez(bits: usize) -> Result<(), Box<dyn std::error::Error>> {
    let precision = Precision {
        bits,
        ..Precision::default()
    };
    let gap = if bits == 128 { "1e-20" } else { "1e-35" };
    let width = if bits == 128 { "1e-25" } else { "1e-45" };
    let report = RemezRequest::new(
        quest_polynomial::function!(|x| x.exp()),
        ExactDomain::binary64(-1.0, 1.0),
        DynamicShape(4),
        MpBackend::new(precision)?,
        MpIntervalBackend::new(precision)?,
        MpHouseholder,
    )
    .options(RemezOptions {
        accuracy: Accuracy::Both {
            uniform: ExactConstant::Decimal("0.006".into()),
            gap: ExactConstant::Decimal(gap.into()),
        },
        root_width: ExactConstant::Decimal(width.into()),
        ..RemezOptions::default()
    })
    .run()?;
    let mut backend = MpBackend::new(precision)?;
    assert!(
        report.uniform_error().unconditional_bound().upper()
            <= &backend.constant(&ExactConstant::Decimal("0.006".into()))?
    );
    assert!(
        report.minimax_gap().gap().upper()
            <= &backend.constant(&ExactConstant::Decimal(gap.into()))?
    );
    assert_eq!(report.polynomial().coefficients().len(), 4);
    black_box(report);
    Ok(())
}

fn qsp(bits: u32) -> Result<(), Box<dyn std::error::Error>> {
    let coefficients = (0_i32..17)
        .map(|k| {
            Complex64::new(
                f64::from(k.rem_euclid(3) - 1) / 128.0,
                f64::from(k.rem_euclid(5) - 2) / 256.0,
            )
        })
        .collect::<Vec<_>>();
    let target = Polynomial::new(Laurent::new(0), coefficients, Limits::default())?;
    let mut policy = OfflinePolicy {
        initial_precision: bits,
        max_precision: bits,
        ..OfflinePolicy::default()
    };
    policy.certification.initial_precision = bits;
    policy.certification.max_precision = bits;
    let solved = OfflineBuilder::new()
        .unit_circle_response(&target)?
        .policy(policy)?
        .solve()?;
    assert!(solved.certified().report().response().upper_f64() <= 1e-11);
    assert!(solved.certified().report().reconstruction().upper_f64() <= 1e-11);
    assert_eq!(solved.report().attempts().len(), 1);
    assert_eq!(
        solved.certified().candidate().target(),
        target.coefficients()
    );
    black_box(solved);
    Ok(())
}

fn contractor(bits: usize) -> Result<(), Box<dyn std::error::Error>> {
    use quest_numerics::{
        arithmetic::{EnclosureBackend, First},
        roots::{Premise, newton},
    };
    let mut b = MpIntervalBackend::new(Precision {
        bits,
        ..Precision::default()
    })?;
    let left = b.point(-2.0)?;
    let right = b.point(2.0)?;
    let x = b.hull(&left, &right)?;
    let center = b.point(0.0)?;
    let result = newton(
        &mut b,
        x,
        center,
        |b, x| {
            let square = b.mul(x.clone(), x.clone())?;
            let one = b.point(1.0)?;
            let two = b.point(2.0)?;
            Ok(First {
                value: b.sub(square, one)?,
                first: b.mul(two, x.clone())?,
            })
        },
        Premise::EnclosesContinuouslyDifferentiableFunction,
    )?;
    assert_eq!(result.images.len(), 2);
    let lower = b.lower_endpoint(&result.images[0])?;
    let upper = b.upper_endpoint(&result.images[1])?;
    let mut p = MpBackend::new(Precision {
        bits,
        ..Precision::default()
    })?;
    assert_eq!(lower, p.point(-2.0)?);
    assert_eq!(upper, p.point(2.0)?);
    black_box(result);
    Ok(())
}
fn exact_synthesis(bits: usize) -> Result<(), Box<dyn std::error::Error>> {
    use quest_math::{Gate, Operation, Sequence, reconstruct};
    let options = quest_synthesis::SynthesisOptions {
        limits: quest_math::Limits {
            precision_bits: bits,
            ..Default::default()
        },
        seed: 1234,
        ..Default::default()
    };
    let word = Sequence {
        qubits: 1,
        operations: [
            Gate::H,
            Gate::T,
            Gate::H,
            Gate::T,
            Gate::Sdg,
            Gate::H,
            Gate::T,
            Gate::W,
        ]
        .into_iter()
        .map(|gate| Operation {
            gate,
            targets: if gate == Gate::W { vec![] } else { vec![0] },
            controls: vec![],
        })
        .collect(),
    };
    let target = reconstruct(&word, options.limits)?;
    let result = quest_synthesis::synthesize_matrix(&target, options.clone())?;
    assert_eq!(reconstruct(result.sequence(), options.limits)?, target);
    black_box(result);
    Ok(())
}
fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("workload,precision,iterations,ns_per_iteration,allocations,peak_live_extra_bytes");
    for bits in [128, 256] {
        for (name, run, count) in [
            (
                "remez_exp_degree3",
                remez as fn(usize) -> Result<(), Box<dyn std::error::Error>>,
                5,
            ),
            ("offline_qsp_degree16", |bits| qsp(bits as u32), 5),
            ("newton_split", contractor, 500),
            ("exact_synthesis", exact_synthesis, 25),
        ] {
            run(bits)?;
            let (ns, allocations, peak) = allocator::sample(count, || run(black_box(bits)))?;
            println!(
                "{name},{bits},{count},{:.3},{allocations},{peak}",
                ns as f64 / count as f64
            );
        }
    }
    Ok(())
}
