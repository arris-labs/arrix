//! `plugins/fasteners` (plans/c3-m3 step 9), a Tier 2 Python plugin, on the
//! spawned binary: `arrix test` passes every check, including its frozen
//! check and the two `[[test.commands]]`; and over `arrix serve`
//! `fasteners.place` on the bracket's counterbored holes (the fixture is
//! `tests/docs/bracket` and an assembly) seats one screw on each picked
//! edge, the head on the plate's top and the shank into the hole, and
//! undoes as one gesture.

mod common;

use std::process::Command;

use common::serve_client::Serve;
use common::workspace;
use serde_json::{Value, json};

const PLUGIN: &str = "plugins/fasteners";
const ASSEMBLY: &str = "3FASTENERS01A";
const BRACKET: &str = "3FASTENERS01B";
const PATTERN: &str = "E635NVKB74361";
const MM: f64 = 1e-3;
/// The plate's top, where the counterbores start.
const TOP: f64 = 5.0 * MM;

fn plugin() -> String {
    workspace().join(PLUGIN).display().to_string()
}

fn doc() -> String {
    workspace()
        .join(PLUGIN)
        .join("fixtures/bracket")
        .display()
        .to_string()
}

/// The counterbore's top edge of the original hole and of the pattern's
/// first copy (plans/c3-m3 Findings 1).
fn holes() -> Value {
    let side = "face:sweep.6YGYXVGFE9Z3Z.end-cap,face:revolve.CETME0KJFBN9F.side.0000000000002";
    json!([
        {"path": [BRACKET], "edge": format!("edge:sweep.6YGYXVGFE9Z3Z.end-cap/gen.CETME0KJFBN9F[{side}]")},
        {"path": [BRACKET], "edge": format!("edge:sweep.6YGYXVGFE9Z3Z.end-cap/gen.{PATTERN}[{side}/inst.{PATTERN}.1]")},
    ])
}

#[test]
fn arrix_test_passes_every_check() {
    let out = Command::new(env!("CARGO_BIN_EXE_arrix"))
        .current_dir(workspace())
        .args(["test", &plugin()])
        .output()
        .expect("run arrix");
    let report: Vec<Value> = String::from_utf8_lossy(&out.stdout)
        .lines()
        .map(|l| serde_json::from_str(l).expect("a JSON line"))
        .collect();
    assert!(
        out.status.success(),
        "{report:?}{}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(report.iter().all(|l| l["status"] == "pass"), "{report:?}");
    for name in [
        "spec",
        "determinism",
        "round_trip",
        "frozen",
        "command",
        "kernel_failure",
    ] {
        assert!(
            report.iter().any(|l| l["check"] == name),
            "no `{name}` in {report:?}"
        );
    }
    let commands = report.iter().filter(|l| l["check"] == "command").count();
    assert_eq!(commands, 2);
}

fn point(v: &Value) -> [f64; 3] {
    [0, 1, 2].map(|i| v[i].as_f64().unwrap())
}

#[test]
fn place_seats_a_screw_on_each_picked_edge_and_undoes_as_one_gesture() {
    let mut s = Serve::spawn_with(&["--plugin", &plugin(), &doc()]);
    let hello = s.call("initialize", json!({"protocol": 1}));
    assert_eq!(hello["result"]["protocol"], 1, "{hello}");

    let ids: Vec<String> = (1..=40).map(|n| format!("{n:013}")).collect();
    let queued = s.session(
        "expand",
        json!({
            "command": "fasteners.place",
            "context": {"assembly": ASSEMBLY},
            "params": {"length": "10 mm"},
            "inputs": {"holes": holes()},
            "ids": ids,
            "base": 0,
        }),
    );
    assert_eq!(queued["ok"], "expanding", "{queued}");
    let answer = s.hear_event(|e| e["kind"] == "expanded");
    // The answer is the command to submit, a `Command::Group`.
    let group = answer["group"]["Ok"].clone();
    assert!(group["group"].is_object(), "{answer}");
    let kinds: Vec<&str> = group["group"]["commands"]
        .as_array()
        .unwrap()
        .iter()
        .map(|c| c.as_object().unwrap().keys().next().unwrap().as_str())
        .collect();
    // Both holes are 8 mm across: one size, one part.
    assert_eq!(
        kinds,
        [
            "add_material",
            "add_part",
            "add_component",
            "add_joint",
            "add_component",
            "add_joint"
        ]
    );

    let applied = s.call(
        "session.submit",
        json!({"author": "0000000000001", "base": 0, "command": group}),
    );
    assert!(applied["result"]["ok"]["applied"].is_object(), "{applied}");
    let assembled = s.hear_event(|e| e["kind"] == "assembled" && e["generation"] == 1);
    let outcome = &assembled["outcome"];
    for joint in outcome["joints"].as_object().unwrap().values() {
        assert_eq!(joint["status"], "ok", "{joint}");
    }
    let screws: Vec<&Value> = outcome["components"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|c| c["path"][0] != BRACKET)
        .collect();
    assert_eq!(screws.len(), 2);
    for (n, screw) in screws.iter().enumerate() {
        // The head on the plate's top, its axis up, at the hole's centre.
        let at = point(&screw["pose"]["origin"]);
        let want = [20.0 * MM, (-12.0 + 10.0 * n as f64) * MM, TOP];
        for (a, b) in at.iter().zip(want) {
            assert!((a - b).abs() < 1e-9, "screw {n}: {at:?} vs {want:?}");
        }
        let z = point(&screw["pose"]["z_axis"]);
        assert!((z[2] - 1.0).abs() < 1e-9, "screw {n} points {z:?}");
    }

    // One gesture, one undo.
    let undone = s.call("session.undo", json!({"author": "0000000000001"}));
    assert!(undone.get("error").is_none(), "{undone}");
    let after = s.hear_event(|e| e["kind"] == "assembled" && e["generation"] == 2);
    assert_eq!(after["outcome"]["components"].as_array().unwrap().len(), 1);
}
