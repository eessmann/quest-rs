//! Collective state-vector execution backed by the installed MPI-enabled `QuEST`.
//!
//! Enable Cargo feature `mpi` and select a native build with MPI and SUBCOMM.
//! Every participating rank must call these operations in matching order. Each
//! operation verifies its arguments and all recoverable admission results before
//! native entry. Native failures abort the distributed job because ordered
//! cleanup can no longer be guaranteed. Application messages use a separate
//! rsmpi communicator and can continue after this environment is dropped.
//!
//! The environment borrows its communicator:
//! ```compile_fail
//! use quest::collective::{CollectiveEnvironment, MpiRuntime};
//! let runtime = MpiRuntime::initialize().unwrap();
//! let comm = runtime.world().unwrap();
//! let env = CollectiveEnvironment::builder(&comm).unwrap().build().unwrap();
//! drop(comm);
//! env.state_vector(quest::QubitCount::new(2).unwrap()).unwrap();
//! ```
//! Registers keep their environment alive:
//! ```compile_fail
//! use quest::collective::{CollectiveEnvironment, MpiRuntime};
//! let runtime = MpiRuntime::initialize().unwrap();
//! let comm = runtime.world().unwrap();
//! let env = CollectiveEnvironment::builder(&comm).unwrap().build().unwrap();
//! let mut register = env.state_vector(quest::QubitCount::new(2).unwrap()).unwrap();
//! drop(env);
//! register.init_zero().unwrap();
//! ```
//! Prepared caches also borrow their owner:
//! ```compile_fail
//! use quest::collective::{CollectiveEnvironment, MpiRuntime};
//! let runtime = MpiRuntime::initialize().unwrap();
//! let comm = runtime.world().unwrap();
//! let env = CollectiveEnvironment::builder(&comm).unwrap().build().unwrap();
//! let region = quest::QuantumRegionBuilder::new(2, 0).unwrap().finish().unwrap();
//! let plan = quest::Program::from_region(region, &[]).unwrap()
//!     .verify().unwrap().lower().unwrap().plan().unwrap();
//! let prepared = env.prepare(plan).unwrap();
//! drop(env);
//! let _ = prepared.plan();
//! ```
//! Owners and resources remain on the initializing thread:
//! ```compile_fail
//! use quest::collective::{CollectiveEnvironment, MpiRuntime};
//! let runtime = MpiRuntime::initialize().unwrap();
//! let comm = runtime.world().unwrap();
//! let env = CollectiveEnvironment::builder(&comm).unwrap().build().unwrap();
//! std::thread::scope(|scope| { scope.spawn(move || drop(env)); });
//! ```
use crate::{
	Complex64, EnvironmentView, Error, Executable, MemoryBudget, Outcome, Probability, Program,
	QubitCount, Register, Result, RunOutput, StateVector, environment::RuntimeResources,
	error::BackendResult, values::reserve_vec,
};
use quest_sys::mpi::{MpiCollectiveLane, MpiQuestEnvironment, MpiQuestEnvironmentBuilder};
pub use quest_sys::mpi::{MpiCommunicator, MpiMessageStatus, MpiRuntime, MpiThreadView};
use std::cell::Cell;

/// Builder carrying an admitted power-of-two communicator and threading support.
pub struct CollectiveEnvironmentBuilder<'comm, 'runtime> {
	communicator: &'comm MpiCommunicator<'runtime>,
	native: MpiQuestEnvironmentBuilder<'comm, 'runtime>,
	budget: MemoryBudget,
}
impl<'comm, 'runtime> CollectiveEnvironmentBuilder<'comm, 'runtime> {
	#[must_use]
	pub const fn memory_budget(mut self, budget: MemoryBudget) -> Self {
		self.budget = budget;
		self
	}
	#[must_use]
	pub const fn with_gpu_acceleration(mut self) -> Self {
		self.native = self.native.with_gpu_acceleration();
		self
	}
	#[must_use]
	pub const fn with_multithreading(mut self) -> Self {
		self.native = self.native.with_multithreading();
		self
	}
	/// # Errors
	/// Rejects mismatched configuration or previous initialization on any rank.
	pub fn build(self) -> Result<CollectiveEnvironment<'comm, 'runtime>> {
		let native = self
			.native
			.build()
			.context("initializing collective environment")?;
		let snapshot =
			fatal(|| quest_sys::get_quest_env().context("reading collective environment"));
		Ok(CollectiveEnvironment {
			_native: native,
			resources: RuntimeResources::new_collective(snapshot, self.budget),
			communicator: self.communicator,
			next_id: Cell::new(0),
		})
	}
}

