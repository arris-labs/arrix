---
name: visual-debug
description: Renders ArriX's GUI headlessly to a PNG the agent can read, and dumps what the app believes it drew as JSON. Use when working on anything visual (shell layout, ribbon, tree, property panel, forms, status bar, viewport, sketch overlays) and the result needs checking rather than reasoning about, when a visual scenario fails, when asked what a screen looks like or whether a UI change worked, and before refreshing any golden under crates/arrix-app/tests/snapshots/.
---

# Seeing the ArriX GUI

The real `ArrixApp` renders headlessly through wgpu on Mesa's lavapipe, and
`ArrixApp::debug_state()` reports what it believes it drew. Look at it
instead of reasoning about pixels from source, and never ask the human to
describe the screen. Why the harness is built this way: `docs/UI-RENDERING.md`
§Visual debugging and ADR-0001 (text goldens, images on demand).

Before writing a scenario or a scratch body, read
[reference.md](reference.md): the helpers, knobs and pitfalls.

## Scratch or scenario?

**Looking at an arbitrary state** → the scratch render, compared against
nothing:

```sh
cargo test -p arrix-app --test visual_scratch -- --nocapture
```

Edit the body of `crates/arrix-app/tests/visual_scratch.rs` to reach the
state, run, then read `target/visual-scratch/scratch.png` with the Read
tool (and `scratch.json` beside it). The file is tracked and its default
body is the startup state: revert your edits before committing.

**Pinning a state against regressions** → a scenario in
`crates/arrix-app/tests/visual/` (one module per area), with its golden in
`crates/arrix-app/tests/snapshots/<name>.json`:

```sh
cargo nextest run -p arrix-app --test visual   # or cargo test -p arrix-app --test visual
```

Every run writes each scenario's frame to `target/snapshots/<name>.png`,
pass or fail. Keep the suite small and aimed at what no unit test reaches;
interaction logic is unit-tested where it lives.

## Reading the golden

A golden is text, never an image:

- `debug_state`: `debug_state()` with keys sorted and floats rounded to six
  places: `window` (`[w, h]` points), `layout` (each shell region as
  `[min_x, min_y, max_x, max_y]`), `shell` (the `ShellView` drawn), and for
  the active document `evaluation`, `bodies`, `camera` (with `view_proj`),
  `selection`, `hover`, `picking` and the sketch fields, each omitted while
  empty.
- `coarse_frame`: the frame as 16×16-pixel cells of mean RGB, six hex
  digits per cell per row. 1440×900 is 90×57 cells (edge cells average what
  they hold).

The picture says *that* something is off; the JSON says *which number*.
Read both.

## When a scenario fails

The failure names the first `debug_state` difference as a JSON pointer with
both values, and the differing coarse-frame regions as pixel rectangles:

```
coarse frame: 270 cells differ by more than 8 (budget 0), in 1 region(s):
  x 0..1440, y 864..900 (270 cells, max delta 63)
```

Look before deciding:

```sh
scripts/snapshot-baseline          # or: scripts/snapshot-baseline <rev>
```

It renders the scenarios at `HEAD` (or `<rev>`) in a temporary worktree and
writes, per scenario, `target/snapshots/<name>.before.png`, `<name>.png`
(the working tree) and `<name>.diff.png` (differing pixels in magenta),
printing the pixel counts. Read all three. The first run builds
`target/baseline/` in a few minutes; later runs are quick.

Only if the change is intended:

```sh
UPDATE_SNAPSHOTS=1 cargo test -p arrix-app --test visual
```

Then read the golden's `git diff` (moved hex rows, `debug_state` lines)
before staging, and say in the commit why the look changed
(`.agents/rules/git.md`). A suite refreshed by reflex looks like coverage
and catches nothing.
