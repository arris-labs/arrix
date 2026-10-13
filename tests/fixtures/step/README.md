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
- `arm.step`: an `arm` assembly of a bracket and a `mount`, itself a
  servo and one screw placed twice, each part coloured, in millimetres.
  Written by Arris 0.6.0's `step::write_products` from the recipe in
  `crates/arrix-kernel/tests/arm/mod.rs`, which `tests/structure.rs` holds
  it to (`UPDATE_SNAPSHOTS=1` writes it again). The structure tests and
  `core.import-step`'s `frame: product` read it. Licence: this
  repository's.
- `horn.step`, `screw.step`: the C2 accept's small parts, a 30 × 10 × 4 mm
  bar with a shaft of radius 3 standing 6 on its top at (5, 5), and a
  cylinder of radius 1.5 and height 8, in millimetres. Written by Arris
  0.8.0's `step::write` from the recipes in
  `crates/arrix-kernel/tests/exchange_fixtures.rs`, which holds them to it
  (`UPDATE_SNAPSHOTS=1` writes them again). Licence: this repository's.
- `nist-ftc-11.stp`: NIST's fully-toleranced test case 11, the AP203
  geometry-only edition (`nist_ftc_11_asme1_rb.stp`), one solid in
  millimetres: the accept's vendor part. Copied from Arris's corpus
  (`tests/fixtures/real/nist-ftc-11/`), sha256 `a5abfea460499dc31bc812aca68917948a117565fea4f5a080e36eb32f39650d`,
  from <https://www.nist.gov/document/nist-pmi-step-files>. Licence: NIST,
  "can be used without any restrictions".
