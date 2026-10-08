//! Runtime storage admission, typed values, and checked reference aliases.
//! Storage estimates and execution share these shape calculations.
use super::{
	Address, BTreeSet, Binding, ClassicalValue, Frame, GateKind, Interface, PreparedArgument,
	QuantumControl, Rc, RefCell, RunOutput, RuntimeCause, RuntimeValue, ScalarType, ScalarValue,
	SlotId, Type, ValueError, ValueId, VerifiedProgram, ssa,
};

/// Conservative VM frame/storage reservation for explicit runtime frame limits.
///
/// Includes scalar/array payloads, slot cells, SSA values and returned outputs.
/// Returns `None` for arithmetic overflow. The runtime applies this same estimate.
#[must_use]
pub fn runtime_storage(program: &VerifiedProgram, call_frames: usize) -> Option<usize> {
	let values = program
		.blocks()
		.iter()
		.flat_map(|block| {
			block
				.arguments
				.iter()
				.chain(block.instructions.iter().flat_map(|i| &i.results))
		})
		.map(|value| value.id.index())
		.max()
		.map_or(Some(0), |index| index.checked_add(1))?;
	estimate_storage(program.program(), values, call_frames)
}
/// Conservative owned result storage, including map nodes, names and nested arrays.
/// Returns `None` if the declared output shapes overflow storage arithmetic.
#[must_use]
pub fn output_storage(program: &VerifiedProgram) -> Option<usize> {
	output_storage_inner(program.program())
}
pub(super) fn output_storage_inner(program: &ssa::Program) -> Option<usize> {
	// A full B-tree node per entry overcounts sparse nodes and internal edges.
	const NODE: usize =
		16 * (size_of::<String>() + size_of::<ClassicalValue>() + size_of::<usize>());
	program
		.slots
		.iter()
		.filter(|slot| slot.interface == Interface::Output)
		.try_fold(size_of::<RunOutput>(), |total, slot| {
			total
				.checked_add(NODE)?
				.checked_add(slot.name.len())?
				.checked_add(type_bytes(&slot.ty)?)
		})
}
pub(super) fn estimate_storage(
	program: &ssa::Program,
	values: usize,
	frames: usize,
) -> Option<usize> {
	let slot_bytes = program.slots.iter().try_fold(0usize, |total, slot| {
		total.checked_add(type_bytes(&slot.ty)?)
	})?;
	let value_bytes = values.checked_mul(size_of::<Option<RuntimeValue>>())?;
	let value_payload_bytes = program
		.blocks
		.iter()
		.flat_map(|block| {
			block.arguments.iter().chain(
				block
					.instructions
					.iter()
					.flat_map(|instruction| &instruction.results),
			)
		})
		.try_fold(0usize, |total, value| {
			total.checked_add(type_bytes(&value.ty)?)
		})?;
	let cell_bytes = program
		.slots
		.len()
		.checked_mul(size_of::<RefCell<Option<RuntimeValue>>>())?;
	let frame_bytes = program
		.slots
		.len()
		.checked_mul(size_of::<Option<Binding>>())?
		.checked_add(value_bytes)?
		.checked_add(value_payload_bytes)?
		.checked_add(cell_bytes)?
		.checked_add(slot_bytes)?;
	frame_bytes
		.checked_mul(frames)?
		.checked_add(output_storage_inner(program)?)
}

pub(super) fn type_bytes(ty: &Type) -> Option<usize> {
	match ty {
		Type::Scalar(scalar) => Some(scalar.storage_bytes()),
		Type::Array {
			dimensions,
			element: _,
		} => dimensions
			.iter()
			.try_fold((0usize, 1usize), |(total, parents), dimension| {
				let items = parents.checked_mul(*dimension)?;
				let bytes = items.checked_mul(size_of::<ClassicalValue>())?;
				Some((total.checked_add(bytes)?, items))
			})
			.map(|(total, _)| total),
		Type::Qubit(count) => count.checked_mul(size_of::<usize>()),
		Type::Memory => Some(1),
		Type::Void => Some(0),
	}
}

