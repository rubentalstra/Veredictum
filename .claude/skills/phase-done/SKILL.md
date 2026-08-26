---
name: phase-done
description: >
  Closes a tracker issue: verifies the acceptance criteria are genuinely met,
  checks the native relationship edges, confirms the changelog entry, writes
  the close narrative into the PR description, and posts the handoff comment.
  Use when the user says a work item is complete or asks to close it out.
allowed-tools: [Read, Edit, Grep, Glob, Bash]
argument-hint: "[issue number] (optional)"
---

# /phase-done

The closing step of the issue workflow (`CLAUDE.md`). Run it once the work is
actually finished — this skill verifies and records, it does not decide the work
is done on your behalf.

Ported from FerroEHR at the Veredictum split (FerroEHR#2789) and trimmed. The
gates it dropped are named in the last section rather than left as steps that
would pass vacuously: a verification step pointing at machinery that does not
exist is worse than no step, because it reports green.

## Steps

1. **Identify the issue being closed** — the user names it, or it is the issue
   this branch's pull request declares `Closes #N` for. Read it with
   `gh issue view <n> --comments`.

2. **Verify every `## Acceptance criteria` checkbox is genuinely ticked.** If any
   remain `- [ ]`, stop and list them. Do not tick a criterion to proceed: a tick
   means someone ran the thing and saw it pass. Tick verified boxes with
   `gh issue edit <n> --body-file`.

3. **Relationships check** (`scripts/gh/rel.sh tree <n>`;
   `.claude/rules/issue-relationships.md`). If the issue is a **parent with open
   sub-issues, do not close it** — finish or reparent the children first, because
   a parent's job is done only when its decomposition is. Closing this issue
   auto-unblocks anything it was `blocking`; note which dependents become
   workable so the handoff comment can point at them.

4. **Specification-adherence check.** For work that shipped spec-facing
   behaviour — an expectation, a citation, a verdict rule — confirm the governing
   released section was read first-hand and is cited, and that the citation
   resolves. Until the spec text is vendored here, the citation names the
   FerroEHR checkout it was read in. An expectation with no citation is not
   reviewable and is not closable.

5. **Changelog check.** If the work changed a user-visible surface — the CLI, the
   published artifact schemas, verdict semantics, the container image, or
   anything a party's published record depends on — confirm a `CHANGELOG.md`
   `[Unreleased]` entry exists in-branch. `scripts/checks/changelog-structure.sh`
   checks the file's shape in CI, not whether the entry is there, so this step is
   the one that catches a missing entry.

6. **Gates green.** Confirm the guard tier actually ran and passed on the work:
   `bash scripts/checks/comment-style.sh --all`,
   `bash scripts/checks/changelog-structure.sh`,
   `bash scripts/checks/ci-conclusion-complete.sh`, and — for any change under
   `.github/` — actionlint and zizmor. Report the real output; never claim a
   green you did not see.

7. **Write the close narrative into the pull-request description**: what shipped,
   the decisions with their specification citations, the gate results, and what
   was deliberately left out with its follow-up issue numbers. The pull-request
   description and the issue thread are the build record — there is no design-doc
   layer.

8. **Post the handoff comment on the issue** (`gh issue comment <n>`): where
   things stand at close, what was left out and where it is tracked, and what a
   follow-up session should do first.

9. **Ensure the pull-request body declares `Closes #<n>`** (`gh pr view` /
   `gh pr edit`) so the merge closes the issue. One `Closes` keyword per issue:
   `Closes #1, #2` closes only #1. Never close by hand when a pull request
   carries the work.

10. **Milestone check.** A closed issue's milestone moves a release toward its
    cut, and a release is cut when its milestone reaches zero open issues. If
    this close empties a milestone, say so.

## What this skill does not do

It does not run the gates for you to "check" the criteria. Those must already
have been run and genuinely passed.

## Gates that arrive with the code, and are therefore not steps here

Named so their absence is a record rather than an omission
([FerroEHR#2789](https://github.com/rubentalstra/FerroEHR/issues/2789)): the
Rust gates (`cargo fmt`, `clippy`, `nextest`, `deny`), the catalogue's own
`validate` gate, the conformance zero-drift comparison against a committed
baseline, the documentation-site page requirement, and the plan-file
delete-on-implementation step. Each becomes a step in the pull request that
brings the machinery it checks.
