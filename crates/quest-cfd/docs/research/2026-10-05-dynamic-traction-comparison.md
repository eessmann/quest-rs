# Pressure-coupled dynamic traction: proposed comparison, not the approved wake case

Status: mathematical research/proposal only. No case manifest, implementation or execution status changes. The approved three-dimensional wake boundary substage remains open.

## Provenance and the unresolved source contract

The [wake paper, section 2.1](https://www.cambridge.org/core/journals/journal-of-fluid-mechanics/article/influence-of-threedimensionality-on-wake-synchronisation-of-an-oscillatory-cylinder/90E5A0DC88CEAFC332FECE5C4D2C47E1) specifies velocity Dirichlet inlet/cylinder data, componentwise far-field normal-gradient zero, spanwise periodicity and a convective outlet. It uses finite volumes and fractional stepping. That section does not give the outlet equation, convection-speed rule, pressure boundary conditions or intermediate/corrected-velocity treatment. In particular, the task's explicit outlet equation with speed U_infinity is an adopted interpretation, not a displayed equation from that section.

[Dong, equations (10)–(12)](https://arxiv.org/html/1506.01320) explicitly distinguishes pressure-coupled dynamic traction from pure convective velocity plus zero pressure. Equations (4)–(9) introduce a separate backflow modification and boundary energy. The simpler equation (10) is the comparison proposed here; it has no general backflow energy guarantee. [Bothe, Kashiwabara and Köhne, Remark 3.1 and section 3.3](https://arxiv.org/pdf/1603.09220) analyze related stress-coupled dynamic conditions and explain a missing-pressure-trace defect for their normal dynamic condition. Their theorem is not a proof for this proposed DG scheme or the literal wake boundary contract.

## Concrete comparison equations

Use the name **pressure-coupled dynamic traction comparison**, with a distinct case identifier if implemented. Use density-normalized pressure, constant viscosity nu>0, fixed geometry and the existing unsymmetrized viscous operator -nu*Delta(u). This traction convention must not be mixed with symmetric Cauchy stress without rederiving both SIP and boundary terms.

On the outlet set tau=nu/U_infinity>0 and prescribe

```
tau*u_t + nu*partial_n(u) - p*n = g_out,
g_out = -p_infinity*n.
```

Pressure p is an unknown, including its outlet value. Equivalently,

```
u_t + U_infinity*partial_n(u) = (U_infinity/nu)*(p-p_infinity)*n.
```

Only tangential components have the pure convective form. The normal component contains pressure. Setting p_infinity=0 chooses the exterior traction reference; it does not impose p=0 on the outlet.

To obtain a complete comparison geometry using the same weak machinery, give the distant lateral boundaries **static traction**

```
nu*partial_n(u) - p*n = -p_infinity*n.
```

This is explicitly a replacement boundary model for the comparison, not componentwise partial_n(u)=0. Tangential derivatives vanish, while the normal derivative is coupled to pressure. Keep the source case's inlet/cylinder velocity and spanwise periodicity. No zero-gradient or source-reproduction label applies to this comparison.

## Independent weak-form and full-coordinate derivation

For velocity tests vanishing on essential Dirichlet boundaries, integration by parts in the momentum equation gives the exterior term

```
< p*n - nu*partial_n(u), v >.
```

Substituting the outlet condition adds a positive boundary mass and a prescribed load:

```
m_star(u_t,v) + c(u;u,v) + a_SIP(u,v) - (p,div(v))
    + interior pressure/normal-continuity terms
  = (f,v) + <g_out,v>_out + <g_far,v>_far,

m_star(w,v) = (w,v)_Omega + tau*<w,v>_out.
```

Use the consistent central/split convection whose energy contraction reproduces physical exterior flux. Do not zero that flux while adding boundary mass. For homogeneous Dirichlet data and zero loads, the continuum calculation is

```
E_star = (||u||_Omega^2 + tau*||u||_out^2)/2,
dE_star/dt = -nu*||grad(u)||_Omega^2
             - (1/2)*integral_open (u.n)*|u|^2.
```

With forces, nonzero Dirichlet data and exterior loads, include their work. The discrete counterpart replaces volume viscous dissipation by the actual coercive SIP form and includes any explicitly chosen numerical dissipation. Dissipation follows for nonnegative normal velocity on all open boundaries; backflow can inject energy. This is an energy identity to test, not an unconditional stability claim. Report physical volume kinetic energy separately from E_star.

For the complete BDM discretization, let c contain every velocity coefficient and let Cc=d(t) impose all cell divergence constraints, interior/periodic normal continuity, and prescribed normal boundary data. Outlet and static-traction normal coefficients remain free. With trace matrix T_out and facet mass W_out,

```
M_star = M + tau*T_out^T*W_out*T_out,
M_star*c' + C^T*lambda = F(c,t),
C*c = d(t).
```

M_star is positive definite because M is. Build a full basis Q of ker(C), without truncation, normalized by Q^T*M_star*Q=I. Choose a compatible lifting l(t), C*l=d, and use

```
c = Q*a + l(t),
a' = Q^T*(F(Q*a+l,t) - M_star*l'),
C^T*lambda = F(c,t) - M_star*c'.
```

Pressure and hybrid multipliers are recovered from the full momentum residual. Constraint dependencies, if any, must be identified by rank, not removed by a hard-coded closed-domain pressure rule. A fixed exterior traction reference generally fixes the pressure level on connected open domains: adding a constant to p alone changes the boundary condition. Do not additionally force zero mean pressure. Shifting p and p_infinity together is a useful invariance test.

No independent boundary state is needed when its trace is T_out*c: those degrees of freedom are already retained in c and receive the new mass. A hybrid implementation introducing independent trace variables must retain their complete trace equations and compatibility relations; any exact algebraic elimination must preserve all independent dynamical coordinates. The mass change requires recomputing whitening and lifting. Volume-energy observables must still use M, not mistake the new coordinate norm for volume kinetic energy.

With fixed tau and central polynomial convection this remains a quadratic full-state ODE, suitable for the existing polynomial/Carleman and history interfaces after exact full-coordinate transformation. The pressure solve, full chart and inverse mass remain nonlocal costs. Count their storage/work; facet locality does not imply a cheap sparse transformed drift. Dong's optional tanh/sign backflow term is a different, nonquadratic boundary model and cannot silently enter the present quadratic adapter.

## Required validation before implementing a wake comparison

1. Independently integrate facet polynomials and verify the positive M_star increment, full rank/nullity, Q^T*M_star*Q=I, and complete trace-coordinate reconstruction. Test tau=0 against static traction and positive tau against a separately assembled saddle system.
2. Use divergence-free manufactured fields with nonzero pressure and prescribed boundary loads. Check momentum, continuity, both traction residuals and outlet acceleration. Include a case with partial_n(u)=0 but p nonzero to prove zero traction is not being mislabeled zero gradient.
3. Check the complete energy identity including boundary storage, convection flux and nonzero-data work. Include backflow and require the measured positive contribution to be reported, not clamped or called stable. Check physical energy and boundary energy separately.
4. Compare full chart drift and recovered pressure with the complete mixed system; check a simultaneous pressure/exterior-reference shift, lifting derivatives, and polynomial residuals at independent states. No coordinate may disappear as tau changes.
5. Run manufactured h/p refinement, then domain/outlet-position and tau sensitivity on a separately named cylinder comparison. Only afterward compare lift/drag/shedding observables; those do not prove equality to the source boundary treatment.

## Information still needed for literal approved-case reproduction

Obtain the source solver's pressure Poisson/correction boundary rules at inlet, cylinder, far field and outlet; its pressure gauge/reference; whether normal-gradient and convective rules apply to predictor or corrected velocity; how the correction preserves them and global mass flux; the actual outlet speed/equation; and boundary-intersection handling. A source-matching DG derivation must then state the resulting coupled velocity-pressure boundary problem and demonstrate consistency/rank before converting it into complete constraint coordinates. Adding an arbitrary p=0 surface condition, or dropping the pressure boundary integral, does not resolve that missing contract.

## Follow-up source audit, 2026-10-06

The published article and [preprint v2, section 2.1](https://arxiv.org/html/2411.06279v2)
were checked again. They retain the velocity Neumann/convective statements
without an outlet equation, convection-speed rule or pressure closure.

The cited [Ham, Mattsson and Iaccarino method, section 2.7](https://web.stanford.edu/group/ctr/ResBriefs06/19_ham1.pdf)
describes a velocity predictor, pressure Poisson equation and corrections, with
boundary velocities supplied as known data. It does not identify how this
particular wake's convective and far-field data are constructed. Transferring
those general formulas cannot establish the missing case-specific contract.

A [later study using the wake data](https://www.cambridge.org/core/journals/flow/article/leveraging-threedimensionality-for-navigation-in-bluffbody-wakes/FE584651157C01793F9504EDDCA38AED)
names Cliff as its flow solver and refers back to Kim et al. for setup details.
Its data-availability statement offers flow data by request. The linked
[public code's README](https://github.com/karkris41295/single-agent-MPC-FTLE)
describes trajectory optimisation examples, rather than providing the required
wake boundary equations. These inspected sources therefore leave the closure
unresolved. No author contact, replacement manifest or alternate outlet
implementation was made during this audit.
