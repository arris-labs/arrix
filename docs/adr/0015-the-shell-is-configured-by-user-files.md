# ADR-0015: The shell is configured by user files

- Status: Accepted
- Date: 2026-10-07

## Context

M3 builds the shell: the command registry, the ribbon with the gears
plugin's tab, the command palette (`docs/ROADMAP.md` §M3). Built as code,
every change a user wants in it (a key, a colour, a tab they never use, the
module their gears usually have) is a change to ArriX. ADR-0016 makes the
user's agent a first-class user of ArriX, and an agent edits text far more
cheaply than it rebuilds an application. Configuring the shell is outside
M3's in-list, so this ADR names the scenario it unlocks.

Product scenario: a user, or their agent, rebinds a command, switches
theme, hides a ribbon tab and sets a plugin feature's parameter default by
editing text files that `arrix config check` validates, and sees the change
without restarting.

## Decision

1. **The shell is data.** Every user action is a command spec with a
   stable id in one registry, which the ribbon, the command palette and the
   keymap all read; a registered feature type contributes its creation
   command. The registry is plain data, in a new crate, `arrix-config`,
   that reads no files (a `ConfigSource` hands it bytes).
2. **Humans and agents write TOML; the machine writes JSON.** The user's
   files are TOML (`settings.toml`, `keybindings.toml`,
   `themes/<name>.toml`). What ArriX emits stays JSON: the document,
   `debug_state()`, `arrix eval`, and `arrix describe`.
3. **Two layers**: the built-in defaults, then the user's config
   directory. The built-in themes are embedded TOML in the user's format.
4. **Fail soft.** A broken file never stops the app: an unknown key, a
   wrong type or a syntax error keeps that key's default and is a
   diagnostic naming the file and line. `arrix config check` reports the
   same diagnostics headless.
5. **Config is client state.** It never enters the document (`SEED.md`
   §6.1 rule 4) and never reaches a feature's `evaluate`: a parameter
   default only seeds a new feature's form, and once the feature is
   committed the value is the document's (`SEED.md` §6.3, determinism).
6. **The app describes itself.** `arrix describe --json` lists everything
   there is to configure, `arrix config schema` emits JSON Schemas for the
   three file kinds, and a generated catalog (`docs/CONFIG-CATALOG.md`) is
   held fresh by a test.
7. **The app reads, never writes, its config.** A settings UI would have
   to keep the user's comments and order; it waits for a scenario.

## Consequences

- Rebinding, theming, hiding a tab and setting a default are file edits
  with no rebuild, checked by the CLI and applied by `core.config.reload`.
- `arrix-plugin-api` is untouched: a feature type's defaults are already
  in its `ParamSpec`, so a user's override needs no plugin cooperation.
  The document schema is unchanged.
- The registry, setting ids, theme roles and keymap contexts become a
  public surface: renaming one breaks a user's file, so ids are kept
  stable (ADR-0016, constraint 1).
- A new crate and a native file reader (`arrix-app`'s `config_files`),
  under the wasm lint; the browser gets a store behind `ConfigSource` in
  C4.
- Not covered: a project or document config layer (it needs a trust
  model), profiles, a plugin's own settings, viewport colours and mouse
  bindings (with the viewport port). Each is a backlog line.

## Alternatives considered

- **A hard-coded shell, configured later**: every M3 widget would be
  rewritten when configuration arrives, and the registry is cheapest to
  make data before anything reads it.
- **JSON for the user's files**: no comments, and poorer line diagnostics
  for hand edits; JSON stays the machine's output.
- **A settings UI first**: more code, and it writes files, which needs
  `toml_edit` to keep comments; the file is the UI for now.
- **A project layer with the user's**: a document's directory could rebind
  keys or run anything it names, which needs a trust model first.
