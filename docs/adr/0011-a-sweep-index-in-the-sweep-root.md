# ADR-0011: A sweep index in the sweep root

- Status: Accepted
- Date: 2026-10-06

## Context

ADR-0010 made piece `0` of a split keep its origin's name. Until then a
feature that swept twice and fused the results (`plans/c1-m1-slice` step 4's
backlog line) got away with it: both sweeps' end caps were
`face:sweep.<F>.end-cap`, and only the `mod.<F>.0` on the one that was holed
told them apart. With the rule of ADR-0010 they are the same text, and the
boolean fails as `kernel.naming` ("names both f4 and f6";
`crates/arrix-plugin-host/tests/tier0.rs`, `demo.stack`). ADR-0010 listed
this as not helped; step 4 of `plans/c1-m2-naming` showed it is a
regression of a case that worked. Product scenario: a plugin feature that
builds two profiles (a stepped shaft, a boss on a disc) and fuses them keeps
every name unique and stable.

## Decision

1. **`NameRoot::Sweep` gains `index: u32`**, the place of the sweep among
   the sweeps (extrudes now, revolves with ADR-0010's root) one feature's
   evaluation made, from `0` in call order.
2. **Text.** Index `0` is not written, so every name stored so far is
   unchanged: `sweep.<ID>.end-cap`. Index `N > 0` is written
   `sweep.<ID>.<N>.end-cap`, `N` a decimal `u32` without leading zeros.
   The grammar gains `root := 'sweep.' ID ('.' N)? '.' part`; a part is a
   word, so a digit after the feature's id is unambiguous. A written `0` is
   a parse error.
3. **Who counts.** The host's per-feature kernel handle counts the sweeps
   its feature makes in an evaluation and hands the count to
   `arrix-kernel`'s `extrude` (and later `revolve`). The plugin API is
   unchanged: a plugin does not see the index.
4. **Stability.** Names are stable while the feature's sweeps are made in the
   same order; a feature that adds a sweep before another renames the later
   ones. That is the feature author's order, and `core.extrude`, which
   sweeps once (twice for a boolean's tool, the tool second), is unaffected.

## Consequences

- No stored name changes, no schema bump.
- A feature that sweeps twice may fuse or cut the results.
- The `kernel.naming` guard stays for what this does not meet.

## Alternatives considered

- **Rework the test plugin** so no feature sweeps twice: hides the case,
  and a plugin author hits it again.
- **Key each sweep's caps by the caller** (an argument of `extrude`): puts
  naming into the plugin API; the count is mechanical.
- **Keep `mod.<F>.0`**: undoes ADR-0010's scenario.