/// Unique collective runtime owner. Its borrowed resources must drop first.
///
/// This owner exposes coherent state-vector execution only. Structured SSA,
/// measurement, reset, noise, distributed solve, and full-state gathering are
/// deliberately outside this interface. Subgroups may execute different plans.
pub struct CollectiveEnvironment<'comm, 'runtime> {
	// Native guard must finalize QuEST before the communicator borrow ends.
	_native: MpiQuestEnvironment<'comm, 'runtime>,
	pub(crate) resources: RuntimeResources,
	communicator: &'comm MpiCommunicator<'runtime>,
	next_id: Cell<u64>,
}
impl<'comm, 'runtime> CollectiveEnvironment<'comm, 'runtime> {
	/// # Errors
	/// Rejects communicator sizes that are not powers of two.
	pub fn builder(
		communicator: &'comm MpiCommunicator<'runtime>,
	) -> Result<CollectiveEnvironmentBuilder<'comm, 'runtime>> {
		let native = communicator
			.quest_environment()
			.context("admitting collective communicator")?;
		Ok(CollectiveEnvironmentBuilder {
			communicator,
			native,
			budget: MemoryBudget::default(),
		})
	}
	#[must_use]
	pub const fn view(&self) -> EnvironmentView<'_> {
		EnvironmentView {
			resources: &self.resources,
		}
	}
	#[must_use]
	pub fn messages(&self) -> MpiThreadView<'_> {
		self.communicator.threaded()
	}
	/// # Errors
	/// Propagates an MPI owner-thread lifecycle error.
	pub fn rank(&self) -> Result<i32> {
		self.communicator.rank().context("reading collective rank")
	}
	/// # Errors
	/// Propagates an MPI owner-thread lifecycle error.
	pub fn size(&self) -> Result<i32> {
		self.communicator.size().context("reading collective size")
	}
	pub(crate) fn identifier(&self) -> u64 {
		let value = self.next_id.get();
		self.next_id.set(
			value
				.checked_add(1)
				.unwrap_or_else(|| quest_sys::mpi::abort_job()),
		);
		value
	}
	pub(crate) fn begin(&self, tag: u64, id: u64, a: u64, b: u64) -> Result<MpiCollectiveLane<'_>> {
		let mut lane = self
			.communicator
			.collective_lane()
			.unwrap_or_else(|_| quest_sys::mpi::abort_job());
		let mut payload = [0_u8; 32];
		for (slot, value) in payload
			.as_chunks_mut::<8>()
			.0
			.iter_mut()
			.zip([tag, id, a, b])
		{
			slot.copy_from_slice(&value.to_le_bytes());
		}
		equal(&mut lane, &payload)?;
		Ok(lane)
	}
	/// Allocate collectively after every rank admits its conservative full-state budget.
	/// # Errors
	/// Rejects mismatched widths or insufficient storage on any rank.
	pub fn state_vector(
		&self,
		count: QubitCount,
	) -> Result<CollectiveRegister<'_, 'comm, 'runtime>> {
		let mut lane = self.begin(
			0,
			self.next_id.get(),
			u64::try_from(count.get()).unwrap_or(u64::MAX),
			0,
		)?;
		let admission =
			if count.dimension() < usize::try_from(self.size()?).map_err(|_| Error::Overflow)? {
				Err(Error::Value(
					"distributed register requires at least one amplitude per rank",
				))
			} else {
				Register::<StateVector>::admit_allocation(&self.resources, count)
			};
		let admission = agree_result(&mut lane, admission)?;
		let inner = fatal(|| Register::allocate_admitted(admission, count));
		Ok(CollectiveRegister {
			inner,
			environment: self,
			id: self.identifier(),
		})
	}
	/// Allocate a forced distributed CPU state using per-rank state/workspace
	/// accounting. Initialization and observation remain local to each partition.
	/// # Errors
	/// Rejects mismatched widths, GPU execution, insufficient local storage or native failures.
	pub fn state_vector_local(
		&self,
		count: QubitCount,
	) -> Result<CollectiveRegister<'_, 'comm, 'runtime>> {
		let mut lane = self.begin(
			40,
			self.next_id.get(),
			u64::try_from(count.get()).unwrap_or(u64::MAX),
			0,
		)?;
		let admission = (|| {
			if self.resources.capabilities().gpu {
				return Err(Error::Unsupported(
					"local-partition state allocation on GPU",
				));
			}
			let ranks = usize::try_from(self.size()?).map_err(|_| Error::Overflow)?;
			if count.dimension() < ranks {
				return Err(Error::Value(
					"distributed register requires an amplitude per rank",
				));
			}
			let entries = count
				.dimension()
				.checked_div(ranks)
				.ok_or(Error::Overflow)?;
			self.resources
				.reserve(crate::values::bytes_for(entries, 4)?)
		})();
		let admission = agree_result(&mut lane, admission)?;
		let inner = fatal(|| Register::allocate_admitted(admission, count));
		Ok(CollectiveRegister {
			inner,
			environment: self,
			id: self.identifier(),
		})
	}
	/// Prepare a common executable program with a static coherent MPI schedule.
	/// Runtime inputs, branching and irreversible quantum effects are rejected
	/// collectively before native allocation.
	/// # Errors
	/// Rejects unsupported effects, unequal payloads and any rank's admission failure.
	pub fn prepare(
		&self,
		program: Program<Executable>,
	) -> Result<CollectivePreparedProgram<'_, 'comm, 'runtime>> {
		let mut lane = self.begin(2, self.next_id.get(), 0, 0)?;
		let output_bytes = agree_result(
			&mut lane,
			crate::output_storage::estimated_bytes(program.ssa()),
		)?;
		let program_bytes = agree_result(
			&mut lane,
			program
				.resources()
				.ir_bytes
				.checked_add(program.resources().source_bytes)
				.and_then(|n| n.checked_add(output_bytes))
				.ok_or(Error::Overflow),
		)?;
		let program_storage = agree_result(&mut lane, self.resources.reserve(program_bytes))?;
		let remaining = self
			.resources
			.memory_budget()
			.bytes()
			.saturating_sub(self.resources.allocated_bytes());
		// Reserve all simultaneous scratch before either VM replay. Region storage
		// includes its bind/plan copies; both encoders enforce their own slice.
		let slice = remaining / 4;
		let (vm_storage, region_storage, payload_storage, output_storage) = agree_result(
			&mut lane,
			(|| {
				Ok((
					self.resources.reserve(slice)?,
					self.resources.reserve(slice)?,
					self.resources.reserve(slice)?,
					self.resources.reserve(slice)?,
				))
			})(),
		)?;
		let limits = crate::InterpreterLimits {
			storage_bytes: slice.min(crate::InterpreterLimits::default().storage_bytes),
			..crate::InterpreterLimits::default()
		};
		let plan = agree_result(
			&mut lane,
			(|| {
				Ok(program
					.try_coherent_region_with_limits(
						limits,
						crate::ProgramLimits::default(),
						slice,
					)?
					.plan()?)
			})(),
		)?;
		agree_result(
			&mut lane,
			crate::collective_admission::admit(
				&plan,
				usize::try_from(self.size()?).map_err(|_| Error::Overflow)?,
				slice,
			),
		)?;
		let output = agree_result(&mut lane, coherent_output(&program, limits))?;
		let payload = agree_result(&mut lane, crate::collective_payload::encode(&plan, slice))?;
		let output_payload =
			agree_result(&mut lane, crate::output_storage::encode(&output, slice))?;
		equal(&mut lane, &payload)?;
		equal(&mut lane, &output_payload)?;
		drop(output_payload);
		drop(output_storage);
		drop(payload);
		drop(payload_storage);
		drop(vm_storage);
		let admission = agree_result(&mut lane, self.resources.admit_plan(plan))?;
		let inner = fatal(|| admission.materialize());
		drop(region_storage);
		Ok(CollectivePreparedProgram {
			inner,
			program,
			output,
			output_bytes,
			_program_storage: program_storage,
			environment: self,
			id: self.identifier(),
		})
	}
}

