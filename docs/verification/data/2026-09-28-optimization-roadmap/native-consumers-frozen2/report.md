# Frozen2 downstream native consumers

PASS: fresh `xtask check-native-consumers` exited 0 after the final circuit budget fix. HEAD before/after: `0bbbd067b433b8c7deedd385d10cb84ab435f59e`. Sorted tracked/untracked root Cargo and `crates/` source hash before/after: `7ff4b82e96d6166e9d390cf6f117854acc13b3913f2f7b8166341d592455076a`. The `source-*.sha256` and `head-*.txt` files record both snapshots.

The command used `cargo run -p xtask --locked --offline -- check-native-consumers --work-dir <this-directory>/fixture`, with `QUEST_ROOT=/var/home/erich/Projects/opt/quest`, the specified serial HDF5 and MPICH compiler environment, `CARGO_BUILD_JOBS=4`, and a separate `<this-directory>/xtask-target`. `LD_LIBRARY_PATH`, `LD_PRELOAD`, and `LD_AUDIT` were removed from the outer process; xtask removed them from child builds, ELF/closure inspections, and runs. The fixture workspace and target directory were new for this run.

| Consumer | ELF | Closure | Numerical execution |
| --- | --- | --- | --- |
| direct `quest-sys` | DT_RUNPATH, no DT_RPATH | `libQuEST.so.4` resolved, none missing | Passed |
| facade `quest-rs` | DT_RUNPATH, no DT_RPATH | `libQuEST.so.4` resolved, none missing | Passed |
| wrapped facade | DT_RUNPATH, no DT_RPATH | `libQuEST.so.4` resolved, none missing | Passed |
| renamed facade | DT_RUNPATH, no DT_RPATH | `libQuEST.so.4` resolved, none missing | Passed |

Installed QuEST 4.3.0 came from `/var/home/erich/Projects/opt/quest` with `/usr/bin/c++`; all four RUNPATHs include its `lib64` and MPICH library directories. `xtask.log`, fixture `build.log`, and the four sets of `*-readelf.log`, `*-ldd.log`, and `*-run.log` preserve direct evidence. Empty run logs accompany successful exit statuses enforced by xtask. No files were staged or committed.
