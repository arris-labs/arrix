//! Commands at Tier 2 (plans/c3-m3 step 8, ADR-0038): `place.py`, the
//! `place` test commands in Python, expands `tests/docs/assembly-exchange`
//! to the group the Tier 0 commands give, to the byte (the golden
//! `commands.rs` holds); a refusal and a bug are the command's
//! diagnostics; a plugin without `document-write` is refused where it
//! describes its commands, and one without `document-read` cannot read
//! the snapshot while it expands.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use arrix_core::{AssemblyId, ComponentId, Id, PersistentName};
use arrix_doc::{Command, Document, EdgePick, Evaluator, Expansion, MemorySource, Registry, open};
use arrix_plugin_api::{Context, Diagnostic};
use arrix_plugin_host::{HostError, register_tier0_commands, register_tier2};

#[path = "plugins/place/src/place.rs"]
mod place;

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

/// `place-py` copied to a directory of its own, its command setting
/// `PYTHONPATH` to the SDK, and `edit` applied to its manifest.
fn staged(name: &str, edit: impl Fn(String) -> String) -> PathBuf {
    let src = root().join("crates/arrix-plugin-host/tests/plugins/place-py");
    let dir = std::env::temp_dir().join(name);
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::copy(src.join("place.py"), dir.join("place.py")).unwrap();
    let sdk = root().join("python");
    let manifest = std::fs::read_to_string(src.join("arrix-plugin.toml"))
        .unwrap()
        .replace(
            "command = [\"python3\"",
            &format!(
                "command = [\"env\", \"PYTHONPATH={}\", \"python3\"",
                sdk.display()
            ),
        );
    std::fs::write(dir.join("arrix-plugin.toml"), edit(manifest)).unwrap();
    dir
}

fn tier0() -> Registry {
    let manifest = std::fs::read_to_string(
        root().join("crates/arrix-plugin-host/tests/plugins/place/arrix-plugin.toml"),
    )
    .unwrap();
    let mut r = Registry::with_core_types();
    register_tier0_commands(&mut r, &manifest, Arc::new(place::Place)).unwrap();
    r
}

fn tier2(name: &str) -> Registry {
    let mut r = Registry::with_core_types();
    register_tier2(&mut r, &staged(name, |m| m)).unwrap();
    r
}

fn files(name: &str) -> MemorySource {
    let dir = root().join("tests/docs").join(name);
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
fn a_python_command_expands_to_the_group_it_gives_compiled_in() {
    let (t0, t2) = (tier0(), tier2("arrix-tier2-commands-same"));
    let group = expand(&t2, "place.block", 5).unwrap();
    assert_eq!(group, expand(&t0, "place.block", 5).unwrap());
    let golden = std::fs::read_to_string(
        root().join("crates/arrix-plugin-host/tests/golden/commands-place-block.json"),
    )
    .unwrap();
    assert_eq!(serde_json::to_string_pretty(&group).unwrap() + "\n", golden);
    assert_eq!(expand(&t2, "place.block", 5).unwrap(), group, "pure");
}

#[test]
fn a_refusal_and_a_bug_are_the_commands_diagnostics_and_the_process_lives() {
    let t2 = tier2("arrix-tier2-commands-refuse");
    let code = |c: &str| expand(&t2, c, 5).unwrap_err().code.to_string();
    assert_eq!(code("place.refuse"), "place.refused");
    assert_eq!(code("place.panic"), "plugin.exception");
    assert_eq!(code("place.rogue"), "command.invalid");
    assert!(expand(&t2, "place.block", 5).is_ok());
}

#[test]
fn a_plugin_without_document_write_is_refused_and_one_without_document_read_cannot_read() {
    let dir = staged("arrix-tier2-commands-no-write", |m| {
        m.replace("document-write = true", "document-write = false")
    });
    let mut r = Registry::with_core_types();
    let err = register_tier2(&mut r, &dir).unwrap_err();
    assert!(matches!(err, HostError::CommandCapability { .. }), "{err}");
    assert!(r.command("place.block").is_none(), "none of them");

    let dir = staged("arrix-tier2-commands-no-read", |m| {
        m.replace("document-read = true", "document-read = false")
    });
    let mut r = Registry::with_core_types();
    register_tier2(&mut r, &dir).unwrap();
    let d = expand(&r, "place.block", 5).unwrap_err();
    assert_eq!(d.code.as_str(), "plugin.capability", "{d:?}");
}
