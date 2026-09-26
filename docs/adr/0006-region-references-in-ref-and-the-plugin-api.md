# ADR-0006: A sketch region is a `Ref`, and a feature input; plugin API 0.2

- Status: Accepted
- Date: 2026-09-27

## Context

A feature that consumes a sketch region (`core.extrude`, and in the
acceptance of `plans/c1-m1-sketch` a feature holding the holed plate)
stores the region's key: the entities bounding the face and a point
inside it (`docs/DATA-MODEL.md` §Sketches). When the key no longer
resolves to exactly one region, the feature fails with `ref.lost` and the
current regions as candidates, never the nearest region.

A diagnostic's candidates are `Ref`s, and a feature's inputs are `Ref`s,
so a region needs a `Ref` form. `Ref` lives in `arrix-core`, is mirrored
by the WIT world's `reference` variant, and is `Eq + Ord + Hash`; the
sketch's `RegionKey` held its sample as `f64`s and lived in
`arrix-sketch`, which `arrix-core` cannot name. A feature type also
declares what each input resolves to (`input-kind`), and the only kind
was a plane. The human chose this over a document-only field (no plugin
could reference a region, and candidates could not be `Ref`s) and over
deferring the reference to `core.extrude`'s plan (2026-09-27).

## Decision

`Ref` gains `Region { feature, key: RegionKey }`, and `RegionKey` moves to
`arrix-core`: the sorted entity ids and the sample in whole nanometres of
the sketch's `(u, v)`, so a key compares, orders and hashes exactly.
`arrix-sketch` resolves a key (`ResolveRegion`) and still makes keys from
its faces. The plugin API gains an input kind `region`, which the host
resolves to the region's keyed `Profile` on the sketch's plane. The WIT
world mirrors both (`region-ref`, `reference.region`, `input-kind.region`,
`input-value.region`), and the plugin API goes from 0.1.0 to 0.2.0, a
breaking change while it is 0.x.

## Consequences

- A lost region is an ordinary lost reference: a `ref.lost` diagnostic
  whose `refs` name the stored key and whose `candidates` are the sketch's
  regions as `Ref::Region`, most shared bounding entities first, which a
  client re-picks by command.
- `core.extrude` and plugin features take a region input the same way,
  and a plugin feature can extrude a sketch region through the kernel
  service with no sketch type in the plugin API.
- A plugin built against 0.1 no longer loads: its manifest's range does
  not match 0.2 (`gears` moves to `^0.2`). A plugin that matched a
  `reference` or `input-value` exhaustively has one more case.
- A nanometre sample is exact data, not a measurement: rounding it moves
  it by at most half a nanometre, a two-thousandth of the length
  tolerance, and the sample sits as far inside its face as the face
  allows.
- The input hash writes a region input as its profile, and a plane as its
  frame, bare, as before, so no existing hash moved.

## Alternatives considered

- **A document-only field for region keys**: no plugin API change, but
  plugins could not reference regions and candidates could not be `Ref`s.
- **Defer region references to `core.extrude`'s plan**: the same change
  later, with this plan's acceptance moved with it.
- **Keep the sample as `f64` behind a total-order wrapper**: `Ref` would
  order by bit patterns, `-0.0` and `0.0` would be two keys, and the WIT
  record would carry floats a plugin could not compare safely.
