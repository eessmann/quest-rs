# Independent downstream native-consumer check

Result: PASS, xtask exit 0. Ran after beam source freeze from HEAD `0bbbd067b433b8c7deedd385d10cb84ab435f59e`. Source/manifest hash before and after was identical: `24bc68edc99e6a61fdffe3a67fec4c96dfa0efb28f34182254e81b0937ec01e5`. The hash is SHA-256 of sorted `sha256sum` records for `git ls-files -co --exclude-standard` under root `Cargo.toml`, `Cargo.lock`, and `crates/`; it includes tracked and untracked source. See `source-before.sha256`, `source-after.sha256`, and the HEAD/status snapshots in this directory.

Command: `env -u LD_LIBRARY_PATH -u LD_PRELOAD -u LD_AUDIT QUEST_ROOT=/var/home/erich/Projects/opt/quest HDF5_DIR=/var/home/erich/Projects/quest-rs/.superpowers/worktrees/qsvt-port/.superpowers/sdd/2026-09-11-qsvt-port/hdf5-serial/install MPICC=/home/linuxbrew/.linuxbrew/bin/mpicc MPICH_CC=/usr/bin/gcc CARGO_TARGET_DIR=<this-directory>/xtask-target CARGO_BUILD_JOBS=4 cargo run -p xtask --locked --offline -- check-native-consumers --work-dir <this-directory>/fixture`. The outer xtask target and generated consumer target were isolated from the timed release benchmark target. xtask stripped the same three loader overrides from each child Cargo, readelf, ldd, and executable invocation. The installed package reported QuEST 4.3.0 at `/var/home/erich/Projects/opt/quest`, compiled with `/usr/bin/c++`.

| Consumer | ELF policy | Native closure | Execution |
| --- | --- | --- | --- |
| direct `quest-sys` | DT_RUNPATH, no DT_RPATH | `libQuEST.so.4` resolved; no missing libraries | Quantum numerical check passed |
| facade `quest-rs` | DT_RUNPATH, no DT_RPATH | `libQuEST.so.4` resolved; no missing libraries | Quantum numerical check passed |
| wrapped facade | DT_RUNPATH, no DT_RPATH | `libQuEST.so.4` resolved; no missing libraries | Quantum numerical check passed |
| renamed facade | DT_RUNPATH, no DT_RPATH | `libQuEST.so.4` resolved; no missing libraries | Quantum numerical check passed |

All four RUNPATHs include the QuEST `lib64` and MPICH library directories. The fixture `build.log`, per-consumer `*-readelf.log`, `*-ldd.log`, and `*-run.log` retain direct evidence; zero-byte run logs accompany successful exit codes enforced by xtask. Full command output is `xtask.log`. This check validates CPU execution and the installed native library closure; it does not claim timed performance or GPU execution. No files were staged or committed.
