//! The program construction entrypoint shares the text frontend's typed semantics.
use crate::{Constructed, LanguageError, OracleFragment, Program};
use quest_language::{
	SourceMap,
	classical::ScalarValue,
	semantic::{
		CompileLimits,
		builder::{self, GateDefinition},
	},
};
use std::{
	collections::BTreeMap,
	ops::{Deref, DerefMut},
};

/// Construct classical control flow and immutable quantum payloads in one program.
#[derive(Debug)]
pub struct ProgramBuilder {
	semantic: builder::Builder,
	origins: Vec<std::sync::Arc<crate::BoundRegion>>,
	captures: Vec<ScalarValue>,
	exact: BTreeMap<usize, crate::Angle>,
	payloads: BTreeMap<usize, crate::QuantumPayload>,
	oracles: BTreeMap<usize, OracleFragment>,
}
impl ProgramBuilder {
	/// # Errors
	/// Rejects exhaustion of checked program identities.
	pub fn new() -> Result<Self, LanguageError> {
		Ok(Self {
			semantic: builder::Builder::new()?,
			origins: Vec::new(),
			captures: Vec::new(),
			exact: BTreeMap::new(),
			payloads: BTreeMap::new(),
			oracles: BTreeMap::new(),
		})
	}
	/// Bind a captured immutable oracle once and return its builder-owned definition.
	/// # Errors
	/// Rejects invalid declarations; verification checks payload identity and arity.
	pub fn oracle(
		&mut self,
		name: &str,
		fragment: OracleFragment,
	) -> Result<GateDefinition, LanguageError> {
		let index = self.oracles.len();
		let definition = self.semantic.oracle(name, fragment.num_qubits(), index)?;
		self.oracles.insert(index, fragment);
		Ok(definition)
	}
	/// Bind an explicit exact angle or an opaque floating value exactly once.
	/// # Errors
	/// Rejects nonfinite values, unbound source obligations, and expression limits.
	pub fn angle(
		&mut self,
		value: impl crate::AngleCapture,
	) -> Result<builder::Expr<builder::Float<64>>, LanguageError> {
		let value = crate::capture_angle(value)?;
		let index = self.captures.len();
		if let Some(exact) = value.exact() {
			self.exact.insert(index, exact.clone());
		}
		self.captures.push(value.scalar());
		Ok(self.semantic.capture(index)?)
	}
	/// Embed an immutable numerical matrix with explicit signed controls.
	/// Targets and controls must be scalar quantum places; index register handles explicitly.
	/// # Errors
	/// Rejects dimensional mismatch, invalid control states, and foreign operands.
	pub fn matrix(
		&mut self,
		matrix: crate::NumericalOperator,
		targets: &[builder::Qubit],
		controls: &[(builder::Qubit, bool)],
	) -> Result<(), LanguageError> {
		if matrix.num_qubits() != targets.len() {
			return Err(crate::Error::MatrixDimension.into());
		}
		let capture = self.payloads.len();
		let operands = controls
			.iter()
			.map(|(q, _)| q.clone())
			.chain(targets.iter().cloned())
			.collect::<Vec<_>>();
		self.semantic.payload(capture, &operands)?;
		self.payloads.insert(
			capture,
			crate::QuantumPayload::Matrix {
				matrix,
				control_states: controls.iter().map(|(_, state)| *state).collect(),
			},
		);
		Ok(())
	}
	/// Embed a completeness-checked channel as an irreversible SSA effect.
	/// Every target is a scalar quantum place, not an unindexed multi-qubit register.
	/// # Errors
	/// Rejects dimension, completeness, budget, and operand failures.
	pub fn channel(
		&mut self,
		kraus: Vec<crate::NumericalOperator>,
		targets: &[builder::Qubit],
		tolerance: f64,
		policy: crate::MatrixPolicy,
	) -> Result<(), LanguageError> {
		crate::matrix::check_channel(&kraus, tolerance, policy)?;
		if kraus
			.first()
			.is_none_or(|matrix| matrix.num_qubits() != targets.len())
		{
			return Err(crate::Error::MatrixDimension.into());
		}
		let capture = self.payloads.len();
		self.semantic.payload(capture, targets)?;
		self.payloads.insert(
			capture,
			crate::QuantumPayload::Channel {
				kraus: kraus.into(),
			},
		);
		Ok(())
	}
	/// Consume construction through the same checker used for source admission.
	/// # Errors
	/// Rejects type, control-flow, effect, alias, and resource violations.
	pub fn finish(self) -> Result<Program<Constructed>, LanguageError> {
		let typed = self.semantic.finish(CompileLimits::default())?;
		Program::from_template(typed, self.captures, SourceMap::default(), Vec::new())
			.with_angle_captures(self.exact)
			.with_quantum_payloads(self.payloads)
			.with_oracles(self.oracles)
			.map(|program| program.with_embedded_origins(self.origins))
	}
}
impl Deref for ProgramBuilder {
	type Target = builder::Builder;
	fn deref(&self) -> &Self::Target {
		&self.semantic
	}
}
impl DerefMut for ProgramBuilder {
	fn deref_mut(&mut self) -> &mut Self::Target {
		&mut self.semantic
	}
}

impl ProgramBuilder {
	/// Embed a specialized finite region over typed quantum and bit operands.
	/// Exact captures, signed controls, phase, matrices, channels and feedback enter
	/// the same SSA admission as surrounding structured statements.
	/// # Errors
	/// Rejects foreign handles, incompatible types or dimensions, and configured construction limits.
	pub fn region(
		&mut self,
		region: crate::BoundRegion,
		qubits: &[builder::Qubit],
		bits: &[builder::Local<builder::Bit<1>>],
	) -> Result<(), LanguageError> {
		if qubits.len() != region.num_qubits() || bits.len() != region.num_bits() {
			return Err(crate::Error::MatrixDimension.into());
		}
		let mut import = crate::import::Import {
			captures: self.captures.clone(),
			exact: self.exact.clone(),
			oracles: self.oracles.clone(),
			payloads: self.payloads.clone(),
		};
		let mut body = Vec::new();
		for instruction in region.instructions() {
			body.push(import.operation(instruction.operation(), instruction.angle_targets())?);
		}
		self.semantic.append_finite_fragment(body, qubits, bits)?;
		self.captures = import.captures;
		self.exact = import.exact;
		self.oracles = import.oracles;
		self.payloads = import.payloads;
		self.origins.push(std::sync::Arc::new(region));
		Ok(())
	}
}
