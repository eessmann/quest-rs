# quest-numerics

Binary64 interval, FFT and convolution kernels, reusable workspaces, caller-owned parallelism and scientific timing.

See the workspace mdBook guide and crate rustdoc for executable examples.

Reusable convolution kernels retain their numerical work buffers. The warmed
single-worker path is tested for zero allocations inside its pool scope.
Multithreaded Rayon may allocate scheduler queue blocks and lazy OS sleep
primitives. Batch calls inside `pool.install` to avoid repeated external job
injection, and account for scheduler allocations separately.

This Rust port draws on `quest-qsvt` revision `7fe7f740579b03c52a8cf48be6a31268b029c19f`. Its MIT notice is retained in `LICENSE-quest-qsvt`.

## Static arithmetic and differentiation

`arithmetic::Backend` owns checked arithmetic and exact constant imports.
`F64Backend`, `Interval64Backend`, `MpBackend`, and `MpIntervalBackend` choose
binary64 point, Maryada enclosure, Dashu ties-to-even, and Dashu directed
enclosure arithmetic respectively. Point and enclosure capabilities are separate;
corresponding pairs transfer coefficient endpoints directly without a decimal or
binary64 round trip. Multiprecision endpoints are exact stored dyadics. Captured
binary64 constants retain their represented bits; decimal and rational constants
are rounded in the selected direction. Binary64 conversion of exact constants
adaptively resolves rounding to avoid an intermediate-precision double rounding.

`JetBackend` carries value, first derivative, and second derivative;
`FirstBackend` carries only value and first derivative. `GradientBackend<B, N>`
uses a static derivative dimension with heap-owned derivative rows.
`ad::jacobian` admits dimensions and actual scalar storage through explicit
`JacobianLimits`; its seed slice and callback output vector avoid quadratic
stack frames. Ordinary backend evaluation supplies order
zero. Scalars need only `Clone`, and no target callback requires `Clone` or `Copy`.
Square root accepts zero as a value but derivative evaluation rejects its singular
endpoint. Logarithm requires a strictly positive complete input enclosure.

`Precision` controls bit precision, admitted exponents and modeled
operation count. Backends validate errors and domains before mathematical
shortcuts. Multiprecision trigonometric enclosures check all possible interior
extrema with directed pi and exact integer bounds; uncertain large reductions
conservatively widen to the full range. Upstream transcendental caches, temporary
allocations and algorithmic iterations are opaque and are not an allocator cap.

## Root covers

`roots` provides generic Newton, Krawczyk and scalar/vector Hansen–Sengupta
contractors and one deterministic scalar cover driver. Vector Jacobians use
`shapes::Matrix<T, N, N>`, a statically shaped heap-owned matrix. Extended division
retains both branches across a pole. Zero or singular preconditioning cannot
establish uniqueness.

A root cover's `covered` boxes enclose every remaining candidate at the requested
resolution (or a proven zero continuum); they do not necessarily contain roots.
`RootEvidence` distinguishes existence, at-most-one, and their conjunction.
`unresolved` retains boxes on budget exhaustion, arithmetic failure, or resolution
stalls; `failure` preserves the backend error and prevents `complete()`.
Stored MP limbs and vector headers enter byte admission, along with conservative
local scratch allowances. Callback allocations and opaque backend caches are
outside this workspace model.

All arbitrary callback results remain conditional on enclosing one continuously
differentiable function and its derivatives. `CertifyingBackend` is sealed and
only certifies the arithmetic implementation; a higher layer must also admit the
function semantics before presenting an unconditional theorem.

## Multiprecision dependencies

All production arbitrary-precision scalars use native Dashu values. Binary point
and endpoint scalars are `FBig<HalfEven, 2>`; nearest, lower and upper operations
use explicit `Context<HalfEven>`, `Context<Down>` and `Context<Up>` respectively.
Fallible context operations preserve upstream domain, exponent and certification
failures as checked arithmetic errors. Each backend owns its transcendental cache.
A directed result changes its type-level rounding mode without rerounding, so the
allowed add/sub guard digit remains part of the stored exact dyadic.

Decimal and integer-ratio ingress constructs exact Dashu integer operands, admits
them before cancellation, and performs one directed binary division. Binary64
import and export operate on exact integer significands and binary exponents,
including signed zeros, subnormals, midpoint ties and the finite overflow boundary.
Storage admission counts the native scalar header plus actual significand words
that exceed Dashu's two-word inline representation; working output estimates
include the permitted guard digit. Allocator capacity and transient allocations
remain outside this model.

`tests/exact_oracle.rs` reconstructs each native endpoint as a canonical Dashu
`RBig` and checks it against independently derived rational series and remainder
bounds. The oracle shares exact integer storage with production but none of the
rounded or transcendental floating-point algorithms. Zero extraction returns
before reading or shifting its special exponent. Rug, GMP, MPFR, Malachite and
backend-selection machinery are absent from this crate.

## Shared operation resources

`OperationLimits` replaces the former flat limits. `ShapeLimits` separates
`max_coefficients`, `max_fft_len`, and `max_completion_grid`; `ResourceLimits`
contains `max_peak_bytes` and `max_work_units`. The transitional `Limits` name is
an alias of this same configuration. The defaults remain conservative.
On 64-bit targets, `OperationLimits::million_degree()` explicitly selects
1,000,001 coefficient slots, a padded FFT limit of 2^21, 8 GiB of modeled peak
storage, and 2^37 cumulative work units. Its completion grid gate is 2^22;
admission of that grid does not establish successful completion or accuracy.

`OperationResources` is a cloneable shared ledger. Pass the same ledger to
`FftWorkspace::new_with_resources`, `ConvolutionWorkspace::new_with_resources`,
or `SharedConvolutionWorkspace::new_with_resources` to account for successive
kernels together. Constructors reserve owned buffers and conservative opaque
RustFFT plan estimates before allocation. Cached plans and scratch remain charged
until their owning workspace drops; output reservations follow output ownership.
An `Accounted<T>` owns both its result and reservation tokens. Keep these together
when using `into_parts`; `map` transfers the same ownership through an in-place
representation change.

Work charges cover complete kernel batches, accumulate across calls, and remain
spent after an execution failure. Parallel allocations are admitted as an ordered
atomic batch before dispatch. Reports separate live owned buffers from planner
estimates and include configured limits, requested peaks/work, admitted peaks/work,
and the last structured rejection. A modeled byte limit is neither a process RSS
quota nor an allocator guarantee; allocator metadata, scheduler state, and opaque
backend allocation details are outside the model.
