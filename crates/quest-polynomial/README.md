# quest-polynomial

Typed polynomial mathematics, static functions and one owning Remez engine.

- `Function<E>` and `function!` retain concrete expression structure and exact captured constants.
- Arithmetic and AD are supplied by `quest-numerics`; binary64 and multiprecision use the same expression and exchange engine.
- `Polynomial<B,C,D>` combines basis, coefficient arithmetic and checked runtime/static shape.
- `RemezRequest` separates candidate generation, uniform-error certification and minimax-gap certification. Failures retain the original request and numerical evidence.
- Binary64 export is explicit and certified after coefficient rounding. Custom functions or enclosing backends retain assumption-bearing evidence.

See the [numerical guide](../../docs/book/src/numerical-polynomials.md), [static numerical architecture plan](../../docs/plans/2026-10-02-static-numerical-core.md), and executable tests/examples. This crate requires the project-local nightly toolchain.
