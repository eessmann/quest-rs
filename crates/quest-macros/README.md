# quest-macros

Procedural `circuit!` frontend for `quest-circuit` and the `quest` facade. Use the
macro through either public crate, including when its dependency is renamed.
Expansion constructs a fallible, native-independent validated program and never
initializes QuEST.

The `quest-circuit` README documents the supported syntax and numerical
conventions. Token parsing uses `syn`; static
arity, identifier, index, angle and unsupported-feature errors point to compiler
source spans. Rust interpolations are evaluated once in source order and pass
through the same finite-value admission as ordinary builder calls.

Emitted operations retain the compiler's display filename and original keyword
byte range through `SourceSpan`. The nightly compiler's
`proc_macro_span` feature provides those offsets. Filenames honor path remapping;
no source files or stringified tokens are read during expansion. Locations do
not include a complete expansion stack.

Cross-crate macro/builder equivalence tests, compile-fail fixtures and hygiene
checks live in `crates/quest-circuit/tests`.
