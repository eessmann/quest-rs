//! Shared syntax/checker entrypoints and compiler-tracked include expansion.
use crate::{adapter, root_path};
use proc_macro2::{Span, TokenStream};
use quest_language::{
    SourceId, SourceMap, SourceSnapshot, SourceSpan,
    semantic::CompileLimits,
    syntax::{self, ParseLimits, Token, TokenKind},
};
use quote::{quote, quote_spanned};
use std::{
    collections::BTreeMap,
    path::{Component, Path, PathBuf},
};

struct Location {
    span: SourceSpan,
    file: String,
    line: usize,
    column: usize,
}
struct Frontend {
    tokens: Vec<Token>,
    rust_spans: Vec<Span>,
    captures: Vec<syn::Expr>,
    locations: Vec<Location>,
    sources: SourceMap,
    next_source: u64,
    source_bytes: usize,
    stack: Vec<PathBuf>,
}
impl Frontend {
    fn new() -> Self {
        Self {
            tokens: Vec::new(),
            rust_spans: Vec::new(),
            captures: Vec::new(),
            locations: Vec::new(),
            sources: SourceMap::default(),
            next_source: 1,
            source_bytes: 0,
            stack: Vec::new(),
        }
    }
    fn source_id(&mut self, span: Span) -> syn::Result<SourceId> {
        let id = self.next_source;
        self.next_source = id
            .checked_add(1)
            .ok_or_else(|| syn::Error::new(span, "source identity exhausted"))?;
        Ok(SourceId::new(id))
    }
    fn expand_tokens(
        &mut self,
        tokens: Vec<Token>,
        spans: Vec<Span>,
        parent: Option<&Path>,
    ) -> syn::Result<()> {
        let mut iter = tokens.into_iter().zip(spans);
        let mut braces = 0usize;
        while let Some((token, span)) = iter.next() {
            if token.kind.spelling() == "include" {
                if braces != 0 {
                    return Err(syn::Error::new(span, "includes require module scope"));
                }
                let Some((
                    Token {
                        kind: TokenKind::String(path),
                        ..
                    },
                    _,
                )) = iter.next()
                else {
                    return Err(syn::Error::new(span, "include requires a quoted path"));
                };
                if iter
                    .next()
                    .is_none_or(|(token, _)| token.kind.spelling() != ";")
                {
                    return Err(syn::Error::new(span, "include requires a semicolon"));
                }
                self.include(&path, parent, span)?;
            } else {
                match token.kind.spelling() {
                    "{" => {
                        braces = braces
                            .checked_add(1)
                            .ok_or_else(|| syn::Error::new(span, "include nesting overflow"))?;
                    }
                    "}" => braces = braces.saturating_sub(1),
                    _ => {}
                }
                if self.tokens.len() >= ParseLimits::default().tokens {
                    return Err(syn::Error::new(span, "expanded token budget exceeded"));
                }
                self.tokens
                    .try_reserve(1)
                    .map_err(|_| syn::Error::new(span, "token allocation failed"))?;
                self.rust_spans
                    .try_reserve(1)
                    .map_err(|_| syn::Error::new(span, "source allocation failed"))?;
                self.tokens.push(token);
                self.rust_spans.push(span);
            }
        }
        Ok(())
    }
    fn include(&mut self, name: &str, parent: Option<&Path>, span: Span) -> syn::Result<()> {
        if self.stack.len() >= 64 {
            return Err(syn::Error::new(span, "include depth budget exceeded"));
        }
        let path = normalize(&parent.map_or_else(|| PathBuf::from(name), |p| p.join(name)));
        if self.stack.contains(&path) {
            return Err(syn::Error::new(span, "include cycle detected"));
        }
        let text = if name == "stdgates.inc" {
            quest_qasm::STANDARD_GATES.to_owned()
        } else {
            let path_text = path
                .to_str()
                .ok_or_else(|| syn::Error::new(span, "include path must be UTF-8"))?;
            let literal = syn::LitStr::new(path_text, span);
            let stream: proc_macro::TokenStream =
                quote_spanned!(span=> include_str!(#literal)).into();
            let expanded = stream.expand_expr().map_err(|error| {
                syn::Error::new(span, format!("compiler include failed: {error}"))
            })?;
            syn::parse::<syn::LitStr>(expanded)?.value()
        };
        self.source_bytes = self
            .source_bytes
            .checked_add(text.len())
            .ok_or_else(|| syn::Error::new(span, "source byte overflow"))?;
        if self.source_bytes > ParseLimits::default().source_bytes {
            return Err(syn::Error::new(
                span,
                "included source byte budget exceeded",
            ));
        }
        let id = self.source_id(span)?;
        let source = SourceSnapshot::new(id, path.to_string_lossy().into_owned(), text);
        let tokens = syntax::lex(&source, ParseLimits::default())
            .map_err(|error| syn::Error::new(span, error.to_string()))?;
        self.sources
            .insert(source)
            .map_err(|error| syn::Error::new(span, error.to_string()))?;
        let spans = vec![span; tokens.len()];
        self.stack.push(path.clone());
        self.expand_tokens(tokens, spans, path.parent())?;
        self.stack.pop();
        Ok(())
    }
    fn error(&self, span: Option<SourceSpan>, message: impl std::fmt::Display) -> syn::Error {
        let rust = self
            .tokens
            .iter()
            .zip(&self.rust_spans)
            .find(|(token, _)| span.is_some() && token.span == span)
            .map_or_else(Span::call_site, |(_, span)| *span);
        let detail = span.and_then(|span| {
            self.sources
                .get(span.source())
                .map(|s| (s.name(), span.range()))
        });
        let message = detail.map_or_else(
            || message.to_string(),
            |(file, range)| format!("{file}:{}: {message}", range.start),
        );
        syn::Error::new(rust, message)
    }
    fn emit(self) -> syn::Result<TokenStream> {
        let mut module = syntax::parse_tokens(&self.tokens, ParseLimits::default())
            .map_err(|error| self.error(error.span, error))?;
        let oracle_captures = module
            .statements
            .iter()
            .filter_map(|statement| {
                if let syntax::StatementKind::Oracle { capture, .. } = &statement.kind {
                    Some(*capture)
                } else {
                    None
                }
            })
            .collect::<std::collections::BTreeSet<_>>();
        // Oracle identities inhabit a separate typed namespace. Compact only scalar
        // captures, preserving source evaluation order without dummy floating values.
        let scalar_indices = (0..self.captures.len())
            .filter(|index| !oracle_captures.contains(index))
            .enumerate()
            .map(|(scalar, source)| (source, scalar))
            .collect::<BTreeMap<_, _>>();
        if !oracle_captures.is_empty() {
            let mut tokens = self.tokens.clone();
            for token in &mut tokens {
                if let syntax::TokenKind::Capture(index) = &mut token.kind
                    && let Some(scalar) = scalar_indices.get(index)
                {
                    *index = *scalar;
                }
            }
            module = syntax::parse_tokens(&tokens, ParseLimits::default())
                .map_err(|error| self.error(error.span, error))?;
        }
        let admitted = quest_qasm::admit_expanded(module, &self.sources, CompileLimits::default())
            .map_err(|error| {
                self.error(
                    error
                        .labels
                        .first()
                        .map(|label| label.span)
                        .or(error.occurrence),
                    error,
                )
            })?;
        let template = quest_language::semantic::template::encode(&admitted)
            .map_err(|error| self.error(error.span, error))?;
        let root = root_path()?;
        let __captures = proc_macro2::Ident::new("__captures", Span::mixed_site());
        let __oracles = proc_macro2::Ident::new("__oracles", Span::mixed_site());
        let __cache = proc_macro2::Ident::new("__QUEST_TEMPLATE", Span::mixed_site());
        let __exact = proc_macro2::Ident::new("__exact", Span::mixed_site());
        let __template = proc_macro2::Ident::new("__template", Span::mixed_site());
        let __sources = proc_macro2::Ident::new("__sources", Span::mixed_site());
        let __locations = proc_macro2::Ident::new("__locations", Span::mixed_site());
        let __value = proc_macro2::Ident::new("__value", Span::mixed_site());

        let captures = &self.captures;
        let capture_statements = captures.iter().enumerate().map(|(index, expression)| {
            if oracle_captures.contains(&index) {
                Ok(quote! {
                    #__oracles.insert(#index, { let #__value: #root::OracleFragment = #expression; #__value });
                })
            } else {
                let index = scalar_indices.get(&index).copied().ok_or_else(|| self.error(None, "missing scalar capture mapping"))?;
                Ok(quote! {
                    let #__value = #root::capture_angle(#expression)?;
                    if let Some(exact) = #__value.exact() { #__exact.insert(#index, exact.clone()); }
                    #__captures.push(#__value.scalar());
                })
            }
        }).collect::<syn::Result<Vec<_>>>()?;
        let capture_count = scalar_indices.len();
        let sources = self.sources.iter().map(|source| {
            let id = source.id().value();
            let name = source.name();
            let text = source.text();
            quote! { #__sources.insert(#root::language::SourceSnapshot::new(
            #root::language::SourceId::new(#id), #name, #text))?; }
        });
        let locations = self.locations.iter().map(|location| {
            let span = emit_span(location.span, &root); let file = &location.file;
            let line = location.line; let column = location.column;
            quote! { #__locations.push(#root::MacroLocation { span: #span, file: #file.into(), line: #line, column: #column }); }
        });
        let location_count = self.locations.len();
        Ok(quote! {
                    (|| -> ::std::result::Result<#root::Program<#root::Constructed>, #root::LanguageError> {
                        let mut #__oracles = ::std::collections::BTreeMap::new();
                        let mut #__exact = ::std::collections::BTreeMap::new();
                        let mut #__captures = ::std::vec::Vec::new();
                        #__captures.try_reserve_exact(#capture_count).map_err(|_| #root::LanguageError::Budget("captures"))?;
                        #(#capture_statements)*
        static #__cache: ::std::sync::OnceLock<::std::result::Result<#root::language::semantic::TypedModule, #root::language::semantic::SemanticError>> = ::std::sync::OnceLock::new();
        let #__template = #__cache.get_or_init(|| #root::language::semantic::template::load(
            #template, #root::language::semantic::CompileLimits::default())).clone()?;
                        let mut #__sources = #root::language::SourceMap::default();
                        #(#sources)*
                        let mut #__locations = ::std::vec::Vec::new();
                        #__locations.try_reserve_exact(#location_count).map_err(|_| #root::LanguageError::Budget("source locations"))?;
                        #(#locations)*
                        #root::Program::<#root::Constructed>::from_template(#__template, #__captures, #__sources, #__locations).with_angle_captures(#__exact).with_oracles(#__oracles)
                    })()
                })
    }
}
pub fn expand(input: TokenStream) -> syn::Result<TokenStream> {
    let mut frontend = Frontend::new();
    let mut files = BTreeMap::new();
    let adapted = adapter::adapt(input, |span| {
        let compiler = span.unwrap();
        let file = compiler.file();
        let id = if let Some(id) = files.get(&file) {
            *id
        } else {
            let id = frontend.source_id(span).ok()?;
            files.insert(file.clone(), id);
            id
        };
        let source = SourceSpan::location(id, compiler.byte_range()).ok()?;
        frontend.locations.push(Location {
            span: source,
            file,
            line: compiler.line(),
            column: compiler.column(),
        });
        Some(source)
    })?;
    frontend.captures = adapted.captures;
    frontend.expand_tokens(adapted.tokens, adapted.rust_spans, None)?;
    frontend.emit()
}
pub fn file(input: TokenStream) -> syn::Result<TokenStream> {
    let file = syn::parse2::<syn::LitStr>(input)?;
    let mut frontend = Frontend::new();
    frontend.include(&file.value(), None, file.span())?;
    frontend.emit()
}
fn emit_span(span: SourceSpan, root: &TokenStream) -> TokenStream {
    let id = span.source().value();
    let range = span.range();
    let start = range.start;
    let end = range.end;
    quote! { #root::language::SourceSpan::location(#root::language::SourceId::new(#id), #start..#end)? }
}
fn normalize(path: &Path) -> PathBuf {
    let mut normalized = PathBuf::new();
    for component in path.components() {
        match component {
            Component::CurDir => {}
            Component::ParentDir if normalized.file_name().is_some_and(|name| name != "..") => {
                normalized.pop();
            }
            _ => normalized.push(component.as_os_str()),
        }
    }
    normalized
}
