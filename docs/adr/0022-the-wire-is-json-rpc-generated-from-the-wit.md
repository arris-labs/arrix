# ADR-0022: The wire is JSON-RPC, newline-framed, generated from the WIT

- Status: Accepted
- Date: 2026-10-09

## Context

The boundary-opened cycle needs one wire for two uses: the Tier 2 host
talking to a plugin's process, and `arrix serve --stdio` talking to an
agent. `docs/PLUGINS.md` left the encoding ⚠ OPEN between JSON-RPC and a
binary encoding of the WIT types, on one constraint: the encoding is
derived from the WIT world, not written by hand beside it. ADR-0016 leaned
to JSON-RPC (an agent reads it, any language speaks it) and asked for size
and speed to be measured first.

The spike (`crates/arrix-cli/tests/wire_spike.rs`, run by name in a release
build) measured, on the reference machine, 2026-10-09:

| The bracket's mesh, 5,136 triangles over both meshes | bytes | Rust encode | Rust decode | Python parse |
|---|---|---|---|---|
| coarse, JSON as serde gives it | 418,675 | 0.65 ms | 0.79 ms | 5.25 ms |
| coarse, JSON, bulk arrays as base64 | 313,008 | 0.42 ms | 0.38 ms | 0.89 ms |
| coarse, CBOR | 264,371 | 0.51 ms | 1.13 ms | no stdlib CBOR |
| refined, JSON | 489,143 | 0.75 ms | 0.91 ms | 6.42 ms |
| refined, JSON, base64 bulk | 355,414 | 0.44 ms | 0.39 ms | 0.99 ms |
| refined, CBOR | 299,261 | 0.64 ms | 1.33 ms | no stdlib CBOR |

A Tier 2 `feature.evaluate` making ten `kernel.extrude` callbacks to a
Python child over a pipe: 1,000 runs, median 0.23 ms, 21 µs a message pair.
A kernel operation is 10 to 100 ms (`--test c1_benchmark`), so the pipe is
under 0.3 % of an evaluation that calls the kernel at all.

## Decision

1. **JSON-RPC 2.0, one JSON object per line**, UTF-8, both directions, on
   the child's stdin and stdout (Tier 2) or the server's (`serve`). No
   `Content-Length` headers: JSON never holds a raw newline, and a line is
   what an agent at a shell reads. A reader caps a line's length and
   answers a longer one with a protocol error.
2. **Either side may request while a request of the other is open.** A
   Tier 2 plugin calls `kernel.*` while answering `feature.evaluate`, and
   the host answers each before its own request's result arrives. The host
   never calls into the plugin while serving its call, so the nesting is
   one deep.
3. **Method names and shapes are the WIT world's**, generated from it:
   `<interface>.<function>` (`feature.evaluate`, `kernel.extrude`), params
   by the WIT names as an object, kebab-case kept. A record is an object; a
   variant a one-key object `{"<case>": payload}` (a case with no payload,
   `{"<case>": null}`); an enum its case's name; an option `null` or the
   value; a `result` the object `{"ok": …}` or `{"err": …}` inside the
   JSON-RPC result; a tuple an array; a resource its handle, a number;
   `types.id` ArriX's 13-character id text, as the document writes it;
   other integers JSON numbers, refused past ±2^53 by the codec.
4. **JSON-RPC's `error` is for the protocol only** (malformed line, unknown
   method, bad params, a version mismatch): a feature's or a kernel call's
   failure is a `diagnostic` in a `result`'s `err`.
5. **Bulk numbers are base64**: a run of `f32` or `u32` (a mesh's
   positions, normals, triangles and edge points) is the base64 of its
   little-endian elements, under a field whose schema names the element
   type. It crosses only on `serve` in this cycle; the plugin world has
   none yet.
6. **No binary encoding.** CBOR is 15 % smaller than JSON with base64 bulk
   and no faster to decode in Rust, and Python has no CBOR in its standard
   library, which the pure-stdlib SDK (ADR-0021) would then have to carry.
   A binary framing for bulk alone is the answer if a later measurement
   shows the pipe, not the kernel or the renderer, as the cost.
7. **The first message is a handshake** (`initialize`, from the host or
   the agent): the protocol's version and, for a plugin, the plugin API's,
   checked against the manifest's `api` range, so a mismatch is one
   diagnostic before any feature is asked anything.

## Consequences

- An agent can drive `serve` with `python3 -c`, `jq` or a line of shell,
  and read a plugin's traffic in a log as it is.
- The shapes are generated (`docs/plans/boundary-m1.md` steps 3 and 4), so
  a WIT change is a regenerated wire with a semver, never a hand edit in
  three languages.
- A mesh reads in Python six times faster as base64 than as JSON numbers,
  at three quarters of the size; the session's own serde form (numbers) is
  unchanged in-process, so `serve`'s codec owns the bulk form.
- The spike stays as a test run by name, so the numbers can be measured
  again against a later encoding.

## Alternatives considered

- **CBOR throughout**: smaller, opaque to an agent and to `cat`, not in
  Python's standard library, and not faster to decode on the bracket.
- **The component model's canonical ABI**: made for linear memory, not a
  pipe; nothing outside wasmtime reads it.
- **LSP's `Content-Length` framing**: robust to raw newlines JSON never
  has, and awkward at a shell.
- **Plain JSON numbers for meshes**: 34 % larger and six times slower for
  a Python reader, for no gain in legibility on a 400 kB line.
