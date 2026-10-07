# ADR-0019: A preview is a speculative evaluation

- Status: Accepted
- Date: 2026-10-08

## Context

M3's accept shows the extrude's preview before its result, and a create
form is a draft that issues no command until OK (ADR-0018). So the viewport
has to show what a command would make without the command being applied:
no generation, no undo entry, nothing in the file. The client cannot
evaluate it itself, since the kernel never runs on the UI thread and a thin
client has no kernel at all (`SEED.md` §6.1). The evaluator is one worker
that already evaluates the newest generation and starts over when a newer
one arrives (`docs/CONCURRENCY-WASM.md` §The evaluator), and a preview
competes with that for it.

## Decision

1. **A preview is a request, not a command.** `Request::Preview { preview,
   envelope }` asks the authority to evaluate its document with the
   envelope's command applied, and to apply nothing. The authority checks
   it as it checks a submit, refused or stale by the same rules, and
   otherwise answers `Outcome::Previewing`. It makes no generation, stamp
   or undo entry; a document's bytes and generation are the same after any
   number of previews.
2. **A preview is session state, named by the client.** The client counts
   its previews (`PreviewId`), and the events of one carry its
   `PreviewTag` (author and id): `Previewed`, every feature's outcome,
   every parameter's value and the final bodies, each saying whether it
   differs from the generation's own; then a `PreviewMeshed` for each body
   that does, coarse first, then refined. A replica ignores them.
3. **One preview per session at a time.** A newer preview supersedes the
   one before it, whoever took it; `Request::ClearPreview { author }`
   drops the author's. The session publishes a preview's events only while
   it is the session's preview, so a superseded or cleared one's late
   events are never heard.
4. **A commit comes first** (OPEN 4 of `plans/forms`, decided here). A
   preview waits until the newest commit is evaluated and meshed, and
   never stops it. A running preview stops between features or meshes for
   anything newer: a commit, a newer preview or a clear. An applied change
   ends the preview, since it was taken on an older document: the client
   previews again against the new generation if it still wants one.
5. **The cache is shared.** A preview evaluates on the same evaluator, so
   what it computes is cached by input hash, and the command applied after
   it finds its result there: OK after a preview costs no second
   evaluation, and its body is the one previewed, mesh included. A
   preview's plugin results are never kept for a save (ADR-0009).

## Consequences

The client gets a preview in the same shape as a real evaluation, over the
same protocol a thin client would use, and draws it by the same mesh path.
A form's preview never slows the evaluation of what the user committed. A
preview the user keeps editing is superseded field by field, so only the
latest is evaluated to the end. The price is a second document state
evaluated on the worker, and a client that must ask again after every
applied change while it previews. With several clients on one session (a
future server), one author's preview supersedes another's; per-author
previews wait for that server, and need either an evaluator each or a
queue fair between them.

Amends `docs/ARCHITECTURE.md` §The protocol boundary and
`docs/CONCURRENCY-WASM.md` §The evaluator. Supersedes nothing.

## Alternatives considered

- **Apply, then undo**: two generations and an undo entry per keystroke,
  in every replica and in the applied stream, for a state the user never
  committed.
- **Evaluate on the client**: the kernel on the UI thread, and nothing a
  thin client could do.
- **A preview that interrupts a commit's evaluation**: the user's
  committed work waits on a speculation, and the next commit would
  re-evaluate it anyway.
- **Queue a preview behind a commit and keep it**: a commit changes the
  document the preview was taken on, so its result would be of a state
  that no longer exists.
- **Preview-tagged `Meshed` events**: a tag every consumer of `Meshed`
  has to remember to check; a `PreviewMeshed` of its own can't be mistaken
  for the document's mesh.
