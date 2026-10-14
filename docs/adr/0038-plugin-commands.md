# ADR-0038: plugin commands

- Status: Accepted
- Date: 2026-10-14
- Amends: ADR-0004 (where the expansion runs; its undo stands), ADR-0037
  §5 (when `document` may be called)

## Context

ADR-0034 gives C3 M3 a `commands` export on all three tiers, for
`fasteners.place`: screws put into picked hole edges. ADR-0004 fixed what
a plugin command is built to: one `Group` of core commands, its undo the
inverse the core computes, the authority never running plugin code. It
left the export, its inputs and its session request to the first plugin
that needs them. plans/c3-m3 step 1 probed the gesture with core commands
alone (`crates/arrix-cli/tests/probe_c3_m3.rs`, the plan's Findings 1 to
4):

- A counterbore's top edge and each pattern copy of it have a persistent
  name that survives width, thickness and spacing edits, so a joint
  anchored on one needs no re-bind.
- Arris orients each of those circles down the hole. A cylinder's end edge
  points out of the cylinder, so a fixed joint needs `flip` to put the
  cylinder in the hole. The circle's orientation is Arris's choice, not a
  stated property; the outward normal of the planar face the edge bounds
  is one by construction.
- A joint on the frozen cylinder's names resolves at the live pose and
  follows the same edits.

The human decided the plan's two open questions: expansion is a request
of its own, then an ordinary submit (OPEN 1); a size used again reuses its
part, found through the snapshot (OPEN 2).

## Decision

