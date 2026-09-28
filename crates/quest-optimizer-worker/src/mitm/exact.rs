use super::representatives::{ParetoIndex, Signature};
use quest_math::{ExactMatrix, Gate, Limits, Operation, Sequence, verify_exact};
use quest_optimizer_protocol::{MitmLimits, Outcome};

struct State {
    matrix: ExactMatrix,
    parent: Option<usize>,
    via: Option<usize>,
    length: usize,
    signature: Signature,
    active: bool,
}
struct Admission {
    limits: MitmLimits,
    states: usize,
    bytes: usize,
    work: u64,
    state_bytes: usize,
    truncated: bool,
}
enum Stop {
    Incomplete(&'static str),
    Failure(String),
}
impl Admission {
    const fn new(limits: MitmLimits, width: usize) -> Result<Self, Stop> {
        let state_bytes = if width == 1 { 8 * 1024 } else { 32 * 1024 };
        // Two full-width target/query matrices and their temporary key can be
        // much larger than any 256-bit table entry.
        let scratch = 2 * 1024 * 1024;
        if limits.table_bytes < scratch {
            return Err(Stop::Incomplete("table scratch"));
        }
        Ok(Self {
            limits,
            states: 0,
            bytes: scratch,
            work: 0,
            state_bytes,
            truncated: false,
        })
    }
    fn charge(&mut self, work: u64) -> Result<(), Stop> {
        self.work = self
            .work
            .checked_add(work)
            .filter(|count| *count <= self.limits.max_work)
            .ok_or(Stop::Incomplete("work"))?;
        Ok(())
    }
    fn retain_state(&mut self, left: bool) -> Result<(), Stop> {
        let states = self
            .states
            .checked_add(1)
            .ok_or(Stop::Incomplete("states"))?;
        let bytes = self
            .bytes
            .checked_add(self.state_bytes)
            .ok_or(Stop::Incomplete("table bytes"))?;
        let state_cap = if left {
            self.limits.max_states / 2
        } else {
            self.limits.max_states
        };
        let byte_cap = if left {
            self.limits
                .table_bytes
                .saturating_div(2)
                .saturating_add(2 * 1024 * 1024)
        } else {
            self.limits.table_bytes
        };
        if states > state_cap || bytes > byte_cap {
            return Err(Stop::Incomplete(if states > state_cap {
                "states"
            } else {
                "table bytes"
            }));
        }
        self.states = states;
        self.bytes = bytes;
        Ok(())
    }
}

pub fn search_exact(target: &Sequence, limits: MitmLimits) -> Outcome {
    if let Err(error) = limits.validate(target.qubits) {
        return Outcome::Failure {
            code: "exact-mitm".into(),
            message: error.to_string(),
        };
    }
    let mut budget = match Admission::new(limits, target.qubits) {
        Ok(budget) => budget,
        Err(Stop::Incomplete(reason)) => {
            return Outcome::Incomplete {
                reason: reason.into(),
                explored: 0,
            };
        }
        Err(Stop::Failure(message)) => {
            return Outcome::Failure {
                code: "exact-mitm".into(),
                message,
            };
        }
    };
    match run(target, limits, &mut budget) {
        Ok(outcome) => outcome,
        Err(Stop::Incomplete(reason)) => Outcome::Incomplete {
            reason: reason.into(),
            explored: u64::try_from(budget.states).unwrap_or(u64::MAX),
        },
        Err(Stop::Failure(message)) => Outcome::Failure {
            code: "exact-mitm".into(),
            message,
        },
    }
}
#[allow(clippy::too_many_lines)] // Admission, both half traversals, and terminal status share one bounded search.
fn run(target: &Sequence, limits: MitmLimits, budget: &mut Admission) -> Result<Outcome, Stop> {
    limits
        .validate(target.qubits)
        .map_err(|error| Stop::Failure(error.to_string()))?;
    budget.charge(
        u64::try_from(target.operations.len())
            .unwrap_or(u64::MAX)
            .saturating_mul(64),
    )?;
    let target_limits = Limits::default();
    let target_matrix = quest_math::reconstruct(target, target_limits)
        .map_err(|error| Stop::Failure(error.to_string()))?;
    let table_limits = Limits {
        qubits: target.qubits,
        gates: 1,
        coefficient_bits: limits.coefficient_bits,
        bytes: limits.table_bytes,
        ..Limits::default()
    };
    let alphabet = alphabet(target.qubits);
    let mut gates = Vec::new();
    gates
        .try_reserve_exact(alphabet.len())
        .map_err(|_| Stop::Incomplete("gate allocation"))?;
    for operation in &alphabet {
        budget.charge(64)?;
        budget.retain_state(true)?;
        gates.push(
            ExactMatrix::for_operation(target.qubits, operation, table_limits).map_err(
                |error| {
                    Stop::Incomplete(if matches!(error, quest_math::Error::Budget { .. }) {
                        "coefficients"
                    } else {
                        "matrix resources"
                    })
                },
            )?,
        );
    }
    let left_depth = limits.max_depth / 2;
    let right_depth = limits
        .max_depth
        .checked_sub(left_depth)
        .ok_or(Stop::Incomplete("depth"))?;
    let identity = ExactMatrix::identity(target.qubits, table_limits)
        .map_err(|_| Stop::Incomplete("identity table"))?;
    let mut left = Vec::new();
    let mut index = ParetoIndex::new();
    budget.retain_state(true)?;
    left.push(State {
        matrix: identity.clone(),
        parent: None,
        via: None,
        length: 0,
        signature: Signature::identity(target.qubits),
        active: true,
    });
    index
        .commit(
            identity
                .full_phase_key(table_limits)
                .map_err(|_| Stop::Incomplete("identity key"))?,
            Signature::identity(target.qubits),
            0,
            |_| {},
        )
        .map_err(|()| Stop::Incomplete("index allocation"))?;
    let mut cursor = 0usize;
    'left_walk: while cursor < left.len() {
        let current = left
            .get(cursor)
            .ok_or_else(|| Stop::Failure("left cursor".into()))?;
        if !current.active {
            cursor = cursor
                .checked_add(1)
                .ok_or(Stop::Incomplete("left cursor"))?;
            continue;
        }
        if current.length < left_depth {
            for (operation_index, (gate, operation)) in gates.iter().zip(&alphabet).enumerate() {
                budget.charge(64)?;
                let current = left
                    .get(cursor)
                    .ok_or_else(|| Stop::Failure("left cursor".into()))?;
                let signature = current
                    .signature
                    .after(operation)
                    .ok_or(Stop::Incomplete("depth"))?;
                let matrix = gate
                    .multiply(&current.matrix, table_limits)
                    .map_err(|error| {
                        Stop::Incomplete(if matches!(error, quest_math::Error::Budget { .. }) {
                            "coefficients"
                        } else {
                            "matrix resources"
                        })
                    })?;
                let key = matrix
                    .full_phase_key(table_limits)
                    .map_err(|_| Stop::Incomplete("coefficients"))?;
                budget.charge(
                    u64::try_from(index.bucket_len(&key))
                        .unwrap_or(u64::MAX)
                        .saturating_mul(2),
                )?;
                if index.is_dominated(&key, &signature) {
                    continue;
                }
                if budget.retain_state(true).is_err() {
                    budget.truncated = true;
                    break 'left_walk;
                }
                let next = left.len();
                let length = current
                    .length
                    .checked_add(1)
                    .ok_or(Stop::Incomplete("depth"))?;
                left.try_reserve(1)
                    .map_err(|_| Stop::Incomplete("left allocation"))?;
                left.push(State {
                    matrix,
                    parent: Some(cursor),
                    via: Some(operation_index),
                    length,
                    signature,
                    active: true,
                });
                index
                    .commit(key, signature, next, |retired| {
                        if let Some(state) = left.get_mut(retired) {
                            state.active = false;
                        }
                    })
                    .map_err(|()| Stop::Incomplete("index allocation"))?;
            }
        }
        cursor = cursor
            .checked_add(1)
            .ok_or(Stop::Incomplete("left cursor"))?;
    }
    let mut right = Vec::new();
    let mut right_index = ParetoIndex::new();
    budget.retain_state(false)?;
    right.push(State {
        matrix: identity.clone(),
        parent: None,
        via: None,
        length: 0,
        signature: Signature::identity(target.qubits),
        active: true,
    });
    right_index
        .commit(
            identity
                .full_phase_key(table_limits)
                .map_err(|_| Stop::Incomplete("identity key"))?,
            Signature::identity(target.qubits),
            0,
            |_| {},
        )
        .map_err(|()| Stop::Incomplete("index allocation"))?;
    cursor = 0;
    'right_walk: while cursor < right.len() {
        if !right
            .get(cursor)
            .ok_or(Stop::Incomplete("right cursor"))?
            .active
        {
            cursor = cursor
                .checked_add(1)
                .ok_or(Stop::Incomplete("right cursor"))?;
            continue;
        }
        budget.charge(128)?;
        let current = right
            .get(cursor)
            .ok_or_else(|| Stop::Failure("right cursor".into()))?;
        let query = current
            .matrix
            .adjoint(target_limits)
            .and_then(|adjoint| adjoint.multiply(&target_matrix, target_limits))
            .map_err(|_| Stop::Incomplete("query coefficients"))?;
        let query_key = query
            .full_phase_key(target_limits)
            .map_err(|_| Stop::Incomplete("query coefficients"))?;
        if let Some(left_index) = index.first(&query_key) {
            let mut operations = reconstruct_path(&left, left_index, &alphabet)?;
            operations.extend(reconstruct_path(&right, cursor, &alphabet)?);
            let candidate = Sequence {
                qubits: target.qubits,
                operations,
            };
            match verify_exact(&candidate, target, target_limits) {
                Ok(_) => {
                    return Ok(Outcome::Candidate {
                        sequence: candidate,
                        engine: "mitm-exact-v1".into(),
                        precision_bits: 0,
                    });
                }
                Err(quest_math::Error::Budget { .. } | quest_math::Error::Resource(_)) => {
                    return Err(Stop::Incomplete("candidate verification"));
                }
                Err(error) => {
                    return Err(Stop::Failure(format!(
                        "matching full-phase key failed verification: {error}"
                    )));
                }
            }
        }
        if current.length < right_depth {
            for (operation_index, (gate, operation)) in gates.iter().zip(&alphabet).enumerate() {
                budget.charge(64)?;
                let current = right
                    .get(cursor)
                    .ok_or_else(|| Stop::Failure("right cursor".into()))?;
                let signature = current
                    .signature
                    .after(operation)
                    .ok_or(Stop::Incomplete("depth"))?;
                let matrix = gate
                    .multiply(&current.matrix, table_limits)
                    .map_err(|error| {
                        Stop::Incomplete(if matches!(error, quest_math::Error::Budget { .. }) {
                            "coefficients"
                        } else {
                            "matrix resources"
                        })
                    })?;
                let key = matrix
                    .full_phase_key(table_limits)
                    .map_err(|_| Stop::Incomplete("coefficients"))?;
                budget.charge(
                    u64::try_from(right_index.bucket_len(&key))
                        .unwrap_or(u64::MAX)
                        .saturating_mul(2),
                )?;
                if right_index.is_dominated(&key, &signature) {
                    continue;
                }
                if budget.retain_state(false).is_err() {
                    budget.truncated = true;
                    break 'right_walk;
                }
                let length = current
                    .length
                    .checked_add(1)
                    .ok_or(Stop::Incomplete("depth"))?;
                right
                    .try_reserve(1)
                    .map_err(|_| Stop::Incomplete("right allocation"))?;
                right.push(State {
                    matrix,
                    parent: Some(cursor),
                    via: Some(operation_index),
                    length,
                    signature,
                    active: true,
                });
                let next = right
                    .len()
                    .checked_sub(1)
                    .ok_or(Stop::Incomplete("right index"))?;
                right_index
                    .commit(key, signature, next, |retired| {
                        if let Some(state) = right.get_mut(retired) {
                            state.active = false;
                        }
                    })
                    .map_err(|()| Stop::Incomplete("index allocation"))?;
            }
        }
        cursor = cursor
            .checked_add(1)
            .ok_or(Stop::Incomplete("right cursor"))?;
    }
    if budget.truncated {
        Ok(Outcome::Incomplete {
            reason: "state enumeration".into(),
            explored: u64::try_from(budget.states).unwrap_or(u64::MAX),
        })
    } else {
        Ok(Outcome::NoCandidate {
            explored: u64::try_from(budget.states).unwrap_or(u64::MAX),
        })
    }
}