pub(super) fn zero_runtime_value<E>(ty: &Type) -> Result<RuntimeValue, RuntimeCause<E>> {
	match ty {
		Type::Scalar(ScalarType::Bit(width)) => {
			let length = usize::from(width.value());
			let mut bits = String::new();
			bits.try_reserve_exact(length)
				.map_err(|_| RuntimeCause::Allocation)?;
			bits.extend(std::iter::repeat_n('0', length));
			ScalarValue::bitstring(&bits)
				.map(|value| RuntimeValue::Classical(ClassicalValue::Scalar(value)))
				.map_err(RuntimeCause::Value)
		}
		Type::Array {
			element,
			dimensions,
		} => zero_array(*element, dimensions).map(RuntimeValue::Classical),
		Type::Scalar(_) | Type::Qubit(_) | Type::Memory | Type::Void => {
			Err(RuntimeCause::InvalidVerifiedProgram("AllocateArray type"))
		}
	}
}

pub(super) fn zero_array<E>(
	element: ScalarType,
	dimensions: &[usize],
) -> Result<ClassicalValue, RuntimeCause<E>> {
	let (&length, remaining) = dimensions
		.split_first()
		.ok_or(RuntimeCause::InvalidVerifiedProgram("empty array shape"))?;
	let mut values = Vec::new();
	values
		.try_reserve_exact(length)
		.map_err(|_| RuntimeCause::Allocation)?;
	for _ in 0..length {
		let value = if remaining.is_empty() {
			zero_scalar(element)?
		} else {
			zero_array(element, remaining)?
		};
		values.push(value);
	}
	Ok(ClassicalValue::Array(values))
}

pub(super) fn zero_scalar<E>(ty: ScalarType) -> Result<ClassicalValue, RuntimeCause<E>> {
	let value = match ty {
		ScalarType::Bool => ScalarValue::boolean(false),
		ScalarType::Bit(width) => {
			let length = usize::from(width.value());
			let mut bits = String::new();
			bits.try_reserve_exact(length)
				.map_err(|_| RuntimeCause::Allocation)?;
			bits.extend(std::iter::repeat_n('0', length));
			ScalarValue::bitstring(&bits).map_err(RuntimeCause::Value)?
		}
		ScalarType::Int(width) => ScalarValue::signed(width, 0).map_err(RuntimeCause::Value)?,
		ScalarType::Uint(width) => ScalarValue::unsigned(width, 0).map_err(RuntimeCause::Value)?,
		ScalarType::Angle(width) => {
			ScalarValue::angle_bits(width, 0).map_err(RuntimeCause::Value)?
		}
		ScalarType::Float(width) => {
			ScalarValue::floating(width, 0.0).map_err(RuntimeCause::Value)?
		}
	};
	Ok(ClassicalValue::Scalar(value))
}

pub(super) fn classical_matches(value: &ClassicalValue, ty: &Type) -> bool {
	match (value, ty) {
		(ClassicalValue::Scalar(value), Type::Scalar(ty)) => value.ty() == *ty,
		(
			ClassicalValue::Array(values),
			Type::Array {
				element,
				dimensions,
			},
		) => dimensions.split_first().is_some_and(|(length, remaining)| {
			*length == values.len()
				&& values
					.iter()
					.all(|value| classical_array_element_matches(value, *element, remaining))
		}),
		_ => false,
	}
}

pub(super) fn classical_array_element_matches(
	value: &ClassicalValue,
	element: ScalarType,
	remaining: &[usize],
) -> bool {
	if remaining.is_empty() {
		matches!(value, ClassicalValue::Scalar(value) if value.ty() == element)
	} else {
		let ClassicalValue::Array(values) = value else {
			return false;
		};
		remaining.split_first().is_some_and(|(length, tail)| {
			*length == values.len()
				&& values
					.iter()
					.all(|value| classical_array_element_matches(value, element, tail))
		})
	}
}

