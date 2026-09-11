//! MPI owned by rsmpi, with its ABI checked against the installed `QuEST::QuEST` target.
//!
//! All ranks must execute matching collective operations in the same order.
//! Mutable communicator access serializes each coordination lane locally.
//! Separate MPI contexts isolate application messages, coordination collectives,
//! and `QuEST` communication. Only borrowed message views can cross threads.
//!
//! Runtime owners and communicators cannot cross threads:
//! ```compile_fail
//! let runtime = quest_sys::mpi::MpiRuntime::initialize().unwrap();
//! std::thread::spawn(move || drop(runtime));
//! ```
//! Communicator ownership also remains on the initializing thread:
//! ```compile_fail
//! let runtime = quest_sys::mpi::MpiRuntime::initialize().unwrap();
//! let communicator = runtime.world().unwrap();
//! std::thread::scope(|scope| { scope.spawn(move || drop(communicator)); });
//! ```
//! A borrowed view cannot outlive its communicator:
//! ```compile_fail
//! let runtime = quest_sys::mpi::MpiRuntime::initialize().unwrap();
//! let communicator = runtime.world().unwrap();
//! let view = communicator.threaded();
//! drop(communicator);
//! view.send(&[], 0, 0).unwrap();
//! ```
//! Collectives require exclusive access:
//! ```compile_fail
//! let runtime = quest_sys::mpi::MpiRuntime::initialize().unwrap();
//! let communicator = runtime.world().unwrap();
//! communicator.all_agree(true).unwrap();
//! ```

use std::{
    cell::{Cell, RefCell, RefMut},
    marker::PhantomData,
    panic::{AssertUnwindSafe, catch_unwind},
    rc::Rc,
    sync::Mutex,
    thread::{self, ThreadId},
};

use ::mpi as rsmpi;
use rsmpi::{
    collective::SystemOperation,
    environment::{Threading, Universe},
    topology::{Color, SimpleCommunicator},
    traits::{
        AsRaw, Communicator, CommunicatorCollectives, Destination, Equivalence, Root, Source,
    },
};

use crate::{QuestError, QuestResult, map_quest_result};

#[cxx::bridge(namespace = "quest_sys")]
mod ffi {
    unsafe extern "C++" {
        include!("quest_mpi.hpp");
        fn mpi_available() -> bool;
        fn mpi_quest_can_initialize() -> bool;
        fn mpi_validate_rsmpi_abi(
            comm_size: usize,
            fint_size: usize,
            status_size: usize,
            version: u32,
            subversion: u32,
            multiple: i32,
            library: &str,
        ) -> Result<()>;
        fn mpi_init_quest(
            communicator: i64,
            rank: i32,
            size: i32,
            gpu: bool,
            threads: bool,
        ) -> Result<()>;
        fn mpi_drop_quest();
        fn mpi_quest_is_quiescent() -> bool;
        fn mpi_abort_job();
    }
}

static INITIALIZATION_ATTEMPTED: Mutex<bool> = Mutex::new(false);

fn lifecycle(message: &str) -> QuestError {
    QuestError::Lifecycle(message.into())
}
fn invalid(message: &str) -> QuestError {
    QuestError::InvalidInput(message.into())
}

/// Abort the active distributed job when ordered native cleanup is impossible.
/// This does not unwind and can be called from a distributed owner's error path.
pub fn abort_job() -> ! {
    ffi::mpi_abort_job();
    std::process::abort()
}

// rsmpi's Universe and communicator destructors can panic. Never allow that
// panic to escape a distributed owner Drop or leave peer ranks waiting forever.
fn mpi_call<T>(operation: impl FnOnce() -> T) -> T {
    catch_unwind(AssertUnwindSafe(operation)).unwrap_or_else(|_| abort_job())
}

