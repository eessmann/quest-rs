# OpenQASM 3.1 source and standard-library provenance

The language target is [OpenQASM 3.1](https://openqasm.com/versions/3.1/).
The text frontend shares the project-owned `quest-language` grammar, gate registry
and typed admission. It is a bounded simulator profile, not a claim to implement
timing, calibration, pulse or every feature of the full specification.

## Pinned upstream input

The official `spec/v3.1.0` release resolves to commit
`c717508162a0eac892fa32134716fe77a284e835`. Its
[`examples/stdgates.inc`](https://github.com/openqasm/openqasm/blob/c717508162a0eac892fa32134716fe77a284e835/examples/stdgates.inc)
is retained verbatim at
`crates/quest-qasm/src/stdlib/stdgates.upstream.inc`. Its original header says
“OpenQASM 3.0”; the provenance is the 3.1 release commit, not that header.

| Artifact | SHA-256 |
|---|---|
| `stdgates.upstream.inc` | `b2b60afcbc0c2195bd3cb3ec0347c4c9f7447fd22efb28b36b5ab7ee721c11c0` |
| corrected `stdgates.inc` | `c27f21a4c72cfe36dfceee47d9812041c9cbaf309568173507b581bf7d74b534` |

The upstream [Apache 2.0 license](https://github.com/openqasm/openqasm/blob/c717508162a0eac892fa32134716fe77a284e835/LICENSE)
is retained alongside both files as `stdlib/LICENSE`. Tests pin both hashes and
assert that the corrected source differs only by its explanatory header and the
two substitutions below. No network or filesystem lookup happens during import.

## Explicit phase corrections

OpenQASM 3.1 defines `U(θ,φ,λ)` with an overall `exp(iθ/2)` multiplying the
conventional Euler-angle matrix. This phase becomes observable under control.
The [gate specification](https://openqasm.com/versions/3.1/language/gates.html)
is the phase authority.

The [standard-library specification](https://openqasm.com/versions/3.1/language/standard_library.html)
defines `cu` with active-control block `exp(iγ) U(θ,φ,λ)` and `CX` as an alias of
`cx`. Two definitions in the pinned example source disagree with those meanings:

| Gate | Pinned example | Bundled correction |
|---|---|---|
| `cu` | `p(γ-θ/2) a; ctrl @ U(θ,φ,λ) a,b;` | `p(γ) a; ctrl @ U(θ,φ,λ) a,b;` |
| `CX` | `ctrl @ U(π,0,π) a,b;` | `cx a,b;` |

The first example removes the 3.1 `U` phase from the active block. The second
produces `iX` on that block instead of `X`. These differences are relative phases,
so they cannot be discarded as a whole-program global phase. The original input
is preserved for audit; the corrections are explicit project decisions following
the displayed mathematical definitions. The upstream `cu` discrepancy is also
tracked in [OpenQASM issue 682](https://github.com/openqasm/openqasm/issues/682).

Compatibility gates `u2` and `u3` retain the upstream phase factors, including
`gphase(-(φ+λ+θ)/2)` for `u3`; they are not silently renamed or assigned another
vendor's convention. All remaining upstream bodies are unchanged.

## Admission and export authority

Only the exact corrected library bytes activate the registry prelude. Matching
registry gate declarations are checked against the complete reparsed pinned AST
and for parameter and operand counts and then
elided from expanded admission, avoiding duplicate builtins. Non-registry standard
names remain explicit user-style gate definitions. An altered library does not
receive this privilege and its conflicting builtin definitions fail admission.
The shared registry owns intrinsic gate semantics; its phase tests and the native
runtime tests establish those semantics independently of the text serializer.

`ImportedModule` retains the original root AST, immutable snapshots and include
edges alongside admitted expanded structure. Canonical root export keeps the
include directive; equivalent reimport requires the retained include contents.
The exporter does not substitute ambient files. Expanded typed export retains
non-registry definitions and relies on the same implicit registry. Tests compare
independently normalized complete ASTs after import/export/import, preserving all
semantic fields while removing only source identities and spans. They additionally
pin the corrected `cu` and `CX` definition ASTs and reject a modified builtin body.
