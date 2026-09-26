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