1. **The spec.** `interface commands` exports `commands() ->
   list<command-spec>`: `id` (`<plugin>.<name>`), `title`, `group` (the
   ribbon group it sits in on the plugin's tab), `params`, `choices` and
   `inputs` as a feature type's specs, and `max-new`, the most ids one
   expansion may use. A command form derives from it as a create form
   does from a feature type (`docs/UI-RENDERING.md` §Forms as built). At
   0.8.0 a command input's kind is `edge` alone; a spec naming another
   kind is refused at registration (`plugin.spec`) until a plugin needs
   one.
2. **The context.** A command runs in a part or an assembly, the one the
   client's tab looks at (ADR-0030): `context: variant { part(id),
   assembly(id) }`. A plugin may refuse a context it has no use for, by a
   diagnostic of its own.
3. **Inputs arrive resolved, in the context's frame.** A picked edge
   carries its component path from the assembly down (empty in a part),
   its persistent name, as an `Anchor` names it (ADR-0029), and its
   geometry: a circle's centre, axis and radius, or a line's two ends, or
   neither for another curve; and the outward normals of the planar faces
   it bounds, in the order of their names. The geometry comes from one
   recorded kernel call per pick (`arrix-kernel`, `SEED.md` §6.6) against
   the base generation's evaluation, transformed by the component's solved
   pose. A plugin reads a hole's size and side without a kernel call of its
   own, and reads the side from a face normal, never from the circle's
   axis alone (Findings 2).
4. **The snapshot during expansion.** Under `document-read`, `document`
   may be called while `expand` runs, against the base generation, as
   while an `exchange` call runs (ADR-0037 §5 widened; `evaluate` still
   gets none). The snapshot's `part-info` gains `features`: each history
   feature's id, type id, name, choices and evaluated params, in history
   order. A plugin finds the parts it made by their features (OPEN 2), not
   by name.
5. **The output.** `expand(id, context, params, choices, inputs, ids) ->
   result<command-group, diagnostic>`, with `params` evaluated and
   `choices` as `evaluate` gets them. A `command-group` is a label and a
   list of command records, a mirror of the core commands a plugin may
   issue: `add-part` (with its features), `add-feature` (at the end of a
   part), `add-component`, `edit-component`, `add-joint`, `edit-joint`,
   `add-material` and `assign-material`. A record's ids are the API's `id`,
   its references the API's names, and its expressions either a quantity
   or an expression's text, which the host parses. Deletes and parameter
   edits are left out until a plugin needs them. The host translates the
   group into one `Command::Group` under that label and refuses it whole
   when a record is malformed, naming the record (`command.invalid`): an
   unparsable expression, an id not from `ids` or used twice, a feature
   type the registry lacks. Applying it is the core's, refused or stale as
   any command is. Its undo is its inverse, computed by the core (ADR-0004).
6. **Ids.** A record's id is minted by the command's author (`docs/DATA-
   MODEL.md` §Identifiers), so the client mints `max-new` fresh ids (64
   when the spec says none) and the request carries them. The plugin takes
   them in order; a record naming an id outside the list is
   `command.invalid`. Expansion is a pure function of its arguments and
   the snapshot: no clock, no randomness, its math the API's (ADR-0036).
   The test kit expands twice and requires equal groups.
7. **Capability.** A plugin contributing commands needs `document-write`.
   A command spec from a plugin without it is refused at registration
   (`plugin.capability`). The manifest's `[contributes] commands` lists
   their ids, informative as `features` is.
8. **The session: two requests (OPEN 1).** `Request::Expand { command,
   context, params, choices, inputs, ids, base }` runs the plugin where
   the session's registry is, once `base` is evaluated, and is answered
   `Expanded { group }` or the plugin's diagnostic. A `base` older than the
   newest evaluated generation is refused as stale. It makes no generation
   and nothing is undone. The client then submits the group as an ordinary
   envelope on the same base, so a commit in between makes the submit
   stale by the usual rule, and a command form's preview is the group's
   `Preview` (ADR-0019) with no new machinery. `params` and `choices` are
   the form's expressions and words, evaluated by the session. The
   fields and the request are additive, so the protocol stays at version 1
   (ADR-0026 §3) and its schema golden changes.
9. **Where the plugin runs.** ADR-0004's "on the client" becomes: on
   request of the client that issues the gesture, wherever the session's
   registry is (the app's session, `arrix serve`'s), never on the apply
   path. The authority applies core commands only, and undo and redo work
   with the plugin absent, as ADR-0004 decided.
10. **The app.** A plugin's command enters the command registry as its id,
    as a button in its ribbon group on the plugin's tab. Running it opens a
    command form in the property card, a draft as a create form is, seeded
    from the selection into the fields it fits. Picks in an assembly's tab
    carry their component path. OK sends `Expand`, then submits the group:
    one gesture, one undo entry. A diagnostic from either shows under the
    form, and the draft stays.
11. **API 0.8.0, breaking**: the world gains an export, and `part-info` a
    field. `export_plugin!` takes `commands = …` beside `features` and
    `exporters`; the Rust mirror is a `Commands` trait and the records as
    plain types. The wire's codec, the Python SDK, `wit_equality.rs`'s
    inventory and every manifest's `api` follow.

## Consequences

- A gesture a plugin offers is undoable, replayable and previewable with
  no plugin code on the authority, and a document it edited opens without
  it: every record it wrote is a core one.
- A command form costs two round trips on OK. Expansion is cheap beside
  the evaluation the submit causes, so this is not felt.
- The group crosses the wire before it applies, so a client can show it,
  log it or refuse it; a remote authority never loads a plugin.
- `PluginData` stays unbuilt. Its records join the mirror when a plugin
  needs document data of its own.
- A command input of a kind other than `edge`, and a delete in a group,
  each wait for a plugin that needs them (backlog lines at the plan's
  close).
- The snapshot's features grow every exporter's `parts()` reply slightly;
  a reader that ignores them is unaffected.

## Alternatives considered

- **One request that expands and submits**: one round trip, but the client
  never sees the group, a preview needs a speculative expansion path of
  its own, and the authority's request handler would run plugin code.
- **A new part per gesture**: no API growth, but a bill of materials lists
  one M3x10 per gesture and the document fills with copies.
- **Finding a plugin's parts by name**: no API growth, but a user's rename
  breaks it silently.
- **The session minting ids**: the session never mints (`docs/DATA-
  MODEL.md` §Identifiers); an author's ids keep its commands self-contained.
- **Inputs as bare names, the plugin calling the kernel**: needs `kernel`
  and an evaluation context during expansion, and each plugin re-derives
  the side its own way.
- **The side read from the circle's axis**: works on `arris` 0.8, but is
  an unstated property of the kernel's.
