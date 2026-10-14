# ADR-0040: nextest is the gate's test runner

- Status: Accepted
- Date: 2026-10-15
- Amends: `docs/ARCHITECTURE.md` §Gates (the commands of pre-push and CI)

## Context

Two agents, ArriX's and Arris's, gate on one machine (32 CPUs, 61 GiB), and
a gate that squeezes memory gets the terminal killed by oomd
(`.agents/rules/resources.md`). The gate also has to be fast enough to run
before every plan step. `cargo test --workspace` runs one test binary after
another, so the suite does not scale: measured here, 280 s of test time at 4
threads and no better beyond. `cargo nextest run` runs the tests of many
binaries at once under one thread budget: 132 s at 4 threads (1325 tests
passed) and 96 s at 8, where `c1_benchmark` (save and load at most 100 ms)
failed under contention. Arris already uses nextest (`test-threads = 16` in
its `.config/nextest.toml`).

This plan is outside the cycle's in-list (tooling). The scenario it unlocks
is the one above: both gates run side by side, each fast, none killing a
terminal and none flaking.

## Decision

1. **`cargo nextest run` is the gate's test runner** in `.githooks/pre-push`,
   CI, `/work` and the plugin workspaces (`plugins/gears`, `plugins/robotics`,
   each a root of its own). The `--all-features` second run stays as it is.
2. **Four test threads**, set in `.config/nextest.toml` (the default profile)
   rather than `RUST_TEST_THREADS`, which nextest ignores. Eight is faster
   and flakes the timing tests; four is the measured knee.
3. **A `timing` test group with `max-threads = 1`** holds the tests that
   assert wall-clock time, so a loaded machine cannot fail them by running
   two at once. Its membership is established by a probe (plan step 2) and
   recorded under *Consequences*.
4. **Doctests stay on `cargo test --workspace --doc`**, which nextest does
   not run, beside it in the gate.
5. **`cargo test -p x --test y` stays valid** for a single test, documented
   beside the nextest form.
6. Nothing is superseded. No change to what is tested, to any test's body,
   or to `arrix-plugin-api`.

## Consequences

- The test phase is expected at about 130 s instead of 280 s; the plan's
  acceptance holds it to at most 150 s.
- Contributors and CI need `cargo-nextest` (`cargo install cargo-nextest
  --locked`); the hook fails with that line when it is missing.
- The serial group's exact membership, and any shared-directory race found
  when binaries overlap (`target/visual-scratch`, `target/out-of-tree`,
  `target/tier1-fixtures`), are written here by plan step 2.
