# Changelog

All notable changes to Veredictum are recorded here. The format follows
[Keep a Changelog 1.1.0](https://keepachangelog.com/en/1.1.0/) and the
versioning follows [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

Every pull request with user-visible changes adds an entry under
`## [Unreleased]

## [0.0.1-alpha.1] - 2026-08-26` in the same pull request. User-visible here means the CLI
surface, the published artifact schemas, verdict semantics, the container
image, or anything a party's published conformance record depends on.

The runner itself still lives in FerroEHR `tools/cnf-runner` until the
migration ([FerroEHR#2789](https://github.com/rubentalstra/FerroEHR/issues/2789))
completes; releases before that carry the repository skeleton and identity.

## [Unreleased]

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
