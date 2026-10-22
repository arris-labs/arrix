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

use arrix_doc::{Command, expr::Expr};
use wasm_bindgen::prelude::*;

use crate::{ArrixApp, commands};

thread_local! {
    /// The evaluator worker's script, as the page gave it to
    /// [`WebHandle::start`]: each document's session starts one.
    static WORKER: RefCell<String> = RefCell::new("./worker.js".to_owned());
}

/// The script a document's session runs its evaluator in.
pub(crate) fn worker_script() -> String {
    WORKER.with(|w| w.borrow().clone())
}

fn error(e: impl std::fmt::Display) -> JsValue {
    JsValue::from_str(&e.to_string())
}

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
    /// The app's context, to ask for a frame after a call changes it: the
    /// page draws only when something asks.
    ctx: Rc<RefCell<Option<egui::Context>>>,
}

#[wasm_bindgen]
impl WebHandle {
    #[expect(clippy::new_without_default, reason = "a JS constructor")]
    #[wasm_bindgen(constructor)]
    pub fn new() -> Self {
        Self {
            runner: eframe::WebRunner::new(),
            backend: Rc::default(),
            ctx: Rc::default(),
        }
    }

    /// Starts the app on `canvas` with the built-in configuration: the
    /// page has no config files (ADR-0015). Each document's evaluator runs
    /// in a module worker of the script at `worker` (`worker.js`).
    ///
    /// # Errors
    ///
    /// When the graphics or the app cannot be set up.
    pub async fn start(
        &self,
        canvas: web_sys::HtmlCanvasElement,
        worker: String,
    ) -> Result<(), JsValue> {
        WORKER.with(|w| *w.borrow_mut() = worker);
        let backend = self.backend.clone();
        let ctx = self.ctx.clone();
        self.runner
            .start(
                canvas,
                eframe::WebOptions::default(),
                Box::new(move |cc| {
                    *backend.borrow_mut() = cc
                        .wgpu_render_state
                        .as_ref()
                        .map(|rs| format!("{:?}", rs.adapter.get_info().backend));
                    *ctx.borrow_mut() = Some(cc.egui_ctx.clone());
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

    /// Opens the `.arrx` bytes `arrx` as a tab named `name`, and shows it.
    ///
    /// # Errors
    ///
    /// The reason it did not open: the app is not running, the bytes are no
    /// document, or its session did not start.
    pub fn open_bytes(&self, name: &str, arrx: &[u8]) -> Result<(), JsValue> {
        let source = arrix_doc::from_zip(arrx).map_err(|e| error(format!("{name}: {e}")))?;
        self.with_app(|app| app.open(name, &source))?.map_err(error)
    }

    /// Sets the active document's parameter called `name` to `expr`, as
    /// one gesture through [`ArrixApp::submit`]: whether it changed
    /// anything.
    ///
    /// # Errors
    ///
    /// The reason it was not applied: no such parameter, an expression that
    /// does not parse, or the authority's refusal.
    pub fn set_param(&self, name: &str, expr: &str) -> Result<bool, JsValue> {
        self.with_app(|app| {
            let param = app
                .active_document()
                .and_then(|d| {
                    let params = d.replica.document().params();
                    params
                        .iter()
                        .find(|(_, p)| p.name == name)
                        .map(|(id, _)| *id)
                })
                .ok_or_else(|| format!("no parameter `{name}` in the active document"))?;
            let expr = Expr::parse(expr).map_err(|e| e.to_string())?;
            app.submit(Command::SetParam { param, expr })
        })?
        .map_err(error)
    }

    /// Queues a file the page read (its input's, `main.js`): an `.arrx` is
    /// opened as a tab and a STEP file imported into the shown one, by the
    /// next frame.
    pub fn accept_file(&self, name: String, bytes: Vec<u8>) {
        crate::web_files::arrive(name, bytes);
        if let Some(ctx) = &*self.ctx.borrow() {
            ctx.request_repaint();
        }
    }

    /// Downloads the active document as an `.arrx` (the app has no save
    /// command yet: native has none either).
    ///
    /// # Errors
    ///
    /// The reason it was not saved.
    pub fn save_download(&self) -> Result<(), JsValue> {
        self.with_app(|app| app.save_download())?.map_err(error)
    }

    /// Runs the command `id` as a button or a key does: what a test page
    /// uses for the commands that open a picker or a download.
    pub fn run_command(&self, id: &str) -> Result<(), JsValue> {
        self.with_app(|app| app.run(id))
    }

    /// The active document's newest finished generation's eval line, as
    /// `arrix eval` prints it (JSON), read from the session's events.
    pub fn eval_line(&self) -> Option<String> {
        let line = self.runner.app_mut::<ArrixApp>()?.eval_line()?;
        serde_json::to_string(&line).ok()
    }

    /// Runs `f` on the app, then asks for a frame to show what it did.
    fn with_app<R>(&self, f: impl FnOnce(&mut ArrixApp) -> R) -> Result<R, JsValue> {
        let result = {
            let mut app = self
                .runner
                .app_mut::<ArrixApp>()
                .ok_or_else(|| error("the app is not running"))?;
            f(&mut app)
        };
        if let Some(ctx) = &*self.ctx.borrow() {
            ctx.request_repaint();
        }
        Ok(result)
    }

    /// Why the app stopped, when it panicked.
    pub fn panic_message(&self) -> Option<String> {
        self.runner.panic_summary().map(|p| p.message())
    }
}
