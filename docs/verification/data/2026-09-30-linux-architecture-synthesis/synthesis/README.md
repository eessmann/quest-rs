# Final grid-pruning acceptance

The Linux process suite exposed a 30-second timeout for fresh full-phase
Rz(pi/4) synthesis at epsilon 1e-12, seed 1234, in both debug and release builds.
Exact intersection pruning now rejects infeasible weighted-sphere branches
using the necessary coefficient-norm ball and an enclosure-derived radial
upper bound. Neither target, epsilon nor process limit was relaxed.

The [independent review](grid-pruning-review.md) records the mathematical and
resource reasoning at the hashes in [source-overlay.sha256](source-overlay.sha256).
Original Linux failure and final Linux process receipts are recorded by the
parent validation receipt; these files below are local aarch64 macOS validation.

Commands, run from the shared worktree through its project development shell:

```sh
devenv shell -- cargo test -p quest-synthesis -p quest-math --message-format short
devenv shell -- cargo clippy -p quest-synthesis --all-targets --message-format short
devenv shell -- cargo test -p quest-synthesis --test resources live_grid_storage_is_unavailable_to_candidate_callbacks -- --exact
```

[Full acceptance](macos-full-acceptance.log) exited zero: 62 tests and one
doctest passed. It includes exhaustive feasible-candidate set comparisons for
seven signed/cardinal/quadrant targets and exponents 0–2, fresh quarter-pi
synthesis at 1e-12, full-phase corruption rejection, sequence/work determinism,
tight work exhaustion, and callback byte-reservation/restoration checks.
[All-target Clippy](macos-clippy.log) exited zero.

The live-grid reservation regression was observed
[failing before the reservation](grid-reservation-red.log), then
[passing after it](grid-reservation-green.log). The complete acceptance above
was run after the reservation and its restoration helper were implemented.
The grid allowance is a conservative modeled category reservation; these
receipts do not claim a universal aggregate allocator or heap-peak bound.

The final [explicit warning-denying Clippy run](macos-explicit-strict.log)
also exited zero with `--all-targets --locked -- -D warnings`; its exact
command and exit file are retained alongside the earlier configured-lint run.
