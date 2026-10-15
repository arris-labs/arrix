//! The executor: the one place a thread or a worker is started
//! (docs/CONCURRENCY-WASM.md §Roles and threads, the wasm lint's permit).
//! Native: named OS threads. On wasm32 a thread fails to spawn with an
//! error, not a panic, and the evaluator runs in a web worker instead
//! (ADR-0042).

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

/// Starts a web worker running the module script at `script`, which the
/// embedder serves: the browser's evaluator (ADR-0042 §Topology).
#[cfg(target_arch = "wasm32")]
pub(crate) fn spawn_worker(script: &str) -> Result<web_sys::Worker, wasm_bindgen::JsValue> {
    let options = web_sys::WorkerOptions::new();
    options.set_type(web_sys::WorkerType::Module);
    web_sys::Worker::new_with_options(script, &options)
}
