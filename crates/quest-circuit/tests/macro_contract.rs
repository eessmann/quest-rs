#![cfg(feature = "macros")]
use googletest::{Result, prelude::*};
use quest_circuit::*;

#[gtest]
fn bell_macro_uses_the_same_builder_semantics() -> Result<()> {
    let p = legacy_circuit! {qubit[2] q; bit[2] c; h q[0]; cx q[0],q[1]; c[0] = measure q[0]; c[1] = measure q[1];}?;
    let plan = p.bind(&[])?.lower()?.plan()?;
    expect_eq!(plan.num_qubits(), 2);
    expect_eq!(plan.num_bits(), 2);
    expect_eq!(plan.instructions().len(), 4);
    if let Operation::Gate {
        controls, targets, ..
    } = plan.instructions()[1].operation()
    {
        expect_eq!(controls[0].qubit().index(), 0);
        expect_eq!(targets[0].index(), 1);
    } else {
        fail!("expected controlled X")?;
    }
    Ok(())
}

#[gtest]
fn interpolation_evaluates_once_in_source_order_and_modifiers_keep_phase() -> Result<()> {
    let mut visits = vec![];
    let __quest_builder = 0.125;
    let p = legacy_circuit! {
        qubit[3] q;
        rx(${ { visits.push(1); __quest_builder } }) q[0];
        ry(${ { visits.push(2); 0.25 } }) q[1];
        negctrl @ inv @ rz(2*pi) q[2],q[1];
        ctrl @ gphase(pi / 2) q[2];
        U(pi/3, pi/5, -pi/7) q[0];
        barrier q[0],q[1];
        reset q[0];
    }?;
    expect_eq!(visits, &[1, 2]);
    expect_eq!(p.bind(&[])?.lower()?.plan()?.instructions().len(), 7);
    Ok(())
}

#[gtest]
fn nonfinite_interpolation_is_rejected_at_shared_admission() {
    let p = legacy_circuit! {qubit q; rx(${f64::NAN}) q;};
    expect_true!(p.is_err());
}

#[gtest]
fn full_macro_gate_inventory_matches_builder() -> Result<()> {
    let macro_program = legacy_circuit! {
        qubit[3] q;
        id q[0]; x q[0]; y q[0]; z q[0]; h q[0];
        s q[0]; sdg q[0]; t q[0]; tdg q[0]; sx q[0]; inv @ sx q[0];
        rx(pi/3) q[0]; ry(-pi/5) q[0]; rz(2*pi) q[0]; p(pi/7) q[0];
        cx q[1],q[0]; cy q[1],q[0]; cz q[1],q[0];
        swap q[2],q[0]; ccx q[2],q[1],q[0]; U(pi/3,pi/5,pi/7) q[0];
    }?;
    let mut builder = ProgramBuilder::new(3, 0)?;
    let q = builder.qubit(0)?;
    let c = builder.qubit(1)?;
    let t = builder.qubit(2)?;
    for gate in [
        Gate::Id,
        Gate::X,
        Gate::Y,
        Gate::Z,
        Gate::H,
        Gate::S,
        Gate::Sdg,
        Gate::T,
        Gate::Tdg,
        Gate::Sx,
        Gate::Sxdg,
        Gate::Rx(Angle::pi(1, 3)?),
        Gate::Ry(Angle::pi(-1, 5)?),
        Gate::Rz(Angle::pi(2, 1)?),
        Gate::Phase(Angle::pi(1, 7)?),
    ] {
        builder.gate(gate, &[q], &[])?;
    }
    for gate in [Gate::X, Gate::Y, Gate::Z] {
        builder.gate(gate, &[q], &[Control::new(c, ControlState::One)])?;
    }
    builder.gate(Gate::Swap, &[t, q], &[])?;
    builder.gate(
        Gate::X,
        &[q],
        &[
            Control::new(t, ControlState::One),
            Control::new(c, ControlState::One),
        ],
    )?;
    builder.gate(
        Gate::U {
            theta: Angle::pi(1, 3)?,
            phi: Angle::pi(1, 5)?,
            lambda: Angle::pi(1, 7)?,
        },
        &[q],
        &[],
    )?;
    let a = macro_program.bind(&[])?.lower()?.plan()?;
    let b = builder.finish()?.bind(&[])?.lower()?.plan()?;
    expect_eq!(a.instructions().len(), b.instructions().len());
    for (a, b) in a.instructions().iter().zip(b.instructions()) {
        match (a.operation(), b.operation()) {
            (
                Operation::Gate {
                    gate: ga,
                    targets: ta,
                    controls: ca,
                },
                Operation::Gate {
                    gate: gb,
                    targets: tb,
                    controls: cb,
                },
            ) => {
                expect_eq!(ga, gb);
                expect_eq!(
                    ta.iter().map(|x| x.index()).collect::<Vec<_>>(),
                    tb.iter().map(|x| x.index()).collect::<Vec<_>>()
                );
                expect_eq!(
                    ca.iter()
                        .map(|x| (x.qubit().index(), x.state()))
                        .collect::<Vec<_>>(),
                    cb.iter()
                        .map(|x| (x.qubit().index(), x.state()))
                        .collect::<Vec<_>>()
                );
                let residual = ga
                    .matrix(MatrixPolicy::default())?
                    .unitarity_residual(MatrixPolicy::default())?;
                expect_lt!(residual, 1e-14);
            }
            _ => fail!("expected gate")?,
        }
    }
    Ok(())
}

#[gtest]
fn macro_operations_retain_original_file_and_keyword_byte_ranges() -> Result<()> {
    let mut visits = vec![];
    let program = legacy_circuit! {
        qubit q;
        bit c;
        rx(${ { visits.push(1); 0.125 } }) q;
        gphase(${ { visits.push(2); 0.25 } });
        c = measure q;
        reset q;
        barrier;
        measure q -> c;
    }?;
    expect_eq!(visits, &[1, 2]);
    let plan = program.bind(&[])?.lower()?.plan()?;
    let mut previous_end = 0;
    let mut source = None;
    for (instruction, keyword) in plan
        .instructions()
        .iter()
        .zip(["rx", "gphase", "measure", "reset", "barrier", "measure"])
    {
        verify_true!(instruction.source().is_some())?;
        let span = instruction.source().unwrap();
        expect_true!(span.source().ends_with("macro_contract.rs"));
        expect_false!(span.source().is_empty());
        let range = span.range();
        expect_eq!(range.len(), keyword.len());
        expect_ge!(range.start, previous_end);
        previous_end = range.end;
        if let Some(previous) = source {
            expect_eq!(span.source(), previous);
        }
        source = Some(span.source());
    }
    Ok(())
}