fn reconstruct_path(
    states: &[State],
    mut index: usize,
    alphabet: &[Operation],
) -> Result<Vec<Operation>, Stop> {
    let mut path = Vec::new();
    while let Some(operation_index) = states.get(index).and_then(|state| state.via) {
        path.try_reserve(1)
            .map_err(|_| Stop::Incomplete("predecessor allocation"))?;
        path.push(
            alphabet
                .get(operation_index)
                .ok_or_else(|| Stop::Failure("predecessor operation".into()))?
                .clone(),
        );
        index = states
            .get(index)
            .and_then(|state| state.parent)
            .ok_or_else(|| Stop::Failure("predecessor state".into()))?;
    }
    path.reverse();
    Ok(path)
}
pub(super) fn alphabet(width: usize) -> Vec<Operation> {
    let mut alphabet = Vec::new();
    for gate in [
        Gate::H,
        Gate::X,
        Gate::Y,
        Gate::Z,
        Gate::S,
        Gate::Sdg,
        Gate::T,
        Gate::Tdg,
    ] {
        for target in 0..width {
            alphabet.push(Operation {
                gate,
                targets: vec![target],
                controls: vec![],
            });
        }
    }
    alphabet.push(Operation {
        gate: Gate::W,
        targets: vec![],
        controls: vec![],
    });
    if width == 2 {
        for gate in [Gate::Cx, Gate::Cz, Gate::Swap] {
            alphabet.push(Operation {
                gate,
                targets: vec![0, 1],
                controls: vec![],
            });
            if gate == Gate::Cx {
                alphabet.push(Operation {
                    gate,
                    targets: vec![1, 0],
                    controls: vec![],
                });
            }
        }
    }
    alphabet
}

