//! The test schema, compiled by `buffa` and extended by this plugin.
//!
//! The crate exists so the plugin's output is type-checked against real
//! buffa-generated messages under the lints a consumer holds it to, and
//! exercised by the tests beside it, which is the only way to catch output
//! that is well-formed Rust but wrong.
//!
//! Hand-written here: the mount points, and the `set_defaults` methods the
//! schema's `skip` and `unexported` messages ask for in place of generated
//! ones.

use protoc_contrib_defaults::SetDefaults;

/// The buffa-generated message types.
pub mod proto {
    #![allow(
        clippy::all,
        clippy::pedantic,
        warnings,
        reason = "buffa-build generated code -- upstream codegen style, not ours to police"
    )]

    include!(concat!(env!("OUT_DIR"), "/tree/proto/_include.rs"));
}

/// The `set_defaults` methods this plugin emits for that schema, in a module
/// tree of their own (`packaging=true`), each with an impl of
/// [`SetDefaults`].
pub mod defaults {
    include!(concat!(env!("OUT_DIR"), "/tree/defaults/mod.rs"));
}

/// A package whose defaults are included into buffa's own module for it
/// (`packaging=false`), as grpc-rust-template mounts them: inherent methods,
/// no trait.
pub mod inline {
    pub mod v1 {
        #![allow(
            clippy::all,
            clippy::pedantic,
            warnings,
            reason = "buffa-build generated code shares this module; the defaults in it are linted where `defaults` mounts the same generator's output"
        )]

        include!(concat!(env!("OUT_DIR"), "/inline/proto/inline.v1.mod.rs"));
        include!(concat!(
            env!("OUT_DIR"),
            "/inline/defaults/inline.v1.defaults.rs"
        ));
    }
}

/// `unexported`: the generated `apply_defaults`, plus a slug no annotation can
/// derive. Inherent, so a `recurse` into the message calls it.
impl proto::example::v1::Draft {
    pub fn set_defaults(&mut self) {
        self.apply_defaults();
        if self.slug.is_empty() {
            self.slug = self.title.to_lowercase().replace(' ', "-");
        }
    }
}

/// `impl_trait` skips an `unexported` message, whose trait impl is as
/// hand-written as its method.
impl SetDefaults for proto::example::v1::Draft {
    fn set_defaults(&mut self) {
        Self::set_defaults(self);
    }
}

/// The ported Go fixture's `unexported` message, wrapped the way
/// protoc-gen-go-defaults' `test.extensions.go` wraps it.
impl proto::tests::TestUnexported {
    pub fn set_defaults(&mut self) {
        self.apply_defaults();
    }
}

/// `skip`: nothing generated, so the whole method is hand-written.
impl proto::example::v1::Frozen {
    pub fn set_defaults(&mut self) {
        if self.state.is_empty() {
            "hand-written".clone_into(&mut self.state);
        }
    }
}

/// The ported Go fixture's `skip` message, recursed into from both of its
/// oneofs. Fills what the annotation would have, to show it was called.
impl proto::tests::OneOfOne {
    pub fn set_defaults(&mut self) {
        if self.string_field.is_empty() {
            "hand-written".clone_into(&mut self.string_field);
        }
    }
}
