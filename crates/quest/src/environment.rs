use crate::error::BackendResult;
use crate::{DensityMatrix, Error, MemoryBudget, QubitCount, Register, Result, StateVector};
use std::{cell::Cell, fmt, marker::PhantomData, rc::Rc};

/// Selection of native environment features.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExecutionMode {
    Auto,
    Enabled,
    Disabled,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[expect(
    clippy::struct_excessive_bools,
    reason = "Independent native feature flags are a capability snapshot, not mutually exclusive states"
)]
pub struct Capabilities {
    pub gpu: bool,
    pub multithreaded: bool,
    pub distributed: bool,
    pub cu_quantum: bool,
}

pub struct EnvironmentBuilder {
    gpu: ExecutionMode,
    threads: ExecutionMode,
    distribution: ExecutionMode,
    budget: MemoryBudget,
}
impl Default for EnvironmentBuilder {
    fn default() -> Self {
        Self {
            gpu: ExecutionMode::Disabled,
            threads: ExecutionMode::Disabled,
            distribution: ExecutionMode::Disabled,
            budget: MemoryBudget::default(),
        }
    }
}
impl EnvironmentBuilder {
    #[must_use]
    pub const fn gpu(mut self, mode: ExecutionMode) -> Self {
        self.gpu = mode;
        self
    }
    #[must_use]
    pub const fn multithreading(mut self, mode: ExecutionMode) -> Self {
        self.threads = mode;
        self
    }
    #[must_use]
    pub const fn distribution(mut self, mode: ExecutionMode) -> Self {
        self.distribution = mode;
        self
    }
    #[must_use]
    pub const fn memory_budget(mut self, budget: MemoryBudget) -> Self {
        self.budget = budget;
        self
    }
    /// Enter native initialization at most once per process.
    ///
    /// Configuration checks before native entry do not consume this attempt.
    /// Once native initialization starts, even a failure permanently prevents
    /// another attempt. Dropping the returned owner ends this runtime forever.
    ///
    /// # Errors
    /// Rejects unsupported distribution, unavailable native modes, previous
    /// initialization attempts, or native environment initialization failures.
    pub fn build(self) -> Result<Environment> {
        // The initial runtime has no collective allocation/error protocol.
        if self.distribution != ExecutionMode::Disabled {
            return Err(Error::Unsupported("distributed runtime resources"));
        }
        let native_mode = |mode| match mode {
            ExecutionMode::Auto => -1,
            ExecutionMode::Enabled => 1,
            ExecutionMode::Disabled => 0,
        };
        quest_sys::init_custom_quest_env_modes(0, native_mode(self.gpu), native_mode(self.threads))
            .context("initializing environment")?;
        let native = match quest_sys::get_quest_env().context("reading environment") {
            Ok(value) => value,
            Err(error) => {
                // Initialization succeeded, so this builder owns cleanup. A
                // rejected initialization above must never retire another owner.
                quest_sys::finalize_quest_env_on_drop();
                return Err(error);
            }
        };
        Ok(Environment {
            resources: RuntimeResources::new(native, self.budget),
        })
    }
}

/// Unique owner of a native runtime, confined to its creating thread.
///
/// Registers and prepared programs (including their native matrices and
/// channels) borrow this owner and are destroyed before it. Scope exit finalizes
/// the runtime automatically. Initialization can only be attempted once per
/// process: `QuEST` may own MPI, whose world model cannot restart after
/// finalization. There is no explicit high-level shutdown operation.
///
/// Drop never panics. If native cleanup fails or an independently retained
/// low-level handle prevents it, the bridge permanently retires the runtime and
/// rejects subsequent native operations. The process can continue; allocations
/// that cannot safely be destroyed remain until process exit.
///
/// Owned register snapshots and pure Rust [`crate::NumericalOperator`] payloads
/// do not borrow this owner and remain usable after its scope ends. RAII cannot
/// ensure cleanup on process abort, forced termination, or [`std::mem::forget`].
pub struct Environment {
    pub(crate) resources: RuntimeResources,
}

