//! Generates the fixture crate twice over: once with `buffa-build`, for the
//! message types, and once with this plugin, for the `set_defaults` methods
//! that extend them.
//!
//! Running the plugin as a library rather than as a `protoc` plugin binary
//! keeps this to one `cargo build` — a build script cannot depend on a binary
//! produced by the same build — and exercises the same passes the binary
//! runs.
//!
//! The schema is generated in both of the ways a consumer can mount it: the
//! main schema as a module tree of its own (`packaging=true`), and the
//! `inline.v1` package included into buffa's own module for it
//! (`packaging=false`), the way grpc-rust-template does.

use std::{
    path::{Path, PathBuf},
    process::Command,
};

use anyhow::{Context, Result, bail};
use buffa::Message;
use buffa_codegen::{
    CodeGenConfig,
    generated::{
        compiler::CodeGeneratorRequest,
        descriptor::{FileDescriptorProto, FileDescriptorSet},
    },
};
use protoc_gen_rust_defaults::{
    emit,
    messages::{self, Schema},
};

/// The schema mounted as a tree of its own, relative to the sibling
/// `tests/proto`.
const SCHEMA: &[&str] = &[
    "tests/types.proto",
    "tests/test.proto",
    "example/v1/legacy.proto",
    "example/v1/editions.proto",
    "example/v1/nested.proto",
    "example/v1/compose.proto",
    "other/v1/catalog.proto",
];

/// The schema included into buffa's module.
const INLINE: &[&str] = &["inline/v1/inline.proto"];

fn main() -> Result<()> {
    let manifest = PathBuf::from(std::env::var("CARGO_MANIFEST_DIR")?);
    let out = PathBuf::from(std::env::var("OUT_DIR")?);
    // This crate lives at `<plugin>/tests/fixture`, so the schema is its
    // sibling and the plugin root is two levels up.
    let tests = manifest.parent().context("fixture sits under tests/")?;
    let plugin = tests
        .parent()
        .context("tests/ sits under the plugin root")?;
    let schema_root = tests.join("proto");
    let includes = [path(&schema_root), path(&protoc_types_protos(plugin)?)];
    let includes: Vec<&str> = includes.iter().map(String::as_str).collect();

    for file in SCHEMA.iter().chain(INLINE) {
        println!(
            "cargo:rerun-if-changed={}",
            schema_root.join(file).display()
        );
    }
    println!("cargo:rerun-if-env-changed=PROTOC");

    generate(
        &schema_root,
        &includes,
        SCHEMA,
        &out.join("tree"),
        &emit::Options::default(),
    )?;
    generate(
        &schema_root,
        &includes,
        INLINE,
        &out.join("inline"),
        &emit::Options {
            packaging: false,
            ..emit::Options::default()
        },
    )
}

/// Compiles `schema` with buffa into `out/proto` and with this plugin into
/// `out/defaults`.
fn generate(
    schema_root: &Path,
    includes: &[&str],
    schema: &[&str],
    out: &Path,
    options: &emit::Options,
) -> Result<()> {
    let files: Vec<String> = schema
        .iter()
        .map(|file| path(&schema_root.join(file)))
        .collect();
    let files: Vec<&str> = files.iter().map(String::as_str).collect();

    let proto = out.join("proto");
    std::fs::create_dir_all(&proto)?;
    buffa_build::Config::new()
        .files(&files)
        .includes(includes)
        .out_dir(&proto)
        // The options are read by the plugin, not used as field types.
        .exclude_package("protoc.contrib.defaults")
        .exclude_package("google.protobuf")
        .generate_views(false)
        .include_file("_include.rs")
        .compile()
        .map_err(|error| anyhow::anyhow!("buffa codegen: {error}"))?;

    let request = request(schema_root, includes, schema, out)?;
    let mut config = CodeGenConfig::default();
    config.extern_paths.clone_from(&options.extern_paths);
    let index = Schema::new(
        &request,
        &config,
        options.packaging.then_some(options.proto_module.as_str()),
    );
    let messages = messages::gather(&request, &index)?;
    let generated = emit::render(&messages, options)?;

    let defaults = out.join("defaults");
    std::fs::create_dir_all(&defaults)?;
    for file in generated {
        let name = file.name.context("generated file has a name")?;
        let content = file.content.unwrap_or_default();
        std::fs::write(defaults.join(&name), content).with_context(|| format!("write {name}"))?;
    }
    Ok(())
}

/// Builds the `CodeGeneratorRequest` the plugin would have received on stdin,
/// out of a descriptor set `protoc` writes for the schema.
fn request(
    schema_root: &Path,
    includes: &[&str],
    schema: &[&str],
    out: &Path,
) -> Result<CodeGeneratorRequest> {
    let descriptors = out.join("schema.bin");
    let protoc = std::env::var("PROTOC").unwrap_or_else(|_| "protoc".to_owned());
    let mut command = Command::new(protoc);
    command
        .arg("--include_imports")
        .arg(format!("--descriptor_set_out={}", descriptors.display()));
    for include in includes {
        command.arg(format!("-I{include}"));
    }
    for file in schema {
        command.arg(schema_root.join(file));
    }
    let output = command.output().context("run protoc")?;
    if !output.status.success() {
        bail!("protoc: {}", String::from_utf8_lossy(&output.stderr));
    }

    let set = FileDescriptorSet::decode_from_slice(&std::fs::read(&descriptors)?)?;
    Ok(CodeGeneratorRequest {
        file_to_generate: schema.iter().map(|file| (*file).to_owned()).collect(),
        proto_file: set.file.into_iter().collect::<Vec<FileDescriptorProto>>(),
        ..Default::default()
    })
}

/// The `proto/` root of the protoc-types checkout the plugin reads its options
/// through, where the schema's `protoc/contrib/defaults/options.proto` import
/// resolves: the schema is compiled against the options the plugin was built
/// with, at the rev `Cargo.lock` pins, rather than a copy that can drift.
///
/// Cargo tells a build script nothing about where a dependency's source is,
/// so ask `cargo metadata`. Not `--offline`: it resolves the whole lockfile,
/// optional dependencies no feature enables included, and those were never
/// downloaded for this build.
fn protoc_types_protos(plugin: &Path) -> Result<PathBuf> {
    let cargo = std::env::var("CARGO").unwrap_or_else(|_| "cargo".to_owned());
    let output = Command::new(cargo)
        .args(["metadata", "--format-version=1", "--locked"])
        .arg("--manifest-path")
        .arg(plugin.join("Cargo.toml"))
        .output()
        .context("run cargo metadata")?;
    if !output.status.success() {
        bail!(
            "cargo metadata: {}",
            String::from_utf8_lossy(&output.stderr)
        );
    }

    let metadata: serde_json::Value = serde_json::from_slice(&output.stdout)?;
    let manifest = metadata["packages"]
        .as_array()
        .context("cargo metadata lists packages")?
        .iter()
        .find(|package| package["name"] == "protoc-types")
        .and_then(|package| package["manifest_path"].as_str())
        .context("cargo metadata lists protoc-types")?;
    Ok(Path::new(manifest)
        .parent()
        .context("a manifest sits in its package")?
        .join("proto"))
}

fn path(path: &Path) -> String {
    path.to_str().expect("utf-8 path").to_owned()
}
