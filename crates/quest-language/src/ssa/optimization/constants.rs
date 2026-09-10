//! Sparse-style monotone constant lattice with executable-edge joins.
use super::{Budget, OptimizationLimits, OptimizationReport, Program, SemanticError, scalar_equal};
use crate::{
    classical::ScalarValue,
    ssa::{Instruction, InstructionKind as K, Terminator, ValueId},
};
use std::collections::{BTreeMap, BTreeSet};
#[derive(Clone, Copy)]
pub(super) enum State {
    Unknown,
    Constant(ScalarValue),
    Varying,
}
impl State {
    fn merged(self, other: Self) -> Self {
        match (self, other) {
            (Self::Varying, _) | (_, Self::Varying) => Self::Varying,
            (Self::Unknown, state) | (state, Self::Unknown) => state,
            (Self::Constant(left), Self::Constant(right)) if scalar_equal(left, right) => {
                Self::Constant(left)
            }
            _ => Self::Varying,
        }
    }
    fn same(self, other: Self) -> bool {
        match (self, other) {
            (Self::Unknown, Self::Unknown) | (Self::Varying, Self::Varying) => true,
            (Self::Constant(left), Self::Constant(right)) => scalar_equal(left, right),
            _ => false,
        }
    }
}
pub(super) type Lattice = BTreeMap<ValueId, State>;
fn merge(states: &mut Lattice, id: ValueId, new: State) -> bool {
    let old = states.get(&id).copied().unwrap_or(State::Unknown);
    let new = old.merged(new);
    states.insert(id, new);
    !old.same(new)
}
pub(super) fn analyze(
    program: &Program,
    limits: OptimizationLimits,
    budget: &mut Budget,
    report: &mut OptimizationReport,
) -> Result<Lattice, SemanticError> {
    let mut states = Lattice::new();
    let mut reachable = program
        .regions
        .iter()
        .map(|region| region.entry)
        .collect::<BTreeSet<_>>();
    for round in 0..limits.rounds {
        budget.tick()?;
        let mut changed = false;
        for block in &program.blocks {
            budget.tick()?;
            if !reachable.contains(&block.id) {
                continue;
            }
            for item in &block.instructions {
                budget.tick()?;
                let value = evaluate(item, &states);
                for result in &item.results {
                    changed |= merge(&mut states, result.id, value);
                }
            }
            if let Some(term) = &block.terminator {
                for edge in selected_edges(term, &states) {
                    budget.tick()?;
                    changed |= reachable.insert(edge.target);
                    let target = program.blocks.get(edge.target.index()).ok_or_else(|| {
                        SemanticError::invalid("constant analysis target missing")
                    })?;
                    for (value, argument) in edge.arguments.iter().zip(&target.arguments) {
                        budget.tick()?;
                        let state = states.get(value).copied().unwrap_or(State::Unknown);
                        changed |= merge(&mut states, argument.id, state);
                    }
                }
            }
        }
        report.analysis_rounds = round.saturating_add(1);
        if !changed {
            return Ok(states);
        }
    }
    Err(SemanticError::limit(
        crate::ResourceKind::OptimizationRounds,
        limits.rounds.saturating_add(1),
        limits.rounds,
        "constant analysis round budget exceeded",
    ))
}
fn selected_edges<'a>(term: &'a Terminator, states: &Lattice) -> Vec<&'a crate::ssa::Edge> {
    match term {
        Terminator::Branch {
            condition,
            then_edge,
            else_edge,
        } => match states.get(condition).copied().unwrap_or(State::Unknown) {
            State::Constant(value) => value.to_bool().map_or_else(
                |_| vec![then_edge, else_edge],
                |value| vec![if value { then_edge } else { else_edge }],
            ),
            State::Unknown => Vec::new(),
            State::Varying => vec![then_edge, else_edge],
        },
        _ => term.edges(),
    }
}
fn evaluate(item: &Instruction, states: &Lattice) -> State {
    if let K::Constant(value) = item.kind {
        return State::Constant(value);
    }
    if !matches!(
        item.kind,
        K::Unary { .. }
            | K::Binary { .. }
            | K::Cast { .. }
            | K::GateParameter { .. }
            | K::Builtin { .. }
    ) {
        return State::Varying;
    }
    let mut values = Vec::new();
    for id in item.kind.operands() {
        match states.get(&id).copied().unwrap_or(State::Unknown) {
            State::Unknown => return State::Unknown,
            State::Varying => return State::Varying,
            State::Constant(value) => values.push(value),
        }
    }
    let result = match (&item.kind, values.as_slice()) {
        (K::Unary { operator, .. }, [value]) => value.unary(*operator),
        (K::Binary { operator, .. }, [left, right]) => left.binary(*operator, right),
        (K::Cast { ty, .. }, [value]) => value.cast(*ty),
        (K::GateParameter { .. }, [value]) => value
            .to_f64()
            .and_then(|value| ScalarValue::floating(crate::classical::FloatWidth::F64, value)),
        (K::Builtin { name, .. }, values) => ScalarValue::function(name, values),
        _ => return State::Varying,
    };
    // A failed evaluation remains an instruction, preserving its runtime trap and position.
    result.map_or(State::Varying, State::Constant)
}
pub(super) fn apply(
    program: &mut Program,
    states: &Lattice,
    budget: &mut Budget,
    report: &mut OptimizationReport,
) -> Result<(), SemanticError> {
    for block in &mut program.blocks {
        for item in &mut block.instructions {
            budget.tick()?;
            if item.results.len() == 1
                && item.effect == crate::ssa::Effect::Pure
                && !matches!(item.kind, K::Constant(_))
                && let Some(State::Constant(value)) = item
                    .results
                    .first()
                    .and_then(|result| states.get(&result.id))
            {
                item.kind = K::Constant(*value);
                item.accesses.clear();
                report.constants_folded = report.constants_folded.saturating_add(1);
            }
        }
        if let Some(Terminator::Branch {
            condition,
            then_edge,
            else_edge,
        }) = &block.terminator
            && let Some(State::Constant(value)) = states.get(condition)
            && let Ok(condition) = value.to_bool()
        {
            block.terminator = Some(Terminator::Jump(if condition {
                then_edge.clone()
            } else {
                else_edge.clone()
            }));
            report.branches_simplified = report.branches_simplified.saturating_add(1);
        }
    }
    Ok(())
}
