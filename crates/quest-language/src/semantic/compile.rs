use super::{CompileLimits, ErrorKind, SemanticError};
use crate::{
    classical::{FloatWidth, ScalarType, ScalarValue, Width},
    ssa::{self, BlockId, RegionId, Type, ValueId},
    syntax::{self, StatementKind},
};
use std::collections::BTreeMap;

#[derive(Clone)]
pub(super) struct Binding {
    pub place: ssa::Place,
    pub ty: Type,
    pub mutable: bool,
    pub constant: Option<ScalarValue>,
}
pub(super) struct Function {
    pub region: RegionId,
    pub body: Vec<syntax::Statement>,
}
pub(super) struct Compiler {
    pub program: ssa::Program,
    pub limits: CompileLimits,
    pub block: BlockId,
    pub memory: ValueId,
    pub region: RegionId,
    pub scopes: Vec<BTreeMap<String, Binding>>,
    pub functions: BTreeMap<String, Function>,
    pub types: BTreeMap<ValueId, Type>,
    pub constants: BTreeMap<ValueId, ScalarValue>,
    pub type_constants: BTreeMap<String, ScalarValue>,
    pub loops: Vec<(BlockId, BlockId)>,
    pub depth: usize,
    pub nodes: usize,
    pub next_value: usize,
}
pub(super) fn compile(
    module: &syntax::Module,
    limits: CompileLimits,
) -> Result<ssa::Program, SemanticError> {
    let owner = ssa::ProgramId::fresh()?;
    let region = RegionId::new(owner, 0);
    let entry = BlockId::new(owner, 0);
    let memory = ValueId::new(owner, 0);
    let mut compiler = Compiler {
        program: ssa::Program {
            id: owner,
            entry: region,
            regions: vec![ssa::Region {
                id: region,
                name: "<main>".into(),
                entry,
                parameters: Vec::new(),
                result: Type::Void,
                gate: false,
                oracle: None,
            }],
            blocks: vec![ssa::Block {
                id: entry,
                region,
                arguments: vec![ssa::Value {
                    id: memory,
                    ty: Type::Memory,
                }],
                instructions: Vec::new(),
                terminator: None,
                predecessors: Vec::new(),
                sealed: false,
            }],
            slots: Vec::new(),
        },
        limits,
        block: entry,
        memory,
        region,
        scopes: vec![BTreeMap::new()],
        functions: BTreeMap::new(),
        types: BTreeMap::from([(memory, Type::Memory)]),
        constants: BTreeMap::new(),
        type_constants: BTreeMap::new(),
        loops: Vec::new(),
        depth: 0,
        nodes: 1,
        next_value: 1,
    };
    compiler.register_constants(&module.statements)?;
    compiler.declarations(&module.statements)?;
    compiler.body(&module.statements, false)?;
    if !compiler.terminated()? {
        compiler.terminate(ssa::Terminator::End {
            memory: compiler.memory,
        })?;
    }
    let definitions = compiler
        .functions
        .iter()
        .map(|(name, function)| (name.clone(), function.region, function.body.clone()))
        .collect::<Vec<_>>();
    for (_, id, body) in definitions {
        compiler.function_body(id, &body)?;
    }
    for block in &mut compiler.program.blocks {
        if block.terminator.is_none() {
            block.terminator = Some(ssa::Terminator::End {
                memory: block
                    .arguments
                    .first()
                    .ok_or_else(|| SemanticError::invalid("missing block memory"))?
                    .id,
            });
        }
        block.sealed = true;
    }
    // Validate memory IR before scalar promotion, then independently verify the promoted result.
    super::allocation::hoist(&mut compiler.program)?;
    compiler.program.validate(limits)?;
    super::promote::promote(super::cfg::prune(compiler.program)?, limits)
}
impl Compiler {
    pub fn budget(&mut self) -> Result<(), SemanticError> {
        self.nodes = self
            .nodes
            .checked_add(1)
            .ok_or_else(|| SemanticError::budget("IR node count overflow"))?;
        if self.nodes > self.limits.nodes {
            return Err(SemanticError::limit(
                crate::ResourceKind::CompileNodes,
                self.nodes,
                self.limits.nodes,
                "IR node budget exceeded",
            ));
        }
        Ok(())
    }
    pub fn value(&mut self, ty: Type) -> Result<ssa::Value, SemanticError> {
        self.budget()?;
        let id = ValueId::new(self.program.id, self.next_value);
        self.next_value = self
            .next_value
            .checked_add(1)
            .ok_or_else(|| SemanticError::budget("value identity overflow"))?;
        self.types.insert(id, ty.clone());
        Ok(ssa::Value { id, ty })
    }
    pub fn emit(
        &mut self,
        kind: ssa::InstructionKind,
        types: Vec<Type>,
        span: Option<crate::SourceSpan>,
    ) -> Result<Vec<ValueId>, SemanticError> {
        self.budget()?;
        if self.terminated()? {
            return Err(SemanticError::new(
                ErrorKind::ControlFlow,
                "instruction after terminator",
            ));
        }
        let results = types
            .into_iter()
            .map(|ty| self.value(ty))
            .collect::<Result<Vec<_>, _>>()?;
        if let Some(value) = results.last().filter(|value| value.ty == Type::Memory) {
            self.memory = value.id;
        }
        if let (ssa::InstructionKind::Constant(constant), Some(result)) = (&kind, results.first()) {
            self.constants.insert(result.id, *constant);
        }
        let ids = results.iter().map(|value| value.id).collect();
        let item = ssa::Instruction {
            results,
            effect: kind.effect(),
            accesses: kind.accesses(),
            kind,
            span,
        };
        self.program
            .blocks
            .get_mut(self.block.index())
            .ok_or_else(|| SemanticError::invalid("missing current block"))?
            .instructions
            .push(item);
        Ok(ids)
    }
    pub fn emit_one(
        &mut self,
        kind: ssa::InstructionKind,
        ty: Type,
        span: Option<crate::SourceSpan>,
    ) -> Result<ValueId, SemanticError> {
        self.emit(kind, vec![ty], span)?
            .first()
            .copied()
            .ok_or_else(|| SemanticError::invalid("missing expression result"))
    }
    pub fn effect(
        &mut self,
        kind: ssa::InstructionKind,
        span: Option<crate::SourceSpan>,
    ) -> Result<(), SemanticError> {
        self.emit(kind, vec![Type::Memory], span)?;
        Ok(())
    }
    pub fn ty(&self, id: ValueId) -> Result<&Type, SemanticError> {
        self.types
            .get(&id)
            .ok_or_else(|| SemanticError::invalid("missing expression type"))
    }
    pub fn new_block(&mut self) -> Result<BlockId, SemanticError> {
        if self.program.blocks.len() >= self.limits.blocks {
            return Err(SemanticError::limit(
                crate::ResourceKind::CompileBlocks,
                self.program.blocks.len().saturating_add(1),
                self.limits.blocks,
                "block budget exceeded",
            ));
        }
        let memory = self.value(Type::Memory)?;
        let id = BlockId::new(self.program.id, self.program.blocks.len());
        self.program.blocks.push(ssa::Block {
            id,
            region: self.region,
            arguments: vec![memory],
            instructions: Vec::new(),
            terminator: None,
            predecessors: Vec::new(),
            sealed: false,
        });
        Ok(id)
    }
    pub fn switch_block(&mut self, id: BlockId) -> Result<(), SemanticError> {
        self.block = id;
        self.memory = self
            .program
            .blocks
            .get(id.index())
            .and_then(|block| block.arguments.first())
            .ok_or_else(|| SemanticError::invalid("missing memory block argument"))?
            .id;
        Ok(())
    }
    pub fn terminated(&self) -> Result<bool, SemanticError> {
        Ok(self
            .program
            .blocks
            .get(self.block.index())
            .ok_or_else(|| SemanticError::invalid("missing block"))?
            .terminator
            .is_some())
    }
    pub fn terminate(&mut self, terminator: ssa::Terminator) -> Result<(), SemanticError> {
        if self.terminated()? {
            return Err(SemanticError::new(
                ErrorKind::ControlFlow,
                "duplicate terminator",
            ));
        }
        for edge in terminator.edges() {
            let target = self
                .program
                .blocks
                .get_mut(edge.target.index())
                .ok_or_else(|| SemanticError::invalid("missing successor"))?;
            if target.sealed {
                return Err(SemanticError::invalid("adding predecessor to sealed block"));
            }
            if !target.predecessors.contains(&self.block) {
                target.predecessors.push(self.block);
            }
        }
        self.program
            .blocks
            .get_mut(self.block.index())
            .ok_or_else(|| SemanticError::invalid("missing block"))?
            .terminator = Some(terminator);
        Ok(())
    }
    pub fn jump(&mut self, target: BlockId) -> Result<(), SemanticError> {
        self.terminate(ssa::Terminator::Jump(ssa::Edge {
            target,
            arguments: vec![self.memory],
        }))
    }
    pub fn branch(
        &mut self,
        condition: ValueId,
        then_target: BlockId,
        else_target: BlockId,
    ) -> Result<(), SemanticError> {
        if self.ty(condition)? != &Type::Scalar(ScalarType::Bool) {
            return Err(SemanticError::new(
                ErrorKind::Type,
                "condition must have bool type",
            ));
        }
        if let Some(value) = self
            .constants
            .get(&condition)
            .and_then(|value| value.to_bool().ok())
        {
            return self.jump(if value { then_target } else { else_target });
        }
        self.terminate(ssa::Terminator::Branch {
            condition,
            then_edge: ssa::Edge {
                target: then_target,
                arguments: vec![self.memory],
            },
            else_edge: ssa::Edge {
                target: else_target,
                arguments: vec![self.memory],
            },
        })
    }
    pub fn binding(&self, name: &str) -> Result<Binding, SemanticError> {
        self.scopes
            .iter()
            .rev()
            .find_map(|scope| scope.get(name))
            .cloned()
            .ok_or_else(|| {
                SemanticError::new(ErrorKind::UnknownSymbol, format!("unknown symbol {name}"))
            })
    }
    pub fn bind(&mut self, name: String, binding: Binding) -> Result<(), SemanticError> {
        let scope = self
            .scopes
            .last_mut()
            .ok_or_else(|| SemanticError::invalid("missing scope"))?;
        if scope.contains_key(&name) {
            return Err(SemanticError::new(
                ErrorKind::DuplicateSymbol,
                format!("duplicate declaration {name}"),
            ));
        }
        scope.insert(name, binding);
        Ok(())
    }
    pub fn slot(
        &mut self,
        name: String,
        ty: Type,
        mutable: bool,
        interface: ssa::Interface,
        reference: bool,
    ) -> Result<Binding, SemanticError> {
        if self.program.slots.len() >= self.limits.slots {
            return Err(SemanticError::limit(
                crate::ResourceKind::CompileSlots,
                self.program.slots.len().saturating_add(1),
                self.limits.slots,
                "slot budget exceeded",
            ));
        }
        super::storage_size(&ty)?;
        let id = ssa::SlotId::new(self.program.id, self.program.slots.len());
        self.program.slots.push(ssa::Slot {
            id,
            region: self.region,
            name,
            ty: ty.clone(),
            mutable,
            interface,
            reference,
        });
        Ok(Binding {
            place: ssa::Place {
                slot: id,
                indices: Vec::new(),
            },
            ty,
            mutable,
            constant: None,
        })
    }
    fn register_constants(
        &mut self,
        statements: &[syntax::Statement],
    ) -> Result<(), SemanticError> {
        for statement in statements {
            if let StatementKind::Declare {
                name,
                ty,
                initializer: Some(initializer),
                qualifier: syntax::Qualifier::Const,
            } = &statement.kind
                && let Type::Scalar(ty) = self.resolve_type(ty)?
            {
                let value = self.const_eval(initializer)?;
                if !value.ty().can_implicitly_cast_to(ty) {
                    return Err(SemanticError::new(
                        ErrorKind::Type,
                        "constant conversion requires an explicit cast",
                    ));
                }
                let value = value.cast(ty).map_err(SemanticError::from)?;
                if self.type_constants.insert(name.clone(), value).is_some() {
                    return Err(SemanticError::new(
                        ErrorKind::DuplicateSymbol,
                        "duplicate global constant",
                    ));
                }
            }
        }
        Ok(())
    }
    fn declarations(&mut self, statements: &[syntax::Statement]) -> Result<(), SemanticError> {
        for statement in statements {
            match &statement.kind {
                StatementKind::Oracle {
                    name,
                    arity,
                    capture,
                } => {
                    let count = self.positive_size(arity)?;
                    if count > self.limits.qubits || count > self.limits.slots {
                        return Err(SemanticError::budget("oracle arity"));
                    }
                    let params = (0..count)
                        .map(|index| (format!("q{index}"), Type::Qubit(1), true, true))
                        .collect();
                    self.declare_function(name, params, Type::Void, true, &[])?;
                    self.program
                        .regions
                        .last_mut()
                        .ok_or_else(|| SemanticError::invalid("missing oracle region"))?
                        .oracle = Some(ssa::OracleId::new(*capture));
                }
                StatementKind::GateDeclaration {
                    name,
                    parameters,
                    qubits,
                    body,
                } => {
                    let float = Type::Scalar(ScalarType::Float(FloatWidth::F64));
                    let params = parameters
                        .iter()
                        .map(|name| (name.clone(), float.clone(), false, false))
                        .chain(
                            qubits
                                .iter()
                                .map(|name| (name.clone(), Type::Qubit(1), true, true)),
                        )
                        .collect();
                    self.declare_function(name, params, Type::Void, true, body)?;
                }
                StatementKind::Subroutine {
                    name,
                    parameters,
                    result,
                    body,
                } => {
                    let params = parameters
                        .iter()
                        .map(|parameter| {
                            Ok((
                                parameter.name.clone(),
                                self.resolve_type(&parameter.ty)?,
                                parameter.mutable
                                    || !matches!(
                                        &parameter.ty,
                                        syntax::Type::Array {
                                            reference: true,
                                            ..
                                        }
                                    ),
                                matches!(
                                    &parameter.ty,
                                    syntax::Type::Qubit(_)
                                        | syntax::Type::Array {
                                            reference: true,
                                            ..
                                        }
                                ),
                            ))
                        })
                        .collect::<Result<Vec<_>, SemanticError>>()?;
                    let result = result
                        .as_ref()
                        .map_or(Ok(Type::Void), |ty| self.resolve_type(ty))?;
                    self.declare_function(name, params, result, false, body)?;
                }
                _ => {}
            }
        }
        Ok(())
    }
    pub(super) fn declare_function(
        &mut self,
        name: &str,
        parameters: Vec<(String, Type, bool, bool)>,
        result: Type,
        gate: bool,
        body: &[syntax::Statement],
    ) -> Result<(), SemanticError> {
        if self.functions.contains_key(name) || crate::GateKind::lookup(name).is_some() {
            return Err(SemanticError::new(
                ErrorKind::DuplicateSymbol,
                format!("duplicate or reserved function {name}"),
            ));
        }
        let main = self.region;
        let region = RegionId::new(self.program.id, self.program.regions.len());
        self.region = region;
        let entry = self.new_block()?;
        let mut slots = Vec::new();
        let mut names = std::collections::BTreeSet::new();
        for (name, ty, mutable, reference) in parameters {
            if !names.insert(name.clone()) {
                return Err(SemanticError::new(
                    ErrorKind::DuplicateSymbol,
                    "duplicate parameter",
                ));
            }
            slots.push(
                self.slot(name, ty, mutable, ssa::Interface::Parameter, reference)?
                    .place
                    .slot,
            );
        }
        self.program.regions.push(ssa::Region {
            id: region,
            name: name.into(),
            entry,
            parameters: slots,
            result,
            gate,
            oracle: None,
        });
        self.functions.insert(
            name.into(),
            Function {
                region,
                body: body.to_vec(),
            },
        );
        self.region = main;
        Ok(())
    }
    fn function_body(
        &mut self,
        id: RegionId,
        body: &[syntax::Statement],
    ) -> Result<(), SemanticError> {
        let region = self
            .program
            .regions
            .get(id.index())
            .cloned()
            .ok_or_else(|| SemanticError::invalid("missing function"))?;
        self.region = id;
        self.switch_block(region.entry)?;
        self.loops.clear();
        let constants = self.scopes.first().map_or_default(|scope| {
            scope
                .iter()
                .filter(|(_, binding)| binding.constant.is_some())
                .map(|(name, binding)| (name.clone(), binding.clone()))
                .collect()
        });
        self.scopes = vec![constants, BTreeMap::new()];
        for parameter in &region.parameters {
            let slot = self
                .program
                .slots
                .get(parameter.index())
                .cloned()
                .ok_or_else(|| SemanticError::invalid("missing parameter"))?;
            self.bind(
                slot.name,
                Binding {
                    place: ssa::Place {
                        slot: slot.id,
                        indices: Vec::new(),
                    },
                    ty: slot.ty,
                    mutable: slot.mutable,
                    constant: None,
                },
            )?;
        }
        self.body(body, false)?;
        if !self.terminated()? {
            if region.result == Type::Void {
                self.terminate(ssa::Terminator::Return {
                    value: None,
                    memory: self.memory,
                })?;
            } else {
                if super::cfg::reachable(&self.program, region.entry).contains(&self.block) {
                    return Err(SemanticError::new(
                        ErrorKind::ControlFlow,
                        "function does not return on every path",
                    ));
                }
                self.terminate(ssa::Terminator::End {
                    memory: self.memory,
                })?;
            }
        }
        Ok(())
    }
    pub fn integer_constant(&mut self, value: i128) -> Result<ValueId, SemanticError> {
        let scalar = ScalarValue::signed(
            Width::new(64)
                .map_err(|error| SemanticError::new(ErrorKind::Numerical, error.to_string()))?,
            value,
        )
        .map_err(|error| SemanticError::new(ErrorKind::Numerical, error.to_string()))?;
        self.emit_one(
            ssa::InstructionKind::Constant(scalar),
            Type::Scalar(scalar.ty()),
            None,
        )
    }
}
