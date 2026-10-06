# ADR-0014: A boolean number in the step

- Status: Accepted
- Date: 2026-10-06

## Context

ADR-0010 made piece `0` of a split keep its origin's name and gave a piece
`k > 0` the step `mod.<F>.<k>`. `plans/c1-m2-features` step 2 showed what
that does when one feature makes two booleans on one target: the second cut
splits piece `0` of what the first split and names its piece `1`
`mod.<F>.1` a second time, so the result is refused as `kernel.naming`
(`crates/arrix-kernel/tests/probe_m2_features.rs`). Under two features, or
with the tools fused first and cut once, the names are clean. It concerns
`core.boolean` with several tools (`common` cannot fuse its tools first), a
pattern or a mirror of features whose sources join and cut (ADR-0013, each
source applied with its own mode), and a tool-side combination of three or
more overlapping tools. Product scenario: a pattern of a pocket and a boss,
the pocket's copies cut and the boss's fused, one feature, one target.

## Decision

1. **`NameStep::Modified` and `NameStep::Generated` gain `boolean: u32`**,
   the place of the call among the booleans (and blends) one feature's
   evaluation made, from `0` in call order. It is ADR-0011's answer for
   sweeps.
2. **Text.** Boolean `0` is not written, so every name stored so far is
   unchanged. `mod.<ID>.<k>` stays; boolean `N > 0` is
   `mod.<ID>.<N>.<k>`. `gen.<ID>[…]` stays; boolean `N > 0` is
   `gen.<ID>.<N>[…]`. The grammar is `step := 'mod.' ID ('.' N)? '.' N |
   'inst.' ID '.' N | 'gen.' ID ('.' N)? '[' … ']'`: one number after a
   `mod`'s id is the split, two are the boolean and the split. A written
   boolean `0` is a parse error. `inst` steps do not change: a copy's index
   is already its own number (ADR-0010).
3. **Who counts.** The host's per-feature kernel handle counts the booleans
   its feature makes (`fuse`, `cut`, `common`, `fillet`, `chamfer`) in an
   evaluation and hands the count to `arrix-kernel`, as it does for sweeps.
   The `Kernel` methods take it as an argument, and a call's record holds
   it. Plugin API 0.4 (step 5) offers the same operations; whether a
   plugin sees the number is that step's, and the default is as for sweeps:
   it does not.
4. **Stability.** Names are stable while the feature's booleans are made in
   the same order; a tool inserted before another renames the later
   booleans' pieces. That is the trade ADR-0011 made for sweeps and the
   plan made for a hole's points.

## Consequences

- No stored name changes, no schema bump; a record read from before has
  `boolean` 0 (`#[serde(default)]`).
- A feature may make any number of booleans on one target.
- The `kernel.naming` guard stays for what this does not meet.

## Alternatives considered

- **One boolean per target per feature** (combine the tools first, refuse a
  pattern whose sources' modes differ): no grammar change, but it restricts
  ADR-0013, and combining three overlapping tools can still split one
  entity twice.
- **Continue the split number `k` across a feature's calls**: `k` would
  depend on how many pieces earlier calls made, so an upstream edit would
  rename pieces.
