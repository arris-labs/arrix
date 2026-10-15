//! The flag a `Kernel` polls inside every Arris call (Arris's `Control`,
//! ADR-0030 there): raised from any thread, it stops the running call at
//! its next loop boundary with the model rolled back
//! (docs/CONCURRENCY-WASM.md §The evaluator). An atomic and no clock, so
//! it is the same on wasm.
//!
//! An embedder that cannot reach the flag from the thread that wants the
//! stop (a browser's main thread, ADR-0042) installs a *source* instead:
//! a function answering a count that only grows. The interrupt then stops
//! a call while the source's count is past the count the evaluator has
//! [`take`](Interrupt::take)n, so nothing is ever lowered.

use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, OnceLock};

#[derive(Debug, Default)]
struct State {
    flag: AtomicBool,
    taken: AtomicU64,
    source: OnceLock<fn() -> u64>,
}

/// A shared handle on a kernel's interrupt; clones share it.
#[derive(Clone, Debug, Default)]
pub struct Interrupt(Arc<State>);

impl Interrupt {
    /// Asks the running call, and every later one, to stop, until
    /// [`lower`](Interrupt::lower)ed.
    pub fn raise(&self) {
        self.0.flag.store(true, Ordering::Relaxed);
    }

    pub fn lower(&self) {
        self.0.flag.store(false, Ordering::Relaxed);
    }

    /// Installs the embedder's count (once; `false` if one is installed
    /// already). Calls stop while it answers more than the taken count,
    /// which starts at zero.
    pub fn set_source(&self, source: fn() -> u64) -> bool {
        self.0.source.set(source).is_ok()
    }

    /// Records that the evaluator has taken the job of count `n`: a source
    /// answering `n` or less no longer stops calls, and one past it does.
    pub fn take(&self, n: u64) {
        self.0.taken.store(n, Ordering::Relaxed);
    }

    pub fn is_raised(&self) -> bool {
        self.0.flag.load(Ordering::Relaxed)
            || self
                .0
                .source
                .get()
                .is_some_and(|count| count() > self.0.taken.load(Ordering::Relaxed))
    }
}
