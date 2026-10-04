# quest-cfd local integration verification

This record covers the implementation, tests, case manifests, research receipts
and crate documentation integrated from `codex/quest-cfd`. Its starting commit
is `77ae6879dfd3245bee78fe5d9a122f4a2ae6b95c`. It supplements the
[implementation acceptance record](2026-10-04-quest-cfd.md); it does not close
the open research requirements listed there or in the
[next-step plan](../../crates/quest-cfd/NEXT_STEPS.md).

## Fresh checks on 2026-10-05

The installed native environment is QuEST 4.3.0 with MPI/subcommunicator support
and matching MPICH 4.2.2. Native discovery verified the rsmpi compiler, QuEST ABI
and loaded library. Tests ran with local MPI socket access and one OpenMP thread.

| Check | Result |
| --- | --- |
| `cargo nextest run --locked --workspace --all-features --no-fail-fast` | 1,237 passed, 3 existing scale tests skipped; 106.355 seconds |
| `cargo test --locked --workspace --all-features --doc` | 65 passed, 1 ignored |
| `cargo clippy --locked --workspace --all-features --all-targets --no-deps -- -D warnings` | Passed |
| `cargo fmt --all --check` | Passed |
| `cargo run --locked -p xtask -- generate-quest-bindings --check` | Passed |
| `git diff --check` | Passed |
| Crate documentation links | 90 local links resolved |

Use the environment below with the commands above; paths are placeholders:

```sh
export QUEST_ROOT=/path/to/installed/quest
export MPICC=/path/to/matching/mpicc
export PATH=/path/to/matching/mpi/bin:$PATH
export CARGO_TARGET_DIR=/path/to/validation-target
export CARGO_INCREMENTAL=0
export CARGO_BUILD_JOBS=2
export CARGO_PROFILE_DEV_DEBUG=0
export CARGO_PROFILE_TEST_DEBUG=0
export OMP_NUM_THREADS=1
```

The all-feature suite includes native matching, compact QSVT, split-communicator
and bounded native-failure regressions. These are local tests, not actual
multi-host capacity acceptance. The existing nightly `generic_const_exprs`
compatibility warning remains.

The documentation review checked equations, implementation boundaries and
dated receipt values against source. It corrected the complete-constraint
projection description and explicitly restricted reciprocal polynomial bounds
to the QSVT interval. All 1,545 non-Markdown files in the pre-documentation
snapshot retained identical content through documentation work. Proposed
additions were checked for private paths, secrets and accidental build artifacts;
JSON receipts and manifests parsed successfully.

The integration does not rerun the standalone release benchmark/smoke campaigns
or establish physical convergence, complete distributed source preparation,
paper-specific PREP/UNPREP, higher BDM order or beyond-node-memory capacity.
Those retain the evidence and limitations of the earlier record. The crate
[reading guide](../../crates/quest-cfd/README.md) links the detailed theory,
quantum/distributed contracts, alternatives and annotated references.
