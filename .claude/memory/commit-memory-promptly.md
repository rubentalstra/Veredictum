---
name: commit-memory-promptly
description: Owner ruling 2026-08-30 — memory files under .claude/memory and .claude/agent-memory never float; commit and push them with the work that produced them
metadata: 
  node_type: memory
  type: feedback
  originSessionId: f667ed44-03ec-4549-bba2-91ef4f127616
  modified: 2026-08-30T08:56:17.081Z
---

Owner ruling (2026-08-30, repeated twice in one session): memory files are
in-repo and MUST NOT be left floating in the working tree. Every session that
writes or updates `.claude/memory/**` or `.claude/agent-memory/**` (agents
write their own memory as a side effect) commits and pushes those files
promptly — ride them on the current work's PR branch, or a direct
`chore(memory): …` commit to main when no PR is in flight.

**Why:** floating memory is invisible to other sessions and dies with a
worktree or a checkout switch; the owner had to ask twice.

**How to apply:** before ending a work leg (and before every PR push), run
`git status --short | grep -E "memory"` and sweep what it shows into a commit.
