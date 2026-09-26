# ADR-0005: The client solves a sketch edit; the authority applies it without solving

- Status: Accepted
- Date: 2026-09-27

## Context

A sketch changes by gestures: a line drawn, a dimension typed, a point
dragged, a curve trimmed. Each ends in a solve, and the solver is the one
piece of the kernel side allowed on the UI thread (`SEED.md` §6.1), since
a drag must answer within a frame (`docs/CONCURRENCY-WASM.md` §Budgets).
The document stores a sketch's last solved positions as recipe
(`docs/DATA-MODEL.md` §Sketches): the solver is nonlinear, its starting
point selects the branch it lands on, and stored positions keep a sketch
on its branch after an edit.

Three parties could run the solve that follows a gesture: the client that
made it, the authority that applies the command, or the evaluator that
turns the document into geometry. The command boundary is a protocol even
locally (`SEED.md` §6.1): the authority may one day be a server applying
commands from several thin clients, and undo is by inverse commands that
must restore the document exactly.

## Decision

The client solves. A `SketchEdit` carries the records the gesture changed,
the positions the client's solve produced among them, and the authority
applies it as data, without solving. The evaluator re-solves every
evaluation from the stored positions, with each dimension's expression
resolved against the current parameters, and never writes its result back
into the document.

## Consequences

- Applying a command stays a pure function of the document and the
  command, so its inverse is exact (the previous records) and the
  apply-then-inverse property covers sketch edits with no solver in the
  loop. Undo restores the earlier positions byte for byte.
- The authority runs no solver: a server needs none, and a command log
  replays without one.
- What the user saw is what is stored: the branch the drag chose is the
  branch the document keeps, and the evaluator starts from it.
- A parameter edit does not rewrite positions. The evaluator solves from
  the stored ones with the new values, so a sketch far from its stored
  pose after a large parameter change may land on another branch; the
  next sketch gesture stores the new pose. A solve that does not converge
  fails the feature soft and names the entities (`core.sketch`).
- A client could submit positions that do not satisfy the constraints.
  That is harmless: they are a starting point, and the evaluator's solve
  decides the geometry.

## Alternatives considered

- **The authority solves**: applying a command stops being pure, its
  inverse depends on a solver's output, and a server needs the solver
  and must answer a drag within a frame over the network.
- **The evaluator writes back its solved positions**: evaluation would
  mutate the recipe outside a command, with no undo entry and no author.
- **No stored positions, solve from a canonical start**: the branch the
  user chose is lost on the next edit, which is what storing them fixes.
