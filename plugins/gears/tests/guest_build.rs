//! `gears` as a Tier 1 component's core module (ADR-0035): built for
//! `wasm32-unknown-unknown` with `--features guest` from this workspace's
//! manifest, in a target directory of its own (`target/guest`: the outer
//! `cargo test` holds `target`'s lock), and
//! checked to export the world's `feature` interface.
//! `arrix-plugin-host`'s Tier 1 tests instantiate it.

use std::path::Path;
use std::process::Command;

#[test]
fn gears_builds_as_a_core_module_exporting_the_feature_world() {
    let manifest_dir = Path::new(env!("CARGO_MANIFEST_DIR"));
    let target_dir = manifest_dir.join("target/guest");
    let status = Command::new(env!("CARGO"))
        .args(["build", "--release", "--target", "wasm32-unknown-unknown"])
        .args(["--features", "guest", "--manifest-path"])
        .arg(manifest_dir.join("Cargo.toml"))
        .arg("--target-dir")
        .arg(&target_dir)
        .current_dir(manifest_dir)
        .env_remove("RUSTFLAGS")
        .env_remove("CARGO_ENCODED_RUSTFLAGS")
        .status()
        .expect("cargo runs");
    assert!(status.success(), "gears builds for wasm32 with `guest`");

    let wasm = std::fs::read(target_dir.join("wasm32-unknown-unknown/release/arrix_gears.wasm"))
        .expect("the cdylib is written");
    assert_eq!(&wasm[..4], b"\0asm", "a wasm module");
    let has = |name: &str| wasm.windows(name.len()).any(|w| w == name.as_bytes());
    for export in ["describe", "evaluate"] {
        assert!(
            has(&format!("arrix:plugin/feature@0.8.0#{export}")),
            "exports {export}"
        );
    }
}
