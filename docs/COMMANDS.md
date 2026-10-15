# Commands

How to run the gate, the accepts, the harnesses and the CLI. Reached from
`AGENTS.md` §Commands; read it when you need to run one of them.
`arrix --help` and `arrix <command> --help` hold the CLI's flags and exit
codes, and `docs/ARCHITECTURE.md` §Gates defines what the gate is, so this
page lists only what neither says: which test is which accept, and the
scripts that run off CI.

Every `cargo test -p x --test y` below runs as well as `cargo nextest run
-p x --test y` (`--no-capture` for `--nocapture`).

- `.githooks/pre-commit`: `cargo fmt --all -- --check`, `cargo clippy
  --workspace --all-targets --all-features -- -D warnings`,
  `scripts/size-lint`, `scripts/wasm-lint`, `scripts/layer-lint`.
- `scripts/gate` (no hook runs it; `/work` does, ADR-0041): `cargo nextest run --workspace` (and `--all-features`),
  `cargo test --workspace --doc`,
  `cargo build --target wasm32-unknown-unknown --workspace --exclude
  arrix-cli --exclude arrix-plugin-test`, `scripts/gate-selftest`.
- `plugins/gears` and `plugins/robotics` are Cargo workspaces of their own:
  pre-commit and the gate also run clippy, `cargo nextest run`, its doctests and the `guest` wasm32 build on
  each by `--manifest-path plugins/<name>/Cargo.toml`. `cargo test -p
  arrix-cli --test out_of_tree` installs `arrix` into `target/out-of-tree`
  and runs `arrix test` from it on every plugin directory.
- CI (`.github/workflows/ci.yml`) runs the same commands; `gate-selftest`
  fails if the two lists drift or a plugin workspace is left out.
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
  `arrix test <plugin-dir> [--schema]` runs the test kit (the plugin's
  `[test]` table, one JSON line per check; the checks are the crate
  `arrix-plugin-test`'s, which a Rust plugin's `cargo test` runs too:
  `plugins/gears/tests/kit.rs`); `eval --schema` and
  `arrix-app render --schema` print the other surfaces' schemas.
- `cargo test -p arrix-cli --test c3_accept` is C3's accept: `arrix serve`
  with `plugins/fasteners` (Python, Tier 2) and `robotics` (Tier 1) opens
  `tests/docs/north-star`, `session.expand` runs `fasteners.place` on the
  patterned holes' edges (then `submit`), the URDF is held to
  `--test c3_accept_urdf`'s golden, and the saved document opened with
  neither plugin keeps its bodies, frozen.
- A Tier 1 plugin is a Rust crate on `arrix-plugin-api` alone with its
  `guest` feature, a `cdylib` built `cargo build --release --target
  wasm32-unknown-unknown --features guest`; its directory holds an
  `arrix-plugin.toml` (`tier = 1`, a `[tier1]` table naming the `.wasm`) and
  the module. `--plugin <dir>` and `session.load_plugin` load it by the
  tier the manifest names, and a directory takes the place of a compiled-in
  plugin of its id (`gears` at Tier 1 for the Tier 0 one).
  `cargo test -p arrix-cli --test c3_m1` is C3 M1's accept, `--test
  tier1_surfaces` the CLI's surfaces; the host's tests build the fixtures
  in `target/tier1-fixtures`.
- `arrix export <doc> --exporter <id> [--assembly <a> | --part <p>]
  [--option <name>=<expr>]… -o <path> [--force]` runs an exporter (a plugin's
  or `core.step|stl|obj`): one file to `<path>`, several into that directory.
  `cargo test -p arrix-cli --test c3_m2` is C3 M2's accept (`plugins/robotics`,
  `tests/docs/arm`, `python/examples/bom`); `scripts/urdf-read <urdf>…`
  writes the goldens of `--test c3_m2_urdf` and `--test c3_accept_urdf`
  (the north star with screws placed) on a machine with urdfdom and `uv`
  (never CI; its header says how).
- `cargo test -p arrix-cli --test c2_accept` is C2's accept: `tests/docs/
  assembly-exchange` exported with `arrix export --assembly`, re-imported,
  and ArriX's reading of it and of the NIST vendor part held to Open
  CASCADE's, frozen in `tests/golden/c2-accept.occt.txt`. `scripts/
  occt-read <step>…` writes that golden on a machine with `cadquery-ocp`
  (never CI; its header says how).
- `arrix eval` also takes several paths and directories, `--sweep
  <target>=<from>..<to>:<n>`, `--summary`, `--timings`, `--jobs <n>` and
  `--budget <steps>` (`docs/CONCURRENCY-WASM.md` §Batch evaluation).
  `cargo run -p arrix-cli -- export <doc> --format stl|obj [--body
  <feature>.<slot>]… -o <file>` writes the final bodies, in metres;
  `step` is written declaring metres (`write_with`, Arris ask A12).
- `cargo run -p arrix-cli -- import <step> -o <dir>` reads a STEP file's
  product tree into a new document directory (parts, assemblies,
  components at the file's placements, colours as materials;
  `tests/fixtures/step/arm.step`, `cargo test -p arrix-cli --test import`;
  the app's `Import…` is the same through the session, and
  `cargo test -p arrix-app --test visual import_structure` is its scenario).
- `cargo run -p arrix-cli -- describe [--json]` lists everything there is to
  configure; `config check [--dir]` validates the files (exit 1 on any
  diagnostic); `config schema [kind]` prints their JSON Schemas; `config
  catalog` prints `docs/CONFIG-CATALOG.md`, regenerated by
  `UPDATE_SNAPSHOTS=1 cargo test -p arrix-cli catalog`.
  `cargo test -p arrix-cli --test config` and `cargo test -p arrix-app
  --test visual configured_shell` are the configurable shell's accept.
- `arrix serve --stdio [--plugin <dir>]… [<doc>]` serves a session as JSON-RPC
  lines (`--schema` prints the protocol's schemas, held fresh by `cargo test
  -p arrix-cli --test serve_schema`; `--test serve` drives the binary);
  `cargo test -p arrix-cli --test boundary_m2` is M2's accept. The SDK's
  `arrix.session` drives `serve`, `arrix.eval` runs `eval`, and
  `cargo test -p arrix-cli --test boundary_m3` is M3's accept.
  `cargo test -p arrix-cli --test c2_m1` is C2 M1's (`tests/docs/assembly`,
  written with `UPDATE_SNAPSHOTS=1`).
  `cargo test -p arrix-app --test visual c2_m2_accept` is C2 M2's (the app
  builds an assembly on `tests/docs/assembly`, minus its assembly).
  `cargo run -p arrix-app -- render <doc> -o <png> [--json <file>] [--view …]
  [--context <part|assembly>] [--size WxH] [--plugin <dir>]…` renders a
  document headless (needs lavapipe).
- The two halves run by hand as `.githooks/pre-commit` and
  `scripts/gate`; both re-exec under `scripts/heavy`, which runs any
  heavy command in a memory-limited scope so an oomd kill spares the
  terminal (`.agents/rules/resources.md`). `scripts/size-lint --list` shows offenders past
  the allowlist.

