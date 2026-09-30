//! Native distributed communication-buffer requirements, checked before native entry.
use crate::{Error, Result};
use quest_circuit::{
    ControlState, Operation, OracleFragment, RegionPlan,
    dispatch_recipe::{self, DispatchStep, PrimitiveGate},
};

pub fn admit(plan: &RegionPlan, ranks: usize, limit: usize) -> Result<()> {
    let local = 1usize
        .checked_shl(u32::try_from(plan.num_qubits()).map_err(|_| Error::Overflow)?)
        .and_then(|n| n.checked_div(ranks))
        .ok_or(Error::Overflow)?;
    let mut admission = Admission {
        local,
        limit,
        seen: Vec::new(),
    };
    for instruction in plan.instructions() {
        admission.operation(instruction.operation(), 0, false, 0)?;
    }
    Ok(())
}
struct Admission<'a> {
    local: usize,
    limit: usize,
    seen: Vec<(&'a OracleFragment, usize, bool)>,
}
impl<'a> Admission<'a> {
    fn targets(&self, count: usize) -> Result<()> {
        let required = 1usize
            .checked_shl(u32::try_from(count).map_err(|_| Error::Overflow)?)
            .ok_or(Error::Overflow)?;
        if self.local < required {
            return Err(Error::Unsupported(
                "distributed communication buffer cannot hold the gate's mixed amplitudes",
            ));
        }
        Ok(())
    }
    fn operation(
        &mut self,
        operation: &'a Operation,
        inherited: usize,
        negative: bool,
        depth: usize,
    ) -> Result<()> {
        match operation {
            Operation::Gate { gate, controls, .. } => {
                let negative = negative || controls.iter().any(|c| c.state() == ControlState::Zero);
                for step in dispatch_recipe::gate_recipe(gate, usize::from(negative))?.steps() {
                    match step {
                        DispatchStep::Native(
                            PrimitiveGate::H
                            | PrimitiveGate::X
                            | PrimitiveGate::Y
                            | PrimitiveGate::Rx(_)
                            | PrimitiveGate::Ry(_),
                        ) => self.targets(1)?,
                        DispatchStep::PhaseGate(_) | DispatchStep::ScalarPhase(_) if negative => {
                            self.targets(1)?;
                        }
                        _ => {}
                    }
                }
            }
            Operation::GlobalPhase { controls, .. } => {
                if negative || controls.iter().any(|c| c.state() == ControlState::Zero) {
                    self.targets(1)?;
                }
            }
            Operation::Numerical {
                matrix,
                targets,
                controls,
            } => {
                if !matrix.is_diagonal() {
                    self.targets(
                        targets
                            .len()
                            .checked_add(controls.len())
                            .and_then(|n| n.checked_add(inherited))
                            .ok_or(Error::Overflow)?
                            .max(1),
                    )?;
                }
            }
            Operation::Oracle {
                fragment, controls, ..
            } => {
                let inherited = inherited
                    .checked_add(controls.len())
                    .ok_or(Error::Overflow)?;
                let negative = negative || controls.iter().any(|c| c.state() == ControlState::Zero);
                self.oracle(fragment, inherited, negative, depth)?;
            }
            Operation::Barrier { .. } => {}
            _ => return Err(Error::Unsupported("collective irreversible operation")),
        }
        Ok(())
    }
    fn oracle(
        &mut self,
        fragment: &'a OracleFragment,
        inherited: usize,
        negative: bool,
        depth: usize,
    ) -> Result<()> {
        if depth >= 64 {
            return Err(Error::Unsupported("collective oracle nesting"));
        }
        if let Some((_, previous, _)) = self
            .seen
            .iter_mut()
            .find(|(body, _, sign)| body.shares_storage_with(fragment) && *sign == negative)
        {
            if *previous >= inherited {
                return Ok(());
            }
            *previous = inherited;
        } else {
            let required = self
                .seen
                .len()
                .checked_add(1)
                .and_then(|n| n.checked_mul(size_of::<(&OracleFragment, usize, bool)>()))
                .ok_or(Error::Overflow)?;
            if required > self.limit {
                return Err(Error::Budget {
                    requested: required,
                    available: self.limit,
                });
            }
            self.seen
                .try_reserve_exact(1)
                .map_err(|_| Error::Allocation)?;
            self.seen.push((fragment, inherited, negative));
        }
        for operation in fragment.operations() {
            self.operation(operation, inherited, negative, depth.saturating_add(1))?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use quest_circuit::{Gate, QuantumRegionBuilder};
    #[test]
    fn one_amplitude_per_rank_accepts_diagonal_but_rejects_dense_primitives() {
        for (gate, accepted) in [
            (Gate::Z, true),
            (Gate::H, false),
            (Gate::X, false),
            (Gate::Y, false),
        ] {
            let mut builder = QuantumRegionBuilder::new(1, 0).unwrap();
            builder
                .gate(gate, &[builder.qubit(0).unwrap()], &[])
                .unwrap();
            let plan = builder.finish().unwrap().bind(&[]).unwrap().plan().unwrap();
            assert_eq!(admit(&plan, 2, 4096).is_ok(), accepted);
        }
    }
    #[test]
    fn controlled_oracle_dense_embedding_and_cache_profiles_are_admitted() {
        use quest_circuit::{Control, MatrixPolicy, NumericalOperator};
        let values = faer::Mat::from_fn(2, 2, |row, col| {
            crate::Complex64::new(f64::from(row != col), 0.)
        });
        let mut body = QuantumRegionBuilder::new(1, 0).unwrap();
        body.numerical(
            NumericalOperator::from_view(&values, MatrixPolicy::default()).unwrap(),
            &[body.qubit(0).unwrap()],
            &[],
        )
        .unwrap();
        let fragment = OracleFragment::builder(body.finish().unwrap().bind(&[]).unwrap())
            .matrix_tolerance(1e-12)
            .unwrap()
            .build()
            .unwrap();
        for (width, accepted) in [(2, false), (3, true)] {
            let mut outer = QuantumRegionBuilder::new(width, 0).unwrap();
            outer
                .oracle(&fragment, &[outer.qubit(0).unwrap()], &[])
                .unwrap();
            outer
                .oracle(
                    &fragment,
                    &[outer.qubit(0).unwrap()],
                    &[Control::new(outer.qubit(1).unwrap(), ControlState::One)],
                )
                .unwrap();
            let plan = outer.finish().unwrap().bind(&[]).unwrap().plan().unwrap();
            assert_eq!(admit(&plan, 2, 4096).is_ok(), accepted);
            assert!(matches!(admit(&plan, 2, 0), Err(Error::Budget { .. })));
        }
    }
    #[test]
    fn negative_phase_controls_admit_the_native_x_wrappers() {
        use quest_circuit::Control;
        for (state, accepted) in [(ControlState::One, true), (ControlState::Zero, false)] {
            let mut builder = QuantumRegionBuilder::new(2, 0).unwrap();
            builder
                .gate(
                    Gate::S,
                    &[builder.qubit(0).unwrap()],
                    &[Control::new(builder.qubit(1).unwrap(), state)],
                )
                .unwrap();
            let plan = builder.finish().unwrap().bind(&[]).unwrap().plan().unwrap();
            assert_eq!(admit(&plan, 4, 4096).is_ok(), accepted);
        }
    }
}
