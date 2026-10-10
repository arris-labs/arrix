# ADR-0026: The harness is a versioned public interface

- Status: Accepted
- Date: 2026-10-10

## Context

ADR-0016 constraint 5 makes the harness product: an agent reads the eval
line, the render's JSON, the test kit's report and the session protocol,
and builds on them. Until now each was whatever the code printed.
ADR-0025 left the render JSON's shape for M3, and `serve` already prints
its schemas (ADR-0023). An agent that parses these needs to know what will
not change under it, and how to tell when something has.

## Decision

1. **Four surfaces, each with a version a reader can find.**
   - The eval line, by its `format`.
   - The render JSON, `{format, adapter, debug_state}`, by its `format`.
   - The kit's report (`arrix test`), one line per check, by its `format`.
   - The session protocol, by `protocol` in `initialize`.

   Each starts at 1.
2. **Each surface's JSON Schema is printed by its own command** (`arrix
   eval --schema`, `arrix-app render --schema`, `arrix test --schema`,
   `arrix serve --schema`) and held by a golden test, refreshed under
   `UPDATE_SNAPSHOTS=1` and read before it is.
3. **Inside a version, changes are additive only**: a new optional field,
   or a new enum value that readers must tolerate. A removal, a rename or
   a change of meaning bumps the version. The commit body says so
   (`Harness: <surface> format N, …`), as the API's semver line does
   (`.agents/rules/git.md`).
4. **`debug_state()` is promised on the fields its schema marks**
   (`x-arrix-promised`): bodies, camera, badges, evaluation and
   diagnostics. Everything else in it is informational and may change
   without a version bump.
5. **The render's pixels stay as ADR-0025 promises them**, on lavapipe
   only. This ADR versions the JSON, not the picture.

## Consequences

- An agent can pin a `format` and fail loudly on a mismatch, instead of
  guessing from a missing key.
- Every harness change is argued twice: once as code, once as a version
  decision, and the golden diff shows which.
- The kit's report and the eval line cannot be reshaped for convenience;
  a better shape is a new version.

## Alternatives considered

- **No versions, schemas only.** A schema says what is valid now, not
  what the reader may rely on later.
- **One version for the whole harness.** The surfaces change at different
  rates, and a bump on one would falsely warn readers of the others.
- **Promising all of `debug_state()`.** It mirrors the app's internals,
  and promising it would freeze the UI's structure.
