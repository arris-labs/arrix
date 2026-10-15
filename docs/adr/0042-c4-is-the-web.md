# ADR-0042: C4 is the web

- Status: Accepted
- Date: 2026-10-15
- Amends: `docs/CONCURRENCY-WASM.md` §The browser build (where the
  authority runs), `docs/ROADMAP.md` §C4 (detailed)

## Context

`docs/ROADMAP.md` §C4 left three things to this ADR: how the web detailed
the cycle, where the authority runs, and how a superseded evaluation is
stopped in a browser (`docs/CONCURRENCY-WASM.md` §The browser build). The
idea `browser-worker-cancellation` argued the third; on 2026-10-15 the
human chose its option A, a `SharedArrayBuffer` counter.

The design in `docs/CONCURRENCY-WASM.md` put the authority and the
evaluator together in one web worker. The app, though, reads
`Session::request` synchronously at about twenty call sites, and a browser
main thread cannot block on a worker. A synchronous read therefore needs
the authority on the main thread. The authority makes no kernel call (it
only orders commands, applies commits and keeps the document), so the rule
that the kernel never runs on the main thread (`SEED.md` §6.7) still holds.

## Decision

1. **Topology.** On the web the authority runs on the main thread and the
   evaluator alone in one dedicated web worker. The two exchange the same
   plain data the session thread and the evaluator thread exchange natively
   (`Job`, `Publish`, `Freeze`). The client still holds a replica and reads
   only the applied stream (`SEED.md` §6.1).
2. **Cancellation.** One small `SharedArrayBuffer`, apart from wasm memory,
   holds two counters.
   - **commits** is bumped by the authority after it applies a commit and
     before it posts that commit's job, and each such job carries the
     count. The worker's `Interrupt` is answered by the poll
     `commits > taken`, where `taken` is the count of the newest commit job
     the worker has taken. Nothing is ever lowered, so native's raise and
     lower ordering has no web counterpart.
   - **supersedes** is bumped for a preview, a clear and a commit, and is
     read only at feature and mesh boundaries. A preview never stops a
     commit, and a running preview stops between features for anything
     newer (ADR-0019).
   - The poll is a non-capturing function over a `thread_local!` handle
     that reads through `Atomics.load` (Arris's `Poll` is `Sync`), at every
     step, or every Nth if the probe measures one read per step as too
     dear. It is Arris's `Control::poll` (A3), nothing new.
3. **Without isolation** (`crossOriginIsolated` false, no
   `SharedArrayBuffer`): the poll answers false, a superseded evaluation
   runs to its end, a preview gives way at job boundaries only, and the
   status line says so once: "Evaluation can't be interrupted on this
   page: it is not cross-origin isolated".
4. **The page.** COOP `same-origin` and COEP `require-corp`. One wasm
   module, built by `cargo build --target wasm32-unknown-unknown` and
   `wasm-bindgen --target web`, is loaded by both the page and the worker,
   each through its own entry point. `scripts/web-serve` is for development
   and tests; a public deployment is not part of C4.
5. **Scope.** Tier 0 plugins only and no `PluginLoader` (`LoadPlugin` and
   `ReloadPlugin` are `Rejected::NoPluginLoader`, ADR-0024). A document
   using a Tier 1 or Tier 2 plugin opens with those features frozen, bodies
   kept, and the page says why. Files are bytes: the browser's file picker
   and drops in, downloads out. The page runs the shipped config defaults
   (ADR-0015).
6. **Gate.** The accept runs in headless Chromium through a native harness
   that serves the built page with the headers above and receives the
   page's report on a POST. Without Chromium it skips loudly, as the visual
   tests skip without lavapipe. The cancellation latency is gated at 50 ms.
   Bit equality with native is probed first on wasm32 under Node, since
   Arris calls the platform's libm; a difference is an Arris finding, not a
   patch here (ADR-0036).
7. **Rejected**, by the idea: threads on nightly with shared wasm memory
   (`-Zbuild-std`, the toolchain stays `stable`), Arris's step budget as
   slices, and terminating the worker (it costs the cache).
8. No document schema change, and no `arrix-plugin-api` change (API stays
   0.8.0). `arrix-doc`'s internal worker messages get a serde form; the
   public protocol and `protocol.schema.json` are unchanged.

## Consequences

- `docs/CONCURRENCY-WASM.md` §Roles and threads gets a web column when the
  cycle lands, and §The browser build says "authority on the main thread".
- `arrix-kernel`'s `Interrupt` gains an embedder-installed poll and a
  sequence form; native keeps its flag unchanged. `arrix-doc` gains
  wasm32-only `wasm-bindgen`, `js-sys` and `web-sys`, pinned to the
  `wasm-bindgen` CLI the gate installs.
- Previews keep ADR-0019's rules on the web; only an unisolated page loses
  the mid-call stop.
- A kernel call that is interrupted late is bounded by the poll's
  frequency, which step 3 of `plans/c4` measures.