pub struct RuntimeResources {
    capabilities: Capabilities,
    budget: MemoryBudget,
    allocated: Cell<usize>,
    seed_storage: Cell<bool>,
    thread: PhantomData<Rc<()>>,
}
impl Environment {
    #[must_use]
    pub fn builder() -> EnvironmentBuilder {
        EnvironmentBuilder::default()
    }
    pub const fn capabilities(&self) -> Capabilities {
        self.resources.capabilities
    }
    pub const fn memory_budget(&self) -> MemoryBudget {
        self.resources.budget
    }
    pub const fn allocated_bytes(&self) -> usize {
        self.resources.allocated.get()
    }
    /// # Errors
    /// Rejects allocation overflow, insufficient memory budget, or native allocation failure.
    pub fn state_vector(&self, count: QubitCount) -> Result<Register<'_, StateVector>> {
        Register::allocate(&self.resources, count)
    }
    /// # Errors
    /// Rejects allocation overflow, insufficient memory budget, or native allocation failure.
    pub fn density_matrix(&self, count: QubitCount) -> Result<Register<'_, DensityMatrix>> {
        Register::allocate(&self.resources, count)
    }
}

/// Read-only capabilities and resource accounting shared by local and collective owners.
/// This view cannot allocate resources or extend the runtime lifetime.
#[derive(Clone, Copy)]
pub struct EnvironmentView<'env> {
    pub(crate) resources: &'env RuntimeResources,
}
impl EnvironmentView<'_> {
    #[must_use]
    pub const fn capabilities(&self) -> Capabilities {
        self.resources.capabilities
    }
    #[must_use]
    pub const fn memory_budget(&self) -> MemoryBudget {
        self.resources.budget
    }
    #[must_use]
    pub const fn allocated_bytes(&self) -> usize {
        self.resources.allocated.get()
    }
}
impl RuntimeResources {
    pub(crate) const fn new(native: quest_sys::QuestEnvironment, budget: MemoryBudget) -> Self {
        Self {
            capabilities: Capabilities {
                gpu: native.is_gpu_accelerated,
                multithreaded: native.is_multithreaded,
                distributed: native.is_distributed,
                cu_quantum: native.is_cu_quantum_enabled,
            },
            budget,
            allocated: Cell::new(0),
            seed_storage: Cell::new(false),
            thread: PhantomData,
        }
    }
    pub(crate) const fn capabilities(&self) -> Capabilities {
        self.capabilities
    }
    pub(crate) const fn memory_budget(&self) -> MemoryBudget {
        self.budget
    }
    pub(crate) const fn allocated_bytes(&self) -> usize {
        self.allocated.get()
    }
    pub(crate) fn state_vector(&self, count: QubitCount) -> Result<Register<'_, StateVector>> {
        Register::allocate(self, count)
    }
    pub(crate) fn density_matrix(&self, count: QubitCount) -> Result<Register<'_, DensityMatrix>> {
        Register::allocate(self, count)
    }
    /// Retain a conservative fixed allowance for `QuEST`'s process RNG seed
    /// storage after the first high-level batch, until this owner is destroyed.
    pub(crate) fn admit_seed_storage(&self) -> Result<()> {
        const BYTES: usize = 4096;
        if self.seed_storage.get() {
            return Ok(());
        }
        let available = self.budget.bytes().saturating_sub(self.allocated.get());
        if available < BYTES {
            return Err(Error::Budget {
                requested: BYTES,
                available,
            });
        }
        self.allocated.set(
            self.allocated
                .get()
                .checked_add(BYTES)
                .ok_or(Error::Overflow)?,
        );
        self.seed_storage.set(true);
        Ok(())
    }
    pub(crate) fn reserve(&self, bytes: usize) -> Result<Reservation<'_>> {
        let available = self.budget.bytes().saturating_sub(self.allocated.get());
        if bytes > available {
            return Err(Error::Budget {
                requested: bytes,
                available,
            });
        }
        self.allocated.set(
            self.allocated
                .get()
                .checked_add(bytes)
                .ok_or(Error::Overflow)?,
        );
        Ok(Reservation {
            environment: self,
            bytes,
        })
    }
}
impl Drop for Environment {
    fn drop(&mut self) {
        quest_sys::finalize_quest_env_on_drop();
    }
}
impl fmt::Debug for Environment {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Environment")
            .field("capabilities", &self.resources.capabilities)
            .field("allocated_bytes", &self.resources.allocated.get())
            .finish_non_exhaustive()
    }
}

pub struct Reservation<'a> {
    pub(crate) environment: &'a RuntimeResources,
    bytes: usize,
}
// Each reservation releases exactly the bytes charged at construction.
impl Drop for Reservation<'_> {
    fn drop(&mut self) {
        if let Some(remaining) = self.environment.allocated.get().checked_sub(self.bytes) {
            self.environment.allocated.set(remaining);
        }
    }
}
