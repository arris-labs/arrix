# ADR-0003: The home is the `arris-labs` organisation

- Status: Accepted
- Date: 2026-09-26
- Supersedes: ADR-0002

## Context

ADR-0002 started ArriX on a personal account and left the organisation's
name open, because `arris-cad` may be too narrow once plugins take the
family past CAD (CAE, BIM). The name was wanted as a brand, without echoing
an existing product: `arriswork(s)` recalls SolidWorks, and `arris-forge`
recalls Autodesk Forge. `arris`, `arrix` and `arrislabs` are taken on
GitHub.

## Decision

The family lives in the GitHub organisation `arris-labs`: `arris-labs/arris`
(the kernel), `arris-labs/arrix` (the app), and the first-party plugins'
repositories once they leave the tree at C3. Both existing repositories
were transferred there on 2026-09-26.

## Consequences

- GitHub redirects the old `Divelix/arris` and `Divelix/arrix` URLs,
  including the links in already-published crates' and the PyPI
  placeholder's metadata. No new repository may be created at either old
  path, or its redirect breaks.
- Each crate's `repository` field moves to `arris-labs` at its next
  release. A trusted publisher registered on crates.io or PyPI names the
  repository owner, so any registered under `Divelix` is re-registered
  under `arris-labs` before the next release from CI.

## Alternatives considered

- **`arris-cad`**: too narrow for a family that grows past CAD.
- **`arris-commons`, `arris-guild`, `arris-studio`, `arris-foundry`**:
  respectively institutional, playful, single-application, and already a
  known product name elsewhere.
- **`arris-rs`**: suggests Rust only, while the Python SDK and plugins live
  there too.
- **Stay on the personal account**: ties the project to one account, with
  no roles for co-maintainers.
