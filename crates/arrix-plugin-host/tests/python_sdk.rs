//! The Python SDK's own tests (`python/tests`, `python -m unittest`), run
//! here so the gate covers them (plans/boundary-m1 step 7). They play a
//! fake host over real pipes against a sample plugin.

use std::path::PathBuf;
use std::process::Command;

#[test]
fn the_sdks_unittests_pass() {
    let python = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../python");
    let out = Command::new("python3")
        .args(["-m", "unittest", "discover", "-s", "tests"])
        .current_dir(&python)
        .env("PYTHONPATH", &python)
        .output()
        .expect("python3 runs");
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
}
