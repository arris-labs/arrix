# ADR-0028: The exchange unit is metres, written as metres

- Status: Accepted
- Date: 2026-10-11

## Context

ArriX's model is SI, in metres (`SEED.md` §6.5). Arris is unit-agnostic by
design: the consumer picks its unit through `Precision`. Only two of its
STEP defaults assume millimetres: the writer declares `SI_UNIT(.MILLI.,
.METRE.)` whatever the model holds, and `ReadOptions::default()` reads as
millimetres. So every STEP `arrix export` wrote through C1 read a thousand
times too small in another CAD. The reader already takes a `length_unit`
(`Metre` included); the writer does not. Arris's `ops::transform` is rigid
only, so ArriX cannot scale a body itself, and a workaround is ruled out
anyway (`SEED.md` §8.3).

## Decision

1. **Each side keeps its convention**: Arris unit-agnostic, ArriX in
   metres. Neither changes for the other.
2. **STEP is read with `length_unit: Metre` passed explicitly**, never
   through the default; the reader converts whatever unit a file declares.
3. **STEP is written in the model's metres, declared as metres**, once
   Arris's writer takes a unit (ask A12, filed 2026-10-11): no scaling, so
   ArriX's own round trip is bit-exact and every reader sees
   `SI_UNIT($,.METRE.)`.
4. **Until A12 lands, `export_step` is refused** as `kernel.export.unit`,
   the call recorded and the message naming the ask. STL and OBJ, which
   carry no unit, are unaffected. Fail soft, never silently: no file is
   better than a file a thousand times wrong.
5. A "write in millimetres" option waits for a reader that needs it.

## Consequences

- Import is not blocked: the reader converts. Export of STEP is, and
  C2's M3 round trip waits on A12.
- `arrix export --format step` exits 2 and the app's `Export…` of a `.step`
  shows the refusal in the status line, until then. Callers that relied on
  STEP in C1 (its accept, the kernel's read-back helper) test STL and OBJ.
- No inexact scaling anywhere in ArriX.

## Alternatives considered

- **ArriX in millimetres.** Changes `SEED.md` §6.5 and §8.2 rule 5,
  migrates every document and golden, mixes units (kg·mm² inertia), and
  robotics and Python want SI.
- **Arris committed to one unit.** Unit-agnostic is right for a library with
  many consumers.
- **Write millimetres.** Needs the same Arris ask and scales every
  coordinate by 1000 and back, inexact in floating point, for no reader that
  needs it.
- **Keep writing the wrong file meanwhile.** Silent corruption of a user's
  exchange.
