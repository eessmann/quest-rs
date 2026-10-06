# Matching State Admission Implementation Plan

> **For agentic workers:** Use `superpowers:subagent-driven-development` or `superpowers:executing-plans` to implement this plan task by task. The checkboxes record implementation work; none is completed by this design document.

**Goal:** Admit distributed CPU matching execution against its audited live storage without exposing an under-reserved general register.

**Architecture:** A sealed `MatchingState` owns a private collective register and supports only matching execution, simple initialization, bounded local amplitude transfer and norm calculation. Admission precedes allocation; native deployment checks confirm the admitted storage contract. Returned amplitude buffers own their reservations for their complete lifetimes.

**Tech stack:** Rust, CXX, native QuEST 4.3.x, checked rsmpi/MPI ABI, CPU/OpenMP.

**Spec:** [Memory boundary](../../research/distributed-capacity-memory.md), [buffer-reuse plan](2026-10-06-matching-buffer-reuse.md), and [local evidence](../../verification/2026-10-06-matching-buffer-reuse.md).

**Status:** Deferred research design, not an active Cirrus workaround. Only the independent MPI request size/alignment witnesses have been implemented and locally tested; no restricted state or reduced reservation is implemented. The user's subsequent direction requires documented Cirrus tools, recording unsupported requirements and moving on. Do not pursue private native exports, new attestation machinery, replacement library packaging or custom memory enforcement to close the current campaign. General register admission remains unchanged.

## Global constraints

- Preserve the full matching unitary, flag, color register, spectator coordinates, both outer-control signs, complex phases and adjoint semantics.
- Preserve the generic four-array CPU reservation in `state_vector_local` and ordinary register allocation. One-rank matching retains its existing input reservation and owned scratch fallback in this initial stage.
- Distributed matching continues to reuse the input communication array exclusively between the opening and closing Hadamards. Preserve preflight agreement and fatal handling after mutation.
- Expose no native pointers, unrestricted register reference, `Deref`, `AsRef<CollectiveRegister>`, conversion or general program execution on the restricted owner. An upgrade API is outside this initial scope.
- Keep CPU-only deployment, owner-thread confinement and communicator lifetimes. Reject GPU/density use and inconsistent rank arguments before allocation or mutation.
- Keep historical receipt schemas intact. Any later harness migration needs a new version and an immutable build identity.
- Native array payload, managed reservations, runtime/allocator overhead and enforced process/node caps are distinct quantities. This stage alone does not close strict capacity.
- Preserve dirty work and evidence; native QuEST source remains unchanged. This design step performs no implementation, commit, deployment or cluster submission; existing session authorization is unchanged.

## Review focus

1. One rank rejects allocation, limits or ownership while peers would proceed: all ranks must return before mutation and remain usable (Tasks 1–3).
2. Native deployment, request-count assumptions or MPI handle layout differs from the admitted profile: restricted admission must fail closed (Tasks 1–2).
3. A returned amplitude vector outlives its call or register: its storage charge must remain live until the buffer owner drops (Task 3).
4. A distributed color Hadamard overwrites the reused communication array: repeated scalar/fused forward/adjoint action must remain correct (Task 4).
5. Multiple states, preparations and output buffers coexist near the budget: every admission must include their combined live charges (Tasks 2–4).

## Storage contract and conservative native request bound

Let `D=2^qubits`, `P` be communicator size, `L=D/P`, and `b=sizeof(qcomp)=16` in the admitted double-precision ABI. At audited native revision `503552065045eaf89baba85e6cd6aad728525554`, `quest/src/api/qureg.cpp` allocates `cpuAmps[L]` and, for distributed CPU execution, `cpuCommBuffer[L]`. The distributed persistent array payload is `2*b*L`; the current generic reservation is `4*b*L`. Native one-rank deployment has one array, but this plan leaves its conservative policy unchanged.

The single-target Hadamards used by matching operate directly on these arrays. Their control lists are fixed-capacity native `List64` objects; their CPU kernels use scalar per-thread amplitude temporaries. Initialization and norm reduction likewise allocate no additional state-sized native array. General dense multi-target operations are excluded: their CPU kernel creates a `2^targets` cache per OpenMP thread.

