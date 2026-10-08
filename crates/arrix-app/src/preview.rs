//! The create form's preview (ADR-0019, docs/UI-RENDERING.md §The
//! viewport): every change that leaves the draft complete enough to
//! evaluate asks the session for a preview of the `AddFeature` OK would
//! submit, and the viewport draws what it heard in place of the document's
//! bodies the preview replaces, its new or changed ones tinted. A preview
//! is session state: it issues no command, and the session ends it on any
//! applied change, so a draft still open asks again against the new
//! generation. Its answer comes as events, so a slow evaluation never holds
//! a frame.

use std::collections::BTreeSet;

use arrix_core::{Diagnostic, FeatureId, IdMinter, MeshDetail, PartId};
use arrix_doc::{
    Command, CommandEnvelope, EvalEvent, FeatureOutcome, Generation, Outcome, PreviewBody,
    PreviewId, Rejected, Session, SessionEvent,
};
use arrix_ui::{FeatureState, FormDiagnostic};
use arrix_viewport::{BodyKey, Renderer};

use crate::ArrixApp;
use crate::documents::{LOCAL_AUTHOR, OpenDocument};

/// A tab's preview: the one it asked for last, and what it heard of it.
#[derive(Default)]
pub struct FormPreview {
    /// How many previews this tab has asked for: names each one.
    count: u64,
    asked: Option<Asked>,
    heard: Option<Heard>,
}

/// The preview asked for last.
struct Asked {
    id: PreviewId,
    generation: Generation,
    command: Command,
    /// Why the authority refused it, if it did: shown, not asked again
    /// until the draft or the document changes.
    refused: Option<Diagnostic>,
}

/// What the session said of a preview.
pub struct Heard {
    id: PreviewId,
    pub events: Vec<EvalEvent>,
    /// The bodies it changed, and how many of them have their refined
    /// mesh.
    changed: usize,
    refined: usize,
}

impl Heard {
    /// Every body it changed is drawn refined.
    pub fn settled(&self) -> bool {
        self.refined >= self.changed
    }
}

fn key(part: PartId, body: &PreviewBody) -> BodyKey {
    BodyKey {
        part,
        feature: body.slot.feature,
        slot: body.slot.slot.clone(),
    }
}

impl FormPreview {
    /// Whether a preview is asked for or shown.
    pub fn active(&self) -> bool {
        self.asked.is_some()
    }

    /// What was heard of the preview asked for last, once it has been.
    pub fn heard(&self) -> Option<&Heard> {
        let asked = self.asked.as_ref()?;
        self.heard.as_ref().filter(|h| h.id == asked.id)
    }

    /// The form's header for the draft `feature`: evaluating until the
    /// preview is heard, then its outcome and diagnostic. Nothing while
    /// no preview is asked for.
    pub fn header(&self, feature: FeatureId) -> (Option<FeatureState>, Option<FormDiagnostic>) {
        let form = |d: &Diagnostic| FormDiagnostic {
            code: d.code.to_string(),
            message: d.message.clone(),
        };
        let Some(asked) = &self.asked else {
            return (None, None);
        };
        if let Some(d) = &asked.refused {
            return (Some(FeatureState::Failed), Some(form(d)));
        }
        let outcome = self
            .heard()
            .and_then(|h| h.events.iter().find(|e| e.feature == feature))
            .map(|e| &e.outcome);
        match outcome {
            None => (Some(FeatureState::Evaluating), None),
            Some(FeatureOutcome::Failed { diagnostic, .. }) => {
                (Some(FeatureState::Failed), Some(form(diagnostic)))
            }
            Some(FeatureOutcome::Ok { diagnostics, .. }) => {
                (Some(FeatureState::Ok), diagnostics.first().map(form))
            }
            Some(FeatureOutcome::Suppressed) => (Some(FeatureState::Suppressed), None),
            Some(FeatureOutcome::RolledBack) => (Some(FeatureState::RolledBack), None),
        }
    }
}

