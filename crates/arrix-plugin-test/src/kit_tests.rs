//! The kit's command check without a plugin directory (plans/c3-m3 step
//! 6): the Tier 0 test command `place.block` of `arrix-plugin-host`'s
//! tests, run on `tests/docs/assembly-exchange` the way a
//! `[[test.commands]]` entry names it. The check on a plugin of a tier the
//! binary loads from a directory is `tests/plugin_test.rs`'s.

use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::Arc;

use arrix_doc::Registry;
use arrix_plugin_host::{CommandFixture, Fixture, PickFixture, register_tier0_commands};

use super::expand_twice_and_undo;
use crate::source::Loaded;

#[path = "../../arrix-plugin-host/tests/plugins/place/src/place.rs"]
mod place;

const MANIFEST: &str = "[plugin]\nid = \"place\"\nversion = \"0.1.0\"\napi = \"^0.8\"\ntier = 0\n\
    title = \"Place\"\nlicence = \"MIT OR Apache-2.0\"\n[capabilities]\ndocument-write = true\n";

fn fresh() -> Result<Registry, String> {
    let mut r = Registry::with_core_types();
    register_tier0_commands(&mut r, MANIFEST, Arc::new(place::Place)).map_err(|e| e.to_string())?;
    Ok(r)
}

fn fixture(command: &str, edges: &[&str]) -> CommandFixture {
    let pick = |side: &&str| PickFixture {
        path: vec!["bracket".into()],
        edge: format!("edge:sweep.EBWK5C332393W.end-edge.{side}"),
    };
    CommandFixture {
        document: Fixture {
            path: "assembly-exchange".into(),
            line: 1,
        },
        command: command.into(),
        assembly: Some("Servo on bracket".into()),
        part: None,
        options: BTreeMap::from([("size".into(), "4 mm".into())]),
        inputs: BTreeMap::from([("edges".into(), edges.iter().map(pick).collect())]),
    }
}

fn source() -> Loaded {
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../tests/docs/assembly-exchange");
    Loaded::open(&dir).unwrap()
}

#[test]
fn a_command_expands_twice_applies_and_undoes() {
    let detail = expand_twice_and_undo(
        &fresh,
        &source(),
        &fixture("place.block", &["3ZPNYGV5VXAGH", "57Z6PA4YG5FQY"]),
    )
    .unwrap();
    assert!(detail.contains("group of 5 record(s)"), "{detail}");
}

#[test]
fn what_a_fixture_gets_wrong_is_said() {
    let run = |f: CommandFixture| expand_twice_and_undo(&fresh, &source(), &f).unwrap_err();
    let ok = fixture("place.block", &["3ZPNYGV5VXAGH"]);

    assert!(run(fixture("place.nothing", &[])).contains("registers no command"));
    let both = CommandFixture {
        part: Some("Bracket".into()),
        ..ok.clone()
    };
    assert!(run(both).contains("one of them"));
    let neither = CommandFixture {
        assembly: None,
        ..ok.clone()
    };
    assert!(run(neither).contains("one of them"));
    let nowhere = CommandFixture {
        assembly: Some("Nowhere".into()),
        ..ok.clone()
    };
    assert!(run(nowhere).contains("no such assembly"));
    let lost = CommandFixture {
        inputs: BTreeMap::from([(
            "edges".into(),
            vec![PickFixture {
                path: vec!["no such component".into()],
                edge: "edge:revolve.0000000000001.side.0000000000002".into(),
            }],
        )]),
        ..ok.clone()
    };
    assert!(run(lost).contains("no component"));
    let bad_edge = CommandFixture {
        inputs: BTreeMap::from([(
            "edges".into(),
            vec![PickFixture {
                path: vec![],
                edge: "not an edge".into(),
            }],
        )]),
        ..ok.clone()
    };
    assert!(run(bad_edge).contains("not an edge"));
    // The plugin refuses, and the diagnostic is the detail.
    let refusing = fixture("place.refuse", &[]);
    assert!(run(refusing).contains("place.refused"));
    // A pick on an edge the body lacks is the host's refusal.
    let gone = CommandFixture {
        inputs: BTreeMap::from([(
            "edges".into(),
            vec![PickFixture {
                path: vec!["bracket".into()],
                edge: "edge:sweep.EBWK5C332393W.end-edge.0000000000009".into(),
            }],
        )]),
        ..ok
    };
    assert!(run(gone).contains("ref.lost"));
}
