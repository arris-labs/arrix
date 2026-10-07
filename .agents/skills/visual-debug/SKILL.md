---
name: visual-debug
description: See ArriX's GUI without a human describing it — render the app headlessly to a PNG you can read, and dump what the app thinks it drew as JSON. Use when working on anything visual and you need to check the result rather than reason about it (the shell's layout, the ribbon, the tree, the property panel, the status bar, later the viewport and sketch overlays), when a visual scenario fails, when asked what a screen looks like or whether a UI change worked, and before refreshing any golden under crates/arrix-app/tests/snapshots/.
---

# Seeing the ArriX GUI

You can look at this app. Do that instead of reasoning about pixels from
source: the real `ArrixApp` renders headlessly through wgpu on Mesa's
lavapipe, and `ArrixApp::debug_state()` reports what it believes it drew.
Never ask the human to describe the screen.

How the harness works and why: `docs/UI-RENDERING.md` §Visual debugging
and ADR-0001 (text goldens, images on demand). This file is how to *use*
it.

## Scratch or scenario?

**Looking at an arbitrary state** → the scratch render. Nothing is
compared, nothing is pinned.

```sh
cargo test -p arrix-app --test visual_scratch -- --nocapture
```

It writes `target/visual-scratch/scratch.png` and `scratch.json` and prints
both paths. Edit the body of `crates/arrix-app/tests/visual_scratch.rs` to
reach the state you want, run, then **read the PNG with the Read tool**; it
renders as an image. Revert your edits to that file before committing: it
is tracked, and its default body is the startup state.

**Pinning a state against regressions** → a scenario in
`crates/arrix-app/tests/visual/` (one module per area: `viewport`,
`highlight`, `camera`, `accept`, …), one golden per scenario in
`crates/arrix-app/tests/snapshots/<name>.json`.

```sh
cargo test -p arrix-app --test visual
```

Every run writes each scenario's full frame to
`target/snapshots/<name>.png`, pass or fail. Keep the suite small and aimed
at what no unit test reaches; interaction logic is unit-tested where it
lives.

## Reading the golden and the JSON

A golden is text, never an image:

- `debug_state`: the app's `debug_state()`, keys sorted, floats rounded to
  six places. Today: `window` (`[w, h]` in points), `layout` (each shell
  region as `[min_x, min_y, max_x, max_y]`), `shell` (the `ShellView` the
  frame was drawn from: ribbon tabs and the open one, tree parts, status),
  and for the active document `evaluation`, `bodies`, `camera` (with its
  `view_proj`), `selection`, `hover` and `picking`, each omitted while
  empty. The sketch fields join as those land.
- `coarse_frame`: the frame reduced to 16×16-pixel cells of mean RGB, one
  string of six hex digits per cell per row (`cell`, `size`, `rows`). A
  cell the frame's edge cuts short averages what it holds: 1440×900 is
  90×57 cells.

The picture says *that* something is off; the JSON says *which number* is
wrong. Read both.

## When a scenario fails

The failure names the first `debug_state` difference as a JSON pointer with
both values, and the coarse frame's differing regions as cell-aligned pixel
rectangles:

```
coarse frame: 270 cells differ by more than 8 (budget 0), in 1 region(s):
  x 0..1440, y 864..900 (270 cells, max delta 63)
```

Then look, before deciding anything:

```sh
scripts/snapshot-baseline          # or: scripts/snapshot-baseline <rev>
```

It renders the scenarios at `HEAD` (or `<rev>`) in a temporary worktree
and writes, per scenario, into `target/snapshots/`: `<name>.before.png`
(the revision), `<name>.png` (the working tree) and `<name>.diff.png`
(differing pixels in magenta over the dimmed frame), and prints how many
pixels differ. **Read all three.** The first run builds its own
`target/baseline/`, which takes a few minutes; later runs are quick.

If, and only if, the change is intended:

```sh
UPDATE_SNAPSHOTS=1 cargo test -p arrix-app --test visual
```

