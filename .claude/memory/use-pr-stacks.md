---
name: use-pr-stacks
description: Owner ruling 2026-08-30 — use GitHub stacked PRs for dependent or batched changes; the serial changelog merge train is the disaster to avoid
metadata: 
  node_type: memory
  type: feedback
  originSessionId: f667ed44-03ec-4549-bba2-91ef4f127616
  modified: 2026-08-30T08:01:19.013Z
---

Owner ruling (2026-08-30, during the v0.1.2 merge train): use GitHub's native
stacked pull requests for future dependent or batched changes, instead of
independent PRs that all touch the CHANGELOG's Unreleased section and go DIRTY
against each other serially. The train that prompted this held five PRs behind
one slow CodeQL results check and forced the same changelog conflict to be
resolved five times.

**Why:** each layer of a stack shows its own diff, CI gates every layer, GitHub
does the cascading rebase, and merges run bottom-up — the changelog conflict is
resolved once (git rerere) instead of once per merge.

**How to apply:** before first real use, run the adoption gate in
[[stacked-prs-rule]] (`.claude/rules/stacked-prs.md`): verify the preview is
enabled for the repo (`gh stack` exit code 9 check), run one throwaway
two-layer stack end to end, record the outcome on a tracker issue. Then, for a
batch of small fixes or a dependent chain: `gh stack init` → `gh stack add -Am`
per layer → `gh stack submit` → `gh stack merge`. One issue per layer, each
with its own `Closes #<n>`, stays the law.
