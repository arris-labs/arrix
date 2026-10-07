//! The command registry: the core table, one creation command per feature
//! type, and a tab per plugin (ADR-0015).

use arrix_config::{CommandRegistry, PALETTE_OPEN};
use arrix_plugin_api::FeatureTypeSpec;

fn ty(id: &str, title: &str) -> FeatureTypeSpec {
    FeatureTypeSpec {
        id: id.into(),
        version: 1,
        title: title.into(),
        params: Vec::new(),
        inputs: Vec::new(),
        outputs: Vec::new(),
        choices: vec![],
    }
}

fn registry() -> CommandRegistry {
    CommandRegistry::new(&[
        ty("core.sketch", "Sketch"),
        ty("core.extrude", "Extrude"),
        ty("gears.spur", "Spur gear"),
    ])
}

fn placed(r: &CommandRegistry, id: &str) -> (String, String) {
    let p = r
        .command(id)
        .and_then(|c| c.ribbon.clone())
        .unwrap_or_else(|| panic!("`{id}` is not on the ribbon"));
    (p.tab, p.group)
}

#[test]
fn a_creation_command_per_feature_type_in_its_tab() {
    let r = registry();
    assert_eq!(
        placed(&r, "core.sketch.create"),
        ("core.sketch".into(), "Sketch".into())
    );
    assert_eq!(
        placed(&r, "core.extrude.create"),
        ("core.part".into(), "Features".into())
    );
    assert_eq!(
        placed(&r, "gears.spur.create"),
        ("gears".into(), "Features".into())
    );
    assert!(r.command("gears.spur.create").unwrap().needs_document);
}

#[test]
fn a_plugin_adds_a_tab_after_the_core_ones() {
    let tabs: Vec<_> = registry()
        .tabs
        .iter()
        .map(|t| (t.id.clone(), t.title.clone()))
        .collect();
    assert_eq!(
        tabs,
        [
            ("core.sketch".into(), "Sketch".into()),
            ("core.part".into(), "Part".into()),
            ("gears".into(), "Gears".into()),
        ]
    );
}

#[test]
fn every_tab_has_a_command_and_ids_are_unique() {
    let r = registry();
    for id in [
        "core.ribbon.sketch",
        "core.ribbon.part",
        "core.ribbon.gears",
    ] {
        assert!(r.command(id).is_some(), "{id}");
    }
    let mut ids: Vec<_> = r.commands.iter().map(|c| c.id.as_str()).collect();
    let n = ids.len();
    ids.sort_unstable();
    ids.dedup();
    assert_eq!(ids.len(), n, "a command id repeats");
}

#[test]
fn the_palette_opens_on_ctrl_k_by_default() {
    assert_eq!(registry().command(PALETTE_OPEN).unwrap().keys, ["ctrl+k"]);
}
