# CLAUDE.md

Veredictum is the independent conformance instrument for openEHR clinical data
repositories: a machine-readable catalogue of spec-cited test cases, executed
against any running CDR over its own wire, judged by pure functions over the
recorded exchanges. One tool covers functional conformance, measured
performance, and step-load stress. The released openEHR specifications are the
only authority it accepts, and every expectation in the catalogue cites the
section it comes from. The instrument is independent on purpose: a CDR must
never be able to grade its own homework, and that property only holds if the
workflow here cannot leak, shortcut, or bend an expectation. **This file is
that workflow.** The tracker is GitHub Issues in this repository; the record is
the closed issues, the PR descriptions, `CHANGELOG.md`, and git history. There
is no design-doc layer: decisions live in this file, in `.claude/rules/*.md`,
and in the code.

## Migration state (read this before anything else)

Veredictum was built inside [FerroEHR](https://github.com/rubentalstra/FerroEHR)
as a workspace member, and **the code lives here now.** The extraction carried
the runner, the catalogue, the corpora, the ambiguity register, the party
declarations and the vendored spec oracle, re-rooted at this repository's root.
A change to instrument behaviour lands here.

What is still open on the migration contract
[FerroEHR#2789](https://github.com/rubentalstra/FerroEHR/issues/2789) is the
CONSUMER side: FerroEHR carries its own copy until this one is published and its
conformance pipeline pins the published version instead. Until that switch the
two trees exist side by side, and this one is the source of truth. Publication —
the crates.io posture, the release pipeline, the container image — is tracked
here as #5 and #6.

Layout:

| Path | Contents |
|---|---|
| `src/**` | the runner machinery: driver, provisioning, resolver, outcome classification, comparator, verdict pipeline, the perf/stress/probe instruments |
| `src/bin/veredictum.rs` | the one binary; every instrument is a subcommand of it |
| `tests/**` | the integration suite: artifact gates, seeded-defect rejection, schema drift, claim completeness, the verification pack, the perf driver |
| `artifacts/**` | the catalogue: `schedule/` case cores, `bindings/` per-ITS operation bindings, `vocab/`, `corpus/`, `registers/ambiguities.yaml` |
| `party/**` | per-party statement + IXIT declarations |
| `schemas/**` | the published JSON Schemas for every artifact family (emitted, drift-guarded) |
| `verification-pack/**` | the recorded transcript proving the verdict pipeline reproduces its adjudicated verdicts |
| `specs/openehr/**` | the vendored released openEHR spec text: the oracle |
| `specs/its-xml-schemas/**`, `specs/its-json-schemas/**`, `specs/rest-oas/**` | the released machine-readable bundles, the second root a citation resolves against where the docs tree carries only prose |
| `scripts/vendor/**` | one script per vendored tree — the only way any of them is refreshed |
| `scripts/checks/**` | the repository guard scripts |
| `.claude/**` | rules, hooks, agents, memory |

The oracle is `specs/openehr/`, and it is in this repository. Read the governing
section there first-hand. Never answer a spec question from memory, from a
vendor's documentation, or from what a server did.

## The instrument's hard rules

### 1. The released openEHR specifications are the only authority

The vendored spec text is the oracle, and **it is never a suspect.** Before
authoring or changing any expectation, read the governing section first-hand
and quote the sentence that assigns the value.

- **Oracle set:** the RELEASED components. RM, BASE, AM, QUERY (AQL), TERM,
  ITS-XML, SM (the operation semantics and the naming the case cores use), and
  the ITS-REST **docs text**. SM anchors the operation, ITS-REST binds it to
  the wire, and both are oracles.
- **One ordered supplement:** the released OpenAPI documents are part of the
  release's own specification artifacts, so they ground an expectation **where
  the docs text is silent**. They lose to the docs text on any conflict, and an
  OAS-grounded expectation is always cited as the OAS, by file and element.
- **Never authority:** the CNF Platform Conformance Test Schedule (openEHR CNF
  never released a stable version, so it says which behaviours to cover, not
  what the correct answer is), the upstream Robot suites and their data sets
  (stalled), any CDR's behaviour, and any prior harness. Where one of these
  conflicts with a released component, the released component wins.
- Where both the docs text and the OAS are silent, the behaviour goes to the
  ambiguity register with a typed disposition. A private resolution is never
  acceptable, and neither is a bent expectation.

### 2. The three-way attribution law (the core discipline)

When a run goes red, the failure is attributed **before anything is changed**,
to exactly one of three suspects, by comparing spec-required against
catalogue-expected against SUT-observed. The full law is
`.claude/rules/cnf-triage.md`; the delegation target is the `cnf-triage` agent.

| Suspect | What it means | Fix path |
|---|---|---|
| **The SUT** | the server under test violates the spec | a defect report to that CDR, with the reproduced exchange and the spec citation. Never a change here |
| **The runner machinery** | the server behaved correctly and `src/**` misdrove the case or misjudged the response | fix the runner module. Those rows were inconclusive, never SUT failures |
| **The catalogue** | the hand-authored expectation is wrong against the spec | edit the artifact, with a new spec-cited source for the corrected expectation |

Two reflexes are banned, and the second one is the failure mode this product
exists to prevent:

1. **"The catalogue must be wrong, our SUT is right."** This is the reflex a
   vendor brings. The instrument answers it by construction: an expectation
   traces to a spec citation, so it is refuted by a better reading of the spec
   and by nothing else.
2. **"The SUT must be wrong, the instrument is right."** Veredictum is the
   thing being trusted, so it is a suspect on every red row, first-class,
   before the server. An instrument that presumes itself correct is worth
   nothing to the people who are supposed to rely on its verdicts. The first
   live triage in FerroEHR attributed 7 of 7 diagnosed defects to the runner,
   zero to the server under test.

Never read a SUT response to decide what an expectation should be. The response
is evidence in the comparison. The spec is the reference.

### 3. Coverage is a mandate, and it ratchets up only

A green run over a thin catalogue proves nothing. The catalogue must exercise
every behaviour the spec defines: every operation, every status-code branch,
every required or conditional header, every negotiation variant, every
precondition and error family, every RM and AQL behaviour. Each gets its own
small isolated case, so a red row names one defect.

- A spec-defined behaviour with no case is a gap to close, or an honest
  boundary recorded in the register. Silence is never coverage.
- Cases are added, never removed to go green. Narrowing coverage needs an
  adjudicated, spec-cited reason.
- **Every refusal the spec requires is an asserted negative test.** When a
  server is spec-right to reject something, the invalid shape is preserved as
  its own corpus entry plus a refusal case, so a lenient server fails it. A
  fixture fix that deletes the invalid shape silently narrows the claim.

### 4. Never weaken, skip, or delete a test

Not to make a build pass, and not to route around a bug a test exposes. If a
test fails and the fix is unclear, leave it failing and record a
`// TODO(#NNNN):` naming its issue. Details in `.claude/rules/testing.md`.

### 5. Comments follow RFC 505 and RFC 1574, with hard budgets

Line comments only; block comments are banned. Pending work is
`// TODO(#NNNN): <what is missing>`, always with its tracker issue. `// NOTE:`
is a settled decision as a citation plus one sentence, at most 3 lines.
A plain `//` run is at most 8 lines. Adjudication essays and change narration
belong on the PR or the issue. No other marker vocabulary exists. Full guide:
`.claude/rules/comments.md`, enforced by `scripts/checks/comment-style.sh`
through the edit hook and by the `guards` job in CI, over every hand-written
`.rs` file in the tree.

### 6. Reliability rules pair with a failing check

No `unsafe`, ever. No `unwrap`/`expect`/`panic!` outside tests. Errors are
typed at every boundary that branches. Determinism where output is compared.
The generic register, with the lint or check that enforces each rule, is
`.claude/rules/reliability.md`. A rule with no failing check is a wish, so when
you add one, add its enforcement in the same change.

### 7. Branches use the conventional types

`<type>/<kebab-case-slug>` with `type` one of `feat`, `fix`, `chore`, `docs`,
`refactor`, `perf`, `test`, `ci`, `build`, `release`. Pick the type by the
dominant change. Never force-push `main`.

### 8. Never add AI or Claude attribution to a commit or a PR

This is absolute and has no exceptions. No `Co-Authored-By` trailer of any
kind, no "Generated with Claude Code", no robot emoji, no similar line in a
commit message, body, trailer, PR title, PR description, PR comment, issue, or
code comment. Commit and PR text describe the change and nothing else. Do not
pass a flag or template that injects attribution. The `no_attribution_guard.sh`
hook blocks the command before it runs.

### 9. Every commit is verified

Local commits are GPG-signed (`commit.gpgsign=true`). Never disable it and
never commit with a stripped identity.

**Learned 2026-08-26: the GitHub contents API creates UNSIGNED commits.** A
commit written through `PUT /repos/{owner}/{repo}/contents/{path}` lands
unverified, so it is not an acceptable way to write to this repository. Two
paths are: a local signed commit, or a workflow that builds the commit through
the Git Data API (blob, then tree, then commit, then ref update), which GitHub
signs as `github-actions[bot]`. Any workflow that writes a commit uses the Git
Data API pattern, never `git commit` and push, and never the contents API.

### 10. Keep the changelog

`CHANGELOG.md` follows Keep a Changelog 1.1.0. Every PR with user-visible
changes adds an entry under `## [Unreleased]` in the same PR. User-visible here
means the CLI surface, the catalogue's published schemas, verdict semantics,
the container image, or anything a party's published record depends on.
Releases are cut from the changelog: rename the Unreleased heading to the
version with its date, re-add an empty one, update the link references, then
tag. Rewrite the HEADING, not the first textual match — the v0.0.1-alpha.1 cut
rewrote a mention inside a paragraph and left the release with no section.
Milestones are releases, and a release is cut when its milestone reaches zero
open issues. `scripts/checks/changelog-structure.sh` runs in CI on every change;
the guard that REQUIRES an entry when a user-visible surface changes is issue
#10 — those paths exist now that the code is here, so it is tracked rather than
assumed.

### 11. Cite only durable references

In code, schema, and doc comments, justify behaviour by citing the vendored
openEHR spec text (file plus section) or official external documentation: the
Rust book and reference, the docs.rs page of a pinned crate, the PostgreSQL
docs where a SUT's storage is relevant. Never cite an internal markdown file,
because internal documents move or die. Where the specs are silent, write the
explicit flag "no openEHR spec governs this — our own design", never a pointer
to a plan file.

### 12. Prose has no AI tells

Every piece of text a human reads as prose is held to
`.claude/rules/writing-style.md`: no "not X, but Y" framing, no decorative
triads, no buzzword vocabulary, an em dash budget, no vague transitions. This
covers the README, the docs site, issue and PR bodies, release notes, and
announcement drafts. It does not loosen any technical rule.

## Issue workflow

The tracker is GitHub Issues in this repository. The open issue list is the
worklist, and issue state is edited only through `gh`. Never track work in chat
alone.

1. Orient with `gh issue list --state open`. Read the contract with
   `gh issue view <n> --comments`. Skip an issue whose blockers are still open.
2. **The issue body is the contract.** It opens with a plain summary, no
   heading: what, why, the owner rulings, the spec citations. Then an
   `## Acceptance criteria` checklist, and an optional `## Tasks` task list.
3. Do the work. Read the governing spec sections first for anything
   spec-facing. Build compiling, tested increments.
4. Record progress on the issue: tick verified criteria with
   `gh issue edit <n>`, post decisions as comments with `gh issue comment <n>`.
5. Commit on a conventional-type branch with a descriptive subject. The PR body
   declares `Closes #<n>` so the merge closes the issue. One `Closes` keyword
   per issue: `Closes #1, #2` closes only #1.
6. New work found en route gets its own issue, linked as a sub-issue of the
   issue it decomposes or as a real `blocked-by` dependency. A prose "see also"
   is not a link, and an edge lives in its native panel, never in the body.

**Relationships are native metadata, set through one command.** `gh` has no
subcommand for sub-issues or dependencies, and the write endpoints take an
issue's database id rather than its number, so every edge is set with
`scripts/gh/rel.sh` (`parent`, `unparent`, `blocked-by`, `unblock`, `blocking`,
`unblocking`, `tree`, `id`) and never with a hand-typed `gh api`. Sub-issues
express decomposition, `blocked-by` expresses real in-repository sequencing, and
neither is ever restated in an issue body — the panel is canonical and a body
copy rots on the first change. Full policy: `.claude/rules/issue-relationships.md`.

**Labels.** Exactly one type label per issue, mapped to the conventional-commit
types: `bug` for fix, `enhancement` for feat, `documentation` for docs, plus
`chore`, `refactor`, `perf`, `ci`. Priority is `P0` through `P3`. The component
an issue adjudicates against carries a `spec:` label (`spec:RM`, `spec:BASE`,
`spec:AM`, `spec:QUERY`, `spec:TERM`, `spec:ITS`, `spec:SM`, `spec:CNF`).
`question` routes a question, `on-hold` marks work parked by owner decision, and
`no-changelog` is the escape hatch the changelog guard reads — a guard label that
does not exist fails silently at apply time, so the label exists before the guard
that reads it. An outbound
report of a defect, contradiction, or silence in a released openEHR
specification is an issue labeled `upstream-report`, and that issue **is** the
report: a plain summary, then `## What the released spec says` with citations,
`## What this implementation does`, `## Resolution sought upstream`. The
register entry points at it. A report is created unverified, gains
`upstream-confirmed` once re-verified first-hand, and closes as the standing
outbound record once its divergence is adjudicated here. A refuted one closes
as refuted, its register entry is removed or re-grounded, and the affected case
becomes gating.

**Milestones are releases.** A milestone is a delivery promise, so an issue
waiting on something upstream carries `blocked-upstream` and no milestone. A
release is cut when its milestone reaches zero open issues; the next milestone
always exists, so triage always has a target. Open today: `v0.0.1` (the
repository standing on its own — identity, discipline, CI, tracker machinery)
and `v0.1.0` (the code migration, FerroEHR#2789, deliberately without a due
date because the date follows the extraction).

**Skills.** `/phase-status` orients, `/next-task` turns an issue into a plan,
`/phase-done` closes one. Each is trimmed to the machinery that exists here and
names what it deliberately does not check, because a verification step pointing
at absent machinery reports green.

## Build and test

Every gate below runs on every pull request, every push to `main` and every
merge-queue entry, behind one required `conclusion` check
(`.github/workflows/ci.yml`; `scripts/checks/ci-conclusion-complete.sh` refuses
a job that runs without gating the merge).

The guard tier, ungated because its inputs exist on every change:

```bash
bash scripts/checks/comment-style.sh --all        # comment form and budgets
bash scripts/checks/changelog-structure.sh        # Keep a Changelog structure
bash scripts/checks/ci-conclusion-complete.sh     # no CI job runs ungated
zizmor --min-severity=low .github/workflows/      # workflow security posture
actionlint                                        # workflow correctness + shellcheck
reuse lint                                        # REUSE 3.3 licensing
```

The Rust tier. Run these before every commit; CI gates them on whether the
change touched anything they read:

```bash
cargo build --all-targets
cargo nextest run                       # never cargo test
cargo clippy --all-targets -- -D warnings
cargo fmt --all --check
cargo deny check
RUSTDOCFLAGS="-D warnings" cargo doc --no-deps --document-private-items
cargo hack check --rust-version --all-targets    # the declared MSRV, verified
cargo machete                                    # no dependency nothing imports
```

**`validate` is the gate the catalogue lives or dies by**, and it is clean
before any SUT is composed:

```bash
cargo run -- validate --root artifacts --specs specs/openehr
```

It is every machine check over the artifact tree — id uniqueness, citation
resolution against the vendored specs, binding completeness, coverage of the
enumerated wire surface, claim completeness against the committed party
statements — and **zero findings is the only passing result.** The instrument's
own canonical CLI table (`validate`, `run`, `verdicts`, `perf`, `stress`,
`aql-probe`, `emit-schemas`) is the authority on how to invoke everything else;
never improvise a flag.

## Model orchestration

The session model orchestrates and does the judgement-heavy work itself: the
attribution calls, the spec adjudications, the catalogue design. It fans
bounded, file-heavy implementation out to subagents through the `Agent` tool,
at most two implementation workers at a time. Delegate by the nature of the
work, never reflexively, and hand every spec-facing worker the exact spec paths
it must read. The defined agents are in `.claude/agents/`:

- `cnf-triage` — read-only attribution of red rows. Every red run goes here
  before any code is touched.
- `spec-researcher` — answers a spec question from the vendored text with exact
  citations, keeping heavy `.adoc` reading out of the main context.
- `implementer` — bounded implementation on a tight spec.

Subagents obey every rule in this file. Tell them not to spawn further
subagents.

## References

- `.claude/rules/cnf-triage.md` — the attribution law and the register lifecycle
- `.claude/rules/testing.md` — test discipline and the coverage mandate
- `.claude/rules/comments.md` — RFC 505 / RFC 1574 with budgets
- `.claude/rules/reliability.md` — the safety rules and their enforcement
- `.claude/rules/writing-style.md` — prose style
- `.claude/rules/ai-code-review.md` — machine review is a second opinion, never
  authority; the SonarQube Cloud setup facts
- `.claude/rules/issue-relationships.md` — the four native issue edges, the one
  sanctioned write path, and the no-duplication law
- `README.md` — the product identity and the origin of the name
- [FerroEHR#2789](https://github.com/rubentalstra/FerroEHR/issues/2789) — the
  migration contract
