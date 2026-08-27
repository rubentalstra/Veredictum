---
name: phase-status
description: >
  Prints the tracker's open issues (pinned first, with native relationship
  edges) and a short git status. Use when the user asks "where are we",
  "what's in flight", or at the start of a work session to orient.
allowed-tools: [Read, Bash]
argument-hint: (none)
---

# /phase-status

A fast orientation dump — step 1 of the issue workflow (`CLAUDE.md`).
Read-only; makes no changes. The live state below is injected at invocation
time — ground the answer in it, not in stale conversation memory.

Ported from FerroEHR at the Veredictum split (FerroEHR#2789) and trimmed: the
plan-file step is gone, because this repository has no `docs/plans/` tree. The
roadmap-board step, deferred at the split, is back since #1 landed the board.

## Live state (injected)

### The tracker — open GitHub issues (with native relationships)

Each line carries `{k/n}` sub-issue progress, `child-of #parent`, and open
`BLOCKED-by` / `blocks` edges (`.claude/rules/issue-relationships.md`).

```!
gh api graphql -f query='query($owner: String!, $name: String!) { repository(owner: $owner, name: $name) { issues(first: 100, states: OPEN, orderBy: {field: CREATED_AT, direction: DESC}) { nodes { number title labels(first: 20) { nodes { name } } milestone { title } parent { number } subIssuesSummary { total completed } blockedBy(first: 30) { nodes { number state } } blocking(first: 30) { nodes { number state } } } } } }' -f owner="$(gh repo view --json owner --jq .owner.login)" -f name="$(gh repo view --json name --jq .name)" --jq '.data.repository.issues.nodes[] | ([.labels.nodes[].name] | join(", ")) as $labels | ([.blockedBy.nodes[] | select(.state == "OPEN") | "#\(.number)"]) as $b | ([.blocking.nodes[] | select(.state == "OPEN") | "#\(.number)"]) as $k | "#\(.number)  \(.title)  [\($labels)]" + (if .milestone then "  (\(.milestone.title))" else "" end) + (if .parent then "  child-of #\(.parent.number)" else "" end) + (if .subIssuesSummary.total > 0 then "  {\(.subIssuesSummary.completed)/\(.subIssuesSummary.total)}" else "" end) + (if ($b | length) > 0 then "  BLOCKED-by \($b | join(","))" else "" end) + (if ($k | length) > 0 then "  blocks \($k | join(","))" else "" end)'
```

### Milestones (releases)

```!
gh api "repos/$(gh repo view --json nameWithOwner --jq .nameWithOwner)/milestones?state=open" --jq '.[] | "\(.title)  open \(.open_issues), closed \(.closed_issues)" + (if .due_on then "  due \(.due_on[0:10])" else "  no due date" end)'
```

### Git

```!
cd "${CLAUDE_PROJECT_DIR}" && git status --short --branch | head -40 && echo "---" && git log --oneline -5
```

## Steps

1. Summarize the tracker state: which issue is the current focus (pinned, or
   the one this branch implements) and what its stated next action is. For an
   in-flight issue, run `gh issue view <n> --comments` and report the latest
   status comment and the unchecked `## Acceptance criteria` boxes, plus
   `scripts/gh/rel.sh tree <n>` for its parent, children and blockers.
   **Flag any open issue shown `BLOCKED-by` an open blocker** — it is stuck
   until the blocker closes and is not a pickup candidate.
2. Report which milestone is closest to its cut (zero open issues cuts a
   release) and flag a milestone whose due date has passed with open issues.
3. Summarize the git state from the injected output: current branch, uncommitted
   files, last commits. Flag uncommitted work that looks finished — finished work
   is never left sitting unmerged.
4. If the user asks how things look publicly, `scripts/gh/project.sh board`
   prints the public roadmap board grouped by Status — a presentation VIEW over
   the same issues, never a second source of truth
   (`.claude/rules/project-board.md`); the issue list above stays the working
   ground truth. Flag any `In Progress` item nobody is actually working on (it
   should be parked back to `todo`).

5. **Do not** modify an issue or make a commit. This is a read-only check. If the
   user wants the next task turned into a work plan, point at `/next-task`.

## What this skill deliberately does not report

Plan-file progress (no `docs/plans/` tree here). The roadmap board is reported
only on request (step 4), never as part of the default dump.
