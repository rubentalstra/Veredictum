# Issue relationships (GitHub native sub-issues and dependencies)

The tracker is GitHub Issues (root `CLAUDE.md` § Issue workflow). GitHub exposes
four native issue relationships; this project uses them as first-class tracker
structure rather than describing structure in prose. This file is the **policy**
(when to use each) and the **canonical commands** (how, with no guessing). The
one sanctioned write path is `scripts/gh/rel.sh`.

Ported from FerroEHR at the Veredictum split
([FerroEHR#2789](https://github.com/rubentalstra/FerroEHR/issues/2789)). The
no-duplication law and the four-relationship policy are unchanged in substance;
the board and workflow-skill cross-references were trimmed to what exists here.

## Two facts that dictate everything below

1. **`gh` has no native subcommand** for sub-issues or dependencies (verified
   against gh 2.88.1). Every relationship goes through `gh api` — or,
   preferably, `scripts/gh/rel.sh`, which wraps the correct endpoints.
2. **Write endpoints take the issue's database `id`, not its `#number`.** The
   sub-issue and dependency bodies want `sub_issue_id` / `issue_id` = the
   numeric database id (`scripts/gh/rel.sh id 3` printed `5258333287` on this
   repository), which is not the `#3` the UI shows. The helper resolves
   `#number → id` and fails loud on a bad number, so **prefer it over raw
   `gh api` for every write.** Reads are `#number`-keyed and need no
   resolution.

## The one sanctioned command surface — `scripts/gh/rel.sh`

| Intent | Command |
|---|---|
| Make #child a sub-issue of #parent | `scripts/gh/rel.sh parent <child> <parent>` |
| Move #child to a new parent (it already has one) | `scripts/gh/rel.sh parent <child> <parent> --replace` |
| Detach #child from its parent | `scripts/gh/rel.sh unparent <child>` |
| #n is blocked by #blocker | `scripts/gh/rel.sh blocked-by <n> <blocker>` |
| Remove "#n blocked-by #blocker" | `scripts/gh/rel.sh unblock <n> <blocker>` |
| #n blocks #blocked | `scripts/gh/rel.sh blocking <n> <blocked>` |
| Remove "#n blocking #blocked" | `scripts/gh/rel.sh unblocking <n> <blocked>` |
| Show every relationship of #n | `scripts/gh/rel.sh tree <n>` |
| Print the database id of #n | `scripts/gh/rel.sh id <n>` |

Each write prints a one-line `ok: …` confirmation. Run the script with no
arguments for the usage banner, which is its own header comment printed
structurally.

## The four relationships and when to use each

### 1. Parent / sub-issue — decomposition

A parent issue breaks into sub-issues; children roll up into a parent progress
bar, which the SessionStart tracker dump renders as `{k/n}`. Limits from the
docs: **at most 100 sub-issues per parent, 8 nesting levels, one parent per
issue** (reparent with `--replace`).

**Use it** to decompose a genuinely multi-part issue into individually
trackable, individually closeable work items — a catalogue chapter into one
child per behaviour family, an audit into one child per component. Each child
is a real issue with its own contract and acceptance criteria.

**Do not** use sub-issues to duplicate release grouping. **Milestones are the
release spine** — a release is cut when its milestone reaches zero open issues —
so no per-release parent issue exists. Sub-issues express *decomposition*,
milestones express *release*.

Work discovered en route gets its own issue (`gh issue create`) that is then
**linked** — a sub-issue of the issue it decomposes, or a dependency of the
issue it sequences — never left as a prose "see also".

### 2. Blocked-by — sequencing

#n cannot start or finish until its blockers close. GitHub marks blocked issues
with a "Blocked" badge, and the SessionStart dump prints `BLOCKED-by #x`. Limit:
**at most 50 issues per direction.**

**Use it** for real in-repository sequencing: #A must merge before #B is
workable. `scripts/gh/rel.sh blocked-by B A`.

**An upstream wait is not this edge.** `blocked-upstream` keeps its narrow
meaning — resolved in the openEHR Jira, normative text not yet published — and
it is a **label**, with no milestone, because an issue waiting on someone else
cannot carry a delivery promise. A wait on a defect **we** reported points at
the in-repository `upstream-report` issue with a native `blocked-by` edge
instead; the report itself gains `upstream-confirmed` once re-verified
first-hand. A wait with no in-repository counterpart stays label-only: an issue
cannot be `blocked_by` a Jira ticket.

### 3. Blocking — the mirror direction

The inverse of blocked-by. GitHub stores this as the *other* issue's
`blocked_by`, which is the only writable direction, so `scripts/gh/rel.sh
blocking n other` posts to #other under the hood. Read it back from either side
with `tree`. Use whichever direction reads more naturally; they describe the
same edge.

### 4. Security alerts — UI-only

Links a **code-scanning alert** to an issue so a security fix appears in
planning. This is **public preview and UI-only — there is no REST, GraphQL or
`gh` API** — so it cannot be scripted and `scripts/gh/rel.sh` does not cover it.
Code scanning is enabled (`codeql.yml`, plus the Trivy SARIF the image-scan
lane uploads), so alerts exist to link; the flow stays manual because the
endpoint is UI-only. The manual flow, once there is something to link: Security tab → Code
scanning → the alert → **Tracking** → *Create issue* or *Add existing GitHub
issue*.

## No duplication — a relationship lives in exactly ONE place

A relationship is GitHub metadata with its own panel. **Never also write it into
an issue body.** A body copy has no backlink, is not updated when the edge
changes, and rots into a contradiction the first time a child is added, closed
or reparented, or a dependency shifts. This decays fast and silently, and it is
the single most likely way this system goes stale. Concretely:

- **A parent's body never lists its sub-issues.** The native **Sub-issues**
  panel and its `{k/n}` progress bar are the single source of truth. Do not
  enumerate child numbers in the body, and do not write a body checklist that
  mirrors the children (`- [ ] #3 …`) — that double-books the progress bar and
  shows the same children twice.
- **An issue's body never lists its blockers or what it blocks.** The
  **Dependencies** panel is canonical.
- **A parent's acceptance criteria are OUTCOMES, not a roll-call of children.**
  "Every sub-issue closed" is already tracked by the progress bar. State the
  outcome the programme must reach, and point at the panel without naming
  individual children.
- **Prose may name an issue only when it is NOT a native edge** — "supersedes
  #X", "context in #Y", "adjudicated in #Z (closed)". If the reference *is* a
  parent, child or blocking edge, set the real relationship and leave it out of
  the body entirely.

Review-enforced: there is no parser for body prose. The structural safeguard is
that `scripts/gh/rel.sh` only ever touches metadata, never issue bodies — so the
only way an edge lands in a body is someone typing it, which this rule forbids.
When you create a parent issue, its body describes the programme and the
contract; the children are the panel.

## Reading relationships

- `scripts/gh/rel.sh tree <n>` — parent, sub-issues, blocked-by, blocking, each
  with state and title.
- The **SessionStart** dump (`.claude/hooks/inject_phase_context.sh`) surfaces,
  per open issue, its sub-issue progress `{k/n}`, `child-of #parent`, and open
  `BLOCKED-by` / `blocks` edges, computed in one batched GraphQL call. A blocked
  issue is not a pickup candidate until its blockers close.

## Interaction with the rest of the workflow

- `/next-task` skips an issue with open blockers unless the user names it, and
  for a parent issue points at the next open child.
- `/phase-done` refuses to close a parent that still has open children; closing
  a blocker unblocks its dependents automatically.
- The public roadmap board (`.claude/rules/project-board.md`) is a presentation
  view over the tracker, never a second tracker. Nothing in this file depends
  on it, and it duplicates none of these edges: blocked-ness on a card comes
  from the native `blocked-by` edge itself.

## Official documentation (durable citations)

- Sub-issues REST API — <https://docs.github.com/en/rest/issues/sub-issues>
- Issue dependencies REST API — <https://docs.github.com/en/rest/issues/issue-dependencies>
- Adding sub-issues — <https://docs.github.com/en/issues/tracking-your-work-with-issues/using-issues/adding-sub-issues>
- Creating issue dependencies — <https://docs.github.com/en/issues/tracking-your-work-with-issues/using-issues/creating-issue-dependencies>
- Linking code-scanning alerts to issues — <https://docs.github.com/en/code-security/how-tos/manage-security-alerts/manage-code-scanning-alerts/linking-code-scanning-alerts-to-github-issues>
