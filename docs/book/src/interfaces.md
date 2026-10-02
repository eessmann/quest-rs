# Choose a frontend

Text import, `circuit!`, `circuit_file!`, and `ProgramBuilder` construct the same staged program. Every executable follows `Program<Constructed> -> Program<Verified> -> Program<Lowered> -> Program<Executable>`, then `Environment::prepare` creates native resources. There is no migration macro or second native preparation API.

| Capability | Text and macros | `ProgramBuilder` |
|---|---|---|
| Typed scalar inputs/outputs, dynamic expressions | OpenQASM declarations and expressions | Typed `Expr<T>` and `Local<T>` handles |
| Runtime feedback | Measurement, reset, branches and loops | `measure`, `reset`, `if_else`, `while_loop` |
| Definitions and modifiers | `gate`, `def`, controls, powers and inverse | `define_gate`, `call_gate`, typed functions and `Modifier` |
| Arrays and references | Fixed arrays and readonly/mutable references | 1D `array` helpers; rank-indexed `RankedArray<T, R>`, array interfaces and mixed signatures |
| Exact angles | `${Angle::pi(1,4)?}` in the Rust macro | `angle(Angle::pi(1,4)?)` |
| Floating angles | Ordinary QASM arithmetic or `${f64}` | `angle(f64)` and floating expressions |
| Native numerical payloads | Explicit captured oracle declarations in the macro | `matrix`, `channel`, `oracle`, and finite `region` embedding |
| Includes | Explicit text resolver; compiler-tracked `circuit_file!` | Import an admitted module |

The macro checks grammar and types at expansion time and emits a reusable checked template. Each expansion materializes and verifies that template once; subsequent invocations clone it and evaluate captures once in lexical order. They do not parse or admit the source again.

A `QuantumRegion` is a finite capability for exact symbolic construction and compiler passes. It can contain effects, so its name grants no unitarity proof. Bind all its original parameter obligations before embedding it with `ProgramBuilder::region`, or import a whole capability with `Program::from_region`. Numerical matrix tolerance never grants symbolic unitarity.

`quest-language` owns syntax, typed values, SSA, exact angles, finite graphs and verification. `quest-compile` owns the canonical compiler API, macros, transformations, compilation stages and artifacts; `quest` adds native runtime ownership.

```rust
{{#include ../../../crates/quest-compile/tests/tutorials.rs:macro_bell}}
```

Typed subroutine signatures compose `ValueParameter<T>`, `ArrayRef<T, N>`, `RankedArrayRef<T, R>`, `QubitParameter`, and `QubitArrayParameter<N>` using nested pairs; nesting supports arbitrary mixed arity. `define_subroutine` and `call_subroutine` retain an indexed classical return type. `define_procedure` and `call_procedure` admit void, effectful calls, including measurement/reset on quantum references. Array references declare mutability, and the shared checker rejects incorrect shapes, foreign handles, writes through readonly references, and effectful calls from unitary gate definitions.

`Program<Verified>::specialize(&RunInputs)` fixes every remaining input exactly once; missing, unknown, wrongly typed or wrongly shaped inputs are errors. `specialize_partial` explicitly leaves omitted inputs dynamic. Scalar and fixed-array bindings retain their original names, types, values and publication history in compiled artifacts, while source export retains original input declarations. Loading rechecks binding types/shapes; historical metadata grants no equivalence proof for arbitrary imported SSA.

Prepared dispatch caches statically resolved gates in entry regions conservatively. Bound input values used as indices retain VM loads so specialization cannot move a deferred bounds trap before preceding effects. Static formal call targets and all dynamic control paths are not promised to have cached dispatch records.

`input_array` and `output_array` expose fixed-shape classical arrays at any admitted rank. `ranked_array` initializes row-major local storage; `ranked_read`, `ranked_write` and `ranked_slice` retain checked indexing and reference alias rules. `define_array_subroutine` and `call_array_subroutine` return array values, while `copy_array` and `assign_array` provide explicit copies. Existing scalar and one-dimensional constructors remain available. Array rank is a Rust type parameter; dimensions are checked values.

Numerical `matrix` and `channel` operands must be scalar quantum places. Index a register explicitly before passing its elements; whole registers are rejected during common admission, before any preceding effects can execute.

Plain QASM source export cannot faithfully represent exact Rust captures or native payloads and returns an explicit diagnostic for them. Compiled-artifact export retains these values and the optimized executable.
