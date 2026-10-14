//! The north star with its screws (plans/c3-m3 steps 15 and 16):
//! `tests/docs/north-star` opened by `arrix serve` with `fasteners` loaded
//! from its directory, `fasteners.place` expanded on the three counterbored
//! holes' top edges and submitted as one gesture, and the document saved.

use std::path::Path;

use base64::Engine as _;
use base64::engine::general_purpose::STANDARD as B64;
use serde_json::{Value, json};

use super::serve_client::Serve;
use super::workspace;

pub const ASSEMBLY: &str = "7ZPA24EGV5499";
/// The bracket's component, the joints' ground.
pub const BRACKET: &str = "F1Q8ZVP5WZSKC";
pub const AUTHOR: &str = "0000000000001";
/// The options of the gesture: M3 in the 8 mm counterbores, which the size
/// rule would read as M6 (plans/c3-m3 Findings 10).
pub const LENGTH: &str = "10 mm";
pub const SIZE: &str = "M3";

/// The counterbore's top edge of the original hole and of the pattern's
/// two copies (plans/c3-m3 Findings 1).
pub fn holes() -> Value {
    let plate = "6YGYXVGFE9Z3Z";
    let hole = "CETME0KJFBN9F";
    let pattern = "E635NVKB74361";
    let side = format!("face:sweep.{plate}.end-cap,face:revolve.{hole}.side.0000000000002");
    let top = format!("edge:sweep.{plate}.end-cap");
    json!([
        {"path": [BRACKET], "edge": format!("{top}/gen.{hole}[{side}]")},
        {"path": [BRACKET], "edge": format!("{top}/gen.{pattern}[{side}/inst.{pattern}.1]")},
        {"path": [BRACKET], "edge": format!("{top}/gen.{pattern}.1[{side}/inst.{pattern}.2]")},
    ])
}

/// A session on the north star with `fasteners` loaded, past the handshake.
pub fn serve() -> Serve {
    let plugin = workspace().join("plugins/fasteners");
    let doc = workspace().join("tests/docs/north-star");
    let mut s = Serve::spawn_with(&["--plugin", plugin.to_str().unwrap(), doc.to_str().unwrap()]);
    let hello = s.call("initialize", json!({"protocol": 1}));
    assert_eq!(hello["result"]["protocol"], 1, "{hello}");
    s
}

/// `fasteners.place` expanded on `holes()` at generation 0 and submitted;
/// the `assembled` event of generation 1.
pub fn place(s: &mut Serve) -> Value {
    let ids: Vec<String> = (1..=40).map(|n| format!("{n:013}")).collect();
    let queued = s.session(
        "expand",
        json!({
            "command": "fasteners.place",
            "context": {"assembly": ASSEMBLY},
            "params": {"length": LENGTH},
            "choices": {"size": SIZE},
            "inputs": {"holes": holes()},
            "ids": ids,
            "base": 0,
        }),
    );
    assert_eq!(queued["ok"], "expanding", "{queued}");
    let answer = s.hear_event(|e| e["kind"] == "expanded");
    let group = answer["group"]["Ok"].clone();
    assert!(group["group"].is_object(), "{answer}");
    let applied = s.session(
        "submit",
        json!({"author": AUTHOR, "base": 0, "command": group}),
    );
    assert!(applied["ok"]["applied"].is_object(), "{applied}");
    s.hear_event(|e| e["kind"] == "assembled" && e["generation"] == 1)
}

/// `session.save` written into `dir`, which is emptied first.
pub fn save(s: &mut Serve, dir: &Path) {
    let saved = s.call("session.save", json!({}));
    let _ = std::fs::remove_dir_all(dir);
    let write = |path: &str, bytes: &[u8]| {
        let to = dir.join(path);
        std::fs::create_dir_all(to.parent().unwrap()).unwrap();
        std::fs::write(to, bytes).unwrap();
    };
    for (path, text) in saved["result"]["files"].as_object().expect("files") {
        write(path, text.as_str().unwrap().as_bytes());
    }
    for (path, b64) in saved["result"]["blobs"].as_object().expect("blobs") {
        write(path, &B64.decode(b64.as_str().unwrap()).unwrap());
    }
}

/// The north star with its screws, saved into `dir`.
pub fn screwed(dir: &Path) {
    let mut s = serve();
    let assembled = place(&mut s);
    for joint in assembled["outcome"]["joints"].as_object().unwrap().values() {
        assert_eq!(joint["status"], "ok", "{joint}");
    }
    save(&mut s, dir);
}
