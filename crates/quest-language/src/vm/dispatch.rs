//! Gate preparation and backend publication.
//! Buffered requests are admitted before insertion; completed backend effects
//! remain observable if a subsequent dispatch fails.
use super::{
	DispatchId, Engine, Frame, GateBuffer, GateKind, GateModifier, GateRequest, ModifierValues,
	OracleRequest, OwnedGate, Place, QuantumBackend, QuantumControl, QuantumKind, RuntimeCause,
	Sink, ValueError, ValueId, VmResult, gate_storage, scalar_value, unique_qubits, validate_gate,
};

impl<B: QuantumBackend> Engine<'_, B> {
	pub(super) fn execute_gate(
		&mut self,
		frame: &Frame,
		gate: GateKind,
		arguments: &[ValueId],
		operands: &[Place],
		modifiers: &[GateModifier],
		sink: &mut Sink<'_>,
	) -> VmResult<(), B::Error> {
		let parameters = arguments
			.iter()
			.map(|value| {
				scalar_value(frame, *value).and_then(|value| {
					value
						.to_f64()
						.map_err(RuntimeCause::Value)
						.and_then(|value| {
							if value.is_finite() {
								Ok(value)
							} else {
								Err(RuntimeCause::Value(ValueError::NonFinite))
							}
						})
				})
			})
			.collect::<Result<Vec<_>, _>>()
			.map_err(|cause| self.fault(cause))?;
		let modifier = self.modifier_values(frame, modifiers)?;
		let explicit_states = modifier.controls;
		let definition = gate.definition();
		let explicit_count = explicit_states.len();
		let control_count = explicit_count
			.checked_add(definition.intrinsic_controls)
			.ok_or_else(|| self.fault(RuntimeCause::StorageLimit))?;
		let resolved = operands
			.iter()
			.map(|place| self.place_qubits(frame, place))
			.collect::<Result<Vec<_>, _>>()?;
		let width = resolved.iter().map(Vec::len).max().unwrap_or(1);
		if resolved
			.iter()
			.any(|qubits| qubits.len() != 1 && qubits.len() != width)
		{
			return Err(self.fault(RuntimeCause::InvalidVerifiedProgram(
				"broadcast width mismatch",
			)));
		}
		let dispatches = width
			.checked_mul(modifier.repetitions)
			.ok_or_else(|| self.fault(RuntimeCause::StepLimit))?;
		self.charge(dispatches)?;
		for _ in 0..modifier.repetitions {
			for lane in 0..width {
				let selected = resolved
					.iter()
					.map(|qubits| {
						let index = if qubits.len() == 1 { 0 } else { lane };
						qubits.get(index).copied().ok_or_else(|| {
							self.fault(RuntimeCause::InvalidVerifiedProgram(
								"broadcast lane is outside operand",
							))
						})
					})
					.collect::<Result<Vec<_>, _>>()?;
				let controls = selected
					.iter()
					.take(explicit_count)
					.zip(&explicit_states)
					.map(|(qubit, positive)| QuantumControl {
						qubit: *qubit,
						positive: *positive,
					})
					.chain(
						selected
							.iter()
							.skip(explicit_count)
							.take(definition.intrinsic_controls)
							.map(|qubit| QuantumControl {
								qubit: *qubit,
								positive: true,
							}),
					)
					.collect::<Vec<_>>();
				let targets = selected
					.get(control_count..)
					.ok_or_else(|| {
						self.fault(RuntimeCause::InvalidVerifiedProgram("gate control arity"))
					})?
					.to_vec();
				validate_gate(gate, &parameters, &targets, &controls)
					.map_err(|cause| self.fault(cause))?;
				let request = OwnedGate {
					kind: QuantumKind::Builtin(gate),
					parameters: parameters.clone(),
					targets,
					controls,
					inverse: modifier.inverse,
				};
				self.dispatch(request, sink, None)?;
			}
		}
		Ok(())
	}

	pub(super) fn modifier_values(
		&self,
		frame: &Frame,
		modifiers: &[GateModifier],
	) -> VmResult<ModifierValues, B::Error> {
		let mut inverse = false;
		let mut exact_inverse = false;
		let mut repetitions = 1i128;
		let mut controls = Vec::new();
		for modifier in modifiers {
			match modifier {
				GateModifier::Inverse => {
					inverse = !inverse;
					exact_inverse = true;
				}
				GateModifier::Adjoint => inverse = !inverse,
				GateModifier::Control { positive, count } => {
					controls.extend(std::iter::repeat_n(*positive, *count));
				}
				GateModifier::Power(value) => {
					let power = scalar_value(frame, *value)
						.and_then(|value| value.to_i128().map_err(RuntimeCause::Value))
						.map_err(|cause| self.fault(cause))?;
					exact_inverse |= power < 0;
					repetitions = repetitions
						.checked_mul(power)
						.ok_or_else(|| self.fault(RuntimeCause::StepLimit))?;
				}
			}
		}
		if repetitions < 0 {
			inverse = !inverse;
		}
		Ok(ModifierValues {
			exact_inverse,
			inverse,
			repetitions: usize::try_from(
				repetitions
					.checked_abs()
					.ok_or_else(|| self.fault(RuntimeCause::StepLimit))?,
			)
			.map_err(|_| self.fault(RuntimeCause::StepLimit))?,
			controls,
		})
	}

	pub(super) fn add_external_controls(
		&mut self,
		buffer: &mut GateBuffer,
		first: usize,
		external: &[QuantumControl],
	) -> VmResult<(), B::Error> {
		let requests = buffer
			.requests
			.get_mut(first..)
			.ok_or_else(|| self.fault(RuntimeCause::InvalidVerifiedProgram("gate buffer index")))?;
		let added = requests
			.len()
			.checked_mul(external.len())
			.and_then(|count| count.checked_mul(size_of::<QuantumControl>()))
			.ok_or_else(|| self.fault(RuntimeCause::StorageLimit))?;
		let total = self
			.storage_used
			.checked_add(added)
			.ok_or_else(|| self.fault(RuntimeCause::StorageLimit))?;
		if total > self.limits.storage_bytes {
			return Err(self.fault(RuntimeCause::StorageLimit));
		}
		for request in requests.iter() {
			let controls = external
				.iter()
				.copied()
				.chain(request.controls.iter().copied())
				.collect::<Vec<_>>();
			match request.kind {
				QuantumKind::Builtin(gate) => {
					validate_gate(gate, &request.parameters, &request.targets, &controls)
				}
				QuantumKind::Oracle(_) => unique_qubits(
					&controls
						.iter()
						.map(|control| control.qubit)
						.chain(request.targets.iter().copied())
						.collect::<Vec<_>>(),
				),
			}
			.map_err(|cause| self.fault(cause))?;
		}
		for request in requests {
			let mut controls = Vec::new();
			controls
				.try_reserve_exact(
					external
						.len()
						.checked_add(request.controls.len())
						.ok_or_else(|| self.fault(RuntimeCause::StorageLimit))?,
				)
				.map_err(|_| self.fault(RuntimeCause::Allocation))?;
			controls.extend_from_slice(external);
			controls.extend_from_slice(&request.controls);
			request.controls = controls;
		}
		buffer.bytes = buffer
			.bytes
			.checked_add(added)
			.ok_or_else(|| self.fault(RuntimeCause::StorageLimit))?;
		self.storage_used = total;
		Ok(())
	}

	pub(super) fn replay(
		&mut self,
		buffer: &[OwnedGate],
		repetitions: usize,
		sink: &mut Sink<'_>,
	) -> VmResult<(), B::Error> {
		let dispatches = buffer
			.len()
			.checked_mul(repetitions)
			.ok_or_else(|| self.fault(RuntimeCause::StepLimit))?;
		self.charge(dispatches)?;
		for _ in 0..repetitions {
			for request in buffer {
				self.dispatch(request.clone(), sink, None)?;
			}
		}
		Ok(())
	}

	pub(super) fn replay_prepared(
		&mut self,
		occurrence: ValueId,
		requests: &[OwnedGate],
		sink: &mut Sink<'_>,
	) -> VmResult<(), B::Error> {
		self.charge(requests.len())?;
		for (lane, request) in requests.iter().enumerate() {
			self.dispatch(request.clone(), sink, Some(DispatchId { occurrence, lane }))?;
		}
		Ok(())
	}
	pub(super) fn dispatch(
		&mut self,
		request: OwnedGate,
		sink: &mut Sink<'_>,
		prepared: Option<DispatchId>,
	) -> VmResult<(), B::Error> {
		match sink {
			Sink::Backend => {
				if let QuantumKind::Oracle(capture) = request.kind {
					self.backend
						.apply_oracle(OracleRequest {
							capture,
							targets: &request.targets,
							controls: &request.controls,
							adjoint: request.inverse,
						})
						.ok_or_else(|| self.fault(RuntimeCause::Unsupported("oracle execution")))?
						.map_err(|error| self.fault(RuntimeCause::Backend(error)))?;
				} else if let QuantumKind::Builtin(gate) = request.kind {
					let request = GateRequest {
						gate,
						parameters: &request.parameters,
						targets: &request.targets,
						controls: &request.controls,
						inverse: request.inverse,
					};
					if let Some(id) = prepared {
						self.backend.apply_prepared_gate(id, request)
					} else {
						self.backend.apply_gate(request)
					}
					.map_err(|error| self.fault(RuntimeCause::Backend(error)))?;
				}
				self.completed_quantum = self
					.completed_quantum
					.checked_add(1)
					.ok_or_else(|| self.fault(RuntimeCause::StepLimit))?;
			}
			Sink::Buffer(buffer) => {
				let bytes =
					gate_storage(&request).ok_or_else(|| self.fault(RuntimeCause::StorageLimit))?;
				let total = self
					.storage_used
					.checked_add(bytes)
					.ok_or_else(|| self.fault(RuntimeCause::StorageLimit))?;
				if total > self.limits.storage_bytes {
					return Err(self.fault(RuntimeCause::StorageLimit));
				}
				buffer
					.requests
					.try_reserve_exact(1)
					.map_err(|_| self.fault(RuntimeCause::Allocation))?;
				buffer.requests.push(request);
				buffer.bytes = buffer
					.bytes
					.checked_add(bytes)
					.ok_or_else(|| self.fault(RuntimeCause::StorageLimit))?;
				self.storage_used = total;
			}
		}
		Ok(())
	}
}
