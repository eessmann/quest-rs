# Coherent observation between temporal DG nodes

The global history stores configuration amplitudes in consecutive temporal
nodal blocks. It does not store probabilities at physical times. On an explicitly
selected slab, let `w_a(t)` be its DG Lagrange weights and let `m` be the complete
lifted configuration dimension. The physical amplitude is

\[
z_i(t)=\sum_a w_a(t)z_{(s q+a)m+i},\qquad
\rho_i(t)=|z_i(t)|^2.
\]

The cross terms in the second expression are essential. A weighted average of
the nodal probabilities is a different observable. The nodal history coefficients
are configuration-mass-weighted amplitudes; no extra temporal quadrature square
root belongs in this interpolation. Quadrature weights are used when computing a
time integral, which is a separate operation. At a shared slab boundary, choosing
the earlier right endpoint or later left endpoint retains the two distinct DG
traces.

`TemporalInterpolation` in [temporal_observation.rs](../src/temporal_observation.rs)
provides admitted DG1/DG2 weights, logical column indices, a scalar reference
evaluation and an outward bound on their Euclidean norm. Its per-coordinate
reference query reads two or three complex amplitudes, respecting declared
callback work and storage. This scalar query alone is not a coherent circuit.

## Rectangular coherent map

[TemporalEncoding](../src/temporal_encoding.rs) implements the rectangular map

\[
C_{i,(s q+a)m+i}=w_a(t),\qquad CC^\dagger=\|w\|_2^2 I_m.
\]

The latter identity follows from disjoint row supports. Its operator norm is
`||w||_2`. The stored binary64 weights have their own rounding and consistency
obligations; the algebraic block-encoding identity does not prove temporal
convergence or floating execution accuracy.

Each temporal node supplies one partial matching. If its block begins at `b`,
the matching sends `b+i` to `i`, for every retained configuration index. For
`b>0`, completion adds `i` to `b+i`, yielding disjoint transpositions. For `b=0`,
the matching is an identity on that interval. These rules give bounded arithmetic
forward and reverse lookups without storing a matrix or permutation table.

The baseline retains `K=next_power_of_two(q)` labels and
`beta=max_a |w_a|`, with normalization `alpha=K*beta`. Negative DG2 weights carry
their phase. Exactly zero weights and padded labels contribute zero success
blocks through the canonical failure rotation; padding and unsuccessful flag
sectors remain part of the complete unitary. Rotation and phase parameters are
prepared once, then reused numerically. The source fingerprint binds the
represented matrix; the construction fingerprint also distinguishes the complete
unitary. Neither 64-bit fingerprint is a security proof.

For a normalized input entirely in the right logical subspace, successful output
amplitudes are `Cz/alpha`, with probability `||Cz||²/alpha²`. The parent inverse's
physical rescaling must be applied exactly once, and interpolation contributes
the additional factor `alpha` when recovering physical amplitudes from this
successful block. A successful field or diagonal observable must retain both
inverse and interpolation selection probabilities.

An inverse output generally also has failure amplitudes. Applying the temporal
unitary while reusing uncleared inverse workspace can mix those amplitudes into
the answer. A composed algorithm must either use separate clean workspace with
the appropriate inverse-success controls, or first measure/postselect inverse
success before reusing its cleaned workspace. Rejected shots require repeating
their charged preparation and solve. The
[composed CPU/MPI observation](distributed-temporal-observation.md) supplies the
second route using unnormalized simulator projectors and this complete unitary.
It reports the explicit temporal slab and left/interior/right trace side,
original-normalized joint success, and the physical amplitude scale. Neither
API executes sampled measurements or recovers an amplitude's sign/phase from its
squared magnitude.

## Storage and circuit cost

Retained source data are fixed-size arrays for at most three nodes plus the
shared owning replay descriptor. Construction still visits every touched record
to compute integrity metadata and admit the complete unitary. Its conservative
work ceiling is checked before those dimension-dependent loops. The shared replay
admission validates completion and counts forward/adjoint streams with bounded
workspace. It retains no circuit stream or complete configuration catalogue.

Portable execution emits controlled rotations, phases and transpositions for
every required index. The count grows with configuration dimension and bit width;
this is not a constant-cost coherent lookup. `replay_gates` counts controlled
portable primitives. Arbitrary multi-controls are not a one/two-qubit or
Clifford+T decomposition. Fault-tolerant synthesis and its precision cost remain
separate. Directory communication is zero; this does **not** mean native MPI
statevector gates communicate zero bytes. Native execution and application
transport have separate admission and accounting.

Given an already admitted `TemporalHistoryRecipe`, construction is:

```rust,ignore
use quest_cfd::temporal_observation::{TemporalInterpolation, TemporalInterpolationLimits};
use quest_cfd::temporal_encoding::{TemporalEncoding, TemporalEncodingLimits};
use quest_qsvt::ReplayEncoding;

let interpolation = TemporalInterpolation::new(
    &history, 0, 0.25, TemporalInterpolationLimits::default(),
)?;
let temporal = TemporalEncoding::new(&interpolation, TemporalEncodingLimits::default())?;
let descriptor = temporal.descriptor()?;
// Supply validated operand mapping, controls and a separately admitted executor.
temporal.visit_mapped_replay(&targets, control_mask, control_value, false, &mut visitor)?;
```

## Executed validation

The bounded DG2 test uses three configuration coordinates, two temporal slabs
and fraction `1/4` on the second slab. Independent weights are
`[3/8, 3/4, -1/8]`. Its arithmetic resource matches the **whole unitary** of an
independently stored matching construction, as well as the extracted `C/alpha`
block. A complex-amplitude example explicitly distinguishes interference from a
probability mixture. Other tests cover zero weights, dummy labels, constant
retained storage, rejected work/byte/gate budgets and early visitor failure.

Native CPU and local MPI 1/2/4/8 ranks plus two split two-rank communicators
compare arbitrary whole-register states with the bounded reference, including
both signs of an outer control, a spectator, reversed operand mapping, failure
flags, padding and adjoints. MPI initialization supplies only local amplitude
shards. The reference alone materializes a small unitary. All five tests passed
in 10.55 seconds on the matching QuEST/MPICH system lane; an independent reviewer
also reran the four non-MPI tests. No OS memory-cap or multi-host test is implied.

```sh
export QUEST_ROOT=/path/to/installed/quest
export MPICC=/path/to/matching/mpi/bin/mpicc
export PATH=/path/to/matching/mpi/bin:$PATH
cargo test -p quest-cfd --features distributed --test temporal_encoding -- --nocapture --test-threads=1
cargo test -p quest-cfd --test temporal_observation
```

These are executed interpolation-circuit checks. A complete inverse/interpolation
measurement campaign, its total conditional error and its physical convergence
remain separate acceptance requirements.