fn set_fatal_errors(comm: &SimpleCommunicator) {
    // SAFETY: the communicator is live, owned by rsmpi, and the runtime has been
    // initialized. This only installs MPI's builtin fatal handler; ownership is
    // unchanged. rsmpi ignores most MPI result codes, so ERRORS_RETURN is unsafe
    // as a recovery policy for all operations and destructors exposed here.
    let status = unsafe {
        rsmpi::ffi::MPI_Comm_set_errhandler(comm.as_raw(), rsmpi::ffi::RSMPI_ERRORS_ARE_FATAL)
    };
    if u32::try_from(status) != Ok(rsmpi::ffi::MPI_SUCCESS) {
        abort_job();
    }
}

fn validate_rsmpi_abi() -> QuestResult<()> {
    // SAFETY: this immutable constant is supplied by mpi-sys's compiled C shim
    // and is readable before MPI initialization. Comparing generated layouts and
    // constants also catches stale bindgen artifacts from an earlier MPICC.
    let multiple = unsafe { rsmpi::ffi::RSMPI_THREAD_MULTIPLE };
    map_quest_result(ffi::mpi_validate_rsmpi_abi(
        size_of::<rsmpi::ffi::MPI_Comm>(),
        size_of::<rsmpi::ffi::RSMPI_Fint>(),
        size_of::<rsmpi::ffi::MPI_Status>(),
        rsmpi::ffi::MPI_VERSION,
        rsmpi::ffi::MPI_SUBVERSION,
        multiple,
        env!("QUEST_RSMPI_LIBRARY"),
    ))
}

/// Admitted rsmpi universe with mandatory `MPI_THREAD_MULTIPLE` support.
///
/// An existing MPI runtime is rejected. Every owner stays on the initializing
/// thread, and the rsmpi universe drops after its borrowed communicators.
/// No explicit shutdown API or fallback threading mode is exposed.
pub struct MpiRuntime {
    universe: Option<Universe>,
    owner: ThreadId,
    communicators: Cell<usize>,
    _thread: PhantomData<Rc<()>>,
}

impl MpiRuntime {
    #[must_use]
    pub fn is_available() -> bool {
        ffi::mpi_available()
    }

    #[must_use]
    pub fn is_finalized() -> bool {
        let _admission = INITIALIZATION_ATTEMPTED
            .lock()
            .unwrap_or_else(|_| abort_job());
        if validate_rsmpi_abi().is_err() {
            abort_job();
        }
        mpi_call(rsmpi::environment::is_finalized)
    }

    pub fn initialize() -> QuestResult<Self> {
        let mut attempted = INITIALIZATION_ATTEMPTED
            .lock()
            .unwrap_or_else(|_| abort_job());
        validate_rsmpi_abi()?;
        if *attempted
            || mpi_call(rsmpi::environment::is_initialized)
            || mpi_call(rsmpi::environment::is_finalized)
        {
            return Err(lifecycle(
                "MPI initialization requires an unattempted, uninitialized runtime",
            ));
        }
        *attempted = true;
        let (universe, provided) =
            mpi_call(|| rsmpi::initialize_with_threading(Threading::Multiple))
                .ok_or_else(|| lifecycle("rsmpi rejected MPI initialization"))?;
        let world = universe.world();
        set_fatal_errors(&world);
        set_fatal_errors(&SimpleCommunicator::self_comm());
        let mut all_multiple = 0;
        mpi_call(|| {
            world.all_reduce_into(
                &i32::from(provided == Threading::Multiple),
                &mut all_multiple,
                SystemOperation::min(),
            );
        });
        if all_multiple == 0 {
            mpi_call(|| drop(universe));
            return Err(lifecycle(
                "MPI_THREAD_MULTIPLE was requested but not provided on every rank",
            ));
        }
        let runtime = Self {
            universe: Some(universe),
            owner: thread::current().id(),
            communicators: Cell::new(0),
            _thread: PhantomData,
        };
        // Keep admission serialized through init/support checks and possible
        // failed-admission finalization, not merely the attempted flag update.
        drop(attempted);
        Ok(runtime)
    }

    pub fn is_active(&self) -> QuestResult<bool> {
        self.ensure_owner()?;
        Ok(!mpi_call(rsmpi::environment::is_finalized))
    }

