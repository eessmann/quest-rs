use super::Context;
use crate::{
    classical::{ScalarType, Width},
    semantic::{ErrorKind, SemanticError, binary_type, place_type},
    ssa::{self, Block, Instruction, InstructionKind as K, Type, ValueId},
};

pub(super) fn check(
    context: &Context<'_>,
    block: &Block,
    item: &Instruction,
) -> Result<(), SemanticError> {
    for access in &item.accesses {
        let slot = context
            .program
            .slots
            .get(access.place.slot.index())
            .ok_or_else(|| SemanticError::invalid("missing accessed slot"))?;
        if slot.region != block.region {
            return Err(SemanticError::invalid(
                "storage access crosses function region",
            ));
        }
        for index in &access.place.indices {
            integer(context.ty(*index)?)?;
        }
        place_type(context.program, &access.place)?;
    }
    let expected = result_types(context, item)?;
    if item
        .results
        .iter()
        .map(|value| &value.ty)
        .ne(expected.iter())
    {
        return Err(SemanticError::invalid(
            "instruction result type or count mismatch",
        ));
    }
    Ok(())
}
fn result_types(context: &Context<'_>, item: &Instruction) -> Result<Vec<Type>, SemanticError> {
    let result = match &item.kind {
        K::Assert { condition, .. } => {
            if context.ty(*condition)? != &Type::Scalar(ScalarType::Bool) {
                return Err(SemanticError::invalid("assertion requires bool"));
            }
            vec![Type::Memory]
        }
        K::Load { place, .. } => vec![place_type(context.program, place)?],
        K::Store {
            place,
            value,
            initializing,
            ..
        } => {
            store_type(context, place, *value, *initializing)?;
            vec![Type::Memory]
        }
        K::Input { slot, .. } => {
            if context
                .program
                .slots
                .get(slot.index())
                .is_none_or(|slot| slot.interface != ssa::Interface::Input)
            {
                return Err(SemanticError::invalid(
                    "input instruction requires input interface slot",
                ));
            }
            vec![Type::Memory]
        }
        K::AllocateArray { slot, .. } => {
            if context.program.slots.get(slot.index()).is_none_or(|slot| {
                !matches!(
                    slot.ty,
                    Type::Array { .. } | Type::Scalar(ScalarType::Bit(_))
                )
            }) {
                return Err(SemanticError::invalid(
                    "array allocation requires array or bit register",
                ));
            }
            vec![Type::Memory]
        }
        K::Allocate { slot, .. } => {
            if context
                .program
                .slots
                .get(slot.index())
                .is_none_or(|slot| !matches!(slot.ty, Type::Qubit(count) if count > 0))
            {
                return Err(SemanticError::invalid(
                    "quantum allocation needs nonempty qubit slot",
                ));
            }
            vec![Type::Memory]
        }
        K::Call {
            region,
            arguments,
            controls,
            modifiers,
            ..
        } => call(context, *region, arguments, controls, modifiers)?,
        K::Gate {
            gate,
            arguments,
            operands,
            modifiers,
            ..
        } => {
            gate_check(context, *gate, arguments, operands, modifiers)?;
            vec![Type::Memory]
        }
        K::Measure { place, .. } => {
            let count = quantum(&place_type(context.program, place)?)?;
            let width = u8::try_from(count)
                .ok()
                .and_then(|width| Width::new(width).ok())
                .ok_or_else(|| SemanticError::invalid("measurement width exceeds 64 bits"))?;
            vec![Type::Scalar(ScalarType::Bit(width)), Type::Memory]
        }
        K::Reset { place, .. } => {
            quantum(&place_type(context.program, place)?)?;
            vec![Type::Memory]
        }
        K::Barrier { places, .. } => {
            for place in places {
                quantum(&place_type(context.program, place)?)?;
            }
            vec![Type::Memory]
        }
        _ => return pure_types(context, item),
    };
    Ok(result)
}
fn store_type(
    context: &Context<'_>,
    place: &ssa::Place,
    value: ValueId,
    initializing: bool,
) -> Result<(), SemanticError> {
    if &place_type(context.program, place)? != context.ty(value)? {
        return Err(SemanticError::invalid("store type mismatch"));
    }
    let slot = context
        .program
        .slots
        .get(place.slot.index())
        .ok_or_else(|| SemanticError::invalid("missing store slot"))?;
    if !slot.mutable && !initializing {
        return Err(SemanticError::new(
            ErrorKind::Alias,
            "write to immutable storage",
        ));
    }
    if matches!(slot.ty, Type::Qubit(_)) {
        return Err(SemanticError::invalid(
            "quantum references cannot be assigned",
        ));
    }
    Ok(())
}
fn pure_types(context: &Context<'_>, item: &Instruction) -> Result<Vec<Type>, SemanticError> {
    Ok(match &item.kind {
        K::RangeAdvance { current, step, end } => {
            let ty = context.ty(*current)?;
            integer(ty)?;
            if context.ty(*step)? != ty || context.ty(*end)? != ty {
                return Err(SemanticError::invalid("range operand types differ"));
            }
            vec![ty.clone(), Type::Scalar(ScalarType::Bool)]
        }
        K::Constant(value) => vec![Type::Scalar(value.ty())],
        K::Unary { operator, value } => {
            let ty = context.ty(*value)?;
            let Type::Scalar(scalar) = ty else {
                return Err(SemanticError::invalid("unary operand must be scalar"));
            };
            let result = scalar
                .unary_result(*operator)
                .map_err(|error| SemanticError::new(ErrorKind::Type, error.to_string()))?;
            vec![Type::Scalar(result)]
        }
        K::Binary {
            operator,
            left,
            right,
        } => vec![binary_type(
            *operator,
            context.ty(*left)?,
            context.ty(*right)?,
        )?],
        K::Cast { value, ty } => {
            if !scalar(context.ty(*value)?)?.can_explicitly_cast_to(*ty) {
                return Err(SemanticError::invalid("undefined explicit scalar cast"));
            }
            vec![Type::Scalar(*ty)]
        }
        K::GateParameter { value } => {
            real_parameter(context.ty(*value)?)?;
            vec![Type::Scalar(ScalarType::Float(
                crate::classical::FloatWidth::F64,
            ))]
        }
        K::Array { values } => vec![array_type(context, values)?],
        K::Builtin { name, arguments } => {
            let types = arguments
                .iter()
                .map(|id| scalar(context.ty(*id)?))
                .collect::<Result<Vec<_>, _>>()?;
            vec![Type::Scalar(
                ScalarType::function_result(name, &types)
                    .map_err(|error| SemanticError::new(ErrorKind::Type, error.to_string()))?,
            )]
        }
        K::Index { value, index } => {
            integer(context.ty(*index)?)?;
            vec![
                context
                    .ty(*value)?
                    .indexed()
                    .ok_or_else(|| SemanticError::invalid("indexing non-indexable value"))?,
            ]
        }
        K::Capture { ty, .. } => {
            scalar(ty)?;
            vec![ty.clone()]
        }
        _ => return Err(SemanticError::invalid("unexpected non-pure instruction")),
    })
}
fn array_type(context: &Context<'_>, values: &[ValueId]) -> Result<Type, SemanticError> {
    let first = values
        .first()
        .ok_or_else(|| SemanticError::invalid("empty array literal"))?;
    let ty = context.ty(*first)?;
    for value in values {
        if context.ty(*value)? != ty {
            return Err(SemanticError::invalid("heterogeneous array literal"));
        }
    }
    match ty {
        Type::Scalar(element) => Ok(Type::Array {
            element: *element,
            dimensions: vec![values.len()],
        }),
        Type::Array {
            element,
            dimensions,
        } => {
            let mut shape = vec![values.len()];
            shape.extend(dimensions);
            Ok(Type::Array {
                element: *element,
                dimensions: shape,
            })
        }
        _ => Err(SemanticError::invalid(
            "array literal elements must be classical",
        )),
    }
}
fn call(
    context: &Context<'_>,
    region: ssa::RegionId,
    arguments: &[ssa::CallArgument],
    controls: &[ssa::Place],
    modifiers: &[ssa::GateModifier],
) -> Result<Vec<Type>, SemanticError> {
    let callee = context
        .program
        .regions
        .get(region.index())
        .filter(|callee| callee.id == region)
        .ok_or_else(|| SemanticError::invalid("unknown or foreign callee"))?;
    if arguments.len() != callee.parameters.len() {
        return Err(SemanticError::invalid("call arity mismatch"));
    }
    if !callee.gate && !modifiers.is_empty() {
        return Err(SemanticError::invalid(
            "gate modifiers applied to mixed subroutine",
        ));
    }
    if modifiers_check(context, modifiers)? != controls.len() {
        return Err(SemanticError::invalid("external control count mismatch"));
    }
    if !callee.gate && !controls.is_empty() {
        return Err(SemanticError::invalid("controls require a unitary gate"));
    }
    let mut references = Vec::new();
    for place in controls {
        quantum(&place_type(context.program, place)?)?;
        references.push((place, true));
    }
    for (argument, parameter) in arguments.iter().zip(&callee.parameters) {
        let slot = context
            .program
            .slots
            .get(parameter.index())
            .ok_or_else(|| SemanticError::invalid("missing parameter"))?;
        match argument {
            ssa::CallArgument::Value(value) if !slot.reference => {
                if context.ty(*value)? != &slot.ty {
                    return Err(SemanticError::invalid("call argument type mismatch"));
                }
            }
            ssa::CallArgument::Reference { place, mutable } if slot.reference => {
                let actual = place_type(context.program, place)?;
                let compatible = actual == slot.ty
                    || (callee.gate
                        && matches!((&slot.ty, &actual), (Type::Qubit(1), Type::Qubit(count)) if *count > 0));
                if !compatible || *mutable != slot.mutable {
                    return Err(SemanticError::invalid(
                        "reference argument type or mutability mismatch",
                    ));
                }
                let root = context
                    .program
                    .slots
                    .get(place.slot.index())
                    .ok_or_else(|| SemanticError::invalid("missing reference root"))?;
                if *mutable && !root.mutable && !matches!(root.ty, Type::Qubit(_)) {
                    return Err(SemanticError::new(
                        ErrorKind::Alias,
                        "mutable reference to immutable storage",
                    ));
                }
                references.push((place, *mutable));
            }
            _ => {
                return Err(SemanticError::invalid(
                    "value/reference call convention mismatch",
                ));
            }
        }
    }
    if callee.gate {
        let mut width = None;
        for (place, _) in &references {
            let count = quantum(&place_type(context.program, place)?)?;
            if count > 1 {
                if width.is_some_and(|width| width != count) {
                    return Err(SemanticError::invalid("gate call broadcast widths differ"));
                }
                width = Some(count);
            }
        }
    }
    for (position, (left, mutable)) in references.iter().enumerate() {
        for (right, other_mutable) in references.iter().skip(position.saturating_add(1)) {
            if (*mutable || *other_mutable) && definitely_overlap(context, left, right) {
                return Err(SemanticError::new(
                    ErrorKind::Alias,
                    "overlapping mutable call references",
                ));
            }
        }
    }
    let mut result = Vec::new();
    if callee.result != Type::Void {
        result.push(callee.result.clone());
    }
    result.push(Type::Memory);
    Ok(result)
}
fn gate_check(
    context: &Context<'_>,
    gate: crate::GateKind,
    arguments: &[ValueId],
    operands: &[ssa::Place],
    modifiers: &[ssa::GateModifier],
) -> Result<(), SemanticError> {
    let definition = gate.definition();
    if arguments.len() != definition.parameter_count {
        return Err(SemanticError::invalid("gate parameter arity mismatch"));
    }
    for argument in arguments {
        real_parameter(context.ty(*argument)?)?;
    }
    let controls = modifiers_check(context, modifiers)?;
    let expected = definition
        .target_count
        .checked_add(definition.intrinsic_controls)
        .and_then(|count| count.checked_add(controls))
        .ok_or_else(|| SemanticError::budget("gate arity overflow"))?;
    if operands.len() != expected {
        return Err(SemanticError::invalid("gate operand arity mismatch"));
    }
    let mut broadcast = None;
    for place in operands {
        let count = quantum(&place_type(context.program, place)?)?;
        if count > 1 {
            if broadcast.is_some_and(|other| other != count) {
                return Err(SemanticError::invalid("gate broadcast widths differ"));
            }
            broadcast = Some(count);
        }
    }
    for (position, left) in operands.iter().enumerate() {
        for right in operands.iter().skip(position.saturating_add(1)) {
            if definitely_overlap(context, left, right) {
                return Err(SemanticError::new(
                    ErrorKind::Alias,
                    "overlapping gate operands",
                ));
            }
        }
    }
    Ok(())
}
fn real_parameter(ty: &Type) -> Result<(), SemanticError> {
    if matches!(
        ty,
        Type::Scalar(
            ScalarType::Bool
                | ScalarType::Int(_)
                | ScalarType::Uint(_)
                | ScalarType::Float(_)
                | ScalarType::Angle(_)
        )
    ) {
        Ok(())
    } else {
        Err(SemanticError::invalid(
            "gate parameters require real values; bit interpretation requires an explicit cast",
        ))
    }
}
fn modifiers_check(
    context: &Context<'_>,
    modifiers: &[ssa::GateModifier],
) -> Result<usize, SemanticError> {
    let mut controls = 0usize;
    for modifier in modifiers {
        match modifier {
            ssa::GateModifier::Control { count, .. } => {
                if *count == 0 {
                    return Err(SemanticError::invalid("control count must be positive"));
                }
                controls = controls
                    .checked_add(*count)
                    .ok_or_else(|| SemanticError::budget("control count overflow"))?;
            }
            ssa::GateModifier::Power(value) => {
                if !matches!(
                    context.ty(*value)?,
                    Type::Scalar(ScalarType::Int(_) | ScalarType::Uint(_))
                ) {
                    return Err(SemanticError::invalid("gate power requires int or uint"));
                }
            }
            ssa::GateModifier::Inverse => {}
        }
    }
    Ok(controls)
}
fn normalized_index(context: &Context<'_>, place: &ssa::Place, position: usize) -> Option<usize> {
    let index = place.indices.get(position)?;
    let slot = context.program.slots.get(place.slot.index())?;
    let size = match &slot.ty {
        Type::Array { dimensions, .. } => dimensions.get(position).copied(),
        Type::Qubit(count) if position == 0 => Some(*count),
        Type::Scalar(ScalarType::Bit(width)) if position == 0 => Some(usize::from(width.value())),
        _ => None,
    }?;
    context.constants.get(index)?.to_index(size).ok()
}
fn definitely_overlap(context: &Context<'_>, left: &ssa::Place, right: &ssa::Place) -> bool {
    left.slot == right.slot && left.indices.iter().zip(&right.indices).enumerate().all(|(position, (lhs, rhs))| {
        lhs == rhs || matches!((normalized_index(context, left, position), normalized_index(context, right, position)), (Some(left), Some(right)) if left == right)
    })
}