pub(super) fn set_value<E>(
	frame: &mut Frame,
	id: ValueId,
	value: RuntimeValue,
) -> Result<(), RuntimeCause<E>> {
	let destination = frame
		.values
		.get_mut(id.index())
		.ok_or(RuntimeCause::InvalidVerifiedProgram("value index"))?;
	*destination = Some(value);
	Ok(())
}
pub(super) fn get_value<E>(frame: &Frame, id: ValueId) -> Result<RuntimeValue, RuntimeCause<E>> {
	frame
		.values
		.get(id.index())
		.and_then(Option::as_ref)
		.cloned()
		.ok_or(RuntimeCause::InvalidVerifiedProgram("undefined SSA value"))
}
pub(super) fn scalar_value<E>(frame: &Frame, id: ValueId) -> Result<ScalarValue, RuntimeCause<E>> {
	let RuntimeValue::Classical(ClassicalValue::Scalar(value)) = get_value(frame, id)? else {
		return Err(RuntimeCause::InvalidVerifiedProgram(
			"SSA value is not scalar",
		));
	};
	Ok(value)
}
pub(super) fn edge_values<E>(
	frame: &Frame,
	edge: &ssa::Edge,
) -> Result<Vec<RuntimeValue>, RuntimeCause<E>> {
	edge.arguments
		.iter()
		.map(|id| get_value(frame, *id))
		.collect()
}
pub(super) fn binding_address(binding: &Binding) -> Address {
	match binding {
		Binding::Owned(cell) => Address {
			cell: Rc::clone(cell),
			path: Vec::new(),
		},
		Binding::Reference(address) => address.clone(),
	}
}
pub(super) fn initialize_binding<E>(
	frame: &Frame,
	slot: SlotId,
	value: RuntimeValue,
) -> Result<(), RuntimeCause<E>> {
	let binding = frame
		.slots
		.get(slot.index())
		.and_then(Option::as_ref)
		.ok_or(RuntimeCause::InvalidVerifiedProgram("slot binding"))?;
	write_address(&binding_address(binding), value, true)
}
pub(super) fn read_address<E>(address: &Address) -> Result<RuntimeValue, RuntimeCause<E>> {
	let value = address
		.cell
		.borrow()
		.clone()
		.ok_or(RuntimeCause::Uninitialized)?;
	address
		.path
		.iter()
		.try_fold(value, |value, index| index_runtime_value_at(&value, *index))
}
pub(super) fn write_address<E>(
	address: &Address,
	value: RuntimeValue,
	initializing: bool,
) -> Result<(), RuntimeCause<E>> {
	let mut root = address
		.cell
		.try_borrow_mut()
		.map_err(|_| RuntimeCause::Alias)?;
	if address.path.is_empty() {
		if initializing && root.is_some() {
			return Err(RuntimeCause::Alias);
		}
		if !initializing && root.is_none() {
			return Err(RuntimeCause::Uninitialized);
		}
		*root = Some(value);
		return Ok(());
	}
	let root = root.as_mut().ok_or(RuntimeCause::Uninitialized)?;
	set_runtime_value(root, &address.path, value)
}
pub(super) fn runtime_length(value: &RuntimeValue) -> Option<usize> {
	match value {
		RuntimeValue::Classical(ClassicalValue::Array(values)) => Some(values.len()),
		RuntimeValue::Classical(ClassicalValue::Scalar(value))
			if matches!(value.ty(), ScalarType::Bit(_)) =>
		{
			value.ty().width().map(|width| usize::from(width.value()))
		}
		RuntimeValue::Qubits(qubits) => Some(qubits.len()),
		RuntimeValue::Memory | RuntimeValue::Classical(_) => None,
	}
}

