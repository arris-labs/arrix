//! The evaluator's side of a session: the worker that evaluates the newest
//! commit's snapshot and answers exports, expansions and previews behind
//! it. It is stepped, one pass over its queue at a time, so a thread
//! drains the queue and waits, and a browser worker drains it until idle
//! (docs/CONCURRENCY-WASM.md §The evaluator).

use std::sync::Arc;
use std::sync::mpsc::{Receiver, Sender};

use super::local::Msg;
use super::preview::{self, Job, Queue};
use super::{
    ExpandContext, ExpandRequest, ExportSubject, ExportedFileEvent, ExporterRequest, SessionEvent,
};
use crate::authority::Generation;
use crate::document::Document;
use crate::eval::{Evaluator, FinalBody, interrupted};
use arrix_core::{Diagnostic, MeshDetail, Severity};
use arrix_kernel::Interrupt;
use arrix_plugin_api::Subject;

/// The evaluator and what it remembers between passes.
pub(super) struct EvalWorker {
    evaluator: Evaluator,
    interrupt: Interrupt,
    /// The final bodies of the newest commit's finished evaluation, which a
    /// preview's bodies are told changed or not against.
    committed: Vec<FinalBody>,
    /// The newest commit evaluated to its end, which an export writes.
    finished: Option<(Generation, Arc<Document>)>,
    /// Whether the evaluator still holds that evaluation (a preview ran
    /// since if not).
    held: bool,
}

impl EvalWorker {
    pub fn new(evaluator: Evaluator) -> Self {
        Self {
            interrupt: evaluator.interrupt(),
            evaluator,
            committed: Vec::new(),
            finished: None,
            held: false,
        }
    }

    /// Runs on a thread: passes over `jobs` until the session thread has
    /// gone.
    pub fn run(mut self, jobs: &Receiver<Job>, publish: &Sender<Msg>) {
        let mut queue = Queue::new(jobs);
        while queue.wait(self.finished.is_some()) {
            if !self.step(&mut queue, publish) {
                return;
            }
        }
    }

    /// One pass: the structures, the registry, then the newest commit, or
    /// failing that an export, an expansion or a preview, whichever is
    /// queued first in that order. `false` once the queue's source has gone.
    pub fn step(&mut self, queue: &mut Queue, publish: &Sender<Msg>) -> bool {
        // Whatever raised the flag is the job taken now, or an older one.
        self.interrupt.lower();
        // A file's tree is a read of its own, wanted at once.
        while let Some(job) = queue.structures.pop_front() {
            let tree = crate::step_tree::read_step_tree(&job.bytes);
            let _ = publish.send(Msg::Publish(SessionEvent::StepTree { tag: job.tag, tree }));
        }
        if let Some((registry, stale)) = queue.registry.take() {
            self.evaluator.set_registry(*registry, &stale);
            // What the evaluator held was of the old registry.
            self.held = false;
        }
        if let Some((generation, doc)) = queue.commit.take() {
            return self.commit(generation, doc, queue, publish);
        }
        let finished = self.finished.clone();
        if let (false, Some((generation, doc))) = (queue.exports.is_empty(), &finished) {
            export_queued(
                &mut self.evaluator,
                (*generation, doc),
                &mut self.held,
                queue,
                publish,
            );
        } else if let (false, Some((generation, doc))) = (queue.expands.is_empty(), &finished) {
            expand_queued(
                &mut self.evaluator,
                (*generation, doc),
                &mut self.held,
                queue,
                publish,
            );
        } else if let Some(job) = queue.preview.take() {
            self.held = false;
            let mut send = |event| {
                let _ = publish.send(Msg::Publish(event));
            };
            preview::run(&mut self.evaluator, job, &self.committed, queue, &mut send);
        }
        true
    }

    fn commit(
        &mut self,
        generation: Generation,
        doc: Arc<Document>,
        queue: &mut Queue,
        publish: &Sender<Msg>,
    ) -> bool {
        let done = self.evaluator.evaluate_while(&doc, |event| {
            let event = SessionEvent::Evaluated {
                generation,
                event: event.clone(),
            };
            let _ = publish.send(Msg::Publish(event));
            queue.poll();
            queue.commit.is_none() && !queue.gone
        });
        if queue.gone {
            return false;
        }
        let _ = publish.send(Msg::Freeze(self.evaluator.frozen().clone()));
        self.held = done.is_some();
        if let Some(evaluation) = done {
            self.finished = Some((generation, doc));
            self.committed = self.evaluator.final_bodies().to_vec();
            for (assembly, outcome) in evaluation.assemblies {
                let assembled = SessionEvent::Assembled {
                    generation,
                    assembly,
                    outcome,
                };
                let _ = publish.send(Msg::Publish(assembled));
            }
            let massed = SessionEvent::Massed {
                generation,
                masses: evaluation.masses,
            };
            let _ = publish.send(Msg::Publish(massed));
            let finished = SessionEvent::Finished {
                generation,
                params: evaluation.params,
            };
            let _ = publish.send(Msg::Publish(finished));
            mesh_finals(&mut self.evaluator, generation, queue, publish);
        }
        true
    }
}

