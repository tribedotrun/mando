//! Proc macros for the daemon HTTP transport layer.
//!
//! Provides `#[instrument_api]` — the only way to annotate a handler that
//! `api_route!` will accept. Expands to
//! `#[tracing::instrument(skip_all, fields(...))]` with the caller's named
//! fields, and rejects empty field lists at expansion time so every handler
//! emits a request-level span with identifying context.

use proc_macro::TokenStream;
use proc_macro2::{Span, TokenStream as TokenStream2};
use quote::quote;
use syn::{
    parse::{Parse, ParseStream},
    parse_macro_input,
    punctuated::Punctuated,
    Expr, Ident, ItemFn, LitStr, Token,
};

/// One `name = expr` field in `#[instrument_api(name = value, ...)]`.
struct InstrumentField {
    name: Ident,
    _eq: Token![=],
    value: Expr,
}

impl Parse for InstrumentField {
    fn parse(input: ParseStream) -> syn::Result<Self> {
        Ok(Self {
            name: input.parse()?,
            _eq: input.parse()?,
            value: input.parse()?,
        })
    }
}

struct InstrumentArgs {
    fields: Punctuated<InstrumentField, Token![,]>,
}

impl Parse for InstrumentArgs {
    fn parse(input: ParseStream) -> syn::Result<Self> {
        Ok(Self {
            fields: Punctuated::parse_terminated(input)?,
        })
    }
}

/// Annotate a daemon HTTP handler with a strict tracing span.
///
/// Every daemon handler registered via `api_route!` must be annotated with
/// `#[instrument_api(field = value, ...)]`. The attribute rejects empty
/// field lists at expansion time: a naked `#[instrument_api]` is a compile
/// error. Expands to
/// `#[tracing::instrument(skip_all, fields(<named fields>))]` applied to
/// the function, which satisfies the "No invisible HTTP requests" invariant.
///
/// A doc-hidden sibling marker `const _INSTRUMENT_API_OK__<fn>: () = ();`
/// is also emitted so a companion check script can verify every handler
/// referenced by `api_route!` is annotated here, not with a plain
/// `#[tracing::instrument]`.
#[proc_macro_attribute]
pub fn instrument_api(args: TokenStream, input: TokenStream) -> TokenStream {
    let args = parse_macro_input!(args as InstrumentArgs);
    if args.fields.is_empty() {
        return syn::Error::new(
            Span::call_site(),
            "#[instrument_api] requires at least one named field \
             (e.g. method = \"GET\", path = \"/api/foo\")",
        )
        .to_compile_error()
        .into();
    }

    let func = parse_macro_input!(input as ItemFn);

    // Reconstruct `fields(name = value, ...)` for tracing::instrument.
    let field_tokens: Vec<TokenStream2> = args
        .fields
        .iter()
        .map(|f| {
            let n = &f.name;
            let v = &f.value;
            quote! { #n = #v }
        })
        .collect();

    let fn_ident = func.sig.ident.clone();
    let marker_ident = Ident::new(
        &format!("_INSTRUMENT_API_OK__{}", fn_ident),
        fn_ident.span(),
    );
    let marker_literal = LitStr::new(&fn_ident.to_string(), fn_ident.span());

    let expanded = quote! {
        #[tracing::instrument(skip_all, fields(#(#field_tokens),*))]
        #func

        #[doc(hidden)]
        #[allow(non_upper_case_globals, dead_code)]
        pub const #marker_ident: &str = #marker_literal;
    };

    expanded.into()
}
