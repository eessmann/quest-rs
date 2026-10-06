# Cargo host and target policy on Cirrus Cray

Cirrus probe `538200` verified separate Cargo host and target compiler settings
with the installed nightly toolchain. The control failed at host linking with
exit 101; both configured cases built and executed successfully. This establishes
the compiler configuration mechanism. Actual trybuild and full-workspace
acceptance remain separate gates.

## Failure and documented configuration

The earlier Cray workspace failure occurred while trybuild built its
dependencies, before the compile-fail fixtures ran. Cargo's explicit `--target`
separates host build scripts from target Rust flags. The Cray `cc` wrapper then
reached Rust's bundled linker without the required host linker policy.
[Cargo documents this distinction](https://doc.rust-lang.org/cargo/reference/unstable.html#target-applies-to-host).

The independent probe uses Cargo's documented nightly `host-config` and
`target-applies-to-host` settings, enabled through their standard environment
forms. `CARGO_TARGET_APPLIES_TO_HOST=false` separates the two domains.
`CARGO_HOST_LINKER=cc` and `CARGO_HOST_RUSTFLAGS` configure host compilation;
the corresponding `CARGO_TARGET_X86_64_UNKNOWN_LINUX_GNU_*` settings configure
the target. Both configured domains use `-C linker-features=-lld` and
`-C link-arg=-mno-daz-ftz`. [Host configuration](https://doc.rust-lang.org/cargo/reference/unstable.html#host-config),
[environment mapping](https://doc.rust-lang.org/cargo/reference/config.html#environment-variables).

An architecture-specific host table overrides the generic table. Cargo checks
for that table through a configuration lookup which excludes environment
variables, so an architecture-specific environment leaf alone does not create
the table. The local exploratory attempt using only those leaves failed with
the missing-host-policy assertion; generic host settings passed. An exact-leaf
configuration query is therefore insufficient evidence of the selected host
policy. [Host table selection](https://doc.rust-lang.org/nightly/nightly-rustc/src/cargo/context/target.rs.html#100-108),
[lookup implementation](https://doc.rust-lang.org/nightly/nightly-rustc/src/cargo/context/mod.rs.html#837-848).

Locked trybuild 1.0.121 removes plain `RUSTFLAGS`, supplies build/target flag
arrays and normally passes `--target`. It retains the host configuration
environment. The probe supplies target arrays to exercise the same configuration
mechanism, while preserving the separate target environment flags. An inherited
`CARGO_ENCODED_RUSTFLAGS` has different precedence and is excluded from this
controlled probe. [Pinned trybuild source](https://github.com/dtolnay/trybuild/blob/1.0.121/src/cargo.rs),
[Cargo flag precedence](https://doc.rust-lang.org/cargo/reference/config.html#buildrustflags).

## Probe identity and results

The [dependency-free Rust fixture](fixtures/cargo-host-policy/README.md) forbids
unsafe code. Its build script requires a host-only configuration marker and
rejects the target marker; the executable makes the reciprocal assertions.
Both execute `black_box(f64::MIN_POSITIVE) * black_box(0.5)` and require a positive
result. This checks runtime subnormal arithmetic rather than a constant-folded
expression. It does not execute MPI or QuEST.

The [batch payload](fixtures/cirrus/cargo-host-policy.sbatch) uses one exclusive
node, eight requested CPUs, `lowpriority` and a twenty-minute limit. It retains
the central `cray-hdf5/1.14.3.5` module, CCE 19.0.0 and the ordinary Cray wrapper.
The recorded Rust compiler is `1.101.0-nightly (282215592 2026-10-04)`, LLVM 23.1.3.
Each case uses a fresh target directory and locked, offline Cargo commands.

| Case | Target selection | Host policy | Cargo result |
| --- | --- | --- | --- |
| Control | Explicit native target | Host marker and `cc`, without the two linker-policy flags | 101; host link failed |
| Implicit | No explicit target | Host marker and both linker-policy flags | 0; host and target assertions executed |
| Explicit | Explicit native target | Host marker and both linker-policy flags | 0; host and target assertions executed |

The control log records `-fuse-ld=lld` and `rust-lld` rejection of Cray plugin
options `lto=0`, `defaults=cray` and `mllvm=-cray-math-precision=none`. Its failure
is established by the actual diagnostic, not merely its nonzero exit status.
Both positive logs show the host marker only on the build-script rustc command,
the target marker only on the executable command, and both linker-policy flags
on each command. The explicit case includes `--target` only on target compilation.
Both print the fixture's success marker after the build script and executable run.

Slurm recorded `COMPLETED`, exit `0:0`, in ten seconds. Accounting reports 288
allocated CPUs for the exclusive node; the requested task CPU count was eight.
The payload's completion and exit statuses are zero, and source checks passed
before and after execution. The local implicit/explicit checks also passed;
their exploratory failures and logs are retained separately.

| Artifact | SHA-256 |
| --- | --- |
| Six-file probe manifest | `7713442ce58a9b4ed6d9d21f9f4b012e2a4d1b7099f53e3daf6688485bc735a2` |
| Probe deployment archive | `91cb82690ddb057e91ddcde9c35377b2ae7310577470cf747dd25b620b683463` |
| Retrieved probe receipts | `6d62db9f5c888bbd25e77f402abd0b546fec6dcc447bceb762d324c8136a5af9` |

The [machine-readable summary](data/2026-10-06-cargo-host-policy/summary.json)
records original artifact/log hashes, source identities, sanitized actual rustc
argument arrays and case outcomes. Raw captures remain under
`target/cargo-host-policy-20261006`; their private deployment paths are not
copied into this public record.

## Full build and stopped runtime follow-up

The reviewed Cray campaign profile now supplies host and target configuration
for ordinary Cargo invocations. It retains caller flags within their respective
channels, records the resulting environment and requires matching build/runtime
receipts. No trybuild patch, `trybuild_no_target`, special UI route, custom compiler
wrapper or native/vendor-library change is involved.

The candidate full-source manifest is
`e68d5468b006d0dd3c5bbb6420d9a5c5db5a5a0727aafd1b8bd93540055e32d0`
with 2,592 files; its archive SHA-256 is
`16bccef38794b94547d3b0d383be4d7f126d0cb72154a198eb984fc2eb0e344d`.
Cray Torc build `538206` completed with Slurm exit `0:0` in three minutes
49 seconds. Native doctor, binding freshness, default/all-feature workspace
compilation, the independent MPI consumer build and final source verification
all passed. The exported Torc job and result are completed, return code zero,
with matching workflow/job/run/attempt identities. The host/target settings are
present in the compiler receipts. This is compilation and discovery evidence;
compile-fail fixture execution was not part of the build stage.

The retained submission observation initially recorded that build
running on one node, Cray eight-node verification `538208` pending with
`afterok:538206,afterany:538207`, and GNU verification `538207` pending with
`afterany:538206`. The GNU job uses the separate `565f59…` source. These
dependencies preserve the eight-node aggregate limit.

The immutable submission receipt SHA-256 is
`35cb0cf03ff16b5d86f5d4ecbc8a891e4a469aee765fffa77134bc2531d44e82`.
After that observation, GNU runtime job `538207` was cancelled by the owner
after 23 seconds running, and Cray runtime job `538208` was cancelled while
pending. A bounded review had identified an HDF5 empty-library-path fallback
gap and a possible false positive in the fatal-drop fixture. Their corrections
require a new source identity and focused checks before further runtime
acceptance. The completed build and original probe evidence remain retained.

The final build-receipt archive, including terminal Slurm accounting, has SHA-256
`8f410013f26b13a2b9b9f52d2d6d8840b3e2912b9fa6a48f278790cf2218ce02`.
It records the successful build and both owner cancellations. No full-workspace
or runtime acceptance is claimed for either cancelled job. The earlier Cray
failures remain recorded in the
[Torc/runtime campaign](2026-10-06-torc-cirrus.md); the probe does not replace them.
