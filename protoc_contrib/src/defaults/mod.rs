//! The `protoc_contrib.defaults` package, from
//! `protoc_contrib/defaults/options.proto`: the counterpart of
//! protoc-gen-go-defaults' `protoc_contrib/defaults` Go package, and like it,
//! part of the plugin's repository,
//! [protoc-gen-rust-defaults](https://github.com/protoc-contrib/protoc-gen-rust-defaults).
//!
//! It holds the options, `protoc_contrib/defaults/options.proto` compiled for
//! buffa: [`FieldDefaults`], [`MessageDefaults`] and the [`VALUE`], [`ONEOF`],
//! [`SKIP`] and [`UNEXPORTED`] extensions, for reading the annotations off a
//! descriptor. The plugin reads them through these too. Generated once and
//! committed, so depending on this crate needs no `protoc`.

#[allow(
    clippy::all,
    clippy::pedantic,
    warnings,
    reason = "buffa-generated code -- upstream codegen style, not ours to police"
)]
mod generated {
    include!("generated/_include.rs");
}

pub use generated::protoc_contrib::defaults::*;

#[cfg(test)]
mod tests {
    use super::*;
    use buffa::Message;

    #[test]
    fn the_options_round_trip_through_the_wire() {
        let defaults = FieldDefaults {
            r#type: Some(field_defaults::Type::Int32(50)),
            ..FieldDefaults::default()
        };
        let decoded = FieldDefaults::decode_from_slice(&defaults.encode_to_vec()).unwrap();
        assert_eq!(decoded.r#type, Some(field_defaults::Type::Int32(50)));
    }

    #[test]
    fn the_extensions_are_numbered_as_options_proto_numbers_them() {
        assert_eq!(VALUE.number(), 55_001);
        assert_eq!(VALUE.extendee(), "google.protobuf.FieldOptions");
        assert_eq!(ONEOF.number(), 55_002);
        assert_eq!(SKIP.number(), 55_003);
        assert_eq!(UNEXPORTED.number(), 55_004);
    }
}
