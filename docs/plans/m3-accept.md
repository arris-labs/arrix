# Plan: m3-accept

- Started: 2026-10-08
- Cycle: C1 M3 (the frozen badge, the gesture-count test over the
  acceptance flow, and M3's closing scenario: the rest of M3's accept)
- Idea: none; verbatim from the human: "yes, plan it" (M3's last plan:
  frozen badge, gesture-count test, closing scenario).

## Goal

M3's accept is met by one scenario, `m3_accept`, driven through the UI by
pointer and keys only: a sketch drawn with its inferences, the extrude's
preview and result, a face picked, a spur gear made from the gears tab and
its form on a datum plane and joined, a fillet too big for its edge failing
in the tree while the rest evaluates, and then the document reopened as a
build without `gears` would: the gear frozen in the tree, its form read
only with the reason, and a **frozen badge** on the bodies its result
lives in; the plate's width edited, and the badge and the tree say
frozen-stale. Every gesture of the flow is in a **gesture log**, and the
scenario checks the log over the whole flow: each gesture applied one
command or none, a gesture expected to change nothing applied none, and
the log accounts for every command the document applied, so no command
is issued outside a gesture.

## Non-goals

- Saving to a file from the UI, and an open dialog: the reopen goes through
  `ArrixApp::saved()` and `open_without`, as `forms_accept` does.
- M4: export, `arrix eval` completion, the C1 benchmark and its budgets.
- A badge for anything but a frozen feature (failed, suppressed or rolled
  back features show in the tree only). Badges on the preview's bodies.
- The tree's own edits (rollback bar, suppress, delete, reorder): out of
  M3's accept, still in `docs/UI-RENDERING.md` §The shell.
- Re-running or merging the existing accepts (`viewport_accept`,
  `sketch_accept`, `forms_accept`): they stay as they are; `m3_accept`
  is one more flow, not a replacement.
- Backlog lines about frozen features (the stored slot's target, an
  edge-sourced axis): no scenario here needs them.

## Design deltas

