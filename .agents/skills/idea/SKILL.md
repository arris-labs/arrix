---
name: idea
description: Turns a raw feature idea, design question or backlog line into a brainstorm under docs/ideas/ (problem, options and trade-offs, conflicts, core or plugin, a recommendation, the decision for the human). Use when the human says "idea:", "what if", "should we", "think about", hands over a backlog line, or asks for a plan too vague or contested to plan directly. Analysis only, no code and no plan.
argument-hint: <idea in one sentence, or a backlog line>
---

# /idea — brainstorm, don't plan

An idea is where thinking happens so a plan doesn't have to. A clear "no,
because…" is a successful outcome.

1. Pick a `kebab-case` noun-phrase slug. If `docs/ideas/<slug>.md` exists,
   extend it.
2. Read what constrains the idea before forming an opinion: `SEED.md`
   (non-goals, §6.2 core vs plugin, §6.3 plugin model, §6.6 the kernel's
   sensor, §9 kickoff decisions), the design docs and ADRs it touches,
   `docs/ROADMAP.md` for its cycle, and the code if it exists. Answer a
   kernel question from Arris's API, corpus and roadmap (the checkout is in
   `docs/notes/LOCAL.md`). Cite paths and ADR numbers; restate anything
   taken from `docs/notes/` rather than citing it.
3. Write `docs/ideas/<slug>.md` from `docs/ideas/TEMPLATE.md`:
   - real options with honest trade-offs, "do nothing" among them;
   - cost in plan steps, not hours;
   - what it conflicts with, by name: a `SEED.md` non-goal, an ADR, a layer
     rule, plugin-API stability;
   - core or plugin, and why (a plugin that needs a new contribution point
     or API type has a core part);
   - any kernel capability Arris lacks, as an Arris dependency the idea may
     recommend asking for, never a workaround;
   - if it sits in no cycle's in-list, the product scenario its ADR would
     have to name.
   Two pages is long; write what the decision needs.
4. End with one recommendation and the decisions the human must make, as
   questions with your preferred answer first.
5. If the idea came from `docs/BACKLOG.md`, remove that line.
6. Reply with the recommendation and the decisions. The file is the record,
   the reply is the conversation.

The idea stays analysis: steps, checkboxes and file lists belong to
`/plan`; an ADR is proposed here and written by the plan; code and design
docs are left untouched.

`$ARGUMENTS`
