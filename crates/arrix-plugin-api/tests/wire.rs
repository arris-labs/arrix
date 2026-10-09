//! The wire generated from the WIT world (ADR-0022; docs/PLUGINS.md §Three
//! tiers), held fresh.
//!
//! One reading of `wit/arrix-plugin.wit` makes every artefact of the wire:
//! the JSON Schema of each type and of each function (`wire/`), and the
//! Rust codec of `arrix-plugin-host` (`src/wire/generated.rs`). A WIT
//! change that is not followed by a regeneration fails here, and so does a WIT item the generators cannot
//! carry. Regenerate with
//! `UPDATE_SNAPSHOTS=1 cargo test -p arrix-plugin-api --test wire`, then
//! read the diff.

use std::path::PathBuf;

#[path = "wire/model.rs"]
mod model;
#[path = "wire/python.rs"]
mod python;
#[path = "wire/rust.rs"]
mod rust;
#[path = "wire/schema.rs"]
mod schema;

fn crate_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

/// Every generated file, by path from the workspace root.
fn outputs(world: &model::World) -> Vec<(PathBuf, String)> {
    vec![
        (
            "crates/arrix-plugin-api/wire/schema.json".into(),
            schema::schema_json(world),
        ),
        (
            "crates/arrix-plugin-api/wire/methods.json".into(),
            schema::methods_json(world),
        ),
        ("python/arrix/_wire.py".into(), python::generate(world)),
        (
            "crates/arrix-plugin-host/src/wire/generated.rs".into(),
            rust::generate(world),
        ),
    ]
}

#[test]
fn the_wire_is_what_the_world_generates() {
    let world = model::World::read(&crate_dir().join("wit"));
    assert_eq!(world.version, arrix_plugin_api::API_VERSION);
    let root = crate_dir().join("../..");
    let update = std::env::var_os("UPDATE_SNAPSHOTS").is_some();
    let mut stale = Vec::new();
    for (path, text) in outputs(&world) {
        let file = root.join(&path);
        if update {
            std::fs::create_dir_all(file.parent().unwrap()).unwrap();
            std::fs::write(&file, &text).unwrap();
        } else if std::fs::read_to_string(&file).ok().as_deref() != Some(text.as_str()) {
            stale.push(path);
        }
    }
    assert!(
        stale.is_empty(),
        "stale, regenerate with UPDATE_SNAPSHOTS=1 and read the diff: {stale:?}"
    );
}

/// The generated files are the wire directory's only content, so a file a
/// removed output left behind is seen.
#[test]
fn the_wire_directory_holds_only_generated_files() {
    let world = model::World::read(&crate_dir().join("wit"));
    let expected: Vec<String> = outputs(&world)
        .into_iter()
        .filter_map(|(p, _)| {
            p.strip_prefix("crates/arrix-plugin-api/wire")
                .ok()
                .map(|p| p.to_string_lossy().trim_start_matches('/').to_owned())
        })
        .collect();
    let mut found: Vec<String> = std::fs::read_dir(crate_dir().join("wire"))
        .into_iter()
        .flatten()
        .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
        .collect();
    found.sort();
    assert_eq!(found, {
        let mut e = expected;
        e.sort();
        e
    });
}