Distributed Hadamards allocate request metadata. `comm_routines.cpp::exchangeArrays` constructs `vector<MPI_Request>(2*m)` and waits for all requests before returning. Under the audited native contract every message contains at least one amplitude, so `1 <= m <= L` for a full partition, and control packing cannot increase that bound. With `r=sizeof(MPI_Request)`, **`2*L*r` is a conservative request-payload allowance requiring no knowledge of the native maximum message size**. Pre-allocation reserves persistent arrays and this allowance, along with bounded metadata, so a later matching gate cannot require an unadmitted state-sized request allowance. Keep its reservation for the restricted state's lifetime in the initial design; do not describe this modeled worst case as an actual native allocation.

Hard admission conditions include positive power-of-two `L`, the audited positive-message/count protocol, checked `2*L*r` arithmetic, and `2*L <= INT_MAX` so even the worst-case `MPI_Waitall` count is representable. The bound alone does not prove that every possible message count fits the communicator's tag range. The audited native partition performs its own tag-count check before posting messages, and the restricted path must either establish a valid actual message-count upper bound against the native communicator's `MPI_TAG_UB` before mutation, or require the stronger sufficient `L <= MPI_TAG_UB` (matching the audited native check, which uses the tag bound as a message-count limit) when only the worst-case bound is available. Query the native communicator's attribute through a checked adapter, preserving its validity/fallback semantics; do not substitute an attribute from a different duplicated communicator. An unsupported/unknown protocol or insufficient count/tag bound rejects admission rather than certifying a guessed message limit.

At the audited revision `dividePow2PayloadIntoMessages(L)` returns one message for `L<M`, otherwise `L/M`, with `M=2^28`. Both supported lengths are powers of two, making `m=max(1,L/M)` exact. A generic ceiling calculation must use checked quotient/remainder arithmetic and cannot certify an unsupported partition rule. The value `2^28` must not become a duplicated policy constant. `MAX_MESSAGE_LENGTH` is an internal mutable symbol without an installed public query; native source is to remain unchanged. Optional tighter bounds may use an existing export only when a compile/link/runtime-identity probe establishes that particular installed package's symbol and audited partition contract. Packages without such support use the conservative bound and report the exact message threshold as unknown (`null`), or reject if its hard count/tag conditions cannot be established. No native-source modification is a prerequisite.

The checked adapter reports `sizeof(MPI_Request)` and its alignment using the verified native MPI headers, plus `sizeof(qcomp)` and the native wrapper object's size. Extend build-time and runtime rsmpi ABI witnesses to compare request size/alignment as well as their existing communicator/status checks. Agree the selected operation/profile identity across ranks before allocation and routing; revalidate any mutable optional native threshold before the opening Hadamard. The selected profile must reject a different native protocol rather than trusting a matching version string alone. The conservative path must remain independently usable without optional private exports.

Charge the array payload before native creation, the wrapper object and known bounded control/request payloads explicitly, and existing descriptor/router allocations through their retained-capacity reservations. Account simultaneous Rust/CXX control-vector copies or replace them with bounded stack slices; do not presume a `Vec` capacity from its length. `std::vector` allocator rounding, native NUMA page rounding, MPI internal requests/buffering, OpenMP runtime storage/stacks and general allocator bookkeeping are not certified by `sizeof` payload counters. Keep a named conservative overhead allowance and independently enforced process caps; report its scope and do not rename the result an exact RSS/virtual-memory bound.

For the current two-color controlled source, `D=8N`. At eight ranks the persistent array reservation would be `256N/P`, with retained native coefficient records adding approximately `112N/P`, before router and other overhead. For an ABI with four-byte `MPI_Request`, the conservative request allowance adds `64N/P`; at eight ranks these terms total `54N`, compared with `64N` original input. This arithmetic is a managed-payload bound, not a predicted cap fit: the three-point producer fit would give `55N` at eight ranks by algebraic extrapolation, and runtime/stack costs remain additional. The measured producer leading term is `440N/P` for the three local workload points only. At `N=2^28`, array payload alone is 8 GiB per rank and original input is 16 GiB: an 8 GiB cap still cannot fit the arrays plus records and runtime. No reservation refinement defeats that physical bound.

