//! The generated code, compared against `tests/golden`.
//!
//! The other tests check that the output compiles and behaves; this one makes
//! a change to what is emitted visible in review, where a behaviour-preserving
//! change in shape would otherwise pass unseen.
//!
//! Regenerate with `UPDATE_GOLDEN=1 cargo test -p defaults-fixture --test
//! golden` and review the diff.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

/// Where each generated tree is compared from, and against.
const TREES: &[(&str, &str)] = &[("tree/defaults", "tree"), ("inline/defaults", "inline")];

#[test]
fn generated_code_matches_the_golden_files() {
    let golden_root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../golden");
    let update = std::env::var_os("UPDATE_GOLDEN").is_some();
    let mut stale = Vec::new();

    for (generated, golden) in TREES {
        let generated = Path::new(env!("OUT_DIR")).join(generated);
        let golden = golden_root.join(golden);
        if update {
            // Removed first, so a file the plugin no longer emits goes too.
            let _ = std::fs::remove_dir_all(&golden);
            std::fs::create_dir_all(&golden).unwrap();
        }

        let names: BTreeSet<_> = files(&generated).union(&files(&golden)).cloned().collect();
        for name in names {
            let actual = std::fs::read_to_string(generated.join(&name)).ok();
            if update {
                if let Some(actual) = &actual {
                    std::fs::write(golden.join(&name), actual).unwrap();
                }
                continue;
            }
            let expected = std::fs::read_to_string(golden.join(&name)).ok();
            if actual != expected {
                stale.push(golden.join(&name));
            }
        }
    }

    assert!(
        stale.is_empty(),
        "generated code differs from:\n{}\nrerun with UPDATE_GOLDEN=1 and review the diff",
        stale
            .iter()
            .map(|path| format!("  {}", path.display()))
            .collect::<Vec<_>>()
            .join("\n")
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