/// A distributed state vector with collective preflight on every public operation.
pub struct CollectiveRegister<'env, 'comm, 'runtime> {
	pub(crate) inner: Register<'env, StateVector>,
	pub(crate) environment: &'env CollectiveEnvironment<'comm, 'runtime>,
	pub(crate) id: u64,
}
impl CollectiveRegister<'_, '_, '_> {
	/// Replace a bounded slice of this rank's CPU partition. The caller supplies
	/// local amplitudes only; no complete-state broadcast or gather occurs.
	/// # Errors
	/// Rejects nonfinite values, invalid local ranges, GPU execution and budgets collectively.
	pub fn write_local_amplitudes(&mut self, start: usize, values: &[Complex64]) -> Result<()> {
		let mut lane = self.environment.begin(41, self.id, 0, 0)?;
		let admission = (|| {
			if self.deployment().is_gpu_accelerated()
				|| start
					.checked_add(values.len())
					.is_none_or(|end| end > self.deployment().local_amplitudes())
				|| values
					.iter()
					.any(|value| !value.re.is_finite() || !value.im.is_finite())
			{
				return Err(Error::Value("invalid local amplitude slice"));
			}
			let reservation = self
				.environment
				.resources
				.reserve(crate::values::bytes_for(values.len(), 1)?)?;
			let values = crate::register::pack(values.iter().copied(), values.len())?;
			Ok((
				reservation,
				values,
				i64::try_from(start).map_err(|_| Error::Overflow)?,
			))
		})();
		let (_reservation, values, start) = agree_result(&mut lane, admission)?;
		fatal(|| {
			quest_sys::write_local_qureg_amps(self.inner.pin(), start, &values)
				.context("writing collective local partition")
		});
		Ok(())
	}
	/// Read a bounded CPU partition slice, without complete-state replication.
	/// # Errors
	/// Rejects invalid local ranges, GPU execution and budgets collectively.
	pub fn read_local_amplitudes(&self, start: usize, count: usize) -> Result<Vec<Complex64>> {
		let mut lane = self.environment.begin(42, self.id, 0, 0)?;
		let admission = (|| {
			if self.deployment().is_gpu_accelerated()
				|| start
					.checked_add(count)
					.is_none_or(|end| end > self.deployment().local_amplitudes())
			{
				return Err(Error::Value("invalid local amplitude slice"));
			}
			let reservation = self
				.environment
				.resources
				.reserve(crate::values::bytes_for(count, 2)?)?;
			let mut values = reserve_vec(count)?;
			values.resize(count, quest_sys::QuestComplex { re: 0.0, im: 0.0 });
			Ok((
				reservation,
				values,
				i64::try_from(start).map_err(|_| Error::Overflow)?,
			))
		})();
		let (_reservation, mut values, start) = agree_result(&mut lane, admission)?;
		fatal(|| {
			quest_sys::read_local_qureg_amps(&self.inner.native, start, &mut values)
				.context("reading collective local partition")
		});
		Ok(values
			.into_iter()
			.map(|value| Complex64::new(value.re, value.im))
			.collect())
	}
	/// Actual native deployment of this register on the calling rank.
	#[must_use]
	pub const fn deployment(&self) -> crate::RegisterDeployment {
		self.inner.deployment()
	}
	#[must_use]
	pub const fn environment(&self) -> EnvironmentView<'_> {
		self.environment.view()
	}
	#[must_use]
	pub const fn num_qubits(&self) -> QubitCount {
		self.inner.num_qubits()
	}
	/// # Errors
	/// Rejects inconsistent operation order between ranks.
	pub fn init_zero(&mut self) -> Result<()> {
		let _lane = self.environment.begin(3, self.id, 0, 0)?;
		fatal(|| self.inner.init_zero());
		Ok(())
	}
	/// # Errors
	/// Rejects inconsistent operation order between ranks.
	pub fn init_plus(&mut self) -> Result<()> {
		let _lane = self.environment.begin(4, self.id, 0, 0)?;
		fatal(|| self.inner.init_plus());
		Ok(())
	}
	/// Initialize from amplitudes supplied only by `root`. Host broadcast storage
	/// is bounded by every rank's budget and released after initialization.
	/// # Errors
	/// Rejects an invalid root, non-root input, bad length, nonfinite values, or any rank's budget failure.
	pub fn init_pure_from_root(
		&mut self,
		root: i32,
		amplitudes: Option<&[Complex64]>,
	) -> Result<()> {
		let mut lane = self
			.environment
			.begin(5, self.id, u64::from(root.cast_unsigned()), 0)?;
		let count = self.inner.dimension();
		let input = (|| {
			let rank = self.environment.rank()?;
			if root < 0 || root >= self.environment.size()? {
				return Err(Error::Value("invalid collective input root"));
			}
			if rank == root {
				let values = amplitudes.ok_or(Error::Value("root amplitudes are required"))?;
				if values.len() != count
					|| values
						.iter()
						.any(|x| !x.re.is_finite() || !x.im.is_finite())
				{
					return Err(Error::Value(
						"root amplitudes have invalid length or nonfinite entries",
					));
				}
			} else if amplitudes.is_some() {
				return Err(Error::Value("only the root supplies amplitudes"));
			}
			let bytes = count.checked_mul(16).ok_or(Error::Overflow)?;
			let storage = self
				.environment
				.resources
				.reserve(bytes.checked_mul(2).ok_or(Error::Overflow)?)?;
			let mut wire = reserve_vec(bytes)?;
			wire.resize(bytes, 0);
			let mut native = reserve_vec(count)?;
			if let Some(values) = amplitudes {
				for (value, slot) in values.iter().zip(wire.as_chunks_mut::<16>().0.iter_mut()) {
					let (re, im) = slot.split_at_mut(8);
					re.copy_from_slice(&value.re.to_bits().to_le_bytes());
					im.copy_from_slice(&value.im.to_bits().to_le_bytes());
				}
			}
			native.resize(count, quest_sys::QuestComplex { re: 0., im: 0. });
			Ok((storage, wire, native))
		})();
		let (_storage, mut wire, mut native) = agree_result(&mut lane, input)?;
		for chunk in wire.chunks_mut(8192) {
			lane.broadcast_bytes(root, chunk)
				.context("broadcasting collective input")?;
		}
		for (slot, value) in native.iter_mut().zip(wire.as_chunks::<16>().0.iter()) {
			let mut re = [0_u8; 8];
			let mut im = [0_u8; 8];
			let (re_bytes, im_bytes) = value.split_at(8);
			re.copy_from_slice(re_bytes);
			im.copy_from_slice(im_bytes);
			*slot = quest_sys::QuestComplex {
				re: f64::from_bits(u64::from_le_bytes(re)),
				im: f64::from_bits(u64::from_le_bytes(im)),
			};
		}
		fatal(|| {
			quest_sys::init_arbitrary_pure_state(self.inner.pin(), &native)
				.context("initializing collective pure state")
		});
		Ok(())
	}
	/// # Errors
	/// Rejects inconsistent operation order between ranks.
	pub fn total_probability(&self) -> Result<f64> {
		let _lane = self.environment.begin(6, self.id, 0, 0)?;
		Ok(fatal(|| self.inner.total_probability()))
	}
	/// # Errors
	/// Rejects mismatched or invalid qubit/outcome arguments.
	pub fn probability(&self, qubit: usize, outcome: Outcome) -> Result<Probability> {
		let mut lane = self.environment.begin(
			7,
			self.id,
			u64::try_from(qubit).unwrap_or(u64::MAX),
			u64::from(outcome.as_bool()),
		)?;
		agree_result(&mut lane, self.inner.check_qubit(qubit))?;
		Ok(fatal(|| self.inner.probability(qubit, outcome)))
	}
	/// Apply an unnormalized one-qubit projector collectively; norm squared is
	/// available through `total_probability` for postselection accounting.
	/// # Errors
	/// Rejects mismatched or invalid qubit/outcome arguments.
	pub fn project(&mut self, qubit: usize, outcome: Outcome) -> Result<()> {
		let mut lane = self.environment.begin(
			8,
			self.id,
			u64::try_from(qubit).unwrap_or(u64::MAX),
			u64::from(outcome.as_bool()),
		)?;
		let qubit = agree_result(&mut lane, self.inner.check_qubit(qubit))?;
		fatal(|| {
			quest_sys::apply_qubit_projector(self.inner.pin(), qubit, i32::from(outcome.as_bool()))
				.context("projecting collective register")
		});
		Ok(())
	}
}

