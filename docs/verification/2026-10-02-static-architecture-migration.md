# Static architecture migration

This breaking migration starts from `001a2b656a5a80a60659a408f87f57670309a09b`.
Use the project-local `devenv shell`; its lock selects the measured nightly.
`rust-toolchain.toml` continues to request nightly for manually managed installations.
Stable Rust is not a compatibility target. Generic const expressions remain an
[incomplete nightly feature](https://doc.rust-lang.org/nightly/unstable-book/language-features/generic-const-exprs.html).

## Functions and arithmetic

Use `quest_polynomial::function!(|x| (1.0 + x * x).ln())`. It constructs a concrete
`Function<E>`; the macro evaluates captures once. `typed_function!`, interpreted
`Expr`, `to_dynamic`, paired derivative callbacks, and the old `Real` delegation
interface have been removed. There is no runtime numerical expression parser.
The compiler's OpenQASM AST and typed classical `Expr<T>` remain: they represent
actual file-driven programs, effects and mutable values, not numerical callbacks.

Import `GenericFunction` for `evaluate`, `first` and `jet`, and pass a mutable
backend and an owned scalar. `quest_numerics::arithmetic` owns `Backend`,
`PointBackend` and `EnclosureBackend`. Scalars need only `Clone`; intervals have
no total-order requirement. `F64Backend`, `Interval64Backend`, `MpBackend` and
`MpIntervalBackend` are concrete choices. `First`, `Jet` and bounded heap-backed
Jacobian evaluation use the same expression with statically selected derivative
order. Const-generic input/output dimensions are checked at construction.

Captured `f64` values mean their exact stored dyadic value. For other input
semantics, use `typed::exact(ExactConstant::Integer(...))`, `Rational(n,d)`,
`Decimal(String)`, or the arbitrary-size integer-string `Ratio` variant. Decimal
and rational values are imported directly into the selected arithmetic; they do
not pass through a preliminary binary64 evaluation.

Sealed library expressions have structural evidence. The open `GenericFunction`
interface is admitted with `AssumedFunction::new` and a `ConsistencyAssumption`.
An open enclosing backend also requires `EnclosureAssumption`. Only a structural
function paired with a sealed audited enclosure backend exposes unconditional
certificate access. A custom point backend or QR solver proposes candidates but
cannot establish a proof: enclosing arithmetic admits actual stored coefficients and their support once
before repeated proof evaluations. Immutable support metadata is then reused
inside the numerical kernels. Custom point backends must obey their documented
arithmetic and ordering laws.

## Approximation and root coverage

The owning `RemezRequest` replaces both Remez builders, including the former QSP
offline approximation engine. Its static `.degree::<N>()` and checked runtime
`DynamicShape` use the same exchange and extrema pipeline. The default binary64
request uses pivoted faer QR. An MP request selects `MpHouseholder`; it does not
switch algorithms after a failure. Arithmetic precision and certificate meaning
are separate choices: `Accuracy` selects uniform error, minimax gap, or both.

A successful report retains the original function, exact inputs, selected
polynomial, attempted precision/work, `UniformErrorCertificate`, and
`MinimaxGapCertificate`. Failures retain the owning request, latest candidate and
available root coverage. MP retries require an explicit bounded
`PrecisionAttempts` schedule; work and retained history storage are bounded.

`.export_binary64()` selects rounded coefficients **before** certification. The
exact exported payload is frozen and checked using enclosing arithmetic. Higher
proof precision cannot make an inaccurate binary64 export satisfy a smaller
tolerance. Without that option, the certificate concerns the stored MP polynomial.

`quest_numerics::roots` replaces the old contractors module. Newton, Krawczyk,
scalar/vector Hansen–Sengupta and the deterministic root-cover driver have generic
function/backend types. Low-level callbacks explicitly acknowledge
`Premise::EnclosesContinuouslyDifferentiableFunction`; that premise is not itself
proof of arbitrary callback consistency. Structural Remez derivatives are
obtained through AD from the same residual expression. Split branches are
retained, and exhaustion leaves unresolved coverage. Complete coverage does not
mean every box contains a root: existence, at-most-one and uniqueness are separate.

MP intervals use directed Astro-float operations and enclose trigonometric
extrema using an enclosed π and exact integer tests. Ambiguous large phase may
widen to `[-1,1]`. Invalid domains, poles, exponent policy and precision stalls
remain runtime obligations. Budgets count modeled application operations and
retained/workspace scalar storage; upstream transcendental internals, allocator
metadata and arbitrary callback temporaries are not an allocator-enforced cap.
`Backend::charge` reserves a batch of modeled opaque-kernel work. Nested budget
wrappers forward the reservation; a successful outer reservation remains charged
if an inner budget subsequently refuses it. This conservative work accounting
avoids an extra counting loop before optimized QR kernels.

## Compiler and native consumers

Replace `quest-circuit` dependencies/imports with `quest-compile` / `quest_compile`.
The compiler owns `circuit!` and `circuit_file!` exports. Renamed dependencies are
resolved by the macro frontend. The in-process synthesis feature directly depends
on `quest-synthesis`; `workers` independently enables external process clients.
There is no native-synthesis relay through the process client.

Compiler errors and evidence now have concrete owned types. Finite operations
enter the semantic/SSA pipeline directly with checked typed capture identities;
synthetic AST reconstruction and placeholder floating captures were removed.
Exact angle sources, capture order, effects, ownership, and independent replay
remain admission obligations.

Compiled publication, historical evidence, and frontend template representations
advance to version 2; unsupported old versions are rejected. Worker and scientific
interchange versions do not change because their representations did not change.
No legacy decoder was added. C++/scientific interchange remains an explicit
external boundary.

Set `QUEST_ROOT` to the exact installation prefix containing `include/quest.h`.
The historical explicit environment spellings and package-directory ancestor
normalization are rejected. Conventional CMake discovery remains supported.
The internal `QuEST_DIR` CMake argument pins the already admitted installation;
it is not an alternate public environment selector.

Native matrix preparation shares one checked resource/pool authority within each
prepared owner. Independent owners retain separate environment reservations and
lifetimes. ABI checks, RAII handles, ordered signed control profiles, loader
checks and process isolation remain in force. The binding generator requires the
reviewed adapter registry; it cannot bootstrap authority from generated output.

The former `benchmarks/architecture/run-functions.sh` targets removed interpreted
examples and is removed. Use the portable
[numerical](fixtures/static-architecture/numerical/README.md) and
[project](fixtures/static-architecture/project/README.md) comparison fixtures.
Dated verification instructions retain their original paths; reproduce them from
the historical revision named in each record.
