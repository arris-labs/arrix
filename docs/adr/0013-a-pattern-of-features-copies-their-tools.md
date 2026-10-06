# ADR-0013: A pattern or a mirror of features copies their tools

- Status: Accepted
- Date: 2026-10-06

## Context

`docs/DATA-MODEL.md` §The core feature types gives `core.pattern-linear`,
`core.pattern-circular` and `core.mirror` "features or a body" as what
they copy. A body is plain: it is a slot. "Features" is not: a feature's
output is the body it modified, not the material it added or removed,
and a feature type's `evaluate` has no frame to be re-run at. M2's accept
patterns a counterbored hole and raises and lowers its count
(`docs/ROADMAP.md` §M2), and `plans/c1-m2-naming` proved the kernel shape
for it: copies of one tool, each named with `inst.<F>.<i>`, fused and
cut, so a count change renames no copy that stays
(`crates/arrix-kernel/tests/instances.rs`). Product scenario: one
counterbored hole patterned 3, then 5, then 2 times, a fillet on the
first hole's rim kept throughout.

## Decision

1. **A feature that joins or cuts keeps its tool.** A type's output may
   carry `tool: option<tool-output>`, the tool body it applied and the
   operation (`fuse` or `cut`). `core.extrude` and `core.revolve` in
   `join` and `cut` modes and `core.hole` return theirs; a plugin may.
   The evaluator holds it with the feature's outputs. It is not a slot:
   it is no part body, nothing reads it by `Ref::Slot`, and it is never
   consumed.
2. **It is read only through a `tool` input** (ADR-0012), by
   `Ref::Feature`. Its input hash is the source feature's.
3. **A pattern or a mirror of features** reads a list of `tool` inputs
   and a `target` body input that its one slot `modifies`. For each tool
   it makes the copies (`Kernel::copy` or `Kernel::mirror`, instance `i`
   from 1), fuses them into one tool, and applies it to the target with
   the source's own operation. The source feature's own cut or join is
   not repeated: instance 0 is the source's work.
4. **A pattern or a mirror of a body** reads a `body` input, copies it,
   and fuses the copies into it; its slot modifies that input.
5. **A frozen source** has no tool (a frozen result stores slots), so a
   pattern of it fails as `input.no-tool` naming it, and evaluates again
   once the source is live.

## Consequences

- One feature in the tree patterns one hole, as a user expects, and the
  names are the ones `c1-m2-naming` proved.
- The source's edits flow into the copies through the ordinary cache:
  its tool changes, its hash changes, the pattern re-evaluates.
- The evaluator keeps one more body per joining or cutting feature for
  as long as it keeps the feature's outputs; the memory cost is measured
  with the C1 benchmark.
- A frozen result does not store a tool, so a pattern of a plugin
  feature needs the plugin present. Storing tools in frozen results is a
  later decision if a scenario asks for it.

## Alternatives considered

- **Bodies only** (a hole tool made as a new body, patterned, then cut by
  `core.boolean`): three features where a user expects one, and the
  hole's form no longer says what it cuts.
- **Re-evaluating the source's type at each copy's frame**: `evaluate`
  has no frame argument, a sketch-based feature would need its sketch
  moved, and it costs `N` times the kernel calls.
- **The tool as a consumed slot**: a part body the user never made, and
  ADR-0007's consumption would end it at the source's own cut.
