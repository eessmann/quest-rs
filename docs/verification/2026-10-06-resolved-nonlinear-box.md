# Fixed-budget nonlinear generated-box inverse attempt

The attempted T=0.1 nonlinear inverse rejected during projector-phase
certification because the declared 64 MiB verification envelope was insufficient.
No native inverse replay occurred. The two-rank trial was not started after the
one-rank rejection. This is capacity evidence; it establishes neither a resolved
nor an unresolved nonlinear inverse signal.

The [rejected receipt](data/2026-10-06-resolved-nonlinear-box/rejected.json),
[exact build-source manifest](data/2026-10-06-resolved-nonlinear-box/source.json)
and [read-only admission diagnostic](data/2026-10-06-resolved-nonlinear-box/certification-admission.json)
are separate artifacts. The [earlier short-window experiment](2026-10-05-generated-box-kvn-history.md)
remains historical evidence with its original source identities and accuracy
limitations. No earlier receipt was replaced.

## Constructed experiment and acceptance

The new ignored [fixture](../../crates/quest-cfd/src/box_kvn_history_tests.rs)
retains the complete five-coordinate periodic BDM1 chart at extent1.7 and zero
viscosity. The configuration grid has one DG2 cell on each [-0.3,0.3] axis,
243 nodes and an off-center width0.55 bump. One temporal DG1 slab produces486
history coefficients. T changes from0.001 to0.1, reciprocal tolerance from0.03
to1e-6, with an explicit degree cap1023 and600-second child deadline. The phase
certificate remains mandatory; no phase-off or larger-budget retry was performed.

The independent bounded reference uses the same native physical chart and
complete drift, including all volume/facet contributions. It isolates quadratic
convection by polarization, checks its equality with the zero-viscosity generator,
and compares every owned generated original-H row with the independent history.
A second actual construction must reproduce the checked local spool digest before
its H-adjoint matching producer is used. This comparison repeats the collective
source construction; its cost is separate from inverse replay. No independently
advancing local reader calls MPI.

The planned acceptance compares the nonlinear reference with the exact
zero-generator DG1 history x0=(z0,z0). With common denominator ||x0||,
S is nonlinear reference change, E is recovered inverse error and Q is observed
change. It requires S>=4e-4, E<=2e-5, S>=10(E+reference allowance),
Q>=S-E-reference allowance, inverse residual<1e-5 and an all-sector
forward/adjoint roundtrip. The allowance combines an outward complex reference
residual with the generated/reference matrix difference and a positive perturbed
singular lower bound. These checks did not reach native readout in this attempt.

Two pure regressions pass: the historical error-dominated response fails the
new acceptance, and known complex norm/reference-residual checks exercise the
outward helpers. Strict distributed library/test Clippy passes. The actual
one-rank attempt rejected after28.13seconds; wrapper time was28.41seconds.

## Why the declared certificate envelope rejected

A bounded read-only debugger diagnostic used the same immutable failed-trial
binary and unchanged limits. The constructed polynomial degree is449 with450
phases. Initial verification has450 coefficients, FFT support1024 and a
39,762,944-byte context at256 bits. Actual projector verification expands to899
coefficients, FFT support2048 and an85,436,416-byte context. Its retained
source/conversion payload is61,200bytes, leaving only67,047,664 of the original
67,108,864-byte cap for that context.

The implemented admission formula is
`64*(count*(ilog2(fft)+1)+fft)*(8*ceil((precision+1)/64)+64)`,
where `fft=next_power_of_two(2*count-1)`. For899 coefficients, context plus
retained conversion needs85,497,616bytes at256 bits,111,785,744 at512 bits and
164,362,000 at1024 bits. The latter two are calculated envelopes, not executed
precision retries. Context allocation, root caches and backend adaptive scratch
remain subject to further admission/runtime behavior; meeting this initial
formula does not guarantee a certificate.

The diagnostic ran construction again and executed no inverse. An earlier
read-only probe could not access local-variable names at the binary's debug
level; verified x86_64 argument/stack offsets then supplied the recorded
counters. These diagnostics are separate from the primary failed timing.

## Storage and scientific limits

The attempted managed limits were64 MiB native/rank,512 MiB source/history/rank,
1 GiB source/history/node for at most two local ranks,64 MiB synthesis and64 MiB
per certificate owner. No OS address-space or RSS cap was imposed. Reference
construction was independently bounded at512 coordinates,128 MiB and3 billion
modeled operations. These component models do not measure process RSS or an
end-to-end physical builder peak.

The history backend conservatively charges the complete native environment cap,
retained recipe/RHS provenance, matching shard and reciprocal polynomial, plus
synthesis and two simultaneous certificate-owner caps. A larger certificate cap
therefore also requires an independently reviewed history rank/node envelope.
The diagnostic artifact contains a proposed256 MiB certificate budget with
1 GiB managed history/rank and2 GiB/node, and a proposed3 GiB OS address-space
limit/rank. Those values have not been implemented or executed by this stage.

The bump touches configuration boundaries. Fixed chart-coordinate bumps and
tensor grids are not invariant under partition-dependent QR rotations, so each
rank count requires its own aligned reference. No boundary resolution,
configuration/physical convergence, certified total quantum error, sampling
probability lower bound, multihost behavior or quantum advantage is claimed.

To reproduce the unchanged fixed-budget attempt with installed matching MPI:

```sh
export QUEST_ROOT=/path/to/installed/quest
export MPICC=/path/to/matching/mpicc
export PATH=/path/to/matching/mpi/bin:$PATH
cargo test -p quest-cfd --features distributed --lib \
  box_kvn_history::tests::resolved_nonlinear_generated_history_inverse \
  -- --ignored --nocapture --test-threads=1
```

The command intentionally retains its admission failure on the published tree;
it does not silently enlarge limits or start the second rank count after failure.
