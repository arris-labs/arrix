//! The web app in a browser (plans/c4 step 9, ADR-0042), run in headless
//! Chromium by the browser harness (`arrix-web-test`): `arrix-app`'s
//! library built as `scripts/web-build` builds it, its page
//! (`crates/arrix-app/web`) and `tests/web/debug.js`, which starts the app
//! on an empty start and POSTs what it drew.
//!
//! `cargo test -p arrix-app --test web_app`; skips loudly without Chromium.
#![cfg(not(target_arch = "wasm32"))]

use std::path::PathBuf;
use std::time::Duration;

use arrix_web_test::{build_lib, chromium, copy_files, open, target_dir};
use serde_json::Value;

/// The app's page and the test's, in a directory of `test`'s own: nextest
/// runs each test in a process of its own.
fn report(isolated: bool, test: &str) -> Option<Value> {
    let browser = chromium(test)?;
    let out = target_dir().join("web-test").join(test.replace(' ', "-"));
    build_lib("arrix-app", &out);
    let crate_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    copy_files(&crate_dir.join("web"), &out);
    copy_files(&crate_dir.join("tests/web"), &out);
    let text = open(
        &browser,
        &out,
        "debug.html",
        isolated,
        Duration::from_secs(120),
    );
    let report: Value = serde_json::from_str(&text).unwrap_or_else(|e| panic!("{e}: {text}"));
    assert!(
        report.get("error").is_none(),
        "the page failed: {}",
        report["error"]
    );
    Some(report)
}

/// The empty start, drawn: the shell's layout and no document.
fn drew_the_empty_start(r: &Value) {
    let state = &r["debug_state"];
    assert_eq!(state["window"].as_array().map(Vec::len), Some(2), "{state}");
    assert!(state["layout"]["viewport"].is_array(), "{state}");
    assert_eq!(
        state["shell"]["documents"]["tabs"],
        serde_json::json!([]),
        "{state}"
    );
    assert!(state["evaluation"].is_null(), "{state}");
    assert_eq!(state["theme"], "dark", "{state}");
}

#[test]
fn the_app_starts_in_the_page_and_draws() {
    let Some(r) = report(true, "web_app isolated") else {
        return;
    };
    assert_eq!(r["isolated"], true, "{r}");
    // `--disable-gpu`: no WebGPU adapter, so wgpu lands on WebGL2
    // (docs/UI-RENDERING.md §The web entry).
    assert_eq!(r["backend"], "Gl", "{r}");
    drew_the_empty_start(&r);
}
