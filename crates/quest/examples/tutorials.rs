//! Executable companion to docs/book. Tests include this file in an isolated process.
// ANCHOR: native_prelude
use quest::{Environment, QubitCount, RunInputs, circuit};
type TutorialResult<T> = Result<T, Box<dyn std::error::Error>>;
// ANCHOR_END: native_prelude

// ANCHOR: bell
/// # Errors
/// Reports compilation, resource, execution, or result validation failure.
pub fn bell(environment: &Environment) -> TutorialResult<(f64, f64)> {
    let program = circuit! {
        qubit[2] q;
        h q[0];
        cx q[0], q[1];
    }?;
    let mut prepared = environment.prepare((program).verify()?.lower()?.plan()?)?;
    let mut state = environment.state_vector(QubitCount::new(2)?)?;
    prepared.run(&mut state, &RunInputs::default())?;
    Ok((
        state.amplitude(0)?.norm_sqr(),
        state.amplitude(3)?.norm_sqr(),
    ))
}
// ANCHOR_END: bell

// ANCHOR: teleportation
/// # Errors
/// Reports compilation, resource, execution, or result validation failure.
pub fn teleportation(environment: &Environment) -> TutorialResult<f64> {
    let program = circuit! {
        gate entangle a, b {
            h a;
            cx a, b;
        }
        qubit[3] q;
        bit first;
        bit second;
        ry(0.7) q[0];
        entangle q[1], q[2];
        cx q[0], q[1];
        h q[0];
        first = measure q[0];
        second = measure q[1];
        if (bool(second)) { x q[2]; }
        if (bool(first)) { z q[2]; }
        reset q[0];
        reset q[1];
    }?;
    let mut prepared = environment.prepare((program).verify()?.lower()?.plan()?)?;
    let mut state = environment.state_vector(QubitCount::new(3)?)?;
    prepared.run(&mut state, &RunInputs::default())?;
    let overlap = std::ops::Add::add(
        std::ops::Mul::mul(state.amplitude(0)?, 0.35_f64.cos()),
        std::ops::Mul::mul(state.amplitude(4)?, 0.35_f64.sin()),
    );
    Ok(overlap.norm_sqr())
}
// ANCHOR_END: teleportation

// ANCHOR: feedback
/// # Errors
/// Reports compilation, resource, execution, or result validation failure.
pub fn feedback(environment: &Environment) -> TutorialResult<f64> {
    let program = circuit! {
        qubit q;
        output bit observed;
        h q;
        observed = measure q;
        if (bool(observed)) { x q; }
    }?;
    let mut prepared = environment.prepare((program).verify()?.lower()?.plan()?)?;
    let mut state = environment.state_vector(QubitCount::new(1)?)?;
    let output = prepared.run(&mut state, &RunInputs::default())?;
    if !output.outputs.contains_key("observed") {
        return Err("measurement output missing".into());
    }
    Ok(state.amplitude(0)?.norm_sqr())
}
// ANCHOR_END: feedback

// ANCHOR: repeat_until_success
/// # Errors
/// Reports compilation, resource, execution, or result validation failure.
pub fn repeat_until_success(environment: &Environment) -> TutorialResult<(bool, i128)> {
    let program = circuit! {
        qubit q;
        output bool succeeded = false;
        output int attempts = 0;
        while (!succeeded && attempts < 2) {
            reset q;
            if (attempts == 0) { h q; } else { x q; }
            succeeded = bool(measure q);
            attempts += 1;
        }
    }?;
    let mut prepared = environment.prepare((program).verify()?.lower()?.plan()?)?;
    let mut state = environment.state_vector(QubitCount::new(1)?)?;
    let output = prepared.run(&mut state, &RunInputs::default())?;
    let succeeded = output
        .outputs
        .get("succeeded")
        .and_then(quest::ClassicalValue::as_scalar)
        .ok_or("success output missing")?
        .to_bool()?;
    Ok((succeeded, integer_output(&output, "attempts")?))
}
// ANCHOR_END: repeat_until_success

// ANCHOR: captures_once
/// # Errors
/// Reports compilation, resource, execution, or result validation failure.
pub fn captures_once(environment: &Environment) -> TutorialResult<(usize, i128)> {
    let mut captured = Vec::new();
    let program = circuit! {
        gate turn(theta) q { rz(theta) q; }
        qubit q;
        output int iterations = 0;
        h q;
        for int i in [0:3] {
            turn(${{ captured.push(()); 0.125 }}) q;
            iterations += 1;
        }
    }?;
    let mut prepared = environment.prepare((program).verify()?.lower()?.plan()?)?;
    let mut state = environment.state_vector(QubitCount::new(1)?)?;
    let result = prepared.run(&mut state, &RunInputs::default())?;
    Ok((captured.len(), integer_output(&result, "iterations")?))
}
// ANCHOR_END: captures_once