## Task 1: Query the installed ABI and test conservative request admission

**Files:** `crates/quest-sys/src/cxx_bindings/quest_bindings.cpp`, `include/quest_bindings.hpp`, `crates/quest-sys/src/lib.rs`; `crates/quest-sys/src/mpi.rs`, `cxx_bindings/quest_mpi.cpp`; `crates/quest-build/native/mpi_abi.c`, `src/rsmpi.rs`, and the existing native capability probe. Native source remains unchanged.

**Interface:** A checked `cpu_matching_storage_layout() -> QuestResult<CpuMatchingStorageLayout>` returns request-size/alignment, complex-size, wrapper-size and native communicator tag-bound fields with an explicit audited protocol identity. An optional message-limit field is absent unless verified for that package. A checked pure calculator consumes this record plus `L`, chooses the conservative bound by default, and produces persistent, request and metadata payload terms using checked arithmetic. Unsupported protocol/ABI or unprovable count/tag conditions return a structured admission error.

- [ ] Add failing native/ABI tests for unknown optional threshold with a valid conservative bound, unknown protocol rejection, request-layout mismatch, zero/non-power-of-two lengths, count/tag-limit rejection and arithmetic overflow. With injected verified optional limits, test `L=M/2`, `M` and `2M` without allocating those amplitude arrays.
- [ ] Implement the guarded ABI/tag-bound query and conservative calculator first. Preserve existing general APIs on unsupported profiles. Add request size/alignment to both ABI witnesses and test deliberate mismatches.
- [ ] Verify the audited positive-message protocol and the conservative result with installed-consumer tests. Optional narrowing, if pursued, additionally checks the actual native partition and mutable exported value; a symbol linking alone never certifies its semantics.
- [ ] Run focused `quest-build` and `quest-sys` tests with matching MPI; review the adapter, count/tag proofs and linked-library identity before proceeding.

## Task 2: Add the sealed state owner and pre-allocation admission

**Files:** create `crates/quest/src/qsvt/matching/collective/state.rs`; modify `matching/collective.rs`, `crates/quest/src/register.rs` only for necessary crate-private allocation support, and `crates/quest/tests/matching_collective/state_admission.rs` with its integration-module declaration.

**Interfaces:** `CollectiveEnvironment::matching_state_local(count: QubitCount) -> Result<MatchingState<'_, 'comm, 'runtime>>`; immutable `deployment()`, `num_qubits()` and environment metadata getters. The new owner privately contains a `CollectiveRegister`; no public method exposes it.

- [ ] Write a P2/P4/P8 regression with a budget between the audited restricted total and the old four-array reservation. Ordinary allocation must fail while restricted allocation succeeds; dropping it restores the exact baseline. Add P1 fallback, multiple-live-owner and one-rank-budget-failure/recovery cases.
- [ ] Reserve the complete constructor payload and conservative metadata allowance collectively **before** `Register::allocate_admitted`. Do not allocate under the old four-array policy and shrink afterward: that cannot admit a legitimately tighter budget.
- [ ] Collectively verify the actual CPU/statevector/distributed deployment, local length, host array bytes and zero device bytes before publishing the owner. Unexpected native storage or post-allocation failures use the existing containment policy; do not expose an undercharged register or silently increase its budget after allocation.
- [ ] Add compile-fail coverage showing that the owner cannot enter `CollectivePreparedProgram::run`, escape as a general register, outlive its environment, or move to another thread. Keep unsupported-profile builds testable and their rejection collective; an unavailable optional threshold alone must not disable a valid conservative admission.

## Task 3: Preserve storage ownership across local amplitude transfer

**Files:** `matching/collective/state.rs`, its tests, and feature-appropriate compile-fail documentation.

