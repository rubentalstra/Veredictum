---
name: migration-state
description: "The instrument's code lives in this repository as of the 2026-08-26 extraction; what it carried, what it re-rooted, and what is still open"
metadata:
  node_type: memory
  type: fact
  originSessionId: 32d068af-12e7-4654-9ece-124240b2367f
  modified: 2026-08-26T00:00:00.000Z
---

State as of 2026-08-26: **the code lives here.** The extraction landed and this
repository is the source of truth for the instrument. The migration contract is
[FerroEHR#2789](https://github.com/rubentalstra/FerroEHR/issues/2789).

## What the extraction carried

`git filter-repo` over a scratch clone of FerroEHR, keeping only the instrument
and its inputs and re-rooting them. 611 commits — every commit that ever touched
the runner, its catalogue, the vendored spec oracle or the vendoring script:

| Was, in the mono-repo | Is, here |
|---|---|
| the crate's own directory under `tools/` | the repository root |
| `docs/specs/openehr/` | `specs/openehr/` |
| `scripts/vendor/spec-docs.sh` | unchanged |

The extraction rewrote commit objects, so the carried history is UNSIGNED — a
rewrite cannot preserve signatures. Signing resumes with the merge commit.

Renamed in the same change: the package, the binary and the library are
`veredictum`; the bin source is `src/bin/veredictum.rs`; the debug switch is
`VEREDICTUM_DEBUG_EXCHANGES`. No trace of the old crate name remains outside the
vendored spec text.

## What was added because the extraction alone was not self-contained

- Three released machine-readable bundles the citation gates resolve against,
  vendored from FerroEHR's `crates/openehr-its/` (no history — they were outside
  the extraction path set): `specs/its-xml-schemas/`, `specs/its-json-schemas/`,
  `specs/rest-oas/`. Load-bearing, not decoration: removing the XSD bundle alone
  produces 14 `spec-ref` findings.
- The three corpus vendoring scripts (`ckm-templates.sh`, `ckm-archetypes.sh`,
  `adl2-archetypes.sh`) plus `generate-ckm-examples.sh`. The vendored-corpora
  rule is that a tree is refreshed only by re-running its script, so a corpus
  whose script stayed behind was un-refreshable. `adl2-archetypes.sh` was split:
  its FerroEHR half vendored an ADL-engine regression library that does not
  exist here.

## What is still open

- The CONSUMER side. FerroEHR keeps its own copy of the runner until this one is
  published and its conformance pipeline pins the published version. FerroEHR
  also keeps its committed conformance baselines: those are claims about that
  CDR, not about this instrument.
- Publication: the crates.io posture (#5) and the release pipeline plus
  container image (#12).
- A fuzz lane (#11), Rust coverage into Sonar (#9), the path-matching half of
  the changelog guard (#10), and an exerciser for the vendored CKM ADL 1.4 pack
  (#8).

## Gates, all verified green at the migration

```
cargo build --all-targets
cargo clippy --all-targets -- -D warnings
cargo fmt --all --check
cargo nextest run                                   # 352 tests
cargo deny check
cargo run -- validate --root artifacts --specs specs/openehr   # 0 findings
bash scripts/checks/comment-style.sh --all
reuse lint
actionlint · zizmor --min-severity=low
```

CI runs all of them: a guard tier and a workflow audit, ungated, plus a Rust
tier (`fmt`, `clippy`, `test`, `doc`, `msrv`, `deny`, `unused-deps`) gated on
the `changes` job, all behind one required `conclusion` check.

## How to apply

- A change to instrument behaviour lands HERE.
- A spec question is answered from `specs/openehr/` in this repository,
  first-hand. XSD, JSON-Schema and OpenAPI citations resolve against the three
  bundles beside it, because the docs tree carries only prose for those
  components.
- Prose that describes absent machinery is a defect. When something lands, the
  claim about it is corrected in the same change; when something is genuinely
  missing, it says so and names its issue rather than promising a future.

## Ported from the FerroEHR root, and what deliberately was not

Landed: the workspace manifest (single package, own SemVer from
`0.1.0-alpha.1`), `rust-toolchain.toml`, `rustfmt.toml`, `clippy.toml`,
`deny.toml`, `.config/nextest.toml`, `Cargo.lock` (committed — this repository
ships a binary), the Rust CI tier, the `rust` leg of `codeql.yml`, and the Sonar
scan scope with its vendored-tree exclusions.

Adapted rather than copied, because the FerroEHR original assumed machinery that
is not here: `clippy.toml`'s method bans drop `std::env::var` (the instrument
reads credentials from the environment BY DESIGN — the ixit declares only the
variable name, so no secret enters the catalogue) and `uuid::Uuid::new_v4` (no
database keys here); `deny.toml` keeps one advisory ignore of the three, and its
licence exceptions and git allowlist are empty because the crates that needed
them are not in this graph; `.config/nextest.toml` carries no `containers` test
group, because a filter matching no test looks like scheduling discipline and
enforces nothing.

Still absent, each waiting on the thing it configures: `.cargo/config.toml`,
`.devcontainer/`, `.dockerignore` / `.hadolint.yaml` / `.trivyignore.yaml` and
`security/vex/` (with the image lane, #12), `.github/actionlint.yaml` (every job
runs on `ubuntu-latest`), the `cargo` and `docker` dependabot ecosystems,
`.fossa.yml` / `.fossabot.yml`, `.mdbook-lint.toml` (with the docs-site
decision), the remaining `scripts/checks/` guards (spec-citation resolver,
default-value style, typed-status style, SPDX headers), and `.github/actions/`
(no step is repeated often enough yet; adding the directory means extending the
zizmor invocation in the same change).

The OpenSSF Best Practices criteria adjudication stays DEFERRED by owner
decision 2026-08-26 — do not propose statuses until the owner picks it up. Six
of its MUST criteria were waiting on the code (basic and interface
documentation, a build system, a test suite, evidence that tests were added,
compiler warning flags); all six are now discharge-able by pointing at this
tree.

Not applicable, checked and recorded so they are not re-surveyed:
`.editorconfig` (does not exist in FerroEHR), `vercel.json`,
`Dockerfile.vercel`, `docker-compose*.yml`, `deploy/`, `website/` — FerroEHR's
CDR, sandbox and documentation-site surfaces with no counterpart here. A compose
file for a target CDR, if one is ever wanted, is new work rather than a port.
