//! The eval line of a document's newest finished generation, read from its
//! session's events alone (`Evaluation::heard`, plans/c4 Findings 4): on
//! wasm32 the evaluator runs in a worker, and what it said is all the page
//! knows. The same on both targets, so native tests hold it too.

use arrix_doc::{Document, EvalLine, Evaluation, Generation, SessionEvent};

/// The events an eval line is made of, kept until their generation
/// finishes; then the line, and nothing of the generations before it.
#[derive(Default)]
pub(crate) struct HeardLine {
    pending: Vec<SessionEvent>,
    line: Option<(Generation, EvalLine)>,
}

impl HeardLine {
    /// Takes in `event`; on a `Finished`, makes that generation's line,
    /// naming the document `name` and its parameters and features as
    /// `doc`, the replica at generation `current`, has them. A generation
    /// the replica has moved past makes none: it is superseded, and `doc`
    /// may no longer hold what it evaluated.
    pub(crate) fn receive(
        &mut self,
        event: &SessionEvent,
        name: &str,
        doc: &Document,
        current: Generation,
    ) {
        let generation = match event {
            SessionEvent::Evaluated { .. }
            | SessionEvent::Assembled { .. }
            | SessionEvent::Massed { .. } => {
                self.pending.push(event.clone());
                return;
            }
            SessionEvent::Finished { generation, .. } => *generation,
            _ => return,
        };
        self.pending.push(event.clone());
        if generation == current
            && let Some(evaluation) = Evaluation::heard(generation, &self.pending)
        {
            self.line = Some((generation, evaluation.line(name, doc)));
        }
        self.pending.retain(|e| match e {
            SessionEvent::Evaluated { generation: g, .. }
            | SessionEvent::Assembled { generation: g, .. }
            | SessionEvent::Massed { generation: g, .. }
            | SessionEvent::Finished { generation: g, .. } => *g > generation,
            _ => false,
        });
    }

    /// The newest finished generation and its line.
    pub(crate) fn line(&self) -> Option<&(Generation, EvalLine)> {
        self.line.as_ref()
    }
}
