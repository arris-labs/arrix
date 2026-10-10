# Plan: boundary-m2

- Started: 2026-10-10
- Cycle: the boundary opened, M2 (the session crosses); ADR-0016
- Idea: docs/ideas/the-session-crosses.md (absorbed: A1, A2 and A3 as
  recommended; unload to the backlog; the Python `serve` client in M3)

## Goal

An agent drives a running ArriX from outside its source. `arrix serve
--stdio` puts the `Session` protocol on JSON-RPC. It has one method per
request, events arrive as notifications, a `Rejected` comes back as a
result's `err`, meshes and blobs travel as base64, and the protocol is
described by JSON Schemas that the gate keeps fresh. A Tier 2 plugin is
loaded into the running session, and reloaded after an edit, by a request.
The authority owns the registry, and each load or reload makes a new
generation, so the features that were frozen for lack of a type evaluate
live and no client learns a second ordering key. `arrix-app render`
renders a saved document headless to a PNG plus `debug_state()` JSON,
through its own offscreen path. Its pixels are held to the visual
harness's by a parity test. The risk retired: the `Session` protocol holds
across a process boundary.

## Non-goals

- `arrix test`, the harness's versioning and stability promise, and the
  cycle's accept: M3.
- A Python client for `serve` (`arrix.session` in the SDK): M3, with the
  kit. M2's accept drives `serve` from Rust.
- Unloading a plugin from a running session: backlog.
- Rendering inside `serve`, or `serve` in `arrix-app` (ADR-0025 says why).
- Any transport beyond stdio, several clients of one `serve`,
  authentication, remote mode.
- Loading plugins in the GUI (a menu or a watcher). The app only follows a
  `SessionEvent::Registry` that it hears.
- Any document schema change: a registry change is never persisted.

## Design deltas

- **ADR-0023, the session on the wire.** `serve` speaks the protocol's
  existing serde forms, with these methods: `session.submit`, `undo`,
  `redo`, `preview`, `clear_preview`, `export`, `save`, `load_plugin` and
  `reload_plugin`. Events are `session.event` notifications carrying a
  `SessionEvent`. The `initialize` handshake carries the session protocol's
  version. `serve`'s codec writes mesh arrays, `Opened` blobs and export
  bytes as base64 (ADR-0022 §5). The schemas are derived (`schemars`,
  behind a `schema` feature). This supersedes ADR-0022 §3 for `serve`
  only: §3 keeps the plugin world.
- **ADR-0024, the authority owns the registry.** The new requests are
  `Request::LoadPlugin { dir }` and `ReloadPlugin { plugin }`, and neither
  is a command (no undo, no gesture). Each makes a new generation, stamping no
  node, announced by `SessionEvent::Registry { generation, added, removed }`
  with the feature-type specs; `Opened` gains the specs loaded after open. A reload drops the evaluator's cache entries for its
  types. A `PluginLoader` trait in `arrix-doc` is handed to
  `LocalSession::open` and implemented by `arrix-plugin-host`. With no
  loader (wasm, or a test), a load is `Rejected::NoPluginLoader`.
- **ADR-0025, the render stack ships in the native app.** `arrix-app
  render <doc> [--plugin <dir>]… [--view …] [--size WxH] -o <png> [--json
  <file>]` builds the real app on an `egui::Context` and paints through
  `egui_wgpu::Renderer` into a texture. It prefers a CPU adapter, falls
  back to any other, and names the adapter in the JSON. Pixels are
  deterministic on lavapipe only. `egui_kittest` stays dev-only.
- **`arrix-doc`**: `Request::LoadPlugin` and `ReloadPlugin`;
  `SessionEvent::Registry` and `Opened`'s loaded specs; `Rejected` gains `NoPluginLoader` and
  `PluginLoad(Diagnostic)`; `Rejected` and `CommandError` gain
  `Serialize` and `Deserialize`; the `PluginLoader` trait; the evaluator
  swaps its registry between evaluations; `Replica::receive` takes
  `SessionEvent::Registry` (no document change).
- **`arrix-plugin-api`**: `schemars` derives behind the `schema` feature
  on the types a command or event carries. API: minor, an additive
  feature-gated derive.
