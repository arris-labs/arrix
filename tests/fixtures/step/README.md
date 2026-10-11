# STEP fixtures

Files `arrix-kernel`'s import and anchor probes read
(`crates/arrix-kernel/tests/import.rs`, `tests/anchor.rs`).

- `servo.step`: a servo-like part in millimetres, a 40 × 20 × 36 body, a
  shaft of radius 3 standing 6 on its top at (10, 10) and a bore of radius
  2 through it at (32, 10). Written by Arris 0.6.0's `step::write` from the
  recipe in the probe's `servo_step`, which holds it to that recipe
  (`UPDATE_SNAPSHOTS=1` writes it again). Licence: this repository's.
- `half-ball.stp`: a half ball of radius 5 whose meridian edge runs through
  a pole, which the reader splits there. Copied from Arris's corpus at
  `v0.6.0` (`tests/fixtures/real/edge-through-sphere-pole/`), written by
  hand there and shrunk from a NIST test part. Licence: Arris's, MIT or
  Apache-2.0.
- `nurbs-cylinder.step`: a cylinder of radius 5 and height 10, Open
  CASCADE's STEP of it converted to B-splines, every face a NURBS: what an
  anchor's axis refuses. Copied from Arris's corpus at `v0.6.0`
  (`tests/fixtures/query/ray-nurbs-cylinder/`). Licence: Arris's, MIT or
  Apache-2.0.
