---
name: next-task
description: >
  Reads the tracker (GitHub Issues), picks the pinned or named open issue
  while respecting native blocked-by and parent edges, and restates it as a
  concrete in-session work plan. Use when the user asks "what's next" or
  "what should I work on".
allowed-tools: [Read, Grep, Glob, Bash]
argument-hint: "[issue number] (optional)"
---

# /next-task

Turns an open tracker issue into an actionable plan — the planning step of the
issue workflow (`CLAUDE.md`). Does not do the work; that is a separate step the
caller takes after seeing the plan.

Ported from FerroEHR at the Veredictum split (FerroEHR#2789) and trimmed to what
this repository actually has. What was dropped, so nobody looks for it: the
crate-map and code-generator routing (there is no Rust code here yet), the
vendored-spec-section step (the spec text arrives with the migration), the
conformance-gate definition of done (no catalogue here yet), and the board move
(deferred). Step 3 below says where each of those lands instead.

## Steps

1. **Read the tracker.** `gh issue list --state open` — the SessionStart dump
   already annotates each issue with `{k/n}` sub-issue progress, `child-of
   #parent`, and open `BLOCKED-by` / `blocks` edges. Take the pinned issue, or
   the issue the user named.

   **Respect relationships** (`.claude/rules/issue-relationships.md`): do NOT
   pick an issue shown `BLOCKED-by` an open issue — surface its blocker as the
   real next task. For a parent issue, point at its next open child rather than
   the parent. Then `gh issue view <n> --comments` for the full contract (the
   opening summary plus `## Acceptance criteria`) and the running discussion,
   and `scripts/gh/rel.sh tree <n>` for its edges.

2. **Turn the task into a plan**, stating:
   - **What** the task requires, in one or two sentences.
   - **Which files** are involved. Search for them rather than guessing paths.
     The planned layout after the split is the table in `CLAUDE.md` §
     Repository layout; anything marked "arrives with FerroEHR#2789" is not
     here yet, so a task naming it is a task for the FerroEHR checkout.
   - **Where the work belongs.** This is the decision that matters most right
     now: **a change to runner behaviour, the catalogue, the schemas or the
     register lands in FerroEHR `tools/cnf-runner`** until the extraction
     completes. Do not re-implement it here. What belongs here is the product's
     identity, its discipline, its CI and release machinery, its documentation,
     and the tracker.
   - **Which released specification sections govern it**, for anything
     spec-facing. The spec text is not vendored here yet: read it first-hand in
     a FerroEHR checkout under `docs/specs/openehr/` and say in the plan which
     checkout was read. Never answer from memory, from a vendor's
     documentation, or from what a server did (`.claude/rules/cnf-triage.md`).
   - **What "done" looks like** for this task: the issue's
     `## Acceptance criteria` checklist, plus what proves each item. Today the
     available proof is the CI guard tier (`bash scripts/checks/*.sh`,
     actionlint, zizmor, reuse lint); the catalogue's own `validate` gate and
     the Rust gates arrive with the code.

3. **Note whether the task suits a subagent.** Bounded, well-specified work with
   the governing material named up front fans out (`.claude/agents/`); the
   adjudication-heavy work — attributing a red row, deciding what a
   specification section requires, designing the catalogue's shape — stays
   in-session.

4. **Do not edit the issue or commit.** Recording progress happens after the
   work is done, not as part of planning it.