pub(super) fn type_length(ty: &Type) -> Option<usize> {
	match ty {
		Type::Array { dimensions, .. } => dimensions.first().copied(),
		Type::Scalar(ScalarType::Bit(width)) => Some(usize::from(width.value())),
		Type::Qubit(count) => Some(*count),
		Type::Scalar(_) | Type::Memory | Type::Void => None,
	}
}
pub(super) fn index_runtime_value<E>(
	value: &RuntimeValue,
	index: &ScalarValue,
) -> Result<RuntimeValue, RuntimeCause<E>> {
	let length = runtime_length(value).ok_or(RuntimeCause::InvalidVerifiedProgram(
		"indexed non-collection value",
	))?;
	let index = index.to_index(length).map_err(RuntimeCause::Value)?;
	index_runtime_value_at(value, index)
}
pub(super) fn index_runtime_value_at<E>(
	value: &RuntimeValue,
	index: usize,
) -> Result<RuntimeValue, RuntimeCause<E>> {
	match value {
		RuntimeValue::Classical(ClassicalValue::Array(values)) => values
			.get(index)
			.cloned()
			.map(RuntimeValue::Classical)
			.ok_or(RuntimeCause::Value(ValueError::Index {
				index: i128::try_from(index).unwrap_or(i128::MAX),
				length: values.len(),
			})),
		RuntimeValue::Classical(ClassicalValue::Scalar(value))
			if matches!(value.ty(), ScalarType::Bit(_)) =>
		{
			let bit = value
				.raw_bits()
				.map_err(RuntimeCause::Value)?
				.checked_shr(
					u32::try_from(index).map_err(|_| RuntimeCause::Value(ValueError::Overflow))?,
				)
				.unwrap_or(0)
				& 1;
			Ok(RuntimeValue::Classical(ClassicalValue::Scalar(
				ScalarValue::bitstring(if bit == 0 { "0" } else { "1" })
					.map_err(RuntimeCause::Value)?,
			)))
		}
		RuntimeValue::Qubits(qubits) => qubits
			.get(index)
			.copied()
			.map(|qubit| RuntimeValue::Qubits(vec![qubit]))
			.ok_or(RuntimeCause::Value(ValueError::Index {
				index: i128::try_from(index).unwrap_or(i128::MAX),
				length: qubits.len(),
			})),
		RuntimeValue::Memory | RuntimeValue::Classical(_) => Err(
			RuntimeCause::InvalidVerifiedProgram("indexed non-collection"),
		),
	}
}
pub(super) fn set_runtime_value<E>(
	root: &mut RuntimeValue,
	path: &[usize],
	value: RuntimeValue,
) -> Result<(), RuntimeCause<E>> {
	let Some((&first, rest)) = path.split_first() else {
		*root = value;
		return Ok(());
	};
	match root {
		RuntimeValue::Classical(ClassicalValue::Array(values)) => {
			let length = values.len();
			let child = values
				.get_mut(first)
				.ok_or(RuntimeCause::Value(ValueError::Index {
					index: i128::try_from(first).unwrap_or(i128::MAX),
					length,
				}))?;
			let mut runtime = RuntimeValue::Classical(child.clone());
			set_runtime_value(&mut runtime, rest, value)?;
			let RuntimeValue::Classical(updated) = runtime else {
				return Err(RuntimeCause::InvalidVerifiedProgram(
					"classical array assignment",
				));
			};
			*child = updated;
			Ok(())
		}
		RuntimeValue::Classical(ClassicalValue::Scalar(current))
			if rest.is_empty() && matches!(current.ty(), ScalarType::Bit(_)) =>
		{
			let RuntimeValue::Classical(ClassicalValue::Scalar(bit)) = value else {
				return Err(RuntimeCause::InvalidVerifiedProgram("bit assignment type"));
			};
			let width = current
				.ty()
				.width()
				.ok_or(RuntimeCause::InvalidVerifiedProgram("bit width"))?;
			let mask = 1u64
				.checked_shl(
					u32::try_from(first).map_err(|_| RuntimeCause::Value(ValueError::Overflow))?,
				)
				.ok_or(RuntimeCause::Value(ValueError::Overflow))?;
			let raw = if bit.raw_bits().map_err(RuntimeCause::Value)? == 0 {
				current.raw_bits().map_err(RuntimeCause::Value)? & !mask
			} else {
				current.raw_bits().map_err(RuntimeCause::Value)? | mask
			};
			*current = ScalarValue::bitstring(&format!(
				"{raw:0width$b}",
				width = usize::from(width.value())
			))
			.map_err(RuntimeCause::Value)?;
			Ok(())
		}
		_ => Err(RuntimeCause::InvalidVerifiedProgram(
			"indexed assignment target",
		)),
	}
}
pub(super) fn addresses_overlap(left: &Address, right: &Address) -> bool {
	Rc::ptr_eq(&left.cell, &right.cell)
		&& (left.path.starts_with(&right.path) || right.path.starts_with(&left.path))
}
pub(super) fn validate_reference_aliases<E>(
	arguments: &[PreparedArgument],
	additional: &[(Address, bool)],
) -> Result<(), RuntimeCause<E>> {
	let references = arguments
		.iter()
		.filter_map(|argument| match &argument.binding {
			Binding::Reference(address) => Some((address.clone(), argument.mutable)),
			Binding::Owned(_) => None,
		})
		.chain(additional.iter().cloned())
		.collect::<Vec<_>>();
	for (position, (left, left_mutable)) in references.iter().enumerate() {
		for (right, right_mutable) in references.iter().skip(position.saturating_add(1)) {
			if (*left_mutable || *right_mutable) && addresses_overlap(left, right) {
				return Err(RuntimeCause::Alias);
			}
		}
	}
	Ok(())
}

