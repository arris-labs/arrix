//! The browser probe (plans/c4 step 3, ADR-0042 §Cancellation): one wasm
//! module, loaded by the page and by its worker. The worker runs an Arris
//! call under `Control::poll` whose poll reads a counter in a
//! `SharedArrayBuffer` through `Atomics.load`; the page bumps the counter
//! mid-call, and the call stops as `Stop::Poll`. `main.js` and `worker.js`
//! beside this file drive it; `tests/web_probe.rs` serves the page.
//!
//! The call is `cut_many`, a box less a grid of cylinders, in one Arris
//! operation. The poll is a function, not a closure: `Poll` is `Sync` and a
//! JS handle is not, so the counter sits in a `thread_local!`. A timed call
//! also notes when each poll ran, so a stop's latency splits into the wait
//! for Arris's next step and the unwinding after it.
#![cfg(target_arch = "wasm32")]

use std::cell::{Cell, RefCell};

use arris::math::{Axis, Point3};
use arris::ops::{OpError, cut_many, primitive_box, primitive_cylinder};
use arris::topo::{Body, Model};
use arris::{Control, Stop};
use js_sys::{Atomics, Int32Array};
use wasm_bindgen::prelude::*;

thread_local! {
    /// The shared counter, once the page has handed it over.
    static COUNTER: RefCell<Option<Int32Array>> = const { RefCell::new(None) };
    /// The count when the running call started: a higher one stops it.
    static TAKEN: Cell<i32> = const { Cell::new(0) };
    /// Read the counter every this many steps; 0 never reads it.
    static EVERY: Cell<u32> = const { Cell::new(1) };
    /// The steps polled in the running call.
    static POLLS: Cell<u32> = const { Cell::new(0) };
    /// In a timed call: when each poll ran, and when one first saw the
    /// counter pass the taken count (`performance.now()`).
    static TIMES: RefCell<Option<Vec<f64>>> = const { RefCell::new(None) };
    static HEARD: Cell<Option<f64>> = const { Cell::new(None) };
    /// The next call's operands.
    static OPERANDS: RefCell<Option<(Model, Body, Vec<Body>)>> = const { RefCell::new(None) };
}

#[wasm_bindgen]
extern "C" {
    #[wasm_bindgen(js_namespace = performance)]
    fn now() -> f64;
}

fn counter() -> i32 {
    COUNTER.with_borrow(|c| c.as_ref().map_or(0, |c| Atomics::load(c, 0).unwrap_or(0)))
}

fn poll() -> bool {
    let n = POLLS.get();
    POLLS.set(n.wrapping_add(1));
    let every = EVERY.get();
    let stop = every != 0 && n.is_multiple_of(every) && counter() > TAKEN.get();
    TIMES.with_borrow_mut(|times| {
        if let Some(times) = times {
            let t = now();
            times.push(t);
            if stop && HEARD.get().is_none() {
                HEARD.set(Some(t));
            }
        }
    });
    stop
}

/// Reads the counter `n` times; the page times it.
#[wasm_bindgen]
pub fn loads(n: u32) -> i32 {
    (0..n).fold(0, |acc, _| acc ^ counter())
}

/// Hands this module the counter, an `Int32Array` over a
/// `SharedArrayBuffer`.
#[wasm_bindgen]
pub fn share(counter: Int32Array) {
    COUNTER.set(Some(counter));
}

/// Bumps the counter, from the page; the new count.
#[wasm_bindgen]
pub fn bump(counter: &Int32Array) -> i32 {
    Atomics::add(counter, 0, 1).unwrap_or(0) + 1
}

/// Builds the next call's operands: a box `side` × `side` cells of 10
/// units and a cylinder through each cell, so the timed call is the cut
/// alone.
#[wasm_bindgen]
pub fn prepare(side: u32) {
    let mut m = Model::default();
    let extent = 10.0 * f64::from(side);
    let (target, _) =
        primitive_box(&mut m, [0.0; 3], [extent, extent, 5.0], &Control::NONE).unwrap();
    let cell = |i: u32| 5.0 + 10.0 * f64::from(i);
    let tools = (0..side * side)
        .map(|i| {
            let axis = Axis::z_at(Point3::new(cell(i % side), cell(i / side), -1.0));
            primitive_cylinder(&mut m, axis, 3.0, 7.0, &Control::NONE)
                .unwrap()
                .0
        })
        .collect();
    OPERANDS.set(Some((m, target, tools)));
}

/// Runs the prepared cut, reading the counter every `every` steps (0:
/// never), and says how it ended as JSON: `{"stopped": bool, "steps": n}`,
/// `steps` the steps polled. `timed` adds the median and longest time
/// between two polls, `gap_ms` and `max_gap_ms`, `heard`, when a poll
/// first saw the bump, and `polled`, when the poll before it ran
/// (`performance.now()`, or `null`).
#[wasm_bindgen]
pub fn cut(every: u32, timed: bool) -> String {
    let (mut m, target, tools) = OPERANDS.take().expect("prepare before cut");
    EVERY.set(every);
    POLLS.set(0);
    TIMES.set(timed.then(Vec::new));
    HEARD.set(None);
    TAKEN.set(counter());
    let start = timed.then(now);
    let result = cut_many(&mut m, target, &tools, &Control::poll(&poll));
    let stopped = match result {
        Ok(_) => false,
        Err(OpError::Interrupted(stop)) if stop.by == Stop::Poll => true,
        Err(e) => panic!("the cut failed: {e:?}"),
    };
    let steps = POLLS.get();
    let Some((start, mut times)) = start.zip(TIMES.take()) else {
        return format!(r#"{{"stopped": {stopped}, "steps": {steps}}}"#);
    };
    times.insert(0, start);
    let mut gaps: Vec<f64> = times.windows(2).map(|w| w[1] - w[0]).collect();
    gaps.sort_by(f64::total_cmp);
    let gap = gaps.get(gaps.len() / 2).copied().unwrap_or(0.0);
    let max_gap = gaps.last().copied().unwrap_or(0.0);
    let heard = HEARD.get();
    let polled = heard.and_then(|h| times.iter().rev().find(|&&t| t < h));
    let json = |t: Option<&f64>| t.map_or("null".to_owned(), f64::to_string);
    format!(
        r#"{{"stopped": {stopped}, "steps": {steps}, "gap_ms": {gap}, "max_gap_ms": {max_gap}, "heard": {}, "polled": {}}}"#,
        json(heard.as_ref()),
        json(polled),
    )
}
