# Reproduction and evidence

The adjacent verification record describes scope, failures, counts and limits.
`evidence.tar.gz` contains command/exit receipts, logs, native consumer checks,
source identities, focused test reports, independent reviews, observer build
metadata, and every official trial. Preparation builds and the earlier smoke
run are stored under a separate `preparation/` prefix and are excluded from
performance summaries.

User-specific paths are normalized to `<home>`, `<repo>`, `<worktree>` and
`<artifacts>`. `evidence-manifest.json` retains both original-capture and archived
SHA-256 values. Source and binary identity receipts describe the original
builds; normalization of a displayed path does not rewrite those original
identity hashes. Native `/nix/store` paths retain their exact dependency
identities. Raw local captures remain under the original repository's
`target/qsp-optimization/` directory.

`snapshots/manifest.json` identifies baseline commit
`d19de2a0fd605b46bbc57e87fee36ba811a7d2b2` and three independent patches. Each
gzip-compressed patch applies directly to that baseline, not to the previous stage. The
patches contain only QSP/numerics changes needed for the staged comparison;
native setup and compiler-style portability repairs are outside that comparison.
Reconstruction was verified file-for-file, including an independent review.

For example, extract a fresh baseline with `git archive`, copy it separately
for A, A+B and A+B+C, and apply the corresponding patch with
`gzip -dc /path/to/a.patch.gz | git apply -` from that snapshot directory. The
manifest records compressed and uncompressed hashes. Use the
[fixture](../../fixtures/qsp-optimization/README.md) to build all four observers
with independent targets before running any trials. The build uses Cargo's
offline cache, populated by the workspace build, and records the resolved
standalone lockfile. Three interleaved trials produce 72 workload records.

The derived `summary.json` retains per-invocation median/min/max observations,
allocation and peak-storage counts, work charges, accepted grids, fingerprints
and input receipt hashes. `results.md` presents its median tables. Process RSS
is preserved in each raw `trial-*.stderr` receipt. Failed or incomplete campaigns
cannot produce a successful summary through the checked-in fixture.
