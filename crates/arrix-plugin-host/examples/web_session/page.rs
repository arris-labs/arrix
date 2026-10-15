//! The web session's page (plans/c4 step 7, ADR-0042): one wasm module,
//! loaded by the page, which runs a `WebSession`'s authority, and by the
//! worker the session starts, which runs its evaluator. `main.js` and
//! `worker.js` beside this file drive it; `tests/web_session.rs` serves it.
//!
//! The page keeps what it hears as a client does, a replica and the
//! events, and reads a generation's eval line from them alone
//! (`Evaluation::heard`). Here rather than in `arrix-doc`, since the
//! bracket uses `gears` and this crate registers a Tier 0 plugin.
#![cfg(target_arch = "wasm32")]

use std::sync::Arc;

use arrix_core::{Id, MeshDetail};
use arrix_doc::{
    AuthorId, Command, CommandEnvelope, Evaluation, EventStream, FeatureOutcome, Generation,
    Outcome, Registry, Replica, Session, SessionEvent, WebSession, expr::Expr, open_zip,
    run_evaluator,
};
use arrix_plugin_host::{register_tier0, register_tier0_exporters};
use serde_json::json;
use wasm_bindgen::prelude::*;

const ME: AuthorId = AuthorId(Id(1));

/// The page's and the worker's feature types: `arrix eval`'s with no
/// `--plugin`.
fn registry() -> Registry {
    let mut r = Registry::with_core_types();
    register_tier0(&mut r, arrix_gears::MANIFEST, Arc::new(arrix_gears::Gears)).unwrap();
    register_tier0_exporters(
        &mut r,
        arrix_robotics::MANIFEST,
        Arc::new(arrix_robotics::Robotics),
    )
    .unwrap();
    r
}

/// The worker's entry: the evaluator, after the messages kept while the
/// module loaded.
#[wasm_bindgen]
pub fn evaluator(early: js_sys::Array) {
    run_evaluator(registry(), &early);
}

/// The page's entry: a session over a document, and what it has heard.
#[wasm_bindgen]
pub struct Page {
    session: WebSession,
    events: EventStream,
    heard: Vec<SessionEvent>,
    replica: Replica,
}

fn error(e: impl ToString) -> JsValue {
    JsValue::from_str(&e.to_string())
}

#[wasm_bindgen]
impl Page {
    /// Opens the `.arrx` bytes `arrx`, its evaluator in a worker running
    /// the script at `worker`.
    #[wasm_bindgen(constructor)]
    pub fn new(arrx: &[u8], worker: &str) -> Result<Page, JsValue> {
        let document = open_zip(arrx).map_err(error)?;
        let session = WebSession::open(document, registry(), worker)?;
        let events = session.subscribe();
        Ok(Self {
            session,
            events,
            heard: Vec::new(),
            replica: Replica::default(),
        })
    }

    pub fn interruptible(&self) -> bool {
        self.session.interruptible()
    }

    /// Takes the events heard so far: whether `generation` has finished
    /// and every final body of it is meshed coarse and refined (the
    /// worker meshes all coarse before any refined).
    pub fn done(&mut self, generation: u32) -> Result<bool, JsValue> {
        while let Ok(event) = self.events.try_recv() {
            self.replica.receive(&event).map_err(error)?;
            self.heard.push(event);
        }
        let g = Generation(generation.into());
        let finished = self
            .heard
            .iter()
            .any(|e| matches!(e, SessionEvent::Finished { generation, .. } if *generation == g));
        let (coarse, refined) = self.meshes(g);
        Ok(finished && refined > 0 && coarse == refined)
    }

    fn meshes(&self, g: Generation) -> (usize, usize) {
        let count = |detail: MeshDetail| {
            self.heard
                .iter()
                .filter(|e| {
                    matches!(e, SessionEvent::Meshed { generation, detail: d, .. }
                        if *generation == g && *d == detail)
                })
                .count()
        };
        (count(MeshDetail::Coarse), count(MeshDetail::Refined))
    }

    /// Sets the parameter called `name` to `expr`: the generation made.
    pub fn set_param(&self, name: &str, expr: &str) -> Result<u32, JsValue> {
        let doc = self.replica.document();
        let param = *doc
            .params()
            .iter()
            .find(|(_, p)| p.name == name)
            .ok_or_else(|| error(format!("no parameter `{name}`")))?
            .0;
        let envelope = CommandEnvelope {
            author: ME,
            base: self.replica.generation(),
            command: Command::SetParam {
                param,
                expr: Expr::parse(expr).map_err(error)?,
            },
        };
        match self.session.submit(envelope) {
            Ok(Outcome::Applied(change)) => {
                Ok(u32::try_from(change.generation.0).expect("a test's generation"))
            }
            other => Err(error(format!("{other:?}"))),
        }
    }

    /// What was heard of `generation`, as JSON: its features' events and
    /// whether each evaluated, its meshes and whether each was made, and
    /// its eval line read from the events.
    pub fn report(&self, generation: u32) -> String {
        let g = Generation(generation.into());
        let outcomes: Vec<&FeatureOutcome> = self
            .heard
            .iter()
            .filter_map(|e| match e {
                SessionEvent::Evaluated { generation, event } if *generation == g => {
                    Some(&event.outcome)
                }
                _ => None,
            })
            .collect();
        let meshes_ok = self.heard.iter().all(|e| match e {
            SessionEvent::Meshed {
                generation, mesh, ..
            } if *generation == g => mesh.is_ok(),
            _ => true,
        });
        let (coarse, refined) = self.meshes(g);
        let line =
            Evaluation::heard(g, &self.heard).map(|e| e.line("bracket", self.replica.document()));
        json!({
            "evaluated": outcomes.len(),
            "all_ok": outcomes.iter().all(|o| matches!(o, FeatureOutcome::Ok { .. })),
            "coarse": coarse,
            "refined": refined,
            "meshes_ok": meshes_ok,
            "line": line,
        })
        .to_string()
    }
}