- **`arrix-plugin-host`**: a `PluginLoader` from a plugin directory
  (Tier 2, through M1's `register_tier2`), with reload restarting the
  process.
- **`arrix-cli`**: `arrix serve --stdio [--plugin <dir>]… [<doc>]` and
  `arrix serve --schema` (the protocol's schemas); a `serve` module that is
  native only.
- **`arrix-app`**: the `render` subcommand and its offscreen module; the
  app follows `SessionEvent::Registry` (its command registry and forms
  rebuilt from the specs).
- **No document schema change**, and no `.arrx` migration.

## Arris dependencies

None new. `serve`, load and render reach the kernel only through calls
`arrix-kernel` already makes on the pinned `arris` 0.6.

## Steps

- [x] **[2]** Step 1: ADR-0023 and ADR-0024 written and indexed, and the
  idea file deleted. Docs only.
- [ ] **[3]** Step 2: `arrix-app render`'s offscreen path, plus ADR-0025.
  `tests/docs/slice` is rendered to PNG and `debug_state()`, waiting for
  evaluation to be idle. A parity test compares its coarse frame with a
  visual-harness scenario of the same document at the same size, within
  the harness's tolerance and with zero budget. ADR-0025 is written from
  the result; if parity fails, the step stops for the human (option B3,
  shipping `egui_kittest`).
- [ ] **[2]** Step 3: render's surface. `--view`, `--size`, `--plugin`,
  and `--without-plugin` so a frozen feature renders with its badge. The
  JSON carries the adapter and a render format version. A document that
  fails to open, or has no adapter, exits non-zero with its diagnostic.
  One test per flag, and a frozen-slice render whose JSON shows the badge.
- [ ] **[3]** Step 4: the schemas. `schemars` derives behind `schema` on
  the protocol types in core, plugin-api and doc. The JSON of every
  existing session round-trip case is validated against its schema
  (`jsonschema` as a dev-dependency). `arrix serve --schema` prints them,
  held fresh under `UPDATE_SNAPSHOTS=1`. Where a derive disagrees with the
  serde form (internally tagged enums, transparent ids), that type's
  schema is hand-written, with its examples validated.
- [ ] **[2]** Step 5: `Rejected` and `CommandError` serialise. Each case
  joins the round-trip test and the schemas.
- [ ] **[2]** Step 6: `arrix serve --stdio`. It covers the handshake, the
  `session.*` methods, events as notifications (`subscribe` implicit at
  `initialize`, one client), the base64 bulk codec, a line limit, and the
  protocol errors (unknown method, bad params, version mismatch). A
  stale or refused submit comes back as a result's `err`. Tested on the
  spawned binary: the plate built by commands, its `Finished` and
  `Meshed` read, a stale submit refused, and `session.save` equal to the
  same commands' in-process save. Each protocol error has a test.
- [ ] **[2]** Step 7: load and reload in `LocalSession`, tested with a
  Tier 0 double loader in `arrix-doc`.
  - A load makes a generation with `SessionEvent::Registry`, and a frozen
    feature of the type evaluates live under it.
  - A reload of a changed type re-evaluates despite an equal version.
  - No loader gives `NoPluginLoader`, and a failed load gives
    `PluginLoad`.
  - Undo and redo across a registry change are unaffected.
  - A replica follows the change, and the app rebuilds its forms from it.
- [ ] **[2]** Step 8: plugins in `serve`. `arrix-plugin-host`'s loader is
  wired in, along with `session.load_plugin`, `session.reload_plugin` and
  `serve --plugin`. Tested on the spawned binary: `tests/docs/standoff`
  opened without the plugin is frozen, then loaded and live; the
  standoff's source is edited in a temporary copy and reloaded, and the
  new height is evaluated; a bad directory gives `PluginLoad`.
- [ ] **[2]** Step 9: M2's accept, `cargo test -p arrix-cli --test
  boundary_m2`.

## Acceptance

`cargo test -p arrix-cli --test boundary_m2`. A test, acting as the agent,
does the following:

1. It starts `arrix serve --stdio` with no plugins and loads
   `python/examples/standoff` by `session.load_plugin`.
2. It builds the plate, the standoff on its top face and a fillet naming
   the standoff's edge, all by `session.submit`, and reads each feature's
   outcome and the final mesh from notifications.
3. It submits stale and refused commands and reads both as results' `err`.
4. It edits a copy of the plugin, reloads it, and reads the new height.
5. It saves through `session.save` and renders the saved document with
   `arrix-app render`. The render's JSON shows the bodies and their
   bounds, and a second render without the plugin shows the badge.

The whole gate stays green, including step 4's schema freshness and step
2's parity test.

## Docs to update on completion

- `docs/ARCHITECTURE.md`:
  - §Crates: `serve` in `arrix-cli`, `render` in `arrix-app`, the
    `schema` feature.
  - §The protocol boundary: `serve` as the second transport, the wire
    `Rejected`, `LoadPlugin` and `ReloadPlugin`, `SessionEvent::Registry`,
    and
    "its wire form comes with serve" removed.
  - §Threads: `serve`'s reader and writer.
  - §Gates: the protocol schemas' freshness and the parity test.
- `docs/CONCURRENCY-WASM.md`:
  - §The evaluator: a registry change as a generation, and reload's cache
    drop.
  - §The browser build: no plugin loader.
- `docs/PLUGINS.md` §Three tiers: Tier 2 loaded and reloaded at runtime.
- `docs/DATA-MODEL.md` §Commands, or wherever `Change` is described:
  `SessionEvent::Registry`, never persisted.
- `docs/UI-RENDERING.md` §Visual debugging: the product render, its flags,
  its JSON and the parity test.
- `docs/adr/README.md`: ADR-0023 to ADR-0025 indexed.
- `docs/BACKLOG.md`: unloading a plugin from a running session; anything
  the steps defer.
- `docs/ROADMAP.md` status line: M2 done, its date.
- `AGENTS.md` current state and §Commands: `serve`, `render`,
  `--test boundary_m2`.

## Open questions

- Settled by step 1 (ADR-0023 §3): a request's response precedes every
  event its request caused; one writer thread owns `serve`'s stdout.
- Settled by step 1 (ADR-0023 §5): the schemas are printed by `arrix serve
  --schema`, not under `arrix config schema`.
- Settled by step 1 (ADR-0024 §3): a reload makes a generation even when
  no spec changed. The registry change is a `SessionEvent::Registry`, not
  a `Change` (a `Change` carries a command, and a load is none).
