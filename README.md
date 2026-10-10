# protoc-gen-rust-defaults

[![CI](https://github.com/protoc-contrib/protoc-gen-rust-defaults/actions/workflows/ci.yml/badge.svg)](https://github.com/protoc-contrib/protoc-gen-rust-defaults/actions/workflows/ci.yml)
[![Release](https://img.shields.io/github/v/release/protoc-contrib/protoc-gen-rust-defaults?include_prereleases)](https://github.com/protoc-contrib/protoc-gen-rust-defaults/releases)
[![License: MIT](https://img.shields.io/badge/License-MIT-yellow.svg)](LICENSE)
[![Rust (edition 2024)](https://img.shields.io/badge/Rust-2024-black?logo=rust)](https://www.rust-lang.org/)
[![Nix Flake](https://img.shields.io/badge/Nix-Flake-5277C3?logo=nixos&logoColor=white)](https://nixos.wiki/wiki/Flakes)
[![protoc](https://img.shields.io/badge/protoc-compatible-blue)](https://protobuf.dev)

A protoc plugin that generates `set_defaults(&mut self)` for the owned message
types [buffa](https://github.com/anthropics/buffa) generates, from
`protoc_contrib.defaults` annotations.

The Rust counterpart of
[protoc-gen-go-defaults](https://github.com/protoc-contrib/protoc-gen-go-defaults).
It reads the **same** `protoc_contrib/defaults/options.proto`, so a schema
shared by a Go and a Rust service declares its defaults once:

```proto
import "protoc_contrib/defaults/options.proto";

message ListBooksRequest {
  int32 page_size = 1 [(protoc_contrib.defaults.value).int32 = 50];
}
```

```rust
let mut request = ListBooksRequest::default();
request.set_defaults();
assert_eq!(request.page_size, 50);
```

Generated code is plain inherent methods. It depends on nothing beyond the
`buffa` and `buffa-types` crates the messages already need.

## Why a plugin

buffa can't carry these defaults itself. It ignores `[default = X]` on a field
with explicit presence, and proto3 rejects `default` outright. Its
`field_attribute` hook is `build.rs`-only, and the useful attribute,
`#[serde(default)]`, applies to JSON decoding, not binary.

## What it generates

For each message with a `(protoc_contrib.defaults.value)` on a field or a
`(protoc_contrib.defaults.oneof)` on a oneof:

```rust
impl ListBooksRequest {
    pub fn set_defaults(&mut self) {
        if self.page_size == 0 {
            self.page_size = 50i32;
        }
    }
}
```

The output for each proto package goes into one file,
`<package>.defaults.rs`, with an `impl` block for each message. A message
without annotations gets nothing, and a package without them gets no file.

| Extension | On | Generates |
| --- | --- | --- |
| `(protoc_contrib.defaults.value)` | a field | Fills the field if it is unset |
| `(protoc_contrib.defaults.oneof)` | a oneof | Sets the named arm, with its own default, if the oneof is unset |
| `(protoc_contrib.defaults.skip)` | a message | Nothing; a hand-written `set_defaults` takes its place |
| `(protoc_contrib.defaults.unexported)` | a message | `pub(crate) fn apply_defaults` instead, for a hand-written `set_defaults` to call |

### Value kinds

These are the same kinds as the Go plugin, and the same checks run at
generation time, with the same error messages. An annotation that doesn't fit
its field fails `buf generate` instead of compiling to something else.

| `value` | Field | Notes |
| --- | --- | --- |
| `float`, `double`, `int32`, `int64`, `uint32`, `uint64`, `bool`, `string`, `bytes` | that scalar, or its `google.protobuf.*Value` wrapper | |
| `sint32`, `sint64`, `fixed32`, `fixed64`, `sfixed32`, `sfixed64` | that scalar | |
| `enum` / `enum_name` | an enum | By number or by name; both must be defined. Emitted as the variant path, not a number. |
| `duration` | `google.protobuf.Duration` | Prometheus grammar: `"2d"`, `"1h30m"`, `"42w"`. Parsed at generation time. |
| `timestamp` | `google.protobuf.Timestamp` | `"now"`, read when the defaults are applied; or RFC3339, RFC1123(Z), RFC850 or RFC822(Z), parsed at generation time. |
| `message = {initialize, recurse}` | a message | `initialize` creates it if unset. `recurse` calls its `set_defaults`, if it is set or was just created. |

Repeated and map fields are rejected, as they are in Go.

### What "unset" means

A field that is already set is never overwritten. Whether a field counts as
set depends on how buffa typed it:

| buffa type | Unset when |
| --- | --- |
| the bare value: proto3 implicit presence, editions `IMPLICIT`, proto2 `required` | it holds the zero value: `0`, `""`, empty bytes, enum value `0`. A float is compared by its bits, so `-0.0` counts as set. |
| `Option<T>`: proto3 `optional`, proto2 `optional`, editions `EXPLICIT` | it is `None`. `Some(0)` is set. |
| `MessageField<T>`, including wrappers and well-known types | there is no message. A wrapper holding `0` is set. |
| a oneof | it is `None`. An active arm is set, even at its zero value. |

The generated code never names a concrete field type. A string is assigned
with `.into()`, and a message is created and filled through
`get_or_insert_default()`. That keeps it independent of how buffa was
configured to represent strings, bytes, boxing, or where `google.protobuf`
maps to.

### Recursion

`message = {recurse: true}` calls the nested message's `set_defaults`, through
a `MessageField` or a boxed oneof arm. Whether that call is emitted depends on
the target:

- **A message with annotations of its own** has a generated method, so the
  call is emitted. This also applies to a message in another package, which
  must be generated with this plugin too.
- **A `skip` or `unexported` message** is expected to have a hand-written
  `set_defaults`, so the call is emitted. If the method is missing, that's a
  compile error, not a silent no-op.
- **A message without annotations, or a foreign type** such as
  `google.protobuf.DescriptorProto`, has nothing to call. Recursion into it
  does nothing, as in Go. `initialize` still creates it.

### Composing with hand-written defaults

Some defaults depend on other fields: minting an id, naming a resource from
its parent, the update mask an update implies. No annotation can describe
them. Mark the message `unexported` and write `set_defaults` around the
generated body, as Go's `SetDefaults() { x._SetDefaults() }` pattern does:

```proto
message CreateBookRequest {
  option (protoc_contrib.defaults.unexported) = true;
  string book_id = 1;
  int32 copies = 2 [(protoc_contrib.defaults.value).int32 = 1];
}
```

```rust
impl CreateBookRequest {
    pub fn set_defaults(&mut self) {
        self.apply_defaults(); // generated
        if self.book_id.is_empty() {
            self.book_id = mint_id();
        }
    }
}
```

Rust can't have two inherent `set_defaults` on one type, so `unexported`
generates `apply_defaults` and leaves the name to you. `skip` generates
nothing, and the whole method is yours. Either way, write it as an
**inherent** method: that's what a `recurse` into the message calls.

### Where it differs from protoc-gen-go-defaults

| | Go | Rust |
| --- | --- | --- |
| An active oneof arm holding its zero value | overwritten with the arm's default | left alone: an active arm is set |
| `bytes` with explicit presence | presence ignored; empty counts as unset | `None` is unset, `Some(vec![])` is set |
| A zone abbreviation in a timestamp (`MST`) | resolved against the generating machine's local zone; an unknown one is read as UTC | the fixed offset RFC 822 gives it (`UT`, `GMT`, `Z`, `EST`…`PDT`); any other abbreviation is an error |
| `float = nan` / `inf` | emits invalid Go | `f32::NAN`, `f32::INFINITY` |
| Editions | unsupported | supported, with presence resolved per field |
| Applying defaults to a message of unknown type | `defaults.Apply` / `ApplyAll`, by reflection | an impl of your own trait, with `impl_trait` |

## Wiring it up

It isn't on crates.io. Take `packages.default` from this repository's flake
(see [Nix](#nix)), download a binary from the
[releases](https://github.com/protoc-contrib/protoc-gen-rust-defaults/releases),
or build it from GitHub:

```sh
cargo install --git https://github.com/protoc-contrib/protoc-gen-rust-defaults protoc-gen-rust-defaults
```

The annotations come from protoc-gen-go-defaults' BSR module, so the schema
depends on it there:

```yaml
# buf.yaml
deps:
  - buf.build/protoc-contrib/protoc-gen-go-defaults
```

Run it as a **local** plugin with `strategy: all`:

```yaml
plugins:
  - local: protoc-gen-rust-defaults
    out: src/defaults
    strategy: all
    opt:
      # Where the buffa-generated message types live. Each leaf of the emitted
      # module tree brings the matching package into scope from here, and a
      # type from another package is named from it. Defaults to crate::proto.
      - proto_module=crate::buffa
```

`strategy: all` is required. The output is one `<package>.defaults.rs` per
proto package, mirroring `protoc-gen-buffa` under `file_per_package=true`.
Under buf's default per-directory strategy, a package spread over two
directories would be generated twice, each run seeing only half of it, and the
second write would win.

Mount the output with `include!`. The generated `mod.rs` has no inner
attributes, so `#[path]` works too:

```rust
pub mod defaults {
    include!("defaults/mod.rs");
}
```

### Or include it beside buffa's types: `packaging=false`

To splice each package's file into the module that already holds its buffa
types, use `packaging=false` and write into buffa's output directory:

```rust
pub mod example { pub mod v1 {
    include!("example.v1.rs");
    include!("example.v1.defaults.rs");
}}
```

With `packaging=false`, no `mod.rs` is written. That matters, because a
generated `mod.rs` in the same directory as a hand-written one silently
replaces it. Types in other packages are reached with `super` hops, exactly as
buffa reaches them, so the included file has to sit at the package's own
position in buffa's tree.

### Generic code: `impl_trait=<path>`

Rust has no reflection that generic code could use to reach a message's
defaults, which Go's `defaults.Apply` does. If you want an interceptor that
defaults whatever request it's handed, point the plugin at a trait with a
`fn set_defaults(&mut self)`. The [`protoc-contrib-defaults`
crate](#the-protoc-contrib-defaults-crate) has one:

```yaml
    opt:
      - impl_trait=::protoc_contrib_defaults::SetDefaults
```

Every generated `set_defaults` then also gets an impl of that trait,
delegating to the inherent method:

```rust
impl ::protoc_contrib_defaults::SetDefaults for ListBooksRequest {
    fn set_defaults(&mut self) {
        Self::set_defaults(self);
    }
}
```

Any trait of the same shape works, so you can define your own instead.

An `unexported` message gets no impl, since its method is yours and so is its
impl. Without `impl_trait`, the output is inherent methods only. That's enough
when callers know the concrete type, for example an interceptor that matches
on the RPC and calls `ListBooksRequest::set_defaults`.

### `extern_path`

If buffa runs with `extern_path`, pass the same entries here. An enum this
plugin names is then named where buffa put it:

```yaml
    opt:
      - extern_path=.google.rpc=::buffa_protos::google::rpc
```

Well-known types need none. They resolve to `buffa-types` here, the same as
in buffa.

### The `protoc-contrib-defaults` crate

`protoc_contrib/defaults` holds the Rust side of the
`protoc_contrib.defaults` package, beside its `options.proto`. It's the
counterpart of protoc-gen-go-defaults' `protoc_contrib/defaults` Go package,
and it has two parts:

- **The options compiled for buffa:** `FieldDefaults`, `MessageDefaults`, and
  the `VALUE`, `ONEOF`, `SKIP` and `UNEXPORTED` extensions, for reading the
  annotations off a descriptor. The plugin reads them through these too. The
  generated code is committed, so depending on the crate needs no `protoc`.
- **`SetDefaults`:** the trait for `impl_trait`, with an impl for `Box<T>`.

The generated code doesn't need the crate unless you use `impl_trait` with
its trait. It isn't on crates.io either, so take it from GitHub, pinned to the
release tag of the plugin that generated your code:

```toml
[dependencies]
protoc-contrib-defaults = { git = "https://github.com/protoc-contrib/protoc-gen-rust-defaults", tag = "vX.Y.Z" }
```

### Nix

```nix
inputs.protoc-gen-rust-defaults.url = "github:protoc-contrib/protoc-gen-rust-defaults";
# ...
packages = [ protoc-gen-rust-defaults.packages.${system}.default ];
```

## Development

The toolchain is pinned by `rust-toolchain.toml` and provided by the flake:

```bash
nix develop          # cargo, rustc, clippy, rustfmt, protoc
cargo test --workspace
```

If the host has no Nix, `.devcontainer/` supplies it: open the repo in a
devcontainer and the same `nix develop` works.

`cargo test --workspace` regenerates the schema in `tests/proto` with both
`buffa` and this plugin. It mounts the result both ways, under the same
pedantic clippy lints a consumer uses, and exercises it, so a change to an
emitter is compiled and run against real buffa types.

`tests/proto/tests` is protoc-gen-go-defaults' own fixture, ported, so both
plugins are checked against one contract. `tests/golden` holds the generated
code, so a change to its shape shows up in review. Regenerate it with:

```bash
UPDATE_GOLDEN=1 cargo test -p defaults-fixture --test golden
```

`protoc_contrib/defaults/src/generated` is buffa's output for
`protoc_contrib/defaults/options.proto`, and a test fails if it drifts from a
fresh generation. After changing the proto or the buffa version, regenerate it
with:

```bash
UPDATE_GENERATED=1 cargo test -p protoc-contrib-defaults --test generated
```

## License

MIT. See [LICENSE](LICENSE).
