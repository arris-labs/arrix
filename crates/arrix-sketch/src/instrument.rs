//! Per-thread counter of constraint solves — the executable half of the
//! "an idle sketch-mode frame performs exactly one sketch solve" contract
//! (docs/UI-RENDERING.md §Sketch mode).
//!
//! It is always compiled rather than `#[cfg(test)]`-gated: the layers that
//! must not re-solve per frame live in *other* crates (the app builds
//! the overlay, toolbar and inspector views), and `cfg(test)` here is off
//! when their test binaries are compiled. The cost is one non-atomic
//! thread-local increment per solve, against a solve that runs a nonlinear
//! least-squares to convergence.
//!
//! Thread-local, not global: a solve runs on whichever thread called it, so
//! test threads counting in parallel cannot corrupt each other.

use std::cell::Cell;

thread_local! {
    static SOLVE_COUNT: Cell<u64> = const { Cell::new(0) };
}

/// Bumped by the one solver funnel, [`crate::solve_with_options`].
pub(crate) fn note_solve() {
    SOLVE_COUNT.with(|c| c.set(c.get() + 1));
}

/// How many sketch solves this thread has run since [`reset_solve_count`].
pub fn solve_count() -> u64 {
    SOLVE_COUNT.with(Cell::get)
}

/// Zeroes this thread's counter. Call it immediately before the window you
/// want to measure.
pub fn reset_solve_count() {
    SOLVE_COUNT.with(|c| c.set(0));
}
