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
use std::sync::atomic::{AtomicUsize, Ordering};

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
    plugin_dir_with(name, manifest, module, "")
}

/// [`plugin_dir`] with `tier1` more lines in the `[tier1]` table (`fuel =
/// …`).
pub fn plugin_dir_with(name: &str, manifest: &str, module: &[u8], tier1: &str) -> PathBuf {
    let dir = target_dir().join("plugins").join(name);
    std::fs::create_dir_all(&dir).unwrap();
    let manifest = manifest.replace("tier = 0", "tier = 1");
    let manifest = format!("{manifest}\n[tier1]\ncomponent = \"{name}.wasm\"\n{tier1}\n");
    let put = |file: &str, bytes: &[u8]| {
        static WRITES: AtomicUsize = AtomicUsize::new(0);
        let n = WRITES.fetch_add(1, Ordering::Relaxed);
        let tmp = dir.join(format!(".{file}.{}.{n}", std::process::id()));
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

/// The misbehaving test plugin's core module (`tests/plugins/misbehave`,
/// outside the workspace), built once per test process.
pub fn misbehave_module() -> &'static [u8] {
    static MODULE: OnceLock<Vec<u8>> = OnceLock::new();
    MODULE.get_or_init(|| {
        let manifest = root().join("crates/arrix-plugin-host/tests/plugins/misbehave/Cargo.toml");
        build(
            &["--manifest-path", manifest.to_str().unwrap()],
            "misbehave",
        )
    })
}

/// The misbehaving plugin as a tier-1 directory named `name`, with the
/// `fuel` its manifest sets and `kernel` granted or not.
pub fn misbehave(name: &str, fuel: u64, kernel: bool) -> PathBuf {
    let manifest = std::fs::read_to_string(
        root().join("crates/arrix-plugin-host/tests/plugins/misbehave/arrix-plugin.toml"),
    )
    .unwrap()
    .replace("kernel = true", &format!("kernel = {kernel}"));
    plugin_dir_with(
        name,
        &manifest,
        misbehave_module(),
        &format!("fuel = {fuel}"),
    )
}
