# quest-circuit

Native-independent circuit construction, validation, parameter binding, exact
local optimization and bounded numerical fusion. This crate does not discover,
link or initialize QuEST. `quest` provides the native preparation and execution
layer.

```rust
use quest_circuit::{Angle, Gate, ProgramBuilder};

let mut builder = ProgramBuilder::new(2, 0)?;
let q = builder.qubit(0)?;
let theta = builder.parameter("theta")?;
builder.gate(Gate::Rx(Angle::parameter(theta)), &[q], &[])?;
let program = builder.finish()?;
let bound = program.bind(&[(theta, 0.25)])?;
let plan = bound.lower()?.plan()?;
# Ok::<(), quest_circuit::Error>(())
```

Identifiers belong to their originating program. Gate definitions contain
complete immutable `UnitaryCircuit` bodies; calls validate ordered arguments and
parameter substitution before appending any operations, allocate fresh
occurrence IDs, and preserve explicit body dependencies. Fully expanded bodies
cannot contain recursive or unresolved calls. Expansion obeys the caller's
operation limit. General programs can contain measurement, reset, classical
conditions, arbitrary finite numerical operators and complete Kraus channels.
Only symbolic unitary programs admit circuit adjoint and coherent control.

## Macro profile

The default `macros` feature exports `circuit!`. Disable default features for the
builder alone. The facade reexports the same macro; renamed direct and facade
dependencies are supported. This is an **OpenQASM-style Rust DSL**, with gate
semantics pinned to OpenQASM 3.1.0.

```rust
use quest_circuit::circuit;
let theta = 0.25;
let program = circuit! {
    qubit[3] q;
    bit c;
    h q[0];
    cx q[0], q[1];
    negctrl @ inv @ rz(2*pi) q[2], q[1];
    rx(${theta}) q[0];
    ctrl @ gphase(pi/2) q[2];
    c = measure q[1];
    reset q[1];
    barrier q[0], q[1];
}?;
# Ok::<(), quest_circuit::Error>(())
```

- Declarations come first: `qubit q;`, `bit c;`, or nonempty static arrays such
  as `qubit[3] q;`. Array operands require a literal index. Broadcasting and
  dynamic indices are unsupported.
- Gates: `id`, `x`, `y`, `z`, `h`, `s`, `sdg`, `t`, `tdg`, `sx`, `rx`, `ry`,
  `rz`, `p`, `cx`, `cy`, `cz`, `swap`, `ccx`, and `U` (also `u`). `U` takes
  `(theta, phi, lambda)` and has matrix
  `[[cos(theta/2), -exp(i*lambda)*sin(theta/2)],
  [exp(i*phi)*sin(theta/2), exp(i*phi)*exp(i*lambda)*cos(theta/2)]]`.
- `ctrl @`, `negctrl @`, and `inv @` may be chained. `ctrl(n) @` and
  `negctrl(n) @` use a positive literal count. Controls precede targets in the
  operand list. `gphase(angle);` is explicit global phase; controlled global
  phase acts only in the selected control subspace.
- Angles accept exact rational arithmetic involving `pi` and integer literals,
  finite decimal literals, or `${ Rust expression }`. Plain integer/rational
  literals are radians. Exact macro arithmetic uses checked signed 128-bit
  intermediates and signed 64-bit final numerator/denominator; the builder's
  fallible `Angle::rational_pi` admits arbitrary normalized `BigRational`
  coefficients. Zero denominators and nonfinite values are errors.
- Interpolations evaluate exactly once in source order when construction reaches
  them. A failed admission stops construction. Interpolated values stay opaque
  to exact optimization; named reusable parameters are available in the builder.
- Measurement supports `c = measure q;` and `measure q -> c;`. Reset takes one
  qubit. `barrier;` covers all qubits; `barrier q[0], q[1];` is scoped.
- Includes, text parsing/export, gate declarations, loops, dynamic indexing,
  broadcasting and classical control syntax are outside this initial profile.
  The builder exposes definitions and `gate_if`.

## Numerical and optimization contracts

`NumericalOperator` owns an immutable `faer::Mat<Complex64>`. Admission copies
logical values from column/row layouts, transposes, adjoints, strided/submatrix
views and conjugated views. Target position zero is the local basis least
significant bit; target order is never sorted. Positive/negative controls are
separate from those target bits. Arbitrary finite numerical operators act as
`A|psi>` or `A rho A†`; empirical unitarity checks grant no exact inverse or
controlled-circuit capability. Channels require a finite tolerance and a
completeness residual within it.

Exact optimization performs adjacent identities, symbolic inverse cancellation
and rational-pi rotation merging. It does not commute through effects or barriers,
and explicit user order constraints conservatively disable exact rewrites.
Opaque angles and symbolic sums remain separate. Successful finite bindings stay
valid; exact identities may remove rational constants that would otherwise
exceed machine-radian range. Ideal phase is preserved, including the minus sign
of a full-turn spin rotation. Bit-identical numerical execution is not promised.

Fusion runs after binding and only combines adjacent operations with identical
ordered targets and controls. Width, operation count and matrix limits bound
each block. A block that cannot fit alongside retained program payloads and
temporary products is skipped. Products use explicit sequential faer kernels;
column matvec avoids hidden GEMM packing storage. Checked budgets include padded
matrix capacity and all retained input/output payloads, conservatively counted
per occurrence even when immutable allocations are shared. These limits cover
matrix storage; Rust metadata allocations are bounded by the configured program
counts, with no separate aggregate metadata-byte limit.

Reports contain before/after operation counts and dependency depth, removed
occurrences, rewrite provenance, matrix storage and peak matrix allocation,
elapsed pass time, and whether numerical rounding changed. Depth counts each
operation, including effects/barriers, as one layer and retains explicit,
quantum, classical and stochastic dependencies. Fusion reports no certified
approximation error bound.

The optional `codespan-reporting` feature adds `Error::render_source` for frontend
source text and validated byte spans. Core errors remain structured `thiserror`
enums; the macro uses compiler token spans for static diagnostics. Every
macro-built operation also retains a `SourceSpan` containing the compiler's
display filename and the half-open byte range of its gate, `measure`, `reset` or
`barrier` keyword in the original source. The filename honors compiler path
remapping and need not identify an on-disk file. A source resolver supplies the
matching full text to render those ranges. Macro expansion never reads files or
reconstructs source text. This is one operation location, with no expansion
stack; fusion keeps its first location and the occurrence IDs of all inputs.

Run pure acceptance checks with `cargo test -p quest-circuit --all-features`.
Tests include independent scalar complex fixtures, deterministic generated
circuits, source diagnostics, graph/effect invariants and compile-fail DSL cases.