    #[must_use]
    pub fn thread_multiple(&self) -> bool {
        mpi_call(rsmpi::environment::threading_support) == Threading::Multiple
    }

    /// Collectively create independent contexts owned by rsmpi communicators.
    pub fn world(&self) -> QuestResult<MpiCommunicator<'_>> {
        self.ensure_owner()?;
        let universe = self.universe.as_ref().unwrap_or_else(|| abort_job());
        Ok(MpiCommunicator::from_source(self, &universe.world()))
    }

    fn ensure_owner(&self) -> QuestResult<()> {
        if self.owner != thread::current().id() {
            return Err(lifecycle(
                "MPI ownership operation requires initializing thread",
            ));
        }
        Ok(())
    }
}

impl Drop for MpiRuntime {
    fn drop(&mut self) {
        let _admission = INITIALIZATION_ATTEMPTED
            .lock()
            .unwrap_or_else(|_| abort_job());
        if self.ensure_owner().is_err()
            || self.communicators.get() != 0
            || !ffi::mpi_quest_is_quiescent()
            || mpi_call(rsmpi::environment::is_finalized)
        {
            abort_job();
        }
        mpi_call(|| drop(self.universe.take()));
        if !mpi_call(rsmpi::environment::is_finalized) {
            abort_job();
        }
    }
}

/// Three owned rsmpi contexts borrowing their admitted runtime.
///
/// Communicator creation, split, duplicate and Drop require matching collective
/// order across ranks. Safe wrappers do not expose rsmpi ownership or raw handles.
pub struct MpiCommunicator<'runtime> {
    // Explicit Option fields permit containing each upstream destructor panic
    // before any other field starts unwinding or frees another context.
    application: Option<SimpleCommunicator>,
    coordination: Option<SimpleCommunicator>,
    quest: Option<SimpleCommunicator>,
    runtime: &'runtime MpiRuntime,
    lane: RefCell<()>,
}

impl<'runtime> MpiCommunicator<'runtime> {
    fn from_source(runtime: &'runtime MpiRuntime, source: &SimpleCommunicator) -> Self {
        let quest = mpi_call(|| source.duplicate());
        set_fatal_errors(&quest);
        let coordination = mpi_call(|| source.duplicate());
        set_fatal_errors(&coordination);
        let application = mpi_call(|| source.duplicate());
        set_fatal_errors(&application);
        runtime.communicators.set(
            runtime
                .communicators
                .get()
                .checked_add(1)
                .unwrap_or_else(|| abort_job()),
        );
        Self {
            application: Some(application),
            coordination: Some(coordination),
            quest: Some(quest),
            runtime,
            lane: RefCell::new(()),
        }
    }

    fn application(&self) -> &SimpleCommunicator {
        self.application.as_ref().unwrap_or_else(|| abort_job())
    }
    fn coordination(&self) -> &SimpleCommunicator {
        self.coordination.as_ref().unwrap_or_else(|| abort_job())
    }
    fn quest(&self) -> &SimpleCommunicator {
        self.quest.as_ref().unwrap_or_else(|| abort_job())
    }

    pub fn rank(&self) -> QuestResult<i32> {
        Ok(mpi_call(|| self.application().rank()))
    }
    pub fn size(&self) -> QuestResult<i32> {
        Ok(mpi_call(|| self.application().size()))
    }

    pub fn duplicate(&mut self) -> QuestResult<Self> {
        self.runtime.ensure_owner()?;
        Ok(Self::from_source(self.runtime, self.quest()))
    }

    /// Split by nonnegative color and key. `None` excludes this rank.
    pub fn split(&mut self, color: Option<i32>, key: i32) -> QuestResult<Option<Self>> {
        if !self.all_agree(color.is_none_or(|color| color >= 0))? {
            return Err(invalid("MPI split color must be nonnegative or excluded"));
        }
        let color = color.map_or_else(Color::undefined, Color::with_value);
        let split = mpi_call(|| self.quest().split_by_color_with_key(color, key));
        let result = split
            .as_ref()
            .map(|source| Self::from_source(self.runtime, source));
        mpi_call(|| drop(split));
        Ok(result)
    }

