# ADR-0023: The session on the wire is its serde form

- Status: Accepted
- Date: 2026-10-10
- Amends: ADR-0022 §3, for `arrix serve` only

## Context

M2 of the boundary opened (ADR-0016) puts the `Session` protocol on stdio
as `arrix serve --stdio`, so an agent can drive a running session. ADR-0022
fixed the framing (JSON-RPC 2.0, one object per line, an `initialize`
handshake, `error` for protocol faults only, bulk numbers as base64). Its
§3 says that method names and shapes are the WIT world's, generated from
it. That fits the plugin world. The session protocol, though, is not in
the WIT world. `Request`, `Outcome`, `Rejected`, `SessionEvent` and
`Command` are serde types in `arrix-doc` (`docs/ARCHITECTURE.md` §The
protocol boundary), and a round-trip test serialises every one of them
through JSON on every run.

There were three ways to give `serve` its shapes:

- reuse the serde forms;
- write a `session` interface in WIT and generate from it, as the plugin
  wire does;
- wrap the whole `Request` enum in one method.

ADR-0016 constraint 3 also asks that the protocol describe itself.

## Decision

1. **`serve`'s shapes are the protocol's serde forms**, the same JSON the
   round-trip test already checks. There is one method per request, named
   after the `Session` trait:
   - `session.submit`, `session.undo`, `session.redo`, `session.preview`,
     `session.clear_preview`, `session.export` and `session.save`;
   - the plugin requests of ADR-0024.

   A method's params are its request's fields as an object. Its result is
   `{"ok": Outcome}` or `{"err": Rejected}` (ADR-0022 §4). `session.save`
   gives the directory form's files.
2. **Events are notifications.** Each `SessionEvent` is a `session.event`
   notification. The one client subscribes by its `initialize`, so `Opened`
   is the first notification. `serve` serves one client and one document,
   opened from a path or empty.
3. **A response precedes every event its request caused.** One writer
   thread owns stdout. It writes a request's response before any event
   queued while the request was served. Every event carries its generation
   or tag, so an event delayed past a later response is still placed.
4. **Bulk bytes are base64 at `serve`'s codec** (ADR-0022 §5): a
   `RenderMesh`'s arrays with their element type, `Opened`'s blobs and an
   export's bytes. The in-process serde form is unchanged.
5. **The protocol is described by JSON Schemas derived from its types**
   (`schemars`, behind a `schema` feature in `arrix-core`,
   `arrix-plugin-api` and `arrix-doc`). Where a derive disagrees with the
   serde form, that type's schema is written by hand. Every round-trip
   case is validated against the schemas. `arrix serve --schema` prints
   them, and a test keeps them fresh. The protocol is not configuration,
   so they are not under `arrix config schema`.
6. **The handshake names the session protocol's version.** It starts at
   1 and is bumped with any change to these shapes. This is separate from
   the plugin API's version, which governs the plugin world.
7. **ADR-0022 §3 stays the plugin world's rule.** For `serve`, this ADR
   replaces it.

## Consequences

- There is one definition of the protocol. A field added to a `Command`
  reaches `serve`, its schema and the round-trip test in one change, and
  never through a hand edit in another language.
- The document's commands stay out of plugin-API semver: the session
  protocol has its own version (point 6). The stability promise of
  ADR-0016 constraint 5 is M3's to attach to it.
- `Rejected` and `CommandError` become serialisable.
- `arrix-plugin-api` gains a feature-gated derive. API: minor.
- An agent reads the schemas, not ArriX's source, to write a
  `session.submit`.

## Alternatives considered

- **A `session` interface in WIT, generated like the plugin wire.** It
  would put `Command` either in two definitions or under plugin-API semver.
  That freezes the command set a cycle early and costs twice the steps.
- **One `session.request` method taking the `Request` enum.** It is the
  same data, harder for an agent to read and write
  (`{"submit": {…}}` inside a method called `request`), for no gain.
- **Schemas written by hand throughout.** They drift unless every type has
  examples, and the derive gives the schema of the code itself.
