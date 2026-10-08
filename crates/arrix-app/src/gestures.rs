//! The gesture log (docs/UI-RENDERING.md §The shell): each closed gesture,
//! what opened it and how many commands it applied, so SEED.md §8.2's one
//! gesture, one command (or none when nothing changed) is checked over a
//! whole flow, and a command applied outside a gesture shows as a log that
//! does not add up.

use std::collections::VecDeque;

use serde::Serialize;

use crate::ArrixApp;
use arrix_ui::UiIntent;

/// How many closed gestures the log keeps, the latest last.
const KEPT: usize = 256;

#[derive(Default)]
pub(crate) struct Gestures {
    open: Option<Opened>,
    log: VecDeque<Gesture>,
}

/// The gesture under way: what opened it, and the document it began on
/// (by serial) with that document's applied total then.
struct Opened {
    what: String,
    document: Option<(u64, usize)>,
}

/// A closed gesture, as `debug_state()` reports it.
#[derive(Clone, Debug, Serialize)]
pub struct Gesture {
    /// What opened it: `key <command>`, `command <command>` for a button
    /// or a palette row, an intent's kind, `part sketch pick` or `submit`.
    pub what: String,
    /// The document it applied to: the one shown when it began.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub document: Option<String>,
    /// The commands it applied, undo and redo included.
    pub commands: usize,
}

impl Gestures {
    pub(crate) fn log(&self) -> impl Iterator<Item = &Gesture> {
        self.log.iter()
    }

    /// The last closed gesture's count; `None` before any.
    pub(crate) fn last(&self) -> Option<usize> {
        self.log.back().map(|g| g.commands)
    }
}

/// What opened a gesture of `intent`: the command a button or palette row
/// ran, else the intent's kind.
pub(crate) fn what(intent: &UiIntent) -> String {
    match intent {
        UiIntent::CommandInvoked { command } => format!("command {command}"),
        _ => {
            let debug = format!("{intent:?}");
            let kind = debug.split([' ', '{', '(']).next().unwrap_or_default();
            kind.to_owned()
        }
    }
}

impl ArrixApp {
    /// Submits `command` to the active document as one gesture, as the
    /// pointer's tools will: how a scenario edits the document before the
    /// UI has the tool for it. Returns whether it changed anything.
    pub fn submit(&mut self, command: arrix_doc::Command) -> Result<bool, String> {
        self.gesture("submit", |app| {
            let result = match app.documents.get_mut(app.active) {
                Some(document) => document.submit(command),
                None => Err("no document is open".to_owned()),
            };
            app.session.notice = result.as_ref().err().cloned();
            result
        })
    }

    /// Runs `f` as one gesture opened by `what`.
    pub(crate) fn gesture<R>(&mut self, what: &str, f: impl FnOnce(&mut Self) -> R) -> R {
        self.open_gesture(what);
        let result = f(self);
        self.close_gesture();
        result
    }

    pub(crate) fn open_gesture(&mut self, what: &str) {
        debug_assert!(
            self.session.gestures.open.is_none(),
            "a gesture opened inside another"
        );
        let document = self.active_document().map(|d| (d.serial, d.applied()));
        self.session.gestures.open = Some(Opened {
            what: what.to_owned(),
            document,
        });
        self.session.notice = None;
    }

    /// Closes the open gesture: its count is the commands its document
    /// applied since it opened. Why it did nothing, if it was refused, is
    /// the notice.
    pub(crate) fn close_gesture(&mut self) {
        let Some(opened) = self.session.gestures.open.take() else {
            return;
        };
        let mut gesture = Gesture {
            what: opened.what,
            document: None,
            commands: 0,
        };
        if let Some((serial, before)) = opened.document
            && let Some(document) = self.documents.iter_mut().find(|d| d.serial == serial)
        {
            gesture.commands = document.applied().saturating_sub(before);
            gesture.document = Some(document.title.clone());
            document.gestured += gesture.commands;
        }
        let log = &mut self.session.gestures.log;
        if log.len() == KEPT {
            log.pop_front();
        }
        log.push_back(gesture);
        for document in &self.documents {
            debug_assert_eq!(
                document.applied(),
                document.gestured,
                "{}: a command was applied outside a gesture",
                document.title
            );
        }
    }
}
