# ArriX

A free, pure-Rust parametric CAD on the Arris kernel: a small, solid core,
and every domain as a plugin (Rust or Python). Native + WASM.

Read in this order: `SEED.md` (the charter: what ArriX is and is not, the
plugin model, the kernel's sensor, the Arris asks), `docs/README.md` (the
docs system), the design docs as they exist, `docs/adr/`, then the rules in
`.agents/rules/*.md` (git, docs lifecycle, resources). Claude Code loads
them automatically via `.claude/rules`; any other agent reads them there.
`GLOSSARY.md` holds the project's words, one meaning each: use them as
written. The skills live in `.agents/skills/` (symlinked as
`.claude/skills`): the idea → plan → work → retire → close-cycle pipeline,
`visual-debug`, and for the human `/next`, `/handoff`, `/retro` and
`/wait-what`.

## Setup (once per clone or worktree)

- `.claude/rules` and `.claude/skills` are symlinks into `.agents/`. On
  Windows, clone with symlinks enabled (`git config --global core.symlinks
  true`, with Developer Mode on), or the skills and rules won't load.
- `cargo install cargo-nextest --locked` (the gate's test runner, ADR-0040).
- `cargo install wasm-bindgen-cli --version '=0.2.129' --locked` and Node
  (the wasm32 tests' runner; the version is the lock's `wasm-bindgen`).
- `git config core.hooksPath .githooks` installs the pre-commit hook.
  Python 3 runs the lints; `rust-toolchain.toml` brings rustfmt, clippy
  and the wasm32 target.
- The visual scenarios need Mesa's lavapipe (`mesa-vulkan-drivers` on
  Debian and Ubuntu), the browser tests Chromium or Chrome on `PATH` (or
  `ARRIX_CHROMIUM`). Without them they skip, loudly.

## Current state

**Seeded 2026-09-26.** Each closed cycle's accept is named in its
`docs/ROADMAP.md` section.
**C1 done 2026-10-08**: the vertical slice, a plugin feature in the
history (ADR-0004 to ADR-0020).
**The boundary opened done 2026-10-11**: Tier 2 Python plugins, `arrix
serve --stdio`, headless render, `arrix test` (ADR-0016, ADR-0021 to 0026).
**C2 done 2026-10-13**: assemblies, STEP with product structure in metres,
materials and mass (ADR-0027 to ADR-0033; `arris` 0.8).
**C3 done 2026-10-14**: Tier 1 components, exporters, plugin commands,
robotics and fasteners as projects of their own; the north star runs
(ADR-0034 to ADR-0039, API 0.8.0).
**C4 done 2026-10-23**: the web, the app in a browser tab with the evaluator
in a worker, cancelled within 50 ms (ADR-0042, `c4_accept`).
**Next:** no cycle is scheduled; `/close-cycle` opens the next
(`docs/ROADMAP.md` §Named cycles).

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
- The agent looks at the UI itself, headless (`visual-debug`), and reads a
  refreshed golden's diff before staging it.
- Decisions go in `docs/adr/` (append-only: supersede, never edit).
  `⚠ OPEN:` in a doc marks a deferred one. `SEED.md` changes only by an ADR.
- Update "Current state" when a cycle lands; keep it under ~15 lines.
- Backlog line → `/idea` → `/plan` → `/work` (one step, one commit) →
  `/retire-plan` (with a review of the plan's diff); `/close-cycle` at a
  cycle boundary (`.agents/rules/docs-lifecycle.md`).
- Trunk-based git, `main` always green, a commit per plan step, no
  attribution trailers, never push unasked (`.agents/rules/git.md`).
- `docs/notes/` is gitignored: private working notes. What survives there
  becomes an idea, an ADR or a design section, which tracked docs cite;
  `docs/notes/LOCAL.md` is the one notes file a tracked doc names.

## Commands

- The gate: `.githooks/pre-commit` (fast) and `scripts/gate` (the slow
  half, which `/work` runs before ticking a step), each re-executed under
  `scripts/heavy` (`.agents/rules/resources.md`); what they run is
  `docs/ARCHITECTURE.md` §Gates.
- One test: `cargo nextest run -p <crate> --test <file>`. Each cycle's
  accept test is named in its `docs/ROADMAP.md` section.
- The CLI: `cargo run -p arrix-cli -- --help`; the GUI headless: the
  `visual-debug` skill. The web app: `scripts/web-build` and
  `scripts/web-serve` (`docs/UI-RENDERING.md` §The web entry); its
  browser tests run in headless Chromium (`cargo nextest run -p arrix-app
  --test web_app --test c4_accept`).
- Which test is which milestone's accept, the Tier 1 build recipe and the
  golden writers that run off CI (`scripts/urdf-read`, `scripts/occt-read`):
  `docs/COMMANDS.md`.

## Local reference checkouts

Machine-local pointers (the Arris checkout, the source of the ported code,
the egui dev tree) are in `docs/notes/LOCAL.md`, which is private and may
not exist on another machine. They are read-only references, never `path =`
dependencies. Arris comes from crates.io. A local egui tree can be ahead of
the pinned release, so verify against
`~/.cargo/registry/src/*/<crate>-<version>/`.
