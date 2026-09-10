# Language grammar and numerical semantics

The text parser accepts an optional `OPENQASM 3.1;` header, includes, declarations, gate and subroutine definitions, aliases, expressions, and executable statements. Statements use semicolons; control-flow and definition bodies use braces. Identifiers include Unicode names such as `π`. Comments use `//` or `/* ... */`.

Supported statement families include typed declarations (`const`, `input`, `output`, local variables), global `qubit` declarations, assignments and compound assignments, gate calls and modifiers, `measure`, `reset`, `barrier`, `if`/`else`, `switch`/`case`/`default`, `for`, `while`, `break`, `continue`, `return`, and `end`. Gate modifiers are `inv @`, `pow(...) @`, `ctrl @`/`ctrl(n) @`, and `negctrl @`/`negctrl(n) @`. The executable power profile requires an `int` or `uint` repetition count; other categories need an explicit integer cast; negative counts apply the adjoint, and fractional matrix powers are outside this profile. A `gphase` operation has no ordinary target operands; controlled phase still acts on its controls.

This list describes the selected simulator profile, not a complete grammar for timed hardware OpenQASM. Pulse calibration, timing constructs, recursion, unsupported slicing/concatenation aliases, and unsupported types produce capability or syntax diagnostics.

## Grammar overview

This abbreviated grammar describes the structure used by the examples. `expression`
is governed by the precedence table below; type and capability admission further
constrain the parsed forms. An optional `?` suffix means zero or one item, `*`
means repetition, and `|` separates alternatives.

```text
module       = header? statement*
header       = "OPENQASM" "3.1" ";"
block        = "{" statement* "}"
declaration  = qualifier? type identifier ("=" expression)? ";"
qualifier    = "const" | "input" | "output"
alias        = "let" identifier "=" expression ";"
gate_def     = "gate" identifier parameters? operands block
subroutine   = "def" identifier "(" formals? ")" ("->" type)? block
assignment   = expression assign_operator expression ";"
gate_call    = modifier* identifier parameters? operands? ";"
modifier     = "inv" "@" | "pow" "(" expression ")" "@"
             | ("ctrl" | "negctrl") parameters? "@"
conditional  = "if" "(" expression ")" block ("else" block)?
for_loop     = "for" type identifier "in" iterable block
while_loop   = "while" "(" expression ")" block
iterable     = expression | "[" expression ":" (expression ":")? expression "]"
exit         = "break" ";" | "continue" ";" | "end" ";"
             | "return" expression? ";"
parameters   = "(" (expression ("," expression)*)? ")"
operands     = expression ("," expression)*
```

Gate parameters in definitions are names; subroutine formals are typed values or
references. `switch` bodies contain `case` label lists and an optional `default`,
each with a block; cases select a block without C-style fallthrough. Array types
carry a scalar element type and fixed dimensions; quantum registers use
`qubit[count]`. Exact accepted/rejected edge cases live in the parser and admission
contract tests, including diagnostics for unsupported profile features.

## Precedence

From weakest to strongest:

| Operators | Meaning |
|---|---|
| `\|\|` | Logical OR, short circuit |
| `&&` | Logical AND, short circuit |
| `\|` | Bitwise OR |
| `^` | Bitwise XOR |
| `&` | Bitwise AND |
| `==`, `!=` | Equality |
| `<`, `<=`, `>`, `>=` | Comparison |
| `<<`, `>>` | Shifts |
| `+`, `-` | Addition, subtraction |
| `*`, `/`, `%` | Multiplication, division, remainder |
| Unary `+`, `-`, `!`, `~` | Prefix operations |
| `**` | Exponentiation, right associative |
| Calls, casts, indexing | Primary expression operations |

Use parentheses when the distinction matters to a reader. Canonical export adds parentheses to preserve the parsed expression tree. Array literals use braces. Inclusive integer ranges use `[start:end]` or `[start:step:end]`; zero steps are rejected.

## Widths and conversions

`int`, `uint`, `bit`, and `angle` widths are checked in 1 through 64. Supported floats are binary32 and binary64. Bare integer literals use signed 64-bit arithmetic; floating literals use binary64. `pi` and `π` are floating constants. An integer expression `1 / 2` is zero, so `pi * (1 / 2)` is zero too. Use a floating operand or an explicit cast when floating division is intended.

```rust
{{#include ../../../crates/quest-circuit/tests/tutorials.rs:numeric_rules}}
```

Signed arithmetic overflow is an error. Unsigned arithmetic wraps at its width. Integer narrowing retains the target-width low bits; an `int[8]` conversion of 128 is -128. These rules apply consistently at constant evaluation and execution. Invalid division, shifts, nonfinite results, and out-of-bounds indices are diagnosed rather than delegated to Rust overflow settings.

Implicit conversions cover the standard `bool`, `int`, `uint`, and `float` categories, same-type copies, angle precision changes, and float-to-angle conversion. Bit and angle conversions to other categories require explicit casts; not every explicit category pair is valid. A measurement bit needs `bool(result)` in a condition. An explicit bit-to-integer reinterpretation requires equal widths before any subsequent numeric widening. Angles cannot be cast to integers or floats, and bits cannot be cast to floats. Gate-argument interpretation is a separate conversion, so a stored angle can still supply a gate parameter in radians. These restrictions follow the [OpenQASM 3.1 allowed-casts table](https://openqasm.com/versions/3.1/language/types.html#allowed-casts).

An `angle[w]` is a fixed-width modular fraction of one turn. Float-to-angle conversion reduces modulo a turn and rounds to the nearest representable angle with ties to even; reducing an existing angle's precision discards low bits. Angle arithmetic follows its own units and wrapping rules. A gate parameter is interpreted as an unwrapped real angle in radians, so storing an expression in `angle` first can change its meaning.

## OpenQASM 3.1 U migration

The registry uses the OpenQASM 3.1 phase convention for `U(θ, φ, λ)`: it is the conventional Euler U matrix multiplied by `exp(i θ/2)`. Its chronological decomposition is `gphase((θ+φ+λ)/2)`, `rz(λ)`, `ry(θ)`, `rz(φ)`. An implementation using only the three rotations loses the required scalar.

Global phase is retained. For example, `Rz(2π) = -I`; it is not the identity when the operation is controlled. Migration tests must compare full complex amplitudes or matrices, including phase, rather than measurement probabilities alone. Ideal `Angle::pi(n,d)` retains exact rational information for proofs; language `pi` does not silently acquire that privilege.
