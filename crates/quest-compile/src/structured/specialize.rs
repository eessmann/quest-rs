//! Immutable named input binding over the checked executable graph.
use super::{
    CompileLimits, Executable, LanguageError, Program, ScalarValue, Verified, artifact, ssa,
};
use quest_language::vm::{ClassicalValue, RunInputs};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
/// Original typed input obligations and the values fixed by one consuming specialization.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct InputSpecialization {
    pub input: ssa::SnapshotId,
    pub output: ssa::SnapshotId,
    pub bindings: Vec<InputBinding>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct InputBinding {
    pub name: String,
    pub ty: ssa::Type,
    pub value: ClassicalValue,
}
impl InputSpecialization {
    pub(super) fn validate(&self) -> Result<(), LanguageError> {
        let mut names = std::collections::BTreeSet::new();
        for binding in &self.bindings {
            if !names.insert(&binding.name) || !binding.value.matches_type(&binding.ty) {
                return Err(LanguageError::Unsupported(
                    "invalid immutable input evidence",
                ));
            }
        }
        Ok(())
    }
}
fn units(value: &ClassicalValue) -> Option<usize> {
    match value {
        ClassicalValue::Scalar(_) => Some(1),
        ClassicalValue::Array(values) => values
            .iter()
            .try_fold(1usize, |n, v| n.checked_add(units(v)?)),
    }
}
fn instruction(
    kind: ssa::InstructionKind,
    results: Vec<ssa::Value>,
    span: Option<quest_language::SourceSpan>,
) -> ssa::Instruction {
    ssa::Instruction {
        effect: kind.effect(),
        accesses: kind.accesses(),
        kind,
        results,
        span,
    }
}
fn static_scalar<'a>(
    place: &ssa::Place,
    bound: &'a BTreeMap<ssa::SlotId, (ssa::Type, ClassicalValue)>,
    constants: &BTreeMap<ssa::ValueId, ScalarValue>,
) -> Option<&'a ScalarValue> {
    let (_, mut value) = bound.get(&place.slot).map(|(ty, v)| (ty, v))?;
    for index in &place.indices {
        let ClassicalValue::Array(values) = value else {
            return None;
        };
        let i = constants.get(index)?.to_index(values.len()).ok()?;
        value = values.get(i)?;
    }
    if let ClassicalValue::Scalar(v) = value {
        Some(v)
    } else {
        None
    }
}
fn emit(
    value: &ClassicalValue,
    ty: &ssa::Type,
    allocator: &mut ssa::ValueAllocator,
    output: &mut Vec<ssa::Instruction>,
    span: Option<quest_language::SourceSpan>,
) -> Result<ssa::ValueId, LanguageError> {
    let kind = match value {
        ClassicalValue::Scalar(v) => ssa::InstructionKind::Constant(*v),
        ClassicalValue::Array(values) => {
            let element = ty
                .indexed()
                .ok_or(LanguageError::Unsupported("immutable input shape"))?;
            let items = values
                .iter()
                .map(|v| emit(v, &element, allocator, output, span))
                .collect::<Result<_, _>>()?;
            ssa::InstructionKind::Array { values: items }
        }
    };
    let result = allocator.allocate(ty.clone())?;
    let id = result.id;
    output.push(instruction(kind, vec![result], span));
    Ok(id)
}
impl Program<Verified> {
    /// Bind every remaining input interface once, retaining immutable source and binding obligations.
    /// # Errors
    /// Rejects missing, unknown, wrongly typed/shaped inputs and compilation storage exhaustion.
    pub fn specialize(self, inputs: &RunInputs) -> Result<Self, LanguageError> {
        self.specialize_inputs(inputs, true)
    }
    /// Explicit partial binding; input interfaces omitted by the caller remain dynamic.
    /// # Errors
    /// Rejects unknown or wrongly typed/shaped inputs and compilation storage exhaustion.
    pub fn specialize_partial(self, inputs: &RunInputs) -> Result<Self, LanguageError> {
        self.specialize_inputs(inputs, false)
    }
    fn specialize_inputs(self, inputs: &RunInputs, complete: bool) -> Result<Self, LanguageError> {
        let limits = CompileLimits::default();
        let mut bound = BTreeMap::new();
        let mut records = Vec::new();
        let mut extra = 0usize;
        for (name, value) in inputs.iter() {
            let slot = self
                .state
                .program
                .slots()
                .iter()
                .find(|slot| slot.interface == ssa::Interface::Input && slot.name == name)
                .ok_or(LanguageError::Unsupported(
                    "unknown or already bound immutable input",
                ))?;
            if !value.matches_type(&slot.ty) {
                return Err(LanguageError::Unsupported("immutable input type or shape"));
            }
            extra = extra
                .checked_add(
                    units(value)
                        .and_then(|n| n.checked_mul(1024))
                        .and_then(|n| n.checked_add(name.len()))
                        .ok_or(LanguageError::Budget("immutable input storage"))?,
                )
                .ok_or(LanguageError::Budget("immutable input storage"))?;
            extra = extra
                .checked_add(
                    artifact::evidence_storage(&(name, &slot.ty, value))
                        .map_err(|_| LanguageError::Budget("immutable input evidence storage"))?,
                )
                .ok_or(LanguageError::Budget("immutable input evidence storage"))?;
            if self
                .retained_bytes()?
                .checked_mul(2)
                .and_then(|n| n.checked_add(extra))
                .ok_or(LanguageError::Budget("immutable input storage"))?
                > limits.storage_bytes
            {
                return Err(LanguageError::Budget("immutable input storage"));
            }
            bound.insert(slot.id, (slot.ty.clone(), value.clone()));
            records.push(InputBinding {
                name: name.into(),
                ty: slot.ty.clone(),
                value: value.clone(),
            });
        }
        if complete
            && self.state.program.slots().iter().any(|slot| {
                slot.interface == ssa::Interface::Input && !bound.contains_key(&slot.id)
            })
        {
            return Err(LanguageError::Unsupported("missing immutable input"));
        }
        let before = self.state.program.snapshot();
        let (mut result, after) = self.transform_ssa(|program| -> Result<_, LanguageError> {
            specialize_graph(program, &bound, limits)
        })?;
        result.state.specializations.push(InputSpecialization {
            input: before,
            output: after,
            bindings: records,
        });
        if result.retained_bytes()? > limits.storage_bytes {
            return Err(LanguageError::Budget("immutable input storage"));
        }
        Ok(result)
    }
}
impl Program<Executable> {
    /// Immutable input binding history. Source export still retains the original input declarations.
    #[must_use]
    pub fn input_specializations(&self) -> &[InputSpecialization] {
        &self.state.verified.state.specializations
    }
}

