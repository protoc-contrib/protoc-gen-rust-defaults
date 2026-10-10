//! The `protoc_contrib.defaults` package, from
//! `protoc_contrib/defaults/options.proto`: the counterpart of
//! protoc-gen-go-defaults' `protoc_contrib/defaults` Go package, and like it,
//! part of the plugin's repository,
//! [protoc-gen-rust-defaults](https://github.com/protoc-contrib/protoc-gen-rust-defaults).
//!
//! It holds two things:
//!
//! - **The options**, `protoc_contrib/defaults/options.proto` compiled for
//!   buffa: [`FieldDefaults`], [`MessageDefaults`] and the [`VALUE`],
//!   [`ONEOF`], [`SKIP`] and [`UNEXPORTED`] extensions, for reading the
//!   annotations off a descriptor. The plugin reads them through these too.
//!   Generated once and committed, so depending on this crate needs no
//!   `protoc`.
//! - **[`SetDefaults`]**, the trait to code against messages generically
//!   with. The generated `set_defaults` methods are inherent; run the plugin
//!   with `impl_trait=::protoc_contrib::defaults::SetDefaults` and each also
//!   implements this.
//!
//! Go's `defaults.Apply` fills a message by reflection, which buffa does not
//! offer; the trait is how generic code reaches the generated methods instead.

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

/// A message whose unset fields can be filled with their declared defaults.
///
/// Implemented by every message protoc-gen-rust-defaults generates a
/// `set_defaults` for, when it is run with
/// `impl_trait=::protoc_contrib::defaults::SetDefaults`. An `unexported`
/// message is the exception: its `set_defaults` is hand-written, and so is its
/// impl.
///
/// ```ignore
/// fn apply<M: protoc_contrib::defaults::SetDefaults>(request: &mut M) {
///     request.set_defaults();
/// }
/// ```
pub trait SetDefaults {
    /// Fills each unset field with its default. A field that is already set
    /// is left as it is.
    fn set_defaults(&mut self);
}

/// A boxed message defaults its contents: buffa boxes a message held in a
/// oneof, and a recursive message field.
impl<T: SetDefaults + ?Sized> SetDefaults for Box<T> {
    fn set_defaults(&mut self) {
        (**self).set_defaults();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use buffa::Message;

    #[derive(Default)]
    struct Counter(u32);

    impl SetDefaults for Counter {
        fn set_defaults(&mut self) {
            self.0 += 1;
        }
    }

    #[test]
    fn a_box_defaults_what_it_holds() {
        let mut boxed = Box::new(Counter::default());
        SetDefaults::set_defaults(&mut boxed);
        assert_eq!(boxed.0, 1);
    }

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