**Interfaces:** `MatchingAmplitudes<'env>` owns a `Vec<Complex64>` and a reservation; exposes only borrowed slices, length and byte metadata. `MatchingState::amplitudes(count) -> Result<MatchingAmplitudes<'env>>` allocates a charged input buffer; `read_local_amplitudes(start,count)` returns the same owner type; `write_local_amplitudes(start,&MatchingAmplitudes)` validates its environment and local range. The buffer may outlive the state but cannot outlive the environment. Declare the payload before its reservation so payload destruction precedes charge release; cloning/resizing/sharing is excluded initially. No `into_vec`, ownership-returning iterator or uncharged extraction exists initially.

- [ ] Write a failing lifetime regression: retain a read result after the read and after dropping its state, show that its charge remains in `allocated_bytes`, reject an otherwise-fitting concurrent allocation, then show recovery when the result drops. Repeat with two outputs.
- [ ] Admit the read's full simultaneous native-wire and result capacities before allocation. Keep both charges through conversion; release the wire only after its owner drops, then retain the result's actual capacity charge in `MatchingAmplitudes`. Avoid assuming iterator collection reuses storage.
- [ ] Allocate arbitrary input states through the charged buffer factory. Admit and release the additional CXX packing buffer around writes; reject a buffer from another environment collectively before mutation. A caller copying a borrowed slice creates caller-owned external storage and must keep a corresponding `reserve_external_bytes` lease; document that this ledger does not inspect arbitrary caller allocations.
- [ ] Add malformed range/NaN, overflow, one-rank allocation failure, returned-owner compile-fail and repeated read/write recovery tests. Verify no complete-state broadcast or gather is introduced.

## Task 4: Connect only the audited matching operations

**Files:** `matching/collective/state.rs`, `matching/collective.rs`, `matching/batched.rs` only if required to share the existing private core; existing whole-unitary tests and new state-admission tests.

**Interfaces:** restricted `init_zero()`, `init_plus()`, `total_probability()`; `PreparedMatching::apply_state(&mut MatchingState, adjoint, outer_mask, outer_value)` and `apply_state_scalar` share the existing matching implementation. Existing unrestricted methods remain unchanged.

- [ ] Write red whole-unitary tests through the new owner at P1/P2/P4/P8 and a split communicator, including arbitrary complex input, distributed color targets, both control signs, inactive sectors and repeated mixed scalar/fused forward/adjoint calls.
- [ ] Before every operation, agree owner/order/arguments, validate the collectively agreed native protocol/profile and reserve any additional simultaneous control payload. The initial state owner already retains the conservative request allowance; do not double-charge it. Optional narrowed request admission must revalidate its threshold and reserve any increase before mutation. Include all other live state, preparation and amplitude-output reservations in rank/node admission. Reject before native mutation; retain current fatal handling afterward.
- [ ] Delegate only to audited initialization, norm and matching code. Keep the existing exclusive routing interval; no general operation is allowed while it borrows the communication array.
- [ ] Run existing missing-buffer and post-staging fatal tests plus budget-rejection/recovery tests through the restricted entry. Confirm the failed preflight leaves all local amplitudes unchanged and the next valid collective operation succeeds.

## Task 5: Review and demonstrate the scoped policy

- [ ] Run matching/LCU/persistence integration tests, native ABI tests, applicable compile-fail tests, formatting, strict Clippy, default/all-feature workspace checks, Nextest and doctests with the verified native MPI profile. Record exact commands, counts and failures.
- [ ] Obtain an independent lifetime/collective/native-storage review before changing the capacity example. The review must distinguish payload guarantees from opaque native and process overhead.
- [ ] In a separate harness change, version restricted-state telemetry, preserve v1–v5 validation, record queried ABI/tag metadata, unknown optional thresholds and the selected admission profile, and rerun small local numerical cases with explicit rank binding. Keep actual native arrays, modeled payload, conservative allowances and measured RSS/address space separate.
- [ ] Freeze source and executable identities before any later multi-host experiment. Stage-aware admission must account for producer/publication/loader/native overlap and actual worker-stack lifetimes; the current flat managed-budget-plus-stack preflight is not changed by this API plan.

Completion of this plan means the restricted policy is justified and tested. Strict capacity remains open until actual canonical stored input exceeds every participating node's enforced cap and the full authorized execution completes within those caps. Source generation, modeled fit, an allocated register or successful admission alone is insufficient.