    /// Split consecutive rank groups of equal power-of-two size.
    pub fn split_power_of_two(&mut self, subgroup_size: i32) -> QuestResult<Self> {
        if !self.all_agree(subgroup_size > 0 && subgroup_size.count_ones() == 1)? {
            return Err(invalid(
                "subgroup size must be a positive power of two on every rank",
            ));
        }
        let mut root_size = subgroup_size.to_le_bytes();
        self.broadcast_bytes(0, &mut root_size)?;
        if !self.all_agree(i32::from_le_bytes(root_size) == subgroup_size)? {
            return Err(invalid("subgroup sizes differ across ranks"));
        }
        let size = self.size()?;
        if !self.all_agree(size.checked_rem(subgroup_size) == Some(0))? {
            return Err(invalid("subgroup size must divide communicator size"));
        }
        let rank = self.rank()?;
        let color = rank
            .checked_div(subgroup_size)
            .ok_or_else(|| invalid("invalid subgroup size"))?;
        self.split(Some(color), rank)?
            .ok_or_else(|| lifecycle("unexpected excluded subgroup rank"))
    }

    pub fn broadcast_bytes(&mut self, root: i32, data: &mut [u8]) -> QuestResult<()> {
        broadcast(self.coordination(), root, data)
    }
    pub fn all_agree(&mut self, value: bool) -> QuestResult<bool> {
        Ok(agree(self.coordination(), value))
    }

    /// Borrow an exclusive coordination lane while other contexts remain usable.
    pub fn collective_lane(&self) -> QuestResult<MpiCollectiveLane<'_>> {
        let exclusive = self
            .lane
            .try_borrow_mut()
            .map_err(|_| lifecycle("communicator already has a collective lane"))?;
        Ok(MpiCollectiveLane {
            communicator: self.coordination(),
            _exclusive: exclusive,
        })
    }

    #[must_use]
    pub fn threaded(&self) -> MpiThreadView<'_> {
        MpiThreadView {
            communicator: self.application(),
        }
    }

    /// Admit a power-of-two communicator for building its borrowed environment.
    pub fn quest_environment(&self) -> QuestResult<MpiQuestEnvironmentBuilder<'_, 'runtime>> {
        let size = self.size()?;
        if size <= 0 || size.count_ones() != 1 {
            return Err(invalid("QuEST communicator size must be a power of two"));
        }
        Ok(MpiQuestEnvironmentBuilder {
            communicator: self,
            gpu: false,
            threads: false,
        })
    }
}

impl Drop for MpiCommunicator<'_> {
    fn drop(&mut self) {
        if self.runtime.ensure_owner().is_err() {
            abort_job();
        }
        mpi_call(|| drop(self.application.take()));
        mpi_call(|| drop(self.coordination.take()));
        mpi_call(|| drop(self.quest.take()));
        self.runtime.communicators.set(
            self.runtime
                .communicators
                .get()
                .checked_sub(1)
                .unwrap_or_else(|| abort_job()),
        );
    }
}

/// Configuration borrowing an admitted communicator and `MPI_THREAD_MULTIPLE` runtime.
/// Native GPU acceleration and native multithreading default to disabled.
pub struct MpiQuestEnvironmentBuilder<'communicator, 'runtime> {
    communicator: &'communicator MpiCommunicator<'runtime>,
    gpu: bool,
    threads: bool,
}
impl<'communicator, 'runtime> MpiQuestEnvironmentBuilder<'communicator, 'runtime> {
    #[must_use]
    pub const fn with_gpu_acceleration(mut self) -> Self {
        self.gpu = true;
        self
    }
    #[must_use]
    pub const fn with_multithreading(mut self) -> Self {
        self.threads = true;
        self
    }

