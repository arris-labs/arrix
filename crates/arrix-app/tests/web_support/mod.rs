//! What the app's browser tests share (`web_app.rs`, `c4_accept.rs`): the
//! app's page and a test page built into a directory of the test's own,
//! the documents it opens as `.arrx` bytes, and run in headless Chromium by
//! the harness (`arrix-web-test`).
#![allow(dead_code, reason = "each test file uses what it needs")]

use std::fs;
use std::path::{Path, PathBuf};
use std::time::Duration;

use arrix_doc::{MemorySource, to_zip};
use arrix_web_test::{build_lib, chromium, copy_files, open, target_dir, workspace_root};
use serde_json::Value;

/// The app's page, the test's and the bracket's bytes, in a directory of
/// `test`'s own (nextest runs each test in a process of its own), opened
/// at `page`: the page's report.
pub fn report(page: &str, isolated: bool, test: &str) -> Option<Value> {
    report_within(page, isolated, test, Duration::from_secs(180))
}

/// [`report`], waiting `timeout` for the page.
pub fn report_within(page: &str, isolated: bool, test: &str, timeout: Duration) -> Option<Value> {
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
    let text = open(&browser, &out, page, isolated, timeout);
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
pub fn source(dir: &Path) -> MemorySource {
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
pub fn without_gears() -> MemorySource {
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
pub fn golden(file: &str, pick: impl Fn(&Value) -> bool) -> Value {
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
