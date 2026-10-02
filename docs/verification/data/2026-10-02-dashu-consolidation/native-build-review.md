# Native build watch-set correction

The unchanged-build investigation identified generated CXX/CMake outputs being registered as Cargo inputs. `bridge_input_file` watched its own `OUT_DIR` and generated source; `watch_inputs` watched generated `CMakeCXXCompiler.cmake` and `CMakeSystem.cmake`. These are recreated after Cargo starts the build script, making later unchanged builds appear stale.

Both emission sites now share one private `emit_input_watches` helper. An input is omitted only when its successfully canonicalized path is inside this build script's canonical `OUT_DIR`. External files and other packages' outputs remain watched. Unresolved paths remain watched conservatively. Canonical containment preserves paths that leave `OUT_DIR` through `..` or a symlink; no path-normalization framework or public API was introduced. Generated sources remain CMake build inputs.

Observed test-first evidence:

- The initial subprocess regression failed with exactly four self-watches: the output directory, generated CXX source, CMake compiler configuration and CMake system configuration. It passed after the watch-set correction.
- Independent review found that the initial lexical-prefix shortcut could suppress external inputs. The extended regression failed for `OUT_DIR/../external.cpp`, an `OUT_DIR` symlink to external headers, and an unresolved future external include directory. Removing that shortcut made all cases pass while actual generated outputs remained excluded.
- The regression also verifies retained external source, package headers, imported library, package CMake configuration and sibling output-directory watches.

Final scoped validation used `CARGO_BUILD_JOBS=2 devenv shell --`:

| Command | Result |
| --- | --- |
| `cargo test -p quest-build --locked cargo_watches_external_inputs_without_watching_its_own_outputs -- --nocapture` | Passed after both observed red cases |
| `cargo test -p quest-build --locked` | 41 passed, 0 failed, 0 ignored; 10.01 s test execution |
| `cargo clippy -p quest-build --all-targets --locked -- -D warnings` | Passed |
| `cargo fmt -p quest-build` | Applied |

Raw final logs are `native-build-tests.log.gz` and `native-build-clippy.log.gz` beside this report. Expected failure-fixture CMake diagnostics occur in the successful test log. This report establishes watch-set behavior and scoped regressions; actual no-op rebuild timing and native-consumer invalidation are separate root-task checks.