    /// Consume the builder after arranging matching native configuration and
    /// lifecycle order on every rank. Native admission remains once per process.
    pub fn build(self) -> QuestResult<MpiQuestEnvironment<'communicator, 'runtime>> {
        self.communicator.runtime.ensure_owner()?;
        let mut lane = self.communicator.collective_lane()?;
        let local_config = [u8::from(self.gpu), u8::from(self.threads)];
        let mut root_config = local_config;
        lane.broadcast_bytes(0, &mut root_config)?;
        if !lane.all_agree(root_config == local_config)? {
            return Err(invalid("QuEST configuration differs between ranks"));
        }
        if !lane.all_agree(ffi::mpi_quest_can_initialize())? {
            return Err(lifecycle(
                "QuEST initialization may only be attempted once on every rank",
            ));
        }
        drop(lane);
        // SAFETY: this live rsmpi communicator remains borrowed for the returned
        // environment. Conversion only creates a Fortran representation, never
        // transfers ownership. Build/runtime ABI checks precede this handoff.
        let handle = unsafe { rsmpi::ffi::RSMPI_Comm_c2f(self.communicator.quest().as_raw()) };
        map_quest_result(ffi::mpi_init_quest(
            i64::from(handle),
            self.communicator.rank()?,
            self.communicator.size()?,
            self.gpu,
            self.threads,
        ))
        .unwrap_or_else(|_| abort_job());
        Ok(MpiQuestEnvironment {
            communicator: self.communicator,
        })
    }
}

fn agree(comm: &SimpleCommunicator, value: bool) -> bool {
    let mut result = 0;
    mpi_call(|| comm.all_reduce_into(&i32::from(value), &mut result, SystemOperation::min()));
    result != 0
}

fn broadcast(comm: &SimpleCommunicator, root: i32, data: &mut [u8]) -> QuestResult<()> {
    let input = [
        i64::from(root),
        i64::try_from(data.len()).unwrap_or(i64::MAX),
    ];
    let mut minimum = [0_i64; 2];
    let mut maximum = [0_i64; 2];
    mpi_call(|| comm.all_reduce_into(&input, &mut minimum, SystemOperation::min()));
    mpi_call(|| comm.all_reduce_into(&input, &mut maximum, SystemOperation::max()));
    if minimum != maximum || root < 0 || root >= comm.size() || i32::try_from(data.len()).is_err() {
        return Err(invalid(
            "MPI broadcast requires matching valid root and bounded byte count on every rank",
        ));
    }
    mpi_call(|| comm.process_at_rank(root).broadcast_into(data));
    Ok(())
}

/// Exclusive owner-thread coordination lane, independent from application messages.
pub struct MpiCollectiveLane<'communicator> {
    communicator: &'communicator SimpleCommunicator,
    _exclusive: RefMut<'communicator, ()>,
}
impl MpiCollectiveLane<'_> {
    pub fn broadcast_bytes(&mut self, root: i32, data: &mut [u8]) -> QuestResult<()> {
        broadcast(self.communicator, root, data)
    }
    pub fn all_agree(&mut self, value: bool) -> QuestResult<bool> {
        Ok(agree(self.communicator, value))
    }
}

/// Borrowed rsmpi message view; cannot free communicators or finalize MPI.
/// Tags use MPI's portable guaranteed range `0..=32767`. Byte counts fit `i32`.
#[derive(Clone, Copy)]
pub struct MpiThreadView<'communicator> {
    communicator: &'communicator SimpleCommunicator,
}

// SAFETY: only a successfully admitted MPI_THREAD_MULTIPLE runtime constructs
// this view. Its borrow keeps all owners alive, and methods expose synchronous
// rsmpi application messages only, never communicator mutation or destruction.
unsafe impl Send for MpiThreadView<'_> {}
// SAFETY: MPI_THREAD_MULTIPLE permits concurrent messages; each receive buffer
// has an exclusive Rust borrow. Matching/order remains a protocol obligation.
unsafe impl Sync for MpiThreadView<'_> {}

