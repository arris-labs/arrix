---
name: retire-plan
description: Closes a completed plan. Verifies every step is ticked and the acceptance test passes, executes its docs-to-update list, drift-checks the design docs it touched, updates the roadmap status line and AGENTS.md, deletes the plan file, commits, and cleans the build cache. Use when the human says "retire", "close the plan", "plan is done", or confirms after the last step's acceptance run.
argument-hint: <plan slug>
---

# /retire-plan — move the durable parts, delete the rest

Deletion is the "done" signal; git keeps the plan. Anything worth keeping
moves first.

1. Open `docs/plans/<slug>.md`. Every step ticked by its own commit, and the
   acceptance green when run now? If not, stop and report which.
2. Execute *Docs to update* line by line. Design docs describe the system
   as it now is, in present tense, without the plan's history. An
   `⚠ OPEN:` closed by a decision gets its ADR now, listed in
   `docs/adr/README.md`.
3. **Drift check** every design doc the plan touched against the code; fix
   each sentence no longer true and list the fixes in the reply.
4. Update the cycle's status line in `docs/ROADMAP.md`. If this plan
   completes the cycle, say so and suggest `/close-cycle`.
5. `AGENTS.md` "Current state": one cycle-level sentence, block under ~15
   lines.
6. Deferred work goes to `docs/BACKLOG.md`, one line naming the plan and
   step that deferred it. Confirm each Arris ask the plan raised exists in
   Arris's backlog or ideas.
7. `git rm docs/plans/<slug>.md`; commit everything as
   `docs: retire plan <slug>`, the body listing the docs updated.
8. **Clean the build cache**: a plan's rebuilds leave tens of gigabytes of
   dead artifacts. Note `du -sh target` before and after. With
   `cargo sweep`: `cargo sweep -s`, rebuild what the gate builds without
   running it (`cargo nextest run --workspace --no-run`, again with
   `--all-features`, `cargo clippy --workspace --all-targets
   --all-features`, the wasm build), `cargo sweep -f`, `rm
   sweep.timestamp`, and `rm -rf target/release` if the gate built no
   release. Without it, `cargo clean`.

`$ARGUMENTS`
