# quest-sys

CXX bindings to **QuEST 4.3.x**, with double precision and deprecated APIs disabled.
Native discovery lives in the shared `quest-build` crate. The tested installed
build recipe is Linux GNU with CMake and a C++20 compiler.

Set `QUEST_ROOT` to the installation prefix, or select its package with
`QUEST_ROOT` (the exact installation prefix) or `CMAKE_PREFIX_PATH`. CXX generates bridge sources; CMake compiles
the static bridge against `QuEST::QuEST`, consuming its public usage requirements.
The installed library must resolve its own dependencies, including private GPU
libraries. Local native installs can opt into CMake's
`CMAKE_INSTALL_RPATH_USE_LINK_PATH` setting.

See `quest-build` for the final-executable helper, which emits required linker
options and direct-library RUNPATH entries. Cargo does not propagate those
arguments through arbitrary Rust library dependencies. The old native JSON
record and runtime-directory override workflow are removed. This crate does
not modify or bundle native installation files.

```rust,no_run
fn main() -> quest_sys::QuestResult<()> {
    quest_sys::init_custom_quest_env(false, false, false)?;
    let mut register = quest_sys::create_qureg(2)?;
    quest_sys::apply_hadamard(register.pin_mut(), 0)?;
    quest_sys::apply_controlled_pauli_x(register.pin_mut(), 0, 1)?;
    let amplitude = quest_sys::get_qureg_amp(&register, 3)?;
    println!("{} + {}i", amplitude.re, amplitude.im);
    drop(register);
    quest_sys::finalize_quest_env()
}
```

Initialization may be attempted only once per process. Every native wrapper
uses serialized owner-thread admission; resources must be destroyed on that
thread before finalization. Opaque owned handles use RAII. Leaking a handle does
not bypass live-resource checks. Destructors cannot throw across FFI and fail
closed when native destruction cannot safely proceed. Native initialization
failures can still invoke QuEST's default error handler before the replacement
handler can be installed.

Safe bindings keep validation enabled; the validation-disable API is absent.
The tolerance setter admits positive finite values only. Native input errors
become structured `QuestError` values after initialization.

Matrices are copied through checked buffers, with no faer dependency:

- `set_comp_matr_flat`: row-major square matrix values.
- `set_density_qureg_amps` / `get_density_qureg_amps`: independent rectangular
  row/column dimensions with row-major interchange buffers.
- `set_density_qureg_flat_amps`: QuEST's column-major flattened density storage.
- `set_kraus_map_flat`: operator-major, then row-major values.

The facade handles faer view conversion and stronger environment lifetimes.
Manual adapters additionally expose explicit seeds, global phase and a numerical
configuration fingerprint. The generated API inventory and unsupported reasons
are in `generated/api_coverage.json`; generated counts are not a promise that
every native feature is supported by the facade.

The common inventory stays identical across MPI and non-MPI packages. The
reviewed header-conditional `initCustomMpiCommQuESTEnv` declaration is inventoried
in `generated/api_coverage_mpi.json`, emitted and checked only when the selected
native package enables both MPI and subcommunicators. That check also detects a
removed or renamed declaration. Its `MPI_Comm` canonical type and overload key
use the logical public typedef so integer and opaque-pointer MPI handles produce
the same coverage receipt; native ABI compatibility is checked separately by the
build probes. New declarations remain in the common inventory until their
conditional requirements have been reviewed.

Edit the adapter registry and generator templates together, then regenerate with
`cargo run -p xtask -- generate-quest-bindings`. `--check` verifies freshness.
Generation requires libclang and the checked-in reviewed `generated_adapters.json`
registry; missing registry data fails instead of recreating adapters from a
coverage receipt. Consuming this crate does not require libclang.

With the optional `mpi` feature, `quest_sys::mpi` owns an official rsmpi
`Universe` and three rsmpi `SimpleCommunicator` contexts per communicator.
The public module, CXX handoff and MPI tests additionally require the selected
native package to enable MPI and subcommunicators: build scripts emit checked
`quest_native_mpi`, and Rust uses `all(feature = "mpi", quest_native_mpi)`.
A non-MPI native package hides the API and skips MPI compatibility witnesses.
Cargo still resolves an explicitly enabled optional dependency before build
scripts run, so leave `mpi` disabled for a fully MPI-free dependency graph.
The default feature set does not add rsmpi, mpi-sys, bindgen or libclang.
Pure circuit/language compilation stays independent of the native runtime.

Set an explicit absolute `MPICC` path before building with `--features mpi`.
The wrapper must select the MPI implementation used by the installed
`QuEST::QuEST` target, and the package must enable MPI and subcommunicators.
The build compiles separate MPI ABI witnesses with that target and with MPICC,
then compares their actual loaded MPI library paths, versions, handle/status
sizes and constants. Runtime validation checks the loaded library and generated
rsmpi layout again before MPI initialization. Unset `MPI_PKG_CONFIG` and
`CRAY_MPICH_DIR` for this supported recipe. Rebuild mpi-sys when changing MPICC
or its compiler environment; upstream MPI discovery does not track every such
change. rsmpi's mpi-sys feature build requires libclang.

`MpiRuntime::initialize()` uses rsmpi's `initialize_with_threading(Multiple)`
and requires that support on every rank. It rejects an existing runtime or a
repeat attempt. Runtime and communicator owners remain on the initializing
thread. The rsmpi universe finalizes MPI only when its runtime owner drops,
after every borrowed communicator has been destroyed.

A communicator owns separate QuEST, coordination and application contexts.
`split(Some(color), key)` permits any positive group size; `split(None, key)`
excludes a rank. `split_power_of_two(size)` creates equal consecutive groups.
`quest_environment()` admits a power-of-two group and returns a builder
borrowing its communicator and admitted runtime. Named
`with_gpu_acceleration()` / `with_multithreading()` options configure native
execution; `build()` consumes the builder to initialize QuEST. The minimal C++
handoff converts a borrowed MPI Fortran handle and retains QuEST's one-attempt
lifecycle guard. QuEST duplicates that context and frees its duplicate on Drop,
leaving the rsmpi-owned universe active.

`collective_lane()` lends an exclusive ordered coordination lane while the
environment lives. `threaded()` lends a `Send + Sync` message view for scoped
workers, with bounded byte send/receive/send-receive and portable tags
`0..=32767`. Views cannot free communicators or finalize MPI, and application
messages do not acquire QuEST's lifecycle mutex. Typed slice methods use
rsmpi's `Equivalence` datatype contract and return pure source/tag/count status
snapshots. rsmpi's `SimpleCommunicator` does not borrow its `Universe`, and even
a shared communicator exposes duplication into untracked owned handles; these
wrappers therefore do not expose the full upstream communicator trait or raw
handles. Callers must maintain matching
collective/lifecycle order and configuration across ranks.

rsmpi generally ignores MPI return codes and its destructors can panic.
Consequently all exposed contexts retain MPI's fatal handler; ordinary
collective preflight errors are agreed before rsmpi payload operations.
Destruction contains upstream panics and aborts the MPI job on unrecoverable
cleanup failure. Low-level resources must drop before the distributed
environment guard. Ordinary local QuEST Drop retains retirement-and-continue
behavior. High-level collective preparation/execution scheduling belongs in
the facade.

MPI integration tests launch isolated one-, two- and four-process cases with
`mpiexec -n` and `timeout` on PATH. The launcher must match the selected MPI
implementation, with local sockets enabled. On the current MPICH installation,
`MPICH_CC=/usr/bin/gcc` selects the available C compiler because its saved
`gcc-13` command is absent; the ABI witnesses still verify the same MPI library.
