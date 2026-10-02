# Native/runtime consolidation report

Baseline: `001a2b656a5a80a60659a408f87f57670309a09b`. Host: Darwin aarch64,
project `devenv shell`, installed Nix QuEST 4.3.0 serial shared CPU/OpenMP prefix.
No C++ source checkout writes, no commits, no global formatter or workspace suite.

## Changes and ownership

- `quest-build` now accepts one explicit selector: `QUEST_ROOT` is the exact
  installation prefix containing `include/quest.h`. Package subdirectories are
  rejected rather than ancestor-normalized. `QUEST_DIR`, `QuEST_DIR`, `QuEST_ROOT`
  are rejected, including empty assignments, with migration guidance. Conventional
  `CMAKE_PREFIX_PATH` and imported target/dependency handling remain supported.
  Internal `-DQuEST_DIR` remains the precise CMake package selector after the
  reviewed prefix is validated; it is not an exposed historical environment alias.
- The generator requires checked-in `generated_adapters.json`; coverage receipts
  cannot bootstrap authority. Removed bootstrap parsing/name derivation and the
  genuinely unused `generated_names.txt` output/file. All Rust/C++ generated
  adapters and canonical reviewed adapter registry are unchanged on regeneration.
  Historical source/validation manifests still refer to the old name file and
  remain untouched as historical receipts.
- One transactional `MatrixPreparation` pool and `MatrixRecipe`-derived
  `admit_matrix` resource calculation serve ordinary region instructions,
  captured numerical payloads, and reachable oracle control profiles. Admission
  uses one shared source-identity/ordered-signed-profile set for each owner.
  Pool metadata is conservatively admitted; handles publish only after both
  forward/adjoint variants succeed. Kraus estimates share one helper as well.
- Native handles reside in the environment-borrowing prepared program/region.
  Payload/oracle caches retain index metadata and separate bounded scratch.
  Signed-control order, aliases, dynamic target mapping, nested adjoints,
  full phases, register/environment checks, interpreter limits, and density
  multiplication paths are preserved. `prepared_matrix_variants()` reports the
  total shared pool while the existing oracle counter reports its distinct used
  variants. Separate prepared owners retain separate reservations and handles.
- Owned runtime sources migrated to canonical `quest_compile`; root owns remaining
  callers and Cargo changes. Package/native documentation describes migration
  and shared pool semantics. `devenv.nix` already exported the exact QUEST_ROOT
  installation prefix, so no mutation was needed.

## Removal and retention inventory

Removed: ancestor selector normalization; explicit historical aliases; coverage
bootstrap and its snake-case name helper; unused generated names output; three
separate native matrix publication maps; duplicated matrix dimension/storage
arithmetic; duplicated Kraus storage arithmetic.

Retained: reviewed CXX signature registry/templates; safe owned RAII handles,
exception conversion, binary64/version/deprecated-API guards, exact imported
library identity and evaluated link order, compiler/sysroot/header coherence,
loader/RUNPATH checks, MPI ABI witness and subcommunicator guards, serial HDF5
admission, native resource/environment lifetimes, distinct meaningful runtime
control/target/depth dimensions, bounded oracle discovery/remapping scratch.

## Evidence

- RED: revised prefix fixture failed twice against prior behavior (lib and lib64
  package subdirectories were incorrectly accepted); revised missing-reviewed-
  registry fixture failed because prior write mode bootstrapped empty coverage.
- GREEN: `devenv shell -- cargo test -p quest-build --lib`: 40/40 tests, including
  imported target configuration, header identities, ABI failure, MPI witness,
  Darwin real consumer, HDF5, linker ordering, and selector migration tests.
- GREEN: `devenv shell -- cargo test -p xtask --bin xtask generate:: --
  --test-threads=2`: 22/22 generator tests.
- GREEN: `devenv shell -- cargo check -p quest-rs --lib`: current default runtime
  compiled after shared pool implementation (later compiler/numerics edits are
  ongoing).
