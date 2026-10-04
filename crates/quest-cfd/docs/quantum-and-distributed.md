# Block encodings, inverse transforms, and distributed execution

This tutorial explains the quantum-circuit layer of `quest-cfd` and its classical distributed execution. It describes the current implementation and evidence recorded on **2026-10-04**, reviewed on 2026-10-05. The executed examples are state-vector simulations. They establish neither quantum hardware performance nor physical convergence of the CFD model. Read [method and theory](method-and-theory.md) for the physical discretization, [alternatives](alternatives.md) for competing approaches, and [NEXT_STEPS](../NEXT_STEPS.md) for outstanding work.

## 1. Which vector space is being encoded?

Let $a\in\mathbb R^m$ denote the independent physical velocity coordinates after enforcing the discrete constraints. Here $m$ is the physical chart dimension, not the number of configuration-grid nodes or ancillary qubits. With $J$ coefficients per configuration axis, the full tensor configuration space has

$$
N=J^m.
$$

The configuration amplitude is mass scaled: $z=W^{1/2}\psi$, where $W$ contains the configuration quadrature weights. The implemented KvN generator is

$$
G=W^{1/2}\left[-\frac12\sum_j(F_jD_j+D_jF_j)\right]W^{-1/2}.
$$

For the admitted real drift and central DG/SBP closure, its skew structure supports amplitude-space evolution. This connects to the unitary discretization principles of [Jemcov and Morris](https://arxiv.org/abs/2605.19187), while the repository's constrained physical drift and global history solve require their own analysis. Skew structure alone does not establish configuration truncation convergence.

With $T$ temporal coefficients, the causal DG history system is

$$
A x=b,\qquad A\in\mathbb C^{D\times D},\qquad D=NT.
$$

The history unknown concatenates configuration amplitudes at temporal nodes. It has configuration mass scaling but no additional temporal square-root mass scaling. Uniform coefficient sampling therefore differs from temporal quadrature sampling. The right-hand side injects the initial condition into the first temporal source block. The solver encodes this nonsymmetric $A$ directly through its adjoint, without forming normal equations. See [configuration assembly](../src/configuration.rs) and [history assembly](../src/history.rs).

## 2. The block-encoding identity

A block encoding consists of a unitary $U$, a positive normalization $\alpha$, and two isometries $P_R$ and $P_L$ embedding the logical column and row spaces into its larger Hilbert space:

$$
\boxed{P_L^\dagger U P_R=A/\alpha.}
$$

The associated orthogonal projectors are $\Pi_R=P_RP_R^\dagger$ and $\Pi_L=P_LP_L^\dagger$. Keeping isometries and projectors distinct matters for rectangular matrices: their logical coordinate counts can differ. Necessarily $\alpha\geq\|A\|_2$, since a compression of a unitary is contractive.

“Exact” here describes the mathematical construction with ideal rotations and phases. Binary64 coefficients, transcendental angle evaluation, and simulator arithmetic introduce separate errors. A verified scalar response polynomial does not automatically certify those encoding or execution errors. Conversely, testing only the successful block cannot verify the complete unitary: QSVT also uses its failure sectors and adjoint.

The [QSVT framework of Gilyén, Su, Low, and Wiebe](https://arxiv.org/abs/1806.01838) transforms singular values through alternating applications of an encoding, its adjoint, and projector phases. The repository exposes these interfaces through [compact logical spaces](../../quest-qsvt/src/space.rs) and owning replay objects rather than requiring a dense matrix for $U$.

## 3. Arithmetic structure and weighted stencils

When a stencil is a sum of arithmetic permutations,

$$
B=\sum_{t=0}^{L-1}w_tS_t,
$$

the shifts can be described without enumerating basis states. [`TensorShiftEncoding`](../../quest-qsvt/src/structured.rs) combines modular additions on disjoint registers. Controlled ripple-carry gates implement the additions, and reversing them implements the adjoint.

[`StructuredStencilEncoding`](../../quest-qsvt/src/structured_stencil.rs) freezes the complex weights and shift recipes. It removes zero-weight terms, pads the number of labels to $K=2^{\lceil\log_2\max(1,L)\rceil}$, and uses $\beta=\max_t|w_t|$, $\alpha=K\beta$. Uniform color preparation, a color-conditioned flag rotation and phase, and the selected arithmetic shift yield the block $B/\alpha$. Unused labels produce failure amplitudes. Coincident shifts can add in the matrix, so $\beta$ here is the largest **term weight**, not necessarily the largest assembled matrix entry.

The implementation stores recipes and coefficients, not CSR rows or a permutation table of length $N$. This is useful structural input access. It does not implement the complete base, preamplified, or PREP/UNPREP constructions studied by [Sünderhauf, Campbell, and Camps](https://arxiv.org/abs/2302.10949v2). Their paper separates normalization and data-loading costs for structured matrices. The current uniform-color weighted stencil has its own explicit normalization; no coefficient-weighted PREP identity or complete paper implementation is claimed.

## 4. Stored sparse matrices: weighted matchings

For a supplied sparse matrix, [`MatchingEncoding::from_sparse`](../../quest-qsvt/src/matching.rs) consumes canonical entries: duplicates have been summed deterministically and exact zeros removed by the [sparse numerical layer](../../quest-numerics/src/sparse.rs). In row-major order, it greedily colors the bipartite row-column graph. Each color has at most one entry per row and per column. If the maximum row and column degrees are $\Delta_r$ and $\Delta_c$, the nonempty construction uses

$$
K_0\leq\Delta_r+\Delta_c-1,\qquad
K=2^{\lceil\log_2K_0\rceil},\qquad
\beta=\max_{ij}|A_{ij}|,\qquad \alpha=K\beta.
$$

This generic greedy matching construction is a repository implementation, not an attribution to the structured-matrix paper. Its normalization can be much larger than the spectral norm. That gap affects inverse degree and success probability even when storage is sparse.

For an $R\times C$ matrix, the system register has $n=\lceil\log_2\max(R,C)\rceil$ qubits, padded to $M=2^n$ basis states. Write $\ell=\log_2K$ for the color width, reserving $m$ for physical coordinates. The encoding layout is flag bit 0, system bits $1,\ldots,n$, and color bits $n+1,\ldots,n+\ell$. Its width is $n+\ell+1$.

An edge $j\mapsto i$ with coefficient $a=|a|e^{i\phi}$ receives

$$
\theta=2\arccos(|a|/\beta),\qquad
D_\phi R_y(\theta)=
\begin{pmatrix}
e^{i\phi}\cos(\theta/2)&-e^{i\phi}\sin(\theta/2)\\
\sin(\theta/2)&\cos(\theta/2)
\end{pmatrix}.
$$

The first column has successful coefficient $a/\beta$; the second column specifies the equally necessary behavior on an incoming failure flag. The phase acts only on flag zero. For each color, replay first applies the default $R_y(\pi)$, then the edge-specific correction $R_y(\theta-\pi)$ controlled on its original column. Missing entries therefore have zero successful coefficient in the ideal construction.

A partial matching is not yet a permutation. Viewing its edges as directed system-index arrows produces disjoint paths and cycles. The constructor closes each path by mapping its terminal index back to its initial index, retains existing cycles, and fixes untouched indices. Completion-only arrows have zero successful coefficient but remain essential to reversibility. Compact touched-index tables support forward and inverse lookup; the entire padded basis is never enumerated during completion.

The completed permutation acts on **both** flag sectors. With Hadamards before and after the color-controlled operations, the zero-color compression averages their successful blocks:

$$
\langle0_{\rm color},0_{\rm flag}|U|0_{\rm color},0_{\rm flag}\rangle
=\frac1K\sum_k A_k/\beta=A/\alpha.
$$

Logical row and column range restrictions remove rectangular padding. Dummy colors and padded inputs still have defined unitary behavior. An empty matrix uses $K=\beta=\alpha=1$ and an entirely unsuccessful logical block; this does not make it invertible. Adjoint replay reverses every operation and angle. Outer controls and target remapping cover the complete circuit, including its default rotations and permutations, so inactive control sectors remain unchanged.

## 5. Compact replay and projector ownership

The owning encoding stores coefficients, touched permutations, source metadata, and checked resource estimates. Its [replay visitor](../../quest-qsvt/src/replay.rs) produces gates incrementally. Permutation cycles become pivot transpositions realized by Gray-code ladders; no dense $U$ or permanently expanded gate sequence is required. The budgeted `to_oracle` conversion is available for small consumers that need expanded instructions.

`LogicalSpace::constrained_range` represents fixed ancilla bits plus a range in packed free-bit coordinates. Free physical bits in ascending order determine the logical coordinate order. This expresses flag/color success and unpadded ranges without a projector table of length $2^Q$.

[`MatchingTransform`](../../quest-qsvt/src/replay_transform.rs) owns the encoding and actual projector-phase payload. Its exported `MatchingSchedule` retains scalar source metadata, phases, readout, and optional response evidence without retaining the full coefficient source. Semantic steps are visited lazily. Native execution consumes that schedule with an owned matching shard. Construction from arbitrary phases makes no certification claim; `from_certified_projector` preserves the exact admitted phase values and readout covered by the supplied certificate.

The transform adds one response qubit to the matching encoding. Thus its total width is $Q=n+\ell+2$; the 17-qubit smoke receipt includes that response ancilla.

## 6. Why the inverse encodes the adjoint

Let $A=V\Sigma W^\dagger$. Odd singular-value transformation of an encoding of $A$ produces $V p(\Sigma/\alpha)W^\dagger$. Even if $p(x)\approx c/x$, that has the orientation of $A$, whereas

$$
A^{-1}=W\Sigma^{-1}V^\dagger.
$$

The [history solver](../src/solve.rs) therefore constructs an encoding of $A^\dagger=W\Sigma V^\dagger$. Its odd response approximates $c\alpha A^{-1}$, taking the normalized RHS to unnormalized successful amplitudes

$$
y\approx c\alpha A^{-1}b/\|b\|_2.
$$

Physical decoding multiplies these amplitudes by

$$
\boxed{\|b\|_2/(\alpha c).}
$$

The decoded result belongs to the history's mass-scaled configuration coordinates. It is not automatically an unweighted physical field or a normalized final-time probability distribution.

[`GeometricReciprocal`](../../quest-qsvt/src/reciprocal.rs) uses the odd polynomial

$$
g_b(x)=c\,\frac{1-(1-x^2)^b}{x},\qquad g_b(0)=0,
\qquad c\leq\frac1{2\sqrt b}.
$$

The subscript $b$ here is an integer approximation parameter, distinct from the RHS vector. For admitted singular-value bounds $0<s_-\leq\sigma(A)\leq s_+$, let $\delta=s_-/\alpha$. On $\delta\leq|x|\leq1$, its reciprocal remainder is bounded by $c(1-\delta^2)^b/\delta$. Throughout $[-1,1]$, $1-(1-x^2)^b\leq\min(bx^2,1)$ yields $|g_b(x)|\leq c\sqrt b\leq1/2$, leaving boundedness margin for synthesis.

The implementation forms Chebyshev coefficients with positive-binomial interval recurrences, accounts for discarded coefficient tails and coefficient rounding, and verifies the resulting global bound. Chebyshev truncation can make the retained degree far smaller than the formal degree $2b-1$. This is an implementation of this geometric approximant, not a claim of optimal reciprocal degree.

The existing QSP synthesis backend produces candidate phases; its nonlinear-Fourier foundations are related to [Laneve's GQSP/NLFT analysis](https://arxiv.org/abs/2503.03026v2). A separate certification stage bounds the actual rounded projector response. The replayed transform includes two coherent response branches and their readout; count its actual encoding calls and gates rather than interpreting polynomial degree as total execution work.

The reusable residual admission API preserves a conditional bound

$$
\frac{\|A\widehat x-b\|_2}{\|b\|_2}
\leq\frac{s_+}{\alpha c}
(\epsilon_{\rm poly}+\epsilon_{\rm projector}+\epsilon_{\rm execution}).
$$

The execution premise must cover encoding and arithmetic errors. Spectral evidence also matters: analytic bounds, dense validation, and caller-supplied premises retain their distinct provenance. Built-in history inverse evidence currently supports admitted DG1 cases; unsupported temporal orders or overflowing bounds remain explicit limitations.

History spectral evidence combines DG1 coercivity and causal inverse estimates with a bound on the finite-precision Hermitian defect. The upper bound uses $\sqrt{\|A\|_1\|A\|_\infty}$, computed from absolute column and row sums. These enclosures concern the directly assembled history operator; they do not silently replace it with a normal-equation surrogate.

The CLI additionally computes a binary64 sparse residual of the decoded solution and rejects excessive residuals. That measured check is useful, but is not an independent interval proof. Neither it nor a uniform response certificate bounds physical discretization, configuration-boundary leakage, initial regularization, or observable bias.

## 7. Preparation, success, and measurement costs

The current solver constructs a full classical RHS vector and initializes a simulator state from it. It does not supply an efficient coherent loader for arbitrary CFD amplitudes. Likewise, reading every successful amplitude from a simulator is a classical verification facility, not a scalable quantum readout procedure.

In the ideal inverse model, the success probability is

$$
p_{\rm succ}=(\alpha c)^2\|A^{-1}b\|_2^2/\|b\|_2^2.
$$

Independent postselection takes about $1/p_{\rm succ}$ attempts per accepted history sample. Conditioning further on a final temporal slice costs its share of successful probability. Temporal quadrature weighting requires a suitable measurement or additional preparation; equal history-index sampling does not implement it automatically.

For a bounded diagonal observable, accepted computational-basis samples can estimate its expectation. If its magnitude is bounded by $M_O$, elementary sampling needs order $M_O^2\epsilon^{-2}\log(1/\eta)$ accepted samples for absolute precision $\epsilon$ and failure probability $\eta$. Each attempt includes RHS preparation, transform execution, and measurement. Non-diagonal observables require additional measurement structure. Amplitude amplification or estimation may change these costs when coherent inverses and reflections are available; those algorithms are not implemented by this CFD CLI.

A useful cost assessment therefore includes classical discretization and sparse preprocessing, synthesis and certification, coherent input access, each full transform attempt, its success rate, and observable sampling. A logarithmic register size alone establishes no quantum advantage. Quantum hardware gate depth, precision, fault-tolerance overhead, and a competing classical observable solver would also be needed for a meaningful comparison.

## 8. Symbolic dimensions versus admitted execution

The symbolic resource planner can express $N=J^m$, $D=NT$, and exponential state-vector storage without materializing those quantities as arrays. Large physical coordinate counts can therefore be discussed honestly as symbolic formulas. This is an estimate, not admission to an executable encoding or simulator.

Executable basis indices must fit checked machine-width arithmetic and the native signed-index bridge. Color normalization also has checked integer conversions. Byte and work limits must admit simultaneously retained inputs, polynomial/certificate workspaces, and execution state/scratch. Distributed execution divides state storage; it does not remove index-width restrictions or exponential total state-vector storage. The solver's byte budget models managed payloads and concurrent workspaces, excluding allocator bookkeeping, shared libraries, and other process overhead: it is not an operating-system RSS cap.

## 9. What MPI owns and communicates

An immutable [`MatchingShard`](../../quest-qsvt/src/matching_shard.rs) owns records whose original source column satisfies $j\bmod P=r$, for rank $r$ among admitted power-of-two parts $P$. Records include completed destinations and flag rotation/phase parameters. Missing records imply identity permutation and zero successful coefficient. Completion-only transitions must still be present. Adjoint execution uses the same original-column coefficient owner, reading the forward destination before applying the inverse operation.

The scalar header includes dimensions, layout, normalization, source identity, expected completed-record count, and a commutative record digest. Collective preparation checks agreement, ownership, aggregate count/digest, and permutation validity. The FNV-based source identity and wrapping digest detect ordinary inconsistency and accidental corruption; they are **not cryptographic authentication**, collision-proof equality, or proof that the physical discretization was correct. Constructing shards from a global encoding remains a convenience producer. A distributed sparse producer/loader that avoids global source storage is pending.

The [local native API](../../quest/src/qsvt/matching.rs) and [collective runtime](../../quest/src/qsvt/matching/collective.rs) prepare owned coefficients, reusable buffers, and a native scratch partition. `CollectiveEnvironment::state_vector_local` admits local state storage explicitly. Bounded local reads and writes avoid a whole-state gather; `init_pure_from_root` is a separate full-host initialization path, not a distributed RHS generator. The prepared objects borrow their live environment and communicator/runtime, so those owners must outlive execution and destruction. This CPU/MPI lane uses the initializing MPI thread; the fused accelerator path is unsupported.

[Batched routing](../../quest/src/qsvt/matching/batched.rs) selects at most 64 flag pairs globally, with at most 128 amplitude records per rank and 5120-byte wire buffers. It routes requests to state owners, amplitudes to coefficient owners, and results to destination owners. Native indexed access handles whole local batches. Outputs go into scratch while reads see the unchanged input, preserving permutations across batches; scratch is installed after completion. The [transform runtime](../../quest/src/qsvt/matching_transform.rs) also streams compact projector phases.

Bounded buffers do not imply scalable communication. Every rank scans the global basis range, and routing uses all-peer count/payload exchanges. Three phases produce $6(P-1)$ exchanges per rank per batch. Count conversions are checked, but large-count jobs and multi-host behavior have not been accepted. Preparation errors can be agreed before mutation. A transport/native failure or panic during collective mutation triggers job abort to avoid stranded peers; the current abort uses `MPI_COMM_WORLD`, including when work used a split communicator. This is neither rollback nor recoverable task isolation.

## 10. Reading the recorded evidence

The [2026-10-04 verification record](../../../docs/verification/2026-10-04-quest-cfd.md) links the machine-readable receipts. The [smoke receipt](../../../docs/verification/data/2026-10-04-quest-cfd/smoke.json) has five physical coordinates, $N=1024$, $D=2048$, 15,060 history nonzeros, $\alpha=8$, degree 401, and 17 total qubits. Its spectral interval is $[0.5,1.0620868511305122]$, reciprocal scale approximately 0.00787817, and physical rescaling approximately 15.8666.

Scalar and QuEST CPU runs both measured relative residual approximately $5.25993\times10^{-6}$ and success probability approximately 0.00794757: about 126 independent attempts per successful history sample. The polynomial bound is approximately $1.10064\times10^{-6}$; the projector response bound is approximately $2.92317\times10^{-14}$. A reported zero conversion-roundoff estimate reflects preservation of the certified payload, not a zero-error certificate for all arithmetic.

Certification took about 21.6 seconds. Execution took about 0.729 seconds for the scalar reference and 5.779 seconds for QuEST CPU. Runs were concurrent, single-sample, single-rank, and used one OpenMP thread. These timings are diagnostic stage measurements, not speedup results. Peak RSS was 271,408 and 273,076 KiB, illustrating the distinction between modeled payload admission and process memory. All configuration cells in this tiny example touch the artificial boundary; it supplies no truncation-convergence evidence.

The separate [MPI receipt](../../../docs/verification/data/2026-10-04-quest-cfd/mpi.json) exercised 512 amplitudes on **one host**, including split communicators:

| World ranks | Maximum batched forward-plus-adjoint time (s) | Maximum sent bytes per rank |
| ---: | ---: | ---: |
| 1 | 0.000473702 | 0 |
| 2 | 0.000662154 | 7,728 |
| 4 | 0.000451483 | 5,904 |
| 8 | 0.000713954 | 6,096 |

Times include intermediate differential readback. Byte counts cover one adjoint query's application count/payload traffic, excluding collective, native, and protocol overhead. These are correctness diagnostics, not scaling benchmarks or distributed CFD solves.

The [acceptance receipt](../../../docs/verification/data/2026-10-04-quest-cfd/acceptance.json) records 1,237 passing tests and three existing scale skips, alongside native ABI and formatting/lint checks. Acceptance still excludes multi-host scale, complete distributed source/RHS production, forced allocator and malformed-transport fault injection, complete paper PREP/UNPREP encodings, and a converged physical observable error budget. Those boundaries guide the experiments in [NEXT_STEPS](../NEXT_STEPS.md).
