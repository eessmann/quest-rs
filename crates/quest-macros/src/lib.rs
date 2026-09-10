#![forbid(unsafe_code)]
#![feature(proc_macro_span, proc_macro_expand)]
//! Token-tree frontend for the documented OpenQASM-style Rust circuit profile.

use num_traits::ToPrimitive;
use proc_macro::TokenStream;
use proc_macro2::{Ident, Span, TokenStream as Tokens};
use quote::{quote, quote_spanned};
use std::collections::BTreeMap;
use syn::{
    Expr, Lit, LitInt, Token, braced, bracketed, parenthesized,
    parse::{Parse, ParseStream},
    spanned::Spanned,
};

#[proc_macro]
pub fn circuit(input: TokenStream) -> TokenStream {
    frontend::expand(input.into())
        .unwrap_or_else(syn::Error::into_compile_error)
        .into()
}

/// Compile a compiler-tracked `OpenQASM` source file.
#[proc_macro]
pub fn circuit_file(input: TokenStream) -> TokenStream {
    frontend::file(input.into())
        .unwrap_or_else(syn::Error::into_compile_error)
        .into()
}

/// Previous static builder DSL, retaining its ideal rational-pi expressions.
#[proc_macro]
pub fn legacy_circuit(input: TokenStream) -> TokenStream {
    match syn::parse::<Circuit>(input).and_then(Circuit::expand) {
        Ok(tokens) => tokens.into(),
        Err(error) => error.to_compile_error().into(),
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum WireKind {
    Qubit,
    Bit,
}
struct Wire {
    kind: WireKind,
    offset: usize,
    length: usize,
    array: bool,
}
struct Circuit {
    wires: BTreeMap<String, Wire>,
    qubits: usize,
    bits: usize,
    statements: Vec<Statement>,
}
struct Operand {
    name: Ident,
    index: usize,
    indexed: bool,
}
enum AngleValue {
    Exact { numerator: i64, denominator: i64 },
    Radians(f64),
    Runtime(Box<Expr>),
}
enum Statement {
    Gate {
        name: Ident,
        angles: Vec<AngleValue>,
        operands: Vec<Operand>,
        controls: Vec<bool>,
        inverse: bool,
    },
    Measure {
        span: Span,
        qubit: Operand,
        bit: Operand,
    },
    Reset {
        span: Span,
        qubit: Operand,
    },
    Barrier {
        span: Span,
        operands: Vec<Operand>,
    },
}

impl Parse for Operand {
    fn parse(input: ParseStream) -> syn::Result<Self> {
        let name = input.parse::<Ident>()?;
        let indexed = input.peek(syn::token::Bracket);
        let index = if indexed {
            let content;
            bracketed!(content in input);
            let value = content.parse::<LitInt>()?.base10_parse::<usize>()?;
            if !content.is_empty() {
                return Err(content.error("circuit wire indices must be integer literals"));
            }
            value
        } else {
            0
        };
        Ok(Self {
            name,
            index,
            indexed,
        })
    }
}
fn operands(input: ParseStream) -> syn::Result<Vec<Operand>> {
    let mut out = vec![];
    while !input.peek(Token![;]) {
        out.push(input.parse()?);
        if input.peek(Token![,]) {
            input.parse::<Token![,]>()?;
        } else {
            break;
        }
    }
    input.parse::<Token![;]>()?;
    Ok(out)
}

impl Parse for Circuit {
    fn parse(input: ParseStream) -> syn::Result<Self> {
        let mut circuit = Self {
            wires: BTreeMap::new(),
            qubits: 0,
            bits: 0,
            statements: vec![],
        };
        while !input.is_empty() {
            let name = input.parse::<Ident>()?;
            let keyword = name.to_string();
            if keyword == "qubit" || keyword == "bit" {
                circuit.parse_declaration(input, &name, &keyword)?;
                continue;
            }
            if keyword == "reset" {
                let q = input.parse()?;
                input.parse::<Token![;]>()?;
                circuit.statements.push(Statement::Reset {
                    span: name.span(),
                    qubit: q,
                });
                continue;
            }
            if keyword == "barrier" {
                circuit.statements.push(Statement::Barrier {
                    span: name.span(),
                    operands: operands(input)?,
                });
                continue;
            }
            if keyword == "measure" {
                let qubit = input.parse()?;
                input.parse::<Token![->]>()?;
                let bit = input.parse()?;
                input.parse::<Token![;]>()?;
                circuit.statements.push(Statement::Measure {
                    span: name.span(),
                    qubit,
                    bit,
                });
                continue;
            }
            if circuit
                .wires
                .get(&keyword)
                .is_some_and(|w| w.kind == WireKind::Bit)
            {
                let indexed = input.peek(syn::token::Bracket);
                let index = if indexed {
                    let content;
                    bracketed!(content in input);
                    let index = content.parse::<LitInt>()?.base10_parse::<usize>()?;
                    if !content.is_empty() {
                        return Err(content.error("circuit wire indices must be integer literals"));
                    }
                    index
                } else {
                    0
                };
                input.parse::<Token![=]>()?;
                let measure = input.parse::<Ident>()?;
                if measure != "measure" {
                    return Err(syn::Error::new(
                        measure.span(),
                        "only measurement assignments are supported",
                    ));
                }
                let qubit = input.parse()?;
                input.parse::<Token![;]>()?;
                circuit.statements.push(Statement::Measure {
                    span: measure.span(),
                    qubit,
                    bit: Operand {
                        name,
                        index,
                        indexed,
                    },
                });
                continue;
            }
            circuit.statements.push(parse_gate(input, name)?);
        }
        if circuit.qubits == 0 {
            return Err(input.error("a circuit requires at least one qubit declaration"));
        }
        Ok(circuit)
    }
}

impl Circuit {
    fn parse_declaration(
        &mut self,
        input: ParseStream,
        name: &Ident,
        keyword: &str,
    ) -> syn::Result<()> {
        if !self.statements.is_empty() {
            return Err(syn::Error::new(
                name.span(),
                "wire declarations must precede circuit operations",
            ));
        }
        let array = input.peek(syn::token::Bracket);
        let length = if array {
            let content;
            bracketed!(content in input);
            let n = content.parse::<LitInt>()?.base10_parse::<usize>()?;
            if !content.is_empty() {
                return Err(content.error("wire array size must be an integer literal"));
            }
            n
        } else {
            1
        };
        if length == 0 {
            return Err(syn::Error::new(name.span(), "wire arrays must be nonempty"));
        }
        let wire_name = input.parse::<Ident>()?;
        input.parse::<Token![;]>()?;
        let kind = if keyword == "qubit" {
            WireKind::Qubit
        } else {
            WireKind::Bit
        };
        let total = if kind == WireKind::Qubit {
            &mut self.qubits
        } else {
            &mut self.bits
        };
        let offset = *total;
        *total = total
            .checked_add(length)
            .ok_or_else(|| syn::Error::new(wire_name.span(), "wire count overflows usize"))?;
        if self
            .wires
            .insert(
                wire_name.to_string(),
                Wire {
                    kind,
                    offset,
                    length,
                    array,
                },
            )
            .is_some()
        {
            return Err(syn::Error::new(
                wire_name.span(),
                "wire name is already declared",
            ));
        }

        Ok(())
    }
}

fn parse_gate(input: ParseStream, name: Ident) -> syn::Result<Statement> {
    let mut gate_name = name;
    let mut controls = vec![];
    let mut inverse = false;
    loop {
        match gate_name.to_string().as_str() {
            "inv" => {
                inverse = !inverse;
                input.parse::<Token![@]>()?;
                gate_name = input.parse()?;
            }
            "ctrl" | "negctrl" => {
                let state = gate_name == "ctrl";
                let count = if input.peek(syn::token::Paren) {
                    let content;
                    parenthesized!(content in input);
                    let count = content.parse::<LitInt>()?.base10_parse::<usize>()?;
                    if !content.is_empty() {
                        return Err(content.error("control count must be an integer literal"));
                    }
                    count
                } else {
                    1
                };
                if count == 0 || count > 1024 {
                    return Err(syn::Error::new(
                        gate_name.span(),
                        "control count must be in 1..=1024",
                    ));
                }
                controls.extend(std::iter::repeat_n(state, count));
                input.parse::<Token![@]>()?;
                gate_name = input.parse()?;
            }
            _ => break,
        }
    }
    let mut angles = vec![];
    if input.peek(syn::token::Paren) {
        let content;
        parenthesized!(content in input);
        while !content.is_empty() {
            angles.push(parse_angle(&content)?);
            if content.is_empty() {
                break;
            }
            content.parse::<Token![,]>()?;
        }
    }
    Ok(Statement::Gate {
        name: gate_name,
        angles,
        operands: operands(input)?,
        controls,
        inverse,
    })
}

fn parse_angle(input: ParseStream) -> syn::Result<AngleValue> {
    if input.peek(Token![$]) {
        input.parse::<Token![$]>()?;
        let content;
        braced!(content in input);
        let expression = content.parse::<Expr>()?;
        if !content.is_empty() {
            return Err(content.error("expected one Rust expression inside ${...}"));
        }
        return Ok(AngleValue::Runtime(Box::new(expression)));
    }
    let expression = input.parse::<Expr>()?;
    if let Some(value) = float_literal(&expression) {
        let value = value?;
        if !value.is_finite() {
            return Err(syn::Error::new(
                expression.span(),
                "angle literal must be finite",
            ));
        }
        return Ok(AngleValue::Radians(value));
    }
    let (numerator, denominator, power) = exact(&expression)?;
    if power == 0 {
        return Ok(AngleValue::Radians(
            numerator
                .to_f64()
                .ok_or_else(|| input.error("numerator is not representable"))?
                / denominator
                    .to_f64()
                    .ok_or_else(|| input.error("denominator is not representable"))?,
        ));
    }
    if power != 1 {
        return Err(syn::Error::new(
            expression.span(),
            "an exact angle must be a rational multiple of pi",
        ));
    }
    let numerator=i64::try_from(numerator).map_err(|_|syn::Error::new(expression.span(),"exact pi numerator exceeds the macro's signed 64-bit literal range; use the builder for larger rationals"))?;
    let denominator=i64::try_from(denominator).map_err(|_|syn::Error::new(expression.span(),"exact pi denominator exceeds the macro's signed 64-bit literal range; use the builder for larger rationals"))?;
    Ok(AngleValue::Exact {
        numerator,
        denominator,
    })
}
fn float_literal(expression: &Expr) -> Option<syn::Result<f64>> {
    match expression {
        Expr::Lit(x) => {
            if let Lit::Float(x) = &x.lit {
                Some(x.base10_parse::<f64>())
            } else {
                None
            }
        }
        Expr::Unary(x) if matches!(x.op, syn::UnOp::Neg(_)) => {
            float_literal(&x.expr).map(|r| r.map(|x| -x))
        }
        Expr::Paren(x) => float_literal(&x.expr),
        _ => None,
    }
}
// Checked rational arithmetic is performed on the AST, never on stringified
// token streams. Unsupported expressions require explicit Rust interpolation.
fn exact(expression: &Expr) -> syn::Result<(i128, i128, i8)> {
    let error = || {
        syn::Error::new(
            expression.span(),
            "expected a rational pi angle or finite literal; use ${...} for a Rust expression",
        )
    };
    let overflow = || syn::Error::new(expression.span(), "exact angle arithmetic overflow");
    let normalized = |n: i128, d: i128, p: i8| -> syn::Result<(i128, i128, i8)> {
        if d == 0 {
            return Err(syn::Error::new(
                expression.span(),
                "angle denominator must be nonzero",
            ));
        }
        let mut a = n.checked_abs().ok_or_else(overflow)?;
        let mut b = d.checked_abs().ok_or_else(overflow)?;
        while b != 0 {
            let next = a.checked_rem(b).ok_or_else(overflow)?;
            a = b;
            b = next;
        }
        let gcd = a.max(1);
        let sign = if d < 0 { -1 } else { 1 };
        Ok((
            n.checked_div(gcd)
                .ok_or_else(overflow)?
                .checked_mul(sign)
                .ok_or_else(overflow)?,
            d.checked_div(gcd)
                .ok_or_else(overflow)?
                .checked_mul(sign)
                .ok_or_else(overflow)?,
            p,
        ))
    };
    match expression {
        Expr::Path(x) if x.path.is_ident("pi") => Ok((1, 1, 1)),
        Expr::Lit(x) => {
            if let Lit::Int(x) = &x.lit {
                Ok((x.base10_parse()?, 1, 0))
            } else {
                Err(error())
            }
        }
        Expr::Paren(x) => exact(&x.expr),
        Expr::Group(x) => exact(&x.expr),
        Expr::Unary(x) if matches!(x.op, syn::UnOp::Neg(_)) => {
            let (n, d, p) = exact(&x.expr)?;
            Ok((n.checked_neg().ok_or_else(overflow)?, d, p))
        }
        Expr::Binary(binary) => {
            let (left_numerator, left_denominator, left_power) = exact(&binary.left)?;
            let (right_numerator, right_denominator, right_power) = exact(&binary.right)?;
            match binary.op {
                syn::BinOp::Mul(_) => normalized(
                    left_numerator
                        .checked_mul(right_numerator)
                        .ok_or_else(overflow)?,
                    left_denominator
                        .checked_mul(right_denominator)
                        .ok_or_else(overflow)?,
                    left_power.checked_add(right_power).ok_or_else(overflow)?,
                ),
                syn::BinOp::Div(_) => normalized(
                    left_numerator
                        .checked_mul(right_denominator)
                        .ok_or_else(overflow)?,
                    left_denominator
                        .checked_mul(right_numerator)
                        .ok_or_else(overflow)?,
                    left_power.checked_sub(right_power).ok_or_else(overflow)?,
                ),
                syn::BinOp::Add(_) | syn::BinOp::Sub(_) if left_power == right_power => {
                    let left = left_numerator
                        .checked_mul(right_denominator)
                        .ok_or_else(overflow)?;
                    let right = right_numerator
                        .checked_mul(left_denominator)
                        .ok_or_else(overflow)?;
                    let numerator = if matches!(binary.op, syn::BinOp::Add(_)) {
                        left.checked_add(right)
                    } else {
                        left.checked_sub(right)
                    }
                    .ok_or_else(overflow)?;
                    normalized(
                        numerator,
                        left_denominator
                            .checked_mul(right_denominator)
                            .ok_or_else(overflow)?,
                        left_power,
                    )
                }
                _ => Err(error()),
            }
        }
        _ => Err(error()),
    }
}

fn root_path() -> syn::Result<Tokens> {
    use proc_macro_crate::{FoundCrate, crate_name};
    for (package, lib) in [("quest-circuit", "quest_circuit"), ("quest-rs", "quest")] {
        if let Ok(found) = crate_name(package) {
            let name = match found {
                FoundCrate::Itself => lib.to_owned(),
                FoundCrate::Name(name) => name,
            };
            let name = Ident::new(&name.replace('-', "_"), Span::call_site());
            return Ok(quote!(::#name));
        }
    }
    Err(syn::Error::new(
        Span::call_site(),
        "circuit! requires a quest-circuit or quest-rs dependency",
    ))
}

impl Circuit {
    fn resolve(&self, operand: &Operand, kind: WireKind) -> syn::Result<usize> {
        let wire = self
            .wires
            .get(&operand.name.to_string())
            .ok_or_else(|| syn::Error::new(operand.name.span(), "unknown circuit wire"))?;
        if wire.kind != kind {
            return Err(syn::Error::new(
                operand.name.span(),
                if kind == WireKind::Qubit {
                    "expected a qubit operand"
                } else {
                    "expected a classical bit operand"
                },
            ));
        }
        if wire.array && !operand.indexed {
            return Err(syn::Error::new(
                operand.name.span(),
                "array operands require an explicit index; broadcast operations are not supported",
            ));
        }
        if operand.index >= wire.length {
            return Err(syn::Error::new(
                operand.name.span(),
                "circuit wire index is out of range",
            ));
        }
        wire.offset
            .checked_add(operand.index)
            .ok_or_else(|| syn::Error::new(operand.name.span(), "wire offset overflows usize"))
    }
    fn expand(self) -> syn::Result<Tokens> {
        let root = root_path()?;
        let builder = Ident::new("__quest_builder", Span::mixed_site());
        let mut emitted = vec![];
        for statement in &self.statements {
            let span = match statement {
                Statement::Gate { name, .. } => name.span(),
                Statement::Measure { span, .. }
                | Statement::Reset { span, .. }
                | Statement::Barrier { span, .. } => *span,
            };
            // Only the compiler provides source locations; no source file is read.
            let compiler_span = span.unwrap();
            let file = compiler_span.file();
            let range = compiler_span.byte_range();
            let (start, end) = (range.start, range.end);
            emitted.push(quote_spanned!(span=>
                #builder.set_source(::core::option::Option::Some(
                    #root::SourceSpan::new(#file, #start, #end)?
                ));
            ));
            match statement {
                Statement::Measure { qubit, bit, .. } => {
                    let q = self.resolve(qubit, WireKind::Qubit)?;
                    let b = self.resolve(bit, WireKind::Bit)?;
                    emitted.push(quote_spanned!(qubit.name.span()=>#builder.measure(#builder.qubit(#q)?,#builder.bit(#b)?)?;));
                }
                Statement::Reset { qubit, .. } => {
                    let q = self.resolve(qubit, WireKind::Qubit)?;
                    emitted.push(
                        quote_spanned!(qubit.name.span()=>#builder.reset(#builder.qubit(#q)?)?;),
                    );
                }
                Statement::Barrier { operands, .. } => {
                    let qs = operands
                        .iter()
                        .map(|q| self.resolve(q, WireKind::Qubit))
                        .collect::<syn::Result<Vec<_>>>()?;
                    emitted.push(quote!(#builder.barrier(&[#(#builder.qubit(#qs)?),*])?;));
                }
                Statement::Gate { .. } => {
                    emitted.push(self.expand_gate(&root, &builder, statement)?);
                }
            }
        }
        let qubits = self.qubits;
        let bits = self.bits;
        Ok(
            quote!((|| -> ::core::result::Result<#root::ValidatedProgram,#root::CircuitError> {
            let mut #builder=#root::ProgramBuilder::new(#qubits,#bits)?;
            #(#emitted)*#builder.finish()
        })()),
        )
    }
}

impl Circuit {
    fn expand_gate(
        &self,
        root: &Tokens,
        builder: &Ident,
        statement: &Statement,
    ) -> syn::Result<Tokens> {
        let Statement::Gate {
            name,
            angles,
            operands,
            controls,
            inverse,
        } = statement
        else {
            return Err(syn::Error::new(
                Span::call_site(),
                "internal gate emission mismatch",
            ));
        };

        let (variant, angle_count, intrinsic) =
            check_signature(name, angles.len(), operands.len(), controls.len())?;
        let indices = operands
            .iter()
            .map(|q| self.resolve(q, WireKind::Qubit))
            .collect::<syn::Result<Vec<_>>>()?;
        let mut unique = std::collections::BTreeSet::new();
        for (operand, index) in operands.iter().zip(&indices) {
            if !unique.insert(index) {
                return Err(syn::Error::new(
                    operand.name.span(),
                    "duplicate target/control operand",
                ));
            }
        }
        let mut states = controls.clone();
        states.extend(std::iter::repeat_n(true, intrinsic));
        let control_tokens = states
            .iter()
            .zip(&indices)
            .map(|(state, index)| {
                let state = if *state { quote!(One) } else { quote!(Zero) };
                quote!(#root::Control::new(#builder.qubit(#index)?,#root::ControlState::#state))
            })
            .collect::<Vec<_>>();
        let targets = indices
            .get(states.len()..)
            .ok_or_else(|| syn::Error::new(name.span(), "control arity exceeds operands"))?;
        let angle_tokens = angles
            .iter()
            .map(|a| match a {
                AngleValue::Exact {
                    numerator,
                    denominator,
                } => quote!(#root::Angle::pi(#numerator,#denominator)?),
                AngleValue::Radians(x) => quote!(#root::Angle::radians(#x)?),
                AngleValue::Runtime(x) => quote!(#root::Angle::radians((#x) as f64)?),
            })
            .collect::<Vec<_>>();
        if variant == "GlobalPhase" {
            let a = angle_tokens
                .first()
                .ok_or_else(|| syn::Error::new(name.span(), "missing gate angle"))?;
            let a = if *inverse {
                quote!((#a).negated())
            } else {
                quote!(#a)
            };
            Ok(quote_spanned!(name.span()=>#builder.global_phase(#a,&[#(#control_tokens),*])?;))
        } else {
            let variant = Ident::new(variant, name.span());
            let gate = if variant == "U" {
                let t = angle_tokens
                    .first()
                    .ok_or_else(|| syn::Error::new(name.span(), "missing gate angle"))?;
                let p = angle_tokens
                    .get(1)
                    .ok_or_else(|| syn::Error::new(name.span(), "missing gate angle"))?;
                let l = angle_tokens
                    .get(2)
                    .ok_or_else(|| syn::Error::new(name.span(), "missing gate angle"))?;
                quote!(#root::Gate::U{theta:#t,phi:#p,lambda:#l})
            } else if angle_count == 1 {
                let a = angle_tokens
                    .first()
                    .ok_or_else(|| syn::Error::new(name.span(), "missing gate angle"))?;
                quote!(#root::Gate::#variant(#a))
            } else {
                quote!(#root::Gate::#variant)
            };
            let gate = if *inverse {
                quote!((#gate).adjoint())
            } else {
                gate
            };
            Ok(
                quote_spanned!(name.span()=>#builder.gate(#gate,&[#(#builder.qubit(#targets)?),*],&[#(#control_tokens),*])?;),
            )
        }
    }
}

fn gate_spec(name: &Ident) -> syn::Result<(&'static str, usize, usize, usize)> {
    let kind = quest_language::GateKind::lookup(&name.to_string()).ok_or_else(|| {
        syn::Error::new(
            name.span(),
            "unsupported circuit construct or gate in the initial DSL profile",
        )
    })?;
    let gate = kind.definition();
    Ok((
        gate.rust_variant,
        gate.target_count,
        gate.parameter_count,
        gate.intrinsic_controls,
    ))
}

fn check_signature(
    name: &Ident,
    actual_angles: usize,
    operand_count: usize,
    control_count: usize,
) -> syn::Result<(&'static str, usize, usize)> {
    let (variant, arity, angle_count, intrinsic) = gate_spec(name)?;
    let expected = arity
        .checked_add(control_count)
        .and_then(|count| count.checked_add(intrinsic))
        .ok_or_else(|| syn::Error::new(name.span(), "gate arity overflows usize"))?;
    if operand_count != expected {
        return Err(syn::Error::new(
            name.span(),
            format!(
                "{name} expects {expected} qubit operands with these modifiers, received {operand_count}"
            ),
        ));
    }
    if actual_angles != angle_count {
        return Err(syn::Error::new(
            name.span(),
            format!("{name} expects {angle_count} angle arguments, received {actual_angles}"),
        ));
    }

    Ok((variant, angle_count, intrinsic))
}

mod adapter;
mod frontend;
