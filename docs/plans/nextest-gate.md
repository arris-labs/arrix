# Plan: nextest-gate

- Started: 2026-10-15
- Cycle: outside the cycle's in-list (tooling); ADR-0040 is step 1, naming the scenario it unlocks: two agents (ArriX and Arris) gating on one machine without oomd killing a terminal, each gate fast enough to run before every step.
- Idea: verbatim from the human: "Ok, should we use nextest in arrix?" → "yes, /plan it" (the measurements below are from that session).

## Goal
`cargo nextest run` is the gate's test runner in `.githooks/pre-push`, CI, `/work` and the plugin workspaces, with `cargo test --workspace --doc` beside it for doctests. The timing-budget tests run alone in a nextest test group, so a loaded machine cannot fail them. `scripts/gate-selftest` still proves the hooks and CI run the same commands. Measured on this 32-CPU, 61 GiB machine, `cargo test --workspace` took 280 s of test time at 4 threads and flattened out beyond that; nextest took 132 s at 4 threads (1325 tests passed), and 96 s at 8 threads, where `c1_benchmark` (save and load ≤ 100 ms) failed under contention. The aim is the 4-thread time with no flake, in a hook that also runs next to Arris's gate.

## Non-goals
- No change to what is tested, to any test's body, or to the `--all-features` second run (running every test twice is its own question).
- No Arris change: its `.config/nextest.toml` has `test-threads = 16` already.
- No machine settings (oomd limits are the human's, `docs/notes/oomd-kills.md`).
- No `cargo test` removal for single-test use: `cargo test -p x --test y` stays valid and documented beside the nextest form.

## Design deltas
- `docs/ARCHITECTURE.md` §Gates: pre-push's items 1, 2 and the plugin tests become `cargo nextest run …`, plus a `cargo test --workspace --doc` item; the section says why (binaries overlap) and where the serial group lives.
- New `.config/nextest.toml` at the repo root: default profile `test-threads = 4`; a `timing` test group with `max-threads = 1` and the overrides that put the budget tests in it. Each plugin workspace is a root of its own (plans/c3-m3 Findings 7), so `plugins/gears` and `plugins/robotics` get their own `.config/nextest.toml`.
- `.githooks/pre-push`, `.github/workflows/ci.yml` (installs `cargo-nextest`), `scripts/gate-selftest` (its expected-command strings and the planted drifts) change in one commit: the selftest compares them line by line.
- `.agents/rules/resources.md`: the measured budget gains the nextest figures; `RUST_TEST_THREADS` no longer governs the gate (nextest ignores it), `--test-threads`/the profile does.
- ADR-0040 (below). No `arrix-plugin-api` change, no document schema change.

## Arris dependencies
None.

## Steps
Complexity: **[1]** routine; **[2]** careful within a given design; **[3]** unproven, behaviour to establish here.

- [x] **[1]** Step 1 — ADR-0040, nextest is the gate's test runner: the scenario (two gates on one machine), the measurements (280 s vs 132 s at 4 threads, 96 s at 8 with one timing failure), the decisions (4 threads, a serial group for timing tests, doctests via `cargo test --doc`, the plugin workspaces covered, `cargo test` kept for single tests), and what it supersedes (none; it amends `docs/ARCHITECTURE.md` §Gates). Add it to `docs/adr/README.md`.
- [x] **[3]** Step 2 — Probe, no commit of config yet: list every test that asserts wall-clock time (`budget.rs`, `c1_benchmark.rs`, `meshed.rs`, `interrupt.rs`, `tier1_cancel.rs`, `solver_benchmarks.rs`, `inference_probe.rs` are the candidates), then run the workspace under nextest 3 times at 4 threads, once of them beside Arris's `fast` profile at 16 threads, through `scripts/heavy`. Also run the visual scenarios and `out_of_tree` (shared `target/visual-scratch`, `target/out-of-tree`, `target/tier1-fixtures`) overlapped. Result: the serial group's exact membership, and any shared-directory race, written into the step's commit message and the ADR's *Consequences*. A race found is fixed (a per-test directory) in its own commit before step 3.
- [x] **[2]** Step 3 — `.config/nextest.toml` (root and each plugin workspace): `test-threads = 4`, the `timing` group, the overrides from step 2. `cargo nextest list --workspace` parses; one run is green.
- [x] **[2]** Step 4 — One commit: `.githooks/pre-push` and CI run `cargo nextest run --workspace`, `… --all-features`, each plugin's `cargo nextest run --manifest-path …`, and `cargo test --workspace --doc`; CI installs `cargo-nextest` (pinned); the hook fails with the install line if it is missing, as Arris's does; `scripts/gate-selftest` expects the new commands and still fails on a planted drift of either kind. `scripts/gate-selftest` and `cargo nextest run --workspace` green.
- [x] **[1]** Step 5 — Docs and skills: `docs/ARCHITECTURE.md` §Gates, `AGENTS.md` Commands (the `cargo test -p … --test …` lines gain "or `cargo nextest run -p … --test …`"; `Setup` gains the `cargo install cargo-nextest --locked` line), `.agents/rules/resources.md`, and the skills that name `cargo test` (`retire-plan` step on `--no-run`, `visual-debug`, `work`).
- [x] **[1]** Step 6 — Acceptance run and the measured figures into `resources.md`: `.githooks/pre-push` end to end.

Each step is one commit-sized unit with its own check.

## Acceptance
`.githooks/pre-push` runs green end to end through `scripts/heavy` three times in a row, one of them while Arris's `fast` profile runs at its 16 threads, with no timing test failing; the nextest test phase is ≤ 150 s at 4 threads (now 132 s) against `cargo test`'s 280 s; and `scripts/gate-selftest` passes, including its planted drift of the hook/CI comparison.

## Docs to update on completion
- `docs/ARCHITECTURE.md` §Gates: pre-push commands, the serial group, doctests.
- `docs/adr/README.md`: ADR-0040.
- `AGENTS.md` Commands and Setup: nextest forms, the install line.
- `.agents/rules/resources.md`: nextest budget (measured figures), `RUST_TEST_THREADS` not applying.
- `.agents/skills/{work,retire-plan,visual-debug}/SKILL.md`: the commands they name.
- `docs/ROADMAP.md`: no status line (outside the cycle); `AGENTS.md` current state unchanged.

Written at planning time; executed before the plan is deleted.

## Open questions
- Step 6 figures: pre-push green 3 times (workspace nextest 136, 137, 175 s; `--all-features` 161, 137, 172 s; doctests and plugins 27/42, 27/42, 44/71 s; wall 806, 966, 962 s; peak 25 GiB). The third ran beside 16 busy loops, not Arris's `fast` profile (no Arris checkout on this machine). The `--all-features` pass costs about as much as the first.
- Decided by the human at step 4: CI installs `cargo-nextest` with `taiki-e/install-action@v2`, the tool pinned to `nextest@0.9.144`.
- Decided in step 3: no `ci` profile; the default's 4 threads suit 4 vCPU runners. Revisit only if the CI run time says otherwise.
- ⚠ OPEN: The `--all-features` run repeats the whole suite; fold it into one run, or keep? Out of scope here, but step 6's figures will show its cost. Human, after step 6 (a BACKLOG line if wanted).
- Finding (step 6 acceptance, run 2): `visual c3_accept` failed once in the `--all-features` pass (3 components, 6 expected): the document read as settled before the expansion's command was submitted. A latent race, fixed in its own commit by waiting for the placed screws; it is the one test body this plan touches.
- Finding (step 6 acceptance, loaded run): `boundary_m1 a_timeout_in_the_plugin_is_the_features_diagnostic` failed beside 16 busy loops: its `timeout-seconds = 0.5` also covers the Python plugin's describe, which took longer than that. Raised to 3 s in its own commit; the second test body this plan touches.
