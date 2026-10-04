# Physical DG foundation and acceptance boundaries

For the full derivation through configuration and time DG, see
[method and theory](docs/method-and-theory.md). The
[next-step plan](NEXT_STEPS.md) separates pending implementations, convergence
studies and benchmark acceptance from the existing foundation described here.

`PeriodicBdm1` and `SimplexBdm` evolve complete incompressible BDM1/P0 DG states. They do not select energetic modes, use a POD basis, freeze the convective operator, or replace Navier–Stokes with Burgers dynamics. The mass chart eliminates algebraic constraints only.

## Discrete equations

Each triangle has six affine velocity coefficients; each tetrahedron has twelve. The pressure is constant on each cell. A normal trace is affine on a facet, so equality at its two/three vertices enforces the complete normal-continuity constraint. Prescribed boundary normal traces and integrated cell divergence enter the same constraint matrix. Open boundaries retain their free normal degrees.

For a stationary lifting `l`, the implementation computes the entire nullspace `Q` of `C` and mass-orthonormalizes its columns. The physical velocity is `u = l + Q z`, with `C l = b`, `C Q = 0`, and `Q^T M Q = I`. The lifting is mass-orthogonal to the chart. Every independent coefficient remains in `z`; there is no model reduction. The resulting quadratic ODE is `z' = Q^T f(l + Q z)`. Coefficient order, dimension, rank and residuals are inspectable.

The two-triangle periodic fixture has 12 local coefficients, six independent normal constraints, one independent divergence constraint and five independent velocity coordinates. Both constant mean-flow modes are retained. The generic box implementation reproduces this dimension and supports the corresponding full tetrahedral spaces. The dense reference constructor admits at most 768 local velocity coefficients; larger spaces are rejected before dense assembly. Topology-based estimates operate beyond that dense boundary.

Convection uses the conservative volume term and central interior/periodic flux. For divergence-free, normal-continuous velocities on a closed domain its kinetic-energy contraction vanishes. Volume quadrature integrates the quadratic term exactly; facet quadrature integrates the cubic flux exactly. The generic implementation uses a symmetric interior penalty Laplacian, with penalty `40/h`, where `h=d*cell_volume/facet_measure`. The original two-triangle fixture uses `20/h`. Essential tangential traces enter the consistency and penalty loads. Boundary convection uses the central average of interior and prescribed traces wherever a velocity is prescribed, and the interior trace at a natural outlet. There is no sign-dependent inflow/outflow switching in the drift.

The full local momentum residual is reconstructed as a combination of normal multipliers and P0 pressure forces. Closed and periodic domains use the volume-weighted zero-mean pressure gauge. Natural traction at an open outlet fixes the pressure level; a mean-zero constraint is not added there. Reference outputs retain full momentum, divergence and boundary residuals.

## Physical case contracts

Six JSON files freeze the requested families and Reynolds variants. `cases::box_reference` supplies actual full-DG initial projection and classical RK4 references for both Taylor–Green and both cavity families. The 2D Taylor–Green reference reports analytic velocity and pressure errors. Cavity outputs include centerline and midplane samples, the full steady residual and an explicitly coarse 2D streamfunction-extremum candidate. A final time by itself does not establish steady state.

`cylinder::reference("shedding2d", ...)` supplies a polygonal approximation to the DFG2D2 channel and cylinder. Radial rays include the rectangular-domain corners, so the exterior channel is preserved. The cylinder is an inscribed polygon whose maximum radial deviation is reported. The quadratic inflow is projected onto each complete affine normal trace, preserving its facet-integrated flux. Nonzero normal lifting, no slip and natural outlet traction are assembled. Forces integrate both recovered pressure and elementwise viscous traction. Short reference runs and decreasing polygon error are not a validated drag/lift/Strouhal benchmark.

The approved `shedding3d` manifest retains the stationary-cylinder setup from [Kim et al., JFM1001 A24](https://doi.org/10.1017/jfm.2024.1079): the specified 50D by 80D by 4D domain, Neumann far field, convective outlet, periodic span, and a minimum of 200 observed shedding cycles. The backend does not yet implement those far-field/outlet dynamics, so physical execution is explicitly rejected. It does not substitute prescribed far-field velocity or natural outlet traction. A span-periodic extrusion and algebraic topology accounting are available for estimates, which do not count as execution admission. Fixed absolute time windows in the manifest are computational budgets and never replace the observed-cycle requirement.

## Evidence and open work

The focused tests check complete rank, mass orthogonality, nonzero nonlinear convection, inviscid energy conservation, viscous decay, constant-flow invariance, pressure/momentum closure, nonzero inlet lifting, manufactured uniform channel flow, RK4 time refinement, and independent 2D Taylor–Green projection refinement. Both triangular and tetrahedral physical spaces are exercised. Cylinder tests compare actual rank with topology accounting and expose polygon geometry error.

Still open: physical BDM order above one, large/distributed physical mesh assembly, curved-boundary convergence, time-dependent normal liftings, the approved 3D cylinder boundary dynamics, established shedding statistics and certified benchmark comparison across the full physical time windows. Dense short references are distinct from the KvN configuration/time discretization and from quantum circuit execution. A successful algebraic QSVT residual alone does not establish physical mesh accuracy or phase-space truncation accuracy.

## Source boundaries

[Fu, An explicit divergence-free DG method for incompressible flow](https://arxiv.org/abs/1808.04669) supplies the divergence-conforming velocity formulation and constraint-elimination context. The implementation above documents its own chosen central and SIP forms; it does not claim to reproduce every operator or benchmark in that paper.

[Jemcov and Morris, Unitary discretization of the Koopman–von Neumann equation](https://arxiv.org/html/2605.19187v1) concerns Weyl ordering, norm-preserving configuration transport, and spectrally truncated examples. Those examples are not used as substitutes for the complete physical BDM/P state here. Configuration finite differences, configuration DG and physical-space DG are separate discretizations and must be identified separately in numerical evidence.
