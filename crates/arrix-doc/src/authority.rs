//! The authority: the one writer of a document (docs/DATA-MODEL.md
//! §Commands and undo). It stamps every node with the generation that last
//! changed it, rejects a command whose base is older than a stamp it
//! touches, and keeps each author's undo and redo stacks of inverses.
//! Stamps and generations are runtime state, never in the file.

use std::collections::{BTreeMap, BTreeSet};

use arrix_core::Id;
use serde::{Deserialize, Serialize};

use crate::command::{Applied, Command, CommandError, apply};
use crate::dag::Node;
use crate::document::Document;

/// Bumped by every applied command, undo and redo included.
#[derive(
    Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize,
)]
#[serde(transparent)]
pub struct Generation(pub u64);

/// Who issued a command: the local user in local mode.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct AuthorId(pub Id);

/// A command, its author and the generation the author saw.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct CommandEnvelope {
    pub author: AuthorId,
    pub base: Generation,
    pub command: Command,
}

/// A change the authority made: one item of the applied stream.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Change {
    pub generation: Generation,
    pub author: AuthorId,
    /// What was applied: the submitted command, or the inverse an undo or
    /// redo applied.
    pub command: Command,
    pub touched: BTreeSet<Node>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Outcome {
    Applied(Change),
    /// The command's effect equals the current state: nothing applied, no
    /// undo entry.
    NoOp,
}

/// Why nothing was applied.
#[derive(Clone, Debug, PartialEq, thiserror::Error)]
pub enum Rejected {
    #[error("stale: {nodes:?} changed after generation {base:?}; rebuild against the new state")]
    Stale { base: Generation, nodes: Vec<Node> },
    #[error(transparent)]
    Refused(#[from] CommandError),
    #[error("nothing to undo")]
    NothingToUndo,
    #[error("nothing to redo")]
    NothingToRedo,
}

/// An undo or redo entry: the command that reverses a change, the
/// generation the change made, and the stamps its nodes had before it.
#[derive(Clone, Debug, PartialEq)]
struct Entry {
    command: Command,
    made: Generation,
    before: BTreeMap<Node, Option<Generation>>,
}

#[derive(Clone, Debug, Default, PartialEq)]
struct Stacks {
    undo: Vec<Entry>,
    redo: Vec<Entry>,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Direction {
    Undo,
    Redo,
}

#[derive(Clone, Debug, Default)]
pub struct Authority {
    document: Document,
    generation: Generation,
    stamps: BTreeMap<Node, Generation>,
    authors: BTreeMap<AuthorId, Stacks>,
}

impl Authority {
    /// The authority over an opened document, at generation 0.
    pub fn new(document: Document) -> Self {
        Self {
            document,
            ..Self::default()
        }
    }

    pub fn document(&self) -> &Document {
        &self.document
    }

    pub fn generation(&self) -> Generation {
        self.generation
    }

    /// Applies a command, or rejects it as refused or stale.
    pub fn submit(&mut self, envelope: &CommandEnvelope) -> Result<Outcome, Rejected> {
        let applied = apply(&self.document, &envelope.command)?;
        if applied.document == self.document {
            return Ok(Outcome::NoOp);
        }
        self.check_fresh(&applied.touched, envelope.base)?;
        let (change, entry) = self.commit(envelope.author, envelope.command.clone(), applied);
        let stacks = self.authors.entry(envelope.author).or_default();
        stacks.undo.push(entry);
        stacks.redo.clear();
        Ok(Outcome::Applied(change))
    }

    /// Applies `author`'s newest inverse, unless another author has since
    /// changed what it touches.
    pub fn undo(&mut self, author: AuthorId) -> Result<Outcome, Rejected> {
        self.step(author, Direction::Undo)
    }

    /// Re-applies what `author` last undid, under the same rule.
    pub fn redo(&mut self, author: AuthorId) -> Result<Outcome, Rejected> {
        self.step(author, Direction::Redo)
    }

    fn step(&mut self, author: AuthorId, dir: Direction) -> Result<Outcome, Rejected> {
        let stacks = self.authors.entry(author).or_default();
        let (from, empty) = match dir {
            Direction::Undo => (&mut stacks.undo, Rejected::NothingToUndo),
            Direction::Redo => (&mut stacks.redo, Rejected::NothingToRedo),
        };
        let entry = from.last().cloned().ok_or(empty)?;
        let applied = apply(&self.document, &entry.command)?;
        self.check_fresh(&applied.touched, entry.made)?;
        let stacks = self.authors.get_mut(&author).expect("inserted above");
        match dir {
            Direction::Undo => stacks.undo.pop(),
            Direction::Redo => stacks.redo.pop(),
        };
        if applied.document == self.document {
            return Ok(Outcome::NoOp);
        }
        let (change, reverse) = self.commit(author, entry.command, applied);
        // The nodes are back as they were before the entry's change, so
        // their stamps are too: the author's next entry stays fresh, and
        // another author's change since would have made this one stale.
        for (node, stamp) in &entry.before {
            match stamp {
                Some(g) => self.stamps.insert(*node, *g),
                None => self.stamps.remove(node),
            };
        }
        let stacks = self.authors.get_mut(&author).expect("inserted above");
        match dir {
            Direction::Undo => stacks.redo.push(reverse),
            Direction::Redo => stacks.undo.push(reverse),
        }
        Ok(Outcome::Applied(change))
    }

    fn check_fresh(&self, touched: &BTreeSet<Node>, base: Generation) -> Result<(), Rejected> {
        let nodes: Vec<Node> = touched
            .iter()
            .filter(|n| self.stamps.get(n).is_some_and(|g| *g > base))
            .copied()
            .collect();
        if nodes.is_empty() {
            Ok(())
        } else {
            Err(Rejected::Stale { base, nodes })
        }
    }

    /// Installs `applied` as the next generation and returns the change
    /// and the entry that reverses it.
    fn commit(&mut self, author: AuthorId, command: Command, applied: Applied) -> (Change, Entry) {
        self.generation = Generation(self.generation.0 + 1);
        let before = applied
            .touched
            .iter()
            .map(|n| (*n, self.stamps.insert(*n, self.generation)))
            .collect();
        self.document = applied.document;
        let entry = Entry {
            command: applied.inverse,
            made: self.generation,
            before,
        };
        let change = Change {
            generation: self.generation,
            author,
            command,
            touched: applied.touched,
        };
        (change, entry)
    }
}
