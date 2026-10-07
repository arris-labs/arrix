# Architecture Decision Records

Short, append-only records of decisions that shape the system. The design
docs describe the current design; ADRs record *why* it is that way and what
was rejected. The decisions taken at kickoff are in `SEED.md` §9 and need no
ADR; changing one does.

## Rules

- One decision per file: `NNNN-kebab-title.md`, numbers never reused.
- Statuses: `Accepted`, `Superseded by ADR-XXXX`, `Rejected` (a proposal
  that lost is worth recording too).
- Never edit an accepted ADR's substance; write a superseding one.
- An ADR is written when a decision is *made*, in the same change as the
  code or doc it affects.
- Work outside the current cycle's in-list gets an ADR naming the product
  scenario it unlocks *before* it starts.

## Template

```markdown
# ADR-NNNN: Title

- Status: Accepted
- Date: YYYY-MM-DD

## Context
What forces are at play; what problem demanded a decision.

## Decision
The decision, in one or two sentences, active voice.

## Consequences
What becomes easier, what becomes harder, what we're betting on.

## Alternatives considered
Each rejected option and the one-line reason it lost.
```

## Index

- [ADR-0001](0001-snapshot-goldens-are-text.md): snapshot goldens are text; images are rendered on demand
- [ADR-0002](0002-home-starts-on-a-personal-account.md): the home starts on a personal account; the organisation is chosen later (superseded)
- [ADR-0003](0003-the-home-is-arris-labs.md): the home is the `arris-labs` organisation
- [ADR-0004](0004-plugin-commands-undo-as-one-group.md): a plugin command's undo is its expansion, inverted by the core
- [ADR-0005](0005-the-client-solves-a-sketch-edit.md): the client solves a sketch edit; the authority applies it without solving
- [ADR-0006](0006-region-references-in-ref-and-the-plugin-api.md): a sketch region is a `Ref`, and a feature input; plugin API 0.2
- [ADR-0007](0007-bodies-as-inputs-and-modified-slots.md): bodies as feature inputs, and output slots that modify them; plugin API 0.3 (decision 5's borrow superseded)
- [ADR-0008](0008-a-body-input-is-an-owned-handle.md): a body input is an owned handle, not a borrow
- [ADR-0009](0009-frozen-results-are-evaluation-output.md): a frozen result is evaluation output kept by the session and written on save, not a command
- [ADR-0010](0010-the-single-piece-rule-instances-and-the-revolve-root.md): piece 0 of a split keeps its name; the Instance step and the revolve root
- [ADR-0011](0011-a-sweep-index-in-the-sweep-root.md): a sweep index in the sweep root, so a feature's two sweeps name their caps apart
- [ADR-0012](0012-list-inputs-and-the-edge-point-axis-and-tool-kinds.md): inputs that are lists, and the `edge`, `point`, `axis` and `tool` kinds; plugin API 0.4, schema 3
- [ADR-0013](0013-a-pattern-of-features-copies-their-tools.md): a feature that joins or cuts keeps its tool; a pattern or a mirror of features copies the tools
- [ADR-0014](0014-a-boolean-number-in-the-step.md): a feature numbers its booleans in a name's `mod` and `gen` steps, so several on one target name cleanly
- [ADR-0015](0015-the-shell-is-configured-by-user-files.md): the shell is data, configured by the user's TOML files, failing soft; config never reaches `evaluate`
- [ADR-0016](0016-the-agentic-user.md): the agentic user: invest in what agents can't rebuild, five constraints above it; the boundary opened before C2
- [ADR-0017](0017-the-minimal-shell.md): the minimal shell: two docked bars, tree and property cards over the viewport, document tabs, panel modes `auto`/`shown`/`hidden`
- [ADR-0018](0018-forms-are-the-feature-types-spec.md): a feature type's form is its spec; choices declared with `shown_when`, plugin API 0.5; a create form is a draft committed by OK as one command
