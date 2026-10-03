use std::{collections::BTreeMap, sync::Arc};

use dashu_base::BitTest;
use dashu_int::IBig;

use super::{Error, NumericalOperator, Result};

macro_rules! owned_id {
	($name:ident) => {
		#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
		#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
		pub struct $name {
			pub owner: u64,
			pub index: usize,
		}
		impl $name {
			pub const fn index(self) -> usize {
				self.index
			}
		}
	};
}
owned_id!(QubitId);
owned_id!(BitId);
pub use crate::angle::{Angle, BoundAngleTarget, ParameterId};
owned_id!(OccurrenceId);
owned_id!(GateDefinitionId);

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum ControlState {
	Zero,
	One,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct Control {
	qubit: QubitId,
	state: ControlState,
}
impl Control {
	#[must_use]
	pub const fn new(qubit: QubitId, state: ControlState) -> Self {
		Self { qubit, state }
	}
	#[must_use]
	pub const fn qubit(self) -> QubitId {
		self.qubit
	}
	#[must_use]
	pub const fn state(self) -> ControlState {
		self.state
	}
}

/// Built-in gates use the `OpenQASM` 3.1 phase convention.
#[derive(Debug, Clone, PartialEq)]
pub enum Gate {
	Id,
	X,
	Y,
	Z,
	H,
	S,
	Sdg,
	T,
	Tdg,
	Sx,
	Sxdg,
	Swap,
	Rx(Angle),
	Ry(Angle),
	Rz(Angle),
	Phase(Angle),
	U {
		theta: Angle,
		phi: Angle,
		lambda: Angle,
	},
}

impl Gate {
	/// Lift a bound gate without assigning exact symbolic identities to radians.
	/// # Errors
	/// Rejects nonfinite bound angle values.
	pub fn from_bound(gate: &BoundGate) -> Result<Self> {
		Ok(match *gate {
			BoundGate::Id => Self::Id,
			BoundGate::X => Self::X,
			BoundGate::Y => Self::Y,
			BoundGate::Z => Self::Z,
			BoundGate::H => Self::H,
			BoundGate::S => Self::S,
			BoundGate::Sdg => Self::Sdg,
			BoundGate::T => Self::T,
			BoundGate::Tdg => Self::Tdg,
			BoundGate::Sx => Self::Sx,
			BoundGate::Sxdg => Self::Sxdg,
			BoundGate::Swap => Self::Swap,
			BoundGate::Rx(value) => Self::Rx(Angle::radians(value)?),
			BoundGate::Ry(value) => Self::Ry(Angle::radians(value)?),
			BoundGate::Rz(value) => Self::Rz(Angle::radians(value)?),
			BoundGate::Phase(value) => Self::Phase(Angle::radians(value)?),
			BoundGate::U { theta, phi, lambda } => Self::U {
				theta: Angle::radians(theta)?,
				phi: Angle::radians(phi)?,
				lambda: Angle::radians(lambda)?,
			},
		})
	}
	/// # Errors
	/// Rejects invalid semantic candidates, incompatible interfaces, and configured resource limits.
	pub fn equivalent_checked(&self, other: &Self) -> Result<bool> {
		match (self, other) {
			(Self::Rx(a), Self::Rx(b))
			| (Self::Ry(a), Self::Ry(b))
			| (Self::Rz(a), Self::Rz(b))
			| (Self::Phase(a), Self::Phase(b)) => Ok(a.equivalent_checked(b)?),
			(
				Self::U {
					theta: at,
					phi: ap,
					lambda: al,
				},
				Self::U {
					theta: bt,
					phi: bp,
					lambda: bl,
				},
			) => Ok(at.equivalent_checked(bt)?
				&& ap.equivalent_checked(bp)?
				&& al.equivalent_checked(bl)?),
			_ => Ok(self == other),
		}
	}
	/// # Errors
	/// Rejects invalid semantic candidates, incompatible interfaces, and configured resource limits.
	pub fn substitute(&self, bindings: &BTreeMap<ParameterId, Angle>, owner: u64) -> Result<Self> {
		Ok(match self {
			Self::Rx(a) => Self::Rx(a.substitute(bindings, owner)?),
			Self::Ry(a) => Self::Ry(a.substitute(bindings, owner)?),
			Self::Rz(a) => Self::Rz(a.substitute(bindings, owner)?),
			Self::Phase(a) => Self::Phase(a.substitute(bindings, owner)?),
			Self::U { theta, phi, lambda } => Self::U {
				theta: theta.substitute(bindings, owner)?,
				phi: phi.substitute(bindings, owner)?,
				lambda: lambda.substitute(bindings, owner)?,
			},
			x => x.clone(),
		})
	}
	#[must_use]
	pub const fn arity(&self) -> usize {
		self.kind().definition().target_count
	}
	/// Shared semantic registry identity for this circuit adapter.
	#[must_use]
	pub const fn kind(&self) -> crate::GateKind {
		match self {
			Self::Id => crate::GateKind::Id,
			Self::X => crate::GateKind::X,
			Self::Y => crate::GateKind::Y,
			Self::Z => crate::GateKind::Z,
			Self::H => crate::GateKind::H,
			Self::S => crate::GateKind::S,
			Self::Sdg => crate::GateKind::Sdg,
			Self::T => crate::GateKind::T,
			Self::Tdg => crate::GateKind::Tdg,
			Self::Sx => crate::GateKind::Sx,
			Self::Sxdg => crate::GateKind::Sxdg,
			Self::Swap => crate::GateKind::Swap,
			Self::Rx(..) => crate::GateKind::Rx,
			Self::Ry(..) => crate::GateKind::Ry,
			Self::Rz(..) => crate::GateKind::Rz,
			Self::Phase(..) => crate::GateKind::Phase,
			Self::U { .. } => crate::GateKind::U,
		}
	}
	/// # Errors
	/// Rejects exhausted exact angle budgets during negation.
	pub fn adjoint(&self) -> Result<Self> {
		Ok(match self {
			Self::S => Self::Sdg,
			Self::Sdg => Self::S,
			Self::T => Self::Tdg,
			Self::Tdg => Self::T,
			Self::Sx => Self::Sxdg,
			Self::Sxdg => Self::Sx,
			Self::Rx(x) => Self::Rx(x.negated()?),
			Self::Ry(x) => Self::Ry(x.negated()?),
			Self::Rz(x) => Self::Rz(x.negated()?),
			Self::Phase(x) => Self::Phase(x.negated()?),
			Self::U { theta, phi, lambda } => Self::U {
				theta: theta.negated()?,
				phi: lambda.negated()?,
				lambda: phi.negated()?,
			},
			other => other.clone(),
		})
	}
	pub fn angles(&self) -> impl Iterator<Item = &Angle> + Clone {
		match self {
			Self::Rx(x) | Self::Ry(x) | Self::Rz(x) | Self::Phase(x) => [Some(x), None, None],
			Self::U { theta, phi, lambda } => [Some(theta), Some(phi), Some(lambda)],
			_ => [None, None, None],
		}
		.into_iter()
		.flatten()
	}
	/// # Errors
	/// Rejects invalid semantic candidates, incompatible interfaces, and configured resource limits.
	pub fn bind(
		&self,
		bindings: &BTreeMap<ParameterId, f64>,
	) -> Result<(BoundGate, Vec<Option<BoundAngleTarget>>)> {
		let mut parameters = [0.0; 3];
		let mut targets = Vec::new();
		for (output, angle) in parameters.iter_mut().zip(self.angles()) {
			let (value, target) = angle.evaluate_target(bindings)?;
			*output = value;
			targets.push(Some(target));
		}
		let parameters = parameters
			.get(..self.kind().definition().parameter_count)
			.ok_or(Error::Unsupported("gate parameter capacity"))?;
		Ok((BoundGate::from_kind(self.kind(), parameters)?, targets))
	}
}

#[derive(Debug, Clone, PartialEq)]
pub enum BoundGate {
	Id,
	X,
	Y,
	Z,
	H,
	S,
	Sdg,
	T,
	Tdg,
	Sx,
	Sxdg,
	Swap,
	Rx(f64),
	Ry(f64),
	Rz(f64),
	Phase(f64),
	U { theta: f64, phi: f64, lambda: f64 },
}

impl BoundGate {
	/// Shared semantic registry identity for this circuit adapter.
	#[must_use]
	pub const fn kind(&self) -> crate::GateKind {
		match self {
			Self::Id => crate::GateKind::Id,
			Self::X => crate::GateKind::X,
			Self::Y => crate::GateKind::Y,
			Self::Z => crate::GateKind::Z,
			Self::H => crate::GateKind::H,
			Self::S => crate::GateKind::S,
			Self::Sdg => crate::GateKind::Sdg,
			Self::T => crate::GateKind::T,
			Self::Tdg => crate::GateKind::Tdg,
			Self::Sx => crate::GateKind::Sx,
			Self::Sxdg => crate::GateKind::Sxdg,
			Self::Swap => crate::GateKind::Swap,
			Self::Rx(..) => crate::GateKind::Rx,
			Self::Ry(..) => crate::GateKind::Ry,
			Self::Rz(..) => crate::GateKind::Rz,
			Self::Phase(..) => crate::GateKind::Phase,
			Self::U { .. } => crate::GateKind::U,
		}
	}
	/// Adapt a registry gate after intrinsic controls have been separated into operands.
	/// Global phase is a scalar operation and must use the caller's scalar dispatch.
	/// # Errors
	/// Rejects missing/extra or nonfinite parameters and scalar global phase.
	pub fn from_kind(kind: crate::GateKind, parameters: &[f64]) -> Result<Self> {
		use crate::{Decomposition, GateKind as G};
		let expected = kind.definition().parameter_count;
		if parameters.len() != expected {
			return Err(Error::ParameterArity {
				expected,
				actual: parameters.len(),
			});
		}
		if parameters.iter().any(|value| !value.is_finite()) {
			return Err(Error::NonFinite);
		}
		if let Decomposition::Controlled { base, .. } = kind.definition().decomposition {
			return Self::from_kind(base, parameters);
		}
		let parameter = |index| {
			parameters.get(index).copied().ok_or(Error::ParameterArity {
				expected,
				actual: parameters.len(),
			})
		};
		Ok(match kind {
			G::Id => Self::Id,
			G::X => Self::X,
			G::Y => Self::Y,
			G::Z => Self::Z,
			G::H => Self::H,
			G::S => Self::S,
			G::Sdg => Self::Sdg,
			G::T => Self::T,
			G::Tdg => Self::Tdg,
			G::Sx => Self::Sx,
			G::Sxdg => Self::Sxdg,
			G::Swap => Self::Swap,
			G::Rx => Self::Rx(parameter(0)?),
			G::Ry => Self::Ry(parameter(0)?),
			G::Rz => Self::Rz(parameter(0)?),
			G::Phase => Self::Phase(parameter(0)?),
			G::U => Self::U {
				theta: parameter(0)?,
				phi: parameter(1)?,
				lambda: parameter(2)?,
			},
			G::GlobalPhase => {
				return Err(Error::Unsupported("global phase requires scalar dispatch"));
			}
			G::Cx | G::Cy | G::Cz | G::Ccx => {
				return Err(Error::Unsupported("registry control base"));
			}
		})
	}
	/// Parameters in shared-registry order, without allocating a temporary list.
	pub fn parameters(&self) -> impl Iterator<Item = f64> + Clone {
		match self {
			Self::Rx(x) | Self::Ry(x) | Self::Rz(x) | Self::Phase(x) => [Some(*x), None, None],
			Self::U { theta, phi, lambda } => [Some(*theta), Some(*phi), Some(*lambda)],
			_ => [None, None, None],
		}
		.into_iter()
		.flatten()
	}
}

#[derive(Debug, Clone, PartialEq, Eq)]
/// A source identifier and half-open byte range into frontend-owned text.
///
/// Macro operations use the compiler's display filename (including any path
/// remapping) and the original operation-keyword span. No file contents or
/// expansion stack are stored. Rendering checks the supplied text's boundaries.
pub struct SourceSpan {
	source: Arc<str>,
	start: usize,
	end: usize,
}
impl SourceSpan {
	/// # Errors
	/// Rejects an end offset preceding the start offset.
	pub fn new(source: impl Into<Arc<str>>, start: usize, end: usize) -> Result<Self> {
		if end < start {
			return Err(Error::SourceRange);
		}
		Ok(Self {
			source: source.into(),
			start,
			end,
		})
	}
	/// Source identifier; text is supplied by the frontend's source resolver.
	#[must_use]
	pub fn source(&self) -> &str {
		&self.source
	}
	#[must_use]
	pub const fn range(&self) -> std::ops::Range<usize> {
		self.start..self.end
	}
}

#[derive(Debug, Clone)]
pub enum SemanticOperation {
	Gate {
		gate: Gate,
		targets: Arc<[QubitId]>,
		controls: Arc<[Control]>,
	},
	GlobalPhase {
		angle: Angle,
		controls: Arc<[Control]>,
	},
	Numerical {
		matrix: NumericalOperator,
		targets: Arc<[QubitId]>,
		controls: Arc<[Control]>,
	},
	Oracle {
		fragment: super::OracleFragment,
		targets: Arc<[QubitId]>,
		controls: Arc<[Control]>,
	},
	Measure {
		qubit: QubitId,
		bit: BitId,
	},
	Reset {
		qubit: QubitId,
	},
	Barrier {
		qubits: Arc<[QubitId]>,
	},
	Channel {
		kraus: Arc<[NumericalOperator]>,
		targets: Arc<[QubitId]>,
	},
	Conditional {
		bit: BitId,
		expected: bool,
		operation: Box<Self>,
	},
}

#[derive(Debug, Clone)]
pub enum Operation {
	Gate {
		gate: BoundGate,
		targets: Arc<[QubitId]>,
		controls: Arc<[Control]>,
	},
	GlobalPhase {
		radians: f64,
		controls: Arc<[Control]>,
	},
	Numerical {
		matrix: NumericalOperator,
		targets: Arc<[QubitId]>,
		controls: Arc<[Control]>,
	},
	Oracle {
		fragment: super::OracleFragment,
		targets: Arc<[QubitId]>,
		controls: Arc<[Control]>,
	},
	Measure {
		qubit: QubitId,
		bit: BitId,
	},
	Reset {
		qubit: QubitId,
	},
	Barrier {
		qubits: Arc<[QubitId]>,
	},
	Channel {
		kraus: Arc<[NumericalOperator]>,
		targets: Arc<[QubitId]>,
	},
	Conditional {
		bit: BitId,
		expected: bool,
		operation: Box<Self>,
	},
}

/// Ordered borrowed operands. A conditional exposes its quantum body's operands.
/// Classical read/write dependencies remain separate from this view.
#[derive(Debug, Clone, Copy)]
pub struct Operands<'a> {
	targets: &'a [QubitId],
	controls: &'a [Control],
}
impl<'a> Operands<'a> {
	#[must_use]
	pub const fn targets(self) -> &'a [QubitId] {
		self.targets
	}
	#[must_use]
	pub const fn controls(self) -> &'a [Control] {
		self.controls
	}
	pub fn qubits(self) -> impl Iterator<Item = QubitId> + Clone + 'a {
		self.targets
			.iter()
			.copied()
			.chain(self.controls.iter().map(|control| control.qubit()))
	}
}
macro_rules! operand_view {
	($operation:expr) => {
		match $operation {
			Self::Gate {
				targets, controls, ..
			}
			| Self::Numerical {
				targets, controls, ..
			}
			| Self::Oracle {
				targets, controls, ..
			} => Operands { targets, controls },
			Self::GlobalPhase { controls, .. } => Operands {
				targets: &[],
				controls,
			},
			Self::Measure { qubit, .. } | Self::Reset { qubit } => Operands {
				targets: std::slice::from_ref(qubit),
				controls: &[],
			},
			Self::Barrier { qubits } => Operands {
				targets: qubits,
				controls: &[],
			},
			Self::Channel { targets, .. } => Operands {
				targets,
				controls: &[],
			},
			Self::Conditional { operation, .. } => operation.operands(),
		}
	};
}
impl Operation {
	/// Retained operand payloads and Arc headers, counted per occurrence even when shared.
	/// Allocator bookkeeping is excluded. Conditional body inline storage is included.
	/// # Errors
	/// Rejects storage arithmetic overflow.
	pub fn operand_storage_bytes(&self) -> Result<usize> {
		fn bank<T>(values: &[T]) -> Result<usize> {
			values
				.len()
				.checked_mul(size_of::<T>())
				.and_then(|bytes| bytes.checked_add(const { 2 * size_of::<usize>() }))
				.and_then(|bytes| bytes.checked_add(align_of::<T>()))
				.ok_or(Error::Budget("operand storage"))
		}
		let bytes = match self {
			Self::Gate {
				targets, controls, ..
			}
			| Self::Numerical {
				targets, controls, ..
			}
			| Self::Oracle {
				targets, controls, ..
			} => bank(targets.as_ref())?.checked_add(bank(controls.as_ref())?),
			Self::GlobalPhase { controls, .. } => Some(bank(controls.as_ref())?),
			Self::Barrier { qubits } => Some(bank(qubits.as_ref())?),
			Self::Channel { targets, kraus } => {
				bank(targets.as_ref())?.checked_add(bank(kraus.as_ref())?)
			}
			Self::Conditional { operation, .. } => operation
				.operand_storage_bytes()?
				.checked_add(size_of::<Self>()),
			Self::Measure { .. } | Self::Reset { .. } => Some(0),
		};
		bytes.ok_or(Error::Budget("operand storage"))
	}
	#[must_use]
	pub fn operands(&self) -> Operands<'_> {
		operand_view!(self)
	}
	pub fn qubits(&self) -> impl Iterator<Item = QubitId> + Clone + '_ {
		self.operands().qubits()
	}
}
pub struct MappedOperands {
	pub targets: Arc<[QubitId]>,
	pub controls: Arc<[Control]>,
}
impl MappedOperands {
	#[must_use]
	pub fn scope(self) -> Arc<[QubitId]> {
		self.targets
			.iter()
			.copied()
			.chain(self.controls.iter().map(|c| c.qubit()))
			.collect()
	}
}
/// # Errors
/// Rejects invalid semantic candidates, incompatible interfaces, and configured resource limits.
pub fn remap_operands(
	local: Operands<'_>,
	mapping: &[QubitId],
	outer: &[Control],
) -> Result<MappedOperands> {
	let map = |qubit: QubitId| mapping.get(qubit.index()).copied().ok_or(Error::InvalidId);
	Ok(MappedOperands {
		targets: local
			.targets
			.iter()
			.copied()
			.map(map)
			.collect::<Result<_>>()?,
		controls: local
			.controls
			.iter()
			.map(|c| Ok(Control::new(map(c.qubit())?, c.state())))
			.chain(outer.iter().copied().map(Ok))
			.collect::<Result<_>>()?,
	})
}

