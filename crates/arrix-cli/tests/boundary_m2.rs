//! M2's accept (plans/boundary-m2 step 9): a test acting as the agent
//! drives `arrix serve --stdio` from outside its source. It loads the
//! Python standoff by `session.load_plugin` (from a temporary copy), builds
//! the plate, the standoff on its top face and the fillet naming the
//! standoff's edge by `session.submit`, reads each feature's outcome and
//! the final mesh from notifications, reads a stale and a refused command as
//! results' `err`, edits the copy and reloads it for the new height, saves
//! by `session.save` and renders the saved document with `arrix-app
//! render`: its JSON shows the bodies and their bounds, and a render
//! without the plugin shows the frozen badge.

mod common;

use std::path::{Path, PathBuf};
use std::process::Command as Process;

use arrix_core::{Id, ParamId};
use arrix_doc::{AuthorId, Command, expr::Expr};
use base64::Engine as _;
use base64::engine::general_purpose::STANDARD as B64;
use common::serve_client::{Serve, standoff_commands};
use common::{arrix_app, copy_tree, workspace};
use serde_json::{Value, json};

const ME: AuthorId = AuthorId(Id(1));
const THEM: AuthorId = AuthorId(Id(2));
const PLATE: &str = "FBPNAWF1N79XE";
const STANDOFF: &str = "9HN9VBVEXFYYQ";
const FILLET: &str = "01JBS2R9CRWRN";
/// The plate is 5 mm thick; the standoff is 10 mm, then 12.5 mm tall.
const TOP: f64 = 0.015;
const TALLER: f64 = 0.0175;

