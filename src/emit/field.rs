//! The statements that default one field outside a oneof, and the literals
//! and fills [`oneof`](super::oneof) shares with them.
//!
//! What "unset" means follows how buffa typed the field ([`Shape`]): the zero
//! value for a bare field, `None` for an `Option`, no message for a
//! `MessageField`. A field that is set is never written, which is the whole
//! contract.
//!
//! A `recurse` calls the nested message's inherent `set_defaults` with
//! method-call syntax, which reaches through the `&mut` a `MessageField` lends
//! and the `Box` a oneof arm holds alike, and needs no trait in scope.
//!
//! Concrete field types are never named. A string is assigned with `.into()`,
//! and a message -- well-known or not -- is created and filled through
//! `get_or_insert_default()`, so the output does not depend on which
//! string, bytes or pointer representation the consumer configured buffa with,
//! nor on where it mapped `google.protobuf`.

use anyhow::Result;
use proc_macro2::{Literal, TokenStream};
use quote::{format_ident, quote};

use crate::check::{Scalar, Value};
use crate::literal::Instant;
use crate::messages::{Field, Shape};

/// The statements that default `field`.
pub(super) fn emit(field: &Field) -> Result<TokenStream> {
    let ident = buffa_codegen::idents::make_field_ident(&field.name);
    let place = quote! { self.#ident };
    Ok(match field.shape {
        Shape::Bare => {
            let Some(literal) = literal(&field.value)? else {
                return Ok(TokenStream::new());
            };
            if is_zero(&field.value) {
                // Defaulting a bare field to its zero value would be a no-op.
                return Ok(TokenStream::new());
            }
            let unset = unset(&field.value, &place);
            quote! {
                if #unset {
                    #place = #literal;
                }
            }
        }
        Shape::Optional => {
            let Some(literal) = literal(&field.value)? else {
                return Ok(TokenStream::new());
            };
            quote! {
                if #place.is_none() {
                    #place = ::core::option::Option::Some(#literal);
                }
            }
        }
        Shape::Message => message(&field.value, &place)?,
    })
}

/// The statements that default a `MessageField`.
fn message(value: &Value, place: &TokenStream) -> Result<TokenStream> {
    if let Value::Message {
        initialize,
        recurse,
    } = value
    {
        // `get_or_insert_default` creates the message only if it is unset, so
        // initializing needs no check of its own -- and when the message is
        // recursed into as well, the reference it returns is the one to
        // recurse through.
        return Ok(match (initialize, recurse) {
            (true, true) => quote! {
                #place.get_or_insert_default().set_defaults();
            },
            (true, false) => quote! {
                #place.get_or_insert_default();
            },
            (false, true) => quote! {
                if let ::core::option::Option::Some(message) = #place.as_option_mut() {
                    message.set_defaults();
                }
            },
            (false, false) => TokenStream::new(),
        });
    }

    let target = quote! { message };
    let fill = fill(value, &target)?;
    if fill.is_empty() {
        return Ok(quote! {
            #place.get_or_insert_default();
        });
    }
    Ok(quote! {
        if #place.is_unset() {
            let message = #place.get_or_insert_default();
            #( #fill )*
        }
    })
}

