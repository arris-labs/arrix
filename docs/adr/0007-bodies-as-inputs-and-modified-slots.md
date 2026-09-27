# ADR-0007: Bodies as feature inputs, and output slots that modify them

- Status: Accepted; decision 5's `borrow<body>` superseded by ADR-0008
- Date: 2026-09-27

## Context

M1's accept joins a gear to a sketched plate and cuts a pocket through
both (`docs/ROADMAP.md` §M1, `plans/c1-m1-slice`). A join or a cut does
not make a new body: it changes one that an earlier feature made. The
data model already says so in outline (`docs/DATA-MODEL.md` §Bodies
across features: *slot `s` as of feature `f`*, a cut body is a new
version of the same slot, deleting a boolean's tools ends their slot),
but nothing states how a feature type declares it, how a reference
resolves to a version, or which DAG edges that implies. The DAG section
deferred those edges "to join and cut".

The words are needed by `core.extrude` (join and cut), `core.boolean`
and, later, fillet, chamfer, hole and patterns. `core.extrude` evaluates
through the plugin API's `Kernel` like every plugin feature, and a
plugin is to be able to do anything a built-in does through the public
API (`SEED.md` §6.3), so the words belong in the plugin API, not in a
document-only extension beside it (as `sketch_slots` is for the one
built-in whose output is not kernel geometry).

## Decision

1. **A body input.** A feature type may declare an input of kind `body`.
   It is written as `Ref::Slot { feature, slot }`, the feature that
   *made* the slot, and it resolves to that slot's body **as of the
   reading feature's place in the history**: the output of the last
   feature before the reader that modifies the slot, or the maker's
   output if none does. Inserting a modifier earlier moves every later
   reader to its version; nothing in the reader's record changes.
2. **A modifying output slot.** An output slot spec may name one of the
   type's body inputs as the one it `modifies`. Its body is then the
   target slot's **next version**, not a slot of its own: readers after it
   name the target's slot and get this body, and `Ref::Slot` on the
   modifier's own slot name names nothing.
3. **Consumption.** A body input that no output slot modifies is read as
   a tool and **ends its slot**: a feature after the consumer that reads
   the slot, by body input or by a persistent name into it, fails as
   `slot.consumed`, naming the consumer.
4. **The implicit edges.** A feature that reads slot `s`, by a body input
   or by a `Topo` name whose root or steps name a feature whose body is a
   version of `s`, reads every feature before it that modifies `s`. A
   `Topo` name resolves against the version of its slot current at the
   reader, so a name kept through a join still resolves after it. The
   edges are derived from the document like every other edge; the
   existing order check and cycle refusal cover them.
5. **One vocabulary.** Built-ins and plugins declare these the same way
   (`input-kind.body`, `slot-spec.modifies`), and the kernel service gains
   `fuse` and `cut` (target, tool), so `core.extrude`'s join and cut, and
   a plugin that modifies a body, go through the same API. The plugin API
   goes to 0.3.0, a minor change: it adds `input-kind.body`,
   `input-value.body(borrow<body>)`, `slot-spec.modifies:
   option<string>`, `kernel.fuse` and `kernel.cut`, and removes nothing.

## Consequences

- A modify-in-place feature needs no new slot names and no rename
  downstream: a pocket after a join names the plate's slot, as does
  everything after the pocket.
- A reader's input hash covers the body version it read, so an upstream
  modifier's change re-evaluates it through the ordinary cache.
- A body is resolved per reader, so the evaluator keeps, for each slot,
  its versions in history order; the evaluated view of a part shows each
  slot's last version.
- A tool body read by `core.boolean` is gone for everything after it,
  which is what a user expects of a boolean's tools; reading it for
  something else first is a matter of order.
- `gears` is unchanged but for its manifest's `api` range: it makes a new
  body and modifies nothing.

## Alternatives considered

- **Every modifier makes a new slot**, and readers after it re-point to
  the new one: explicit, but inserting a join earlier would silently
  leave later features on the old body, or need a re-bind command for
  each of them, which is the silent re-bind `SEED.md` §8.2 forbids.
- **A document-level extension** for modifying slots, as `sketch_slots`
  is for sketches: no plugin could modify a body, and `core.extrude`
  would evaluate outside the one path every feature shares.
- **A tool body left alive after a boolean**: nothing reads it that could
  not read it before the boolean, and a body that is both inside the
  result and a slot of its own is two answers to "which body is this".
