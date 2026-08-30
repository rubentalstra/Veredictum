#!/usr/bin/env bash
# SPDX-FileCopyrightText: Veredictum contributors
# SPDX-License-Identifier: Apache-2.0
# .claude/hooks/inject_phase_context.sh
#
# Ported from FerroEHR at the Veredictum split (FerroEHR#2789); the tracker dump
# is unchanged, and the oracle banner states where the vendored spec text is,
# or says loudly that it is missing from the checkout.
#
# Claude Code SessionStart hook: prints the open GitHub issue list (the
# tracker — CLAUDE.md issue workflow) annotated with native issue
# relationships (parent/sub-issue progress + blocked-by/blocking, via one
# batched GraphQL call), git status, and the last 10 commits so every session
# starts oriented. Also records the session-start HEAD and timestamp so
# phase_gate.sh (Stop hook) can tell whether a commit or issue activity
# happened during the session.
#
# No offline fallback: agents work online by definition; a failed `gh` call
# just surfaces its error.

set -uo pipefail

root="${CLAUDE_PROJECT_DIR:-$(git rev-parse --show-toplevel 2>/dev/null || pwd)}"
cd "$root" || exit 0

mkdir -p .claude
git rev-parse HEAD >.claude/.session-start-head 2>/dev/null || true
date -u +%Y-%m-%dT%H:%M:%SZ >.claude/.session-start-time 2>/dev/null || true

echo "=== spec oracle ==="
if [[ -d specs/openehr ]]; then
  echo "Vendored released openEHR spec text: specs/openehr/ (index: its README.md). Derive every expectation from that text first-hand — never from memory, a vendor's docs, or a server's behaviour (.claude/rules/cnf-triage.md)."
else
  echo "The vendored openEHR spec text is MISSING from this checkout — specs/openehr/ is not there. Do not answer a spec question until it is restored: re-run scripts/vendor/spec-docs.sh. Never answer from memory, from a vendor's documentation, or from what a server did (.claude/rules/cnf-triage.md)."
fi
echo "Released machine-readable bundles (the second root for XSD / JSON-Schema / OpenAPI citations): specs/its-xml-schemas/, specs/its-json-schemas/, specs/rest-oas/."
echo "The instrument's code lives HERE. Gate before every commit: cargo clippy --all-targets -- -D warnings && cargo nextest run && cargo run -- validate --root artifacts --specs specs/openehr (zero findings)."
echo
echo "=== tracker: open GitHub issues (gh issue view <n> --comments for the contract + discussion) ==="
echo "--- pinned (current focus) ---"
repo_nwo="$(gh repo view --json nameWithOwner --jq .nameWithOwner 2>/dev/null || true)"
gh api graphql \
  -f query='query($owner: String!, $name: String!) { repository(owner: $owner, name: $name) { pinnedIssues(first: 3) { nodes { issue { number title } } } } }' \
  -f owner="${repo_nwo%%/*}" -f name="${repo_nwo##*/}" \
  --jq '.data.repository.pinnedIssues.nodes[].issue | "#\(.number)  \(.title)"' 2>&1 || true
echo "--- open (child-of = sub-issue; {k/n} = sub-issue progress; BLOCKED-by = has an open blocker) ---"
# One batched GraphQL call yields each open issue's labels, milestone, parent,
# sub-issue progress, and open blockers/blocks — so the tracker shows work
# structure, not just a flat list. Falls back gracefully if gh/GraphQL fails.
gh api graphql \
  -f query='query($owner: String!, $name: String!) { repository(owner: $owner, name: $name) { issues(first: 100, states: OPEN, orderBy: {field: CREATED_AT, direction: DESC}) { nodes { number title labels(first: 20) { nodes { name } } milestone { title } parent { number } subIssuesSummary { total completed } blockedBy(first: 30) { nodes { number state } } blocking(first: 30) { nodes { number state } } } } } }' \
  -f owner="${repo_nwo%%/*}" -f name="${repo_nwo##*/}" \
  --jq '.data.repository.issues.nodes[]
    | ([.labels.nodes[].name] | join(", ")) as $labels
    | ([.blockedBy.nodes[] | select(.state == "OPEN") | "#\(.number)"]) as $b
    | ([.blocking.nodes[]  | select(.state == "OPEN") | "#\(.number)"]) as $k
    | "#\(.number)  \(.title)  [\($labels)]"
      + (if .milestone then "  (\(.milestone.title))" else "" end)
      + (if .parent then "  child-of #\(.parent.number)" else "" end)
      + (if .subIssuesSummary.total > 0 then "  {\(.subIssuesSummary.completed)/\(.subIssuesSummary.total)}" else "" end)
      + (if ($b | length) > 0 then "  BLOCKED-by \($b | join(","))" else "" end)
      + (if ($k | length) > 0 then "  blocks \($k | join(","))" else "" end)' 2>&1 || true
echo
echo "=== git status ==="
git status --short --branch 2>/dev/null | head -40
echo
echo "=== last 10 commits ==="
git log --oneline -10 2>/dev/null

exit 0
