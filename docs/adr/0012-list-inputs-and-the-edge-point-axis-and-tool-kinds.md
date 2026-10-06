# ADR-0012: Inputs that are lists, and the edge, point, axis and tool kinds

- Status: Accepted
- Date: 2026-10-06

## Context

A feature record holds one `Ref` per named input (`FeatureRecord.inputs:
BTreeMap<String, Ref>`), and the plugin API knows three input kinds:
`plane`, `region`, `body` (ADR-0006, ADR-0007). M2's part features need
more (`docs/ROADMAP.md` §M2, `plans/c1-m2-features`): a fillet names
several edges of one body, a hole stands at several sketch points, a
revolve turns about an axis, and a pattern or mirror of features copies
those features' tools (ADR-0013). Built-ins evaluate through the plugin
API like every plugin feature (ADR-0007, decision 5), so what they read
has to be words of the plugin API, not a document-only side channel.
Product scenario: a fillet of the four top edges of a plate, and a
counterbored hole at three points of a sketch on its top face.

## Decision

1. **A list input.** An input spec says whether it takes a list
   (`input-spec.list: bool`). In a record, a list input is a JSON array
   of `Ref`s, a single input a `Ref` as today: `FeatureRecord.inputs`
   maps a name to an `Input`, one `Ref` or many, read untagged, so every
   record written so far reads unchanged. An array on a single input, or
   a lone `Ref` on a list input, is `input.list`; an empty array is the
   input unset. Order is the record's and is kept.
2. **Resolved, a list is its elements in order**, each a `resolved-input`
   with the input's name. `resolved-input` keeps its shape; a type reads
   a single input as before and a list by collecting the entries with its
   name (`FeatureArgs::inputs_named`). No recursive variant is needed in
   the WIT world.
3. **`edge`.** A `Ref::Topo` naming an edge, resolved like any `Topo`
   reference (a lost name is `ref.lost` with candidates, a face or a
   vertex `input.wrong-kind`), and passed as its persistent name. Which
   body it is on is the kernel call's to check (`kernel.unknown-name`).
4. **`point`.** A `Ref::Sketch` naming a sketch point, or a `Ref::Topo`
   naming a vertex, resolved to `point { at: vec3, normal: option<vec3> }`
   in world SI: a sketch point carries its sketch plane's normal, a vertex
   none.
5. **`axis`.** A `Ref::Sketch` naming a line, or a `Ref::Topo` naming a
   straight edge, resolved to `axis { origin: vec3, direction: vec3 }`
   (a line's start and its unit direction; an edge's as Arris orients
   it). A curved edge is `input.wrong-kind`. A world axis is a type's own
   choice, as a world plane is `core.datum-plane`'s, not an input.
6. **`tool`.** A `Ref::Feature` naming a feature that joins or cuts,
   resolved to that feature's kept tool: `tool { body: body, op:
   tool-op }`, `tool-op` `fuse` or `cut` (ADR-0013). A feature that
   keeps no tool is `input.no-tool`, naming it. A tool input reads the
   feature as any `Ref::Feature` does (a DAG edge), and its input hash is
   that feature's.
7. **API.** `arrix-plugin-api` 0.4.0: `input-spec.list`, the four kinds
   and their `input-value` cases, the `point`, `axis` and `tool` records.
   API: minor (breaking under 0.x). It ships with the kernel operations
   `plans/c1-m2-features` adds, in one bump.
8. **Persistence.** List inputs are a new stored shape, so the schema
   goes to 3, with an identity migration from 2 (an older build refuses
   the document by name rather than failing to parse an array).

## Consequences

- One `FeatureEdit` sets an input whole, list or single, and its inverse
  is the earlier value; adding an edge to a fillet is one command.
- A list input is one DAG edge per element; a lost element fails the
  feature, naming that element, and never drops it silently.
- `gears` takes 0.4 with its `api` range only.
- Choice fields stay undeclared in the API (`docs/BACKLOG.md`).

## Alternatives considered

- **Numbered single inputs** (`edge.0`, `edge.1`): no schema change, but
  a form cannot declare "any number", and removing one renumbers the rest.
- **A `list` case in `input-value`**: WIT has no recursive variants.
- **A separate face-and-edge selection record** beside `inputs`: a second
  reference path the DAG and the resolver would both have to learn.
- **Edges checked against a named body input by the resolver**: needs the
  resolver to know which input an edge belongs to; the kernel already
  refuses a name not on the body.
