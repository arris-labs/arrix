# ADR-0002: The home starts on a personal account; the organisation is chosen later

- Status: Superseded by ADR-0003
- Date: 2026-09-26

## Context

`SEED.md` §2 and §9 ("Home") placed ArriX, Arris and the first-party
plugins in a GitHub organisation named `arris-cad`. No organisation exists
yet; Arris lives at `Divelix/arris`. The name `arris-cad` was questioned
before it was created: with plugins, the family can grow past CAD (CAE,
BIM), so a name that says "CAD" may be too narrow. The names `arris` and
`arrix` are taken on GitHub. The crates.io and PyPI placeholders need a
repository link that works now and keeps working.

## Decision

ArriX starts at `github.com/Divelix/arrix`, beside `Divelix/arris`. The
organisation, and its name, is a later decision with its own ADR. When it
exists, both repositories are transferred into it; GitHub's redirects keep
every old link, including those in published crates' metadata, working.
First-party plugins leave the tree at C3 into whatever home exists then.

## Consequences

- Nothing waits on choosing a name, and the placeholders link to a
  repository that exists.
- The transfer is a later chore: `repository` fields updated at the next
  release of each crate, remotes updated, and no new repository created
  at an old path (that would break its redirect).
- Until then the project's home depends on one personal account, which
  matters only once there are co-maintainers.

## Alternatives considered

- **Create `arris-cad` now**: its name may be too narrow for a family that
  grows past CAD, and renaming an organisation later is more disruptive
  than creating one.
- **Choose another organisation name now**: no candidate is settled yet,
  and nothing needs one before C3.
