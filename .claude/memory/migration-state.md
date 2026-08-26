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

Added 2026-08-26: the starter CI (`.github/workflows/ci.yml` — the `guards` and
`workflow-audit` tiers behind one required `conclusion` check —,
`scorecard.yml`, `sonar.yml` with `sonar-project.properties` and `.mcp.json`),
the two guards those lanes needed
(`scripts/checks/changelog-structure.sh`, `scripts/checks/ci-conclusion-complete.sh`),
and `.claude/rules/ai-code-review.md` as the law over the machine reviewer.

**What has not:** the runner source, the catalogue artifacts, the vendored spec
text, the corpora and their PROVENANCE trees, the ambiguity register, the party
statements and IXIT examples, the workspace scaffolding
(`Cargo.toml`, `rust-toolchain.toml`, `clippy.toml`, `deny.toml`,
`rustfmt.toml`), the Rust CI lanes, the release pipeline, and the container
image. The checklist below is the full inventory.

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
- [x] `.github/workflows/` — the starter lanes landed 2026-08-26, small on
      purpose: `ci.yml` (a `guards` tier, a `workflow-audit` tier, one required
      `conclusion` gate), `scorecard.yml`, `sonar.yml`. Still to come, each with
      the thing it builds: the release pipeline, the image build and
      scan-before-tag lanes, the published-image scan, latest-deps, and the
      watcher engine. `.github/actions/` stays absent until a step is repeated
      often enough to be a composite action — the zizmor invocation names only
      `.github/workflows/`, so adding the directory means extending that
      invocation in the same change.
- [ ] `.github/actionlint.yaml` — the runner-label allowlist. Still not needed:
      every job runs on `ubuntu-latest`, and this file exists only to teach
      actionlint about labels its built-in roster lags. It lands with the first
      job that needs a non-default runner.
- [ ] `.github/dependabot.yml` — extend with the `cargo` entry (and `docker`
      when a Dockerfile lands). The `github-actions` entry is already live.
- [ ] `.fossa.yml`, `.fossabot.yml` — both scan the Cargo graph; meaningless
      before one exists. Also a decision, not a default: FerroEHR's exclusion
      list exists to keep vendored third-party trees out of a licence scan, and
      this repository will have its own.
- [ ] `.mdbook-lint.toml` — arrives with the docs-site decision (#2789 lists it
      as open: mdBook on GitHub Pages or a domain).
- [x] `.mcp.json` — landed 2026-08-26 with the analysis lane, keyed to
      `rubentalstra_Veredictum` (the owner created the Sonar project that day),
      reading a `SONARQUBE_TOKEN` from the local environment.
- [ ] `scripts/checks/` — the remaining guards: the spec-citation resolver, the
      default-value style check, the typed-style check, and the SPDX header
      check. The changelog structure check landed 2026-08-26, together with
      `ci-conclusion-complete.sh` (no CI job runs without gating the merge).
- [ ] `security/vex/` — OpenVEX documents for inherited container-layer
      findings; arrives with the image scan lane.
- [x] The Scorecard workflow landed 2026-08-26 and publishes its score to the
      OpenSSF API; both OpenSSF badges are in the README.
- [ ] The OpenSSF Best Practices criteria adjudication. The project entry exists
      (14252, created by the owner 2026-08-26) and its badge is in the README,
      so the score is already public and honest. Filling the criteria in is
      DEFERRED by owner decision 2026-08-26 — do not propose statuses until the
      owner picks it up. What the deferral is waiting on is mostly the code:
      six MUST criteria are about producing and testing software (basic and
      interface documentation, a build system, a test suite, evidence that tests
      were added, compiler warning flags), and all six discharge with the
      migration rather than by argument.

Not applicable, checked and recorded so they are not re-surveyed: `.editorconfig`
(does not exist in FerroEHR), `vercel.json`, `Dockerfile.vercel`,
`docker-compose*.yml`, `deploy/`, `website/`, `fuzz/`, `docs/` — all FerroEHR's
CDR, sandbox, or documentation-site surfaces with no counterpart here. A
Veredictum compose file for a target CDR, if one is ever wanted, is new work
rather than a port.

`sonar-project.properties` was on that list until 2026-08-26 and is now live:
the owner created the SonarQube Cloud project that day, which is what made a
scan scope meaningful. Its exclusion tables are deliberately empty — a
`sonar.exclusions` line naming a vendored or generated path that does not exist
would be a scope claim about nothing.
