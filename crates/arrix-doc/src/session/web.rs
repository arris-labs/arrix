//! The web session (ADR-0042): the authority on the page's main thread,
//! synchronous and kernel-free, and the evaluator alone in a web worker
//! the executor starts. They exchange the jobs and answers of the native
//! session in their wire form (`wire.rs`), by `postMessage`.
//!
//! On a cross-origin-isolated page they also share two counters in a
//! `SharedArrayBuffer`. **commits** is bumped after the authority applies
//! a commit and before it posts that commit's job; **supersedes** for a
//! commit, a preview and a clear. Every job carries both counts as they
//! were when it was posted, so the worker knows what it has received. Its
//! interrupt's source is `commits`, which stops a kernel call while a
//! newer commit is on its way, and its passes stop between features and
//! meshes as the native worker's do when it polls its queue: a commit
//! evaluation for a newer commit, a preview for anything newer. Without
//! isolation there are no counters: a superseded evaluation runs to its
//! end, and a preview gives way between jobs.

use std::cell::{Cell, RefCell};
use std::collections::BTreeMap;
use std::rc::Rc;
use std::sync::Arc;
use std::sync::mpsc::{Sender, channel};

use js_sys::{Array, Atomics, Int32Array, Reflect, SharedArrayBuffer};
use wasm_bindgen::JsCast;
use wasm_bindgen::prelude::*;
use web_sys::{DedicatedWorkerGlobalScope, MessageEvent, Worker};

use super::core::Core;
use super::plugins::Plugins;
use super::preview::{Job, Pending, Queue};
use super::wire::{self, Heard};
use super::worker::{EvalWorker, Outbox};
use super::{EventStream, Request, Session, SessionEvent};
use crate::authority::{Authority, Outcome, Rejected};
use crate::document::Document;
use crate::eval::Evaluator;
use crate::executor;
use crate::frozen::Frozen;
use crate::open::MemorySource;
use crate::registry::Registry;
use arrix_core::FeatureId;
use arrix_kernel::Interrupt;

/// The counters' places in the shared array.
const COMMITS: u32 = 0;
const SUPERSEDES: u32 = 1;

/// Whether this page can share memory with its worker: cross-origin
/// isolated, so `SharedArrayBuffer` exists.
fn isolated() -> bool {
    let global = js_sys::global();
    Reflect::get(&global, &"crossOriginIsolated".into())
        .ok()
        .and_then(|v| v.as_bool())
        .unwrap_or(false)
        && Reflect::has(&global, &"SharedArrayBuffer".into()).unwrap_or(false)
}

/// A session whose evaluator runs in a web worker. The authority answers
/// on the calling thread, so `request` returns at once; the events arrive
/// on the subscriptions as the page's event loop hears the worker.
/// Dropping it ends the worker.
pub struct WebSession {
    core: Rc<RefCell<Core>>,
    poster: Poster,
    _heard: Closure<dyn FnMut(MessageEvent)>,
    _failed: Closure<dyn FnMut(JsValue)>,
}

