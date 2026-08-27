# Security assurance case

Veredictum publishes verdicts about other people's servers. This page is the
assurance argument behind that claim: what an attacker would want out of the
instrument, where the trust boundaries sit, and which machine check enforces
each security-relevant requirement. It is the document the OpenSSF Best
Practices `assurance_case` criterion asks for. Every row below names a file in
this repository, so each claim can be opened and checked.

Reporting routes, supported versions, and the repository's GitHub security
posture live in [SECURITY.md](SECURITY.md), which this page does not repeat.

## 1. The product, and what an attacker gains

The instrument drives a clinical data repository (CDR) over its own wire,
records the exchanges, and judges them with pure functions against a
specification-cited catalogue. Three assets follow from that.

**Verdict integrity.** A wrong green is the worst outcome the product has. A
party that can influence its own result has defeated the reason the instrument
is independent. The shapes that would do it: tampering with a recorded run,
getting a forged record past a schema, or having a server-controlled input
trusted where it should not be.

**Record integrity.** A published conformance record is evidence a third party
reads months later, so substituted bytes in a record bundle, a release tarball,
or a container image are the same attack at three layers.

**Confidentiality of the operator's inputs.** A run is handed credentials for
the system under test, and the exchanges it records come from a server the
operator does not own. Leaking either into a published artifact, an error
message, or a log line is a vulnerability in this instrument
([SECURITY.md § Scope notes](SECURITY.md#scope-notes)).

## 2. Trust boundaries

| Boundary | Trust | What holds it |
|---|---|---|
| **The catalogue and the vendored specifications** (`artifacts/**`, `specs/**`) | trusted, committed, reviewed | `veredictum validate --root artifacts --specs specs/openehr` is every machine gate over the tree (id uniqueness, citation resolution against the vendored text, binding completeness, wire-surface coverage, claim completeness), and zero findings is the only passing result. Entry point: `validate_tree` in `app/veredictum/src/pipeline/catalogue.rs`. |
| **The system under test** | untrusted by definition | A SUT's responses are evidence in a comparison, never instructions. The attribution law makes this structural: an expectation is refuted by a better reading of the released specification and by nothing else, so a server cannot move the reference it is measured against (`.claude/rules/cnf-triage.md`). |
| **The operator's IXIT and credentials** | trusted input, kept out of records and logs | `AuthMode` in `app/veredictum/src/ixit.rs` holds credential *references* only: environment-variable names for Basic and Bearer, a declared key file for the SMART lane. The IXIT file is committed and shared, so the model makes an inline secret unrepresentable. |
| **The console's HTTP surface** (`app/veredictum-console`) | every endpoint is public | Mandate 3 of `app/veredictum-console/CLAUDE.md`: "Every `#[server]` fn is a publicly reachable HTTP endpoint, and the console has no login by design: it binds `127.0.0.1` by default, wider exposure is the operator's decision with their own gate in front. So every server fn treats input as untrusted and stays under the mounted roots, and SUT credentials … live in memory and the spawned run's environment only — never in client-readable signals, props, or serialized resource data, never in a file, never in a log line." The default bind is `site-addr = "127.0.0.1:3000"` in that crate's `Cargo.toml`; the container image sets `LEPTOS_SITE_ADDR=0.0.0.0:3000` inside its own network namespace (`docker/Dockerfile`). |
| **The published record** | tamper-evident, verifiable without this tool | `app/veredictum/src/record.rs` seals a bundle with a byte-deterministic SHA-256 digest manifest (`record-manifest.json`) and an armored RFC 9580 detached signature over its exact bytes (`record-manifest.json.asc`). `veredictum verify-record --record DIR --key FILE` recomputes both; `gpg --verify record-manifest.json.asc record-manifest.json` accepts the same bundle with no Veredictum binary present. `RecordError::UnsafeFileName` and `RecordError::DuplicateFile` refuse a manifest entry that would read outside the bundle or silently replace another digest. |

Two properties of the record boundary are worth stating outright. The manifest
is byte-deterministic (file names in a `BTreeMap`, JSON key order fixed by field
order), so one signature stays checkable against a regenerated bundle. And
`record::HONESTY_LINE` ships in every verification output: a valid signature
proves integrity and origin since signing, and says nothing about the conditions
the run executed under.

## 3. Requirement, and the check that enforces it

| Requirement | Enforcing check | Source |
|---|---|---|
| No `unsafe`, ever | `unsafe_code = "forbid"` — a `forbid` cannot be relaxed by any attribute | `Cargo.toml` `[workspace.lints.rust]` |
| No panic path in production code | `unwrap_used`, `expect_used`, `panic`, `unimplemented`, `todo`, `unreachable`, `panic_in_result_fn` all at `deny` | `Cargo.toml` `[workspace.lints.clippy]` |
| No panicking indexing on a data path | `indexing_slicing` and `string_slice` at `deny` (UTF-8 boundaries panic, and clinical text is multi-byte) | same table |
| Integer overflow fails loudly instead of wrapping into a wrong verdict | `overflow-checks = true`, with `panic = "unwind"` and `debug = "line-tables-only"` pinned so a release panic names its file and line | `Cargo.toml` `[profile.release]` |
| Errors are typed at every branching boundary | `thiserror` in library code, `anyhow` only in the binary entry point; `map_err_ignore` at warn under CI `-D warnings` | `.claude/rules/reliability.md`; `Cargo.toml` lint tables |
| A suppression cannot accumulate silently | `allow_attributes_without_reason` at `deny`, `allow_attributes` at warn to steer toward `#[expect]`, whose expectation self-reports when it stops being fulfilled | `Cargo.toml` `[workspace.lints.clippy]` |
| Emitted artifacts are byte-reproducible | `iter_over_hash_type` at `deny`; drift-tested snapshots over the emitted schemas and the adjudicated verdicts | `Cargo.toml`; `app/veredictum/tests/it/schema_drift.rs`, `verification_pack.rs` |
| No closed vocabulary silently falls back to a default | every one is an enum or newtype, and an unknown token is a validate-time finding plus a loud drive-time error | `.claude/rules/reliability.md`; `app/veredictum/src/model/**` |
| Readers that parse outside text never panic, abort, or hang | six libFuzzer targets over the party record, the third-party catalogue, and the operator IXIT (`reference_grammar`, `literal_grammar`, `citation`, `artifact_yaml`, `party_document`, `hdr_v2`); the `fuzz-build` CI job compiles every harness on each code pull request, `fuzz.yml` campaigns weekly, and each reproducing input is committed under `fuzz/regressions/<target>/` | `fuzz/README.md`; `.github/workflows/ci.yml`, `.github/workflows/fuzz.yml` |
| Advisories, licenses, and dependency sources are gated | `cargo deny check` (RustSec database, `yanked = "deny"`, plus `[licenses]`, `[bans]`, `[sources]`); every accepted advisory carries a published OpenVEX justification, checked two-way against the ignore set | `deny.toml`; the `deny` job and `scripts/checks/vex-advisories.sh` in `.github/workflows/ci.yml` |
| No dependency is carried that nothing imports | `cargo machete` reads the sources rather than the manifest | the `unused-deps` job, `.github/workflows/ci.yml` |
| The workflows are analysed like code | `zizmor --min-severity=low` over `.github/workflows/` and `.github/actions/` with `GH_TOKEN` set, so the online impostor-commit audit runs; `actionlint` in the digest-pinned official image, which bundles the shellcheck pass over every embedded `run:` block | the `workflow-audit` job, `.github/workflows/ci.yml` |
| Static analysis reads the Rust and the pipeline itself | CodeQL on a `rust` and an `actions` matrix, per pull request and weekly | `.github/workflows/codeql.yml` |
| The supply-chain posture carries a score anyone can read back | OSSF Scorecard weekly, uploading SARIF to code scanning | `.github/workflows/scorecard.yml` |
| The published image carries no known HIGH or CRITICAL vulnerability | Trivy per published platform, with the severity floor and `ignore-unfixed` in one shared config | `trivy.yaml`; `.github/workflows/image-scan.yml` |
| Secrets, shell, and workflow YAML get a second independent sweep | SonarQube Cloud over the whole tree minus the vendored bytes, including secret detection. Advisory by standing rule: it gates no merge and never outranks the specification text or the local gates | `sonar-project.properties`; `.github/workflows/sonar.yml`; `.claude/rules/ai-code-review.md` |
| No CI job runs without gating the merge | `scripts/checks/ci-conclusion-complete.sh` refuses a job missing from the single required `conclusion` check's `needs` list | the `workflow-audit` job, `.github/workflows/ci.yml` |
| Every commit reaching `main` is signed and reviewed through a pull request | the `main` ruleset (signed commits, no force-push, pull request required, strict required checks); the GitHub contents API is banned because it writes unsigned commits | `CLAUDE.md` hard rules 8 and 9; [SECURITY.md § Repository security settings](SECURITY.md#repository-security-settings--the-posture-of-record) |
| No release artifact can be substituted undetected | binaries and images build inside reusable workflows per GitHub's documented SLSA Build L3 construction, each carrying a signed provenance attestation on its digest plus an attested CycloneDX SBOM; `cargo-auditable` embeds the dependency list in the binary; the image publishes by digest before any tag applies; `finalize` refuses to publish until every expected asset is attached | `.github/workflows/release-build.yml`, `build-image.yml`, `release.yml`; the badge rationale in `README.md` |
| A bad release cut cannot be papered over | the `refs/tags/v*` ruleset forbids tag deletion and non-fast-forward updates and requires a signature, so the recovery is the next version | [SECURITY.md § Repository security settings](SECURITY.md#repository-security-settings--the-posture-of-record) |

## 4. Requirements with no machine check

Three security-relevant rules are enforced by review alone. They are listed
here because a rule with no failing check is a wish, and an assurance case that
hides its wishes is worth less than one that names them
(`.claude/rules/reliability.md`).

- **A fallible conversion whose failure means "defective input" must propagate a
  typed error, never become `None`.** `.filter_map(|x| f(x).ok())` and `f(x).ok()?`
  turn an error into a missing element with no trace, which is how a case
  silently drops out of a run or an unparsable response field becomes a passing
  comparison. Clippy has no lint for it, and a grep gate cannot make the
  distinction the rule turns on, because the legitimate shape (a value that is
  genuinely absent) is textually identical. The rule requires a `// NOTE:` at
  every site that converts deliberately.
- **Never log a SUT response body at info level or above.** A customer's CDR can
  hold real data even in a test EHR. There is no logging framework in the
  instrument today (no `tracing` or `log` dependency in
  `app/veredictum/Cargo.toml`), and `print_stdout` and `print_stderr` are denied
  outside the binary's own root, so the surface this rule governs is small and
  reviewable. It is still a review rule.
- **Two GitHub secret-scanning sub-settings are plan-gated off** (non-provider
  patterns and validity checks; both ship as part of GitHub Secret Protection,
  which needs a Team or Enterprise plan this user-owned repository does not
  have — the adjudication and the docs citation are in SECURITY.md's posture
  table and on #115). The credential class they would catch is covered only by
  the baseline detector and push protection until the plan changes.

## 5. What this case does not claim

- **No formal verification.** Nothing here is proved. The argument is a set of
  requirements, each paired with a check that fails on violation, plus the three
  named gaps above.
- **The console has no authentication, by design.** It binds loopback and the
  operator owns any wider exposure with their own gate. Putting the console on a
  reachable interface without one is outside this case.
- **The instrument does not sanitize what it records.** A recorded exchange
  contains whatever the server under test returned. The operator chooses which
  system to point it at and owns the handling of the resulting record, which is
  why `SECURITY.md` says never to run it against a live clinical deployment you
  do not own.
- **A verified signature is not a claim about the run.** `record::HONESTY_LINE`
  states the limit in the verification output itself.

Findings against any statement on this page go through the private reporting
route in [SECURITY.md](SECURITY.md#reporting-a-vulnerability). A claim here that
turns out to be stale is a defect in the assurance case, and it is fixed the
same way any other defect is.
