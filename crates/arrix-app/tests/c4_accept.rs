//! C4's acceptance (plans/c4 §Acceptance, ADR-0042): the web app in
//! headless Chromium on a cross-origin-isolated page, through the browser
//! harness (`arrix-web-test`). `tests/web/accept.js` drives the app by its
//! handle and POSTs what it measured and heard:
//!
//! 1. `tests/docs/bracket` opens from bytes; a width edit evaluates in the
//!    worker, and its eval line equals `arrix eval`'s for the same edit, to
//!    the bit.
//! 2. An edit made during an evaluation stops it within 50 ms (median over
//!    the run's samples), and the newest generation finishes.
//! 3. A document with a feature of a plugin the page lacks opens with it
//!    frozen, its bodies kept and its reason shown; `tests/docs/north-star`
//!    opens with nothing frozen.
//! 4. Saved as a download and opened natively, the document evaluates to
//!    native's eval line.
//!
//! `cargo test -p arrix-app --test c4_accept -- --nocapture` prints the
//! numbers; skips loudly without Chromium.
#![cfg(not(target_arch = "wasm32"))]

use std::time::Duration;

use arrix_app::ArrixApp;
use base64::Engine as _;
use egui_kittest::Harness;
use serde_json::{Value, json};

mod common;
mod web_support;
use web_support::{golden, report_within};

/// The gate: the native budget (docs/CONCURRENCY-WASM.md §Budgets).
const BUDGET_MS: f64 = 50.0;

/// What names the run rather than the evaluation: its document's path and
/// sweep point.
fn bare(mut line: Value) -> Value {
    let object = line.as_object_mut().unwrap();
    object.remove("doc");
    object.remove("sweep");
    line
}

/// The line the page wrote, without its document's name.
fn written(text: &Value) -> Value {
    bare(serde_json::from_str(text.as_str().unwrap()).unwrap())
}

fn median(mut runs: Vec<f64>) -> f64 {
    runs.sort_by(f64::total_cmp);
    runs[runs.len() / 2]
}

/// The saved download opened in the native app: the line it evaluates to.
fn native_line(download: &Value) -> Value {
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(download["base64"].as_str().unwrap())
        .unwrap();
    let source = arrix_doc::from_zip(&bytes).unwrap();
    let mut h = Harness::builder()
        .with_size(egui::vec2(1440.0, 900.0))
        .build_eframe(|cc| ArrixApp::new(cc));
    h.run();
    h.state_mut().open("saved.arrx", &source).unwrap();
    common::wait(&mut h, "the saved document finished", |h| {
        h.state().eval_line().is_some_and(|l| {
            l.params
                .iter()
                .any(|p| p.name == "w" && p.value == Some(0.08))
        })
    });
    bare(serde_json::to_value(h.state().eval_line().unwrap()).unwrap())
}

#[test]
fn c4_accept() {
    let Some(r) = report_within("accept.html", true, "c4_accept", Duration::from_secs(600)) else {
        return;
    };
    assert_eq!(r["isolated"], true, "{r}");
    assert_eq!(r["interruptible"], true, "{r}");

    // 1. The bracket as opened and as edited is `arrix eval`'s, to the bit.
    let opened = golden("eval_docs.jsonl", |l| l["doc"] == "tests/docs/bracket");
    let edited = golden("eval_bracket_sweep.jsonl", |l| l["sweep"]["index"] == 2);
    assert_eq!(written(&r["opened"]), bare(opened), "the opened line");
    assert_eq!(
        written(&r["edited"]),
        bare(edited.clone()),
        "the edited line"
    );

    // 2. A commit stops a superseded evaluation within the budget.
    let runs = r["stops"].as_array().unwrap();
    assert!(runs.len() >= 5, "{r}");
    for run in runs {
        assert_eq!(run["staleFinished"], false, "the stale one finished: {r}");
        assert_eq!(run["newerFinished"], true, "{r}");
    }
    let times: Vec<f64> = runs.iter().map(|x| x["ms"].as_f64().unwrap()).collect();
    let median = median(times.clone());
    println!("cancellation in the app: median {median:.1} ms over {times:.1?}");
    assert!(
        median <= BUDGET_MS,
        "median {median:.1} ms > {BUDGET_MS} ms"
    );

    // 3. A plugin the page lacks: frozen, bodies kept, reason shown.
    let reason = "Feature type standoff.standoff is not available in the browser";
    let missing = &r["missing"];
    // Stale, since its type is not the one the result was made under.
    assert_eq!(missing["frozen"], json!(["frozen-stale"]), "{missing}");
    assert!(missing["bodies"].as_u64().unwrap() > 0, "{missing}");
    let badges = missing["badges"].as_array().unwrap();
    assert_eq!(badges.len(), 1, "{missing}");
    assert_eq!(badges[0]["reason"], reason, "{missing}");
    assert!(
        missing["readOnly"].as_str().unwrap().starts_with(reason),
        "{missing}"
    );
    let north_star = &r["northStar"];
    assert_eq!(north_star["frozen"], json!([]), "{north_star}");
    assert_eq!(north_star["badges"], json!([]), "{north_star}");
    assert!(north_star["bodies"].as_u64().unwrap() > 0, "{north_star}");

    // 4. Saved as a download, the document is native's when opened there.
    assert_eq!(r["download"]["name"], "bracket.arrx", "{r}");
    assert_eq!(native_line(&r["download"]), bare(edited), "the saved line");
}