Then read the golden's `git diff` (the hex rows that moved and the
`debug_state` lines) before staging it, and say in the commit why the look
changed (`.agents/rules/git.md`). A suite refreshed by reflex is worse than
none, because it looks like coverage. Never commit an image.

## The knobs

| Knob | Default | Where |
|---|---|---|
| cell size | 16 px | `coarse::CELL` |
| per-cell tolerance | 8 of 255, any channel of the cell mean | `coarse::Tolerance::default()` |
| cell budget | 0 cells over the tolerance | `coarse::Tolerance::default()` |
| per scenario | a `Tolerance` of its own | `snapshot::check_with(h, name, tolerance)` |
| window | 1440×900 points | `harness::SIZE` |

Frames are identical run to run on lavapipe, so a scenario that needs a
budget is hiding something; raise one only for a reason written beside it.

| Environment variable | Effect |
|---|---|
| `UPDATE_SNAPSHOTS=1` | rewrite the goldens instead of comparing |
| `ARRIX_REQUIRE_GPU=1` | fail, not skip, without a CPU adapter (CI, the baseline script) |
| `ARRIX_SNAPSHOT_DIR=<dir>` | write full frames there instead of `target/snapshots/` |
| `ARRIX_SNAPSHOT_DIFF=1` | also write `<name>.diff.png` against `<name>.before.png` (set by the baseline script) |

## Writing a scenario or a scratch body

Helpers are in `crates/arrix-app/tests/visual/harness.rs`, the golden in
`snapshot.rs`, the reduction in `coarse.rs`.

| Need | Call |
|---|---|
| the real app at the reference size, skipping without lavapipe | `harness::with_app(name, \|h\| …)` |
| settle until egui stops asking for repaints | `harness::settle(h)` |
| anything that needs the GPU to have run | `harness::pump_rendered(h, n)` |
| open a document and wait until it is meshed refined | `harness::open_settled(h, "tests/docs/slice")` (`_without(h, path, &["gears"])` leaves a plugin out) |
| where a world point is on screen | `h.state().screen_point([x, y, z])` |
| click, hover, drag in the viewport | `set_render_every_step(true)`, then the helpers in `tests/visual/viewport.rs` (`click`) and `camera.rs`; wait on `debug_state().picking` |
| finish a camera flight | `h.state_mut().skip_animations()` |
| the frame | `harness::render(h)` |
| compare against the golden | `snapshot::check(h, name)` |
| the state dump | `h.state().debug_state()` / `debug_state_json()` |

## Pitfalls

1. **`Harness::step` does no GPU work.** Only `render` runs the paint
   callbacks. Anything that depends on a rendered frame (picking, hover)
   needs `pump_rendered` or `set_render_every_step(true)`; an answered pick
   is drawn by the frame after it, so step once more before reading pixels
   or the status hint. The harness's `drop_at` removes the pointer and so
   clears hover: hover again before asserting a hint.
2. **A click is one event per rendered frame.** A press and a release
   queued into one step produce a click frame that is computed and thrown
   away.
3. **Evaluation is asynchronous** (from M1): settle before asserting, or
   the frame shows a half-built model.
4. **No wall clock reaches a pixel.** A camera flight is finished with
   `skip_animations` before anything is read; no frame-time readout. A
   document with repaints pending never lets `run()` finish: `run_steps`.
5. **Scenarios run one at a time.** `with_app` holds the GPU lock;
   concurrent lavapipe devices crash inside the driver. Go through it.
6. **A skip is loud, not green.** Without a CPU adapter a scenario prints
   `SKIPPING …` and returns. Install `mesa-vulkan-drivers`. In CI a skip
   is a failure.

## Limits

- Native only: the harness is a dev-dependency and never in the wasm build.
- lavapipe is the reference on purpose; a hardware GPU rasterises
  differently, so a scenario never runs on one.
- A change smaller than a cell (a 1-px outline, a shifted glyph) is below
  the coarse frame; text is `debug_state()`'s to catch.
- Text queries (`get_by_label`, …) reach egui widgets, not the viewport or
  painter overlays; that is what `debug_state()` is for.
