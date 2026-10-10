//! The Rust side of the `protoc_contrib.*` proto packages, beside their
//! `.proto` files: the counterpart of the Go packages under
//! protoc-contrib's `protoc_contrib/` directories, one module per package.
//!
//! - [`defaults`]: `protoc_contrib.defaults`, read by
//!   [protoc-gen-rust-defaults](https://github.com/protoc-contrib/protoc-gen-rust-defaults).

pub mod defaults;
