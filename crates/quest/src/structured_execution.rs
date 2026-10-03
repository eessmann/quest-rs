//! Environment-bound preparation and native dispatch for structured SSA.
use crate::oracle_execution::{OracleCache, OracleInventory};
use crate::{
	Environment, Error, Outcome, QubitCount, Register, RegisterKind, Result,
	environment::Reservation,
	error::{BackendResult, StructuredExecutionError},
	execution::{NativeControls, admit_fingerprint, apply_gate, phase, reset_channel},
	values::reserve_vec,
};
use cxx::UniquePtr;
use quest_compile::{
	BoundGate, Executable, Program,
	dispatch_recipe::{self, GateRecipe},
	language::{
		Adjoint, GateKind,
		ssa::InstructionKind,
		vm::{self, GateRequest, QuantumBackend},
	},
};

/// A prepared structured interpreter. Native reset resources and scratch buffers
/// are admitted before publication and dropped before the owning environment.
pub struct PreparedProgram<'env> {
	matrices: Vec<crate::execution::NativeMatrix>,
	reset: Option<UniquePtr<quest_sys::KrausMap>>,
	payloads: crate::payload_execution::PayloadCache,
	static_gates: std::collections::BTreeMap<vm::DispatchId, StaticGate>,
	oracles: OracleCache,
	oracle_ids: std::collections::BTreeMap<usize, (usize, bool)>,
	controls: NativeControls,
	targets: Vec<i32>,
	reservation: Reservation<'env>,
	plan: Program<Executable>,
	fingerprint: quest_sys::NumericalFingerprint,
}
/// Results of independent zero-initialized executions, using the same result
/// model as `run`. Keeping every run preserves feedback paths and step counts.
#[derive(Debug)]
pub struct SampleResult {
	pub runs: Vec<vm::RunOutput>,
	pub seeds: Vec<u32>,
}
impl Environment {
	/// Transfer an independently verified structured plan into environment-bound resources.
	///
	/// # Errors
	/// Rejects native index, configuration and memory limits transactionally.
	pub fn prepare(&self, plan: Program<Executable>) -> Result<PreparedProgram<'_>> {
		QubitCount::new(plan.num_qubits())?;
		let fingerprint =
			quest_sys::get_numerical_fingerprint().context("checking structured environment")?;
		admit_fingerprint(&fingerprint)?;
		let reset_needed = plan
			.ssa()
			.blocks()
			.iter()
			.flat_map(|block| &block.instructions)
			.any(|instruction| matches!(instruction.kind, InstructionKind::Reset { .. }));
		let scratch = plan.num_qubits().checked_mul(32).ok_or(Error::Overflow)?;
		let inventory = OracleInventory::from_structured(
			&plan,
			self.resources
				.memory_budget()
				.bytes()
				.saturating_sub(self.resources.allocated_bytes()),
		)?;
		let mut seen_matrices = std::collections::BTreeSet::new();
		let oracle_bytes = inventory
			.estimated_bytes(
				plan.num_qubits(),
				self.resources.capabilities().gpu,
				&mut seen_matrices,
			)?
			.checked_add(
				plan.oracle_captures()
					.len()
					.checked_mul(128)
					.ok_or(Error::Overflow)?,
			)
			.ok_or(Error::Overflow)?;
		let static_bytes = static_storage(&plan)?;
		let payload_bytes = crate::payload_execution::PayloadCache::estimated_bytes(
			plan.quantum_payloads(),
			self.resources.capabilities().gpu,
			&mut seen_matrices,
		)?;
		let required = plan
			.resources()
			.ir_bytes
			.checked_add(plan.resources().source_bytes)
			.and_then(|n| n.checked_add(oracle_bytes))
			.and_then(|n| n.checked_add(scratch))
			.and_then(|n| n.checked_add(payload_bytes))
			.and_then(|n| n.checked_add(static_bytes))
			.and_then(|n| n.checked_add(if reset_needed { 4096 } else { 0 }))
			.ok_or(Error::Overflow)?;
		let reservation = self.resources.reserve(required)?;
		let static_gates = plan
			.dispatch()
			.gates()
			.map(|(id, request)| Ok((id, StaticGate::prepare(request, plan.num_qubits())?)))
			.collect::<Result<_>>()?;
		drop(seen_matrices);
		let mut matrices = crate::execution::MatrixPreparation::default();
		let payloads = crate::payload_execution::PayloadCache::prepare(
			plan.quantum_payloads(),
			&mut matrices,
		)?;
		let oracles = OracleCache::prepare(&inventory, plan.num_qubits(), &mut matrices)?;
		// Unreachable captures remain owned by the source plan without native resources.
		let oracle_ids = plan
			.oracle_captures()
			.iter()
			.filter_map(|(id, fragment)| {
				inventory
					.index(fragment)
					.ok()
					.map(|index| (*id, (index, fragment.is_adjoint())))
			})
			.collect();
		let capacity = plan.num_qubits();
		let controls = NativeControls::with_capacity(capacity, false)?;
		let targets = reserve_vec(capacity)?;
		let reset = if reset_needed {
			Some(reset_channel()?)
		} else {
			None
		};
		Ok(PreparedProgram {
			matrices: matrices.finish(),
			reset,
			payloads,
			static_gates,
			oracles,
			oracle_ids,
			controls,
			targets,
			reservation,
			plan,
			fingerprint,
		})
	}
}
fn static_storage(plan: &Program<Executable>) -> Result<usize> {
	plan.dispatch()
		.gates()
		.try_fold(0usize, |total, (_, request)| {
			let operands = request
				.targets
				.len()
				.checked_mul(size_of::<i32>())
				.and_then(|n| {
					request
						.controls
						.len()
						.checked_mul(32)
						.and_then(|controls| n.checked_add(controls))
				})
				.and_then(|n| n.checked_add(const { size_of::<StaticGate>() + 128 }))
				.ok_or(Error::Overflow)?;
			total.checked_add(operands).ok_or(Error::Overflow)
		})
}
impl PreparedProgram<'_> {
	/// Seed the process RNG once and execute each shot from the zero state.
	/// Inputs remain fixed across shots; runtime feedback is evaluated per shot.
	///
	/// # Errors
	/// Rejects invalid seed counts, numerical configuration changes, allocation
	/// limits, and execution failures. Returns no partial batch after a failure.
	pub fn sample_zeroed(
		&mut self,
		shots: crate::Shots,
		seeds: &[u32],
		inputs: &vm::RunInputs,
	) -> Result<SampleResult> {
		if seeds.is_empty() || seeds.len() > 16 {
			return Err(Error::Value(
				"sampling requires between 1 and 16 explicit RNG seeds",
			));
		}
		let fingerprint =
			quest_sys::get_numerical_fingerprint().context("checking sample environment")?;
		if fingerprint != self.fingerprint {
			return Err(Error::ConfigurationChanged);
		}
		let environment = self.reservation.environment;
		let bytes = crate::output_storage::estimated_bytes(self.plan.ssa())?
			.checked_mul(shots.get())
			.and_then(|n| {
				seeds
					.len()
					.checked_mul(size_of::<u32>())
					.and_then(|seed_bytes| n.checked_add(seed_bytes))
			})
			.ok_or(Error::Overflow)?;
		let _storage = environment.reserve(bytes)?;
		let mut runs = reserve_vec(shots.get())?;
		let mut recorded_seeds = reserve_vec(seeds.len())?;
		recorded_seeds.extend_from_slice(seeds);
		environment.admit_seed_storage()?;
		quest_sys::set_qu_est_seeds(seeds).context("seeding sample batch")?;
		let count = QubitCount::new(self.plan.num_qubits())?;
		if self.payloads.requires_density() {
			let mut register = environment.density_matrix(count)?;
			for _ in 0..shots.get() {
				register.init_zero()?;
				runs.push(self.run(&mut register, inputs)?);
			}
		} else {
			let mut register = environment.state_vector(count)?;
			for _ in 0..shots.get() {
				register.init_zero()?;
				runs.push(self.run(&mut register, inputs)?);
			}
		}
		Ok(SampleResult {
			runs,
			seeds: recorded_seeds,
		})
	}
	/// Static built-in dispatch records resolved and lowered once during preparation.
	#[must_use]
	pub fn prepared_static_gates(&self) -> usize {
		self.static_gates.len()
	}

	/// Distinct shared canonical oracle bodies retained in native preparation.
	#[must_use]
	pub const fn prepared_oracle_bodies(&self) -> usize {
		self.oracles.body_count()
	}
	/// Distinct numerical/control variants used by reachable oracle bodies.
	#[must_use]
	pub const fn prepared_oracle_matrix_variants(&self) -> usize {
		self.oracles.matrix_count()
	}
	/// Native forward/adjoint pairs shared by payloads and reachable oracle bodies.
	#[must_use]
	pub const fn prepared_matrix_variants(&self) -> usize {
		self.matrices.len()
	}
	#[must_use]
	pub const fn plan(&self) -> &Program<Executable> {
		&self.plan
	}
	/// Execute against the current register state with default bounded limits.
	///
	/// # Errors
	/// Reports configuration, input or runtime failure with the completed quantum prefix.
	pub fn run<K: RegisterKind>(
		&mut self,
		register: &mut Register<'_, K>,
		inputs: &vm::RunInputs,
	) -> Result<vm::RunOutput> {
		self.run_with_limits(register, inputs, vm::InterpreterLimits::default())
	}
	/// Execute with explicit step, call-frame and storage budgets.
	///
	/// # Errors
	/// Rejects resource admission before execution; later failures retain completed effects.
	pub fn run_with_limits<K: RegisterKind>(
		&mut self,
		register: &mut Register<'_, K>,
		inputs: &vm::RunInputs,
		mut limits: vm::InterpreterLimits,
	) -> Result<vm::RunOutput> {
		if !std::ptr::eq(register.resources(), self.reservation.environment)
			|| register.num_qubits().get() != self.plan.num_qubits()
			|| (!register.is_density() && self.payloads.requires_density())
		{
			return Err(Error::RegisterMismatch);
		}
		let fingerprint = quest_sys::get_numerical_fingerprint()
			.context("checking structured execution environment")?;
		if fingerprint != self.fingerprint {
			return Err(Error::ConfigurationChanged);
		}
		// Limits are upper bounds. Programs with no calls need one frame, and
		// the interpreter must share the environment's remaining memory budget.
		if !self
			.plan
			.ssa()
			.blocks()
			.iter()
			.flat_map(|block| &block.instructions)
			.any(|instruction| matches!(instruction.kind, InstructionKind::Call { .. }))
		{
			limits.call_frames = limits.call_frames.min(1);
		}
		limits.storage_bytes = limits.storage_bytes.min(
			self.reservation
				.environment
				.memory_budget()
				.bytes()
				.saturating_sub(self.reservation.environment.allocated_bytes()),
		);
		let _run_storage = self.reservation.environment.reserve(limits.storage_bytes)?;
		let mut backend = Backend {
			register,
			reset: self.reset.as_ref(),
			matrices: &self.matrices,
			payloads: &self.payloads,
			static_gates: &self.static_gates,
			oracles: &mut self.oracles,
			oracle_ids: &self.oracle_ids,
			controls: &mut self.controls,
			targets: &mut self.targets,
		};
		vm::Interpreter::new(limits)
			.run_prepared(
				self.plan.ssa(),
				self.plan.dispatch(),
				&mut backend,
				inputs,
				self.plan.captures(),
			)
			.map_err(|error| {
				let diagnostic = runtime_diagnostic(&error, &self.plan);
				Error::StructuredExecution(Box::new(StructuredExecutionError::new(
					error, diagnostic,
				)))
			})
	}
}
fn runtime_diagnostic(
	error: &vm::RuntimeError<Error>,
	plan: &Program<Executable>,
) -> quest_compile::language::Diagnostic {
	let mut diagnostic = error.diagnostic(plan.sources());
	if diagnostic.labels.is_empty()
		&& let Some(span) = diagnostic.occurrence
		&& let Some(location) = plan
			.locations()
			.iter()
			.find(|location| location.span == span)
	{
		diagnostic.notes.push(format!(
			"Rust source location: {}:{}:{}",
			location.file, location.line, location.column
		));
	}
	diagnostic
}
struct Backend<'a, 'env, K: RegisterKind> {
	register: &'a mut Register<'env, K>,
	reset: Option<&'a UniquePtr<quest_sys::KrausMap>>,
	matrices: &'a [crate::execution::NativeMatrix],
	payloads: &'a crate::payload_execution::PayloadCache,
	static_gates: &'a std::collections::BTreeMap<vm::DispatchId, StaticGate>,
	oracles: &'a mut OracleCache,
	oracle_ids: &'a std::collections::BTreeMap<usize, (usize, bool)>,
	controls: &'a mut NativeControls,
	targets: &'a mut Vec<i32>,
}
impl<K: RegisterKind> QuantumBackend for Backend<'_, '_, K> {
	type Error = Error;
	fn apply_prepared_gate(&mut self, id: vm::DispatchId, _request: GateRequest<'_>) -> Result<()> {
		let gate = self
			.static_gates
			.get(&id)
			.ok_or(Error::Value("missing static dispatch"))?;
		for step in gate.recipe.steps() {
			crate::execution::execute_step(self.register, step, &gate.targets, &gate.controls)?;
		}
		Ok(())
	}
	fn apply_payload(&mut self, capture: usize, wires: &[usize]) -> Option<Result<()>> {
		Some(
			self.payloads
				.execute(capture, wires, self.register, self.targets, self.matrices),
		)
	}
	fn apply_oracle(&mut self, request: vm::OracleRequest<'_>) -> Option<Result<()>> {
		Some((|| {
			let &(body, adjoint) = self
				.oracle_ids
				.get(&request.capture)
				.ok_or(Error::Value("unprepared structured oracle"))?;
			self.oracles.run(
				body,
				request.targets,
				request.controls,
				adjoint ^ request.adjoint,
				self.register,
				self.matrices,
			)
		})())
	}
	fn apply_gate(&mut self, request: GateRequest<'_>) -> Result<()> {
		self.targets.clear();
		for target in request.targets {
			if self.targets.len() == self.targets.capacity() {
				return Err(Error::Value("target scratch capacity exceeded"));
			}
			self.targets.push(self.register.check_qubit(*target)?);
		}
		self.controls.load(
			request
				.controls
				.iter()
				.map(|control| Ok((self.register.check_qubit(control.qubit)?, control.positive))),
			self.targets.first().copied(),
		)?;
		let (kind, parameters) = adjoint_parameters(request)?;
		if kind == GateKind::GlobalPhase {
			return phase(self.register, parameter(&parameters, 0)?, self.controls);
		}
		let gate = bound_gate(kind, &parameters)?;
		apply_gate(self.register, &gate, self.targets, self.controls)
	}
	fn measure(&mut self, qubit: usize) -> Result<bool> {
		Ok(self.register.measure(qubit)?.as_bool())
	}
	fn reset(&mut self, qubit: usize) -> Result<()> {
		let target = self.register.check_qubit(qubit)?;
		if self.register.is_density() {
			let reset = self
				.reset
				.ok_or(Error::Value("missing prepared reset map"))?;
			quest_sys::mix_kraus_map(self.register.pin(), &[target], reset)
				.context("resetting structured density qubit")
		} else {
			if self.register.measure(qubit)? == Outcome::One {
				self.register.x(qubit)?;
			}
			Ok(())
		}
	}
	fn barrier(&mut self, qubits: &[usize]) -> Result<()> {
		for qubit in qubits {
			self.register.check_qubit(*qubit)?;
		}
		Ok(())
	}
}
fn parameter(values: &[f64], index: usize) -> Result<f64> {
	values
		.get(index)
		.copied()
		.filter(|value| value.is_finite())
		.ok_or(Error::Value("missing or nonfinite gate parameter"))
}
fn adjoint_parameters(request: GateRequest<'_>) -> Result<(GateKind, [f64; 3])> {
	let mut result = [0.; 3];
	let mut kind = request.gate;
	if request.parameters.len() != kind.definition().parameter_count {
		return Err(Error::Value("gate parameter arity"));
	}
	if request.inverse {
		match kind.definition().adjoint {
			Adjoint::Gate(gate) => kind = gate,
			Adjoint::Parameters(mapping) => {
				for (target, mapping) in result.iter_mut().zip(mapping) {
					let value = parameter(request.parameters, mapping.input)?;
					*target = if mapping.negate { -value } else { value };
				}
				return Ok((kind, result));
			}
			Adjoint::SelfInverse => {}
		}
	}
	for (target, value) in result.iter_mut().zip(request.parameters) {
		*target = *value;
	}
	Ok((kind, result))
}
fn bound_gate(kind: GateKind, parameters: &[f64]) -> Result<BoundGate> {
	let parameters = parameters
		.get(..kind.definition().parameter_count)
		.ok_or(Error::Value("gate parameter arity"))?;
	Ok(BoundGate::from_kind(kind, parameters)?)
}

