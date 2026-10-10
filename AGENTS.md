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

**Seeded 2026-09-26.** `SEED.md` agreed; the docs system scaffolded; the
design docs of `SEED.md` §10.2 written.
**C1 done 2026-10-08** (M0 to M4: the gate and harness, the document and a
plugin feature in its history, part features and naming, the app, export and
batch; `arris` 0.5, 0.6 after; ADR-0004 to ADR-0020): `cargo test -p arrix-cli --test
c1_accept` is its accept, `--test c1_benchmark -- --nocapture` its budgets
(the first coarse mesh misses, an Arris finding), `cargo test -p arrix-app
--test visual m3_accept` the app's.
**Open 2026-10-09: the boundary opened** (ADR-0016, ADR-0021, ADR-0022), M1 to
M3 in `docs/ROADMAP.md`; **M1 done 2026-10-09**: a Python feature plugin runs
as Tier 2 over a JSON-RPC wire, frozen without it (`--test boundary_m1`).

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
  or an `.arrx` and prints its JSON line (`--without-plugin <id>`,
  repeatable, leaves a plugin out; `tests/docs/empty` is the M0 scenario,
  `tests/docs/gear-on-plane` M1's first, `tests/docs/slice` its accept's
  document, each with a golden in `crates/arrix-cli/tests/golden/`).
  `cargo test -p arrix-cli --test slice` is M1's accept, frozen goldens
  included. `cargo test -p arrix-cli --test naming_scenarios` is M2's
  (`tests/docs/naming`, written with `UPDATE_SNAPSHOTS=1`), with
  `--test save_fixtures` for the oversize fillet's recipe.
  `cargo test -p arrix-cli --test c1_accept` is C1's (the bracket,
  `tests/docs/bracket`), and `--test c1_benchmark -- --nocapture` prints
  the budgets of `docs/CONCURRENCY-WASM.md` §Budgets.
- `arrix eval` and `describe` take `--plugin <dir>` (repeatable), a plugin
  directory with an `arrix-plugin.toml`; the Python SDK is `python/arrix`
  (`python/examples/standoff`, `tests/docs/standoff`), its `python -m
  unittest` run by `cargo test -p arrix-plugin-host --test python_sdk`, and
  `cargo test -p arrix-cli --test boundary_m1` is M1's accept.
- `arrix eval` also takes several paths and directories, `--sweep
  <target>=<from>..<to>:<n>`, `--summary`, `--timings`, `--jobs <n>` and
  `--budget <steps>` (`docs/CONCURRENCY-WASM.md` §Batch evaluation).
  `cargo run -p arrix-cli -- export <doc> --format step|stl|obj [--body
  <feature>.<slot>]… -o <file>` writes the final bodies, in metres.
- `cargo run -p arrix-cli -- describe [--json]` lists everything there is to
  configure; `config check [--dir]` validates the files (exit 1 on any
  diagnostic); `config schema [kind]` prints their JSON Schemas; `config
  catalog` prints `docs/CONFIG-CATALOG.md`, regenerated by
  `UPDATE_SNAPSHOTS=1 cargo test -p arrix-cli catalog`.
  `cargo test -p arrix-cli --test config` and `cargo test -p arrix-app
  --test visual configured_shell` are the configurable shell's accept.
- `arrix serve --stdio [<doc>]` serves a session as JSON-RPC lines
  (`--schema` prints the protocol's schemas; `cargo test -p arrix-cli
  --test serve` drives the binary).
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
