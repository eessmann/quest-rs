# Maintained MathCore

`quest-mathcore` is a normal workspace dependency, named `mathcore` in Rust.
It owns the current exact Dashu affine engine, scoped symbol identities,
backend-neutral arithmetic contracts and constants, ordered typed expressions,
bounded dynamic expressions, and sparse multivariate rational polynomials.
Numerical implementations, interval payloads, AD and proof orchestration remain
in `quest-numerics` and `quest-polynomial`. `quest-symbolic` retains independent
source replay and binding checks outside the engine being checked.

The neutral `geometry` module provides bounded exact dyadic triangle/tetrahedron
conformity, standalone nondegeneracy and exact displacement comparisons. Its
division-free integer predicates share the binary64 decoder with exact rational
import; they perform no symbolic construction. See [GEOMETRY.md](GEOMETRY.md)
for coordinate, arithmetic-profile, work and storage admission and the separate
topology/numerical-conditioning obligations.

`ExactConstant` distinguishes integers, exact ratios, exact decimal text,
symbolic pi and stored binary64 values. Binary64 source values keep their bits,
including signed zero. Explicit `multivariate::rational_constant` imports a
binary64 value as its exact dyadic rational; that conversion deliberately has
rational zero semantics. Polynomial extraction from a dynamic expression
rejects floating constants and pi until the caller makes such a conversion.

Typed expressions retain their original static nodes and operand order.
`DynamicExpression::from_typed` captures those nodes once with supplied scoped
symbols. `lower` converts constants and bindings once for a selected backend;
kernel evaluation has no symbolic construction or constant parsing. Dynamic
differentiation retains a source guard: evaluating the derivative first executes
the original expression, so identities cannot erase missing bindings or domains
such as division by zero. Formal polynomial simplification returns both the
canonical exact polynomial and the ordered source; it never claims permission
to reassociate floating arithmetic.

`SparsePolynomial::from_terms` accepts a fixed ordered symbol vector and terms
of `(Vec<u32>, RBig)`, where each exponent vector has one entry per symbol.
Terms have deterministic lexicographic ordering. Addition, multiplication,
differentiation and simultaneous substitution retain exact coefficients.
`lower` converts each coefficient once into a `PolynomialKernel` that uses
backend operations and validates every input.

Explicit limits bound variables, terms, degree, coefficient growth, modeled
storage, logical expansion work, expression nodes and depth. Stored vector
capacities are checked even for terms that later cancel. Sharing and repeated
lowering retain logical source admission costs. Numerical kernels use backend
visits and storage admission; backends may additionally enforce their precision
and operation budgets. The supported dynamic operations are ordered add,
subtract, multiply, divide, negate, exp, log, sin, cos and sqrt; general power
requires explicit multiplication. No epsilon simplifier or approximate legacy
CAS participates in exact proofs. See `UPSTREAM.md` for pinned provenance and
`LICENSE` for the upstream notice.

Scope validation admits its complete sorted `(symbol, original input index)`
scratch buffer before allocating it. Its logical work model charges `2n` for
copying and scanning plus `4n ceil(log2(n))` for the in-place sort. Typed capture
also admits all input nodes, the complete output structure, and their combined
work before construction; owned constant payloads are checked before cloning.
Lowering admits source, scope scratch and projected kernel storage together,
and charges scope lookup work before numerical backend visits. These checks
apply even when the expression uses only one symbol or is constant. Sorting
the validation buffer never changes the caller's numerical input order.

A lowered kernel freezes the scalar values produced by the lowering backend.
Use a compatible evaluation policy; lower again to change coefficient precision.
The kernel is an owned numerical representation, rather than a new exact-source
certificate. `quest-polynomial::ExactMonomialTarget` makes this distinction
explicit for univariate QSP/QSVT target construction: it retains the ordered
source, exact polynomial, selected binary64 coefficients, and an outward
coefficient-rounding bound on [-1,1]. Basis-conversion and circuit errors remain
separate obligations.
