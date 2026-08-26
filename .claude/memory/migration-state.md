---
name: migration-state
description: "The runner code still lives in FerroEHR tools/cnf-runner until the #2789 extraction lands; this repo owns the identity and the tracker"
metadata:
  node_type: memory
  type: fact
  originSessionId: 32d068af-12e7-4654-9ece-124240b2367f
  modified: 2026-08-26T00:00:00.000Z
---

State as of 2026-08-26: the Veredictum repository holds the product identity,
the agent discipline, and the tracker. **The living code is still FerroEHR
`tools/cnf-runner`.** The migration contract is
[FerroEHR#2789](https://github.com/rubentalstra/FerroEHR/issues/2789).

**What has moved:** the name and the origin story (`README.md`), the root
`CLAUDE.md`, `.claude/rules/*`, `.claude/hooks/*`, `.claude/agents/*`,
`.claude/memory/*`, `AGENTS.md`, `scripts/checks/comment-style.sh`, the identity
set (`NOTICE`, `CITATION.cff`, `.zenodo.json`, `assets/brand/*`,
`.gitattributes`), the community and policy set (`CONTRIBUTING.md`,
`CODE_OF_CONDUCT.md`, `GOVERNANCE.md`, `MAINTAINERS.md`, `SECURITY.md`,
`SUPPORT.md`, `AI_STATEMENT.md`), the GitHub collaboration surface
(`.github/ISSUE_TEMPLATE/*`, `.github/pull_request_template.md`,
`.github/CODEOWNERS`, `.github/FUNDING.yml`, `.github/dependabot.yml` with the
github-actions ecosystem only), the licensing declaration (`REUSE.toml`,
`LICENSES/Apache-2.0.txt`), the attribution-stripping git hook
(`.githooks/commit-msg`, `scripts/install-hooks.sh`), and the Rust `.gitignore`
set.

**What has not:** the runner source, the catalogue artifacts, the vendored spec
text, the corpora and their PROVENANCE trees, the ambiguity register, the party
statements and IXIT examples, the workspace scaffolding
(`Cargo.toml`, `rust-toolchain.toml`, `clippy.toml`, `deny.toml`,
`rustfmt.toml`), every CI lane, and the container image. The checklist below is
the full inventory.

**How to apply:**
- A change to runner behaviour lands in FerroEHR until the extraction
  completes. Do not re-implement it here.
- A spec question is answered from a FerroEHR checkout at
  `docs/specs/openehr/` until the spec text is vendored here, and the answer
  names which checkout was read.
- A rule or hook that references machinery not yet present says so in place.
  Do not delete such a rule to make the tree self-consistent, and do not write
  prose that implies the machinery exists.
- FerroEHR keeps its committed conformance baselines: they are claims about
  that CDR, not about this instrument. After the split, FerroEHR pins a
  released Veredictum version instead of building the runner from source.

## Arrives with the code migration

The full-inventory sweep of the FerroEHR root, taken 2026-08-26 while completing
the repository skeleton. Every item below was examined and deliberately NOT
ported, because it is inert or misleading without the code it configures. Each
lands in the pull request that adds the thing it configures, adapted rather than
copied. Nothing here is forgotten work: this list is the record.

- [ ] `Cargo.toml` + `Cargo.lock` — the workspace, own SemVer starting fresh.
- [ ] `rust-toolchain.toml` — the pinned toolchain and MSRV policy.
- [ ] `rustfmt.toml`, `clippy.toml` — formatting and the disallowed-API bans.
- [ ] `deny.toml` — the single advisory, licence, yanked and source gate.
- [ ] `.cargo/config.toml` — build configuration.
- [ ] `.config/nextest.toml` — test-runner profiles and the container-group
      filters.
- [ ] `.devcontainer/` — FerroEHR's carries stack pull/start scripts for a CDR
      plus PostgreSQL; a Veredictum devcontainer needs the toolchain and a
      target CDR to point at, so it is authored fresh, not copied.
- [ ] `.dockerignore`, `.hadolint.yaml`, `.trivyignore.yaml` — arrive with the
      container image (the distroless multi-arch GHCR image of #2789).
- [ ] `.github/workflows/` + `.github/actions/` — the PR lane, the one release
      pipeline, the image build and scan-before-tag lanes, the scheduled
      published-image scan, latest-deps and Scorecard lanes. Instantiate the
      pattern small; do not copy 23 files.
- [ ] `.github/actionlint.yaml` — the runner-label allowlist; inert until a
      workflow exists.
- [ ] `.github/dependabot.yml` — extend with the `cargo` entry (and `docker`
      when a Dockerfile lands). The `github-actions` entry is already live.
- [ ] `.fossa.yml`, `.fossabot.yml` — both scan the Cargo graph; meaningless
      before one exists. Also a decision, not a default: FerroEHR's exclusion
      list exists to keep vendored third-party trees out of a licence scan, and
      this repository will have its own.
- [ ] `.mdbook-lint.toml` — arrives with the docs-site decision (#2789 lists it
      as open: mdBook on GitHub Pages or a domain).
- [ ] `.mcp.json` — FerroEHR's declares one MCP server, SonarQube Cloud, keyed
      to `rubentalstra_FerroEHR`. Not portable: no Sonar project exists for this
      repository, and a config naming a project that does not exist is worse
      than none. Land it if and when the analysis lane is set up.
- [ ] `scripts/checks/` — the remaining guards: the spec-citation resolver, the
      default-value style check, the typed-status check, the SPDX header check,
      the changelog structure check. Each with the rule text that justifies it.
- [ ] `security/vex/` — OpenVEX documents for inherited container-layer
      findings; arrives with the image scan lane.
- [ ] The OpenSSF Best Practices badge entry and the Scorecard workflow (#2789
      names both; FerroEHR's answers are the template).

Not applicable, checked and recorded so they are not re-surveyed: `.editorconfig`
(does not exist in FerroEHR), `sonar-project.properties`, `vercel.json`,
`Dockerfile.vercel`, `docker-compose*.yml`, `deploy/`, `website/`, `fuzz/`,
`docs/` — all FerroEHR's CDR, sandbox, or documentation-site surfaces with no
counterpart here. A Veredictum compose file for a target CDR, if one is ever
wanted, is new work rather than a port.
