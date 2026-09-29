# QSP/QSVT documentation and integration checks — 2026-09-11

These documentation checks supplement the
[QSP/QSVT numerical verification record](2026-09-11-qsvt-port.md).

## Documentation coverage

- QSP and QSVT READMEs are included directly as crate rustdoc, with executable
  canonical/generalized synthesis, encoding and transform examples.
- Public builders, owning stages, phase conventions, projectors, route semantics,
  resource policies and structured error outcomes have expanded API documentation.
- QSVT's analysis guide is also module rustdoc, including conditional-bound and
  numerical-diagnostic examples. Named mathematical assumptions remain explicit.
- The book connects polynomial preparation, synthesis, separate certification,
  pure QSVT construction, conditional analysis and prepared native execution.
- Root and facade setup guidance now distinguishes serial HDF5 application
  requirements from the facade's requirements, and local from collective runtime
  ownership. Crate rustdoc links use repository URLs rather than paths relative
  to the rendered HTML directory.
- The QSP executable tutorial is `qsp_tutorials`, avoiding an output collision
  with the facade's `tutorials` example during a workspace all-target build.

The documentation pass changed no numerical or native execution behavior.
Production remains binary64, optional cold precision uses Astro Float, and
production failures never dispatch to offline synthesis. A QSP certificate is
separate from a constructed QSVT transform and does not automatically cover phase
conversion, numerical lowering or native execution.

## Fresh local checks

The environment matches the implementation record: Linux x86_64, pinned
`nightly-2026-09-06`, QuEST 4.3.0 with MPI/SUBCOMM, MPICH 5.0.1 and serial HDF5
1.14.6. Native tests used CPU execution and local MPI subprocess sockets.
The checks used:

```sh
export QUEST_ROOT=/var/home/erich/Projects/opt/quest
export MPICC=/home/linuxbrew/.linuxbrew/bin/mpicc
export MPICH_CC=/usr/bin/gcc
export HDF5_DIR=/path/to/serial-hdf5-1.14.6
export CARGO_BUILD_JOBS=2
```

| Check | Result |
| --- | --- |
| `cargo build --workspace --all-features --all-targets --locked --offline` | Passed; tutorial output collision removed. |
| `cargo nextest run --workspace --all-features --locked --offline` | **560 passed**, 3 deliberately separate scale tests skipped. |
| `cargo test --doc --workspace --all-features --locked --offline` | **56 passed**, including ownership/stage compile-fail cases. |
| `cargo clippy --workspace --all-targets --all-features --locked --offline -- -D warnings` | Passed under the unchanged workspace lint policy. |
| `cargo fmt --all -- --check`, staged `git diff --check` | Passed. |
| `cargo doc -p quest-qsp -p quest-qsvt --all-features --no-deps --locked --offline` | Passed with `RUSTDOCFLAGS='-D warnings'` and deliberately nonexistent QuEST/libclang paths. |
| `mdbook build docs/book` | Passed; log and rendered chapters checked for unresolved includes. |
| `cargo run -p quest-qsp --example qsp_tutorials --all-features --locked --offline` | Passed, including certification and explicit offline tutorials. |
| `cargo run -p quest-rs --example qsvt --features qsvt --locked --offline` | Passed; complex overlap, repeated execution and snapshot after retirement demonstrated. |
| `cargo run -p xtask --locked --offline -- generate-quest-bindings --check` | Passed. |
| `cargo package --workspace --list --allow-dirty --locked --offline` | Passed; crate READMEs, analysis guide, tutorials, data and license notices included. This is a contents check, not publication. |

The three release scale tests, all 43 catalog certificates, four downstream
consumers, serial-native API hiding and C++ comparisons retain their earlier
results in the implementation record. They were not rerun for documentation
changes. This pass adds no GPU execution, multi-node MPI or alternate-provider
coverage.
