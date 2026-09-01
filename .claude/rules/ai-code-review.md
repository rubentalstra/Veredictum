# Machine review (SonarQube Cloud) — what it is and what it is not

Every pull request and every push to `main` is analysed by SonarQube Cloud
(`.github/workflows/sonar.yml`; scope in `sonar-project.properties`). It exists
because the guard tier catches what a guard can catch, review is one person,
and a deterministic sweep also reads trees no other check here reads: the shell
under `scripts/**` and `.claude/hooks/**`, workflow YAML, HTML and CSS, and
secret detection across the whole repository.

It is a **second opinion**. It is not authority, and it gates no merge.

## Precedence — a finding never outranks the sources

1. The released openEHR specification text — the oracle for every expectation
   this instrument encodes.
2. The hard rules: the root `CLAUDE.md` and the rule files beside this one.
3. The local gates: the CI guard tier, the Rust tier, and `validate` over the
   catalogue.
4. The analyzer.

A finding that contradicts a specification citation is wrong by construction:
the specification text is never a suspect (`cnf-triage.md`). A finding that asks
for something the rules forbid is wrong the same way. Nothing it reports relaxes
`testing.md` — never weaken a test, never adjust a catalogue expectation, and
never edit a corpus fixture because a finding suggested it.

## What it reads today

Rust, first-party: the analyzer runs Clippy itself from the package manifest,
under its own managed rule profile. That profile is a deliberately independent
second Clippy configuration beside the deny-tier lanes in `ci.yml`, so for pure
Rust it mostly re-reports what those already enforce. Its added value is the
multi-language sweep the Rust gates never see: the shell under `scripts/**` and
`.claude/hooks/**`, the workflow YAML, the brand study's HTML and CSS, and
secret detection. Coverage rides the same lane (#9): the job runs
`cargo llvm-cov` over the whole workspace, including the console, and imports
the merged lcov. The vendored trees are excluded in
`sonar-project.properties`, because acting on a finding inside vendored bytes is
forbidden here.

The shell coverage was verified rather than assumed, and the assumption it
replaced was wrong: the first scan produced 11 findings, all `shelldre:S7688`
(`[` versus `[[`) in the ported hooks, which is a Shell rule. It is a second
opinion beside the shellcheck bundled in the actionlint image, not a
replacement — shellcheck reads the shell embedded in workflow `run:` blocks,
which Sonar never sees as shell, so the two cover different files.

The fork-parity adjudication that kept those 11 findings is retired by owner
ruling. This repository owns its hooks now, so there is no upstream original
for a style edit to fork from. The S7688 family is fixed: every flagged POSIX
`[ … ]` test now reads `[[ … ]]`.

## New Code = since the last release

The project's New Code definition is **"Previous version"**, anchored by
`sonar.projectVersion`, which the lane reads from the latest release tag at scan
time rather than from a second copy that can drift. Releases are cut per
milestone, so the quality gate's `new_*` conditions and the pull-request
decoration measure the same window the changelog section describes. Do not
switch the definition to a day count or a reference branch without
re-adjudicating that alignment.

## The findings mirror into GitHub code scanning (#543)

Sonar's own GitHub integration is checks and PR decoration only, so a push to
`main` also exports the project's OPEN and CONFIRMED findings as SARIF
(`scripts/sonar/issues-to-sarif.sh`) and uploads them under the
`sonarqube-cloud` category. The dashboard stays canonical: the query excludes
ACCEPTED and FALSE_POSITIVE, so a disposition recorded there closes its GitHub
alert on the next push, and the mirror can never disagree with the dashboard
for longer than one analysis. Pull requests are not uploaded — Sonar's own
decoration covers them — and the mirror gates nothing.

## It does not gate a merge, and it never writes

No quality gate blocks a merge. A finding worth acting on is written by hand in
a normal change — never applied through a UI that would attribute a commit to a
bot, because the no-attribution rule has no exceptions. Promotion to a gating
check would follow a precision measurement, the same bar every reviewer here is
held to.

## False positives are data

Record a wrong finding on the tracker rather than silencing it. A scope or
profile change is made when the scope is actually wrong, never to move a number.

## Setup facts (so they are not rediscovered)

- Analysis is **CI-based**, not Automatic Analysis. The two are mutually
  exclusive per project, and Automatic Analysis cannot read Rust, so CI-based
  analysis is the only mode that reads this tree.
- The lane authenticates with a `SONAR_TOKEN` repository secret. A fork's pull
  request cannot read it, so the lane skips a fork and the guard tier in
  `ci.yml`, which needs no secret, is what gates that contribution.
- `.mcp.json` declares the SonarQube MCP server for local sessions, reading a
  `SONARQUBE_TOKEN` from the environment.

Official documentation (durable citations):
<https://docs.sonarsource.com/sonarqube-cloud/> ·
<https://docs.sonarsource.com/sonarqube-cloud/getting-started/github/> ·
<https://docs.sonarsource.com/sonarqube-cloud/standards/about-new-code>
