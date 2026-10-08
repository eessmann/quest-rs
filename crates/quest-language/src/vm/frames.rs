//! Call frames, control-flow execution, and transactional call preparation.
//! Arguments and buffered modifiers are checked before backend replay begins.
use super::{
	Binding, BlockId, CallArgument, Engine, Frame, GateBuffer, GateModifier, OwnedGate, Place,
	PreparedArgument, QuantumBackend, QuantumControl, QuantumKind, Rc, RefCell, RegionId,
	RuntimeCause, RuntimeContext, RuntimeValue, Sink, SlotId, Terminator, Type, VmResult,
	address_qubit_count, edge_values, get_value, merge_broadcast_width, scalar_value, select_qubit,
	selected_argument, set_value, ssa, validate_reference_aliases, validate_selected_aliases,
};

impl<B: QuantumBackend> Engine<'_, B> {
	pub(super) fn make_frame(&self, region: RegionId) -> VmResult<Frame, B::Error> {
		let mut slots = Vec::new();
		slots
			.try_reserve_exact(self.program.slots.len())
			.map_err(|_| self.fault(RuntimeCause::Allocation))?;
		slots.resize_with(self.program.slots.len(), || None);
		for slot in self
			.program
			.slots
			.iter()
			.filter(|slot| slot.region == region)
		{
			let destination = slots
				.get_mut(slot.id.index())
				.ok_or_else(|| self.fault(RuntimeCause::InvalidVerifiedProgram("slot index")))?;
			*destination = Some(Binding::Owned(Rc::new(RefCell::new(None))));
		}
		let mut values = Vec::new();
		values
			.try_reserve_exact(self.value_capacity)
			.map_err(|_| self.fault(RuntimeCause::Allocation))?;
		values.resize_with(self.value_capacity, || None);
		Ok(Frame {
			region,
			slots,
			values,
		})
	}

	pub(super) fn execute_region(
		&mut self,
		region: RegionId,
		arguments: Vec<(SlotId, Binding)>,
		sink: &mut Sink<'_>,
	) -> VmResult<Option<RuntimeValue>, B::Error> {
		self.frames = self
			.frames
			.checked_add(1)
			.ok_or_else(|| self.fault(RuntimeCause::FrameLimit))?;
		if self.frames > self.limits.call_frames {
			self.frames = self.frames.saturating_sub(1);
			return Err(self.fault(RuntimeCause::FrameLimit));
		}
		self.charge(1)?;
		let region_data = self
			.program
			.regions
			.get(region.index())
			.filter(|candidate| candidate.id == region)
			.ok_or_else(|| self.fault(RuntimeCause::InvalidVerifiedProgram("missing region")))?;
		let mut frame = self.make_frame(region)?;
		for (slot, binding) in arguments {
			let destination = frame.slots.get_mut(slot.index()).ok_or_else(|| {
				self.fault(RuntimeCause::InvalidVerifiedProgram("missing parameter"))
			})?;
			*destination = Some(binding);
		}
		let entry = region_data.entry;
		let result = self.execute_blocks(&mut frame, entry, vec![RuntimeValue::Memory], sink);
		self.frames = self.frames.saturating_sub(1);
		result
	}

	pub(super) fn execute_blocks(
		&mut self,
		frame: &mut Frame,
		mut block_id: BlockId,
		mut arguments: Vec<RuntimeValue>,
		sink: &mut Sink<'_>,
	) -> VmResult<Option<RuntimeValue>, B::Error> {
		loop {
			self.charge(1)?;
			let block = self
				.program
				.blocks
				.get(block_id.index())
				.filter(|block| block.id == block_id && block.region == frame.region)
				.cloned()
				.ok_or_else(|| self.fault(RuntimeCause::InvalidVerifiedProgram("missing block")))?;
			self.context.push(RuntimeContext {
				region: frame.region,
				block: block_id,
			});
			if block.arguments.len() != arguments.len() {
				return Err(self.fault(RuntimeCause::InvalidVerifiedProgram(
					"block argument count changed after verification",
				)));
			}
			for (value, argument) in block.arguments.iter().zip(std::mem::take(&mut arguments)) {
				set_value(frame, value.id, argument).map_err(|cause| self.fault(cause))?;
			}
			for (index, instruction) in block.instructions.iter().enumerate() {
				self.charge(1)?;
				if let Err(mut error) = self.execute_instruction(frame, instruction, sink) {
					error.block = Some(block_id);
					error.instruction = Some(index);
					error.span = instruction.span;
					return Err(error);
				}
			}
			self.charge(1)?;
			let terminator = block.terminator.as_ref().ok_or_else(|| {
				self.fault(RuntimeCause::InvalidVerifiedProgram("missing terminator"))
			})?;
			match terminator {
				Terminator::Jump(edge) => {
					arguments = edge_values(frame, edge).map_err(|cause| self.fault(cause))?;
					block_id = edge.target;
				}
				Terminator::Branch {
					condition,
					then_edge,
					else_edge,
				} => {
					let condition = scalar_value(frame, *condition)
						.and_then(|value| value.to_bool().map_err(RuntimeCause::Value))
						.map_err(|cause| self.fault(cause))?;
					let edge = if condition { then_edge } else { else_edge };
					arguments = edge_values(frame, edge).map_err(|cause| self.fault(cause))?;
					block_id = edge.target;
				}
				Terminator::Return { value, .. } => {
					let result = value
						.map(|value| get_value(frame, value))
						.transpose()
						.map_err(|cause| self.fault(cause))?;
					self.context.pop();
					return Ok(result);
				}
				Terminator::End { .. } => {
					self.context.pop();
					return Ok(None);
				}
			}
			self.context.pop();
		}
	}

	#[expect(
		clippy::too_many_lines,
		reason = "Gate calls prepare checked references, lane broadcasts, modifiers, and buffered replay atomically"
	)]
	pub(super) fn execute_call(
		&mut self,
		frame: &Frame,
		region: RegionId,
		arguments: &[CallArgument],
		controls: &[Place],
		modifiers: &[GateModifier],
		sink: &mut Sink<'_>,
	) -> VmResult<Option<RuntimeValue>, B::Error> {
		let callee = self
			.program
			.regions
			.get(region.index())
			.filter(|candidate| candidate.id == region)
			.ok_or_else(|| self.fault(RuntimeCause::InvalidVerifiedProgram("missing callee")))?;
		let oracle = callee.oracle.as_ref().map(ssa::OracleId::index);
		let parameters = callee.parameters.clone();
		let gate = callee.gate;
		if arguments.len() != parameters.len() {
			return Err(self.fault(RuntimeCause::InvalidVerifiedProgram("call argument arity")));
		}
		let mut prepared = Vec::new();
		prepared
			.try_reserve_exact(arguments.len())
			.map_err(|_| self.fault(RuntimeCause::Allocation))?;
		for (argument, parameter) in arguments.iter().zip(parameters) {
			let parameter_slot = self.slot(parameter)?;
			let (binding, mutable) = match argument {
				CallArgument::Value(value) => (
					Binding::Owned(Rc::new(RefCell::new(Some(
						get_value(frame, *value).map_err(|cause| self.fault(cause))?,
					)))),
					false,
				),
				CallArgument::Reference {
					place,
					mutable: is_mutable,
				} => {
					let address = self.resolve_place(frame, place)?;
					(Binding::Reference(address), *is_mutable)
				}
			};
			prepared.push(PreparedArgument {
				slot: parameter,
				binding,
				mutable,
				broadcast_qubit: gate && parameter_slot.ty == Type::Qubit(1),
			});
		}
		if !gate {
			validate_reference_aliases(&prepared, &[]).map_err(|cause| self.fault(cause))?;
			let bindings = prepared
				.into_iter()
				.map(|argument| (argument.slot, argument.binding))
				.collect();
			return self.execute_region(region, bindings, sink);
		}
		let modifier = self.modifier_values(frame, modifiers)?;
		if controls.len() != modifier.controls.len() {
			return Err(self.fault(RuntimeCause::InvalidVerifiedProgram(
				"external control arity",
			)));
		}
		let external = controls
			.iter()
			.zip(&modifier.controls)
			.map(|(place, positive)| {
				self.resolve_place(frame, place)
					.map(|address| (address, *positive))
			})
			.collect::<Result<Vec<_>, _>>()?;
		let mut width = 1usize;
		for argument in &prepared {
			if argument.broadcast_qubit {
				let Binding::Reference(address) = &argument.binding else {
					return Err(self.fault(RuntimeCause::InvalidVerifiedProgram(
						"gate qubit parameter is not a reference",
					)));
				};
				width = merge_broadcast_width(
					width,
					address_qubit_count(address).map_err(|cause| self.fault(cause))?,
				)
				.map_err(|cause| self.fault(cause))?;
			}
		}
		for (address, _) in &external {
			width = merge_broadcast_width(
				width,
				address_qubit_count(address).map_err(|cause| self.fault(cause))?,
			)
			.map_err(|cause| self.fault(cause))?;
		}
		let mut buffer = GateBuffer::default();
		for lane in 0..width {
			let bindings = prepared
				.iter()
				.map(|argument| {
					selected_argument(argument, lane).map(|binding| (argument.slot, binding))
				})
				.collect::<Result<Vec<_>, _>>()
				.map_err(|cause| self.fault(cause))?;
			let selected_controls = external
				.iter()
				.map(|(address, positive)| {
					select_qubit(address, lane).map(|qubit| QuantumControl {
						qubit,
						positive: *positive,
					})
				})
				.collect::<Result<Vec<_>, _>>()
				.map_err(|cause| self.fault(cause))?;
			validate_selected_aliases(&bindings, &prepared, &external, lane)
				.map_err(|cause| self.fault(cause))?;
			let first = buffer.requests.len();
			let result = if let Some(capture) = oracle {
				let targets = prepared
					.iter()
					.map(|argument| {
						let Binding::Reference(address) = &argument.binding else {
							return Err(RuntimeCause::InvalidVerifiedProgram(
								"oracle operand is not a reference",
							));
						};
						select_qubit(address, lane)
					})
					.collect::<Result<Vec<_>, _>>()
					.map_err(|cause| self.fault(cause))?;
				self.dispatch(
					OwnedGate {
						kind: QuantumKind::Oracle(capture),
						parameters: Vec::new(),
						targets,
						controls: Vec::new(),
						inverse: false,
					},
					&mut Sink::Buffer(&mut buffer),
					None,
				)?;
				None
			} else {
				self.execute_region(region, bindings, &mut Sink::Buffer(&mut buffer))?
			};
			if result.is_some() {
				return Err(self.fault(RuntimeCause::InvalidVerifiedProgram(
					"unitary gate returned a value",
				)));
			}
			self.add_external_controls(&mut buffer, first, &selected_controls)?;
		}
		if modifier.exact_inverse
			&& buffer
				.requests
				.iter()
				.any(|request| matches!(request.kind, QuantumKind::Oracle(_)))
		{
			return Err(self.fault(RuntimeCause::Unsupported(
				"exact inverse or negative power of a numerical oracle",
			)));
		}
		if modifier.inverse {
			buffer.requests.reverse();
			for request in &mut buffer.requests {
				request.inverse = !request.inverse;
			}
		}
		self.replay(&buffer.requests, modifier.repetitions, sink)?;
		self.storage_used = self.storage_used.saturating_sub(buffer.bytes);
		Ok(None)
	}
}
