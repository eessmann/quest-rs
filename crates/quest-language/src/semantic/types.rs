use super::{ErrorKind, SemanticError};
use crate::{
	ssa::{Place, Program, Type},
	syntax::BinaryOperator,
};

pub fn binary_type(
	operator: BinaryOperator,
	left: &Type,
	right: &Type,
) -> Result<Type, SemanticError> {
	let (Type::Scalar(lhs), Type::Scalar(rhs)) = (left, right) else {
		return Err(SemanticError::new(
			ErrorKind::Type,
			"scalar operands required",
		));
	};
	lhs.binary_result(operator, *rhs)
		.map(Type::Scalar)
		.map_err(|error| SemanticError::new(ErrorKind::Type, error.to_string()))
}
pub fn place_type(program: &Program, place: &Place) -> Result<Type, SemanticError> {
	let slot = program
		.slots
		.get(place.slot.index())
		.filter(|slot| slot.id == place.slot)
		.ok_or_else(|| SemanticError::invalid("unknown or foreign storage slot"))?;
	let mut ty = slot.ty.clone();
	for _ in &place.indices {
		ty = ty
			.indexed()
			.ok_or_else(|| SemanticError::invalid("indexing a scalar storage slot"))?;
	}
	Ok(ty)
}
pub fn storage_size(ty: &Type) -> Result<usize, SemanticError> {
	if matches!(ty, Type::Array { dimensions, .. } if dimensions.is_empty()) {
		return Err(SemanticError::invalid("array type requires dimensions"));
	}
	match ty {
		Type::Scalar(_) => Ok(8),
		Type::Qubit(count) => Ok(*count),
		Type::Array { dimensions, .. } => dimensions.iter().try_fold(8usize, |size, dimension| {
			if *dimension == 0 {
				return Err(SemanticError::new(
					ErrorKind::Type,
					"array dimensions must be positive",
				));
			}
			size.checked_mul(*dimension)
				.ok_or_else(|| SemanticError::budget("array storage overflow"))
		}),
		Type::Memory | Type::Void => {
			Err(SemanticError::invalid("non-storage type in a storage slot"))
		}
	}
}
