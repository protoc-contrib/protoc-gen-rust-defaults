# Changelog

## [0.3.0](https://github.com/protoc-contrib/protoc-gen-rust-defaults/compare/v0.2.0...v0.3.0) (2026-10-11)


### ⚠ BREAKING CHANGES

* the impl_trait plugin option and protoc_contrib::defaults::SetDefaults are removed. A buf.gen.yaml that still sets impl_trait keeps generating, since unknown options are ignored, but no trait impls are emitted.

### Features

* recurse into repeated and map message fields ([#9](https://github.com/protoc-contrib/protoc-gen-rust-defaults/issues/9)) ([4db5ae4](https://github.com/protoc-contrib/protoc-gen-rust-defaults/commit/4db5ae45efa8da4be78264a489de72dfd3ed5269))


### Code Refactoring

* read the options from protoc-types and drop impl_trait ([#7](https://github.com/protoc-contrib/protoc-gen-rust-defaults/issues/7)) ([4cbb1bc](https://github.com/protoc-contrib/protoc-gen-rust-defaults/commit/4cbb1bcb1e358a1ea959be913648e99209a487c5))

## [0.2.0](https://github.com/protoc-contrib/protoc-gen-rust-defaults/compare/v0.1.0...v0.2.0) (2026-10-10)


### Features

* add the protoc-contrib crate beside options.proto ([#5](https://github.com/protoc-contrib/protoc-gen-rust-defaults/issues/5)) ([370ac38](https://github.com/protoc-contrib/protoc-gen-rust-defaults/commit/370ac3803a4a31566894a78d2a4d7b8b5a8409bb))
* generate set_defaults() for buffa messages from protoc_contrib.defaults options ([#2](https://github.com/protoc-contrib/protoc-gen-rust-defaults/issues/2)) ([80a9272](https://github.com/protoc-contrib/protoc-gen-rust-defaults/commit/80a92728e591625800625c9013c3f3b4898a5fa4)), closes [#1](https://github.com/protoc-contrib/protoc-gen-rust-defaults/issues/1)
