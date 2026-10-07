# Plan: configurable-shell

- Started: 2026-10-07
- Cycle: C1 M3, its first plan; the configuration half is outside M3's
  in-list and is justified by ADR-0015 (step 1)
- Idea: docs/ideas/configurable-shell.md (absorbed), and
  docs/ideas/agentic-users.md (absorbed: the strategic direction ADR-0016
  records, and the roadmap decisions step 1 writes down)

## Goal

The shell is data-driven, and the data is text. Every user action is a
`CommandSpec` with a stable id in one registry, which the ribbon, the
command palette and the keymap all read. That includes one creation
command per registered feature type, the gears plugin's included. A user,
or their agent, can do four things by editing TOML files in their config
directory: rebind a command, switch theme, hide a ribbon tab, and set a
feature type's parameter default. `arrix config check` validates those
files with diagnostics that name the file and line. `arrix describe
--json` lists everything there is to configure, and the app applies an
edit on `core.config.reload` without restarting. A broken file never stops
the app: the bad key falls back to its default and shows as a diagnostic.
Two ADRs record the decision: ADR-0015 for the configurable shell and its
formats, and ADR-0016 for the charter direction this plan is the first
application of: ArriX invests in what users' agents can't rebuild and
makes everything above it replaceable by them.

## Non-goals

- A settings UI. The file is the UI, and the app never writes config
  (writing would need `toml_edit` to keep comments).
- A project or document config layer, and the trust model it needs.
  Profiles. Plugin packages and installing plugins.
- Runtime-loaded plugin tiers (Tier 1, Tier 2). Whether to bring them
  forward is raised at C1's `/close-cycle` (the idea's decision 4).
- A plugin's own settings for commands, importers or exporters. Those
  contribution points don't exist yet; this goes to the backlog.
- Viewport colour roles, mouse and camera bindings, and dockable layout.
  They join with the viewport port, M3's next plan.
- Config storage in the browser. The `ConfigSource` seam exists; a store
  behind it comes in C4.
- Opening the config file in an external editor; i18n.

## Design deltas

- **ADR-0015, the configurable shell.** Scenario: a user, or their agent,
  rebinds a command, switches theme, hides a ribbon tab and sets a plugin
  feature's default by editing text files that `arrix config check`
  validates, and sees the change without restarting. Decisions:
  - Humans and agents write TOML; the machine writes JSON (the document,
    `debug_state()`, `arrix eval`, `describe`).
  - Two layers: built-in defaults, then the user's directory.
  - Config is client state and never enters the document (`SEED.md` §6.1
    rule 4). It never reaches a feature's `evaluate` either: a parameter
    default only seeds a new feature's form, and once committed, the value
    is the document's (`SEED.md` §6.3).
