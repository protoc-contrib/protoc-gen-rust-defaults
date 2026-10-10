//! Generates `set_defaults(&mut self)` for the owned message types
//! `protoc-gen-buffa` emits, from the `protoc_contrib.defaults` annotations on
//! a protobuf schema.
//!
//! The Rust counterpart of
//! [protoc-gen-go-defaults](https://github.com/protoc-contrib/protoc-gen-go-defaults),
//! reading the same `protoc_contrib/defaults/options.proto`, so a schema shared
//! by a Go and a Rust service declares its defaults once. Generated code
//! implements the `SetDefaults` trait from
//! [protoc-contrib-defaults](https://crates.io/crates/protoc-contrib-defaults).
//!
//! The binary is the entry point; this library exists so the passes can be
//! driven from a test without going through stdin.

pub mod annotations;
mod check;
pub mod emit;
mod idents;
mod literal;
pub mod messages;
mod presence;
