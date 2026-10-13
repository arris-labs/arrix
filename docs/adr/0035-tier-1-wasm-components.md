# ADR-0035: Tier 1, plugins as WebAssembly components

- Status: Accepted
- Date: 2026-10-13

## Context

ADR-0034 puts Tier 1 first in C3. `docs/PLUGINS.md` §Three tiers sketches
it: a `wasmtime` host, a store per component, fuel as the step budget, an
epoch interrupt for cancellation. The open choices were where the guest
bindings' `unsafe` lives under the workspace's `forbid(unsafe_code)`, which
target an author builds for, and how the evaluator's interrupt reaches a
running component. plans/c3-m1 step 2 probed `gears` as a component in
wasmtime (`crates/arrix-plugin-host/tests/probe_tier1.rs`) and found:

- `wasm32-unknown-unknown`, already in the toolchain, componentised at load
  by `wit-component` (12 ms for a 126 KB module), gives a component whose
  imports are `arrix:plugin/types` and `arrix:plugin/kernel` alone.
- No `unsafe` exception is needed. An SDK crate that calls `generate!` and
  a plugin crate that expands the SDK's export macro both build under
  `forbid(unsafe_code)`: the glue is a proc macro's output, and rustc does
  not report `unsafe_code` inside another crate's macro. The glue expanded
  from the plugin's own `export!` does fail, which is why the macro lives
  in the SDK.
- A guest rebuild is 0.17 s warm, `Component::new` 231 ms, an instance
  0.1 ms, a spur's `evaluate` 0.11 ms and 435 483 fuel.
- `wasmtime` (`runtime`, `cranelift`, `component-model`) adds 13.4 MB and
  `wit-component` 1.9 MB to a stripped release build, and about 62 s to a
  cold build.
- The component computes involute points up to 2 ulp (1.7e-18 m) away
  from the same code compiled natively: `sin` and `cos` are the system's
  libm there and Rust's own on wasm32. Whether that reaches the eval line
  is plans/c3-m1 step 7's to measure.

## Decision

1. **The guest SDK is a `guest` feature of `arrix-plugin-api`**, API 0.6.0
   unchanged in version (the world is unchanged, and the WIT package's
   version is the crate's; plans/c3-m1 step 4). It holds the `generate!` bindings, the
   conversions between the world's types and the API's (moved out of
   `tests/wit_equality.rs`, which now uses them), the Rust `Kernel` over the
   world's `kernel` import, and `arrix_plugin_api::export_plugin!(T)` for a
   unit struct `T: Feature`. A plugin depends on `arrix-plugin-api` alone,
   keeps `forbid(unsafe_code)`, and has one export line under a `guest`
   feature of its own, with `crate-type = ["rlib", "cdylib"]`.
2. **`unsafe` stays out of the tree.** `arrix-plugin-api` keeps the
   workspace's `forbid`; the export glue expands in the plugin's crate from
   the SDK's macro (the probe's shape), and no allowlist or `allow` is
   written. The backlog line this absorbs is removed.
3. **An author builds for `wasm32-unknown-unknown`** with `cargo build
   --target wasm32-unknown-unknown`, and the host componentises the core
   module at load with `wit-component`, so no other tool is needed. A
   module that already is a component loads as it is. `wasm32-wasip2` is
   not supported: its `std` imports WASI, which the sandbox refuses.
4. **The sandbox.** One `Engine` per host (component model, fuel, NaN
   canonicalisation, epoch interruption) and one `InstancePre` per
   plugin, from which each evaluation makes a **fresh store**, so no state
   survives between evaluations, and a result does not depend on which
   evaluations ran before it. The linker holds the world's imports and
   nothing else: a component that imports anything more (a WASI interface,
   any other package) is refused at load as `plugin.load` naming the
   import, and the plugin registers no types.
5. **Failures end the feature, not the document**, each a diagnostic on
   that feature:
   - `plugin.fuel`: the store's fuel ran out. The default is
     2^30 (about 10^9, some 2 400 spurs); the manifest's `[tier1] fuel`
     may raise it to at most 2^36, a larger value being refused at load.
     Fuel is deterministic, so the same inputs run out at the same place
     every time.
   - `plugin.memory`: linear memory past 256 MiB per store (not
     configurable), or a table past 100 000 elements.
   - `plugin.trap`: any other trap, a guest panic included (a panic is an
     `unreachable` trap in a wasm guest).
   - `plugin.capability`: a kernel call when the manifest says `kernel =
     false`, refused by the host as at Tier 2.
6. **Handles.** The world's `body` resource is, on the host, a table of
   that evaluation's handles into the same `&mut dyn Kernel` a Tier 0
   plugin gets; a component's kernel calls are recorded like any other
   (`SEED.md` §6.6). The table dies with the store.
7. **Cancellation is an epoch deadline callback.** The engine's epoch is
   ticked every 5 ms by one thread, started with the first Tier 1 plugin;
   a store's callback checks the evaluator's interrupt flag (the
   `Arc<AtomicBool>` of `docs/CONCURRENCY-WASM.md` §Cancellation, passed
   host-side in `FeatureArgs`, never through the API) and traps if it is
   raised, else continues for another tick. The trap ends the feature as
   `kernel.interrupted`, as a stopped kernel call does. A wall clock
   decides only when the flag is looked at, never a result; fuel is the
   only budget in a result.
8. **Native only.** `tier1` is gated out of wasm32 builds. There a Tier 1
   plugin cannot load, and its features open frozen with that reason,
   like Tier 2's (`docs/PLUGINS.md` §Frozen fallback).
9. **Always built on native.** Tier 1 has no cargo feature, since the
   build and binary cost is bounded (`arrix` 12.9 → about 28 MB, `arrix-app`
   33.8 → about 49 MB) and a second code path would need its own tests.
   Decided by the human, 2026-10-13.
10. **A loaded component is not cached across runs** (wasmtime's
    serialised modules) until a load budget asks for one.

## Consequences

- `arrix-plugin-host` gains native dependencies `wasmtime` and
  `wit-component`; the layer-lint edges table follows.
- A Rust plugin works at Tier 0 or Tier 1 from the same source; the
  manifest's tier and its `[tier1]` table say which.
- The determinism promise is not yet "a result is independent of its
  tier": the last-place difference above is plans/c3-m1 step 7's open
  question, decided there with its numbers.

## Alternatives considered

- **`wasm32-wasip2`**: a component straight from rustc, but every `std`
  plugin imports WASI, to be refused; the host-side componentise costs
  12 ms.
- **An `unsafe` exception in `arrix-plugin-api`** (`deny` plus an
  `allow` on a `guest` module): unneeded, per the probe.
- **Fuel slices for cancellation**: a synchronous call cannot resume after
  running out of fuel, so slices need the async API; the epoch callback
  needs none.
- **Ticking the epoch where the flag is raised**: couples the evaluator to
  the Tier 1 host; a 5 ms ticker keeps the flag the only coupling.