struct StaticGate {
	recipe: GateRecipe,
	targets: Vec<i32>,
	controls: NativeControls,
}
impl StaticGate {
	fn prepare(request: GateRequest<'_>, qubits: usize) -> Result<Self> {
		let check = |wire: usize| {
			if wire >= qubits {
				return Err(Error::RegisterMismatch);
			}
			i32::try_from(wire).map_err(|_| Error::Overflow)
		};
		let targets = request
			.targets
			.iter()
			.map(|&wire| check(wire))
			.collect::<Result<Vec<_>>>()?;
		let (kind, parameters) = adjoint_parameters(request)?;
		let mut controls =
			NativeControls::with_capacity(request.controls.len(), !targets.is_empty())?;
		controls.load(
			request
				.controls
				.iter()
				.map(|control| Ok((check(control.qubit)?, control.positive))),
			if kind == GateKind::GlobalPhase {
				None
			} else {
				targets.first().copied()
			},
		)?;
		let recipe = if kind == GateKind::GlobalPhase {
			dispatch_recipe::scalar_phase_recipe(parameter(&parameters, 0)?, controls.zeros.len())?
		} else {
			dispatch_recipe::gate_recipe(&bound_gate(kind, &parameters)?, controls.zeros.len())?
		};
		Ok(Self {
			recipe,
			targets,
			controls,
		})
	}
}
