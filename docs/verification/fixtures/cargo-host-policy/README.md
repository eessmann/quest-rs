# Cargo host and target compiler policies

This independent, dependency-free fixture checks the documented nightly Cargo
host configuration. The build script requires a host-only configuration marker;
the executable requires a distinct target-only marker. Both reject leakage from
the other configuration and check runtime subnormal arithmetic using
`black_box`, rather than a constant-folded expression. Rust unsafe code is
forbidden by the package's lint configuration.

The motivating Cray failure occurs before compile-fail fixtures run: Cargo's
explicit `--target` separates host build scripts from target Rust flags, while
the default bundled linker rejects options emitted by the Cray wrapper. This
fixture neither patches the compiler nor changes trybuild's target selection.
The supplied target arrays model the configuration mechanism used by trybuild;
actual trybuild acceptance remains a separate required test.

[Cargo host configuration](https://doc.rust-lang.org/cargo/reference/unstable.html#host-config)
documents `host-config` and `target-applies-to-host`. Standard generic settings
`CARGO_HOST_LINKER` and `CARGO_HOST_RUSTFLAGS` configure the host without a file.
An architecture-specific host table, when present in Cargo configuration,
overrides the generic table. Setting only an architecture-specific environment
leaf does not create that table; inspect the actual verbose compiler commands.
[Cargo host selection](https://doc.rust-lang.org/nightly/nightly-rustc/src/cargo/context/target.rs.html#100-108)

The [Cray batch probe](../cirrus/cargo-host-policy.sbatch) records a control with
the incompatible default host linker, then tests documented flags with implicit
and explicit native targets. Each case uses a fresh target directory. It
records every exit status, preserves logs and verifies source contents before
and after execution. No MPI or QuEST execution is claimed by this fixture.
