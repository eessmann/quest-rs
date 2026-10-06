# Durable MPI fatal-boundary witness evidence

The [one-file source manifest](source.json) binds a test-only correction to the checked native failure subprocess oracle. The [owner report](report.md), [focused receipt](focused.json), [final MPI log](final-green.log) and [strict test Clippy log](final-clippy.log) retain the focused evidence. The [read-only diagnosis](diagnosis.md) and [original failure excerpt](historical-red.log) explain why mandatory launcher-delivered abort prose was removed. The complete original checkpoint 08 gates, including unrelated CMake failures, belong to their separately published checkpoint records.

Two bounded MPI2 jobs require durable native-entered, peer-ready and actual checked-error witnesses, unsuccessful exits other than timeout 124/137, and absent return witnesses. The second job suppresses stderr. Both final jobs exited 1. Each job has a 20-second deadline plus 5-second forced-cleanup grace. Production fatal/native code and QuEST were unchanged. Peer readiness is arrival at the receive stage, not a measured instant of blocking inside MPI_Recv.

Existing log_sha256/log_bytes in the focused receipt bind original logs. Added published hashes and the [provenance table](provenance.json) bind normalized copies. Initial successful owner checks are retained by original hash only; their redundant payloads are not copied. No original numerical/status fields were changed. Source JSON is byte-identical.

These are private publication candidates. No tests, builds or scientific jobs ran during staging; no repository source/publication files were written. Checkpoint 09 broad results are pending and are not inferred from these focused checks.
