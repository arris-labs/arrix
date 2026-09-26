# ArriX

A free, pure-Rust parametric CAD on the Arris kernel: a small, solid core,
and every domain as a plugin (Rust or Python). Native + WASM.

Read in this order: `SEED.md` (the charter: what ArriX is and is not, the
plugin model, the kernel's sensor, the Arris asks), `docs/README.md` (the
docs system), the design docs as they exist, `docs/adr/`, then the rules in
`.agents/rules/*.md` (git, docs lifecycle). Claude Code loads them
automatically via `.claude/rules`; any other agent reads them there. The
skills for the idea → plan → work → retire → close-cycle pipeline live in
`.agents/skills/` (symlinked as `.claude/skills`).

## Setup (once per clone or worktree)

- `.claude/rules` and `.claude/skills` are symlinks into `.agents/`. On
  Windows, clone with symlinks enabled (`git config --global core.symlinks
  true`, with Developer Mode on), or the skills and rules won't load.
- `git config core.hooksPath .githooks` installs the gate's hooks.
  Python 3 runs the lints; `rust-toolchain.toml` brings rustfmt, clippy
  and the wasm32 target.
- The visual scenarios need Mesa's lavapipe (`mesa-vulkan-drivers` on
  Debian and Ubuntu). Without it they skip, loudly.

## Current state

**Seeded 2026-09-26.** `SEED.md` agreed; the docs system is scaffolded; the
design docs of `SEED.md` §10.2 are written, with C1 in full in
`docs/ROADMAP.md` (M0 gate and harness → M1 the document and a plugin
feature → M2 part features and naming → M3 the app → M4 exchange and batch).
**C1 M0 landed 2026-09-26:** the workspace, the gate (hooks, lints, CI,
self-test), the headless visual harness on an empty shell, and `arrix
eval` on an empty document. **Now:** C1 M1's first plan,
`docs/plans/c1-m1-document.md` (of three).

## Rules that are not derivable from the code

- The layer rules in `SEED.md` §6.7 are binding: Arris types only in
  `arrix-kernel`; egui/eframe/wgpu only in viewport/ui/app; `arrix-ui` never
  names a doc or sketch type; plugins depend on `arrix-plugin-api` alone;
  mutation only through `Command`s; the kernel never on the UI thread.
- The command/event boundary is a protocol even locally (`SEED.md` §6.1):
  commands, events and view types are plain serialisable data; undo is
  per author by inverse commands; a command names its base generation;
  session state (selection, camera, active sketch) stays out of the
  document. This keeps a future server-and-thin-client mode possible.
- First-party plugins use the public plugin API only, and there are no
  private hooks. A need the API can't meet is an API change, never a
  back door.
- Every kernel call goes through `arrix-kernel` as a replayable record
  (`SEED.md` §6.6). A kernel gap becomes an Arris fixture or a line in Arris's
  backlog, never a workaround or an `#[ignore]` here.
- One gesture = one command; fail soft, never silently; no silent re-bind of
  a lost reference; SI inside, Z-up, right-handed (`SEED.md` §8.2).
- The agent looks at the UI itself, headless. Never ask the human to
  describe the screen, and never accept a refreshed golden without reading
  its diff.
- Decisions go in `docs/adr/` (append-only: supersede, never edit).
  `⚠ OPEN:` in a doc marks a deferred one. `SEED.md` changes only by an ADR.
- Update "Current state" when a cycle lands; keep it under ~15 lines.
- Backlog line → `/idea` → `/plan` → `/work` (one step, one commit) →
  `/retire-plan`; `/close-cycle` at a cycle boundary
  (`.agents/rules/docs-lifecycle.md`).
- Trunk-based git, `main` always green, a commit per plan step, no
  attribution trailers, never push unasked (`.agents/rules/git.md`).
- `docs/notes/` is gitignored: private working notes. Design content is
  never cited from there; what survives becomes an idea, an ADR or a design
  section. `docs/notes/LOCAL.md` is the one file a tracked doc names.

## Commands

The gate, as `docs/ARCHITECTURE.md` §Gates defines it; `/work` runs all of
it before ticking a step:

- `.githooks/pre-commit`: `cargo fmt --all -- --check`, `cargo clippy
  --workspace --all-targets --all-features -- -D warnings`,
  `scripts/size-lint`, `scripts/wasm-lint`, `scripts/layer-lint`.
- `.githooks/pre-push`: `cargo test --workspace` (and `--all-features`),
  `cargo build --target wasm32-unknown-unknown --workspace --exclude
  arrix-cli`, `scripts/gate-selftest`.
- CI (`.github/workflows/ci.yml`) runs the same commands; `gate-selftest`
  fails if the two lists drift.
- The visual harness (`docs/UI-RENDERING.md` §Visual debugging, the
  `visual-debug` skill): `cargo test -p arrix-app --test visual` runs the
  scenarios against their text goldens; `cargo test -p arrix-app --test
  visual_scratch -- --nocapture` renders a state to
  `target/visual-scratch/`; `scripts/snapshot-baseline [<rev>]` renders
  before, after and diff images into `target/snapshots/`, read before any
  `UPDATE_SNAPSHOTS=1`.
- `cargo run -p arrix-cli -- eval <dir>` evaluates a document directory
  and prints its JSON line (`tests/docs/empty` is the M0 scenario).
- Either hook runs by hand as `.githooks/pre-commit` or
  `.githooks/pre-push`. `scripts/size-lint --list` shows offenders past
  the allowlist.

## Local reference checkouts

Machine-local pointers (the Arris checkout, the source of the ported code,
the egui dev tree) are in `docs/notes/LOCAL.md`, which is private and may
not exist on another machine. They are read-only references, never `path =`
dependencies. Arris comes from crates.io. A local egui tree can be ahead of
the pinned release, so verify against
`~/.cargo/registry/src/*/<crate>-<version>/`.