- **ADR-0016, the agentic user.** ArriX invests in what agents can't
  cheaply rebuild: the kernel and naming, a document that outlives its
  plugins, determinism and the kernel record, the solver, the protocol,
  the headless harness. Everything above that is replaceable by users'
  agents, under five constraints that bind every later cycle:
  1. Stable ids are the API.
  2. Text in, diagnostics out, failing soft.
  3. The app describes itself (`describe`, schemas, generated catalogs
     held fresh by the gate).
  4. A short extension loop: edit, reload, see, with no rebuild of ArriX.
  5. The harness is product: the headless render, `debug_state()`, the
     eval JSON and the test kit are a public interface, with a stability
     promise from the runtime-tier cycle on.

  It amends `SEED.md` §1, §5 and §8.2 (rule 9), and records the roadmap
  decisions taken with it:
  - The cycle after C1 is "the boundary opened": the Tier 2 host, `arrix
    serve --stdio` (the `Session` protocol on stdio, the agent's way in),
    the Python SDK and the test kit. C2 follows it, and the north-star
    accept moves after C2. C1's `/close-cycle` details the section. Its
    scenario: a user's agent, without ArriX's source, writes a Python
    feature plugin, loads it into a running session, builds a document
    over the stdio protocol, renders it headless and checks the result;
    the document then opens on a machine without the plugin, the feature
    frozen.
  - The Tier 2 wire encoding leans to JSON-RPC derived from the WIT
    world. The ⚠ OPEN stays open until that cycle's ADR, which measures
    size and speed.
  - "Distributions" (a named, shareable composition of plugins, config
    and ribbon layout) is a named, unordered cycle, after the runtime
    tier.
- **New crate `arrix-config`** (depends on `arrix-core` and
  `arrix-plugin-api` for `FeatureTypeSpec`; external `toml`, `serde`).
  - Holds the setting specs, the layered model, keymap and theme parsing
    with spans turned into `Diagnostic`s, and `CommandSpec` with the
    command registry as plain data.
  - Reads no files: a `ConfigSource` hands it bytes, like `arrix-doc`'s
    `DocumentSource`.
  - Both `arrix-app` and `arrix-cli` depend on it. This moves the
    registry's data type out of `arrix-app`, which still owns the
    session-state predicates and the dispatch.
  - The layer lint's edge table gains the crate. `arrix-ui` does not
    depend on it: it gets a `ThemeView` and a `PaletteView` as plain view
    data.
- **Files** live in `$ARRIX_CONFIG_DIR`, else the platform's config
  directory joined with `arrix` (⚠ OPEN 1). There are three:
  - `settings.toml`: `[ui]`, plus `[defaults."<feature-type>"]`.
  - `keybindings.toml`: `[[bind]] key = "ctrl+k"`, `command = "…"`, and
    an optional `when = "<context>"`.
  - `themes/<name>.toml`: `vars`, plus `colors` keyed by role.
  - The built-in `dark` and `light` themes are embedded TOML in the same
    format.
- **Native file reading** happens in `crates/arrix-app/src/config_files`.
  The wasm lint's `fs` table and `gate-selftest` gain it.
- **`arrix-ui`**: `ThemeView` maps to egui `Visuals`, and `PaletteView`
  gives the palette. New intents: `CommandInvoked { command }`,
  `PaletteQuery { text }`, `PaletteClosed`.
- **`arrix-cli`** gains three subcommands: `describe [--json]`, `config
  check [--dir]` and `config schema`. The JSON output carries a `"schema":
  1` version.
- **A generated reference doc**, `docs/CONFIG-CATALOG.md` (commands,
  settings, theme roles, keymap contexts). A test holds it fresh, and
  `UPDATE_SNAPSHOTS=1` regenerates it. `docs/README.md` names the generated
  doc as a kind of doc.
- **`debug_state()`** gains three things: the effective theme name, the
  keymap in force, and config diagnostics.
- **What doesn't change.** `arrix-plugin-api` is untouched: a feature
  type's parameter defaults are already declared in its `ParamSpec`, so the
  user's overrides need no plugin cooperation. The document schema is
  unchanged.

## Arris dependencies

None. No step calls the kernel.

## Steps

- [x] **[1]** Step 1: write ADR-0015 and ADR-0016 (above).
  - `SEED.md` §1, §5 and §8.2 rule 9 amended through ADR-0016, and §7's
    cycle order with it.
  - `docs/ROADMAP.md`:
    - §M3's in-list gains "the shell configured by user files
      (ADR-0015)".
    - The spine names the cycle after C1 as "the boundary opened", then
      C2; C3 is reduced to what is left (Tier 1, robotics moved out, the
      north-star accept). The new section is one line until C1's
      `/close-cycle` details it.
    - "Distributions" joins the named, unordered cycles.
  - `docs/PLUGINS.md` ⚠ OPEN (Tier 2 wire encoding) notes the JSON-RPC
    lean and ADR-0016.
  - `docs/BACKLOG.md` gains a "Rejected" line for everything-is-a-plugin
    (it reopens `SEED.md` §6.2 and §6.3 for flexibility the configurable
    shell mostly gives), and one for an agent-only, headless ArriX (it
    gives up the interactive document `SEED.md` §3 exists for), plus
    lines for:
    - plugin-own settings
    - the project layer with trust
    - profiles
    - a settings UI over `toml_edit`
- [x] **[2]** Step 2: create the `arrix-config` crate: setting specs, the
  layered model and `ConfigSource`.
  - A `SettingSpec` has an id, kind, default and doc. The first settings:
    - `ui.theme`
    - `ui.ribbon.hidden-tabs`
    - `ui.panel.tree.visible`
    - `ui.panel.properties.visible`
  - Parsing `settings.toml` over the defaults gives one `Diagnostic` with
    file and line for each of:
    - an unknown key
    - a wrong type
    - a TOML syntax error
  - Each such key keeps its default.
  - The layer lint's edges are updated, and the crate builds for wasm32.
  - Tests: every diagnostic case, the layer order, and a clean file giving
    none.
- [ ] **[2]** Step 3: build the command registry.
  - `CommandSpec` has an id, title, ribbon tab and group, default keys and
    a `when` context.
  - The core table holds `core.palette.open`, `core.config.reload`,
    `core.panel.tree.toggle`, `core.panel.properties.toggle` and the
    ribbon-tab commands. There is also one `<type>.create` per registered
    feature type: core ones in `core.sketch`/`core.part`, and `gears.spur`
    in a `gears` tab.
  - A creation command is disabled while no document is open; M3's
    document plan enables it.
  - `arrix-app` builds the `RibbonView` from the registry, minus
    `ui.ribbon.hidden-tabs`.
  - The `empty_shell` golden is refreshed after reading its
    before/after/diff images.
- [ ] **[2]** Step 4: the keymap.
  - Parse `keybindings.toml`: the key syntax (`ctrl`, `shift`, `alt`,
    `super`; letters, digits, named keys, `f1`–`f12`) and `when` contexts
    taken from session state (⚠ OPEN 2).
  - A user binding replaces the default for its command; an empty list
    unbinds it.
  - Each of these is a diagnostic:
    - two commands on one key in one context
    - an unknown command
    - an unparsable key
  - `arrix-app` turns egui key events into `CommandInvoked`. A text field
    with focus keeps its keys.
  - Tests: parse cases, and a kittest pressing a rebound key that runs the
    command.
- [ ] **[2]** Step 5: the command palette (`core.palette.open`, `ctrl+k`
  by default).
  - The palette searches the registry by title and id, showing each
    command's binding and whether it is enabled. Enter runs the command;
    Escape closes the palette.
  - It is session state and issues no document command.
  - `debug_state()` reports the query and its results.
  - New snapshot scenario: `palette_open` with a query.
- [ ] **[2]** Step 6: themes.
  - The theme format: `vars` with references, and `colors` by role. A
    missing role falls back to the built-in theme's role, with a
    diagnostic.
  - The built-in `dark` (the current look) and `light` themes.
  - `ThemeView` is turned into egui `Visuals` in `arrix-ui`.
  - `ui.theme` selects a built-in theme or `themes/<name>.toml`.
  - New snapshot scenario: `light_theme`. Its coarse frame is read before
    pinning.
- [ ] **[2]** Step 7: feature parameter defaults.
  - `[defaults."<type>"] <param> = "<expression>"` is checked against the
    type's `ParamSpec`. These are diagnostics:
    - an unknown type or parameter
    - an expression that doesn't parse
    - a value of the wrong quantity kind
  - The expression is checked by `arrix-doc`'s parser, which is injected
    into `arrix-config`'s check.
  - `effective_params(type)` gives a new feature's initial form. M3's
    document plan uses it.
  - A test with `gears.spur`'s module set to `1.5 mm` asserts that no
    feature's `evaluate` input hash changes with config. It evaluates a
    document under two configs to identical JSON.
- [ ] **[2]** Step 8: `arrix describe [--json]` and `arrix config check
  [--dir]`.
  - `describe` prints:
    - commands with their effective keys
    - settings with type, default, effective value and doc
    - theme roles
    - keymap contexts
    - feature types with their parameters and effective defaults
    - the compiled-in plugins
  - `config check` prints one JSON line per diagnostic. It exits 0 when
    clean and 1 otherwise.
  - Goldens: an empty config directory and a broken one, under
    `tests/config/`.
- [ ] **[1]** Step 9: `arrix config schema` and the generated catalog.
  - `config schema` emits JSON Schemas for the three file kinds, so taplo
    can complete them in an editor (`#:schema`).
  - `docs/CONFIG-CATALOG.md` is generated from the registry and specs, and
    a test holds it fresh.
- [ ] **[2]** Step 10: config files on native, and reload.
  - The config directory is resolved and read through `config_files`. The
    wasm lint's `fs` table and the selftest are updated.
  - `core.config.reload` re-reads the files and applies them. The status
    bar's diagnostics count includes config diagnostics.
  - Changed files are noticed by a polled modification check (⚠ OPEN 3).
  - Tests: a reload against a temporary directory, and an unreadable
    directory giving the defaults plus a diagnostic.
- [ ] **[2]** Step 11: the acceptance scenario (below), pinned as a visual
  scenario and a CLI test.

## Acceptance

Two checks close the plan.

**`cargo test -p arrix-app --test visual`, scenario `configured_shell`:**
1. The app starts on a temporary config directory.
2. That directory gets these files:
   - a `settings.toml` that hides the Part tab, selects a user theme and
     sets `gears.spur`'s module
   - a `keybindings.toml` that rebinds the palette
   - `themes/mine.toml`
3. `core.config.reload` runs.
4. The golden shows:
   - the ribbon without Part, with the gears tab
   - the theme's colours in the coarse frame
   - the rebound key opening the palette
   - `debug_state()` reporting the effective config and no diagnostics
5. A broken key is then written and reloaded: the app is still up, that
   key has its default, and one diagnostic names its file and line.

**`cargo test -p arrix-cli --test config`:** `arrix config check` on the
same directories gives the same diagnostics. `arrix describe --json`
matches its golden and shows `gears.spur`'s effective module.

The whole gate is green, the wasm build included.

## Docs to update on completion

- `docs/UI-RENDERING.md`:
  - §The shell: the registry's data in `arrix-config`, the keymap, the
    palette and themes as built.
  - A new §Configuration: the files, the layers, fail-soft, reload, and
    the line that config never reaches `evaluate`.
  - §View types: `ThemeView`, `PaletteView` and the new intents.
  - §Visual debugging: the new `debug_state()` fields.
- `docs/ARCHITECTURE.md` §Crates: the `arrix-config` row and its edges.
  §Gates: the wasm lint's `fs` entry.
- `docs/PLUGINS.md` §1. Features: a user may override a type's parameter
  defaults, and this never reaches `evaluate`.
- `docs/README.md`: the generated reference doc and how it is held fresh.
- `docs/ROADMAP.md` status line: M3 plan 1 landed, with ADR-0015 and
  ADR-0016 (the reorder and distributions are already written by step 1).
- `AGENTS.md`:
  - Current state.
  - §Commands: `arrix describe`, `arrix config check`/`schema`, and
    regenerating the catalog.

## Open questions

- ⚠ OPEN 1: where the config directory lives on each platform.
  - On Linux: `$XDG_CONFIG_HOME`, else `~/.config`.
  - On macOS: `~/Library/Application Support`.
  - On Windows: `%APPDATA%`.
  - Whether to take the `dirs` crate or read the variables directly.
  - Agent decides by step 10.
- ⚠ OPEN 2: the `when` grammar. Should it be a single context name
  (`sketch`, `part`, `palette`, `text`) or VS Code-style expressions?
  Preferred: a single name until a binding needs more. Agent decides by
  step 4.
- ⚠ OPEN 3: how a changed file is noticed. Options are a polled
  modification time (no dependency, about 1 s), the `notify` crate, or
  reload-on-command only. Tests use the command either way. Agent decides
  by step 10.
- ⚠ OPEN 4: may the user hide core ribbon tabs (`core.sketch`,
  `core.part`), or only plugin tabs? Preferred: any tab, since the palette
  still reaches every command. Human, by step 3.
