//! The Python SDK's own tests (`python/tests`, `python -m unittest`), run
//! here so the gate covers them (plans/boundary-m1 step 7). `test_sdk` plays
//! a fake host over real pipes against a sample plugin; `test_session` drives
//! the built `arrix` through `arrix.session` and `arrix.eval`
//! (plans/boundary-m3 step 7).

use std::path::{Path, PathBuf};
use std::process::Command;

/// The built `arrix` binary, which another package's tests cannot name:
/// `cargo build` puts it beside this test's `deps` directory.
fn arrix() -> PathBuf {
    let cargo = std::env::var("CARGO").unwrap_or_else(|_| "cargo".into());
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let built = Command::new(cargo)
        .args(["build", "-q", "-p", "arrix-cli"])
        .current_dir(&root)
        .status()
        .expect("cargo runs");
    assert!(built.success(), "arrix builds");
    let exe = std::env::current_exe().unwrap();
    let debug = exe.parent().and_then(Path::parent).unwrap();
    debug.join("arrix")
}

#[test]
fn the_sdks_unittests_pass() {
    let python = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../python");
    let out = Command::new("python3")
        .args(["-m", "unittest", "discover", "-s", "tests"])
        .current_dir(&python)
        .env("PYTHONPATH", &python)
        .env("ARRIX", arrix())
        .output()
        .expect("python3 runs");
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    // The session tests are not skipped: `ARRIX` was set.
    let report = String::from_utf8_lossy(&out.stderr);
    assert!(!report.contains("skipped"), "{report}");
}
