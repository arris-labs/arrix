# ADR-0021: Tier 2 builds by callback, and is trusted, not sandboxed

- Status: Accepted
- Date: 2026-10-09
- Amends: `SEED.md` §8.4

## Context

The boundary-opened cycle (ADR-0016) puts a Python feature plugin into a
real history over a process boundary. A Python process has two ways to make
geometry. It can call back into the host's kernel through the WIT world's
`kernel` imports while `evaluate` runs. Or it can run Arris itself through
Arris's Python binding (A10) and return bodies as bytes, as
`docs/PLUGINS.md` §Three tiers and the roadmap's first draft of the
section assumed.

The second breaks `SEED.md` §6.6 for exactly the plugins an agent writes
first: the plugin's operations happen outside `arrix-kernel`, so they are
not recorded, a failure leaves no replayable recipe, and the host cannot
name a body it did not build (Arris's provenance would have to cross the
boundary too). The first costs a round trip per kernel call, sub-millisecond
on a pipe against operations of 10 to 100 ms, and needs nothing compiled in
the plugin's process.

Tier 2 is also the user's own Python, with the user's files and network.
`docs/PLUGINS.md` §Capabilities calls Tiers 0 and 2 trusted with declarative
capabilities, but does not say which of them a Tier 2 host can enforce.

## Decision

1. **A Tier 2 feature builds geometry only through the host's kernel**, by
   the `kernel` imports called back over the wire during `evaluate`. A body
   crosses as the plugin API's `u32` handle, valid for that evaluation and
   freed when it returns; body bytes never cross for a feature's output.
   Its kernel calls are recorded, named and replayable exactly as a Tier 0
   plugin's are.
2. **Reading a body for analysis** (its bytes or a mesh, handed to a plugin
   that computes on it, as `SEED.md` §5's FEA example does) is allowed by
   this decision and built when a scenario asks for it, as a WIT addition
   with its semver. **Returning a body built outside the host is refused**;
   reopening it needs an ADR that answers how such a body is named and
   recorded.
3. **A Tier 2 plugin is trusted, not sandboxed.** The host enforces the
   manifest's `kernel`, `document-read` and `document-write` at the wire:
   a call outside them is refused with a `plugin.capability` diagnostic.
   `files` and `network` are declared and shown, not enforced: the process
   is the user's own Python. A feature's `evaluate` gets the kernel and its
   resolved inputs only, whatever the manifest says (§Capabilities).
4. **The Python SDK is pure stdlib**: generated stubs and a thin runtime on
   the wire. It does not depend on `arris`; a plugin may import whatever it
   likes.
5. **`SEED.md` §8.4 is amended**: the four asks that blocked C1 are
   history, and Arris's Python binding is not needed by the boundary cycle.

## Consequences

- A Python feature is a first-class history entry by the same evidence as
  the gear: call records, names from provenance, recipes on failure,
  frozen results.
- The SDK installs anywhere Python 3 runs, with no wheel to build, and its
  kernel follows ArriX's pinned `arris` without a second version to match.
- A plugin that wants its own numerics on a body waits for decision 2's
  read half; one that wants to build geometry the kernel facade cannot
  express asks for a facade operation, or for the ADR decision 2 names.
- A malicious Tier 2 plugin can do anything the user can. The manifest's
  `files` and `network` lines inform the user; Tier 1 (C3) is the sandbox.
- Every kernel call of a Python feature is a wire round trip. ADR-0022's
  spike measures whether that, not the kernel, is ever the cost.

## Alternatives considered

- **Arris in the plugin process, bodies out as bytes** (A10): fast and free
  of round trips, but unrecorded, unnamed and not replayable (§6.6).
- **Both, the plugin's choice per call**: two naming regimes in one
  history, and the unrecorded one is the easy one.
- **Enforcing `files` and `network` at Tier 2** (seccomp, a namespace, a
  Python audit hook): platform-specific, bypassable from Python, and
  outside a CAD's competence; Tier 1 is where enforcement is real.