impl WebSession {
    /// Starts the authority over `document`, at generation 0, and its
    /// evaluator in a worker running `script`, a module script that calls
    /// [`run_evaluator`] with the same feature types as `registry`. There
    /// is no plugin loader in a browser, so `LoadPlugin` and
    /// `ReloadPlugin` are `Rejected::NoPluginLoader` (ADR-0024).
    ///
    /// # Errors
    ///
    /// When the worker cannot be started.
    pub fn open(document: Document, registry: Registry, script: &str) -> Result<Self, JsValue> {
        let worker = executor::spawn_worker(script)?;
        let counters = isolated().then(|| Int32Array::new(&SharedArrayBuffer::new(8)));
        // The first message hands the worker the counters, or says there
        // are none.
        worker.post_message(counters.as_ref().map_or(&JsValue::NULL, |c| c.as_ref()))?;
        let (core, first) = Core::new(Authority::new(document), Plugins::new(registry, None));
        let core = Rc::new(RefCell::new(core));
        let heard = {
            let core = core.clone();
            Closure::<dyn FnMut(MessageEvent)>::new(move |message: MessageEvent| {
                let text = message
                    .data()
                    .as_string()
                    .expect("the evaluator answers in text");
                match wire::decode_heard(&text).expect("an answer decodes as it was encoded") {
                    Heard::Publish(event) => core.borrow_mut().publish(*event),
                    Heard::Freeze(results) => core.borrow_mut().freeze(results),
                }
            })
        };
        worker.set_onmessage(Some(heard.as_ref().unchecked_ref()));
        // A worker that fails to load or panics is said, not silent.
        let failed = Closure::<dyn FnMut(JsValue)>::new(|error: JsValue| {
            web_sys::console::error_2(&"arrix: the evaluator's worker failed:".into(), &error);
        });
        worker.set_onerror(Some(failed.as_ref().unchecked_ref()));
        let poster = Poster {
            worker,
            counters,
            commits: Cell::new(0),
            supersedes: Cell::new(0),
        };
        poster.post(first);
        Ok(Self {
            core,
            poster,
            _heard: heard,
            _failed: failed,
        })
    }

    /// Whether a commit stops a superseded evaluation inside its kernel
    /// call: false on a page that is not cross-origin isolated, which the
    /// app says once (ADR-0042 §Without isolation).
    pub fn interruptible(&self) -> bool {
        self.poster.counters.is_some()
    }
}

impl Session for WebSession {
    fn request(&self, request: Request) -> Result<Outcome, Rejected> {
        let (result, jobs) = self.core.borrow_mut().request(request);
        self.poster.post(jobs);
        result
    }

    fn save(&self) -> MemorySource {
        self.core.borrow().save()
    }

    fn subscribe(&self) -> EventStream {
        let (tx, rx) = channel();
        self.core.borrow_mut().subscribe(tx);
        rx
    }
}

impl Drop for WebSession {
    fn drop(&mut self) {
        self.poster.worker.set_onmessage(None);
        self.poster.worker.set_onerror(None);
        self.poster.worker.terminate();
    }
}

/// The main thread's side of the channel to the worker.
struct Poster {
    worker: Worker,
    counters: Option<Int32Array>,
    commits: Cell<u32>,
    supersedes: Cell<u32>,
}

impl Poster {
    /// Posts `jobs` in order, each as `[commits, supersedes, job]`, the
    /// counters bumped before the job that bumps them is posted.
    fn post(&self, jobs: Vec<Job>) {
        for job in jobs {
            let commit = matches!(job, Job::Commit(_));
            if commit {
                self.bump(COMMITS, &self.commits);
            }
            if commit || matches!(job, Job::Preview(_) | Job::ClearPreview) {
                self.bump(SUPERSEDES, &self.supersedes);
            }
            let text = wire::encode_job(&job).expect("a web session loads no plugin");
            let message = Array::of3(
                &self.commits.get().into(),
                &self.supersedes.get().into(),
                &text.into(),
            );
            self.worker
                .post_message(&message)
                .expect("a job is plain data, which a worker takes");
        }
    }

    fn bump(&self, index: u32, count: &Cell<u32>) {
        count.set(count.get() + 1);
        if let Some(counters) = &self.counters {
            let n = i32::try_from(count.get()).expect("fewer than 2³¹ jobs in a session");
            Atomics::store(counters, index, n).expect("the counters are an Int32Array");
        }
    }
}

thread_local! {
    /// In the worker: the authority's counters, once handed over, when the
    /// page is isolated.
    static COUNTERS: RefCell<Option<Int32Array>> = const { RefCell::new(None) };
    /// In the worker: the counts the newest job received carries.
    static RECEIVED: Cell<(u32, u32)> = const { Cell::new((0, 0)) };
}

fn load(index: u32) -> u32 {
    COUNTERS.with_borrow(|c| {
        c.as_ref().map_or(0, |c| {
            let n = Atomics::load(c, index).expect("the counters are an Int32Array");
            u32::try_from(n).unwrap_or(0)
        })
    })
}