/// Answers the exports queued, oldest first, from the newest finished
/// commit `(generation, doc)`: evaluated again first (the cache makes it
/// cheap) when a preview has run since and `held` is false. An export
/// stopped by a newer commit stays queued, for that commit's evaluation.
fn export_queued(
    evaluator: &mut Evaluator,
    (generation, doc): (Generation, &Arc<Document>),
    held: &mut bool,
    queue: &mut Queue,
    publish: &Sender<Msg>,
) {
    while let Some(job) = queue.exports.front() {
        if !*held {
            if evaluator.evaluate_while(doc, |_| true).is_none() {
                return;
            }
            *held = true;
        }
        let (bytes, files) = if let Some(request) = &job.exporter {
            match export_files(evaluator, doc, request) {
                Ok(files) => (Ok(Vec::new()), files),
                Err(d) => (Err(d), Vec::new()),
            }
        } else {
            let bytes = match job.assembly {
                Some(assembly) => evaluator.export_assembly(doc, assembly),
                None => evaluator.export(&job.bodies, job.format),
            };
            (bytes, Vec::new())
        };
        if matches!(&bytes, Err(d) if interrupted(d)) {
            return;
        }
        let event = SessionEvent::Exported {
            generation,
            format: job.format,
            bytes,
            files,
        };
        let _ = publish.send(Msg::Publish(event));
        queue.exports.pop_front();
    }
}

/// Answers the expansions queued, oldest first, from the newest finished
/// commit `(generation, doc)`, evaluated again first when a preview has
/// run since. One asked on another generation than that is refused as
/// stale; one asked on a newer one waits for it. An expansion stopped by
/// a newer commit stays queued.
fn expand_queued(
    evaluator: &mut Evaluator,
    (generation, doc): (Generation, &Arc<Document>),
    held: &mut bool,
    queue: &mut Queue,
    publish: &Sender<Msg>,
) {
    while let Some(job) = queue.expands.front() {
        let base = job.request.base;
        if base > generation {
            return;
        }
        let group = if base < generation {
            Err(Diagnostic::new(
                Severity::Error,
                "command.stale",
                format!("the command was asked on generation {base:?}, and the document is at {generation:?}"),
            )
            .expect("a literal code is well-formed"))
        } else {
            if !*held {
                if evaluator.evaluate_while(doc, |_| true).is_none() {
                    return;
                }
                *held = true;
            }
            expand(evaluator, doc, &job.request)
        };
        if matches!(&group, Err(d) if interrupted(d)) {
            return;
        }
        let event = SessionEvent::Expanded { generation, group };
        let _ = publish.send(Msg::Publish(event));
        queue.expands.pop_front();
    }
}

/// Runs the command `request` names on `doc`, evaluated again first
/// (cached, and the plugin reads the evaluation it was made on).
fn expand(
    evaluator: &mut Evaluator,
    doc: &Arc<Document>,
    request: &ExpandRequest,
) -> Result<crate::command::Command, Diagnostic> {
    let Some(command) = evaluator.registry().command(&request.command).cloned() else {
        return Err(Diagnostic::new(
            Severity::Error,
            "command.unknown",
            format!("no command `{}`", request.command),
        )
        .expect("a literal code is well-formed"));
    };
    let Some(evaluation) = evaluator.evaluate_while(doc, |_| true) else {
        return Err(Diagnostic::new(
            Severity::Error,
            "kernel.interrupted",
            "the expansion was stopped".to_string(),
        )
        .expect("a literal code is well-formed"));
    };
    let context = match request.context {
        ExpandContext::Part(p) => arrix_plugin_api::Context::Part(p),
        ExpandContext::Assembly(a) => arrix_plugin_api::Context::Assembly(a),
    };
    evaluator.expand(
        doc,
        &evaluation,
        &crate::eval::Expansion {
            command: command.as_ref(),
            context,
            params: &request.params,
            choices: &request.choices,
            picks: &request.inputs,
            ids: &request.ids,
        },
    )
}

/// Runs the exporter `request` names on `doc`, evaluated again first
/// (cached, and the exporter reads the evaluation it was made on).
fn export_files(
    evaluator: &mut Evaluator,
    doc: &Arc<Document>,
    request: &ExporterRequest,
) -> Result<Vec<ExportedFileEvent>, Diagnostic> {
    let Some(exporter) = evaluator.registry().exporter(&request.id).cloned() else {
        return Err(Diagnostic::new(
            Severity::Error,
            "export.unknown-exporter",
            format!("no exporter `{}`", request.id),
        )
        .expect("a literal code is well-formed"));
    };
    let Some(evaluation) = evaluator.evaluate_while(doc, |_| true) else {
        return Err(Diagnostic::new(
            Severity::Error,
            "kernel.interrupted",
            "the export was stopped".to_string(),
        )
        .expect("a literal code is well-formed"));
    };
    let subject = match request.subject {
        ExportSubject::Part(p) => Subject::Part(p),
        ExportSubject::Assembly(a) => Subject::Assembly(a),
    };
    let files = evaluator.export_with(
        doc,
        &evaluation,
        exporter.as_ref(),
        subject,
        &request.params,
        &request.choices,
    )?;
    Ok(files
        .into_iter()
        .map(|f| ExportedFileEvent {
            name: f.name,
            bytes: f.bytes,
        })
        .collect())
}

/// Meshes the final bodies of the evaluation of `generation`, every body
/// coarse and then every body refined, publishing each as it is made.
/// Stops between meshes when a newer commit is queued, or the session
/// thread has gone.
fn mesh_finals(
    evaluator: &mut Evaluator,
    generation: Generation,
    queue: &mut Queue,
    publish: &Sender<Msg>,
) {
    let finals = evaluator.final_bodies().to_vec();
    for detail in [MeshDetail::Coarse, MeshDetail::Refined] {
        for body in &finals {
            queue.poll();
            if queue.commit.is_some() || queue.gone {
                return;
            }
            let mesh = evaluator.mesh(body, detail);
            if matches!(&mesh, Err(d) if interrupted(d)) {
                return;
            }
            let event = SessionEvent::Meshed {
                generation,
                part: body.part,
                slot: body.slot.clone(),
                detail,
                mesh,
            };
            let _ = publish.send(Msg::Publish(event));
        }
    }
}
