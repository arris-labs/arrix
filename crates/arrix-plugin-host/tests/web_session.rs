//! The web session in a browser (plans/c4 step 7, ADR-0042), run in
//! headless Chromium by the browser harness (`arrix-web-test`):
//! `examples/web_session` built for wasm32 and served with
//! `tests/docs/bracket` as `.arrx` bytes.
//!
//! The authority runs on the page's main thread and the evaluator in a
//! web worker. The page opens the bracket, hears generation 0 evaluate and
//! mesh, sets the width to 70 mm (from 60), and hears that generation's events, its
//! `Finished` and its meshes, and its eval line read from the events.
//! Served with the isolation headers the session shares its counters with
//! the worker; served without, it has none and still evaluates.
//!
//! Bit equality with native `arrix eval` is the accept's (plans/c4
//! Acceptance 1; `tests/wasm_determinism.rs` holds it without a page).
//!
//! `cargo test -p arrix-plugin-host --test web_session`; skips loudly
//! without Chromium.
#![cfg(not(target_arch = "wasm32"))]

use std::time::Duration;

use arrix_web_test::{chromium, open};
use serde_json::Value;
use web::page;

mod web;

fn report(isolated: bool, test: &str) -> Option<Value> {
    let browser = chromium(test)?;
    let text = open(
        &browser,
        &page(test),
        "index.html",
        isolated,
        Duration::from_secs(180),
    );
    let report: Value = serde_json::from_str(&text).unwrap_or_else(|e| panic!("{e}: {text}"));
    assert!(
        report.get("error").is_none(),
        "the page failed: {}",
        report["error"]
    );
    Some(report)
}

/// The width parameter's value in an eval line, in metres.
fn width(line: &Value) -> f64 {
    line["params"]
        .as_array()
        .unwrap()
        .iter()
        .find(|p| p["name"] == "w")
        .and_then(|p| p["value"].as_f64())
        .unwrap_or_else(|| panic!("no width in {line}"))
}

/// What both pages must have heard: each generation evaluated every
/// feature, finished and meshed, and the edit is in the second's line.
fn heard_both(r: &Value) {
    assert_eq!(r["generation"], 1, "{r}");
    for (name, g) in [("opened", &r["opened"]), ("edited", &r["edited"])] {
        assert!(g["evaluated"].as_u64().unwrap() > 0, "{name}: {g}");
        assert_eq!(g["all_ok"], true, "{name}: {g}");
        assert_eq!(g["meshes_ok"], true, "{name}: {g}");
        assert!(g["refined"].as_u64().unwrap() > 0, "{name}: {g}");
        assert_eq!(g["coarse"], g["refined"], "{name}: {g}");
        assert_eq!(g["line"]["status"], "ok", "{name}: {g}");
        assert_eq!(
            g["line"]["features"].as_array().unwrap().len() as u64,
            g["evaluated"].as_u64().unwrap(),
            "{name}: {g}"
        );
    }
    let (opened, edited) = (&r["opened"]["line"], &r["edited"]["line"]);
    assert!((width(edited) - 0.070).abs() < 1e-12, "{edited}");
    assert_ne!(width(opened), width(edited));
    assert_ne!(
        opened["bodies"], edited["bodies"],
        "the edit changed the bodies"
    );
}

#[test]
fn an_edit_evaluates_in_the_worker_and_the_page_hears_it() {
    let Some(r) = report(true, "web_session isolated") else {
        return;
    };
    assert_eq!(r["isolated"], true, "{r}");
    assert_eq!(r["interruptible"], true, "{r}");
    heard_both(&r);
}

#[test]
fn without_isolation_the_session_still_evaluates() {
    let Some(r) = report(false, "web_session unisolated") else {
        return;
    };
    assert_eq!(r["isolated"], false, "{r}");
    assert_eq!(r["interruptible"], false, "{r}");
    heard_both(&r);
}
