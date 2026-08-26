---
name: contents-api-commits-unsigned
description: "Learned 2026-08-26 — the GitHub contents API creates UNSIGNED commits; use a local signed commit or the Git Data API"
metadata:
  node_type: memory
  type: feedback
  originSessionId: 32d068af-12e7-4654-9ece-124240b2367f
  modified: 2026-08-26T00:00:00.000Z
---

Learned 2026-08-26, first-hand: a commit written through the GitHub **contents
API** (`PUT /repos/{owner}/{repo}/contents/{path}`) lands **unsigned**. It
shows as unverified in the history, which fails the every-commit-is-verified
rule.

**Why it matters here:** an unverified commit in a conformance instrument's
history weakens exactly the provenance the product sells. The tool's whole
claim is that its own artifacts are re-checkable, and the same standard applies
to how they got into the repository.

**How to apply:**
- Local commits: `git commit -S` with `commit.gpgsign=true`. Verify with
  `git log --format='%G?'` and expect `G` on every line.
- Never seed or fix up a repository through the contents API, however
  convenient it looks for a one-file change.
- Workflow-created commits go through the **Git Data API** (blob, then tree,
  then commit, then ref update) with the workflow token, which GitHub signs as
  `github-actions[bot]`. Never `git commit` and push from a workflow.
