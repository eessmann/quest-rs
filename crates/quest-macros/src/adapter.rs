//! Rust token trees become shared grammar tokens, never reconstructed QASM text.
use proc_macro2::{Delimiter, Spacing, Span, TokenStream, TokenTree};
use quest_language::{
    SourceSpan,
    syntax::{ParseLimits, Token, TokenKind},
};

pub struct Adapted {
    pub tokens: Vec<Token>,
    pub captures: Vec<syn::Expr>,
    pub rust_spans: Vec<Span>,
}
/// Adapt one token stream with a caller-supplied compiler-location mapper.
pub fn adapt(
    input: TokenStream,
    mut location: impl FnMut(Span) -> Option<SourceSpan>,
) -> syn::Result<Adapted> {
    let mut output = Adapted {
        tokens: Vec::new(),
        captures: Vec::new(),
        rust_spans: Vec::new(),
    };
    walk(input, &mut output, &mut location, 0)?;
    Ok(output)
}
fn push(
    output: &mut Adapted,
    kind: TokenKind,
    span: Span,
    location: &mut impl FnMut(Span) -> Option<SourceSpan>,
) -> syn::Result<()> {
    if output.tokens.len() >= ParseLimits::default().tokens {
        return Err(syn::Error::new(span, "macro token budget exceeded"));
    }
    output
        .tokens
        .try_reserve(1)
        .map_err(|_| syn::Error::new(span, "macro token allocation failed"))?;
    output
        .rust_spans
        .try_reserve(1)
        .map_err(|_| syn::Error::new(span, "macro source allocation failed"))?;
    output.tokens.push(Token {
        kind,
        span: location(span),
    });
    output.rust_spans.push(span);
    Ok(())
}
fn walk(
    input: TokenStream,
    output: &mut Adapted,
    location: &mut impl FnMut(Span) -> Option<SourceSpan>,
    depth: usize,
) -> syn::Result<()> {
    if depth >= ParseLimits::default().nesting {
        return Err(syn::Error::new(
            Span::call_site(),
            "macro nesting budget exceeded",
        ));
    }
    let mut trees = input.into_iter().peekable();
    while let Some(tree) = trees.next() {
        let span = tree.span();
        match tree {
            TokenTree::Ident(name) => push(
                output,
                TokenKind::Identifier(name.to_string()),
                span,
                location,
            )?,
            TokenTree::Literal(literal) => push(output, literal_kind(literal)?, span, location)?,
            TokenTree::Group(group) => {
                let delimiters = match group.delimiter() {
                    Delimiter::Parenthesis => Some(("(", ")")),
                    Delimiter::Bracket => Some(("[", "]")),
                    Delimiter::Brace => Some(("{", "}")),
                    Delimiter::None => None,
                };
                if let Some((open, _)) = delimiters {
                    push(
                        output,
                        TokenKind::Symbol(open.into()),
                        group.span_open(),
                        location,
                    )?;
                }
                walk(
                    group.stream(),
                    output,
                    location,
                    depth
                        .checked_add(1)
                        .ok_or_else(|| syn::Error::new(span, "macro depth overflow"))?,
                )?;
                if let Some((_, close)) = delimiters {
                    push(
                        output,
                        TokenKind::Symbol(close.into()),
                        group.span_close(),
                        location,
                    )?;
                }
            }
            TokenTree::Punct(punctuation) if punctuation.as_char() == '$' => {
                let Some(TokenTree::Group(group)) = trees.next() else {
                    return Err(syn::Error::new(
                        span,
                        "Rust captures require a dollar sign and a braced expression; hardware addresses are unsupported",
                    ));
                };
                if group.delimiter() != Delimiter::Brace {
                    return Err(syn::Error::new(
                        group.span(),
                        "Rust captures require braces",
                    ));
                }
                let expression = syn::parse2::<syn::Expr>(group.stream())?;
                let index = output.captures.len();
                output
                    .captures
                    .try_reserve(1)
                    .map_err(|_| syn::Error::new(span, "macro capture allocation failed"))?;
                output.captures.push(expression);
                push(output, TokenKind::Capture(index), span, location)?;
            }
            TokenTree::Punct(punctuation) => {
                let mut text = punctuation.as_char().to_string();
                let mut joint = punctuation.spacing() == Spacing::Joint;
                while joint {
                    let Some(TokenTree::Punct(next)) = trees.peek() else {
                        break;
                    };
                    let candidate = format!("{text}{}", next.as_char());
                    if ![
                        "==", "!=", "<=", ">=", "&&", "||", "<<", ">>", "<<=", ">>=", "**", "->",
                        "+=", "-=", "*=", "/=", "%=", "&=", "|=", "^=", "++",
                    ]
                    .contains(&candidate.as_str())
                    {
                        break;
                    }
                    joint = next.spacing() == Spacing::Joint;
                    text = candidate;
                    trees.next();
                }
                push(output, TokenKind::Symbol(text), span, location)?;
            }
        }
    }
    Ok(())
}
fn literal_kind(literal: proc_macro2::Literal) -> syn::Result<TokenKind> {
    let spelling = literal.to_string();
    let span = literal.span();
    match syn::parse2::<syn::Lit>(TokenStream::from(TokenTree::Literal(literal)))? {
        syn::Lit::Int(number) if number.suffix().is_empty() => Ok(TokenKind::Number(spelling)),
        syn::Lit::Float(number) if number.suffix().is_empty() => Ok(TokenKind::Number(spelling)),
        syn::Lit::Str(string) => Ok(TokenKind::String(string.value())),
        _ => Err(syn::Error::new(
            span,
            "use QASM numeric/string literals; Rust values require ${expression}",
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::adapt;
    use googletest::prelude::*;
    use quest_language::syntax::{ParseLimits, StatementKind, TokenKind, parse_tokens};
    use quote::quote;

    #[gtest]
    fn shared_grammar_retains_control_and_capture_order() -> Result<()> {
        let adapted = adapt(
            quote!(qubit q; gate f(a) q { rx(a) q; } while (true) { f(${first()}) q; ry(${second()}) q; break; }),
            |_| None,
        )?;
        expect_eq!(adapted.captures.len(), 2);
        let ids = adapted
            .tokens
            .iter()
            .filter_map(|token| {
                if let TokenKind::Capture(id) = token.kind {
                    Some(id)
                } else {
                    None
                }
            })
            .collect::<Vec<_>>();
        expect_eq!(ids, vec![0, 1]);
        let module = parse_tokens(&adapted.tokens, ParseLimits::default())?;
        expect_true!(matches!(
            module.statements[1].kind,
            StatementKind::GateDeclaration { .. }
        ));
        expect_true!(matches!(
            module.statements[2].kind,
            StatementKind::While { .. }
        ));
        Ok(())
    }
    #[gtest]
    fn token_punctuation_preserves_operators_and_string_literals() -> Result<()> {
        let adapted = adapt(
            quote!(bit[2] b = "01"; bool less = 1 <= 2 && 3 != 4;),
            |_| None,
        )?;
        let spellings = adapted
            .tokens
            .iter()
            .map(|token| token.kind.spelling())
            .collect::<Vec<_>>();
        expect_true!(spellings.contains(&"<="));
        expect_true!(spellings.contains(&"&&"));
        expect_true!(spellings.contains(&"!="));
        expect_true!(
            adapted
                .tokens
                .iter()
                .any(|token| token.kind == TokenKind::String("01".into()))
        );
        expect_true!(parse_tokens(&adapted.tokens, ParseLimits::default()).is_ok());
        Ok(())
    }
}
