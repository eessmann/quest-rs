//! Bounded `QuiZX` 0.3.0 candidate adapter with independent full-phase certification.
use quest_math::{Gate, Limits, Operation, Sequence};
use quizx::{
    circuit::Circuit,
    extract::ToCircuit,
    gate::{GType, Gate as ZxGate},
    graph::GraphLike,
    vec_graph::Graph,
};
use std::collections::BTreeSet;

const MAX_INPUT_GATES: usize = 128;
const MAX_OUTPUT_GATES: usize = 1_000;
const MAX_GRAPH_VERTICES: usize = 4_096;

/// Generate a bounded extracted Clifford+T candidate and recover its exact scalar phase.
///
/// Flow simplification is deterministic; the protocol seed is accepted but no stochastic pass runs.
/// # Errors
/// Rejects malformed interfaces, unsupported controls, engine/extraction failures and failed exact certificates.
pub fn optimize(sequence: &Sequence, _seed: u64) -> Result<Sequence, String> {
    admit(sequence)?;
    let generated = std::panic::catch_unwind(|| generate(sequence))
        .map_err(|_| "QuiZX candidate engine panicked".to_string())??;
    let recovered = quest_math::recover_eighth_root_phase(
        &generated,
        sequence,
        Limits {
            gates: MAX_OUTPUT_GATES,
            ..Limits::default()
        },
    )
    .map_err(|error| format!("extracted candidate failed full-phase certification: {error}"))?;
    Ok(recovered.sequence().clone())
}
fn admit(sequence: &Sequence) -> Result<(), String> {
    if sequence.qubits > 4 {
        return Err("ZX capability is limited to four qubits".into());
    }
    if sequence.operations.len() > MAX_INPUT_GATES {
        return Err("ZX input exceeds 128 gates".into());
    }
    for operation in &sequence.operations {
        if operation.targets.len() != operation.gate.target_count() {
            return Err("gate target arity mismatch".into());
        }
        let mut seen = BTreeSet::new();
        for qubit in operation
            .targets
            .iter()
            .copied()
            .chain(operation.controls.iter().map(|control| control.qubit))
        {
            if qubit >= sequence.qubits || !seen.insert(qubit) {
                return Err("gate interface has an invalid or repeated qubit".into());
            }
        }
    }
    Ok(())
}
fn generate(sequence: &Sequence) -> Result<Sequence, String> {
    let mut circuit = Circuit::new(sequence.qubits);
    let mut scalar = 0usize;
    for operation in &sequence.operations {
        if operation.gate == Gate::W && operation.controls.is_empty() {
            scalar = scalar.saturating_add(1) % 8;
            continue;
        }
        for control in &operation.controls {
            if !control.positive {
                circuit.push(ZxGate::new(GType::NOT, vec![control.qubit]));
            }
        }
        translate(&mut circuit, operation)?;
        for control in operation.controls.iter().rev() {
            if !control.positive {
                circuit.push(ZxGate::new(GType::NOT, vec![control.qubit]));
            }
        }
    }
    let mut graph: Graph = circuit.to_graph();
    if graph.num_vertices() > MAX_GRAPH_VERTICES {
        return Err("ZX graph vertex budget exceeded".into());
    }
    quizx::simplify::flow_simp(&mut graph);
    if graph.inputs().len() != sequence.qubits || graph.outputs().len() != sequence.qubits {
        return Err("ZX simplification changed the quantum interface".into());
    }
    // Default extraction includes the final wire permutation; up_to_perm is intentionally not used.
    let extracted = graph
        .to_circuit_mut()
        .map_err(|error| format!("QuiZX extraction failed: {error}"))?;
    if extracted.num_qubits() != sequence.qubits {
        return Err("ZX extraction changed the qubit count".into());
    }
    let mut output = Sequence {
        qubits: sequence.qubits,
        operations: Vec::new(),
    };
    for gate in &extracted.gates {
        from_gate(&mut output, gate)?;
    }
    for _ in 0..scalar {
        push(&mut output, Gate::W, &[])?;
    }
    admit_candidate(&output)?;
    Ok(output)
}
fn translate(circuit: &mut Circuit, operation: &Operation) -> Result<(), String> {
    let controls = operation
        .controls
        .iter()
        .map(|control| control.qubit)
        .collect::<Vec<_>>();
    let targets = &operation.targets;
    let kind = match (operation.gate, controls.len()) {
        (Gate::H, 0) => GType::HAD,
        (Gate::X, 0) => GType::NOT,
        (Gate::Z, 0) => GType::Z,
        (Gate::S, 0) => GType::S,
        (Gate::Sdg, 0) => GType::Sdg,
        (Gate::T, 0) | (Gate::W, 1) => GType::T,
        (Gate::Tdg, 0) => GType::Tdg,
        (Gate::Cx, 0) | (Gate::X, 1) => GType::CNOT,
        (Gate::Cz, 0) | (Gate::Z, 1) => GType::CZ,
        (Gate::Swap, 0) => {
            let [first, second] = targets.as_slice() else {
                return Err("swap arity mismatch".into());
            };
            // QuiZX 0.3.0 tracks SWAP only in an internal wire map and omits a
            // trailing permutation from graph outputs. Explicit CNOTs preserve it.
            for qubits in [
                vec![*first, *second],
                vec![*second, *first],
                vec![*first, *second],
            ] {
                circuit.push(ZxGate::new(GType::CNOT, qubits));
            }
            return Ok(());
        }
        (Gate::X, 2) | (Gate::Cx, 1) => GType::TOFF,
        (Gate::Z, 2) | (Gate::Cz, 1) => GType::CCZ,
        (Gate::Y, 0) => {
            for kind in [GType::Sdg, GType::NOT, GType::S] {
                circuit.push(ZxGate::new(kind, targets.clone()));
            }
            return Ok(());
        }
        _ => return Err("ZX capability does not exactly support this controlled gate".into()),
    };
    let qubits = controls
        .into_iter()
        .chain(targets.iter().copied())
        .collect();
    circuit.push(ZxGate::new(kind, qubits));
    Ok(())
}
fn from_gate(output: &mut Sequence, gate: &ZxGate) -> Result<(), String> {
    if gate.vars != quizx::params::Parity::default() {
        return Err("ZX extraction produced classical parameters".into());
    }
    let kind = match gate.t {
        GType::HAD => Gate::H,
        GType::NOT => Gate::X,
        GType::Z => Gate::Z,
        GType::S => Gate::S,
        GType::Sdg => Gate::Sdg,
        GType::T => Gate::T,
        GType::Tdg => Gate::Tdg,
        GType::CNOT => Gate::Cx,
        GType::CZ => Gate::Cz,
        GType::SWAP => Gate::Swap,
        GType::ZPhase | GType::XPhase => {
            if gate.qs.len() != 1 {
                return Err("ZX phase gate has invalid arity".into());
            }
            let phase = gate.phase.to_rational();
            let numerator = phase
                .numer()
                .checked_mul(4)
                .ok_or_else(|| "ZX phase numerator overflow".to_string())?;
            let denominator = *phase.denom();
            if denominator <= 0 || numerator.checked_rem(denominator) != Some(0) {
                return Err("ZX extracted phase is outside Clifford+T".into());
            }
            let power = numerator
                .checked_div(denominator)
                .ok_or_else(|| "ZX phase division failed".to_string())?
                .rem_euclid(8);
            if gate.t == GType::XPhase {
                push(output, Gate::H, &gate.qs)?;
            }
            let gates: &[Gate] = match power {
                0 => &[],
                1 => &[Gate::T],
                2 => &[Gate::S],
                3 => &[Gate::S, Gate::T],
                4 => &[Gate::Z],
                5 => &[Gate::Z, Gate::T],
                6 => &[Gate::Sdg],
                7 => &[Gate::Tdg],
                _ => return Err("invalid normalized ZX phase".into()),
            };
            for gate_type in gates {
                push(output, *gate_type, &gate.qs)?;
            }
            if gate.t == GType::XPhase {
                push(output, Gate::H, &gate.qs)?;
            }
            return Ok(());
        }
        _ => return Err("ZX extraction produced an unsupported gate or ancilla operation".into()),
    };
    push(output, kind, &gate.qs)
}
fn push(output: &mut Sequence, gate: Gate, targets: &[usize]) -> Result<(), String> {
    if output.operations.len() >= MAX_OUTPUT_GATES.saturating_sub(7) {
        return Err("ZX output gate budget exceeded".into());
    }
    output.operations.push(Operation {
        gate,
        targets: targets.to_vec(),
        controls: Vec::new(),
    });
    Ok(())
}
fn admit_candidate(sequence: &Sequence) -> Result<(), String> {
    if sequence.qubits > 4 {
        return Err("ZX candidate added qubits".into());
    }
    if sequence.operations.len() > MAX_OUTPUT_GATES {
        return Err("ZX candidate output budget exceeded".into());
    }
    for gate in &sequence.operations {
        if gate.targets.len() != gate.gate.target_count()
            || gate.targets.iter().any(|qubit| *qubit >= sequence.qubits)
        {
            return Err("ZX candidate gate has invalid interface".into());
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use googletest::prelude::*;
    use quest_math::{Control, Gate, Limits, Operation, Sequence};
    fn op(gate: Gate, targets: &[usize]) -> Operation {
        Operation {
            gate,
            targets: targets.to_vec(),
            controls: Vec::new(),
        }
    }
    #[gtest]
    fn real_engine_cancels_gates_and_preserves_idle_interface() -> Result<()> {
        let original = Sequence {
            qubits: 3,
            operations: vec![
                op(Gate::H, &[0]),
                op(Gate::H, &[0]),
                op(Gate::T, &[2]),
                op(Gate::Tdg, &[2]),
            ],
        };
        let candidate = optimize(&original, 7).map_err(std::io::Error::other)?;
        verify_eq!(candidate.qubits, 3)?;
        verify_that!(candidate.operations.len(), lt(original.operations.len()))?;
        quest_math::verify_exact(&candidate, &original, Limits::default())?;
        Ok(())
    }
    #[gtest]
    fn extracted_scalar_phase_is_recovered_with_candidate_target_order() -> Result<()> {
        let original = Sequence {
            qubits: 1,
            operations: vec![
                op(Gate::X, &[0]),
                op(Gate::Z, &[0]),
                op(Gate::X, &[0]),
                op(Gate::Z, &[0]),
                op(Gate::W, &[]),
            ],
        };
        let mut missing_phase = generate(&original).map_err(std::io::Error::other)?;
        // A negative certificate fixture derived from a real extraction: deliberately omit W.
        missing_phase
            .operations
            .retain(|operation| operation.gate != Gate::W);
        verify_that!(
            quest_math::verify_exact(&missing_phase, &original, Limits::default()),
            err(anything())
        )?;
        let phase_recovery =
            quest_math::recover_eighth_root_phase(&missing_phase, &original, Limits::default())?;
        verify_eq!(phase_recovery.phase_power(), 1)?;
        let candidate = optimize(&original, 0).map_err(std::io::Error::other)?;
        let recovery =
            quest_math::recover_eighth_root_phase(&candidate, &original, Limits::default())?;
        verify_eq!(recovery.certificate().target(), &original)?;
        verify_eq!(recovery.certificate().candidate(), recovery.sequence())?;
        quest_math::verify_exact(&candidate, &original, Limits::default())?;
        verify_that!(
            quest_math::verify_exact(
                &candidate,
                &Sequence {
                    qubits: 1,
                    operations: Vec::new()
                },
                Limits::default()
            ),
            err(anything())
        )?;
        Ok(())
    }
    #[gtest]
    fn exactly_supported_signed_controls_survive_extraction() -> Result<()> {
        for positive in [true, false] {
            let original = Sequence {
                qubits: 2,
                operations: vec![Operation {
                    gate: Gate::X,
                    targets: vec![1],
                    controls: vec![Control { qubit: 0, positive }],
                }],
            };
            let candidate = optimize(&original, 4).map_err(std::io::Error::other)?;
            quest_math::verify_exact(&candidate, &original, Limits::default())?;
        }
        Ok(())
    }
    #[gtest]
    fn unsupported_controls_and_malformed_interfaces_return_errors() -> Result<()> {
        let unsupported = Sequence {
            qubits: 2,
            operations: vec![Operation {
                gate: Gate::T,
                targets: vec![1],
                controls: vec![Control {
                    qubit: 0,
                    positive: true,
                }],
            }],
        };
        verify_that!(optimize(&unsupported, 0), err(anything()))?;
        for sequence in [
            Sequence {
                qubits: 5,
                operations: Vec::new(),
            },
            Sequence {
                qubits: 1,
                operations: vec![op(Gate::X, &[1])],
            },
            Sequence {
                qubits: 1,
                operations: vec![op(Gate::X, &[0]); 129],
            },
            Sequence {
                qubits: 2,
                operations: vec![op(Gate::Cx, &[0, 0])],
            },
        ] {
            verify_that!(optimize(&sequence, 0), err(anything()))?;
        }
        Ok(())
    }
    #[gtest]
    fn complete_uncontrolled_gate_set_and_exact_control_extensions_certify() -> Result<()> {
        for gate in [
            Gate::H,
            Gate::X,
            Gate::Y,
            Gate::Z,
            Gate::S,
            Gate::Sdg,
            Gate::T,
            Gate::Tdg,
            Gate::Cx,
            Gate::Cz,
            Gate::Swap,
            Gate::W,
        ] {
            let targets = (0..gate.target_count()).collect::<Vec<_>>();
            let original = Sequence {
                qubits: 3,
                operations: vec![op(gate, &targets)],
            };
            let candidate = optimize(&original, 9)
                .map_err(|error| std::io::Error::other(format!("{gate:?}: {error}")))?;
            quest_math::verify_exact(&candidate, &original, Limits::default())?;
        }
        for (gate, targets, control_wires) in [
            (Gate::X, vec![2], vec![0, 1]),
            (Gate::Z, vec![2], vec![0, 1]),
            (Gate::Cx, vec![1, 2], vec![0]),
            (Gate::Cz, vec![1, 2], vec![0]),
            (Gate::W, vec![], vec![0]),
        ] {
            for positive in [true, false] {
                let original = Sequence {
                    qubits: 3,
                    operations: vec![Operation {
                        gate,
                        targets: targets.clone(),
                        controls: control_wires
                            .iter()
                            .map(|qubit| Control {
                                qubit: *qubit,
                                positive,
                            })
                            .collect(),
                    }],
                };
                let candidate = optimize(&original, 9)
                    .map_err(|error| std::io::Error::other(format!("{gate:?}: {error}")))?;
                quest_math::verify_exact(&candidate, &original, Limits::default())?;
            }
        }
        Ok(())
    }
    #[gtest]
    fn zero_qubit_scalar_and_deterministic_seed_interface_are_preserved() -> Result<()> {
        let original = Sequence {
            qubits: 0,
            operations: vec![op(Gate::W, &[]); 3],
        };
        let candidate = optimize(&original, 1).map_err(std::io::Error::other)?;
        quest_math::verify_exact(&candidate, &original, Limits::default())?;
        verify_eq!(
            &candidate,
            &optimize(&original, 99).map_err(std::io::Error::other)?
        )?;
        Ok(())
    }
    #[gtest]
    fn extracted_ancilla_and_changed_interfaces_are_not_admitted() -> Result<()> {
        let mut candidate = Sequence {
            qubits: 1,
            operations: Vec::new(),
        };
        verify_that!(
            from_gate(&mut candidate, &ZxGate::new(GType::InitAncilla, vec![1])),
            err(anything())
        )?;
        let original = Sequence {
            qubits: 2,
            operations: Vec::new(),
        };
        verify_that!(
            quest_math::recover_eighth_root_phase(&candidate, &original, Limits::default()),
            err(anything())
        )?;
        Ok(())
    }
}