impl SemanticOperation {
	/// # Errors
	/// Rejects invalid semantic candidates, incompatible interfaces, and configured resource limits.
	pub fn equivalence_work_estimate(&self) -> Result<u64> {
		match self {
			Self::Gate { gate, .. } => gate.angles().try_fold(1u64, |total, angle| {
				total
					.checked_add(angle.equivalence_work_estimate()?)
					.ok_or(Error::Budget("operation equivalence work"))
			}),
			Self::GlobalPhase { angle, .. } => Ok(angle.equivalence_work_estimate()?),
			_ => Ok(1),
		}
	}
	/// # Errors
	/// Rejects invalid semantic candidates, incompatible interfaces, and configured resource limits.
	pub fn binding_work_estimate(&self) -> Result<u64> {
		let work = match self {
			Self::Gate { gate, .. } => gate.angles().try_fold(1u64, |total, angle| {
				total
					.checked_add(angle.binding_work_estimate()?)
					.ok_or(Error::Budget("angle binding work"))
			})?,
			Self::GlobalPhase { angle, .. } => angle.binding_work_estimate()?,
			Self::Conditional { operation, .. } => operation.binding_work_estimate()?,
			Self::Numerical { .. }
			| Self::Oracle { .. }
			| Self::Measure { .. }
			| Self::Reset { .. }
			| Self::Barrier { .. }
			| Self::Channel { .. } => 1,
		};
		Ok(work)
	}
	/// # Errors
	/// Rejects invalid semantic candidates, incompatible interfaces, and configured resource limits.
	pub fn retained_bytes(&self) -> Result<usize> {
		let operands = self.operands();
		let array = |count: usize, element: usize| {
			count
				.checked_mul(element)
				.and_then(|bytes| bytes.checked_add(const { 3 * std::mem::size_of::<usize>() }))
				.ok_or(Error::Budget("ideal operand storage"))
		};
		let mut bytes = array(operands.targets().len(), std::mem::size_of::<QubitId>())?
			.checked_add(array(
				operands.controls().len(),
				std::mem::size_of::<Control>(),
			)?)
			.ok_or(Error::Budget("ideal operand storage"))?;
		let extra = match self {
			Self::Gate { gate, .. } => gate.angles().try_fold(0usize, |total, angle| {
				total
					.checked_add(angle.retained_bytes()?)
					.ok_or(Error::Budget("ideal angle storage"))
			})?,
			Self::GlobalPhase { angle, .. } => angle.retained_bytes()?,
			Self::Numerical { matrix, .. } => matrix.bytes(),
			Self::Oracle { fragment, .. } => {
				super::OracleFragment::shared_storage_bytes([fragment])?
			}
			Self::Channel { kraus, .. } => kraus.iter().try_fold(
				array(kraus.len(), std::mem::size_of::<NumericalOperator>())?,
				|total, matrix| {
					total
						.checked_add(matrix.bytes())
						.ok_or(Error::Budget("ideal channel storage"))
				},
			)?,
			Self::Conditional { operation, .. } => std::mem::size_of::<Self>()
				.checked_add(operation.retained_bytes()?)
				.ok_or(Error::Budget("ideal conditional storage"))?,
			Self::Measure { .. } | Self::Reset { .. } | Self::Barrier { .. } => 0,
		};
		bytes = bytes
			.checked_add(extra)
			.ok_or(Error::Budget("ideal storage"))?;
		Ok(bytes)
	}
	#[must_use]
	pub fn operands(&self) -> Operands<'_> {
		operand_view!(self)
	}
	pub fn qubits(&self) -> impl Iterator<Item = QubitId> + Clone + '_ {
		self.operands().qubits()
	}
	#[must_use]
	pub const fn stochastic(&self) -> bool {
		matches!(
			self,
			Self::Measure { .. } | Self::Reset { .. } | Self::Channel { .. }
		)
	}
	#[must_use]
	pub const fn exact_unitary(&self) -> bool {
		matches!(
			self,
			Self::Gate { .. } | Self::GlobalPhase { .. } | Self::Barrier { .. }
		)
	}
	/// # Errors
	/// Rejects invalid semantic candidates, incompatible interfaces, and configured resource limits.
	pub fn bind(
		&self,
		b: &BTreeMap<ParameterId, f64>,
	) -> Result<(Operation, Vec<Option<BoundAngleTarget>>)> {
		Ok(match self {
			Self::Gate {
				gate,
				targets,
				controls,
			} => {
				let (gate, identities) = gate.bind(b)?;
				(
					Operation::Gate {
						gate,
						targets: targets.clone(),
						controls: controls.clone(),
					},
					identities,
				)
			}
			Self::GlobalPhase { angle, controls } => {
				let (radians, target) = angle.evaluate_target(b)?;
				(
					Operation::GlobalPhase {
						radians,
						controls: controls.clone(),
					},
					vec![Some(target)],
				)
			}
			Self::Numerical {
				matrix,
				targets,
				controls,
			} => (
				Operation::Numerical {
					matrix: matrix.clone(),
					targets: targets.clone(),
					controls: controls.clone(),
				},
				vec![],
			),
			Self::Oracle {
				fragment,
				targets,
				controls,
			} => (
				Operation::Oracle {
					fragment: fragment.clone(),
					targets: targets.clone(),
					controls: controls.clone(),
				},
				vec![],
			),
			Self::Measure { qubit, bit } => (
				Operation::Measure {
					qubit: *qubit,
					bit: *bit,
				},
				vec![],
			),
			Self::Reset { qubit } => (Operation::Reset { qubit: *qubit }, vec![]),
			Self::Barrier { qubits } => (
				Operation::Barrier {
					qubits: qubits.clone(),
				},
				vec![],
			),
			Self::Channel { kraus, targets } => (
				Operation::Channel {
					kraus: kraus.clone(),
					targets: targets.clone(),
				},
				vec![],
			),
			Self::Conditional {
				bit,
				expected,
				operation,
			} => {
				let (body, identities) = operation.bind(b)?;
				(
					Operation::Conditional {
						bit: *bit,
						expected: *expected,
						operation: Box::new(body),
					},
					identities,
				)
			}
		})
	}
}

