//! `gears` as a Tier 1 component's core module (ADR-0035): built for
//! `wasm32-unknown-unknown` with `--features guest`, in the target
//! directory of the Tier 1 test plugins (`target/tier1-fixtures`), and
//! checked to export the world's `feature` interface.
//! `arrix-plugin-host`'s Tier 1 tests instantiate it.

use std::path::Path;
use std::process::Command;

#[test]
fn gears_builds_as_a_core_module_exporting_the_feature_world() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let target_dir = root.join("target/tier1-fixtures");
    let status = Command::new(env!("CARGO"))
        .args(["build", "--release", "--target", "wasm32-unknown-unknown"])
        .args(["-p", "arrix-gears", "--features", "guest", "--target-dir"])
        .arg(&target_dir)
        .current_dir(&root)
        .status()
        .expect("cargo runs");
    assert!(status.success(), "gears builds for wasm32 with `guest`");

    let wasm = std::fs::read(target_dir.join("wasm32-unknown-unknown/release/arrix_gears.wasm"))
        .expect("the cdylib is written");
    assert_eq!(&wasm[..4], b"\0asm", "a wasm module");
    let has = |name: &str| wasm.windows(name.len()).any(|w| w == name.as_bytes());
    for export in ["describe", "evaluate"] {
        assert!(
            has(&format!("arrix:plugin/feature@0.7.0#{export}")),
            "exports {export}"
        );
    }
}
