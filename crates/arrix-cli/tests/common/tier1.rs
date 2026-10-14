//! Tier 1 plugins for the CLI's tests (plans/c3-m1 steps 10 and 12): the
//! host's builder of components (`arrix-plugin-host/tests/support`), `gears`
//! with its fixtures, and a one-feature document of the `height` plugin.

#[path = "../../../arrix-plugin-host/tests/support/tier1.rs"]
pub mod build;

use std::path::PathBuf;

use arrix_doc::MemorySource;

use super::{copy_tree, workspace};

/// `gears` at Tier 1 with the fixtures its `[test]` table names, in a
/// directory of its own (`arrix test` reads the fixtures beside the
/// manifest).
pub fn gears() -> PathBuf {
    with_fixtures(build::gears(), "gears")
}

/// `robotics` at Tier 1 with the fixtures its `[test]` table names.
pub fn robotics() -> PathBuf {
    with_fixtures(build::robotics(), "robotics")
}

/// The Tier 1 directory `built` of `plugins/<name>`, with that plugin's
/// `fixtures` beside its manifest, in a directory of its own.
fn with_fixtures(built: &std::path::Path, name: &str) -> PathBuf {
    let dir = built.with_file_name(format!("{name}-with-fixtures"));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    for file in ["arrix-plugin.toml".to_owned(), format!("{name}.wasm")] {
        std::fs::copy(built.join(&file), dir.join(&file)).unwrap();
    }
    copy_tree(
        &workspace().join(format!("plugins/{name}/fixtures")),
        &dir.join("fixtures"),
    );
    dir
}

/// A copy of the plugin directory `from` at `to`, replaced if it exists.
pub fn copy_plugin(from: &std::path::Path, name: &str) -> PathBuf {
    let to = std::env::temp_dir().join(format!("arrix-tier1-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&to);
    copy_tree(from, &to);
    to
}

/// A one-part document with a feature of each of `features` (id, type),
/// none with inputs.
pub fn document_of(features: &[(&str, &str)]) -> MemorySource {
    let records: serde_json::Map<_, _> = features
        .iter()
        .map(|(id, ty)| {
            (
                (*id).to_owned(),
                serde_json::json!({
                    "id": id, "inputs": {}, "name": ty, "params": {},
                    "suppressed": false, "type_id": ty, "type_version": 1,
                }),
            )
        })
        .collect();
    let history: Vec<_> = features.iter().map(|(id, _)| *id).collect();
    let part = serde_json::json!({
        "features": records, "history": history, "id": "2000000000001",
        "name": "Tier 1", "rollback": null,
    });
    let mut files = MemorySource::default();
    for (path, json) in [
        ("document.json", serde_json::json!({ "schema": 5 })),
        ("params.json", serde_json::json!({})),
        ("parts/2000000000001.json", part),
    ] {
        files
            .0
            .insert(path.to_owned(), serde_json::to_vec_pretty(&json).unwrap());
    }
    files
}

/// A one-part document holding one `height.block`.
pub fn height_document() -> MemorySource {
    document_of(&[("1000000000001", "height.block")])
}

/// [`height_document`] written to a directory of its own.
pub fn height_document_dir(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("arrix-tier1-doc-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    for (path, bytes) in &height_document().0 {
        let to = dir.join(path);
        std::fs::create_dir_all(to.parent().unwrap()).unwrap();
        std::fs::write(to, bytes).unwrap();
    }
    dir
}