/// The interrupt's source: the commits posted (Arris's poll, every step).
fn commits() -> u64 {
    u64::from(load(COMMITS))
}

/// What is posted and not yet received, by the counters.
fn pending() -> Pending {
    let (commits, supersedes) = RECEIVED.get();
    Pending {
        commit: load(COMMITS) > commits,
        supersede: load(SUPERSEDES) > supersedes,
    }
}

/// The worker's answers, posted to the page as text.
struct PostBack(DedicatedWorkerGlobalScope);

impl Outbox for PostBack {
    fn publish(&self, event: SessionEvent) {
        let _ = self.0.post_message(&wire::encode_publish(&event).into());
    }

    fn freeze(&self, results: BTreeMap<FeatureId, Arc<Frozen>>) {
        let _ = self.0.post_message(&wire::encode_freeze(&results).into());
    }
}

/// The evaluator in its worker, between messages.
struct Evaluating {
    worker: EvalWorker,
    queue: Queue,
    jobs: Sender<Job>,
    interrupt: Interrupt,
    outbox: PostBack,
    /// Whether the counters' message has come: it is the first.
    shared: bool,
    /// Whether a pass is due on the timer: the messages that came during a
    /// pass are all received before the next.
    due: bool,
}

/// Runs the evaluator of a [`WebSession`] in the worker that calls it,
/// with `registry`'s feature types (the page's): takes over the worker's
/// `onmessage`, after handling `early`, the messages the worker's script
/// kept while the module loaded, in order. Call it once, in the worker.
///
/// # Panics
///
/// Outside a dedicated worker.
pub fn run_evaluator(registry: Registry, early: &Array) {
    let scope: DedicatedWorkerGlobalScope = js_sys::global()
        .dyn_into()
        .expect("the evaluator runs in a dedicated worker");
    let evaluator = Evaluator::new(registry);
    let interrupt = evaluator.interrupt();
    let (jobs, rx) = channel();
    let state = Rc::new(RefCell::new(Evaluating {
        worker: EvalWorker::new(evaluator),
        queue: Queue::new(rx, Some(pending)),
        jobs,
        interrupt,
        outbox: PostBack(scope.clone()),
        shared: false,
        due: false,
    }));
    let global = scope.clone();
    let pass = {
        let state = state.clone();
        Closure::<dyn FnMut()>::new(move || {
            let state = &mut *state.borrow_mut();
            state.due = false;
            state.worker.drain(&mut state.queue, &state.outbox);
        })
    };
    let handle = move |data: &JsValue| {
        if receive(&mut state.borrow_mut(), data) {
            scope
                .set_timeout_with_callback(pass.as_ref().unchecked_ref())
                .expect("a worker has timers");
        }
    };
    for data in early.iter() {
        handle(&data);
    }
    let heard = Closure::<dyn FnMut(MessageEvent)>::new(move |message: MessageEvent| {
        handle(&message.data())
    });
    global.set_onmessage(Some(heard.as_ref().unchecked_ref()));
    // The worker lives as long as the page keeps it; so do its handlers.
    heard.forget();
}

/// Takes one message from the page: whether a pass is now due on the
/// timer.
fn receive(state: &mut Evaluating, data: &JsValue) -> bool {
    if !state.shared {
        state.shared = true;
        if let Some(counters) = data.dyn_ref::<Int32Array>() {
            COUNTERS.set(Some(counters.clone()));
            state.interrupt.set_source(commits);
        }
        return false;
    }
    let message: &Array = data.unchecked_ref();
    let count = |i| message.get(i).as_f64().expect("a count") as u32;
    let (commits, supersedes) = (count(0), count(1));
    RECEIVED.set((commits, supersedes));
    // Every commit up to this count has now been received.
    state.interrupt.take(u64::from(commits));
    let text = message.get(2).as_string().expect("a job is text");
    let job = wire::decode_job(&text).expect("a job decodes as it was encoded");
    let _ = state.jobs.send(job);
    !std::mem::replace(&mut state.due, true)
}
