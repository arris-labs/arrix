//! The web app in a browser (plans/c4 steps 9 and 10, ADR-0042), run in
//! headless Chromium by the browser harness (`arrix-web-test`):
//! `arrix-app`'s library built as `scripts/web-build` builds it, its page
//! (`crates/arrix-app/web`) and a test page of `tests/web`, which drives
//! the app through its handle (a page can't click egui) and POSTs what it
//! heard.
//!
//! `debug.js` starts the app on an empty start. `edit.js` opens
//! `tests/docs/bracket` from bytes, its evaluator in a worker, and sets its
//! width as one gesture: each generation's eval line, read from the
//! events, is `arrix eval`'s (`crates/arrix-cli/tests/golden`).
//!
//! `files.js` brings files in through the page's input and by a drop on the
//! canvas, and catches the downloads of a save and an export.
//!
//! `frozen.js` opens the north star as saved with a plugin the page lacks
//! (a feature frozen, its bodies kept), `tests/docs/standoff`, which has no
//! stored result to show, and `tests/docs/north-star` as it is.
//!
//! `cargo test -p arrix-app --test web_app`; skips loudly without Chromium.
#![cfg(not(target_arch = "wasm32"))]

use std::fs;
use std::path::{Path, PathBuf};
use std::time::Duration;

use arrix_doc::{MemorySource, to_zip};
use arrix_web_test::{build_lib, chromium, copy_files, open, target_dir, workspace_root};
use serde_json::{Value, json};

/// The app's page, the test's and the bracket's bytes, in a directory of
/// `test`'s own (nextest runs each test in a process of its own), opened
/// at `page`: the page's report.
fn report(page: &str, isolated: bool, test: &str) -> Option<Value> {
    let browser = chromium(test)?;
    let out = target_dir().join("web-test").join(test.replace(' ', "-"));
    build_lib("arrix-app", &out);
    let crate_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    copy_files(&crate_dir.join("web"), &out);
    copy_files(&crate_dir.join("tests/web"), &out);
    for doc in ["bracket", "standoff", "north-star"] {
        let source = source(&workspace_root().join("tests/docs").join(doc));
        fs::write(out.join(format!("{doc}.arrx")), to_zip(&source).unwrap()).unwrap();
    }
    fs::write(
        out.join("north-star-missing.arrx"),
        to_zip(&without_gears()).unwrap(),
    )
    .unwrap();
    let screw = workspace_root().join("tests/fixtures/step/screw.step");
    fs::copy(screw, out.join("screw.step")).unwrap();
    let text = open(&browser, &out, page, isolated, Duration::from_secs(180));
    let report: Value = serde_json::from_str(&text).unwrap_or_else(|e| panic!("{e}: {text}"));
    assert!(
        report.get("error").is_none(),
        "the page failed: {}",
        report["error"]
    );
    Some(report)
}

/// The files of the document directory `dir`, by their `/`-separated
/// path inside it.
fn source(dir: &Path) -> MemorySource {
    fn walk(dir: &Path, prefix: &str, out: &mut MemorySource) {
        for entry in fs::read_dir(dir).unwrap() {
            let path = entry.unwrap().path();
            let name = path.file_name().unwrap().to_str().unwrap().to_owned();
            if path.is_dir() {
                walk(&path, &format!("{prefix}{name}/"), out);
            } else {
                out.0
                    .insert(format!("{prefix}{name}"), fs::read(&path).unwrap());
            }
        }
    }
    let mut out = MemorySource::default();
    walk(dir, "", &mut out);
    out
}

/// The north star as a plugin the page lacks would have saved it: its gear
/// is a `standoff.standoff` feature, whose stored result is kept. The
/// committed `tests/docs/standoff` has no stored result, since nothing
/// saved it with the plugin loaded.
fn without_gears() -> MemorySource {
    let mut north_star = source(&workspace_root().join("tests/docs/north-star"));
    for (path, bytes) in &mut north_star.0 {
        if path.starts_with("parts/") {
            let text = String::from_utf8(std::mem::take(bytes)).unwrap();
            *bytes = text.replace("gears.spur", "standoff.standoff").into_bytes();
        }
    }
    north_star
}

/// The golden line of `file` that `pick` chooses, without what names the
/// run rather than the evaluation: its sweep point.
fn golden(file: &str, pick: impl Fn(&Value) -> bool) -> Value {
    let path = workspace_root()
        .join("crates/arrix-cli/tests/golden")
        .join(file);
    let mut line = fs::read_to_string(path)
        .unwrap()
        .lines()
        .map(|l| serde_json::from_str::<Value>(l).unwrap())
        .find(|l| pick(l))
        .unwrap_or_else(|| panic!("no such line in {file}"));
    line.as_object_mut().unwrap().remove("sweep");
    line
}

/// The empty start, drawn: the shell's layout and no document.
fn drew_the_empty_start(r: &Value) {
    let state = &r["debug_state"];
    assert_eq!(state["window"].as_array().map(Vec::len), Some(2), "{state}");
    assert!(state["layout"]["viewport"].is_array(), "{state}");
    assert_eq!(state["shell"]["documents"]["tabs"], json!([]), "{state}");
    assert!(state["evaluation"].is_null(), "{state}");
    assert_eq!(state["theme"], "dark", "{state}");
}

