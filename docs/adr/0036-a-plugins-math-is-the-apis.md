# ADR-0036: a plugin's math is the API's, the same at every tier

- Status: Accepted
- Date: 2026-10-13
- Amends: ADR-0035 (its consequence "not yet independent of its tier")

## Context

plans/c3-m1 step 7 compared `gears` compiled in (Tier 0) with `gears`
built as a component (Tier 1, ADR-0035) on `gear-on-plane` and the
bracket. Statuses, topology counts and volumes were equal, and a document
saved at either tier opened live at the other, but the bits were not:

- `f64::sin`, `cos` and `tan` are the system's libm natively and Rust's
  own (`libm`) on wasm32. They differ in the last place: 21 of a spur's
  891 profile numbers, by at most 2 ulp (1.7e-18 m), and 24 numbers of
  the kernel calls. `sqrt` and the basic operations are exact under IEEE
  754 and were equal.
- The difference reached the eval line: `gear-on-plane`'s gear centroid y
  (9.3e-19 against 5.3e-19 m, noise about zero) and three numbers of the
  bracket's, each in the last place. The frozen body's bytes differed,
  so its blob's hash, the part file naming it, and the saved bytes.

`docs/CONCURRENCY-WASM.md` §Determinism promises byte-identical results
"on any machine, native or wasm". The same gap reaches C4, where Tier 0
plugins run inside the wasm app, and any plugin whose native result is
held to a golden.

## Decision

1. **`arrix_plugin_api::math`** re-exports the `libm` crate's `f64`
   transcendental functions (`sin`, `cos`, `tan`, `sincos`, the inverse
   and hyperbolic ones, `exp`, `log`, `pow`, `hypot`, `cbrt`, and their
   variants). It is compiled into the plugin, so a plugin's math is the
   same code at Tier 0 natively, at Tier 1 and in a wasm build. `libm` is
   pure Rust, no C, and was already in the lock file (through `wasmtime`
   and `naga`).
2. **A plugin's `evaluate` uses it, never `f64`'s methods**, for
   anything transcendental. `docs/PLUGINS.md` §1. Features states the
   rule; `plugins/gears/clippy.toml` holds `gears` to it with
   `disallowed-methods`, the shape a plugin outside the tree may copy.
   `sqrt` and the arithmetic stay `f64`'s.
3. **The promise is now "a result does not depend on its tier"**:
   `crates/arrix-plugin-host/tests/tier1_parity.rs` holds `gears` at Tier
   0 and Tier 1 to the same eval line, kernel calls and saved bytes, to
   the bit.
4. **API 0.6.0 unchanged in version.** The module is additive and leaves
   the world alone, as the `guest` feature did (ADR-0035 §1).
5. **Today's goldens move once.** `gears`' native numbers now come from
   `libm`, so every golden naming a gear was refreshed in the same commit:
   the CLI's eval, sweep, summary and slice goldens, the C1 and C2
   accepts', the bracket's mass-properties recipe, and
   `tests/docs/assembly-exchange`'s frozen gear. Each changed number is
   in the last places, or noise about an exact zero (a symmetric gear's
   centroid and products of inertia); no status, count or key changed,
   and Open CASCADE's reading of the exported assembly still matches.

## Consequences

- The core's own math (`arrix-core`, `arrix-sketch`, `arrix-doc`) is not
  covered. It runs on the host, the same at every plugin tier; whether a
  wasm app matches a native one there is C4's question, with this ADR's
  module as the candidate answer.
- A Python plugin (Tier 2) uses CPython's `math`, which is the system's
  libm; it runs natively at its only tier, so it has no tier to differ
  from.
- `libm` is slower than the system's libm on some functions; a spur's
  evaluation is dominated by the kernel, so no budget moves.

## Alternatives considered

- **A tolerance in the parity test**: no golden moves, but saved bytes
  then differ by tier, and memoisation, frozen-result hashes and replay
  would hold only within a tier.
- **The kernel serving transcendental math through the world**: a WIT
  change, a call across the boundary per `sin`, out of M1's scope
  (plans/c3-m1 *Non-goals*).
- **The system libm inside the component**: there is none to import in
  the sandbox, and importing one would be a capability.