impl MpiThreadView<'_> {
    fn validate(self, count: usize, peer: i32, tag: i32) -> QuestResult<()> {
        if i32::try_from(count).is_err()
            || peer < 0
            || peer >= self.communicator.size()
            || !(0..=32767).contains(&tag)
        {
            return Err(invalid(
                "MPI element count, peer or portable tag is out of range",
            ));
        }
        Ok(())
    }
    pub fn send(self, data: &[u8], destination: i32, tag: i32) -> QuestResult<()> {
        self.send_typed(data, destination, tag)
    }
    pub fn receive(self, data: &mut [u8], source: i32, tag: i32) -> QuestResult<usize> {
        self.receive_typed(data, source, tag)
            .map(|status| status.count)
    }
    pub fn send_receive(
        self,
        send: &[u8],
        destination: i32,
        send_tag: i32,
        receive: &mut [u8],
        source: i32,
        receive_tag: i32,
    ) -> QuestResult<usize> {
        self.send_receive_typed(send, destination, send_tag, receive, source, receive_tag)
            .map(|status| status.count)
    }

    /// Send a typed slice using rsmpi's datatype equivalence contract.
    pub fn send_typed<T: Equivalence>(
        self,
        data: &[T],
        destination: i32,
        tag: i32,
    ) -> QuestResult<()> {
        self.validate(data.len(), destination, tag)?;
        mpi_call(|| {
            self.communicator
                .process_at_rank(destination)
                .send_with_tag(data, tag);
        });
        Ok(())
    }

    /// Receive a typed slice and snapshot rsmpi's status while MPI is live.
    pub fn receive_typed<T: Equivalence>(
        self,
        data: &mut [T],
        source: i32,
        tag: i32,
    ) -> QuestResult<MpiMessageStatus> {
        self.validate(data.len(), source, tag)?;
        let status = mpi_call(|| {
            self.communicator
                .process_at_rank(source)
                .receive_into_with_tag(data, tag)
        });
        Ok(message_status::<T>(status))
    }

    /// Exchange typed slices without transferring any rsmpi handle ownership.
    pub fn send_receive_typed<T: Equivalence>(
        self,
        send: &[T],
        destination: i32,
        send_tag: i32,
        receive: &mut [T],
        source: i32,
        receive_tag: i32,
    ) -> QuestResult<MpiMessageStatus> {
        self.validate(send.len(), destination, send_tag)?;
        self.validate(receive.len(), source, receive_tag)?;
        let status = mpi_call(|| {
            rsmpi::point_to_point::send_receive_into_with_tags(
                send,
                &self.communicator.process_at_rank(destination),
                send_tag,
                receive,
                &self.communicator.process_at_rank(source),
                receive_tag,
            )
        });
        Ok(message_status::<T>(status))
    }
}

/// Pure receive-status snapshot. Counting uses the message's rsmpi datatype
/// while the borrowed runtime is live, so retaining this value cannot issue
/// native MPI calls after finalization.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct MpiMessageStatus {
    pub source_rank: i32,
    pub tag: i32,
    pub count: usize,
}

fn message_status<T: Equivalence>(status: rsmpi::point_to_point::Status) -> MpiMessageStatus {
    let count = mpi_call(|| status.count(T::equivalent_datatype()));
    MpiMessageStatus {
        source_rank: status.source_rank(),
        tag: status.tag(),
        count: usize::try_from(count).unwrap_or_else(|_| abort_job()),
    }
}

/// Borrowed `QuEST` environment. Destroy every low-level native resource first;
/// invalid distributed destruction order aborts the job rather than unwinding.
pub struct MpiQuestEnvironment<'communicator, 'runtime> {
    communicator: &'communicator MpiCommunicator<'runtime>,
}
impl Drop for MpiQuestEnvironment<'_, '_> {
    fn drop(&mut self) {
        if self.communicator.runtime.ensure_owner().is_err() {
            abort_job();
        }
        ffi::mpi_drop_quest();
    }
}
