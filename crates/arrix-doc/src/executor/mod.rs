//! The executor: the one place a thread is spawned (docs/CONCURRENCY-WASM.md
//! §Roles and threads, the wasm lint's permit). Native: named OS threads.
//! On wasm32 spawning fails with an error, not a panic, until C4 runs the
//! authority and the evaluator in a web worker.

use std::thread::{Builder, JoinHandle};

/// A running thread, joined when it is dropped.
pub(crate) struct Worker(Option<JoinHandle<()>>);

/// Runs `f` on a new thread called `name`.
pub(crate) fn spawn(name: &str, f: impl FnOnce() + Send + 'static) -> std::io::Result<Worker> {
    let handle = Builder::new().name(name.into()).spawn(f)?;
    Ok(Worker(Some(handle)))
}

impl Drop for Worker {
    /// Waits for the thread to end. A panic on it was already reported by
    /// the panic hook; it is not raised a second time here.
    fn drop(&mut self) {
        if let Some(handle) = self.0.take() {
            let _ = handle.join();
        }
    }
}