// ANCHOR: array_arguments
/// # Errors
/// Reports compilation, resource, execution, or result validation failure.
pub fn array_arguments(environment: &Environment) -> TutorialResult<i128> {
    let program = circuit! {
        qubit q;
        def increment(mutable array[int, 2] values) {
            values[0] += 1;
        }
        def total(readonly array[int, 2] values) -> int {
            return values[0] + values[1];
        }
        array[int, 2] values = {2, 3};
        increment(values);
        output int result = total(values);
    }?;
    let mut prepared = environment.prepare((program).verify()?.lower()?.plan()?)?;
    let mut state = environment.state_vector(QubitCount::new(1)?)?;
    let result = prepared.run(&mut state, &RunInputs::default())?;
    integer_output(&result, "result")
}
// ANCHOR_END: array_arguments

fn integer_output(output: &quest::RunOutput, name: &str) -> TutorialResult<i128> {
    Ok(output
        .outputs
        .get(name)
        .and_then(quest::ClassicalValue::as_scalar)
        .ok_or("integer output missing")?
        .to_i128()?)
}

// ANCHOR: certified_synthesis
#[cfg(feature = "workers")]
/// # Errors
/// Reports compilation, resource, execution, or result validation failure.
pub fn certified_synthesis(executable: &std::path::Path) -> TutorialResult<usize> {
    use quest::{
        certified::{AngleTarget, Axis, Limits, Target},
        optimizer::{Client, WorkerLimits},
    };
    let client = Client::new(executable, WorkerLimits::default())?;
    let target = Target {
        axis: Axis::Z,
        angle: AngleTarget::DyadicRadians {
            bits: 0.17_f64.to_bits(),
        },
    };
    let certificate = client.synthesize(&target, 1.0e-12, 2026, Limits::default())?;
    if certificate.target() != &target {
        return Err("certificate target changed".into());
    }
    Ok(certificate.candidate().operations.len())
}
// ANCHOR_END: certified_synthesis

// ANCHOR: structured_certified_loop
#[cfg(feature = "workers")]
/// # Errors
/// Reports compilation, worker, certificate, or bound validation failure.
pub fn structured_certified_loop(executable: &std::path::Path) -> TutorialResult<usize> {
    use quest::{
        certified::Limits,
        optimizer::{Client, WorkerLimits},
    };
    let mut captures = 0usize;
    let original = circuit! {
        qubit q;
        int completed = 0;
        while (completed < int(${2.0_f64})) {
            rz(${{ captures = captures.saturating_add(1); 0.17_f64 }}) q;
            completed += 1;
        }
    }?
    .verify()?;
    let client = Client::new(executable, WorkerLimits::default())?;
    let epsilon = 1e-12_f64;
    let (candidate, report) =
        original.synthesize_rotations(&client, epsilon, 2026, Limits::default())?;
    let expected = quest::certified::dyadic_from_bits(epsilon.to_bits(), Limits::default())?;
    let expected = std::ops::Mul::mul(expected, quest::certified::RBig::from(2));
    if captures != 1 || report.rotations.len() != 1 || report.operator_error_bound != Some(expected)
    {
        return Err("once-evaluated capture or proven two-iteration bound changed".into());
    }
    // The executable SSA changed; export still reads the frozen original syntax.
    let _plan = candidate.lower()?.plan()?;
    Ok(captures)
}
// ANCHOR_END: structured_certified_loop

#[cfg(not(test))]
fn main() -> TutorialResult<()> {
    {
        let environment = Environment::builder().build()?;
        println!("Bell probabilities: {:?}", bell(&environment)?);
        println!("Teleportation fidelity: {}", teleportation(&environment)?);
        println!("Feedback P(0): {}", feedback(&environment)?);
        println!("Repeat result: {:?}", repeat_until_success(&environment)?);
        println!(
            "Captured values / iterations: {:?}",
            captures_once(&environment)?
        );
        println!("Array result: {}", array_arguments(&environment)?);
    }
    if let Some(path) = std::env::args_os().nth(1) {
        #[cfg(feature = "workers")]
        println!(
            "Certified synthesis gates: {}",
            certified_synthesis(std::path::Path::new(&path))?
        );
        #[cfg(not(feature = "workers"))]
        {
            let _ = path;
            return Err("worker path requires --features workers".into());
        }
    }
    Ok(())
}
