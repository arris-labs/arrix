//! The browser probe (plans/c4 step 3, ADR-0042 §Cancellation), run in
//! headless Chromium by the browser harness (`arrix-web-test`):
//! `examples/web_probe` built for wasm32 and served twice.
//!
//! With the isolation headers the page is cross-origin isolated, its
//! worker shares a `SharedArrayBuffer` with it, and a bump of the counter
//! from the page stops the worker's Arris call, mid-call, as `Stop::Poll`.
//! Without them the page is not isolated and has no `SharedArrayBuffer`,
//! which is how the web session tells that a superseded evaluation cannot
//! be stopped. The numbers (a load's cost, Arris's steps, the stop's
//! latency, waited and unwound) are reported, not gated: step 8 gates the
//! latency on the bracket (`docs/CONCURRENCY-WASM.md` §The browser build).
//!
//! `cargo test -p arrix-kernel --test web_probe -- --nocapture`; skips
//! loudly without Chromium.
#![cfg(not(target_arch = "wasm32"))]

use std::path::PathBuf;
use std::time::Duration;

use arrix_web_test::{build_example, chromium, copy_files, open, target_dir};
use serde_json::Value;

/// The page, built into a directory of `test`'s own: nextest runs each
/// test in a process of its own, and one must not rewrite the page another
/// is serving.
fn page(test: &str) -> PathBuf {
    let out = target_dir().join("web-test").join(test.replace(' ', "-"));
    build_example("arrix-kernel", "web_probe", &out);
    copy_files(
        &PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("examples/web_probe"),
        &out,
    );
    out
}

fn report(isolated: bool, test: &str) -> Option<Value> {
    let browser = chromium(test)?;
    let text = open(
        &browser,
        &page(test),
        "index.html",
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

#[test]
fn a_bump_from_the_page_stops_the_workers_arris_call() {
    let Some(r) = report(true, "web_probe isolated") else {
        return;
    };
    assert_eq!(r["isolated"], true, "{r}");
    assert_eq!(r["shared"], true, "{r}");
    for interval in ["never", "every", "nth"] {
        assert_eq!(
            r["cost"][interval]["stopped"], false,
            "an unbumped cut ran to its end: {r}"
        );
    }
    for interval in ["every", "nth"] {
        let latency = &r["latency"][interval];
        assert_eq!(
            latency["all_stopped"], true,
            "every bump stopped its cut: {latency}"
        );
    }
    let ms = |v: &Value| v.as_f64().unwrap_or(f64::NAN);
    eprintln!(
        "web probe, a {side}² grid cut: one Atomics.load {load:.0} ns; the cut {never:.0} ms reading \
         never, {every:.0} ms every step, {nth:.0} ms every {n}th, over {steps} steps; between steps \
         {gap:.4} ms median, {max_gap:.0} ms longest",
        side = r["side"],
        load = ms(&r["load_ns"]),
        never = ms(&r["cost"]["never"]["ms"]),
        every = ms(&r["cost"]["every"]["ms"]),
        nth = ms(&r["cost"]["nth"]["ms"]),
        n = r["nth"],
        steps = r["cost"]["every"]["steps"],
        gap = ms(&r["steps"]["gap_ms"]),
        max_gap = ms(&r["steps"]["max_gap_ms"]),
    );
    for interval in ["every", "nth"] {
        let l = &r["latency"][interval];
        eprintln!(
            "web probe, reading {interval}: bump to return {med:.1} ms median, {max:.1} ms longest; \
             {wait:.1} ms waiting for a step, {unwind:.1} ms unwinding; heard at the first poll \
             after the bump: {first}",
            med = ms(&l["median_ms"]),
            max = ms(&l["max_ms"]),
            wait = ms(&l["median_wait_ms"]),
            unwind = ms(&l["median_unwind_ms"]),
            first = l["all_first_poll_after"],
        );
    }
}

#[test]
fn without_the_headers_the_page_is_not_isolated() {
    let Some(r) = report(false, "web_probe unisolated") else {
        return;
    };
    assert_eq!(r["isolated"], false, "{r}");
    assert_eq!(r["shared"], false, "{r}");
}