impl OpenDocument {
    /// Asks for a preview of `command` against the generation this replica
    /// has heard, unless that very preview is the one asked for last.
    fn ask_preview(&mut self, command: Command) {
        let generation = self.replica.generation();
        let same = self
            .preview
            .asked
            .as_ref()
            .is_some_and(|a| a.generation == generation && a.command == command);
        if same {
            return;
        }
        self.preview.count += 1;
        let id = PreviewId(self.preview.count);
        let envelope = CommandEnvelope {
            author: LOCAL_AUTHOR,
            base: generation,
            command: command.clone(),
        };
        let refused = match self.session().preview(id, envelope) {
            Ok(Outcome::Previewing) => None,
            Ok(_) => return,
            // The replica has not heard a change yet: asked again next frame.
            Err(Rejected::Stale { .. }) => return,
            Err(Rejected::Refused(e)) => Some(e.diagnostic()),
            Err(other) => {
                self.problems.push(other.to_string());
                return;
            }
        };
        if refused.is_some() {
            self.view.scene.clear_preview();
        }
        self.preview.asked = Some(Asked {
            id,
            generation,
            command,
            refused,
        });
    }

    /// Drops the preview, the session's and the drawn one.
    pub(crate) fn clear_preview(&mut self) {
        if self.preview.asked.take().is_some() {
            let _ = self.session().clear_preview(LOCAL_AUTHOR);
        }
        self.preview.heard = None;
        self.view.scene.clear_preview();
    }

    /// Takes in a preview's event, if it is of the preview asked for last:
    /// a superseded one's are dropped.
    pub(crate) fn receive_preview(&mut self, event: &SessionEvent, renderer: Option<&Renderer>) {
        let asked = self.preview.asked.as_ref().map(|a| a.id);
        match event {
            SessionEvent::Previewed {
                tag,
                events,
                bodies,
                ..
            } if Some(tag.preview) == asked => {
                // A body the preview leaves as it is stays drawn; every
                // other body of the document is replaced: changed, or gone.
                let (changed, unchanged): (Vec<_>, Vec<_>) = bodies.iter().partition(|b| b.changed);
                let changed: BTreeSet<BodyKey> = changed.iter().map(|b| key(b.part, b)).collect();
                let unchanged: BTreeSet<BodyKey> =
                    unchanged.iter().map(|b| key(b.part, b)).collect();
                let scene = &mut self.view.scene;
                let replaced = scene
                    .keys()
                    .filter(|k| !unchanged.contains(k))
                    .cloned()
                    .collect();
                scene.start_preview(|k| changed.contains(k), replaced);
                self.preview.heard = Some(Heard {
                    id: tag.preview,
                    events: events.clone(),
                    changed: changed.len(),
                    refined: 0,
                });
            }
            SessionEvent::PreviewMeshed {
                tag,
                part,
                slot,
                detail,
                mesh,
            } if Some(tag.preview) == asked => match mesh {
                Ok(mesh) => {
                    if let (MeshDetail::Refined, Some(heard)) = (detail, &mut self.preview.heard) {
                        heard.refined += 1;
                    }
                    let key = BodyKey {
                        part: *part,
                        feature: slot.feature,
                        slot: slot.slot.clone(),
                    };
                    if let Some(renderer) = renderer {
                        self.view
                            .scene
                            .set_preview_body(renderer.device(), &key, mesh);
                    }
                }
                Err(diagnostic) => self.problems.push(diagnostic.message.clone()),
            },
            _ => {}
        }
    }
}

impl crate::form::Form {
    /// Whether the draft is worth evaluating: every reference field shown
    /// holds a reference. Its texts are checked as OK checks them.
    fn complete(&self) -> bool {
        self.shown_inputs(None).all(|i| {
            self.inputs
                .get(&i.name)
                .is_some_and(|v| !v.refs().is_empty())
        })
    }
}

impl ArrixApp {
    /// Keeps each tab's preview in step with its create form: a draft
    /// complete enough to evaluate is previewed, once per change of the
    /// draft or of the document; anything else has none.
    pub(crate) fn follow_previews(&mut self) {
        for document in &mut self.documents {
            let doc = document.replica.document();
            let command = match document.form.as_mut() {
                Some(form) if form.edit.is_none() && form.complete() => {
                    form.command(doc, &mut self.ids).ok()
                }
                _ => None,
            };
            match command {
                Some(command) => document.ask_preview(command),
                None if document.preview.active() => document.clear_preview(),
                None => {}
            }
        }
    }
}

/// Mints `slot`'s id the first time it is asked for, and the same one
/// after: a draft's preview and its OK add the very same feature.
pub(crate) fn minted(slot: &mut Option<arrix_core::Id>, ids: &mut IdMinter) -> arrix_core::Id {
    *slot.get_or_insert_with(|| ids.next_id())
}