#[test]
fn the_app_starts_in_the_page_and_draws() {
    let Some(r) = report("debug.html", true, "web_app isolated") else {
        return;
    };
    assert_eq!(r["isolated"], true, "{r}");
    // `--disable-gpu`: no WebGPU adapter, so wgpu lands on WebGL2
    // (docs/UI-RENDERING.md §The web entry).
    assert_eq!(r["backend"], "Gl", "{r}");
    drew_the_empty_start(&r);
}

/// What both edit pages must have heard: the bracket opened at generation
/// 0 and the edit made generation 1, each settled with its bodies drawn,
/// and each line `arrix eval`'s to the bit.
fn heard_the_edit(r: &Value) {
    assert_eq!(r["applied"], true, "{r}");
    let opened = golden("eval_docs.jsonl", |l| l["doc"] == "tests/docs/bracket");
    let edited = golden("eval_bracket_sweep.jsonl", |l| l["sweep"]["index"] == 2);
    for (name, heard, generation, line) in [
        ("opened", &r["opened"], 0, opened),
        ("edited", &r["edited"], 1, edited),
    ] {
        let evaluation = &heard["evaluation"];
        assert_eq!(evaluation["generation"], generation, "{name}: {evaluation}");
        assert_eq!(evaluation["problems"], json!([]), "{name}: {evaluation}");
        assert!(heard["bodies"].as_u64().unwrap() > 0, "{name}: {heard}");
        let mut heard_line: Value = serde_json::from_str(heard["line"].as_str().unwrap()).unwrap();
        heard_line.as_object_mut().unwrap().remove("sweep");
        assert_eq!(
            heard_line, line,
            "{name}: the eval line is not arrix eval's"
        );
    }
}

#[test]
fn an_edit_in_the_page_evaluates_in_the_worker_as_arrix_eval_does() {
    let Some(r) = report("edit.html", true, "web_app edit isolated") else {
        return;
    };
    assert_eq!(r["isolated"], true, "{r}");
    assert_ne!(r["hint"], arrix_app::UNINTERRUPTIBLE, "{r}");
    heard_the_edit(&r);
}

#[test]
fn without_isolation_the_app_says_so_and_still_evaluates() {
    let Some(r) = report("edit.html", false, "web_app edit unisolated") else {
        return;
    };
    assert_eq!(r["isolated"], false, "{r}");
    assert_eq!(r["hint"], arrix_app::UNINTERRUPTIBLE, "{r}");
    heard_the_edit(&r);
}

#[test]
fn files_come_in_as_bytes_and_go_out_as_downloads() {
    let Some(r) = report("files.html", true, "web_app files") else {
        return;
    };
    // Picked and dropped: a tab each, named for the file.
    assert_eq!(r["picked"], json!(["bracket.arrx"]), "{r}");
    assert_eq!(r["dropped"], json!(["bracket.arrx", "dropped.arrx"]), "{r}");
    // A save is the document's zip; the export a STEP file of its bodies.
    assert_eq!(r["saved"]["name"], "dropped.arrx", "{r}");
    assert_eq!(r["saved"]["head"].as_str().unwrap()[..2], *"PK", "{r}");
    assert!(r["saved"]["size"].as_u64().unwrap() > 1000, "{r}");
    assert_eq!(r["exported"]["name"], "dropped.step", "{r}");
    assert!(
        r["exported"]["head"]
            .as_str()
            .unwrap()
            .starts_with("ISO-10303-21"),
        "{r}"
    );
    // A STEP file added parts, and a file of no known kind is refused aloud.
    assert!(r["imported"].as_u64().unwrap() > 0, "{r}");
    assert!(
        r["refused"].as_str().unwrap().contains("open an .arrx"),
        "{r}"
    );
}

#[test]
fn a_feature_of_a_plugin_the_page_lacks_opens_frozen_with_its_bodies() {
    let Some(r) = report("frozen.html", true, "web_app frozen") else {
        return;
    };
    let reason = "Feature type standoff.standoff is not available in the browser";
    // The stored result is kept and drawn; the reason is the badge's and
    // the form's.
    let missing = &r["missing"];
    assert_eq!(missing["frozen"].as_array().unwrap().len(), 1, "{missing}");
    assert!(missing["bodies"].as_u64().unwrap() > 0, "{missing}");
    let badges = missing["badges"].as_array().unwrap();
    assert_eq!(badges.len(), 1, "{missing}");
    assert_eq!(badges[0]["reason"], reason, "{missing}");
    let read_only = missing["readOnly"].as_str().unwrap();
    assert!(read_only.starts_with(reason), "{missing}");
    // With nothing stored the feature fails, and says why.
    let standoff = &r["standoff"];
    assert_eq!(standoff["badges"], json!([]), "{standoff}");
    assert!(
        standoff["notes"][0]
            .as_str()
            .unwrap()
            .contains("cannot be evaluated here and has no stored result"),
        "{standoff}"
    );
    // The north star's plugins are the app's own: nothing is frozen.
    let north_star = &r["northStar"];
    assert_eq!(north_star["frozen"], json!([]), "{north_star}");
    assert_eq!(north_star["badges"], json!([]), "{north_star}");
    assert!(north_star["bodies"].as_u64().unwrap() > 0, "{north_star}");
}
