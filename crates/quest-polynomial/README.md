# quest-polynomial

Typed polynomial mathematics, static functions and one owning Remez engine.

- `Function<E>` and `function!` retain concrete expression structure and exact captured constants.
- Arithmetic and AD are supplied by `quest-numerics`; binary64 and multiprecision use the same expression and exchange engine.
- `Polynomial<B,C,D>` combines basis, coefficient arithmetic and checked runtime/static shape.
- `RemezRequest` separates candidate generation, uniform-error certification and minimax-gap certification. Failures retain the original request and numerical evidence.
- Binary64 export is explicit and certified after coefficient rounding. Custom functions or enclosing backends retain assumption-bearing evidence.

See the [numerical guide](../../docs/book/src/numerical-polynomials.md), [static numerical architecture plan](../../docs/plans/2026-10-02-static-numerical-core.md), and executable tests/examples. This crate requires the project-local nightly toolchain.

## Sharing resources

Use `OperationLimits { shapes: ShapeLimits { ... }, resources: ResourceLimits {
... } }` instead of flat coefficient/byte/work limits. `Limits` remains a
transitional alias of this unified configuration. Numerical accuracy belongs to
the relevant numerical policy and is independent of these capacity limits.

`Polynomial::new_with_resources` and `Polynomial::from_scalars_with_resources`
accept a shared `OperationResources`. Polynomial clones share their immutable
coefficient allocation and its reservation; the final owner releases it.
Evaluation work is cumulative. Binary64 basis conversion, differentiation, and
norm covers charge their modeled work before the batch and reserve their output
and temporary storage. Derived polynomials retain the originating ledger.
`operation_resources()` lets later numerical stages share that ledger explicitly.
Generic callback arithmetic and independent root/approximation proof policies
retain their own domain-specific contracts; resource admission does not establish
convergence, conditioning, or certification.
