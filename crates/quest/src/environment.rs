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
    pub fn gpu(mut self, mode: ExecutionMode) -> Self {
        self.gpu = mode;
        self
    }
    pub fn multithreading(mut self, mode: ExecutionMode) -> Self {
        self.threads = mode;
        self
    }
    pub fn distribution(mut self, mode: ExecutionMode) -> Self {
        self.distribution = mode;
        self
    }
    pub fn memory_budget(mut self, budget: MemoryBudget) -> Self {
        self.budget = budget;
        self
    }
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
                let _ = quest_sys::finalize_quest_env();
                return Err(error);
            }
        };
        Ok(Environment {
            active: true,
            capabilities: Capabilities {
                gpu: native.is_gpu_accelerated,
                multithreaded: native.is_multithreaded,
                distributed: native.is_distributed,
                cu_quantum: native.is_cu_quantum_enabled,
            },
            budget: self.budget,
            allocated: Cell::new(0),
            seed_storage: Cell::new(false),
            thread: PhantomData,
        })
    }
}

/// Unique active native environment, restricted to its creating thread.
/// Registers, native matrices and prepared programs borrow this owner.
pub struct Environment {
    active: bool,
    capabilities: Capabilities,
    budget: MemoryBudget,
    allocated: Cell<usize>,
    seed_storage: Cell<bool>,
    thread: PhantomData<Rc<()>>,
}
impl Environment {
    pub fn builder() -> EnvironmentBuilder {
        EnvironmentBuilder::default()
    }
    pub fn capabilities(&self) -> Capabilities {
        self.capabilities
    }
    pub fn memory_budget(&self) -> MemoryBudget {
        self.budget
    }
    pub fn allocated_bytes(&self) -> usize {
        self.allocated.get()
    }
    pub fn state_vector(&self, count: QubitCount) -> Result<Register<'_, StateVector>> {
        Register::allocate(self, count)
    }
    pub fn density_matrix(&self, count: QubitCount) -> Result<Register<'_, DensityMatrix>> {
        Register::allocate(self, count)
    }
    pub fn close(mut self) -> std::result::Result<(), CloseError> {
        match quest_sys::finalize_quest_env().context("finalizing environment") {
            Ok(()) => {
                self.active = false;
                Ok(())
            }
            Err(error) => Err(CloseError {
                environment: self,
                error,
            }),
        }
    }
    /// Retain a conservative fixed allowance for QuEST's process RNG seed
    /// storage after the first high-level batch. It remains charged until close.
    pub(crate) fn admit_seed_storage(&self) -> Result<()> {
        if self.seed_storage.get() {
            return Ok(());
        }
        const BYTES: usize = 4096;
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
        if self.active {
            let _ = quest_sys::finalize_quest_env();
        }
    }
}
impl fmt::Debug for Environment {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Environment")
            .field("capabilities", &self.capabilities)
            .field("allocated_bytes", &self.allocated.get())
            .finish_non_exhaustive()
    }
}

/// Failed shutdown retains its environment, allowing outstanding leaked native
/// resources to be recovered through low-level interoperability before retrying.
#[derive(Debug)]
pub struct CloseError {
    pub environment: Environment,
    pub error: Error,
}
impl fmt::Display for CloseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.error.fmt(f)
    }
}
impl std::error::Error for CloseError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        Some(&self.error)
    }
}

pub(crate) struct Reservation<'a> {
    pub(crate) environment: &'a Environment,
    bytes: usize,
}
impl Drop for Reservation<'_> {
    fn drop(&mut self) {
        self.environment
            .allocated
            .set(self.environment.allocated.get() - self.bytes);
    }
}
