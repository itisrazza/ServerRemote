//! Proc macros for referring to a Bootstrap Icon by its kebab-case name,
//! e.g. `bi!(gear-wide-connected)`, instead of spelling out the PascalCase
//! `Icons` enum variant by hand.
//!
//! These are re-exported (not meant to be depended on directly) as
//! `bootstrap_icons::bi!` and `bootstrap_icons::iced::bi!`.

use proc_macro::TokenStream;
use proc_macro2::{Span, TokenTree};
use quote::{quote, quote_spanned};

/// Reconstructs the kebab-case icon name from a token stream like
/// `gear - wide - connected` (how `gear-wide-connected` tokenizes).
fn kebab_name(input: proc_macro2::TokenStream) -> Result<String, proc_macro2::TokenStream> {
    let mut segments = Vec::new();
    for token in input {
        match token {
            TokenTree::Ident(ident) => segments.push(ident.to_string()),
            TokenTree::Literal(literal) => segments.push(literal.to_string()),
            TokenTree::Punct(punct) if punct.as_char() == '-' => {}
            other => {
                let message = format!("unexpected token `{other}` in Bootstrap Icon name");
                return Err(quote_spanned! { other.span() => compile_error!(#message); });
            }
        }
    }
    Ok(segments.join("-"))
}

/// Resolves a token stream naming an icon to a fully-qualified path
/// expression pointing at its `Icons` enum variant.
fn icon_path(input: TokenStream) -> Result<proc_macro2::TokenStream, proc_macro2::TokenStream> {
    let kebab = kebab_name(input.into())?;
    let ident = proc_macro2::Ident::new(
        &bootstrap_icons_codegen::to_pascal_ident(&kebab),
        Span::call_site(),
    );
    Ok(quote! { ::bootstrap_icons::Icons::#ident })
}

/// Expands to the `&'static [u8]` SVG source of the named icon, e.g.
/// `bi!(gear-wide-connected)`.
#[proc_macro]
pub fn bi(input: TokenStream) -> TokenStream {
    match icon_path(input) {
        Ok(path) => quote! { #path.svg() }.into(),
        Err(error) => error.into(),
    }
}

/// Expands to an `iced::widget::svg::Svg` element for the named icon, e.g.
/// `iced::bi!(gear-wide-connected)`.
#[proc_macro]
pub fn iced_bi(input: TokenStream) -> TokenStream {
    match icon_path(input) {
        Ok(path) => quote! { ::bootstrap_icons::iced::element(#path) }.into(),
        Err(error) => error.into(),
    }
}