/// The assignments that give a freshly defaulted message -- a wrapper, a
/// `Duration` or a `Timestamp`, reached through `target` -- its value.
///
/// Field by field rather than as a struct literal, which would have to name
/// the type and every field of it; and zero fields are left to the
/// `Default::default()` the message was created with.
pub(super) fn fill(value: &Value, target: &TokenStream) -> Result<Vec<TokenStream>> {
    Ok(match value {
        Value::Scalar(scalar) => {
            if is_zero(value) {
                Vec::new()
            } else {
                let literal = scalar_literal(scalar)?;
                vec![quote! { #target.value = #literal; }]
            }
        }
        Value::Duration(instant) | Value::Timestamp(instant) => instant_fill(*instant, target)?,
        // Converted through the field's own `Timestamp` type, which buffa-types
        // gives a `From<SystemTime>`, and copied over field by field: the
        // target may be the message or a box of it, and field access reaches
        // through either.
        Value::Now { path } => {
            let path: syn::Path = syn::parse_str(path)?;
            vec![quote! {
                let now = #path::from(::std::time::SystemTime::now());
                #target.seconds = now.seconds;
                #target.nanos = now.nanos;
            }]
        }
        Value::Enum { .. } | Value::Message { .. } => Vec::new(),
    })
}

fn instant_fill(instant: Instant, target: &TokenStream) -> Result<Vec<TokenStream>> {
    let mut fill = Vec::new();
    if instant.seconds != 0 {
        let seconds = number(&instant.seconds.to_string(), "i64")?;
        fill.push(quote! { #target.seconds = #seconds; });
    }
    if instant.nanos != 0 {
        let nanos = number(&instant.nanos.to_string(), "i32")?;
        fill.push(quote! { #target.nanos = #nanos; });
    }
    Ok(fill)
}

/// The expression a scalar or enum default is assigned as, or `None` for a
/// value that is not one -- a message, a `Duration`, a `Timestamp`.
pub(super) fn literal(value: &Value) -> Result<Option<TokenStream>> {
    Ok(match value {
        Value::Scalar(scalar) => Some(scalar_literal(scalar)?),
        Value::Enum {
            path,
            variant,
            closed,
        } => {
            let path: syn::Path = syn::parse_str(path)?;
            let variant = buffa_codegen::idents::make_field_ident(variant);
            Some(if *closed {
                quote! { #path::#variant }
            } else {
                quote! { ::buffa::EnumValue::Known(#path::#variant) }
            })
        }
        Value::Duration(_) | Value::Timestamp(_) | Value::Now { .. } | Value::Message { .. } => {
            None
        }
    })
}

fn scalar_literal(scalar: &Scalar) -> Result<TokenStream> {
    Ok(match scalar {
        Scalar::Float(v) => float(f64::from(*v), &format!("{v:?}"), "f32")?,
        Scalar::Double(v) => float(*v, &format!("{v:?}"), "f64")?,
        Scalar::Int32(v) => number(&v.to_string(), "i32")?,
        Scalar::Int64(v) => number(&v.to_string(), "i64")?,
        Scalar::Uint32(v) => number(&v.to_string(), "u32")?,
        Scalar::Uint64(v) => number(&v.to_string(), "u64")?,
        Scalar::Bool(v) => quote! { #v },
        Scalar::String(v) => {
            let literal = Literal::string(v);
            quote! { #literal.into() }
        }
        Scalar::Bytes(v) => {
            let literal = Literal::byte_string(v);
            quote! { #literal.as_slice().into() }
        }
    })
}

/// A float literal from its shortest round-tripping rendering, or the named
/// constant for a value no literal spells.
fn float(value: f64, rendered: &str, suffix: &str) -> Result<TokenStream> {
    let ty = format_ident!("{suffix}");
    if value.is_nan() {
        return Ok(quote! { #ty::NAN });
    }
    if value.is_infinite() {
        return Ok(if value > 0.0 {
            quote! { #ty::INFINITY }
        } else {
            quote! { #ty::NEG_INFINITY }
        });
    }
    let (mantissa, exponent) = match rendered.split_once('e') {
        Some((mantissa, exponent)) => (mantissa, format!("e{exponent}")),
        None => (rendered, String::new()),
    };
    let grouped = match mantissa.split_once('.') {
        Some((whole, fraction)) => format!("{}.{}", group(whole), group_fraction(fraction)),
        None if exponent.is_empty() => format!("{}.0", group(mantissa)),
        None => group(mantissa),
    };
    parse(&format!("{grouped}{exponent}{suffix}"))
}

/// An integer literal, with its digits grouped in threes so a long one passes
/// `clippy::unreadable_literal` in the consumer.
fn number(rendered: &str, suffix: &str) -> Result<TokenStream> {
    parse(&format!("{}{suffix}", group(rendered)))
}

fn group(digits: &str) -> String {
    let (sign, digits) = match digits.strip_prefix('-') {
        Some(digits) => ("-", digits),
        None => ("", digits),
    };
    if digits.len() < 5 {
        return format!("{sign}{digits}");
    }
    let mut out = String::with_capacity(digits.len() + digits.len() / 3);
    for (index, digit) in digits.chars().enumerate() {
        if index > 0 && (digits.len() - index) % 3 == 0 {
            out.push('_');
        }
        out.push(digit);
    }
    format!("{sign}{out}")
}

fn group_fraction(digits: &str) -> String {
    if digits.len() < 5 {
        return digits.to_owned();
    }
    let mut out = String::with_capacity(digits.len() + digits.len() / 3);
    for (index, digit) in digits.chars().enumerate() {
        if index > 0 && index % 3 == 0 {
            out.push('_');
        }
        out.push(digit);
    }
    out
}

fn parse(source: &str) -> Result<TokenStream> {
    source
        .parse()
        .map_err(|error| anyhow::anyhow!("{source:?} is not a Rust literal: {error}"))
}

/// The test for whether a bare field holding `value`'s type is at its zero
/// value, which for a field without presence is the only "unset" there is.
///
/// A float is compared by its bits, so `-0.0` -- which proto3 serializes, and
/// so counts as set -- is left alone, and so the consumer's
/// `clippy::float_cmp` has nothing to say.
fn unset(value: &Value, place: &TokenStream) -> TokenStream {
    match value {
        Value::Scalar(Scalar::Float(_) | Scalar::Double(_)) => quote! { #place.to_bits() == 0 },
        Value::Scalar(Scalar::Bool(_)) => quote! { !#place },
        Value::Scalar(Scalar::String(_) | Scalar::Bytes(_)) => quote! { #place.is_empty() },
        Value::Enum { closed: true, .. } => {
            quote! { ::buffa::Enumeration::to_i32(&#place) == 0 }
        }
        Value::Enum { closed: false, .. } => quote! { #place.to_i32() == 0 },
        _ => quote! { #place == 0 },
    }
}

/// Whether `value` is its type's zero value, so defaulting a bare field to it
/// would change nothing.
fn is_zero(value: &Value) -> bool {
    match value {
        Value::Scalar(scalar) => match scalar {
            Scalar::Float(v) => v.to_bits() == 0,
            Scalar::Double(v) => v.to_bits() == 0,
            Scalar::Int32(v) => *v == 0,
            Scalar::Int64(v) => *v == 0,
            Scalar::Uint32(v) => *v == 0,
            Scalar::Uint64(v) => *v == 0,
            Scalar::Bool(v) => !v,
            Scalar::String(v) => v.is_empty(),
            Scalar::Bytes(v) => v.is_empty(),
        },
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn render(tokens: &TokenStream) -> String {
        tokens.to_string().replace(' ', "")
    }

    #[test]
    fn groups_long_numbers_in_threes() {
        assert_eq!(render(&number("42", "i64").unwrap()), "42i64");
        assert_eq!(render(&number("1234", "i64").unwrap()), "1234i64");
        assert_eq!(render(&number("25401600", "i64").unwrap()), "25_401_600i64");
        assert_eq!(
            render(&number("-562032000", "i64").unwrap()),
            "-562_032_000i64"
        );
    }

    #[test]
    fn renders_floats_that_round_trip() {
        assert_eq!(
            render(&scalar_literal(&Scalar::Float(0.42)).unwrap()),
            "0.42f32"
        );
        assert_eq!(
            render(&scalar_literal(&Scalar::Double(42.0)).unwrap()),
            "42.0f64"
        );
        assert_eq!(
            render(&scalar_literal(&Scalar::Double(1e20)).unwrap()),
            "1e20f64"
        );
        assert_eq!(
            render(&scalar_literal(&Scalar::Double(0.123_456_789)).unwrap()),
            "0.123_456_789f64"
        );
    }

    #[test]
    fn spells_the_floats_no_literal_can() {
        assert_eq!(
            render(&scalar_literal(&Scalar::Float(f32::NAN)).unwrap()),
            "f32::NAN"
        );
        assert_eq!(
            render(&scalar_literal(&Scalar::Double(f64::NEG_INFINITY)).unwrap()),
            "f64::NEG_INFINITY"
        );
    }

    #[test]
    fn a_negative_zero_is_not_zero() {
        assert!(is_zero(&Value::Scalar(Scalar::Double(0.0))));
        assert!(!is_zero(&Value::Scalar(Scalar::Double(-0.0))));
    }
}
