//! The app's eval line (plans/c4 step 10): read from the session's events
//! alone (`Evaluation::heard`), as the web page reads it from its worker,
//! it is `arrix eval`'s line for the same document, to the bit
//! (`crates/arrix-cli/tests/golden`): the bracket as opened, and its width
//! set to 80 mm as the sweep's last point.
#![cfg(not(target_arch = "wasm32"))]

use std::path::{Path, PathBuf};

use arrix_app::ArrixApp;
use egui_kittest::Harness;
use serde_json::Value;

mod common;
use common::wait;

fn workspace() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

/// The golden line of `file` that `pick` chooses, without what names the
/// run rather than the evaluation: its document's path and sweep point.
fn golden(file: &str, pick: impl Fn(&Value) -> bool) -> Value {
    let text =
        std::fs::read_to_string(workspace().join("crates/arrix-cli/tests/golden").join(file))
            .unwrap();
    let mut line = text
        .lines()
        .map(|l| serde_json::from_str::<Value>(l).unwrap())
        .find(|l| pick(l))
        .unwrap_or_else(|| panic!("no such line in {file}"));
    bare(&mut line);
    line
}

fn bare(line: &mut Value) {
    let object = line.as_object_mut().unwrap();
    object.remove("doc");
    object.remove("sweep");
}

/// Steps until the line of the generation whose width is `w` (metres) has
/// been heard.
fn hear_width(h: &mut Harness<'_, ArrixApp>, w: f64) {
    wait(h, "the generation finished", |h| {
        h.state()
            .eval_line()
            .is_some_and(|l| l.params.iter().any(|p| p.name == "w" && p.value == Some(w)))
    });
}

fn heard(h: &Harness<'_, ArrixApp>) -> Value {
    let mut line =
        serde_json::to_value(h.state().eval_line().expect("a finished generation")).unwrap();
    bare(&mut line);
    line
}

#[test]
fn the_heard_eval_line_is_arrix_evals() {
    let mut h = Harness::builder()
        .with_size(egui::vec2(1440.0, 900.0))
        .build_eframe(|cc| ArrixApp::new(cc));
    h.run();
    assert!(h.state().eval_line().is_none());
    h.state_mut()
        .open_path(&workspace().join("tests/docs/bracket"))
        .unwrap();
    hear_width(&mut h, 0.06);
    let opened = golden("eval_docs.jsonl", |l| l["doc"] == "tests/docs/bracket");
    assert_eq!(heard(&h), opened);

    let w = h
        .state()
        .eval_line()
        .unwrap()
        .params
        .iter()
        .find(|p| p.name == "w")
        .unwrap()
        .id;
    let expr = arrix_doc::expr::Expr::parse("80 mm").unwrap();
    assert_eq!(
        h.state_mut()
            .submit(arrix_doc::Command::SetParam { param: w, expr }),
        Ok(true)
    );
    hear_width(&mut h, 0.08);
    let edited = golden("eval_bracket_sweep.jsonl", |l| l["sweep"]["index"] == 2);
    assert_eq!(heard(&h), edited);
}
