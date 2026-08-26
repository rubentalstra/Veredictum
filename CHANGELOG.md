# Changelog

All notable changes to Veredictum are recorded here. The format follows
[Keep a Changelog 1.1.0](https://keepachangelog.com/en/1.1.0/) and the
versioning follows [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

Every pull request with user-visible changes adds an entry under the Unreleased
heading below, in the same pull request. User-visible here means the CLI surface,
the published artifact schemas, verdict semantics, the container image, or
anything a party's published conformance record depends on. The heading is named
in prose rather than quoted verbatim on purpose: a release cut rewrites the first
literal occurrence of it, and quoting it here is what turned this paragraph into
a stray release heading at the v0.0.1-alpha.1 cut.

The runner itself still lives in FerroEHR `tools/cnf-runner` until the
migration ([FerroEHR#2789](https://github.com/rubentalstra/FerroEHR/issues/2789))
completes; releases before that carry the repository skeleton and identity.

## [Unreleased]

### Added

- Continuous integration. `ci.yml` runs on every pull request, every push to
  `main` and every merge-queue entry: a guard tier (comment style, changelog
  structure, the no-attribution scan over the pushed commits, REUSE 3.3
  licensing), a workflow audit (zizmor for the security posture, actionlint with
  bundled shellcheck for correctness, and a check that every job actually gates
  the merge), and a single required `conclusion` check. The Rust gates arrive
  with the code migration; the header of the workflow says so rather than
  leaving the absence to be inferred.
- `scorecard.yml`: the weekly OpenSSF Scorecard analysis, publishing its score
  to the OpenSSF API and its findings into code scanning.
- `sonar.yml` and `sonar-project.properties`: SonarQube Cloud analysis of the
  whole tree on every pull request and every push to `main`, advisory under
  `.claude/rules/ai-code-review.md`, with the New Code window anchored to the
  latest release tag.
- The OpenSSF Best Practices and OpenSSF Scorecard badges in the README, both
  reading live scores rather than asserting a posture.
- Two ported guard scripts: `scripts/checks/changelog-structure.sh` (Keep a
  Changelog structure) and `scripts/checks/ci-conclusion-complete.sh` (no CI job
  runs without gating the merge).
- The tracker machinery. `scripts/gh/rel.sh` is the one sanctioned write path
  for GitHub's four native issue edges — sub-issue, blocked-by and their
  inverses — resolving an issue number to the database id the write endpoints
  actually want and failing loud on a bad one, with
  `.claude/rules/issue-relationships.md` as its policy. The label taxonomy is
  complete against the scheme `CLAUDE.md` defines: `blocked-upstream`,
  `on-hold`, `no-changelog`, and the eight `spec:` component labels join the
  type and priority sets. Two milestones open the release spine, `v0.0.1` and
  `v0.1.0`. `/phase-status`, `/next-task` and `/phase-done` are ported and
  trimmed to the machinery that exists, each naming what it deliberately does
  not check.

### Removed

- The `accessibility` label. Nothing referenced it and it is not part of the
  taxonomy `CLAUDE.md` defines.

### Fixed

- The changelog's own intro paragraph, which the v0.0.1-alpha.1 cut turned into
  a stray release heading by rewriting the first literal `## [Unreleased]` it
  found — which was in prose, not the heading. The v0.0.1-alpha.1 section now
  exists as a real section, and the paragraph names the heading instead of
  quoting it.
- `SUPPORT.md` said GitHub Discussions was not enabled. It is, with six
  categories.

## [0.0.1-alpha.1] - 2026-08-26

### Added

- The repository's working discipline: the root `CLAUDE.md`, the rule files
  under `.claude/rules/`, the guard hooks under `.claude/hooks/`, the agent
  definitions under `.claude/agents/`, the in-repo memory under
  `.claude/memory/`, and the comment-style guard under `scripts/checks/`.
- The product identity: the README, the origin of the name, and the pointer to
  the migration contract.
- The repository skeleton a public project is read by: `CONTRIBUTING.md`,
  `CODE_OF_CONDUCT.md`, `GOVERNANCE.md`, `MAINTAINERS.md`, `SECURITY.md`,
  `SUPPORT.md`, `AI_STATEMENT.md`, the `.github/` collaboration surface (issue
  forms, pull-request template, `CODEOWNERS`, `FUNDING.yml`, and a
  `dependabot.yml` covering the github-actions ecosystem), the `REUSE.toml`
  licensing declaration with `LICENSES/Apache-2.0.txt`, the
  attribution-stripping `commit-msg` hook with `scripts/install-hooks.sh`, and
  the Rust `.gitignore` set.

[unreleased]: https://github.com/rubentalstra/Veredictum/compare/v0.0.1-alpha.1...HEAD
[0.0.1-alpha.1]: https://github.com/rubentalstra/Veredictum/releases/tag/v0.0.1-alpha.1
