# Frozen-source downstream native consumers

PASS: `xtask check-native-consumers` exited 0 against HEAD `0bbbd067b433b8c7deedd385d10cb84ab435f59e`. Sorted tracked/untracked `Cargo.toml`, `Cargo.lock`, and `crates/` source hash was identical before and after: `5d60a626b73f3ee65d0311fbd077d5af6e7400cbbe2e3885fbf5a291334c3d98`. See `source-before.sha256`, `source-after.sha256`, and HEAD snapshots. This rerun followed the final oracle facade lint refactor and the timed campaigns.

Command: `env -u LD_LIBRARY_PATH -u LD_PRELOAD -u LD_AUDIT QUEST_ROOT=/var/home/erich/Projects/opt/quest HDF5_DIR=/var/home/erich/Projects/quest-rs/.superpowers/worktrees/qsvt-port/.superpowers/sdd/2026-09-11-qsvt-port/hdf5-serial/install MPICC=/home/linuxbrew/.linuxbrew/bin/mpicc MPICH_CC=/usr/bin/gcc CARGO_TARGET_DIR=<this-directory>/xtask-target CARGO_BUILD_JOBS=4 cargo run -p xtask --locked --offline -- check-native-consumers --work-dir <this-directory>/fixture`. The outer and generated consumer targets were isolated. The harness also cleared those loader overrides for child builds, `readelf`, `ldd`, and executable runs.

| Consumer | RUNPATH | `ldd` | Numerical run |
| --- | --- | --- | --- |
| direct `quest-sys` | DT_RUNPATH; no DT_RPATH | `libQuEST.so.4` resolved; no missing library | Passed |
| facade `quest-rs` | DT_RUNPATH; no DT_RPATH | `libQuEST.so.4` resolved; no missing library | Passed |
| wrapped facade | DT_RUNPATH; no DT_RPATH | `libQuEST.so.4` resolved; no missing library | Passed |
| renamed facade | DT_RUNPATH; no DT_RPATH | `libQuEST.so.4` resolved; no missing library | Passed |

All four RUNPATHs include QuEST `lib64` and MPICH directories. The installed package was QuEST 4.3.0 at `/var/home/erich/Projects/opt/quest`, with `/usr/bin/c++`. `xtask.log`, fixture `build.log`, and per-consumer `*-readelf.log`, `*-ldd.log`, and `*-run.log` preserve the direct evidence. The run logs are empty because successful numerical assertions print nothing; xtask enforced each zero exit status. No files were staged or committed.