pub(super) fn terminator(
    context: &Context<'_>,
    block: &Block,
    term: &ssa::Terminator,
    memory: ValueId,
) -> Result<(), SemanticError> {
    match term {
        ssa::Terminator::Branch { condition, .. } => {
            if context.ty(*condition)? != &Type::Scalar(ScalarType::Bool) {
                return Err(SemanticError::invalid("branch condition must be bool"));
            }
        }
        ssa::Terminator::Return {
            value,
            memory: input,
        } => {
            let region = context
                .program
                .regions
                .get(block.region.index())
                .ok_or_else(|| SemanticError::invalid("missing return region"))?;
            let returned = value.map_or(Ok(&Type::Void), |id| context.ty(id))?;
            if returned != &region.result || *input != memory {
                return Err(SemanticError::invalid("return type or memory mismatch"));
            }
        }
        ssa::Terminator::End { memory: input } => {
            let region = context
                .program
                .regions
                .get(block.region.index())
                .ok_or_else(|| SemanticError::invalid("missing end region"))?;
            if region.gate && (block.id == region.entry || !block.predecessors.is_empty()) {
                return Err(SemanticError::invalid("end is not a unitary gate effect"));
            }
            if *input != memory {
                return Err(SemanticError::invalid("end uses stale memory"));
            }
        }
        ssa::Terminator::Jump(_) => {}
    }
    Ok(())
}
pub(super) fn gate_effect(
    context: &Context<'_>,
    block: &Block,
    item: &Instruction,
) -> Result<(), SemanticError> {
    let region = context
        .program
        .regions
        .get(block.region.index())
        .ok_or_else(|| SemanticError::invalid("missing region"))?;
    if region.gate
        && let K::Call { region: callee, .. } = item.kind
        && context
            .program
            .regions
            .get(callee.index())
            .is_none_or(|region| !region.gate)
    {
        return Err(SemanticError::new(
            ErrorKind::Type,
            "mixed subroutine called from a gate",
        ));
    }
    if region.gate
        && (matches!(item.kind, K::Reset { .. })
            || !matches!(
                item.effect,
                ssa::Effect::Pure | ssa::Effect::Read | ssa::Effect::Quantum | ssa::Effect::Call
            ))
    {
        return Err(SemanticError::invalid(
            "nonunitary effect in gate declaration",
        ));
    }
    Ok(())
}

fn scalar(ty: &Type) -> Result<ScalarType, SemanticError> {
    if let Type::Scalar(ty) = ty {
        Ok(*ty)
    } else {
        Err(SemanticError::invalid("scalar type required"))
    }
}
fn integer(ty: &Type) -> Result<(), SemanticError> {
    if matches!(
        ty,
        Type::Scalar(ScalarType::Int(_) | ScalarType::Uint(_) | ScalarType::Bit(_))
    ) {
        Ok(())
    } else {
        Err(SemanticError::invalid("integer index required"))
    }
}
fn quantum(ty: &Type) -> Result<usize, SemanticError> {
    if let Type::Qubit(count) = ty
        && *count > 0
    {
        return Ok(*count);
    }
    Err(SemanticError::invalid(
        "nonempty quantum reference required",
    ))
}
