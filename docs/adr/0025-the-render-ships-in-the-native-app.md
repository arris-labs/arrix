# ADR-0025: The headless render ships in the native app

- Status: Accepted
- Date: 2026-10-10

## Context

ADR-0016's scenario ends with the agent rendering what it built and
checking the result, and constraint 5 makes the headless render product.
Until now it existed only as the visual harness: `egui_kittest` with a wgpu
device, a dev-dependency of `arrix-app`'s tests (`docs/UI-RENDERING.md`
§Visual debugging). The layer rule (`SEED.md` §6.7) keeps wgpu out of
`arrix-cli`, so `arrix serve` cannot draw.

The options were:

- ship `egui_kittest` in the binary;
- give the app its own offscreen path;
- render inside `serve`, which would move `serve` into `arrix-app`;
- render the viewport alone.

Measured in plans/boundary-m2 step 2, on Mesa 25.2.8's lavapipe: an
offscreen path that steps the app as the harness does (the same raw input,
style, theme and frame step, the same CPU-first adapter choice and
`RendererOptions::PREDICTABLE`) draws `tests/docs/slice`, settled at
1440×900, with **zero pixels** different from the harness's frame and the
same `debug_state()`. The whole command, from start to written PNG, takes
2.5 s.

## Decision

1. **`arrix-app render <doc> -o <png> [--json <file>]`** is a product
   command of the native app. It opens the document on the built-in
   configuration, waits until the document is evaluated and every body is
   meshed refined, finishes any camera animation, and renders one frame.
   The JSON is `{adapter, debug_state}`.
2. **The app has its own offscreen path** (`arrix_app::render`). The app
   is built on an `egui::Context` with a wgpu render state
   (`ArrixApp::headless`), stepped through `ArrixApp::show`, and painted
   through `egui_wgpu::Renderer` into a texture that is read back.
   `egui_kittest` stays a dev-dependency.
3. **Pixels are promised on lavapipe only.** The render prefers a CPU
   adapter and draws on any other, and the JSON names the adapter, so a
   reader can tell which promise applies. `debug_state()` does not depend
   on the adapter.
4. **The harness holds the render.** The `render_parity` scenario draws
   one document by both paths on one device and requires identical pixels
   and the same `debug_state()`. A change to either path that moves a
   pixel fails it.
5. **Rendering stays out of `serve`.** An agent saves through `serve` and
   renders the saved document. The headless `arrix` binary never links
   wgpu, and a session no agent renders never starts a GPU.

## Consequences

- An agent sees what it built with one command and no test tree. The
  picture is the one a scenario's golden would hold.
- `image`, `pollster` and `clap` become native dependencies of
  `arrix-app`, and none of them reaches the wasm build.
- The render's JSON format falls under the stability promise of
  ADR-0016 constraint 5 when M3 versions the harness.
- Drawing a live session's state needs a save first. A render request
  inside `serve` would need `serve` to own a GPU, which this ADR declines.

## Alternatives considered

- **Shipping `egui_kittest`.** It would match the harness by construction,
  but a testing library's API and its accesskit plumbing would become
  product. The measured parity makes this unnecessary.
- **Rendering inside `serve`.** It saves a file round trip, but puts wgpu
  into the headless binary and every agent's session, against the layer
  rule's intent.
- **The viewport alone, through `arrix-viewport`.** It loses the tree,
  forms and frozen badges an agent checks, and `debug_state()` belongs to
  the app.
