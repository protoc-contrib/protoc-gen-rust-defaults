//! `src/generated` is buffa's output for the `options.proto` beside this
//! crate, committed so depending on it needs no `protoc`. This regenerates it and fails if the
//! committed copy has drifted -- from the proto, or from the buffa it targets.
//!
//! Regenerate with `UPDATE_GENERATED=1 cargo test -p protoc-contrib-defaults
//! --test generated` and review the diff.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

const OPTIONS: &str = "protoc_contrib/defaults/options.proto";

#[test]
fn committed_code_matches_a_fresh_generation() {
    let manifest = Path::new(env!("CARGO_MANIFEST_DIR"));
    // The crate sits beside the proto at `<repo>/protoc_contrib/defaults`, so
    // the repository root is the proto's include root.
    let proto_root = manifest.join("../..");
    let committed = manifest.join("src/generated");
    let fresh = Path::new(env!("CARGO_TARGET_TMPDIR")).join("protoc-contrib-defaults");
    let _ = std::fs::remove_dir_all(&fresh);
    std::fs::create_dir_all(&fresh).unwrap();

    let include = protoc_include()
        .expect("google/protobuf/descriptor.proto not found; install protoc or set PROTOC");
    buffa_build::Config::new()
        .files(&[path(&proto_root.join(OPTIONS))])
        .includes(&[path(&proto_root), path(&include)])
        .out_dir(&fresh)
        // `descriptor.proto` is imported only to be extended -- an extendee is
        // named by string, so nothing here refers to a `google.protobuf` type,
        // and generating it would fork the descriptor types buffa-codegen
        // reads options off into a second, incompatible copy. Nothing reads
        // the options without owning them first, so views are dead weight.
        .exclude_package("google.protobuf")
        .generate_views(false)
        .include_file("_include.rs")
        .compile()
        .expect("buffa codegen");

    if std::env::var_os("UPDATE_GENERATED").is_some() {
        let _ = std::fs::remove_dir_all(&committed);
        std::fs::create_dir_all(&committed).unwrap();
        for name in files(&fresh) {
            std::fs::copy(fresh.join(&name), committed.join(&name)).unwrap();
        }
        return;
    }

    let names: BTreeSet<_> = files(&fresh).union(&files(&committed)).cloned().collect();
    let stale: Vec<_> = names
        .into_iter()
        .filter(|name| {
            std::fs::read_to_string(fresh.join(name)).ok()
                != std::fs::read_to_string(committed.join(name)).ok()
        })
        .collect();
    assert!(
        stale.is_empty(),
        "src/generated is out of date: {stale:?}\n\
         rerun with UPDATE_GENERATED=1 and review the diff"
    );
}

fn files(directory: &Path) -> BTreeSet<PathBuf> {
    std::fs::read_dir(directory)
        .into_iter()
        .flatten()
        .filter_map(Result::ok)
        .map(|entry| PathBuf::from(entry.file_name()))
        .collect()
}

/// The directory holding protoc's bundled well-known types, found by stepping
/// from the `protoc` binary up to its prefix and back down into `include/`.
/// A symlinked protoc -- a Homebrew shim, say -- has its includes beside the
/// real binary rather than beside the link.
fn protoc_include() -> Option<PathBuf> {
    let protoc = std::env::var_os("PROTOC").map(PathBuf::from).or_else(|| {
        let path = std::env::var_os("PATH")?;
        std::env::split_paths(&path)
            .map(|directory| directory.join("protoc"))
            .find(|candidate| candidate.is_file())
    })?;
    let protoc = std::fs::canonicalize(&protoc).unwrap_or(protoc);
    protoc
        .parent()
        .and_then(Path::parent)
        .map(|prefix| prefix.join("include"))
        .filter(|include| include.join("google/protobuf/descriptor.proto").exists())
}

fn path(path: &Path) -> String {
    path.to_str().expect("utf-8 path").to_owned()
}
