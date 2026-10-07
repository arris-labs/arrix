# ADR-0017: The minimal shell

- Status: Accepted
- Date: 2026-10-07

## Context

The shell M0 built docks five regions: ribbon, feature tree, viewport,
property panel and status bar. On an empty start that is two empty side
panels around an empty viewport. M3's viewport port is the first plan with
something to show, and the human's direction for it is a minimal interface:
panels appear only on demand, the object tree floats over the 3D view
instead of taking a full panel, and only two bars are docked.

Several open documents at once is outside M3's in-list (one document), so
this ADR names the scenario it unlocks.

Product scenario: a user keeps a plugin's test document open beside the
part it goes into and switches between them with a click, each where they
left it (its camera, selection and cards).

## Decision

1. **Exactly two docked bars.** The ribbon on top; at the bottom the tab
   strip of the open documents with the status line under it. The viewport
   fills everything between them.
2. **The tree and the property panel are cards**, overlays on the viewport
   anchored to its rect (fixed, not draggable, so a frame's layout stays a
   function of the window size and of which cards show), never docked
   panels. A pointer over a card never reaches the viewport. Every card is
   hidden until it has something to show or is asked for.
3. **The status line is help text** on the left (what the hovered button,
   palette row or viewport entity is and does, with its keys; else the
   gestures the viewport takes) and the diagnostics count and units on the
   right.
4. **Several documents open at once**, a tab each, with its own session,
   scene, camera, selection and cards. The active tab is session state and
   issues no command. Open, switch and close only; no reordering, no
   unsaved-changes prompt (nothing here issues a command).
5. **Settings.** `ui.panel.tree.visible` and `ui.panel.properties.visible`
   (bool) become `ui.panel.tree` and `ui.panel.properties`, each `"auto" |
   "shown" | "hidden"`, default `"auto"`. `auto` shows the tree card
   whenever the active document has a part, and the property card while a
   feature is being edited. The old keys are a diagnostic naming the new
   one: config has no users yet, so no migration beyond the message. The
   toggle commands flip the session's visibility over the setting.

## Consequences

The empty start is a ribbon, a viewport, an empty tab strip and a hint. The
layout is a function of the window and of which cards show, which the
golden layouts keep deterministic. Per-document session state makes a
future server-and-thin-client mode no harder: it is all client state
(`SEED.md` §6.1). A setting with three values needs an enum kind in
`arrix-config`.

Amends `docs/UI-RENDERING.md` §The shell (its diagram was never an ADR).
Supersedes nothing.

## Alternatives considered

- **Docked side panels that collapse**: still reserve the edge and shrink
  the viewport; the human asked for floating.
- **Draggable cards**: a layout that depends on a saved position is not a
  function of the window; later, if asked.
- **Keep the bool settings and add `auto` elsewhere**: two keys for one
  thing; the rename costs nothing while there are no users.
- **One document at a time**: the scenario above needs two.
