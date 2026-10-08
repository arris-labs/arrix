//! The flag a `Kernel` polls inside every Arris call (Arris's `Control`,
//! ADR-0030 there): raised from any thread, it stops the running call at
//! its next loop boundary with the model rolled back
//! (docs/CONCURRENCY-WASM.md §The evaluator). An atomic and no clock, so
//! it is the same on wasm.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

/// A shared handle on a kernel's interrupt flag; clones share it.
#[derive(Clone, Debug, Default)]
pub struct Interrupt(Arc<AtomicBool>);

impl Interrupt {
    /// Asks the running call, and every later one, to stop, until
    /// [`lower`](Interrupt::lower)ed.
    pub fn raise(&self) {
        self.0.store(true, Ordering::Relaxed);
    }

    pub fn lower(&self) {
        self.0.store(false, Ordering::Relaxed);
    }

    pub fn is_raised(&self) -> bool {
        self.0.load(Ordering::Relaxed)
    }
}
