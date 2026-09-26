# ADR-0004: A plugin command's undo is its expansion, inverted by the core

- Status: Accepted
- Date: 2026-09-26

## Context

A plugin command (`docs/PLUGINS.md` §Contribution points, 3) is a gesture
that edits the document through core commands and edits to the plugin's
own data. Undo is per author, by inverse commands (`SEED.md` §6.1), and
one gesture is one undo entry (`SEED.md` §8.2). Left open at kickoff:
whether a plugin command's undo entry is the literal reverse of what it
expanded into, or an inverse the plugin supplies over its own data, which
would let a plugin have undo that is not the reverse of its steps.

What the core already is (`docs/DATA-MODEL.md` §Commands and undo):
applying a command is a pure function returning its inverse, the
authority keeps per-author stacks of those inverses, and a property test
holds apply-then-inverse to the identity. Documents open without the
plugins that made them (`SEED.md` §6.4), and the command boundary is a
protocol, so the authority may one day be a server with no plugin
installed.

## Decision

A plugin command runs on the client that issues it: the plugin expands
the gesture into one `Group` of core commands and `PluginData` edits,
and that `Group` is what is submitted. Its undo entry is that group's
inverse, computed by the core like any other command's. A `PluginData`
edit is a whole-record put or remove in the plugin's section, so its
inverse is the record's previous bytes or its removal, computed without
reading them. A plugin never supplies an inverse, and the authority never
runs plugin code to apply, undo or redo.

## Consequences

- Undo and redo work with the plugin absent: in a document opened
  without it, on a thin client, on a server, after it failed to load.
- The identity property covers plugin commands with no plugin code in
  the loop, and a command log replays byte for byte without plugins.
- Staleness is per record: a plugin record becomes a DAG node when
  `PluginData` lands, stamped like any other, so an undo that would
  clobber another author's edit to the same record is refused.
- A plugin cannot give undo its own meaning (undoing a solve by
  re-solving, say). Its undo is the literal reverse of what it wrote; a
  plugin that wants a different result issues a new command.
- An undo entry holds the whole previous record, not a delta. A large
  record costs memory per edit; a delta encoding, if a budget asks for
  one, is the core's to add and changes no plugin.
- `PluginData` and the `commands` export of the WIT world land when the
  first plugin needs them (M3's ribbon at the earliest); this decision is
  what they are built to.

## Alternatives considered

- **The plugin supplies its inverse**: undo would need the plugin
  present, at its matching version, on whatever runs the authority, and
  the core could no longer test that an inverse inverts.
- **One undo entry per expanded command**: breaks one gesture = one
  command; undoing half a plugin gesture leaves a state no gesture made.
- **Record edits as patches the plugin interprets**: the core could not
  invert them without the plugin, which is the first alternative again.
