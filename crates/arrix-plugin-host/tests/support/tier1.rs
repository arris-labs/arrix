//! Tier 1 plugins for tests (ADR-0035): a Rust plugin built for
//! `wasm32-unknown-unknown` with its `guest` feature by a nested cargo, in
//! `target/tier1-fixtures`, and laid out as a tier-1 plugin directory
//! there, its manifest's tier set to 1 and its `[tier1]` table naming the
//! module. Cargo's own cache keeps a rebuild warm. Included by `#[path]`
//! wherever a test needs one.

#![allow(dead_code)]

use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::OnceLock;

pub fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn target_dir() -> PathBuf {
    root().join("target/tier1-fixtures")
}

/// Builds `package` for wasm32 with `--features guest` (`args` say where
/// it is: `-p <name>` in the workspace, or `--manifest-path`) and returns
/// its core module.
pub fn build(args: &[&str], package: &str) -> Vec<u8> {
    let status = Command::new(env!("CARGO"))
        .args(["build", "--release", "--target", "wasm32-unknown-unknown"])
        .args(["--features", "guest", "--target-dir"])
        .arg(target_dir())
        .args(args)
        .current_dir(root())
        .env_remove("RUSTFLAGS")
        .env_remove("CARGO_ENCODED_RUSTFLAGS")
        .status()
        .expect("cargo runs");
    assert!(status.success(), "{package} builds for wasm32 with `guest`");
    let wasm = format!(
        "wasm32-unknown-unknown/release/{}.wasm",
        package.replace('-', "_")
    );
    std::fs::read(target_dir().join(wasm)).expect("the cdylib is written")
}

/// A tier-1 plugin directory named `name`: `manifest` (a tier-0 one)
/// made tier 1, and `module` as `<name>.wasm`. Each file is written whole
/// and renamed into place, so tests in other processes never read half.
pub fn plugin_dir(name: &str, manifest: &str, module: &[u8]) -> PathBuf {
    let dir = target_dir().join("plugins").join(name);
    std::fs::create_dir_all(&dir).unwrap();
    let manifest = manifest.replace("tier = 0", "tier = 1");
    let manifest = format!("{manifest}\n[tier1]\ncomponent = \"{name}.wasm\"\n");
    let put = |file: &str, bytes: &[u8]| {
        let tmp = dir.join(format!(".{file}.{}", std::process::id()));
        std::fs::write(&tmp, bytes).unwrap();
        std::fs::rename(&tmp, dir.join(file)).unwrap();
    };
    put(&format!("{name}.wasm"), module);
    put("arrix-plugin.toml", manifest.as_bytes());
    dir
}

/// `gears` at Tier 1, built once per test process.
pub fn gears() -> &'static Path {
    static DIR: OnceLock<PathBuf> = OnceLock::new();
    DIR.get_or_init(|| {
        let module = build(&["-p", "arrix-gears"], "arrix-gears");
        let manifest =
            std::fs::read_to_string(root().join("plugins/gears/arrix-plugin.toml")).unwrap();
        plugin_dir("gears", &manifest, &module)
    })
}
