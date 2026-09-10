# quest-qasm

Native-independent `OpenQASM` 3.1 simulator-profile text import and canonical export.
The frontend uses `quest-language` syntax, gate definitions, typed admission and SSA
verification. It performs no filesystem reads and does not execute quantum code.

```rust
use quest_language::{SourceId, SourceSnapshot, semantic::CompileLimits};
use quest_qasm::{ExportLimits, ImportLimits, StandardLibrary, export, import};

let source = SourceSnapshot::new(
    SourceId::new(1), "bell.qasm",
    "OPENQASM 3.1; include \"stdgates.inc\"; qubit[2] q; h q[0]; cx q[0], q[1];",
);
let mut includes = StandardLibrary::new(SourceId::new(2));
let module = import(source, &mut includes, ImportLimits::default(), CompileLimits::default())?;
let canonical = export(&module, ExportLimits::default())?;
assert!(canonical.contains("include \"stdgates.inc\";"));
let verified = module.into_typed().into_ssa()?;
# Ok::<(), Box<dyn std::error::Error>>(())
```

`parse` resolves source snapshots and retains syntax without granting semantic
admission. `ParsedModule::admit` checks the expanded module. `import` combines
these stages. `admit_expanded` shares pinned-library validation with other frontends
that already have a source map and expanded syntax. `ImportedModule` exposes immutable syntax, admitted structure,
include edges and owned source snapshots; construction is private.

Implement `IncludeResolver` to supply immutable snapshots explicitly. A source ID
must identify exactly one name and text. Active IDs or source names detect include
cycles. Include count, depth, cumulative expanded source bytes, parser tokens and
parser nesting have explicit limits. Repeated inclusion counts toward expansion
budgets even when the source snapshot is shared. Includes are accepted only at
global scope. `NoIncludes` rejects every request; `StandardLibrary` supplies only
the pinned `stdgates.inc` and needs an ID distinct from the caller's sources.

`export` serializes the retained root with its include directives. Reimport must
use the same included contents, available through `sources()` and `includes()`.
`export_typed` serializes expanded admitted structure, retaining user definitions
while registry standard gates remain implicit. `export_syntax` accepts unadmitted
syntax and checks text representability, without proving types or executability.

Canonical output preserves structured definitions, aliases, declarations, types,
input/output qualifiers, arrays and references, modifiers, expressions, branches,
switches, loops, calls, measurement, reset and barriers. Comments and whitespace
are canonicalized. Captures, invalid lexical leaves, non-scalar casts and AST
shapes without equivalent parser syntax return owned export diagnostics. Bare zero-operand custom gate calls are represented as callable expressions;
explicitly modified zero-operand calls retain their gate-statement representation. Timing, calibration and other constructs outside the language
simulator profile are rejected by the shared frontend.

Diagnostics own their source maps and include traces. The optional
`codespan-reporting` feature enables the shared renderer without reading files or
installing global hooks. The optional `serde` feature propagates serialization of
the owned shared diagnostic model. See [`OpenQASM` provenance](../../docs/openqasm-provenance.md)
for the bundled source, license, pinned hashes and explicit phase corrections.
