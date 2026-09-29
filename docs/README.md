# Documentation

Start with the [user guide](book/src/index.md) for the supported language,
runtime, optimization and QSP/QSVT interfaces. Runnable examples and integration
tests supply the guide's code snippets.

| Topic | Entry point |
| --- | --- |
| Installation and native dependencies | [Build instructions](../README.md#build) |
| Grace Hopper with manual/Spack dependencies | [Grace Hopper setup](grace-hopper.md) |
| Public interfaces and examples | [Interface matrix](book/src/interfaces.md) |
| Register ownership and execution | [Runtime guide](book/src/runtime.md) |
| Structured OpenQASM programs | [Language guide](book/src/language.md) |
| Circuit transformations and certificates | [Optimization guide](book/src/optimization.md) |
| Polynomial preparation, QSP and QSVT | [Numerical guide](book/src/numerical-polynomials.md) |
| Phase conventions | [Phase semantics](phase-semantics.md) |
| OpenQASM sources and licensing | [OpenQASM provenance](openqasm-provenance.md) |
| Development and validation commands | [Contributing](../CONTRIBUTING.md) |
| Platform checks, migrations and measurements | [Verification index](verification/README.md) |
| Independent C++ numerical comparisons | [Reference benchmarks](../benchmarks/reference/README.md) |

The crate READMEs provide API-specific examples. Build the HTML guide with
`mdbook build docs/book` and Rust API documentation with `cargo doc`; see
[documentation validation](book/src/validation.md) for requirements.
