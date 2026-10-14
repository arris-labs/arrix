//! The first-party Python plugins' own unit tests (`plugins/<name>/tests`,
//! `python -m unittest`), run here so the gate covers them (plans/c3-m3
//! step 9): `fasteners`' table and its `place` expansion, against a fake
//! snapshot.

use std::path::PathBuf;
use std::process::Command;

#[test]
fn fasteners_unittests_pass() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let out = Command::new("python3")
        .args(["-m", "unittest", "discover", "-s", "tests"])
        .current_dir(root.join("plugins/fasteners"))
        .env("PYTHONPATH", root.join("python"))
        .output()
        .expect("python3 runs");
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
}
