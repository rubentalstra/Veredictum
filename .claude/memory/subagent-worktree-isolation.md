---
name: subagent-worktree-isolation
description: Parallel implementation agents must get isolated worktrees — a shared checkout mixes authorship and loses commits
metadata: 
  node_type: memory
  type: feedback
  originSessionId: 32ee1aff-5e0d-4b1b-a080-d604f6efa1bb
  modified: 2026-08-27T16:30:13.245Z
---

Two implementer agents plus the orchestrator sharing one checkout collided
twice on 2026-08-27: branch switches under a working agent reset its ref
after push, one agent's PR opened over another's diff, and a commit survived
only because the object was still reachable. Both agents burned recovery
effort (`git branch -f`, `--force-with-lease`, re-cutting in a temp worktree).

**Why:** `git checkout` is process-global per worktree; every agent in the
same directory shares HEAD and the index.

**How to apply:** when launching an implementation agent that edits files,
pass `isolation: "worktree"` on the Agent call (and delete the worktree when
its PR merges — standing owner rule). For the orchestrator's own parallel
commits while an agent holds the checkout, use the temp-index plumbing
(`GIT_INDEX_FILE` + `hash-object` + `commit-tree`) — proven for the
[[roadmap-board]] and release-branch commits. Related: GraphQL `gh` calls
(pr create/merge/checks) hit a secondary rate limit when several agents run;
REST equivalents (`gh api repos/.../pulls`) keep working.
