# ADR-0020: A name into a consumed slot follows its consumer

- Status: Accepted
- Date: 2026-10-08
- Amends: ADR-0007 (the consumed slot, for persistent names only)

## Context

ADR-0007 ends a body slot at the feature that reads it as a tool: a later
reader of the slot, by body input or by persistent name, fails as
`slot.consumed`. A join is such a consumer. `core.boolean` in `fuse` mode
reads the gear as a tool and makes the plate's next version, and an edge of
the gear the join did not touch keeps its name unchanged (`chain: []`), so
its name is rooted at the gear and resolved against the gear's slot, which
the join ended. C1's accept fillets an edge of the joined gear downstream,
by persistent name, live and frozen; under ADR-0007 alone that fillet fails
`slot.consumed` (`plans/m3-accept` step 2's finding).

## Decision

A `Topo` name whose slot a feature consumed is looked for in the slot that
feature modifies, at its version current at the reader, and again through
that slot's own consumer, if it has one. Found there, the name reads that
body. Not found, or the consumer modifies no slot (or more than one), the
reader fails `slot.consumed` as before, naming the consumer. A body input
on a consumed slot still fails `slot.consumed`: a tool's body is gone, and
only the names of what survived it carry on. The DAG holds the edges this
reads: a name's reader reads the consumers of its slot and the features
that touch each slot it follows into.

## Consequences

No name is rewritten and no stored record or input hash changes: a name
that resolved before resolves to the same entity, and one that failed with
`slot.consumed` now resolves where the entity actually survived. A
reference to a joined plugin feature's edge works with the plugin absent,
since a frozen result's names are the live ones. A name a cut removed is
still `slot.consumed`, not a lost reference with candidates, which keeps
the cause (the tool is gone) visible over a guess.

## Alternatives considered

- The join adds a step to every name of its tool, so the names live in the
  target's slot: every joined tool's names and the hashes over them would
  change, a migration for no new information.
- A name into a consumed slot as a lost reference with candidates: it
  hides why the entity is gone and invites a re-pick of the wrong one.
