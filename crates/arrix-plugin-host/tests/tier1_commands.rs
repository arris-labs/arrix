//! Commands at Tier 1 (plans/c3-m3 step 7, ADR-0038): the `place` test
//! commands built as a component expand `tests/docs/assembly-exchange` to
//! the group they give compiled in (the golden `commands.rs` holds), a
//! plugin's own refusal and panic are the command's diagnostics, and the
//! `document` import is refused without `document-read`, the commands
//! without `document-write`.

#[path = "support/tier1.rs"]
mod tier1_build;

use std::collections::BTreeMap;
use std::sync::Arc;

use arrix_core::{AssemblyId, ComponentId, Id, PersistentName};
use arrix_doc::{Command, Document, EdgePick, Evaluator, Expansion, MemorySource, Registry, open};
use arrix_plugin_api::{Context, Diagnostic};
use arrix_plugin_host::{register_tier0_commands, register_tier1};

#[path = "plugins/place/src/place.rs"]
mod place;

fn manifest() -> String {
    std::fs::read_to_string(
        tier1_build::root().join("crates/arrix-plugin-host/tests/plugins/place/arrix-plugin.toml"),
    )
    .unwrap()
}

fn tier0() -> Registry {
    let mut r = Registry::with_core_types();
    register_tier0_commands(&mut r, &manifest(), Arc::new(place::Place)).unwrap();
    r
}

fn tier1() -> Registry {
    let mut r = Registry::with_core_types();
    register_tier1(&mut r, &tier1_build::place("place-granted", true, true)).unwrap();
    r
}

fn files(name: &str) -> MemorySource {
    let dir = tier1_build::root().join("tests/docs").join(name);
    let mut files = MemorySource::default();
    let mut pending = vec![dir.clone()];
    while let Some(d) = pending.pop() {
        for entry in std::fs::read_dir(&d).unwrap() {
            let path = entry.unwrap().path();
            if path.is_dir() {
                pending.push(path);
            } else {
                let rel = path.strip_prefix(&dir).unwrap().to_str().unwrap();
                files
                    .0
                    .insert(rel.replace('\\', "/"), std::fs::read(&path).unwrap());
            }
        }
    }
    files
}

const SIDES: [&str; 2] = ["3ZPNYGV5VXAGH", "57Z6PA4YG5FQY"];

/// `command` expanded on both bracket edges of the exchange assembly.
fn expand(registry: &Registry, command: &str, n_ids: u64) -> Result<Command, Diagnostic> {
    let document: Document = open(&files("assembly-exchange")).unwrap();
    let mut evaluator = Evaluator::new(registry.clone());
    let evaluation = evaluator.evaluate(&document);
    let assembly: AssemblyId = *document.assemblies().keys().next().unwrap();
    let bracket: ComponentId = "C4VR764XTFX0Z".parse().unwrap();
    let picks: Vec<_> = SIDES
        .iter()
        .map(|side| EdgePick {
            path: vec![bracket],
            edge: format!("edge:sweep.EBWK5C332393W.end-edge.{side}")
                .parse::<PersistentName>()
                .unwrap(),
        })
        .collect();
    let picks = BTreeMap::from([("edges".to_owned(), picks)]);
    let none = BTreeMap::new();
    let ids: Vec<_> = (1..=n_ids).map(|i| Id(1000 + i)).collect();
    let command = registry.command(command).expect("registered").clone();
    evaluator.expand(
        &document,
        &evaluation,
        &Expansion {
            command: command.as_ref(),
            context: Context::Assembly(assembly),
            params: &none,
            choices: &none,
            picks: &picks,
            ids: &ids,
        },
    )
}

#[test]
fn a_component_expands_to_the_group_it_gives_compiled_in() {
    let (t0, t1) = (tier0(), tier1());
    let group = expand(&t1, "place.block", 5).unwrap();
    assert_eq!(group, expand(&t0, "place.block", 5).unwrap());
    // And to the byte, which the golden of `commands.rs` holds for Tier 0.
    let golden = std::fs::read_to_string(
        tier1_build::root().join("crates/arrix-plugin-host/tests/golden/commands-place-block.json"),
    )
    .unwrap();
    assert_eq!(serde_json::to_string_pretty(&group).unwrap() + "\n", golden);
    assert_eq!(expand(&t1, "place.block", 5).unwrap(), group, "pure");
}

#[test]
fn a_refusal_and_a_panic_are_the_commands_diagnostics() {
    let t1 = tier1();
    let code = |c: &str, ids| expand(&t1, c, ids).unwrap_err().code.to_string();
    assert_eq!(code("place.refuse", 5), "place.refused");
    assert_eq!(code("place.panic", 5), "plugin.trap");
    assert_eq!(code("place.rogue", 5), "command.invalid");
}

#[test]
fn the_document_import_needs_document_read_and_commands_need_document_write() {
    let mut r = Registry::with_core_types();
    let err = register_tier1(&mut r, &tier1_build::place("place-blind", false, true)).unwrap_err();
    assert!(err.to_string().contains("document-read"), "{err}");
    let err =
        register_tier1(&mut r, &tier1_build::place("place-read-only", true, false)).unwrap_err();
    assert!(err.to_string().contains("document-write"), "{err}");
    assert!(r.command("place.block").is_none(), "none of them");
}
