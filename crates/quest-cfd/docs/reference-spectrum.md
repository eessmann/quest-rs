# Bounded interval spectral evidence for stored histories

`history_spectrum::reference_spectrum` supplies an explicit small-reference
certificate when the existing sparse DG1/DG2 history bounds are inconclusive.
The default solver continues to require its analytic sparse evidence. This
opt-in path consumes classical dense storage and cubic construction work, and
is restricted to 512 history coordinates by default and 2048 absolutely.
Its returned evidence is labelled `DenseReference`.

## Derivation and independent verification

Let H be the exact matrix represented by the stored binary64 CSR entries.
Partial-pivot Gauss–Jordan arithmetic constructs a finite binary64 candidate X.
No accuracy property of that elimination is assumed. A separate checker uses
`quest_numerics::Interval` directed operations to enclose the complex entries of

\[
R=I-XH.
\]

It accumulates upper bounds on the absolute row and column sums, including every
stored complex phase. For any matrix Z,

\[
\|Z\|_2\le\sqrt{\|Z\|_1\|Z\|_\infty}.
\]

Let the resulting upper bounds be \(\rho\ge\|R\|_2\) and
\(\chi\ge\|X\|_2\). If \(\rho<1\), the Neumann expansion makes XH
invertible. Since H and X are square, H is invertible, and

\[
H^{-1}=(XH)^{-1}X,\qquad
\|H^{-1}\|_2\le\frac{\chi}{1-\rho},\qquad
\sigma_{\min}(H)\ge\frac{1-\rho}{\chi}.
\]

The last expression is evaluated with downward rounding. The upper singular
bound comes from the existing outward sparse row/column norm calculation.
This posterior-verification approach is part of the standard approximate-inverse
framework discussed by [Rump, *Verification methods for dense and sparse systems
of equations* (2010)](https://www.tuhh.de/ti3/rump/intlab/ActaNumerica2010.pdf).
The equations above specify the particular normwise certificate implemented here;
we do not claim to implement all of that paper's algorithms.

The proof applies equally to complex nonnormal histories and to temporal DG1 and
DG2. It requires neither eigenvalue decay nor normal equations. A singular H,
nonfinite candidate, interval overflow, nonpositive bound or \(\rho\ge1\)
rejects. There is no epsilon-based algebraic zero or rank decision. A failed proof
does not imply the matrix is singular.

This certifies the stored rounded operator. Physical assembly error, temporal
quadrature error, Carleman truncation, configuration approximation, state
preparation, phase synthesis and measurement remain separate obligations.

## Ownership, limits and execution

The constructor admits the complete borrowed history, both dense arrays,
interval row/column scratch and metadata before reference allocation. Its report
records the conservative simultaneous byte/work envelopes, actual elapsed time,
\(\rho\), \(\chi\), both singular bounds and the temporal order. These are managed
storage envelopes, not RSS or allocator-bookkeeping measurements. Preceding
physical/lift construction is outside this constructor's envelope.

The candidate X is discarded before circuit preparation. The public
`solve_history_with_reference_spectrum` computes the certificate for the exact
history supplied to that call, then passes it to the private solver core. Callers
cannot substitute another history behind a saved certificate through this API.
The circuit still coherently prepares the original RHS and executes QSVT of the
matching encoding of H adjoint, with the existing physical scale and residual
check. It never returns X times the RHS as a quantum result.

The CLI opt-in is explicit:

```sh
cargo run -p quest-cfd --features quantum -- solve --case burgers \
  --lift carleman --carleman-order 2 --horizon 0.1 --time-cells 2 \
  --reference-spectral-bound --max-degree 16383 --backend quest-cpu
```

This command is an attempted experiment, not a promise of admission: the QSP
certificate, state, query or execution budgets can still reject. Passing
`--no-certify` explicitly omits the optional phase certificate; a successful
execution under that option must retain this limitation. Analytic-default
`solve`, distributed generated histories and `resource-build` do not silently
invoke this dense reference path.

Run the focused verification with:

```sh
cargo test -p quest-cfd --test history_spectrum
cargo test -p quest-cfd --lib history_spectrum
```

The tests cover a known constant-history singular value, complex nonnormal
DG1/DG2, the full eight-coordinate Burgers order-two history at T=0.1, resource
rejections, singular histories, malformed candidates and the unchanged coherent
scalar circuit. Broader physical convergence is not inferred from these tests.
