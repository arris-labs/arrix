# ADR-0009: A frozen result is evaluation output, written on save

- Status: Accepted
- Date: 2026-09-30

## Context

Every plugin feature stores its last successful result in the document
(`docs/DATA-MODEL.md` §Frozen results), so a machine without the plugin
can open the file and evaluate against it. That result is not authored:
it is what the plugin computed from the feature's inputs. The document
changes only by `Command`s (`SEED.md` §6.1), each with a generation, an
author, an inverse and a place in per-author undo, and the model is
silent on how a result gets from the evaluator into the file. `plans/
c1-m1-slice` (step 13) has to write it, and OPEN 2 of that plan asked
the human's side to decide between two shapes. It was decided by the
agent, on the plan's own recommendation.

## Decision

1. **A frozen result is evaluation output, not a command.** The session
   keeps the latest live result of each plugin feature, with the input
   hash it was computed from, beside the document as it keeps generation
   stamps. It is session state, not document state.
2. **`save` writes it.** Saving puts the kept result into the feature's
   record and its blobs when the kept input hash equals the feature's
   current one, and otherwise keeps what the document was opened with.
   A feature the session never evaluated live keeps its stored result
   unchanged.
3. **It has no undo entry and no generation**, and it is never
   stale-rejected: a command's `base` names document generations, and a
   frozen result is not one.
4. **Determinism is the invariant.** Evaluation is deterministic (same
   inputs, same bytes), so undo, re-evaluate and save gives the bytes of
   the earlier save, which is what the undo scenarios assert.
5. **Staleness is decided on load, not stored.** A frozen feature is
   stale when its current input hash differs from the stored one; the
   stored result is kept until the plugin returns and re-freezes it.

## Consequences

- The command stream holds authored gestures only, which is what a
  future server-and-thin-client mode needs: a client never replays
  machine output as if a person had made it.
- A generation moves only when the document's authored content does, so
  evaluating twice does not change a generation, a hash of the document
  or an undo stack.
- Save is the one place the result reaches the file, so a document that is
  evaluated and never saved carries the result it was opened with. Nothing
  else reads the stored result while the plugin is present.
- Two sessions on one document each keep their own latest results; the
  authority's document does not, so a saved file carries the result of the
  session that saved it.

## Alternatives considered

- **A non-undoable `Freeze` command** issued by the session after each
  evaluation. It puts machine output in the command stream, moves the
  generation on every evaluation, and makes "undo returns the earlier
  bytes" depend on when the evaluator finished.
- **Writing the result into the record on every evaluation**, outside
  the command path: the document then changes without a generation, so a
  client replica cannot tell that it did.