#[derive(Debug, Clone)]
pub struct Occurrence {
	pub id: OccurrenceId,
	pub provenance: super::ProvenanceId,
	pub source: Option<SourceSpan>,
	pub operation: SemanticOperation,
}

#[derive(Debug, Clone)]
pub struct Instruction {
	pub id: OccurrenceId,
	pub provenance: super::ProvenanceId,
	pub source: Option<SourceSpan>,
	pub operation: Operation,
	pub angle_targets: Arc<[Option<BoundAngleTarget>]>,
}
impl Instruction {
	#[must_use]
	pub const fn id(&self) -> OccurrenceId {
		self.id
	}
	#[must_use]
	pub const fn provenance(&self) -> super::ProvenanceId {
		self.provenance
	}
	#[must_use]
	pub const fn source(&self) -> Option<&SourceSpan> {
		self.source.as_ref()
	}
	#[must_use]
	pub const fn operation(&self) -> &Operation {
		&self.operation
	}
	/// Exact source target identities for this instruction's angle parameters.
	#[must_use]
	pub fn angle_targets(&self) -> &[Option<BoundAngleTarget>] {
		&self.angle_targets
	}
	/// Conservative retained storage for exact angle-target sidecars.
	/// # Errors
	/// Rejects size arithmetic overflow.
	pub fn angle_target_storage_bytes(&self) -> Result<usize> {
		fn coefficient(value: &IBig) -> Result<usize> {
			let limbs = u64::try_from(value.bit_len())
				.unwrap_or(u64::MAX)
				.div_ceil(64);
			let bytes = usize::try_from(limbs)
				.map_err(|_| Error::Budget("angle target storage"))?
				.checked_mul(8)
				.and_then(|x| x.checked_mul(2))
				.and_then(|x| x.checked_add(const { 3 * std::mem::size_of::<usize>() }))
				.ok_or(Error::Budget("angle target storage"))?;
			Ok(bytes)
		}
		let mut bytes = self
			.angle_targets
			.len()
			.checked_mul(std::mem::size_of::<Option<BoundAngleTarget>>())
			.and_then(|x| x.checked_add(const { 3 * std::mem::size_of::<usize>() }))
			.ok_or(Error::Budget("angle target storage"))?;
		for target in self.angle_targets.iter().flatten() {
			let coefficients: &[&IBig] = match target {
				BoundAngleTarget::DyadicRadians { .. } => &[],
				BoundAngleTarget::RationalPi {
					numerator,
					denominator,
				} => &[numerator, denominator],
				BoundAngleTarget::AffinePi {
					radians_numerator,
					radians_denominator,
					pi_numerator,
					pi_denominator,
				} => &[
					radians_numerator,
					radians_denominator,
					pi_numerator,
					pi_denominator,
				],
			};
			for value in coefficients {
				bytes = bytes
					.checked_add(coefficient(value)?)
					.ok_or(Error::Budget("angle target storage"))?;
			}
		}
		Ok(bytes)
	}
}

#[cfg(test)]
mod rational_pi_binding_tests {
	use super::{Angle, IBig};
	use crate::quantum::RBig;
	use googletest::prelude::*;
	#[gtest]
	fn rational_pi_binding_rounds_only_after_multiplying_by_pi() -> googletest::Result<()> {
		let tiny =
			RBig::from_parts_signed(IBig::from(1), std::ops::Shl::shl(IBig::from(1), 1075usize));
		let angle = Angle::rational_pi(tiny)?;
		expect_eq!(
			angle
				.evaluate(&std::collections::BTreeMap::new())?
				.to_bits(),
			2
		);
		Ok(())
	}
}
