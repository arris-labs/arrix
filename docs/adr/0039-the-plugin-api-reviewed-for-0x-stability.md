# ADR-0039: `arrix-plugin-api` reviewed for 0.x stability

- Status: Accepted
- Date: 2026-10-14
- Amends: `docs/PLUGINS.md` §Versioning (what a 0.x minor may break)

## Context

ADR-0034 puts a review of `arrix-plugin-api` in C3 M3, before the human
publishes the crate and the first plugins leave the tree. The API has had
eight minors (0.1.0 to 0.8.0), five of them breaking, each driven by a
first-party plugin's need. Three tiers and the Python SDK now mirror it:
the WIT world (the authority), the Rust traits and plain types, the wire
codec generated from the WIT, and `python/arrix`. `tests/wit_equality.rs`
holds the first three equal. Every item below was read against the plugins
that use it (`gears`, `robotics`, `fasteners`, the standoff, `bom`) and
against the test kit (`arrix-plugin-test`).

## Decision

1. **The review's outcome: every item is kept; nothing is cut or renamed
   now.** Each of the world's six interfaces has a user in tree and none is
   dead:

   | Item | Used by | Outcome |
   |---|---|---|
   | `types` (ids, quantities, references, diagnostic, profile, frame, motion) | all | kept |
   | `kernel` (`extrude`, `revolve`, `names`, `face-frame`, `measure`, `fuse`, `cut`, `common`, `fillet`, `chamfer`, `copy`, `mirror`) | `gears`, `fasteners`, the standoff | kept |
   | `feature` (`describe`, `evaluate`, specs, inputs, slots, tool, choices, `shown-when`) | `gears`, `fasteners`, the standoff | kept |
   | `document` (`parts`, `mesh`, `mass`, `assemblies`) | `robotics`, `bom`, commands | kept |
   | `exchange` (`exporters`, `run`) | `robotics`, `bom` | kept |
   | `commands` (`commands`, `expand`, the command records) | `fasteners` | kept |
   | Rust: `Feature`, `Exporter`, `Commands`, `Kernel`, `export_plugin!`, `math`, the `guest` feature | all Rust plugins | kept |
   | Python: `feature_type`, `exporter`, `command`, `param`, `choice`, `input`, `slot`, `diagnostic`, `key`, `profile` helpers, `session`, `eval` | the Python plugins, the tests | kept |

   `reference`'s `plugin(plugin-ref)` case has no producer (plugin document
   data is unbuilt, ADR-0038), and is kept as the reserved place of one: a
   case removed and re-added is a breaking change twice.

2. **Warts found, left as they are, each a backlog line.** None is worth a
   breaking release by itself; each is cheap to fix with the next break that
   happens anyway.
   - Python `arrix.input` and `arrix.eval` shadow builtins under `from
     arrix import *`. Docs and examples import the module, so none does.
   - `kernel`'s `mass-properties` (at unit density) and `document`'s
     `inertial` (with a material) are two shapes of mass, named alike.
   - The `kernel` interface has no delete or query of a body beyond
     `names`; an operation arrives with the plugin that needs it, as
     `revolve` and `fillet` did.
   - `command-group` carries no `delete`; ADR-0038 left it out until a
     plugin asks.

3. **What a 0.x minor may break, from here on.** The WIT world, the Rust
   mirror and the Python SDK's helper signatures: any of their items may
   change in a minor bump, 0.N to 0.N+1, with the commit body stating
   `API: breaking, …` and an ADR when the change is a design choice rather
   than a correction. What it may not break, whatever the version:
   - **A patch** (0.8.x) never changes the world; it is for the crate's
     non-WIT code (the `guest` feature, `math`) and documentation.
   - **The meaning of a stored thing.** A document's file format has its
     own `schema` number and migration (`docs/DATA-MODEL.md`), and a
     persistent name, an id or a `type_version` keeps its meaning across API
     minors. A minor that changes what a plugin sees, not what a document
     holds, never rewrites a document.
   - **Results.** `arrix_plugin_api::math` returns the same bits at every
     tier (ADR-0036); a change to its output is an ADR and refreshes goldens
     deliberately.
   - **The manifest.** `arrix-plugin.toml`'s `[plugin]`, `[capabilities]`,
     `[contributes]` and `[test]` tables change by their own ADR; an `api`
     range is `^0.N`, which matches the one minor, as the host supports one
     range at a time (`docs/PLUGINS.md` §Versioning).

4. **A break is paid for in tree.** A breaking minor updates the WIT, the
   Rust mirror, the wire codec, the Python stubs, every manifest's `api`
   and every first-party plugin in the same commit, as 0.7.0 and 0.8.0 did.
   A plugin outside the tree finds its `api` range no longer overlapping and
   opens frozen with that reason; it is not partly loaded.

5. **What 1.0 waits for.** Not this ADR: a published `arrix-plugin-api`
   and a plugin in a repository of its own, both the human's acts after the
   accept (ADR-0034), and a cycle with no breaking minor. The test kit
   (`run_tier0`, `run_tier1`) is the author's guard: a plugin that passes it
   against a minor passes the same checks against the next one it is
   updated to.

## Consequences

- No API bump lands with this ADR; the API stays at 0.8.0 into the accept.
- The warts of §2 are the first candidates when a break is needed for
  another reason, so they do not each cost a release.
- A plugin author reads one paragraph (§3) for what to expect from an
  upgrade; the commit bodies' `API:` lines remain the per-change record.

## Alternatives considered

- **Cut `plugin-ref` and re-add it with plugin data.** Keeps the world
  smaller, and breaks it twice; the reservation costs one variant case.
- **Land the renames now as 0.9.0.** Every wart in §2 is a name, and a name
  fixed before any outside plugin exists is cheapest, but the accept is two
  steps away and a break now re-touches the wire, the SDK and five plugins
  for no scenario. Left to the next real break.
- **Freeze the world at 0.8.0 for the 0.x line.** A stability the API has
  not earned: robotics, fasteners and the kit each found a gap in the
  last three minors.
