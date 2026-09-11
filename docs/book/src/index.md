# Start here

The workspace provides two complementary program models. A **structured program** expresses classical computation, runtime control flow, calls, arrays, quantum operations, measurement, and feedback. An **ideal circuit** expresses a finite quantum dependency graph with exact symbolic angles, explicit effects, and bounded optimization passes. Both can be prepared for an installed QuEST backend.

Start with [the interface matrix](interfaces.md), then run the [Bell tutorial](bell.md). Every Rust tutorial shown here is included from a compiled example or integration test. Test fragments use `googletest` assertions and the imports in their containing source file; runnable native functions share the example module imports shown in the Bell chapter. The native example executes several algorithms inside one environment; its integration test starts a separate process to respect QuEST's process lifecycle.

For polynomial quantum algorithms, follow [polynomial preparation](numerical-polynomials.md),
[canonical and generalized QSP synthesis](qsp-synthesis.md),
[independent certification](qsp-certification.md), and
[QSVT encodings and transform builders](qsvt-model.md). These stages need no
native simulator. Continue with [prepared QSVT execution](qsvt-runtime.md) and
[applications](qsvt-applications.md) for QuEST resources, postselection, physical
solve scaling, file interchange and optional distributed runs.

The compiler pipeline keeps distinct trust boundaries:

1. Parse owned source snapshots into structured syntax.
2. Admit types, scope, calls, initialization, aliases, and resource use.
3. Construct SSA and independently verify its control flow, values, interfaces, and effects.
4. Optionally transform executable SSA and verify the result again.
5. Lower and plan, then prepare resources for native execution.
6. Run with explicit inputs and bounded interpreter resources.

Structured syntax remains the authority for canonical export even when executable SSA changes. A generated SSA listing is not an OpenQASM roundtrip format.

This is a bounded OpenQASM 3.1 simulator profile. Timed hardware execution, pulse calibration, recursive calls, arbitrary-width arithmetic, and every language extension are not implied by the `OPENQASM 3.1` header. Unsupported capabilities produce diagnostics. The next chapter states the boundaries of each public interface.
