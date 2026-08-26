# Machine review (SonarQube Cloud) — what it is and what it is not

Every pull request and every push to `main` is analysed by SonarQube Cloud
(`.github/workflows/sonar.yml`; scope in `sonar-project.properties`). It exists
because the guard tier catches what a guard can catch, review is one person,
and a deterministic sweep also reads trees no other check here reads: workflow
YAML, HTML and CSS, and secret detection across the whole repository.

It is a **second opinion**. It is not authority, and it gates no merge.

## Precedence — a finding never outranks the sources

1. The released openEHR specification text — the oracle for every expectation
   this instrument encodes.
2. The hard rules: the root `CLAUDE.md` and the rule files beside this one.
3. The local gates: the CI guard tier, and the Rust and catalogue gates that
   arrive with the code.
4. The analyzer.

A finding that contradicts a specification citation is wrong by construction:
the specification text is never a suspect (`cnf-triage.md`). A finding that asks
for something the rules forbid is wrong the same way. Nothing it reports relaxes
`testing.md` — never weaken a test, never adjust a catalogue expectation, and
never edit a corpus fixture because a finding suggested it.

## What it reads today

There is no first-party source code in this repository yet: the runner still
lives in FerroEHR `tools/cnf-runner` until the extraction lands
([FerroEHR#2789](https://github.com/rubentalstra/FerroEHR/issues/2789)). So the
analyzer's material is the workflow YAML, the brand study's HTML and CSS, and
secret detection. Sonar ships no Shell analyzer, so `scripts/checks/*.sh` are
read by secret detection alone — shellcheck, bundled inside the actionlint
image, is what lints them.

Rust is analysed first-party once the code arrives (the analyzer runs Clippy
itself from the workspace manifest, as a second and deliberately independent
Clippy configuration beside our own deny-tier lanes). Coverage rides the same
lane at that point, imported as lcov.

## New Code = since the last release

The project's New Code definition is **"Previous version"**, anchored by
`sonar.projectVersion`, which the lane reads from the latest release tag at scan
time rather than from a second copy that can drift. Releases are cut per
milestone, so the quality gate's `new_*` conditions and the pull-request
decoration measure the same window the changelog section describes. Do not
switch the definition to a day count or a reference branch without
re-adjudicating that alignment.

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
  exclusive per project, and Automatic Analysis cannot read Rust, so the mode is
  already the one the code migration needs.
- The lane authenticates with a `SONAR_TOKEN` repository secret. A fork's pull
  request cannot read it, so the lane skips a fork and the guard tier in
  `ci.yml`, which needs no secret, is what gates that contribution.
- `.mcp.json` declares the SonarQube MCP server for local sessions, reading a
  `SONARQUBE_TOKEN` from the environment.

Official documentation (durable citations):
<https://docs.sonarsource.com/sonarqube-cloud/> ·
<https://docs.sonarsource.com/sonarqube-cloud/getting-started/github/> ·
<https://docs.sonarsource.com/sonarqube-cloud/standards/about-new-code>
