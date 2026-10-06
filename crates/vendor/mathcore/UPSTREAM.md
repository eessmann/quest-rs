# Maintained MathCore provenance

The official crates.io `mathcore` 0.3.1 archive is
<https://static.crates.io/crates/mathcore/mathcore-0.3.1.crate>. Its pinned
SHA-256 is `367d848a146b75a63af4fc2eafd6791327a662ccb4ff5965220fcfa7dfa1c96f`.
The original archive's `.cargo_vcs_info.json`, retained here, identifies
upstream commit `d13a333a6c69a231f210cdc3fb1c4843ee7bd2eb`.
The upstream MIT license and Nonanti copyright notice are preserved in
`LICENSE`; this metadata does not claim that the maintained source is the
unmodified upstream archive.

Quest-owned bounded exact affine algebra was introduced in the maintained
fork at quest-rs commit `7148ccaa47e9e39506d62ba1e441d657040b83f8`, formerly
`crates/vendor/mathcore/src/exact/mod.rs` and `tests/exact_affine.rs`. That
engine was extracted into `quest-symbolic` during the 2026-09-29 architecture
overhaul. The 2026-10-05 consolidation relocates the *current Dashu engine*
and its regression suite into this normal workspace dependency. It does not
replace current arithmetic with historical num-bigint arithmetic.

The maintained package is `quest-mathcore` version `0.3.1-quest.2`, exposed
under the dependency alias and library name `mathcore`. Its neutral backend
contracts and ordered static expression nodes are migrated from the current
`quest-numerics` and `quest-polynomial` interfaces. Bounded dynamic expressions
and sparse multivariate exact polynomials are quest-owned additions. Exact
algebra, source evaluation, and independent verification have separate roles:
`quest-symbolic` retains its independent source replay and binding checks.

The old approximate CAS, epsilon simplifier, parsers, evaluators, feature
layout, and legacy dependency graph are not restored. All current MathCore
modules are available without optional feature flags. Numerical implementations
and interval-bearing errors remain owned by `quest-numerics`; no native QuEST
code participates in this algebra layer.

The2026-10-06 neutral affine-simplex predicates are quest-owned additions using
the existing Dashu integer backend. Binary64 import and geometry share a finite
dyadic decoder while retaining separate admission policies. This does not
restore a symbolic geometry CAS or change upstream archive provenance; the
exact predicates, their independent Cramer oracle and resource contract are
described in [GEOMETRY.md](GEOMETRY.md).