#[cfg(test)]
mod tests {
    use super::search_exact;
    use googletest::{Result, prelude::*};
    use quest_math::{Gate, Limits, Operation, Sequence, verify_exact};
    use quest_optimizer_protocol::{MitmLimits, Outcome};

    fn op(gate: Gate) -> Operation {
        Operation {
            gate,
            targets: if gate == Gate::W { vec![] } else { vec![0] },
            controls: vec![],
        }
    }

    #[gtest]
    fn exact_mitm_reconstructs_noncommuting_chronological_halves() -> Result<()> {
        let target = Sequence {
            qubits: 1,
            operations: vec![op(Gate::H), op(Gate::T)],
        };
        let mut limits = MitmLimits::for_qubits(1)?;
        limits.max_depth = 2;
        match search_exact(&target, limits) {
            Outcome::Candidate { sequence, .. } => {
                verify_exact(&sequence, &target, Limits::default())?;
            }
            other => {
                return fail!("expected exact candidate, got {other:?}");
            }
        }
        Ok(())
    }

    #[gtest]
    fn exact_mitm_preserves_scalar_phase_and_reports_capacity_cutoff() -> Result<()> {
        let target = Sequence {
            qubits: 1,
            operations: vec![op(Gate::W)],
        };
        let mut limits = MitmLimits::for_qubits(1)?;
        limits.max_depth = 1;
        match search_exact(&target, limits) {
            Outcome::Candidate { sequence, .. } => {
                verify_exact(&sequence, &target, Limits::default())?;
            }
            other => {
                return fail!("expected phase-sensitive candidate, got {other:?}");
            }
        }
        limits.max_states = 1;
        expect_true!(matches!(
            search_exact(&target, limits),
            Outcome::Incomplete { .. }
        ));
        Ok(())
    }

