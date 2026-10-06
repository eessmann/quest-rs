# Full auxiliary-field KdV discretization

The frozen nonlinear case is `u_t+6*u*u_x+u_xxx=0` on periodic `[0,2*pi]`, with `u(0,x)=0.05*cos(x)`, final time 0.1, four physical DG2 cells and two initial DG1 time slabs. An auxiliary field obeys `phi_t-phi_xxx=0`, initially zero. Its **evolving coefficients are part of the physical discretization**, so the default complete dimension is 24. Refining the physical grid or degree retains both full fields. `cases/kdv.json` records this scalar-DG contract separately from the incompressible benchmark schema.

The construction follows the doubled-field ultraweak method of [Fu and Shu](https://arxiv.org/html/1805.04471v1); its [PDF](https://arxiv.org/pdf/1805.04471) was also checked. Their modification conserves the combined squared L2 norm, while the fields can exchange energy. It does not justify removing the auxiliary state or claiming that the physical-field energy alone remains constant. The implementation supports physical DG2 and DG3.

For a cell of length h, the complete mass-orthonormal basis is

```
e_l(x) = sqrt((2*l+1)/h) P_l(xi),  xi in [-1,1].
u_h = sum_l a_l e_l,   phi_h = sum_l b_l e_l.
```

Every coefficient is retained, ordered as the complete u block followed by the complete phi block. Eight-point Gauss integration is exact for the polynomial integrands here: the nonlinear volume term has degree at most `3*p-1`, which is eight for DG3. Initial cosine projection and analytic error comparison use this same high-order quadrature; this does not assert exact integration of trigonometric functions.

The implementation derives its signs from the requested PDE. With outward boundary sign n, define

```
B(q_hat,v) = q_hat*v_xx - q_x_hat*v_x + q_xx_hat*v.
<a_t,v> = integral(u*v_xxx) - sum_faces n*B(u_hat,v)
          + integral(3*u^2*v_x) - sum_faces n*f_hat*v.
<b_t,v> = -integral(phi*v_xxx) + sum_faces n*B(phi_hat,v).
```

At an interface the derivative fluxes use averages of the same field plus half the right-minus-left jump of the other field. The nonlinear numerical flux is `u_left^2+u_left*u_right+u_right^2`, consistent with the physical flux `3*u^2` and its square-entropy potential `u^3`.

The displayed left-face signs in equations 2.4c–d of the source do not form an oriented boundary bracket. Its displayed nonlinear operator/sign combination also needs care when translating the stated PDE. Those displays are not copied literally into the implementation. The oriented integration-by-parts formulas above are checked through a skew full linear operator, constant fields, both field means, the nonlinear combined-energy identity, the Airy propagation direction, and a separate nonlinear-convection consistency check. Reversing the PDE sign can preserve energy; the consistency/phase tests are therefore necessary independent checks.

The weak-form coefficients are assembled directly into shared MathCore sparse quadratic polynomials. Mesh pi, normalization square roots and quadrature have already been evaluated in binary64; the resulting coefficients are explicitly recorded as exact dyadics. This is exact algebra over the numerical coefficient records, not a claim that pi or its square roots became rational. Polynomial lowering occurs once. `direct_drift` independently computes volume integrals and interface fluxes without the extracted coefficient records. The classical RK4 helper uses this independent full residual and is cross-checked against the prepared polynomial trajectory.

`KdvDg::new(cells,order)` selects nonlinear KdV. `linear_airy(cells,order)` explicitly selects the distinct analytic validation problem with exact physical field `amplitude*cos(x+t)`. Nonlinear reference reports do not label that Airy solution as a KdV exact reference. The report includes all final u and phi coordinates, both means, separate and combined energy diagnostics, and the auxiliary L2 norm. Energy methods use squared L2 norms without a factor one half. RK4 introduces temporal error and is not claimed to conserve energy exactly.

Construction admits the full dimension, raw contribution count, work, and conservative concurrent assembly/kernel storage before term allocation. Allocation-heavy local vectors reserve fallibly. Shared MathCore/PolynomialOde budgets apply in addition. Default aggregate limits are 128 complete coordinates, 512 MiB and 256 million logical assembly work units; shared polynomial limits are 256 MiB and 256 million work units. The classical integrator separately rejects more than one million steps or one billion modeled scalar operations. Limits reject the whole requested case rather than deleting modes. Degrees outside 2 and 3 and fewer than two cells are unsupported.

The skew conservative system has no strict decay certificate. Existing coefficient/scaling diagnostics retain means and auxiliary coordinates, report experimental normalization, and do not manufacture a positive RC decay certificate. Full symmetric Carleman dimensions use all 24 default coordinates. The order-one hierarchy has dimension 24; higher orders use the shared arbitrary-width combinatorial estimator and are subject to their own simulation admission.

The test report in `.superpowers/sdd/2026-10-05-sparse-mathcore-cfd/task-9a-report.md` records actual Airy and temporal refinements. In particular, the coarsest DG2 pair is pre-asymptotic with the mandated zero auxiliary initial field; an ideal coarse-grid rate is not asserted. Actual quantum CFD execution, coherent RHS preparation, history-error control and native circuit admission belong to the common solver workflow, not to this classical reference module.
