# ADR-0041: the gate runs before a step, not before a push

- Status: Accepted
- Date: 2026-10-15
- Amends: `docs/ARCHITECTURE.md` §Gates (where the slow half runs),
  ADR-0040 §Decision 1 (nextest's place in `.githooks/pre-push`)

## Context

The slow half of the gate (the workspace's tests twice, the doctests, the
wasm32 builds, each plugin workspace, `gate-selftest`) ran three times for
one change: `/work` runs it before a step's box is ticked
(`.agents/rules/git.md`), the pre-push hook ran it again on the human's
push, and CI runs it a third time, command for command. The second run cost
minutes and about 25 GiB of peak memory per push, and on 2026-10-15 it
cost two pushes: a push of tags and a push of `main` started together ran
two gates in one target directory, whose fixed scratch directories raced
(`arrix-cli::arm`'s URDF golden failed), and the push of `main` sent
nothing after a gate that passed, the SSH connection opened before the
hook having gone idle.

Arris, beside it on the same machine, has no pre-push hook: a pre-commit
sized to what is staged, and CI and the plan's retirement for depth.

This is tooling outside the cycle's in-list. The scenario it unlocks: a
push is a push, seconds long, and the gate's depth is where the work is.

## Decision

1. **`.githooks/pre-push` is removed.** Its commands move unchanged to
   `scripts/gate`, which re-execs under `scripts/heavy` as the hook did.
2. **`/work` runs `scripts/gate`** before it ticks a step, as it already
   promised to run the whole gate. Pre-commit stays: fmt, clippy and the
   lints, seconds when warm.
3. **CI is the backstop** for a push that skipped the gate. Its `gate: …`
   steps run exactly the commands of `.githooks/pre-commit` and
   `scripts/gate`, which `gate-selftest` checks as it checked the hooks.
4. **One gate at a time per target directory**: `scripts/gate` holds a
   `flock` on `target/.gate.lock`, so a second run waits instead of racing
   the tests' fixed scratch directories or doubling the memory.
5. Rejected: keeping the hook and fixing each scratch directory (the race
   was one symptom; the cost was the third run); Arris's staged-path sizing
   of the gate (its ADR-0032), a larger change worth taking when a step's
   gate gets slow.

## Consequences

- A push no longer runs tests. A commit made outside `/work` (a fix, a
  docs change) has run pre-commit only, so the person or agent making it
  runs `scripts/gate` when it can change what the suite tests; CI catches
  the rest, after the push.
- `main` on GitHub can be red for the length of a CI run when that is
  skipped; it is fixed forward (`.agents/rules/git.md`).
- `git config core.hooksPath .githooks` still installs pre-commit.
