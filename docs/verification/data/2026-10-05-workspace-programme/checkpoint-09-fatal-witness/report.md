# Matching fatal-boundary durable witness correction

Implemented only `crates/quest/src/qsvt/matching/collective/failure_tests.rs`. The prior checkpoint 08 failure is retained; missing launcher-forwarded abort text was its sole assertion failure, with prompt unsuccessful/non-timeout subprocess termination. This corrects the test oracle, not the production fatal boundary.

The parent creates a unique private directory (mode 0700 on Unix). Fixed files are exclusively created, written and synced: both completed native H calls, the peer receive-stage witness, and the actual expected checked native partition-range error. The two ranks agree after their native/peer witnesses. Rank 0 verifies the exact Validation error before persisting its witness and passing that same error to the unchanged fatal function. Rank 1 enters the unmatched receive. No response is introduced. Parent requires exact witness contents and absence of either after-fatal-return marker.

Each of two MPI2 jobs has a 20-second deadline and 5-second forced-cleanup grace; successful exits and timeout exits 124/137 are rejected. The second job redirects stderr to null, proving the oracle works without launcher diagnostic delivery. Parent prints exact observed status. Both final jobs exited 1, and the single parent test passed in 1.02 s (cargo invocation 2.14 s). Strict quest-rs lib+tests Clippy passed in 0.74 s; scoped rustfmt and whitespace checks passed. Existing upstream quest-polynomial next-solver warning is retained in logs.

The peer-ready witness establishes arrival at the receive stage; it does not measure the instant the peer is already blocked inside MPI_Recv. Failure to persist a required witness fails fixture acceptance. Directory cleanup is best effort after parent checks. Test filesystem payload is fixed and tiny; no new Cargo dependency, sleeps, fflush, production modification, broad suite rerun, or scientific campaign was introduced.

## Evidence

- `source.json`: exact final one-file hash and historical checkpoint source delta. All other 974 historical source files matched during final capture.
- `focused.json`: final checks, exact child statuses, all original/final log hashes and retained original checkpoint counts.
- `historical-red.log`: original missing-diagnostic failure excerpt; original full log remains immutable at its existing path.
- `final-green.log`: final bounded pair, stderr captured and suppressed.
- `final-clippy.log`: strict cfg(test)-included final Clippy.

Initial private rustfmt invocation used nonexistent rustfmt.toml and failed before formatting; corrected .rustfmt.toml invocation passed. Initial witness jobs used the inherited 20-second timeout; root review added explicit 5-second forced cleanup and exit137 rejection before the final pair. Those earlier successful logs remain historical, distinct from the final source.
