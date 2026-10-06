# Persisted weighted readout tag mismatch

Read-only diagnosis during campaign 3. No source changes, builds, native jobs, debugger attachments or running-job instrumentation were performed. The companion JSON pins the inspected files and exact pair/tag table.

**Confirmed source defect:** `execution.rs:150–154` passes `rank ^ (parts/2)` as both peer and the third argument to `MpiCollectiveLane::send_receive_bytes`. That third argument is a message **tag**, not a source rank. `quest-sys/src/mpi.rs:518–529` forwards the peer and tag for both send and receive.

For a pair `(r, p)` with `p=r XOR (P/2)`, rank r sends/expects tag p, while peer p sends/expects tag r. Since r and p differ at every supported P>1, neither receive can match the other's message. P2 gives tags1 versus0; P4 pairs0/2 and1/3 mismatch likewise; P8 has the same property. If execution reaches the first readout exchange, this protocol cannot complete normally. P1 bypasses the exchange entirely.

This is consistent with the reported P1 success and P2/P4 deadlines, but **the timed-out jobs have no stage trace**, so the diagnosis does not establish that those particular jobs reached this line. Timeout artifacts must retain their actual deadline classification, not be rewritten as a measured deadlock-location claim. The source-level defect and observed job-stage uncertainty are separate evidence.

The preceding `read_local_amplitudes` is collective (`collective.rs:427` enters environment operation42), despite reading only the local partition. Here all ranks have the same 1024/P local length, chunk count and `parts` branch, and call it in the same order. Its enclosing consumer coordination uses a separate duplicated readout communicator. No additional divergence at that site was established by this read-through. The exact tag mismatch is sufficient to require correction without speculating about transform performance or MPI deployment.

## Maintained-test coverage gap

`tests/persisted_weighted_transform.rs` imports the production `execution` module but calls only `execution::readout_floor` for admission failure checks. Its cold differential directly reads local amplitudes and compares against a bounded portable whole-unitary reference. There is no actual `execution::readout` invocation in the maintained test; that function is called only by production replay. Thus the existing transform/transport tests did not execute the faulty readout pairing protocol.

After campaign closure and explicit source release, the narrow fix is a common fixed tag on the dedicated readout communicator, preserving the XOR peer mapping and same chunk schedule. A maintained real readout test should use a bounded1024-amplitude distributed fixture with all32 success coordinates and nonzero failure-sector mass, check analytic paired residual and normalization, and exercise1/2/4/8 plus split communicators under an external deadline. It needs no producer rebuild, phase synthesis or inverse circuit to expose the bug. The original tag must yield a bounded timeout RED; the corrected shared tag must complete with the independently expected observables. No execution of this proposed test has occurred in this diagnosis.

## Saved-only campaign audit after closure

Audit the immutable terminal receipt and all raw stdout/stderr/timing hashes, build/source/runner identities before/after, every seven-job outcome and retained partial artifacts. Validate completed P1 outputs against the exact native accuracy predicate, original fixed caps and matching imported phases. Preserve each multi-rank deadline and unknown in-job stage. Recompute source/phase equality only from saved bytes. Any later readout correction, focused test and new campaign must have separate source/binary identities and cannot retroactively convert timeout rows to successes.