fn specialize_graph(
    program: ssa::VerifiedProgram,
    bound: &BTreeMap<ssa::SlotId, (ssa::Type, ClassicalValue)>,
    limits: CompileLimits,
) -> Result<(ssa::VerifiedProgram, ssa::SnapshotId), LanguageError> {
    let mut raw = program.into_unverified();
    let mut allocator = raw.value_allocator(limits)?;
    let constants = raw
        .blocks
        .iter()
        .flat_map(|b| &b.instructions)
        .filter_map(|i| {
            if let ssa::InstructionKind::Constant(value) = &i.kind {
                Some((i.results.first()?.id, *value))
            } else {
                None
            }
        })
        .collect::<BTreeMap<_, _>>();
    // Replacing an index operand with a literal can turn a deferred source bounds trap
    // into a verifier rejection. Keep these loads in the VM; their storage is still bound once.
    let mut deferred_indices = std::collections::BTreeSet::new();
    for item in raw.blocks.iter().flat_map(|b| &b.instructions) {
        deferred_indices.extend(
            item.accesses
                .iter()
                .flat_map(|a| a.place.indices.iter().copied()),
        );
        if let ssa::InstructionKind::Index { index, .. } = item.kind {
            deferred_indices.insert(index);
        }
    }
    for slot in &mut raw.slots {
        if bound.contains_key(&slot.id) {
            slot.interface = ssa::Interface::Local;
        }
    }
    for block in &mut raw.blocks {
        let mut output = Vec::new();
        for item in std::mem::take(&mut block.instructions) {
            match &item.kind {
                ssa::InstructionKind::Input { slot, memory } if bound.contains_key(slot) => {
                    let (ty, value) = bound
                        .get(slot)
                        .ok_or(LanguageError::Unsupported("immutable input disappeared"))?;
                    let value = emit(value, ty, &mut allocator, &mut output, item.span)?;
                    output.push(instruction(
                        ssa::InstructionKind::Store {
                            place: ssa::Place {
                                slot: *slot,
                                indices: vec![],
                            },
                            value,
                            memory: *memory,
                            initializing: true,
                        },
                        item.results,
                        item.span,
                    ));
                }
                ssa::InstructionKind::Load { place, .. } => {
                    if let Some(value) = static_scalar(place, bound, &constants).filter(|_| {
                        !item
                            .results
                            .iter()
                            .any(|v| deferred_indices.contains(&v.id))
                    }) {
                        output.push(instruction(
                            ssa::InstructionKind::Constant(*value),
                            item.results,
                            item.span,
                        ));
                    } else {
                        output.push(item);
                    }
                }
                _ => output.push(item),
            }
        }
        block.instructions = output;
    }
    let checked = raw.verify(limits)?;
    let snapshot = checked.snapshot();
    Ok((checked, snapshot))
}
