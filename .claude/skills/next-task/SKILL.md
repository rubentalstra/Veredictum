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
this repository has. What was dropped, so nobody looks for it: the
code-generator routing (nothing here is generated from a meta-model) and the
project-board move (deferred by owner decision).

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
   - **Which files** are involved. Search for them rather than guessing paths;
     the layout table in `CLAUDE.md` § Migration state names every tree.
   - **Which of the three the change touches** — the runner machinery under
     `app/veredictum/src/`, the catalogue under `artifacts/`, or the repository's own
     machinery. The attribution law turns on that distinction, so the plan
     states it before any code is written.
   - **Which released specification sections govern it**, for anything
     spec-facing. Read them first-hand in `specs/openehr/` and quote the
     sentence that assigns the value. An XSD, JSON-Schema or OpenAPI citation
     resolves against the bundles beside it. Never answer from memory, from a
     vendor's documentation, or from what a server did
     (`.claude/rules/cnf-triage.md`).
   - **What "done" looks like** for this task: the issue's
     `## Acceptance criteria` checklist, plus what proves each item. The
     available proof is the guard tier (`bash scripts/checks/*.sh`, actionlint,
     zizmor, reuse lint), the Rust tier (`cargo clippy --all-targets -- -D
     warnings`, `cargo nextest run`, `cargo deny check`) and
     `cargo run -- validate --root artifacts --specs specs/openehr` at zero
     findings.

3. **Note whether the task suits a subagent.** Bounded, well-specified work with
   the governing material named up front fans out (`.claude/agents/`); the
   adjudication-heavy work — attributing a red row, deciding what a
   specification section requires, designing the catalogue's shape — stays
   in-session.

4. **Do not edit the issue or commit.** Recording progress happens after the
   work is done, not as part of planning it.
