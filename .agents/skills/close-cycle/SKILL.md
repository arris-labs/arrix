---
name: close-cycle
description: Close a finished cycle: verify, drift-review every design doc and the glossary, compress its roadmap section, propose the next, hand the tag to the human.
disable-model-invocation: true
argument-hint: <cycle, e.g. C1>
---

# /close-cycle — one roadmap, one section per cycle

`docs/ROADMAP.md` is a living design doc, not a log: a finished cycle
shrinks to its status line, and the next one is already below it.

1. **Check it is finished**: every in-list bullet done or moved to
   `docs/BACKLOG.md`; `docs/plans/` holds only `TEMPLATE.md`; the cycle's
   **Accept** run (the manual part named in the reply); the whole gate
   green (`docs/ARCHITECTURE.md` §Gates). Otherwise stop and report exactly
   what is open.
2. **Drift review**, the one place it is mandated. Read every design doc
   (`ARCHITECTURE`, `DATA-MODEL`, `PLUGINS`, `UI-RENDERING`,
   `CONCURRENCY-WASM`) and `GLOSSARY.md` against the code and fix each
   sentence no longer true; a term the docs now use and the glossary
   lacks gets its entry. Read `SEED.md` too, without editing it: a contradiction with the
   code goes to the human as a needed ADR. Bring *Arris dependencies* up to
   date (asks landed in a released `arris`, asks still open). List the
   fixes in the reply, or why you believe nothing drifted.
3. **Compress the finished section** to five parts, under ~40 lines:

   ```
   ## CN — <theme>
   *Goal: …*                        ≤ 3 lines
   **Status: done <date>, tag `cN`.** <the risk retired, the ADRs taken>
                                    ≤ 5 lines, one paragraph
   - <in bullets, as written when the section opened>
   **Out:** …
   **Accept:** …
   ```

   Before deleting a sentence, grep its fact in `docs/`, `crates/`,
   `README.md` and the workflows. A fact that lives only here moves to its
   home:

   | Fact | Home |
   |---|---|
   | crate layout, layer rule, kernel choke point, testing, a lint gate | `ARCHITECTURE.md` |
   | document type, schema version, naming, frozen-result or evaluation rule | `DATA-MODEL.md` |
   | contribution point, tier, manifest field, API version rule | `PLUGINS.md` |
   | render path, picking, panel, shortcut, snapshot-harness rule | `UI-RENDERING.md` |
   | executor, wasm, determinism, batch evaluation, performance budget | `CONCURRENCY-WASM.md` |
   | why a decision went the way it did | its ADR (a new one if none says it) |
   | something deferred | `BACKLOG.md`, one line |

4. **Propose the next section.** Re-read the next cycle's goal/in/out/accept
   against `docs/BACKLOG.md`, the ⚠ OPENs it touches, what this cycle
   changed and which Arris asks landed, and propose amendments. Its theme
   and in/out split are the human's commitment: ask when the answer is not
   obvious, and leave plans for `/idea` and `/plan` afterwards.
5. Update the roadmap's `Spine:` line, and `AGENTS.md` "Current state" (one
   sentence for the closed cycle, a new `**Next:**`, under ~15 lines).
6. Commit as `docs: close <cycle>`, the body listing the docs updated and
   the drift fixed. Then ask the human to tag `cN` on this commit; tags and
   pushes are theirs.

`$ARGUMENTS`
