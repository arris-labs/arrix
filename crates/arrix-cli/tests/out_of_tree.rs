//! The first-party plugins out of tree (plans/c3-m3 step 11, ADR-0034):
//! `arrix` installed by `cargo install` into `target/out-of-tree`, and its
//! `arrix test` run on a copy of each plugin's directory outside the
//! repository's layout, with nothing but the Python SDK on `PYTHONPATH`.
//! `gears` and `robotics` also pass as Tier 1 components built from their
//! own workspaces.

mod common;

use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::OnceLock;

use common::tier1;
use common::{copy_tree, workspace};
use serde_json::Value;

fn out_of_tree() -> PathBuf {
    workspace().join("target/out-of-tree")
}

/// `arrix`, installed once per test process. A debug build in a target
/// directory of its own, so a rerun is incremental and never waits on the
/// lock of the `cargo test` running this.
fn installed() -> &'static Path {
    static BIN: OnceLock<PathBuf> = OnceLock::new();
    BIN.get_or_init(|| {
        let root = out_of_tree();
        let status = Command::new(env!("CARGO"))
            .args(["install", "--locked", "--debug", "--force", "--path"])
            .arg(workspace().join("crates/arrix-cli"))
            .arg("--root")
            .arg(&root)
            .arg("--target-dir")
            .arg(root.join("build"))
            .env_remove("RUSTFLAGS")
            .env_remove("CARGO_ENCODED_RUSTFLAGS")
            .status()
            .expect("cargo runs");
        assert!(status.success(), "`cargo install` installs arrix");
        root.join("bin")
            .join(format!("arrix{}", std::env::consts::EXE_SUFFIX))
    })
}

/// A copy of the plugin directory `from` (relative to the workspace) under
/// `target/out-of-tree/plugins`, without its build output or bytecode.
fn copied(from: &str) -> PathBuf {
    let name = Path::new(from).file_name().unwrap();
    let dir = out_of_tree().join("plugins").join(name);
    let _ = std::fs::remove_dir_all(&dir);
    copy_tree(&workspace().join(from), &dir);
    for skip in ["target", "__pycache__", "tests/__pycache__"] {
        let _ = std::fs::remove_dir_all(dir.join(skip));
    }
    dir
}

/// `arrix test` on `dir` from the installed binary, in `dir`, every line
/// of its report passing.
fn passes(dir: &Path) {
    let out = Command::new(installed())
        .current_dir(dir)
        .env("PYTHONPATH", workspace().join("python"))
        .arg("test")
        .arg(dir)
        .output()
        .expect("run the installed arrix");
    let report: Vec<Value> = String::from_utf8_lossy(&out.stdout)
        .lines()
        .map(|l| serde_json::from_str(l).expect("a JSON line"))
        .collect();
    assert!(
        out.status.success() && !report.is_empty(),
        "{}: {report:?}{}",
        dir.display(),
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(
        report.iter().all(|l| l["status"] == "pass"),
        "{}: {report:?}",
        dir.display()
    );
}

#[test]
fn every_plugin_directory_passes_from_the_installed_arrix() {
    for plugin in [
        "plugins/gears",
        "plugins/robotics",
        "plugins/fasteners",
        "python/examples/standoff",
        "python/examples/bom",
    ] {
        passes(&copied(plugin));
    }
}

#[test]
fn gears_and_robotics_pass_as_components_built_from_their_workspaces() {
    for dir in [tier1::gears(), tier1::robotics()] {
        passes(&dir);
    }
}
