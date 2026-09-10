# Circuit frontend and optimization research

Date: 2026-09-10. Supports the [proposed architecture](2026-09-10-quest-rust-design.md). Recommendations below are design inferences from the cited sources, not claims that an external library already implements the proposed quest-rs architecture.

## Research outcome

Start with a Rust procedural macro, shared validated semantic IR, and conservative optimization inside admitted unitary regions. Keep external compiler representations behind adapters. OpenQASM text import/export comes later, as the user requested.

The supplied [awesome-quantum-compiler collection](https://github.com/yucheol-choi/awesome-quantum-compiler) is useful for discovering projects. Technical decisions should rely on specifications, papers, and maintainers' documentation rather than treating that collection as verification of a project's suitability.

### What the supplied paper contributes

[Booth, arXiv:1206.3348](https://arxiv.org/abs/1206.3348) studies faster Fowler-style searches for approximate **single-qubit** discrete gate sequences, using bidirectional search, improved lookup, and compact SU(2) representations. Its search remains exponential and its distance treats global phase as irrelevant. Its reported speedups concern that search problem, not arbitrary circuit DAG optimization or QuEST runtime performance.

Use it as a candidate technique for a later synthesis component with an explicit target gate set and metric. Its phase convention is insufficient by itself for reusable subcircuits that may subsequently be coherently controlled. Preserve/correct phase or restrict the use context. Read the [full paper](https://arxiv.org/pdf/1206.3348) before translating implementation details.

## Frontend and library choices

| Component | Evidence | Recommendation |
|---|---|---|
| Procedural macro | Rust defines function-like procedural macros over token streams in a separate proc-macro crate | Parse an explicit grammar directly; emit ordinary builder calls. [Rust reference](https://doc.rust-lang.org/reference/procedural-macros.html) |
| `syn`, `quote`, `proc-macro2` | Maintainer projects support custom parsing, token emission and testing; MIT OR Apache-2.0 | Use focused features, preserving source spans and renamed-crate hygiene. [syn](https://github.com/dtolnay/syn), [quote](https://github.com/dtolnay/quote), [proc-macro2](https://github.com/dtolnay/proc-macro2) |
| `petgraph` | StableGraph retains other live indices after deletion, but internally reuses vacant slots; it supplies algorithms, not quantum invariants | Private graph substrate with independent public IDs, cycle validation and stable scheduling policy. [StableGraph](https://docs.rs/petgraph/latest/petgraph/stable_graph/struct.StableGraph.html), [implementation](https://docs.rs/petgraph/latest/src/petgraph/graph_impl/stable_graph/mod.rs.html) |
| Qiskit `oq3_*` | Inspected 0.7.0 syntax/semantic crates separate lexing, parsing, source management and type/name analysis; Apache-2.0; official implementation inventory still describes work in progress | Evaluate when text import begins; test the exact chosen language profile rather than assuming full conformance. [Manifest](https://raw.githubusercontent.com/Qiskit/openqasm3_parser/main/Cargo.toml), [semantic model](https://docs.rs/oq3_semantics/latest/oq3_semantics/index.html), [inventory](https://github.com/openqasm/openqasm/blob/main/implementations.md) |
| `quizx` | Inspected 0.3.0, Rust port of core PyZX functionality, Apache-2.0; rules expose applicability checks and unchecked variants | Optional later ZX adapter, preserving scalar/phase and validating extraction. [Crate](https://docs.rs/quizx), [rule preconditions](https://docs.rs/quizx/latest/quizx/basic_rules/index.html) |

Context7 was used for petgraph and CXX documentation discovery. Its synthesized petgraph summary contained misleading index-stability language; primary StableGraph source was checked before selecting the identifier policy. Context7 results are documentation leads, not a substitute for the library's actual contracts.

CXX opaque types do not automatically establish native thread safety; manual Send/Sync promises require evidence. UniquePtr supplies C++ destruction, while pinned mutable references protect opaque object placement. Neither mechanism enforces QuEST's global lifecycle on its own. [CXX opaque C++ types](https://cxx.rs/extern-c%2B%2B.html).

Two textual parser projects were considered without selecting them: [jlapeyre/openqasm-rust](https://github.com/jlapeyre/openqasm-rust) describes itself as a crate-to-be using an OpenQASM 3.0 ANTLR grammar and a third-party Rust target; [tuomas56/openqasm-rs](https://github.com/tuomas56/openqasm-rs) documents OpenQASM 2.0 with version 3 as future work. They are not evidence of a complete current 3.1 frontend.

## OpenQASM semantic contract

The official [release page](https://github.com/openqasm/openqasm/releases) identifies **3.1.0** as the latest stable specification at this inspection. [openqasm.com](https://openqasm.com/) displays a live specification, then labelled `spec/v3.1.0-78`. Pin the [versioned 3.1 specification](https://openqasm.com/versions/3.1/) or exact specification tag for gate semantics and later conformance tests.

The initial macro is an OpenQASM-style Rust DSL with a documented subset. A Rust token stream is not an original OpenQASM source file. Rust warns that `TokenStream::to_string()` whitespace can change, while OpenQASM pragmas/annotations have line-delimited syntax. Do not parse stringified tokens as though they preserved the user's source. [TokenStream contract](https://doc.rust-lang.org/proc_macro/struct.TokenStream.html), [OpenQASM directives](https://openqasm.com/versions/3.1/language/directives.html).

OpenQASM's global phase becomes relative phase when controls are added. Its `U` gate also has a specified phase convention that differs from earlier definitions. Therefore the IR must preserve phase during composition, inverse operations, and decomposition. [Gate semantics](https://openqasm.com/versions/3.1/language/gates.html).

OpenQASM `angle[n]` has fixed-width modular semantics. The proposed exact pi-expression plus finite-radians representation is an internal Rust representation, not an implementation of that complete type system. Later text lowering must implement the admitted type semantics or reject them explicitly. [Types and casting](https://openqasm.com/versions/3.1/language/types.html).

Measurement both records a classical outcome and changes quantum state; reset discards the old state and prepares zero. Neither may be crossed by generic unitary cancellation. [Nonunitary instructions](https://openqasm.com/versions/3.1/language/insts.html). For dynamic circuits, returned classical data is part of the semantics, not just the final density operator after forgetting all outcomes. [Dynamic circuit equivalence](https://arxiv.org/abs/2106.01658).

Arbitrary interpolated Rust expressions are evaluated once in source order before their admitted values reach optimization. Typed symbolic parameter slots are distinct from those computations. This prevents an optimizer from changing host-side behavior when removing or moving a quantum operation.

## QSVT reference: reuse and limits

Fresh source inspection found the reference checkout clean at `7fe7f740579b03c52a8cf48be6a31268b029c19f`; no native builds were performed there.

| Reference under `/var/home/erich/Projects/quest-qsvt` | Lesson |
|---|---|
| `src/qsvt_tools/src/circuit/arena.hpp:35`, `execution_walk.hpp:210` | Nodes share sequence/adjoint/control/shift expressions; walking them executes occurrences in order. This is composition sharing, not a general scheduling DAG. |
| `src/qsvt_tools/include/circuit/values.hpp:14`, `src/qsvt_tools/src/circuit/values.cpp:87` | Typed qubits/angles and signed validated controls transfer well. Canonical control order must not change ordered matrix targets. |
| `test/qsvt_tools/executor/quest_executor_lowering_tests.cpp:143` | The asymmetric `[3, 0]` target-order test is a valuable lowering fixture. |
| `src/qsvt_tools/src/backend/quest/detail/validation.hpp:97`, `planning.hpp:27` | The current validated lowered program still borrows mutable state and planning rejects rvalues. Owning Rust transitions are a proposed improvement, not existing C++ behavior. |
| `src/qsvt_tools/src/circuit/freeze.cpp:1049`, `src/qsvt_tools/src/backend/quest/executor.cpp:1930` | Validate/freeze storage and prepare native resources transactionally before publication. |
| `src/qsvt_tools/src/backend/quest/detail/content_catalog.hpp:130` | Hash lookup must confirm exact content; hashes are not proofs of identity. |
| `src/qsvt_tools/src/circuit/numeric_environment.cpp:94`, `src/qsvt_tools/CMakeLists.txt:64` | Numerical evidence depends on actual floating-point assumptions; typestate cannot freeze ambient runtime/backend configuration. |

Exclude QSVT phase synthesis, projector-route policies, postselection, query-degree bookkeeping, and theorem certificates from the general core. They can be consumers of the new circuit APIs. Borrow the ownership and evidence boundaries without silently importing QSVT-specific promises.

## Staged transformations

| Tier | Passes | Required domain / evidence | Primary source |
|---|---|---|---|
| Admission | Bounds, arity, signed-control validity, symbols, effects, DAG acyclicity, checked dimensions | Shared validator; parser success alone is insufficient | [OpenQASM grammar caveat](https://openqasm.com/versions/3.0/grammar/index.html) |
| Local exact algebra | Identity removal, known inverse cancellation, exact symbolic same-axis rotation merging | Unit-only region; phase-correct definitions; no epsilon deletion; inspect intervening effects | [VOQC](https://arxiv.org/abs/1912.02250) |
| Checked movement | Independent-wire movement, proven Pauli/diagonal commutation, bounded template matching | Valid commutation predicate plus quantum/classical/effect constraints; deterministic search | [Iten et al.](https://arxiv.org/abs/1909.05270) |
| Simulator planning | Bounded fusion and bound-payload caching | Preserve order inside a fused block; explicit width/bytes limit; phase-correct layout; numerical differential tests | [qsim fusion](https://quantumai.google/reference/cc/qsim/class/qsim/basic-gate-fuser) |
| Structured unitary regions | Clifford simplification and CNOT/phase-polynomial resynthesis | Recognized gate fragment, phase and ancilla conventions, target cost model | [Amy–Maslov–Mosca](https://arxiv.org/abs/1303.2042) |
| Optional ZX | Simplification plus circuit extraction | Supported fragment, scalar tracking, checked rewrite premises, successful extraction and cost check | [Duncan et al.](https://arxiv.org/abs/1902.03178) |
| Optional synthesis | Discrete-basis approximation, single-qubit search | Explicit gate set, error metric/budget and phase/call-context contract | [Booth](https://arxiv.org/abs/1206.3348), [Ross–Selinger](https://arxiv.org/abs/1403.2975) |
| Optional hardware target | Layout, routing and scheduling | Device connectivity, physical/logical mapping, timing semantics | [Qiskit transpiler stages](https://quantum.cloud.ibm.com/docs/en/api/qiskit/transpiler) |

Wire order and commutation dependence are distinct: Qiskit exposes both [DAGCircuit](https://docs.quantum.ibm.com/api/qiskit/qiskit.dagcircuit.DAGCircuit) and [DAGDependency](https://docs.quantum.ibm.com/api/qiskit/qiskit.dagcircuit.DAGDependency). Start conservatively and relax order only through justified transformations.

Gate fusion, T-depth optimization, and hardware routing optimize different costs. A QuEST simulator does not normally need physical-connectivity SWAP insertion. Measure native kernel/application count, memory, preparation overhead and repeated execution rather than equating fewer source gates with faster simulation.

Noise policy matters: explicitly encoded channels are program effects and must be preserved. A separate noise model injected after compilation describes the compiled implementation. Removing inverse ideal gates before such injection changes the modeled experiment compared with injecting noise after every original source gate. These modes must be named and never silently exchanged.

## Correctness and validation policy

Exact algebraic identities preserve the ideal map; they do not guarantee bitwise-identical floating simulation. Approximate passes must identify their metric, phase convention and composable budget. Ross–Selinger's optimality/runtime statements have stated factoring or number-theoretic qualifications; do not detach those conditions when selecting a synthesis method. [Paper](https://arxiv.org/abs/1403.2975).

Small-matrix and state differential tests are useful regression evidence, but are not formal proofs for arbitrary symbolic inputs. VOQC demonstrates proof against mathematical semantics. Parameterized circuits, ancillas, and incomplete checking procedures need additional care. [VOQC](https://arxiv.org/abs/1912.02250), [parameterized equivalence](https://arxiv.org/abs/2210.12166), [ZX checking limits](https://arxiv.org/abs/2208.12820).

Validate a rewrite before comparing its target cost; a smaller incorrect circuit is not a candidate result. Keep a reference interpreter/unoptimized path and record transformations, settings, bindings, seeds and backend capabilities. Distinguish an exact symbolic proof rule from empirical numerical evidence.

## Zotero status

The requested Zotero plugin helper was used to check the local service. Zotero 10.0.2's connector is running, but every library API probe returned HTTP 403 with `Local API is not enabled`. No local library search could be completed. No library records were added or changed, and no attachment/full-text library access was attempted. Research above uses the linked public primary sources; it does not claim Zotero-backed citations or library imports.

## Implementation decisions (2026-09-10)

The initial optimizer implements conservative adjacent exact identities, inverse
pairs and exact angle merging, plus bounded numerical fusion on identical ordered
target/control interfaces. Effectful instructions and barriers terminate fusion
blocks. Reports retain occurrence provenance and identify changed rounding; these
passes do not implement the full synthesis algorithms surveyed above.

Faer 0.24.4 supplies dense complex mathematics with explicit sequential policy.
Matrix admission accounts for its padded column storage and reads logical views.
Numerically checked unitarity remains empirical evidence, separate from exact
symbolic capability. No whole-circuit dense matrix is constructed by the executor.
See [faer matrix layout](https://docs.rs/faer/latest/faer/mat/type.Mat.html),
[features](https://docs.rs/crate/faer/latest/features), and
[parallelism](https://docs.rs/faer/latest/faer/enum.Par.html).

OpenQASM-style Rust tokens are the first frontend; text import/export and advanced
ZX, synthesis and hardware-routing passes remain separately scoped later work.
