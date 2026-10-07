# ADR-0016: The agentic user

- Status: Accepted
- Date: 2026-10-07

## Context

`SEED.md` names agents twice: as ArriX's developers, and as a way to drive
ArriX ("It never has to click", §5). Since the seed, the user's own agent
has become the cheapest way to change a tool. An agent can write a panel,
a theme, a keymap, a feature plugin or a whole shell around a stable core
in an afternoon. What it cannot cheaply rebuild is a B-Rep kernel with
provenance, persistent naming, a document that outlives its plugins, a
deterministic evaluator with replayable kernel records, a constraint
solver, and a protocol and harness it can trust. So the value of ArriX
moves toward that core, and toward how easily an agent can work above it.
The configurable shell (ADR-0015) is the first decision this shapes, and
the cycle order is the second.

## Decision

1. **ArriX invests in what users' agents can't cheaply rebuild**: the
   kernel and naming, a document that outlives its plugins, determinism
   and the kernel record, the solver, the protocol, the headless harness.
   **Everything above it is replaceable by them**, under five constraints
   that bind every later cycle:
   1. **Stable ids are the API.** Commands, settings, theme roles, keymap
      contexts, feature types and their parameters are named by ids that
      are kept, versioned or migrated, never silently renamed.
   2. **Text in, diagnostics out, failing soft.** What a user or agent
      writes is text; a mistake in it is a diagnostic with a file and line,
      and the rest keeps working.
   3. **The app describes itself**: `arrix describe`, JSON Schemas, and
      generated catalogs held fresh by the gate.
   4. **A short extension loop**: edit, reload, see, with no rebuild of
      ArriX.
   5. **The harness is product.** The headless render, `debug_state()`,
      the eval JSON and the test kit are a public interface, with a
      stability promise from the runtime-tier cycle on.
2. **`SEED.md` is amended**: §1 names the user's agent beside the user,
   §5 describes what an agent edits and how it gets in, §8.2 gains rule 9
   (the five constraints), and §7 takes the cycle order below.
3. **The cycle after C1 is "the boundary opened"**: the Tier 2 host,
   `arrix serve --stdio` (the `Session` protocol on stdio, the agent's way
   in), the Python SDK and the test kit. C2 follows it, and C3 keeps Tier
   1, robotics moved out and the north-star accept. C1's `/close-cycle`
   details the section and numbers it. Its scenario: a user's agent,
   without ArriX's source, writes a Python feature plugin, loads it into a
   running session, builds a document over the stdio protocol, renders it
   headless and checks the result; the document then opens on a machine
   without the plugin, the feature frozen.
4. **The Tier 2 wire encoding leans to JSON-RPC** derived from the WIT
   world: an agent reads it, any language speaks it, and `arrix serve`
   uses the same framing. The ⚠ OPEN in `docs/PLUGINS.md` stays open until
   that cycle's ADR, which measures size and speed against a binary
   encoding.
5. **"Distributions"** (a named, shareable composition of plugins, config
   and ribbon layout) is a named, unordered cycle, after the runtime tier.

## Consequences

- The boundary opens before the mechanical core, so an agent can extend
  ArriX in Python without its source a cycle earlier; assemblies and STEP
  import wait one cycle.
- Each later design is checked against the five constraints: an id that
  changes, a failure without a file and line, a surface `describe` cannot
  list or a change that needs a rebuild is a defect.
- The harness's text formats stop being internal once the promise starts:
  a change to them is versioned like the plugin API.
- The core does not grow: what an agent can build above it stays out of
  it, and the plugin model (`SEED.md` §6.2, §6.3) is unchanged.

## Alternatives considered

- **Everything is a plugin** (the shell, the sketcher and the part
  features as plugins too): it reopens `SEED.md` §6.2 and §6.3 for
  flexibility the configurable shell mostly gives.
- **An agent-only, headless ArriX**: it gives up the interactive document
  `SEED.md` §3 exists for.
- **Keep the seed's order** (C2, then the boundary in C3): an agent would
  wait two cycles for a way in that is not the CLI, while the core it
  would build on is already C1's.