pub(super) fn validate_selected_aliases<E>(
	bindings: &[(SlotId, Binding)],
	prepared: &[PreparedArgument],
	external: &[(Address, bool)],
	lane: usize,
) -> Result<(), RuntimeCause<E>> {
	let mut references = bindings
		.iter()
		.zip(prepared)
		.filter_map(|((_, binding), argument)| match binding {
			Binding::Reference(address) => Some((address.clone(), argument.mutable)),
			Binding::Owned(_) => None,
		})
		.collect::<Vec<_>>();
	for (address, _) in external {
		references.push((select_qubit_address(address, lane)?, true));
	}
	validate_address_aliases(&references)
}

pub(super) fn validate_address_aliases<E>(
	references: &[(Address, bool)],
) -> Result<(), RuntimeCause<E>> {
	for (position, (left, left_mutable)) in references.iter().enumerate() {
		for (right, right_mutable) in references.iter().skip(position.saturating_add(1)) {
			if (*left_mutable || *right_mutable) && addresses_overlap(left, right) {
				return Err(RuntimeCause::Alias);
			}
		}
	}
	Ok(())
}

pub(super) const fn merge_broadcast_width<E>(
	current: usize,
	candidate: usize,
) -> Result<usize, RuntimeCause<E>> {
	if candidate == 0 {
		return Err(RuntimeCause::InvalidVerifiedProgram("empty qubit operand"));
	}
	if current == 1 {
		return Ok(candidate);
	}
	if candidate == 1 || candidate == current {
		Ok(current)
	} else {
		Err(RuntimeCause::InvalidVerifiedProgram(
			"broadcast width mismatch",
		))
	}
}

pub(super) fn address_qubit_count<E>(address: &Address) -> Result<usize, RuntimeCause<E>> {
	let RuntimeValue::Qubits(qubits) = read_address(address)? else {
		return Err(RuntimeCause::InvalidVerifiedProgram(
			"gate reference is not quantum",
		));
	};
	Ok(qubits.len())
}

pub(super) fn select_qubit_address<E>(
	address: &Address,
	lane: usize,
) -> Result<Address, RuntimeCause<E>> {
	let count = address_qubit_count(address)?;
	let index = if count == 1 { 0 } else { lane };
	if index >= count {
		return Err(RuntimeCause::InvalidVerifiedProgram(
			"broadcast lane is outside operand",
		));
	}
	let mut selected = address.clone();
	selected
		.path
		.try_reserve_exact(1)
		.map_err(|_| RuntimeCause::Allocation)?;
	selected.path.push(index);
	Ok(selected)
}

pub(super) fn select_qubit<E>(address: &Address, lane: usize) -> Result<usize, RuntimeCause<E>> {
	let selected = read_address(&select_qubit_address(address, lane)?)?;
	let RuntimeValue::Qubits(qubits) = selected else {
		return Err(RuntimeCause::InvalidVerifiedProgram(
			"selected operand is not quantum",
		));
	};
	qubits
		.first()
		.copied()
		.ok_or(RuntimeCause::InvalidVerifiedProgram("empty qubit operand"))
}

pub(super) fn selected_argument<E>(
	argument: &PreparedArgument,
	lane: usize,
) -> Result<Binding, RuntimeCause<E>> {
	if !argument.broadcast_qubit {
		return Ok(argument.binding.clone());
	}
	let Binding::Reference(address) = &argument.binding else {
		return Err(RuntimeCause::InvalidVerifiedProgram(
			"gate qubit parameter is not a reference",
		));
	};
	select_qubit_address(address, lane).map(Binding::Reference)
}
pub(super) fn unique_qubits<E>(qubits: &[usize]) -> Result<(), RuntimeCause<E>> {
	let mut unique = BTreeSet::new();
	if qubits.iter().all(|qubit| unique.insert(*qubit)) {
		Ok(())
	} else {
		Err(RuntimeCause::Alias)
	}
}
pub(super) fn validate_gate<E>(
	gate: GateKind,
	parameters: &[f64],
	targets: &[usize],
	controls: &[QuantumControl],
) -> Result<(), RuntimeCause<E>> {
	let definition = gate.definition();
	if parameters.len() != definition.parameter_count
		|| targets.len() != definition.target_count
		|| parameters.iter().any(|value| !value.is_finite())
	{
		return Err(RuntimeCause::InvalidVerifiedProgram(
			"gate request signature",
		));
	}
	let mut qubits = controls
		.iter()
		.map(|control| control.qubit)
		.chain(targets.iter().copied())
		.collect::<Vec<_>>();
	unique_qubits(&qubits)?;
	qubits.clear();
	Ok(())
}
