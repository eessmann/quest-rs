# Additive DG2 energy-shell diagnostic

This is a separately versioned **one-row protocol**, following the immutable
[seven-row initial diagnostic campaign](../../../docs/verification/2026-10-06-configuration-weak-campaign.md).
It does not extend that campaign's default row list or replace any rejection. The
new physical row has not been executed by the implementation's unit/schema tests.
Execution requires its own reviewed, stable build pin and retained outcome.

The new example `configuration_weak_energy_shell_v2` accepts only
`p2-c2-e1-w1_2`: configuration DG2, two cells per axis, extent 1, width 0.5,
center `[0.15,-0.1,0.07,0.11,-0.04]`, viscosity 0.01 and time zero. It retains all
five original physical coordinates and all 7,776 configuration coefficients.
The original `configuration_weak` entry continues accepting exactly its original
seven IDs and rejects this new ID. Both entry points use one shared producer,
serializer, types and complete admission ledger in
`examples/support/configuration_weak_protocol.rs`.

## Why this point is informative

The previous DG1/three-cell grid has only ±1/3 as strict interior nodal locations.
Every occupied ideal five-coordinate state therefore has integrated kinetic energy
`E=0.5*5/9=5/18`, independently of the bump's probabilities. For any matrix L,
complex state z, probability q and normalized diagonal-observable expectation mu,

```text
mu_dot = 2 Re sum_i conj(z_i) (E_i - mu) (Lz)_i / q.
```

If E is constant wherever z is nonzero, this derivative is zero even when L is
not skew and qdot is nonzero. Periodic central skew-adjointness additionally makes
the raw norm rate vanish, but is not the cause needed for this normalized-shell
identity. Actual floating nodes and the approximate mass chart introduce tiny
rounding differences; this is an ideal algebraic explanation, not a bit-exact
runtime zero certificate. The physical viscous energy rate can remain nonzero.
Changing width while retaining that same interior energy shell cannot cure it.

The maintained regression uses an independent non-skew complex 3-by-3 matrix,
`z=(1+i,2-i,0)` and `Lz=(3-2i,1+4i,1+8i)`, giving q=7 and qdot=-2. The actual
normalization accumulator returns zero for observable `[5/18,5/18,7]`, despite
nonzero raw rate `-5/63`. Observable `[1,3,7]` instead gives `-36/49` by the direct
quotient derivative. This checks a nonconstant counterexample and a nonzero action
at an initially unoccupied coefficient without any physical-flow simulation.

DG2/two-cell interior locations are `[-1/2,0,0,+1/2]`. Each specified bump factor
occupies 0 and one side, giving two distinct support points and three coefficient
entries. Predicted support is 243 coefficients, including duplicates, with
nonconstant energy on support. The unchanged minimum-two policy is still necessary
and still insufficient to establish probability or generator resolution.

## Unchanged caps and shared evidence

The [existing producer ledger](configuration-weak-experiment.md) is reused without
a phase-local allowance. Predictions from the current source model are:

```text
N = 6^5 = 7776
input work = 32997376
source work = 319676832
original physical callbacks = 545
```

These are predictions, not hardcoded admission outcomes or accuracy targets. The
actual source and original drift receipts decide admission. The same 256 MiB
managed, one-billion source-work, 100-billion physical-work and one-million-call
caps apply. The original bump, full-state validation, exact-zero-only row omission,
actual capacities, fixed 64 KiB serializer and declared overlap remain unchanged.
The additional static protocol selector fits the existing fixed helper envelope.
No complete drift table, generator or history is constructed.

The row keeps the shared `quest-configuration-weak-row-v1` wire format; its explicit
ID selects the new request. The new runner's outer receipt is
`quest-configuration-weak-energy-shell-v2`. It calls the shared strict validator,
collector and pair computation with explicit request arguments, without modifying
shared global `ROWS` or monkey-patching parameters. Typed failures retain their
phase/progress and never become successful rate comparisons.

Only the immutable published DG1/four-cell row is allowed as reference; its raw
SHA-256 is checked before launching a child and again afterward. Although its
distinct physical points match this new grid, its aggregate interior weights are
`[1/2,1/2,1/2]`, versus `[2/3,1/3,2/3]` here. Thus this is a **quadrature and stencil
sensitivity** comparison, not pure h or p refinement, and the sampled ensemble
changes. Compare the physical sampled rate separately from the weak-generator
rate using the existing `Delta G`, `Delta P`, `Delta E` decomposition. Mean-minus-
center biases, concentration, qdot and physical-reference resolution remain visible.
A nonzero energy rate would remove the old algebraic obstruction; it would not
establish accuracy, continuum/history convergence or quantum execution.

## Focused verification and later single execution

```sh
cargo test -p quest-cfd --lib configuration_weak
cargo test -p quest-cfd --example configuration_weak \
  --example configuration_weak_energy_shell_v2
python3 -B docs/verification/fixtures/quest-cfd/test_configuration_weak.py
python3 -B docs/verification/fixtures/quest-cfd/test_configuration_weak_energy_shell_v2.py
```

The focused schema fixture injects a Rust `cfg(test)` failure before grid allocation
and runs the real shared error/serializer path. It does not construct a physical
source or execute this diagnostic. Fake subprocess tests verify one failed child
is retained without retry. Read-only tests revalidate the previously published
seven rows and reproduce their comparison receipts exactly after helper extraction.

After independent review and a fresh default compiler-artifact/source attestation,
the single execution command is:

```sh
python3 -B docs/verification/fixtures/quest-cfd/configuration_weak_energy_shell_v2.py \
  /path/to/pinned/configuration_weak_energy_shell_v2 /path/to/new-output
```

Exactly one child is attempted, with unchanged 512 MiB address-space, 180-second
and 4 MiB captured-file limits. The runner retains raw stdout/stderr/timing, hashes,
actual available RSS, typed rejection or malformed output, and atomic receipts.
No retry, replacement point, width/cap change or data-fitted accuracy tolerance is
permitted. Build and runtime closure provenance remains a separate required step;
the runner alone does not certify a source-to-binary build. Root owns any later
public data release.
