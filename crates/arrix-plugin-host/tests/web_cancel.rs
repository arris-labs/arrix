//! Cancellation in a browser (plans/c4 step 8, ADR-0042), run in headless
//! Chromium by the browser harness (`arrix-web-test`):
//! `examples/web_session/cancel.js` drives a `WebSession` over
//! `tests/docs/bracket`.
//!
//! On an isolated page a commit stops a superseded evaluation inside its
//! kernel call, through the shared counter the worker's Arris poll reads,
//! and the stop is gated at 50 ms, as the native one is (the C1
//! benchmark's measurement: from a newer width edit's submit, made while
//! the one before is inside the gear's join, to the first event of the
//! newer generation). A preview never stops a commit; a commit stops a
//! running preview. Without isolation a superseded evaluation runs to its
//! end and the newest generation still finishes.
//!
//! `cargo test -p arrix-plugin-host --test web_cancel -- --nocapture`
//! prints the numbers; skips loudly without Chromium.
#![cfg(not(target_arch = "wasm32"))]

use std::time::Duration;

use arrix_web_test::{chromium, open};
use serde_json::Value;
use web::page;

mod web;

/// The gate: the native budget (docs/CONCURRENCY-WASM.md §Budgets).
const BUDGET_MS: f64 = 50.0;

fn report(isolated: bool, test: &str) -> Option<Value> {
    let browser = chromium(test)?;
    let text = open(
        &browser,
        &page(test),
        "cancel.html",
        isolated,
        Duration::from_secs(600),
    );
    let report: Value = serde_json::from_str(&text).unwrap_or_else(|e| panic!("{e}: {text}"));
    assert!(
        report.get("error").is_none(),
        "the page failed: {}",
        report["error"]
    );
    Some(report)
}

fn median(mut runs: Vec<f64>) -> f64 {
    runs.sort_by(f64::total_cmp);
    runs[runs.len() / 2]
}

#[test]
fn a_commit_stops_a_superseded_evaluation_within_the_budget() {
    let Some(r) = report(true, "web_cancel isolated") else {
        return;
    };
    assert_eq!(r["isolated"], true, "{r}");
    assert_eq!(r["interruptible"], true, "{r}");
    let runs = r["stops"].as_array().unwrap();
    assert!(runs.len() >= 5, "{r}");
    for run in runs {
        assert_eq!(run["staleFinished"], false, "the stale one finished: {r}");
        assert_eq!(run["newerFinished"], true, "{r}");
    }
    let times: Vec<f64> = runs.iter().map(|x| x["ms"].as_f64().unwrap()).collect();
    let median = median(times.clone());
    println!("cancellation in the browser: median {median:.1} ms over {times:.1?}");
    assert!(
        median <= BUDGET_MS,
        "median {median:.1} ms > {BUDGET_MS} ms"
    );

    // A preview behind a commit leaves it to finish, and then runs.
    assert_eq!(r["commit"]["finished"], true, "{r}");
    assert!(r["commit"]["previewed"].as_u64().unwrap() >= 1, "{r}");
    // A commit stops a running preview: it never says it is done, and the
    // newest generation finishes.
    assert_eq!(r["preview"]["finished"], true, "{r}");
    assert_eq!(r["preview"]["previewed"], 0, "the preview ran on: {r}");
    println!(
        "a commit over a running preview: first event after {:.1} ms",
        r["preview"]["ms"].as_f64().unwrap()
    );
}

#[test]
fn without_isolation_the_stale_evaluation_runs_to_its_end() {
    let Some(r) = report(false, "web_cancel unisolated") else {
        return;
    };
    assert_eq!(r["isolated"], false, "{r}");
    assert_eq!(r["interruptible"], false, "{r}");
    let run = &r["stops"].as_array().unwrap()[0];
    assert_eq!(run["staleFinished"], true, "it ran to its end: {r}");
    assert_eq!(run["newerFinished"], true, "{r}");
    println!(
        "unisolated: the newer generation's first event after {:.0} ms",
        run["ms"].as_f64().unwrap()
    );
    assert_eq!(r["commit"]["finished"], true, "{r}");
    assert_eq!(r["preview"]["finished"], true, "{r}");
}
