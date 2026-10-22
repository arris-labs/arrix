//! Files in the browser (ADR-0042, docs/UI-RENDERING.md §The web entry): a
//! browser has no filesystem, so a file comes in as bytes, from the page's
//! `<input type=file>` or dropped on the canvas, and goes out as a
//! download. The bytes wait in a queue the frame polls, and an `.arrx` is
//! opened as a tab and a STEP file imported into the shown one, as native
//! does with the path a dialog answers.

use std::cell::RefCell;

use wasm_bindgen::JsCast as _;
use wasm_bindgen::prelude::*;

use crate::ArrixApp;

/// The page's file input (`web/main.js`), which `import` clicks.
const INPUT: &str = "arrix-open";

/// How long a download's object URL lives: the browser reads it
/// asynchronously after the click.
const REVOKE_AFTER_MS: i32 = 60_000;

thread_local! {
    /// Files that came in and are not yet taken in, by name.
    static ARRIVED: RefCell<Vec<(String, Vec<u8>)>> = const { RefCell::new(Vec::new()) };
}

/// Queues a file that came in from the page; the next frame takes it in.
pub(crate) fn arrive(name: String, bytes: Vec<u8>) {
    ARRIVED.with(|a| a.borrow_mut().push((name, bytes)));
}

fn arrived() -> Vec<(String, Vec<u8>)> {
    ARRIVED.with(|a| std::mem::take(&mut *a.borrow_mut()))
}

/// Opens the browser's file picker, on the page's input.
pub(crate) fn pick() {
    let input = web_sys::window()
        .and_then(|w| w.document())
        .and_then(|d| d.get_element_by_id(INPUT))
        .and_then(|e| e.dyn_into::<web_sys::HtmlElement>().ok());
    if let Some(input) = input {
        input.click();
    }
}

/// Hands `bytes` to the browser as a download called `name`.
///
/// # Errors
///
/// What the browser refused, for the user.
pub(crate) fn download(name: &str, bytes: &[u8]) -> Result<(), String> {
    let refused = |e: JsValue| format!("{name}: the browser refused the download: {e:?}");
    let window = web_sys::window().ok_or("no window")?;
    let body = window
        .document()
        .and_then(|d| d.body())
        .ok_or("no page body")?;
    let parts = js_sys::Array::of1(&js_sys::Uint8Array::from(bytes));
    let blob = web_sys::Blob::new_with_u8_array_sequence(&parts).map_err(refused)?;
    let url = web_sys::Url::create_object_url_with_blob(&blob).map_err(refused)?;
    let anchor = window
        .document()
        .ok_or("no document")?
        .create_element("a")
        .map_err(refused)?
        .dyn_into::<web_sys::HtmlAnchorElement>()
        .map_err(|_| "no anchor element".to_owned())?;
    anchor.set_href(&url);
    anchor.set_download(name);
    anchor
        .style()
        .set_property("display", "none")
        .map_err(refused)?;
    body.append_child(&anchor).map_err(refused)?;
    anchor.click();
    anchor.remove();
    let revoke = Closure::once_into_js(move || {
        let _ = web_sys::Url::revoke_object_url(&url);
    });
    window
        .set_timeout_with_callback_and_timeout_and_arguments_0(
            revoke.unchecked_ref(),
            REVOKE_AFTER_MS,
        )
        .map_err(refused)?;
    Ok(())
}

/// Reads the files dropped on the canvas, which the browser hands over
/// asynchronously, and queues each as it arrives.
pub(crate) fn read_dropped(ctx: &egui::Context) {
    let dropped = ctx.input(|i| i.raw.dropped_files.clone());
    for file in dropped {
        let name = file.path().to_string_lossy().into_owned();
        let ctx = ctx.clone();
        wasm_bindgen_futures::spawn_local(async move {
            match file.bytes_async().await {
                Ok(bytes) => arrive(name, bytes),
                Err(reason) => arrive_failed(&name, &reason),
            }
            ctx.request_repaint();
        });
    }
}

thread_local! {
    /// Files the browser could not read, as the notice says.
    static FAILED: RefCell<Vec<String>> = const { RefCell::new(Vec::new()) };
}

fn arrive_failed(name: &str, reason: &str) {
    FAILED.with(|f| f.borrow_mut().push(format!("{name}: {reason}")));
}

impl ArrixApp {
    /// Takes in the files that came since the last frame: dropped ones are
    /// read first, then each queued file is opened or imported.
    pub(crate) fn take_in_files(&mut self, ctx: &egui::Context) {
        read_dropped(ctx);
        if let Some(reason) = FAILED.with(|f| f.borrow_mut().pop()) {
            self.session.notice = Some(reason);
        }
        for (name, bytes) in arrived() {
            let extension = name
                .rsplit_once('.')
                .map(|(_, e)| e.to_ascii_lowercase())
                .unwrap_or_default();
            let result = match extension.as_str() {
                "arrx" => arrix_doc::from_zip(&bytes)
                    .map_err(|e| format!("{name}: {e}"))
                    .and_then(|source| self.open(&name, &source)),
                "step" | "stp" => self.request_import(name.clone(), bytes),
                _ => Err(format!("{name}: open an .arrx or import a .step")),
            };
            if let Err(reason) = result {
                self.session.notice = Some(reason);
            }
        }
    }

    /// The active document as an `.arrx` download.
    ///
    /// # Errors
    ///
    /// When no document is shown, or the browser refuses.
    pub(crate) fn save_download(&self) -> Result<(), String> {
        let document = self.active_document().ok_or("no document to save")?;
        let bytes = arrix_doc::to_zip(&document.save()).map_err(|e| e.to_string())?;
        let stem = std::path::Path::new(&document.title)
            .file_stem()
            .map_or_else(|| "document".into(), |s| s.to_string_lossy().into_owned());
        download(&format!("{stem}.arrx"), &bytes)
    }
}