- `docs/UI-RENDERING.md` §The shell, gestures: the app keeps a **gesture
  log** of closed gestures, each with what opened it (a key's command, an
  intent, a pointer gesture's kind) and the commands it applied, as session
  state; `debug_state()` reports it (`gestures`, capped at the last 256,
  agent's choice) beside `gesture_commands`, and the active document's
  applied total (`commands_applied`), so a scenario can check that the
  log's sum is the total. A command applied with no gesture open is a
  bug the check catches, not a case the log records.
- `docs/UI-RENDERING.md` §The viewport, overlays: a **frozen badge** is
  anchored on the faces of a body whose name root's feature is the frozen
  feature (`NameRoot::feature`; a frozen body is named by its stored table,
  so its faces keep their live roots, `Sweep { feature }` for the gear, and
  no code mints `NameRoot::Frozen`, found at step 2): the scene, GPU-free,
  gives each body's per-feature bounds of its faces (`arrix-viewport`), the app projects their centre and gives `arrix-ui` a
  `BadgeView` (feature title, `frozen` or `frozen, stale`, screen
  position) to paint over the viewport; a joined body carries the badge
  at the frozen feature's faces, not at the body's centre. Hovering the
  badge puts the reason in the status line (the form's words:
  "Feature type gears.spur is not available"). `debug_state()` lists the
  badges drawn.
- `docs/UI-RENDERING.md` §View types: `BadgeView`.
- No ADR: both are within ADR-0017 (overlays) and `SEED.md` §8.2 rule 2.
  No schema change, no plugin-API change.

## Arris dependencies

None new. The flow uses what `arris` 0.5 already gives the evaluator:
extrude, fillet (its failure category), boolean join, tessellation with
names. The frozen result's names (`NameRoot::Frozen`) are ArriX's own
(ADR-0009), so whether they survive a downstream join and fillet is
probed at step 2, in this repository.

## Steps

- [x] **[2]** Step 1 — The gesture log. `Gestures` keeps the closed
  gestures (what, count); `debug_state()` gains `gestures` and
  `commands_applied`. Every path that applies a command (keys, intents,
  ribbon and palette, tree, form fields and OK, sketch tools, undo/redo,
  `submit`) runs inside a gesture; a `debug_assert` in `close_gesture` and
  a test in `crates/arrix-app/tests/edit.rs` that the log's sum equals
  the applied total after a mixed sequence (a sketch edit, a form OK, an
  undo, a refused field, a hover). Goldens gain the two keys only (read
  in the diff).
- [x] **[3]** Step 2 — Where a frozen feature's faces are. A probe test in
  `arrix-cli` (`tests/frozen_faces.rs`; `arrix-doc` can't evaluate the
  gear to store its result) on `tests/docs/slice` (the gear joined by
  `core.boolean`) opened without `gears`, with a fillet on a plate edge
  added by command: the frozen gear's faces keep their gear-rooted names
  on the final body after the join and the downstream fillet. Then the
  scene's per-body bounds of the faces of each frozen root
  (`arrix-viewport`, GPU-free, unit tested on a mesh with two roots).
  If the probe finds the names gone, the step stops and the badge's
  anchor goes back to the human.
- [ ] **[2]** Step 3 — The frozen badge drawn. `BadgeView` in `arrix-ui`
  and its painter, the app's projection and its hover reason in the
  status line, the badges in `debug_state()`. A visual scenario
  `frozen_badge`: `tests/docs/slice` opened without `gears` shows the
  badge on the gear's faces, frozen; a width edit through the form makes
  it frozen-stale. Golden written, frame read before it is committed.
- [ ] **[2]** Step 4 — `m3_accept`, part mode up to the failed fillet:
  new document, the plate's sketch drawn with an inference shown and
  committed, the extrude's preview then OK, a face of the plate picked,
  a datum plane made, the gears tab opened and the spur gear's form
  filled on the plane, joined, a fillet too big for its edge. Snapshots
  at the preview (`m3_accept_preview`), the gear's form with its tab
  (`m3_accept_gear`) and the tree after the failed fillet
  (`m3_accept_failed`); each frame read.
- [ ] **[2]** Step 5 — `m3_accept`, reopened without the plugin, and the
  gesture-count test: `saved()` opened with `open_without(&["gears"])`;
  the tree shows the gear frozen, its form read only with the reason, the
  badge on the gear's faces, the bodies' volume as the live run's;
  the plate's width edited through its form: the gear frozen-stale in
  the tree and the badge, the rest re-evaluated. Snapshot
  `m3_accept_frozen`. Then the log over the whole flow: every count 0
  or 1, the gestures expected to change nothing at 0 (opening a form,
  a pick, a hover, Cancel, a refused text), the sum equal to the
  applied total of each document.

## Acceptance

`cargo test -p arrix-app --test visual m3_accept` green against its
goldens, with `frozen_badge` and the earlier scenarios unchanged
(`cargo test -p arrix-app --test visual`), and the whole gate green
(`.githooks/pre-commit`, `.githooks/pre-push`).

## Docs to update on completion

- `docs/UI-RENDERING.md` §The shell (the gesture log), §View types
  (`BadgeView`), §The viewport (the badge's anchor and hover), §Visual
  debugging (the `m3_accept` scenario and its goldens) — in the steps'
  commits.
- `docs/ROADMAP.md` C1 status line: M3 landed (plan 5 `m3-accept`), M4
  next.
- `AGENTS.md` current state: M3 landed, the accept's command
  (`cargo test -p arrix-app --test visual m3_accept`); now M4, to be
  planned.

## Open questions

- ⚠ OPEN: the badge's look: a chip ("Spur gear 1 · frozen") at the frozen
  faces' centre, as proposed, or the frozen faces tinted (hatched) as
  well. Human, before step 3.
- Settled at step 2: a frozen result's names survive the join and a
  downstream fillet, so the badge goes on the frozen feature's faces. The
  probe also found that an edge of the gear the join left alone can't be
  read by a downstream feature (`slot.consumed`, live or frozen: its name
  roots at the consumed tool slot); a backlog line, not this plan's. Step
  4's fillet "too big for its edge" goes on a plate edge.
