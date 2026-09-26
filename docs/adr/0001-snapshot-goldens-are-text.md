# ADR-0001: Snapshot goldens are text; images are rendered on demand

- Status: Accepted
- Date: 2026-09-26

## Context

The headless visual harness is ported (`SEED.md` §8.1). As it exists, it
commits a full-resolution PNG beside each scenario's `debug_state()` JSON.
A 1440×900 frame of a CAD UI is roughly 50–150 KB as PNG. PNG's deflate
stream changes throughout on a small visual change, so git's deltas save
little, and every golden refresh adds close to the full size to history.
Thirty scenarios refreshed a few hundred times over the project's life is
hundreds of megabytes in `.git`, for data that is derived: the same commit
and the same software renderer (lavapipe) produce the same image.

A snapshot test still needs a stored reference. It runs in every pre-push
and CI run and has to compare against something without building a second
revision. And refreshing a golden in a commit is the recorded approval of
an intended look.

## Decision

A scenario's golden is one text file: its `debug_state()` JSON and a
**coarse frame**. The coarse frame is the rendered frame reduced to 16×16
pixel cells of mean RGB, written as hex rows (about 30 KB at 1440×900),
compared per cell with a tolerance and a budget of cells allowed to differ.
No image, and no other binary golden, is committed. Full frames are written
to `target/` on every run. `scripts/snapshot-baseline <rev>` renders the
same scenarios at a revision in a temporary worktree, so a before, after
and diff image exist whenever someone needs to look. CI uploads them as
artifacts when a scenario fails.

## Consequences

- History grows by a few KB per golden refresh instead of close to the
  full PNG. Most of the cost is the text, which deltas well because a
  local visual change touches only the hex rows over it.
- `git diff` of a refreshed golden shows which rows of the frame changed,
  next to what `debug_state()` says changed.
- Visual changes smaller than a cell (a 1-px outline, a shifted glyph) are
  not caught by the gate. Text is caught by `debug_state()`. If sub-cell
  regressions prove to matter, a scenario gets a finer grid, possibly over
  a crop such as the viewport, before images come back.
- Looking at a failure costs a build and a render of the baseline revision.
  That cost is paid only when someone looks, not on every run.
- The per-cell tolerance also absorbs Mesa-version noise that exact PNG
  comparison needed a pixel threshold for.

## Alternatives considered

- **Commit PNGs** (the ported harness): every pixel is checked, but history
  grows by roughly the full image on each refresh.
- **Git LFS**: keeps `.git` lean, but needs LFS hosting and quota, and a
  clone without LFS has no goldens.
- **A separate snapshots repository or orphan branch**: keeps main's
  history clean, but a golden refresh becomes a change spread across two
  places kept in sync by hand.
- **`debug_state()` JSON only**: smallest, but a purely visual regression
  (a shader, the outline pass, colours, overlapping widgets) passes the
  gate.
- **No stored reference; render the parent commit every run**: builds two
  revisions on every pre-push and CI run, and loses the refresh commit as
  the explicit approval of a look.
- **The coarse frame as a small PNG**: a smaller file per refresh, but it
  deltas poorly and its diff is not readable in review.
