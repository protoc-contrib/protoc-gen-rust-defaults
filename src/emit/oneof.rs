//! The statements that default a oneof.
//!
//! A oneof that is unset is set to the arm `(protoc_contrib.defaults.oneof)`
//! names, holding that arm's own default if it has one. Then whichever arm is
//! active, the caller's or the one just set, has its `set_defaults` called if
//! it is a message arm annotated `recurse`.
//!
//! An active scalar arm is never overwritten, even at its zero value: a oneof
//! arm has explicit presence, so being active is being set. This is where the
//! plugin parts from protoc-gen-go-defaults, which fills a zero-valued active
//! scalar arm with its default.

use anyhow::Result;
use proc_macro2::TokenStream;
use quote::quote;

use super::field::{fill, literal};
use crate::messages::Oneof;

/// The statements that default `oneof`.
pub(super) fn emit(oneof: &Oneof) -> Result<TokenStream> {
    let field = buffa_codegen::idents::make_field_ident(&oneof.name);
    let place = quote! { self.#field };
    let path: syn::Path = syn::parse_str(&oneof.path)?;

    let default = match &oneof.default {
        Some(arm) => {
            let variant = variant(&arm.name);
            let target = quote! { message };
            let (init, fill) = match &arm.value {
                Some(value) if arm.message => (None, fill(value, &target)?),
                Some(value) => (literal(value)?, Vec::new()),
                None => (None, Vec::new()),
            };
            let init = init.unwrap_or_else(|| quote! { ::core::default::Default::default() });
            // The arm's type is only known through the variant, so a wrapper
            // or well-known arm is created empty and filled through a borrow
            // of what was just stored.
            let fill = (!fill.is_empty()).then(|| {
                quote! {
                    if let ::core::option::Option::Some(#path::#variant(message)) = &mut #place {
                        #( #fill )*
                    }
                }
            });
            quote! {
                if #place.is_none() {
                    #place = ::core::option::Option::Some(#path::#variant(#init));
                    #fill
                }
            }
        }
        None => TokenStream::new(),
    };

    let recurse = match oneof.recurse.as_slice() {
        [] => TokenStream::new(),
        [arm] => {
            let variant = variant(arm);
            quote! {
                if let ::core::option::Option::Some(#path::#variant(message)) = &mut #place {
                    ::protoc_contrib_defaults::SetDefaults::set_defaults(message);
                }
            }
        }
        arms => {
            let variants = arms.iter().map(|arm| variant(arm));
            quote! {
                match &mut #place {
                    #(
                        ::core::option::Option::Some(#path::#variants(message)) => {
                            ::protoc_contrib_defaults::SetDefaults::set_defaults(message);
                        }
                    )*
                    _ => {}
                }
            }
        }
    };

    Ok(quote! { #default #recurse })
}

/// Whether defaulting `oneof` creates its default arm with
/// `Default::default()`: a message arm, or one without a default of its own.
///
/// buffa boxes a message arm unless configured not to, and nothing in the
/// request says which it did, so the arm's type cannot be named -- which is
/// what the consumer's `clippy::default_trait_access` asks for.
pub(super) fn creates_default(oneof: &Oneof) -> bool {
    oneof
        .default
        .as_ref()
        .is_some_and(|arm| arm.message || arm.value.is_none())
}

/// The variant buffa names a oneof arm: its field name in `PascalCase`.
fn variant(name: &str) -> syn::Ident {
    buffa_codegen::idents::make_field_ident(&crate::idents::pascal_case(name))
}
