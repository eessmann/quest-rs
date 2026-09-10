# quest-sys

CXX bindings to **QuEST 4.3.x**, with double precision and deprecated APIs disabled.
Native discovery lives in the shared `quest-build` crate. The tested installed
build recipe is Linux GNU with CMake and a C++20 compiler.

Set `QUEST_ROOT` to the installation prefix, or select its package with
`QuEST_DIR` or `CMAKE_PREFIX_PATH`. CXX generates bridge sources; CMake compiles
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

Edit the adapter registry and generator templates together, then regenerate with
`cargo run -p xtask -- generate-quest-bindings`. `--check` verifies freshness.
Generation requires libclang; consuming this crate does not.