/// Immutable coherent plan and native caches borrowing their collective owner.
pub struct CollectivePreparedProgram<'env, 'comm, 'runtime> {
	inner: crate::execution::PreparedRegion<'env>,
	program: Program<Executable>,
	output: RunOutput,
	output_bytes: usize,
	_program_storage: crate::environment::Reservation<'env>,
	pub(crate) environment: &'env CollectiveEnvironment<'comm, 'runtime>,
	pub(crate) id: u64,
}
impl CollectivePreparedProgram<'_, '_, '_> {
	#[must_use]
	pub const fn plan(&self) -> &Program<Executable> {
		&self.program
	}
	#[must_use]
	pub const fn prepared_oracle_bodies(&self) -> usize {
		self.inner.prepared_oracle_bodies()
	}
	/// # Errors
	/// Rejects different prepared/register identities or changed execution admission on any rank.
	pub fn run(&mut self, register: &mut CollectiveRegister<'_, '_, '_>) -> Result<RunOutput> {
		let mut lane = self.environment.begin(9, self.id, register.id, 0)?;
		let bits = agree_result(&mut lane, self.inner.admit_run(&register.inner))?;
		let _output_storage = agree_result(
			&mut lane,
			self.environment.resources.reserve(self.output_bytes),
		)?;
		let output = self.output.clone();
		fatal(|| self.inner.run_admitted(&mut register.inner, bits));
		Ok(output)
	}
}
/// Static coherent admission has already rejected every outcome-dependent effect.
/// Replay classical instructions to retain the common output and step accounting.
fn coherent_output(
	program: &Program<Executable>,
	limits: crate::InterpreterLimits,
) -> Result<RunOutput> {
	use crate::language::vm::{self, QuantumBackend};
	struct Transcript;
	impl QuantumBackend for Transcript {
		type Error = Error;
		fn apply_gate(&mut self, _: vm::GateRequest<'_>) -> Result<()> {
			Ok(())
		}
		fn apply_oracle(&mut self, _: vm::OracleRequest<'_>) -> Option<Result<()>> {
			Some(Ok(()))
		}
		fn apply_payload(&mut self, _: usize, _: &[usize]) -> Option<Result<()>> {
			Some(Ok(()))
		}
		fn measure(&mut self, _: usize) -> Result<bool> {
			Err(Error::Unsupported("collective measurement"))
		}
		fn reset(&mut self, _: usize) -> Result<()> {
			Err(Error::Unsupported("collective reset"))
		}
		fn barrier(&mut self, _: &[usize]) -> Result<()> {
			Ok(())
		}
	}
	vm::Interpreter::new(limits)
		.run_prepared(
			program.ssa(),
			program.dispatch(),
			&mut Transcript,
			&vm::RunInputs::default(),
			program.captures(),
		)
		.map_err(|_| Error::Value("static collective program failed classical admission"))
}
fn fatal<T>(operation: impl FnOnce() -> Result<T>) -> T {
	// Any unwind after native entry would leave peers in a different collective
	// schedule. Contain it here, before owner unwinding can finalize QuEST.
	std::panic::catch_unwind(std::panic::AssertUnwindSafe(operation))
		.unwrap_or_else(|_| quest_sys::mpi::abort_job())
		.unwrap_or_else(|error| {
			eprintln!("collective native operation failed: {error}");
			quest_sys::mpi::abort_job()
		})
}
fn agree_result<T>(lane: &mut MpiCollectiveLane<'_>, value: Result<T>) -> Result<T> {
	if !lane
		.all_agree(value.is_ok())
		.context("agreeing collective admission")?
	{
		return Err(Error::Value(
			"collective preflight rejected on one or more ranks",
		));
	}
	value
}
pub(crate) fn equal(lane: &mut MpiCollectiveLane<'_>, bytes: &[u8]) -> Result<()> {
	let local_len = u64::try_from(bytes.len())
		.map_err(|_| Error::Overflow)?
		.to_le_bytes();
	let mut root_len = local_len;
	lane.broadcast_bytes(0, &mut root_len)
		.context("comparing collective payload length")?;
	if !lane
		.all_agree(root_len == local_len)
		.context("agreeing collective payload length")?
	{
		return Err(Error::Value("collective payloads differ between ranks"));
	}
	let mut buffer = [0_u8; 8192];
	let mut matching = true;
	for chunk in bytes.chunks(buffer.len()) {
		let wire = buffer
			.get_mut(..chunk.len())
			.unwrap_or_else(|| quest_sys::mpi::abort_job());
		wire.copy_from_slice(chunk);
		lane.broadcast_bytes(0, wire)
			.context("comparing collective payload")?;
		matching &= wire == chunk;
	}
	if !lane
		.all_agree(matching)
		.context("agreeing collective payload")?
	{
		return Err(Error::Value("collective payloads differ between ranks"));
	}
	Ok(())
}
