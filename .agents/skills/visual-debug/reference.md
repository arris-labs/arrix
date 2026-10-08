# Visual harness reference

Helpers are in `crates/arrix-app/tests/visual/harness.rs`, the golden in
`snapshot.rs`, the coarse reduction in `coarse.rs`.

## Helpers

| Need | Call |
|---|---|
| the real app at the reference size, skipping without lavapipe | `harness::with_app(name, \|h\| …)` |
| settle until egui stops asking for repaints | `harness::settle(h)` |
| anything that needs the GPU to have run | `harness::pump_rendered(h, n)` |
| open a document, wait until meshed refined | `harness::open_settled(h, "tests/docs/slice")`; `open_settled_without(h, path, &["gears"])` leaves a plugin out |
| a world point on screen | `h.state().screen_point([x, y, z])` |
| click, hover, drag in the viewport | `set_render_every_step(true)`, then `click` in `tests/visual/viewport.rs` and the helpers in `camera.rs`; wait on `debug_state().picking` |
| finish a camera flight | `h.state_mut().skip_animations()` |
| the frame | `harness::render(h)` |
| compare against the golden | `snapshot::check(h, name)` |
| the state dump | `h.state().debug_state()` / `debug_state_json()` |

## Pitfalls

1. **`Harness::step` does no GPU work**; only `render` runs the paint
   callbacks. Picking and hover need `pump_rendered` or
   `set_render_every_step(true)`. An answered pick is drawn by the next
   frame, so step once more before reading pixels or the status hint.
   `drop_at` removes the pointer and clears hover: hover again before
   asserting a hint.
2. **One click per rendered frame.** A press and release queued into one
   step produce a click frame that is computed and thrown away.
3. **Evaluation is asynchronous**: settle before asserting, or the frame
   shows a half-built model.
4. **No wall clock reaches a pixel.** Finish camera flights with
   `skip_animations`. A document with repaints pending never lets `run()`
   finish: use `run_steps`.
5. **Scenarios run one at a time.** `with_app` holds the GPU lock, because
   concurrent lavapipe devices crash the driver; always go through it.
6. **A skip is loud, not green.** Without a CPU adapter a scenario prints
   `SKIPPING …` and returns; install `mesa-vulkan-drivers`. In CI a skip
   fails.

## Knobs

| Knob | Default | Where |
|---|---|---|
| cell size | 16 px | `coarse::CELL` |
| per-cell tolerance | 8 of 255, any channel of the cell mean | `coarse::Tolerance::default()` |
| cell budget | 0 cells over the tolerance | `coarse::Tolerance::default()` |
| per scenario | its own `Tolerance` | `snapshot::check_with(h, name, tolerance)` |
| window | 1440×900 points | `harness::SIZE` |

Frames are identical run to run on lavapipe, so a scenario that needs a
budget is hiding something; raise one only with the reason written beside
it.

| Environment variable | Effect |
|---|---|
| `UPDATE_SNAPSHOTS=1` | rewrite the goldens instead of comparing |
| `ARRIX_REQUIRE_GPU=1` | fail, not skip, without a CPU adapter (CI, the baseline script) |
| `ARRIX_SNAPSHOT_DIR=<dir>` | write frames there instead of `target/snapshots/` |
| `ARRIX_SNAPSHOT_DIFF=1` | also write `<name>.diff.png` against `<name>.before.png` (the baseline script sets it) |

## Limits

- Native only: the harness is a dev-dependency, never in the wasm build.
- lavapipe is the reference on purpose; a hardware GPU rasterises
  differently, so scenarios never run on one.
- A change smaller than a cell (a 1-px outline, a shifted glyph) is below
  the coarse frame; text is for `debug_state()` to catch.
- Text queries (`get_by_label`, …) reach egui widgets, not the viewport or
  painter overlays; that is what `debug_state()` is for.