- GREEN: regeneration and `generate-quest-bindings --check` succeeded. Coverage
  receipt now labels QUEST_ROOT; the current serial Darwin header inventory lacks
  one old gated-MPI declaration (`initCustomMpiCommQuESTEnv`), so its receipt entry
  was removed. Generated wrapper count remains 249. This does not validate MPI.
- Focused source formatting and `git diff --check` passed.
- Added pure admission tests for exact ordered signed/source alias identity,
  independent equal-valued sources, CPU/GPU forecast distinction, scalar minimum
  native dimension, and overflow without inventory publication.
- Added real runtime regression requiring two total variants for one shared
  negative-control payload/oracle matrix plus a distinct positive-control profile.
  Existing oracle runtime suite exercises full phase, permuted targets, density,
  nested adjoints, bounded profiles and preparation budgets.

## Runtime and independent consumer verification

- GREEN: the standalone current-source oracle_runtime smoke suite passed 7/7,
  including the new total shared pool count and existing full-phase, density,
  permutation, nested adjoint, budget and unused-capture cases. Its final-target
  build script uses the same quest-build runtime helper. This isolates unrelated
  quest-qsp dev dependencies while exercising real current native/compiler code.
- GREEN: full `check-native-consumers --backends cpu,omp --work-dir
  .superpowers/sdd/2026-10-02-static-numerical-core/native-consumers-final` passed,
  with CARGO_BUILD_JOBS=2. Direct state/density CPU/OpenMP checks, native mode
  checks, facade/wrapped/renamed consumer checks, installed native closure and
  Mach-O LC_RPATH/loader inspections all passed outside Cargo with loader
  overrides removed. Logs/fixtures are preserved in native-consumers-final/.
- The initial workspace oracle_runtime gate stopped in concurrently edited
  quest-numerics; the first external fixture compile stopped in the concurrently
  edited compiler builder. Those blockers were repaired by their owners before
  these successful current-source gates.
- Pure matrix_resource_tests and feature-gated region preparation tests await the
  integrated workspace test-dependency migration. Their source is included in the
  change; the default production runtime check and seven current-source runtime
  tests above have passed.
- No current Linux/MPI or accelerator runtime evidence was obtained. Darwin
  CPU/OpenMP acceptance does not establish MPI, HDF5/Linux or GPU execution.

## Rulings

- Share within one prepared owner, retain separate pools between owners: one
  owner has one environment reservation and identical recipe semantics, while
  independently prepared values have independent lifetime/resource contracts.
  Cost if wrong: cross-owner sharing would require a new lifetime/cache policy.
- Preserve internal CMake QuEST_DIR selection: it pins the validated exact prefix
  even alongside unrelated CMAKE_PREFIX_PATH entries. Cost if wrong: removing it
  would let conventional search select another installation despite QUEST_ROOT.
- Keep historical generated_names inventory references: they describe prior
  snapshots. Cost if wrong: treating them as current requires reading their date.
- Keep the regenerated Darwin coverage receipt: this is the current installed
  header inventory, not evidence that the old MPI declaration became unsupported.
  Cost if wrong: regenerating with another installation changes its coverage
  receipt; safe reviewed adapters remain separately authoritative.

Additional scoped lint pass found local visibility declarations redundant inside the private execution module and a preparation method exceeding the workspace line limit after shared admission. These were corrected without suppressing the lints: internal declarations use the module's existing visibility style, and static dispatch storage accounting is a checked helper. The post-adjustment `cargo clippy -p quest-rs --lib` gate passes (`native-clippy.log`). Integrated tests now also pass: the two matrix resource unit tests (`native-matrix-unit-tests.log`) and all seven oracle runtime tests (`native-oracle-integrated-tests.log`), using the actual package rather than the earlier temporary consumer.

Final native dispatch integration: `structured_runtime` passes5; feature-gated CPU `qsvt_runtime` passes5 and `qsvt_native_dispatches` passes1. Alongside oracle7 and matrix-unit2, twenty integrated native tests pass. Receipts: `native-structured-integrated-tests.log`, `native-qsvt-integrated-tests.log`. This exercises both structured prepared programs and PreparedRegion materialization after shared pooling. It is CPU runtime evidence only; no MPI/accelerator claim is made.