fn scratch(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("arrix-m2-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

/// A feature's outcome in the evaluation of `generation`.
fn outcome(s: &mut Serve, generation: u64, feature: &str) -> Value {
    let event = s.hear_event(|e| {
        e["kind"] == "evaluated"
            && e["generation"] == generation
            && e["event"]["feature"] == feature
    });
    event["event"]["outcome"].clone()
}

fn volume(outcome: &Value) -> f64 {
    outcome["slots"]["body"]["body"]["volume"].as_f64().unwrap()
}

/// The refined mesh of `generation`, its runs decoded from base64.
fn final_mesh(s: &mut Serve, generation: u64) -> (usize, usize) {
    let meshed = s.hear_event(|e| {
        e["kind"] == "meshed" && e["generation"] == generation && e["detail"] == "refined"
    });
    let mesh = &meshed["mesh"]["Ok"];
    let run = |key: &str| B64.decode(mesh[key].as_str().unwrap()).unwrap().len();
    (run("positions") / 12, run("triangles") / 12)
}

/// `arrix-app render` of `doc`; `None` where the machine has no adapter
/// (the visual scenarios skip there too, loudly).
fn render(app: &Path, dir: &Path, doc: &Path, extra: &[&str]) -> Option<Value> {
    let json_file = dir.join("render.json");
    let out = Process::new(app)
        .current_dir(workspace())
        .arg("render")
        .arg(doc)
        .args(["--size", "480x300", "-o"])
        .arg(dir.join("render.png"))
        .arg("--json")
        .arg(&json_file)
        .args(extra)
        .output()
        .expect("arrix-app runs");
    if !out.status.success() {
        let err = String::from_utf8_lossy(&out.stderr);
        if err.contains("no wgpu") {
            eprintln!("SKIPPED the render half of boundary_m2: {err}");
            return None;
        }
        panic!("arrix-app render failed: {err}");
    }
    Some(serde_json::from_slice(&std::fs::read(json_file).unwrap()).unwrap())
}

#[test]
fn an_agent_drives_a_session_from_outside_its_source() {
    let dir = scratch("accept");
    let copy = dir.join("standoff");
    copy_tree(&workspace().join("python/examples/standoff"), &copy);
    let _ = std::fs::remove_dir_all(copy.join("__pycache__"));

    // 1. No plugins; the copy is loaded by a request.
    let mut s = Serve::start();
    let loaded = s.session("load_plugin", json!({"dir": copy.to_str().unwrap()}));
    assert_eq!(loaded["ok"]["loaded"]["generation"], 1, "{loaded}");
    let registry = s.hear_event(|e| e["kind"] == "registry");
    assert_eq!(registry["added"][0]["id"], "standoff.standoff");

    // 2. The plate, the standoff on its top face, the fillet on its edge.
    let commands = standoff_commands(4);
    let n = commands.len() as u64;
    for (i, command) in commands.iter().cloned().enumerate() {
        let result = s.submit(ME, 1 + i as u64, command);
        assert_eq!(
            result["ok"]["applied"]["generation"],
            2 + i as u64,
            "{result}"
        );
    }
    let built = n + 1;
    for feature in [PLATE, STANDOFF, FILLET] {
        let done = outcome(&mut s, built, feature);
        assert_eq!(done["status"], "ok", "{feature}: {done}");
    }
    let tall = volume(&outcome(&mut s, built, STANDOFF));
    s.hear_event(|e| e["kind"] == "finished" && e["generation"] == built);
    let (vertices, triangles) = final_mesh(&mut s, built);
    assert!(vertices > 0 && triangles > 0);

    // 3. A stale and a refused command are results' `err`.
    let Command::AddParam { param, record } = commands[0].clone() else {
        panic!("the commands start with the parameter");
    };
    let set = |expr: &str| Command::SetParam {
        param,
        expr: Expr::parse(expr).unwrap(),
    };
    let stale = s.submit(THEM, 1, set("70 mm"));
    assert!(stale["err"]["stale"].is_object(), "{stale}");
    let refused = s.submit(ME, built, Command::AddParam { param, record });
    assert!(refused["err"]["refused"].is_object(), "{refused}");
    let _: ParamId = param;

    // 4. Edit the copy and reload it: the new height is evaluated.
    let source = copy.join("standoff.py");
    let text = std::fs::read_to_string(&source).unwrap();
    assert!(text.contains(r#"default="10 mm""#));
    std::fs::write(
        &source,
        text.replace(r#"default="10 mm""#, r#"default="12.5 mm""#),
    )
    .unwrap();
    let reloaded = s.session("reload_plugin", json!({"plugin": "standoff"}));
    let after = built + 1;
    assert_eq!(reloaded["ok"]["loaded"]["generation"], after, "{reloaded}");
    let taller = outcome(&mut s, after, STANDOFF);
    assert_eq!(taller["status"], "ok", "{taller}");
    assert!(volume(&taller) > tall * 1.1);
    assert_eq!(outcome(&mut s, after, FILLET)["status"], "ok");
    s.hear_event(|e| e["kind"] == "finished" && e["generation"] == after);

    // 5. Save, and render the saved document with the product's renderer.
    let saved = s.session("save", json!({}));
    let doc = dir.join("saved");
    for (path, text) in saved["files"].as_object().unwrap() {
        let at = doc.join(path);
        std::fs::create_dir_all(at.parent().unwrap()).unwrap();
        std::fs::write(at, text.as_str().unwrap()).unwrap();
    }
    for (path, bytes) in saved["blobs"].as_object().unwrap() {
        let at = doc.join(path);
        std::fs::create_dir_all(at.parent().unwrap()).unwrap();
        std::fs::write(at, B64.decode(bytes.as_str().unwrap()).unwrap()).unwrap();
    }
    drop(s);

    let app = arrix_app();
    let plugin = copy.to_str().unwrap();
    if let Some(live) = render(&app, &dir, &doc, &["--plugin", plugin]) {
        let bodies = live["debug_state"]["bodies"].as_array().unwrap();
        let standoff = bodies.iter().find(|b| b["feature"] == STANDOFF);
        let standoff = standoff.expect("the standoff is drawn");
        let top = standoff["bounds"][5].as_f64().unwrap();
        assert!((top - TALLER).abs() < 1e-6 && top > TOP, "{standoff}");
        assert!(
            live["debug_state"]["badges"]
                .as_array()
                .is_none_or(Vec::is_empty)
        );

        let frozen = render(&app, &dir, &doc, &[]).expect("the same machine renders twice");
        let badges = frozen["debug_state"]["badges"].as_array().unwrap();
        assert_eq!(badges.len(), 1, "{badges:?}");
        assert_eq!(badges[0]["title"], "Standoff");
        assert_eq!(badges[0]["state"], "frozen");
    }
    let _ = std::fs::remove_dir_all(&dir);
}
