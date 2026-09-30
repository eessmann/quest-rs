#![forbid(unsafe_code)]
#![feature(proc_macro_span, proc_macro_expand)]
//! One token frontend for the shared checked program pipeline.
use proc_macro::TokenStream;
use proc_macro2::{Ident, Span, TokenStream as Tokens};
use quote::quote;
#[proc_macro]
pub fn circuit(input: TokenStream) -> TokenStream {
    frontend::expand(input.into())
        .unwrap_or_else(syn::Error::into_compile_error)
        .into()
}
#[proc_macro]
pub fn circuit_file(input: TokenStream) -> TokenStream {
    frontend::file(input.into())
        .unwrap_or_else(syn::Error::into_compile_error)
        .into()
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

mod adapter;
mod frontend;