    #[gtest]
    fn exact_mitm_distinguishes_complete_absence_from_partial_enumeration() -> Result<()> {
        let target = Sequence {
            qubits: 1,
            operations: vec![op(Gate::H), op(Gate::T)],
        };
        let mut limits = MitmLimits::for_qubits(1)?;
        limits.max_depth = 1;
        expect_true!(
            matches!(search_exact(&target, limits), Outcome::NoCandidate { explored } if explored > 0)
        );
        limits.max_depth = 12;
        match search_exact(&target, limits) {
            Outcome::Candidate { sequence, .. } => {
                verify_exact(&sequence, &target, Limits::default())?;
            }
            other => {
                return fail!(
                    "default search should use partial table before reporting cutoff: {other:?}"
                );
            }
        }
        Ok(())
    }

    #[gtest]
    fn two_qubit_ordered_targets_use_full_embedded_matrices() -> Result<()> {
        let target = Sequence {
            qubits: 2,
            operations: vec![
                Operation {
                    gate: Gate::H,
                    targets: vec![0],
                    controls: vec![],
                },
                Operation {
                    gate: Gate::Cx,
                    targets: vec![1, 0],
                    controls: vec![],
                },
            ],
        };
        let mut limits = MitmLimits::for_qubits(2)?;
        limits.max_depth = 2;
        match search_exact(&target, limits) {
            Outcome::Candidate { sequence, .. } => {
                verify_exact(&sequence, &target, Limits::default())?;
            }
            other => {
                return fail!("two-qubit noncommuting candidate missing: {other:?}");
            }
        }
        Ok(())
    }

    #[gtest]
    fn legal_large_target_is_not_rejected_by_small_table_coefficient_cap() -> Result<()> {
        let mut operations = Vec::new();
        for _ in 0..300 {
            operations.push(op(Gate::H));
            operations.push(op(Gate::T));
        }
        let target = Sequence {
            qubits: 1,
            operations,
        };
        quest_math::reconstruct(&target, Limits::default())?;
        let mut limits = MitmLimits::for_qubits(1)?;
        limits.max_depth = 1;
        expect_true!(matches!(
            search_exact(&target, limits),
            Outcome::NoCandidate { .. } | Outcome::Incomplete { .. }
        ));
        Ok(())
    }
}
