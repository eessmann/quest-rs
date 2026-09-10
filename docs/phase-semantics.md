# Phase semantics

The semantic pin is [OpenQASM 3.1, Gates, built-in U](https://openqasm.com/versions/3.1/language/gates.html#built-in-gates), checked on 2026-09-10. The specification defines

```text
U(theta, phi, lambda) = exp(i theta/2) *
  [[cos(theta/2),             -exp(i lambda) sin(theta/2)],
   [exp(i phi) sin(theta/2),   exp(i phi) exp(i lambda) cos(theta/2)]]
```

This matrix is 2π-periodic in theta. Its adjoint is `U(-theta, -lambda, -phi)`. The phase is retained in numerical matrices, native execution, and fusion. Adding positive or negative controls restricts the entire matrix, including this phase, to the selected control subspace. Separate exponentials avoid overflowing finite `phi + lambda` before trigonometric evaluation.

The native decomposition executes `Phase(lambda)`, `Ry(theta)`, `Phase(phi)`, then a global phase `theta/2` with the same controls. No approximate equivalence modulo global phase is used.

## Independent regression fixtures

- `matrix_contract::openqasm31_u_has_specification_phase_and_two_pi_periodicity`: fixed complex entries `(1+i)/2` and `-(1+i)/2` for `U(pi/2,0,pi)`, repeated at theta plus 2π.
- `matrix_contract::openqasm31_u_adjoint_keeps_the_conjugated_specification_phase`: conjugated fixed entries for the symbolic adjoint.
- `runtime::openqasm31_u_phase_survives_native_adjoint_signed_controls_and_fusion`: superposed control branches, nonconsecutive controls, positive and negative polarity, direct and fused execution, forward and adjoint U. Applying H after U yields an independently specified phase on each active branch.
- `matrix_contract::registry_u_decomposition_reconstructs_the_full_specification_matrix`: evaluates the complete shared-registry U decomposition at theta = phi = lambda = pi/2 and compares literal entries from the exponential definition.
- The existing finite-large-phase fixture retains phases near `1e308` and checks both a matrix entry and unitarity residual.

The phase and adjoint fixtures failed before the implementation correction. They compare complex amplitudes directly, without aligning global phases. See the task validation report for the commands and results of the current run.
