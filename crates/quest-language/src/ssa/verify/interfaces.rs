use crate::{
    semantic::SemanticError,
    ssa::{InstructionKind as K, Interface, Program, Type},
};
use std::collections::BTreeSet;

pub(super) fn verify(program: &Program) -> Result<(), SemanticError> {
    let mut initializers = BTreeSet::new();
    let mut interface_names = BTreeSet::new();
    regions(program)?;
    for slot in &program.slots {
        if matches!(slot.interface, Interface::Input | Interface::Output)
            && !interface_names.insert(&slot.name)
        {
            return Err(SemanticError::invalid("duplicate named program interface"));
        }
        if matches!(slot.ty, Type::Qubit(_)) && !slot.reference && slot.region != program.entry {
            return Err(SemanticError::invalid(
                "owned quantum storage must belong to the entry region",
            ));
        }
        if slot.reference
            && (slot.interface != Interface::Parameter
                || !matches!(slot.ty, Type::Array { .. } | Type::Qubit(_)))
        {
            return Err(SemanticError::invalid("invalid reference interface"));
        }
        if slot.reference && matches!(slot.ty, Type::Qubit(_)) && !slot.mutable {
            return Err(SemanticError::invalid(
                "quantum reference parameter must be exclusive",
            ));
        }
        if matches!(slot.interface, Interface::Input | Interface::Output)
            && (slot.region != program.entry || matches!(slot.ty, Type::Qubit(_)))
        {
            return Err(SemanticError::invalid("invalid program I/O interface"));
        }
        if slot.interface == Interface::Input && slot.mutable {
            return Err(SemanticError::invalid("input interface is immutable"));
        }
    }
    for block in &program.blocks {
        if block
            .arguments
            .iter()
            .skip(1)
            .any(|value| !matches!(value.ty, Type::Scalar(_)))
        {
            return Err(SemanticError::invalid("non-scalar promoted block argument"));
        }
        for item in &block.instructions {
            match &item.kind {
                K::Store {
                    place,
                    initializing,
                    ..
                } => {
                    let slot = program
                        .slots
                        .get(place.slot.index())
                        .ok_or_else(|| SemanticError::invalid("missing store interface"))?;
                    if slot.interface == Interface::Input {
                        return Err(SemanticError::invalid("store to input interface"));
                    }
                    if *initializing {
                        if !place.indices.is_empty() || slot.interface == Interface::Parameter {
                            return Err(SemanticError::invalid("invalid storage initialization"));
                        }
                        register(&mut initializers, place.slot)?;
                    }
                }
                K::Allocate { slot, .. } => {
                    let entry = program
                        .regions
                        .get(program.entry.index())
                        .ok_or_else(|| SemanticError::invalid("entry region missing"))?
                        .entry;
                    if block.id != entry {
                        return Err(SemanticError::invalid(
                            "quantum allocations require the entry block",
                        ));
                    }
                    register(&mut initializers, *slot)?;
                }
                K::AllocateArray { slot, .. } | K::Input { slot, .. } => {
                    register(&mut initializers, *slot)?;
                }
                _ => {}
            }
        }
    }
    Ok(())
}
fn register(
    initializers: &mut BTreeSet<crate::ssa::SlotId>,
    slot: crate::ssa::SlotId,
) -> Result<(), SemanticError> {
    if !initializers.insert(slot) {
        return Err(SemanticError::invalid(
            "multiple initialization sites for one storage slot",
        ));
    }
    Ok(())
}

fn regions(program: &Program) -> Result<(), SemanticError> {
    for region in &program.regions {
        if region.oracle.is_some() {
            let blocks = program
                .blocks
                .iter()
                .filter(|block| block.region == region.id)
                .collect::<Vec<_>>();
            if !region.gate
                || region.id == program.entry
                || region.parameters.is_empty()
                || blocks.len() != 1
                || blocks.iter().any(|block| !block.instructions.is_empty())
                || region.parameters.iter().any(|id| {
                    program
                        .slots
                        .get(id.index())
                        .is_none_or(|slot| slot.ty != Type::Qubit(1))
                })
            {
                return Err(SemanticError::invalid(
                    "oracle requires an empty coherent region with a fixed qubit signature",
                ));
            }
        }

        if region.id == program.entry
            && (region.gate || !region.parameters.is_empty() || region.result != Type::Void)
        {
            return Err(SemanticError::invalid(
                "program entry must be a parameterless void mixed region",
            ));
        }
        let entry = program
            .blocks
            .get(region.entry.index())
            .ok_or_else(|| SemanticError::invalid("missing function entry"))?;
        if !entry.predecessors.is_empty() || entry.arguments.len() != 1 {
            return Err(SemanticError::invalid(
                "function entry has invalid predecessors or arguments",
            ));
        }
        if matches!(region.result, Type::Memory | Type::Qubit(_))
            || (region.gate && region.result != Type::Void)
        {
            return Err(SemanticError::invalid("invalid function result interface"));
        }
        let listed = region.parameters.iter().copied().collect::<BTreeSet<_>>();
        let actual = program
            .slots
            .iter()
            .filter(|slot| slot.region == region.id && slot.interface == Interface::Parameter)
            .map(|slot| slot.id)
            .collect::<BTreeSet<_>>();
        if listed != actual || listed.len() != region.parameters.len() {
            return Err(SemanticError::invalid("parameter interface mismatch"));
        }
    }
    Ok(())
}
