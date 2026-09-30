use googletest::{Result, prelude::*};
#[allow(unused_imports)]
use quest_circuit::prelude::*;
use quest_circuit::{language::GateKind, *};
use std::sync::Arc;

#[gtest]
fn checked_gate_adapter_uses_registry_arity_and_finite_parameters() -> Result<()> {
    for &kind in GateKind::ALL {
        let count = kind.definition().parameter_count;
        let parameters = vec![0.25; count];
        if kind == GateKind::GlobalPhase {
            expect_true!(BoundGate::from_kind(kind, &parameters).is_err());
        } else {
            let gate = BoundGate::from_kind(kind, &parameters)?;
            expect_eq!(
                gate.matrix(MatrixPolicy::default())?.num_qubits(),
                kind.definition().target_count
            );
        }
        expect_true!(BoundGate::from_kind(kind, &vec![0.25; count.saturating_add(1)]).is_err());
        if count != 0 {
            expect_true!(
                BoundGate::from_kind(kind, &parameters[..count.saturating_sub(1)]).is_err()
            );
            for invalid in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
                expect_true!(BoundGate::from_kind(kind, &vec![invalid; count]).is_err());
            }
        }
    }
    expect_eq!(BoundGate::from_kind(GateKind::Ccx, &[])?, BoundGate::X);
    expect_eq!(
        BoundGate::from_kind(GateKind::U, &[0.1, -0.2, 0.3])?,
        BoundGate::U {
            theta: 0.1,
            phi: -0.2,
            lambda: 0.3
        }
    );
    Ok(())
}

#[gtest]
fn bound_plan_checks_native_indices_without_an_empty_intermediate_stage() -> Result<()> {
    let count = usize::try_from(i32::MAX)?
        .checked_add(1)
        .ok_or_else(|| std::io::Error::other("index overflow"))?;
    let builder = QuantumRegionBuilder::with_limits(
        count,
        0,
        ProgramLimits {
            max_qubits: count,
            ..ProgramLimits::default()
        },
    )?;
    expect_true!(matches!(
        builder.finish()?.bind(&[])?.plan(),
        Err(Error::NativeIndex)
    ));
    let plan = QuantumRegionBuilder::new(1, 0)?
        .finish()?
        .bind(&[])?
        .plan()?;
    expect_eq!(plan.num_qubits(), 1);
    Ok(())
}

#[gtest]
fn bindings_share_frozen_operands_without_merging_occurrences() -> Result<()> {
    let mut builder = QuantumRegionBuilder::new(3, 0)?;
    let targets = [builder.qubit(2)?];
    let controls = [Control::new(builder.qubit(0)?, ControlState::Zero)];
    let first = builder.gate(Gate::H, &targets, &controls)?;
    let second = builder.gate(Gate::H, &targets, &controls)?;
    let program = builder.finish()?;
    let left = program.clone().bind(&[])?;
    let right = program.bind(&[])?;
    expect_ne!(first, second);
    let (
        Operation::Gate {
            targets: lt,
            controls: lc,
            ..
        },
        Operation::Gate {
            targets: rt,
            controls: rc,
            ..
        },
    ) = (
        left.instructions()[0].operation(),
        right.instructions()[0].operation(),
    )
    else {
        return fail!("expected two bound gates");
    };
    expect_true!(Arc::ptr_eq(lt, rt));
    expect_true!(Arc::ptr_eq(lc, rc));
    expect_eq!(
        left.instructions()[0]
            .operation()
            .qubits()
            .collect::<Vec<_>>(),
        vec![targets[0], controls[0].qubit()]
    );
    expect_ne!(
        left.instructions()[0].provenance(),
        left.instructions()[1].provenance()
    );
    Ok(())
}
