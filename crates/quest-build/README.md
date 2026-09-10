# quest-build

Shared installed-QuEST discovery and runtime-path configuration for `quest-sys`
and final executable build scripts. The current verified recipe supports native
Linux GNU targets, QuEST 4.3.x, binary64, and deprecated APIs disabled. It needs
CMake 3.24+, a C++20 compiler, and `ldd`. Cross compilation, macOS, Windows,
relocatable bundles, and static linker groups require separate recipes and fail
explicitly when unsupported.

From the repository, record the selected native installation:

```sh
QUEST_ROOT=/path/to/quest \
QUEST_RUNTIME_LIBRARY_PATH=/path/to/indirect/native/libraries \
cargo run -p xtask -- configure-native /absolute/path/quest-native.json
```

`QUEST_RUNTIME_LIBRARY_PATH` is a path-separated list. Supply any extra library
directories needed by indirect dependencies; for CUDA/cuQuantum this may include
the CUDA target library directory. Setup links a CMake executable against the
installed `QuEST::QuEST` target and runs it with `LD_LIBRARY_PATH`, `LD_PRELOAD`,
and `LD_AUDIT` removed. An incomplete native dependency closure fails setup.

The record includes the Rust target, canonical prefix, compiler, exact version
and installed configuration macros, CMake usage requirements, and SHA-256
identities for installed headers, CMake files, and resolved native libraries.
SONAME symlinks retain both their lookup paths and canonical targets. Changed
files or retargeted symlinks require regenerating the record. This record is
local build input and normally belongs outside version control.

Set `QUEST_NATIVE_CONFIG` to that same absolute file path when building all
consumers. Add this to the **final executable package**, including applications
that reach QuEST through another Rust library:

```toml
[build-dependencies]
quest-build = "0.1"
```

```rust,ignore
// build.rs
fn main() -> quest_build::Result<()> {
    quest_build::emit_final_target_runtime_paths()
}
```

Both this helper and `quest-sys` validate the same record and watch its native
inputs. The helper does not build a host copy of `quest-sys`. `QUEST_ROOT` and
`CMAKE_PREFIX_PATH` discovery remain available when there is no record, but
independent discovery is not evidence that separate consumers chose identical
native configurations.

For this Linux development recipe, the final executable receives
`--disable-new-dtags` and absolute `DT_RPATH` entries. Unlike `DT_RUNPATH`, these
paths participate in indirect dependency lookup. This also gives them priority
over `LD_LIBRARY_PATH`; use a new record and rebuild to select another native
installation. This is deliberately an absolute-path development policy. It
does not copy libraries, patch installed ELF files, or produce a relocatable
bundle. Native inputs must remain unchanged between validation and execution.

Run the standalone consumer acceptance fixture after building the workspace:

```sh
python3 crates/quest-build/scripts/check_consumers.py \
  --config /absolute/path/quest-native.json
```

It creates an independent workspace under `/tmp`, builds direct, wrapped, and
renamed-dependency consumers, checks their dynamic tags and `ldd` output, and
executes their macro and quantum state checks from outside the repository with loader variables removed. It
prints the preserved fixture directory and logs for inspection.
