---
name: retire-plan
description: Close a completed plan: verify, review its diff against the plan, update the docs, delete the plan, clean the build cache.
disable-model-invocation: true
argument-hint: <plan slug>
---

# /retire-plan — move the durable parts, delete the rest

Deletion is the "done" signal; git keeps the plan. Anything worth keeping
moves first.

1. Open `docs/plans/<slug>.md`. Every step ticked by its own commit, and the
   acceptance green when run now? If not, stop and report which.
2. **Review the diff against the plan**, in a subagent so it reads with
   fresh eyes. Give it the plan file, `.agents/review.md` (its brief and
   standards), `GLOSSARY.md`, and the plan's commits: `git log
   --format='%h %s' --grep='plans/<slug>'` (another plan's commits may sit
   between them, so it reads each with `git show`, not one range diff).
   Report its findings by axis, unmerged. Fix a hard finding in its own
   commit (`fix(scope): … (plans/<slug> review)`) before going on; a
   judgement finding the human decides, and one left for later is a
   backlog line.
3. Execute *Docs to update* line by line. Design docs describe the system
   as it now is, in present tense, without the plan's history. An
   `⚠ OPEN:` closed by a decision gets its ADR now, listed in
   `docs/adr/README.md`.
4. **Drift check** every design doc the plan touched against the code; fix
   each sentence no longer true and list the fixes in the reply.
5. Update the cycle's status line in `docs/ROADMAP.md`. If this plan
   completes the cycle, say so and suggest `/close-cycle`.
6. `AGENTS.md` "Current state": one cycle-level sentence, block under ~15
   lines.
7. Deferred work goes to `docs/BACKLOG.md`, one line naming the plan and
   step that deferred it. Confirm each Arris ask the plan raised exists in
   Arris's backlog or ideas.
8. `git rm docs/plans/<slug>.md`; commit everything as
   `docs: retire plan <slug>`, the body listing the docs updated.
9. **Clean the build cache**: a plan's rebuilds leave tens of gigabytes of
   dead artifacts. Note `du -sh target` before and after. With
   `cargo sweep`: `cargo sweep -s`, rebuild what the gate builds without
   running it (`cargo nextest run --workspace --no-run`, again with
   `--all-features`, `cargo clippy --workspace --all-targets
   --all-features`, the wasm build), `cargo sweep -f`, `rm
   sweep.timestamp`, and `rm -rf target/release` if the gate built no
   release. Without it, `cargo clean`.
10. End with the next step: `/retro <slug>` if the plan's sessions had
    friction worth mining (long stalls, repeated corrections, the human
    asking where things stand), and the plan or cycle that comes next.

`$ARGUMENTS`
