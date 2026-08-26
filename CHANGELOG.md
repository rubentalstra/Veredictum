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

The instrument's code lives here as of the migration
([FerroEHR#2789](https://github.com/rubentalstra/FerroEHR/issues/2789)). Releases
before it carried the repository skeleton and the product identity alone.

## [Unreleased]

### Added

- **The instrument itself.** The runner, the catalogue, the corpora, the
  ambiguity register, the party declarations and the vendored openEHR
  specification oracle now live in this repository, extracted from the FerroEHR
  mono-repo with 611 commits of history and re-rooted: the crate's own directory
  became the repository root, and `docs/specs/openehr/` became `specs/openehr/`.
- The command is `veredictum`. The package, the binary and the library carry the
  product's name, and so does the debug switch, now
  `VEREDICTUM_DEBUG_EXCHANGES`. Every subcommand keeps its name and its flags:
  `validate`, `run`, `verdicts`, `perf`, `stress`, `aql-probe`,
  `stress-compare`, `perf-assets`, `conformance-assets`, `emit-schemas`. Two
  paths move with the tree — an artifact root is now `artifacts` and a spec root
  is now `specs/openehr`.
- The standalone workspace: one package at the root, its own SemVer line from
  `0.1.0-alpha.1`, edition 2024, Apache-2.0, with the deny-tier lint tables,
  `rust-toolchain.toml`, `rustfmt.toml`, `clippy.toml`, `deny.toml` and
  `.config/nextest.toml` carried over and adapted to what this tree actually
  contains. `Cargo.lock` is committed, because this repository ships a binary.
- Three released machine-readable bundles beside the specification text, so a
  citation that can only resolve against a schema resolves here rather than
  nowhere: `specs/its-xml-schemas/` (the two XSD lineages),
  `specs/its-json-schemas/` (the ITS-JSON validation oracle) and
  `specs/rest-oas/` (the 21 released ITS-REST OpenAPI bundles).
- The corpus vendoring scripts, so every vendored tree can still be refreshed
  the only sanctioned way, by re-running its script:
  `scripts/vendor/ckm-templates.sh`, `scripts/vendor/ckm-archetypes.sh`,
  `scripts/vendor/adl2-archetypes.sh` and `scripts/generate-ckm-examples.sh`.
- The Rust CI tier, gated on whether a change touches anything it reads:
  `rustfmt`, `clippy --all-targets` at `-D warnings`, build plus `nextest` plus
  the instrument's own `validate` self-check, the rustdoc gate, the declared
  MSRV verified with `cargo hack check --rust-version`, `cargo deny check`, and
  `cargo machete` for dependencies nothing imports. All seven join the single
  required `conclusion` check.
- CodeQL analyzes `rust` beside `actions`, and the SonarQube scope covers `src/`
  and `tests/` with the vendored trees excluded.
- Continuous integration. `ci.yml` runs on every pull request, every push to
  `main` and every merge-queue entry: a guard tier (comment style, changelog
  structure, the no-attribution scan over the pushed commits, REUSE 3.3
  licensing), a workflow audit (zizmor for the security posture, actionlint with
  bundled shellcheck for correctness, and a check that every job actually gates
  the merge), and a single required `conclusion` check.
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
