//! The web entry (ADR-0042, docs/UI-RENDERING.md §The web entry): one wasm
//! module, loaded by the page (`web/index.html`), which runs the app on
//! eframe's `WebRunner`, and by the evaluator's worker (`web/worker.js`),
//! which runs [`run_evaluator`] on the app's Tier 0 registry. The kernel
//! never runs on the page's main thread.
//!
//! `scripts/web-build` builds this library as a cdylib and runs
//! `wasm-bindgen --target web` on it.

use std::cell::RefCell;
use std::rc::Rc;

use wasm_bindgen::prelude::*;

use crate::{ArrixApp, commands};

/// The worker's entry: the evaluator on [`commands::feature_types`], after
/// the messages `worker.js` kept while the module loaded.
#[wasm_bindgen]
pub fn run_evaluator(early: js_sys::Array) {
    arrix_doc::run_evaluator(commands::feature_types(), &early);
}

/// The page's handle on the running app.
#[wasm_bindgen]
pub struct WebHandle {
    runner: eframe::WebRunner,
    /// The graphics backend the app's renderer landed on, once started.
    backend: Rc<RefCell<Option<String>>>,
}

#[wasm_bindgen]
impl WebHandle {
    #[expect(clippy::new_without_default, reason = "a JS constructor")]
    #[wasm_bindgen(constructor)]
    pub fn new() -> Self {
        Self {
            runner: eframe::WebRunner::new(),
            backend: Rc::default(),
        }
    }

    /// Starts the app on `canvas` with the built-in configuration: the
    /// page has no config files (ADR-0015).
    ///
    /// # Errors
    ///
    /// When the graphics or the app cannot be set up.
    pub async fn start(&self, canvas: web_sys::HtmlCanvasElement) -> Result<(), JsValue> {
        let backend = self.backend.clone();
        self.runner
            .start(
                canvas,
                eframe::WebOptions::default(),
                Box::new(move |cc| {
                    *backend.borrow_mut() = cc
                        .wgpu_render_state
                        .as_ref()
                        .map(|rs| format!("{:?}", rs.adapter.get_info().backend));
                    Ok(Box::new(ArrixApp::new(cc)))
                }),
            )
            .await
    }

    /// The backend wgpu draws through: `BrowserWebGpu` or `Gl` (WebGL2);
    /// `None` before the app starts or without wgpu.
    pub fn backend(&self) -> Option<String> {
        self.backend.borrow().clone()
    }

    /// What the app believes it drew, as `debug_state()`'s JSON
    /// (docs/UI-RENDERING.md §Visual debugging); `None` before the app
    /// starts or after it panicked.
    pub fn debug_state(&self) -> Option<String> {
        Some(self.runner.app_mut::<ArrixApp>()?.debug_state_json())
    }

    /// Why the app stopped, when it panicked.
    pub fn panic_message(&self) -> Option<String> {
        self.runner.panic_summary().map(|p| p.message())
    }
}
