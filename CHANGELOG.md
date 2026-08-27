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

Releases before `0.1.0-alpha.1` carried the repository's identity and its
discipline; the instrument itself builds and runs from this repository from that
version on.

## [Unreleased]

### Added

- **`run --progress` (#81).** One machine-parseable stdout line per processed
  case — `progress: 0/<n>` once the selection is final, then
  `progress: <k>/<n> <case-id>` — line-flushed so a driver reading through a
  pipe sees each case as it happens. Off by default: without the flag,
  existing output is byte-identical. The lib reports the same facts as typed
  `run::Progress` events through a callback on `execute_run`, a peer of the
  warning channel, and a unit test pins the line grammar.
- **The console's run wizard, first half (#65).** Connect: the CDR base URL,
  display name and version label, the authentication choice exactly as the
  ixit's vocabulary (none, basic, bearer), and a probe-before-continue whose
  answer — status line, elapsed, or the transport's own words — renders
  verbatim, with Continue gated on 2xx and a stated "continue anyway".
  Scope: the statement picker over the mounted party declarations, the
  case-id filter, and the honest preview: N cases in scope with the
  per-chapter breakdown, held by a test to what the engine then actually
  processes. Credential values live in the server-side draft and reach only
  the spawned run's environment — the client-safe view carries no secret by
  construction, tested. The probe is the one carved-out console request to a
  CDR (a diagnostic, never a judgement), recorded in the crate's mandates.
- **The console's read surfaces (#64).** The instrument landing shows the
  catalogue's own numbers — case cores, bindings, party statements, findings
  — read once at startup through the published lib, from the same
  expressions the validate summary prints (held by a test), with the mounts
  named and a full-screen explanation when no catalogue is mounted
  (`VEREDICTUM_ROOT` / `VEREDICTUM_SPECS`; the image defaults them to the
  documented `/work` mount). The catalogue explorer walks chapters → cases →
  one case in full: the test purpose, every spec citation verbatim, the
  realizing bindings with their realized/unrealized state, and the corpus
  references — filter, search and paging all in the URL.
- **The console's shell and design system (#63).** The sidebar chrome — the
  seal, one entry per surface, the engine pin and a dark-mode toggle in the
  footer — around every routed page, in the brand palette as semantic design
  tokens (warm paper surfaces, the teal action accent, the orange reserved
  for the running state and the seal, green and red only as verdict
  semantics). The shared kits every screen composes from: page header, stat
  card, surfaces, form controls, empty state, listing table with URL-state
  pagination, toast plus inline message bar, and the verbatim pane with a
  copy affordance. Surfaces still under construction render an honest
  placeholder naming their tracker issue.
- **The signed run record (#62).** `run` and `verdicts` take `--sign-key
  <FILE>`, an armored OpenPGP secret key, with its passphrase read from
  `VEREDICTUM_SIGN_PASSPHRASE`. The emitted documents are then sealed with
  `record-manifest.json`, a byte-deterministic SHA-256 digest manifest
  carrying the instrument's name and version, and `record-manifest.json.asc`,
  a detached RFC 9580 signature over it. The bundle is ordinary files, so
  `gpg --verify record-manifest.json.asc record-manifest.json` accepts it
  without this tool. A new `verify-record --record <DIR> --key <FILE>`
  recomputes every digest the manifest names and checks the signature against
  a supplied public key, printing the signer fingerprint, the signing time and
  one line per file; a mismatch, a missing file or a rejected signature exits
  `1` naming what failed. Every verification prints what the signature does
  not establish: it proves integrity and origin since signing, and says
  nothing about the conditions the run executed under.
- **The console's engine seam (#54).** `app/veredictum-console` consumes the
  instrument as `veredictum = "=0.1.0-alpha.4"` from crates.io — never a path
  dependency — and a console-started run spawns that same pinned binary as a
  subprocess (`engine::Engine`), located on `PATH` or through
  `VEREDICTUM_ENGINE` and refused unless `--version` reports the exact pin.
  Reads parse through the published lib's typed record. SUT credentials reach
  only the spawned run's environment, redacted from every rendering. The
  byte-identity gate runs in CI: the same fixture campaign driven through the
  seam and through the CLI must emit byte-identical `results.json` and
  `run-exceptions.json` — no tolerated delta, since the record carries no
  wall-clock stamp.

### Changed

- **The repository is restructured: both products live under `app/`.** The
  instrument crate moved from the repository root to `app/veredictum` and the
  root manifest is a virtual workspace (#55). Every documented invocation is
  unchanged — the data trees stayed at the root and `cargo run -- <subcommand>`
  still means the instrument. The published crate carries the same files as
  before minus 26 stragglers the old repo-root package had been picking up
  from the vendored trees (their README and LICENSE files), which were never
  part of the crate's declared contents.
- `cargo publish` names its package in both publish lanes: the workspace
  carries the unpublishable console beside the crate, and the bare form
  refused on it — the v0.1.0-alpha.4 publish leg failed on exactly that.

## [0.1.0-alpha.4] - 2026-08-27

### Added

- A committed example results document, `examples/results.example.json`,
  generated by the crate's own machinery (`cargo run --example
  make_example_results`, deterministic): schema-valid, invariant-checked,
  with a real embedded HDR V2 histogram and a verdict computed against the
  catalogue's POC case. It doubles as reader documentation for the results
  schema and as a real seed for the `party_document` and `hdr_v2` fuzz
  targets, which previously started from mutations.
- A deliberate library API, `veredictum::pipeline`, so the engine is
  consumable by something other than the command line. It carries one seam per
  whole operation — `catalogue` validates an artifact tree, `conformance`
  drives it against a running system under test, `judgement` computes the
  verdicts and renders the submission set, `assets` renders the published
  visuals and the schema set, and `measured` runs the class window, the stress
  ladder and the AQL probe. Every seam returns typed values: a validation
  carries its findings and the tree it loaded, a run carries the results
  record and its outcome tally, a judgement carries the verdict report and its
  documents as named bodies, and the measured window reports its progress as
  typed events. Nothing returns console text, so a consumer renders its own
  views over the same facts the command line prints. The `veredictum` binary
  is now a clap front end over exactly those seams; its behaviour, its output
  and its exit codes are unchanged.
- A documentation website at <https://veredictum.eu>, built from `website/` and
  deployed to GitHub Pages by a new `Docs` workflow. The root serves a
  hand-written landing page in the project's own brand palette, and `/docs/`
  serves an mdBook with five chapters: an introduction, installation, running
  the instrument, a command reference covering every subcommand with its real
  flags, the conformance method (the attribution law, positive and negative
  testing, the ambiguity-register lifecycle), and catalogue authoring. The site
  loads nothing from an external host, renders in both light and dark, and takes
  its palette from the brand tokens. `scripts/site/build.sh` assembles the same
  tree locally that the workflow deploys, the `CNAME` for the custom domain
  included. A pull request touching the site builds, lints and link-checks it
  without deploying.
- The vendored CKM ADL 1.4 archetype pack is exercised in this repository, by
  `tests/it/corpus_packs.rs` on every `cargo nextest run`. All 944 ADL 1.4
  exports are decoded as UTF-8 and required to open with an `archetype (…)`
  header declaring `adl_version=1.4` and to declare the archetype id their file
  name carries; all 944 AM 1.4 XML twins are read to end of input and required
  to root at `archetype` in `http://schemas.openehr.org/v1` with that same
  identity; both counts are pinned against the pack's own inventory record. The
  pack had no exerciser here — its only one was an ADL-engine parse gate in the
  repository this instrument was split out of, and this repository ships no ADL
  parser. The pack stays as reserve material for wire batteries the catalogue
  has not authored yet, and the exercise is at the byte level, which is what the
  instrument can perform first-hand.
- The ADL 2 pair pack and the CKM Operational Template breadth pack gain the
  same byte-level exercisers, so every vendored corpus tree in this repository
  now has one. The pair pack's 654 files are all read and refused when empty,
  its 322 ADL 2 sources are checked for `adl_version=2.0.6` and its 330 ADL 1.4
  twins for `adl_version=1.4`, each against the archetype id written inside it,
  and the 321 archetypes upstream published in both dialects are proven to pair
  with a twin in the same directory. The files that do not pair are pinned as
  what they are: one ADL 2 template, which the archetypes-only 1.4 half has
  nothing to hold, and nine ADL 1.4 archetypes this snapshot never converted.
  The template pack's 305 exports are each parsed to end of input and checked
  to root at `template` in `http://schemas.openehr.org/v1` carrying a template
  id, and its file list is compared against the record's own vendored table
  rather than against its count alone.
- A fuzzing lane over the readers that parse text or bytes the instrument did
  not write, in its own nightly `fuzz/` workspace: six libFuzzer targets
  covering the `${…}` reference and identifier grammars, the decision-table
  literal grammar, the citation reader, a case core end to end through YAML and
  the published schema into the typed model, the IXIT, statement and results
  documents a party publishes, and the HDR histogram V2 decode path a measured
  verdict is re-derived from. Seeds come from the catalogue and the party
  declarations already committed here; recorded findings live in
  `fuzz/regressions/` and are re-checked by every run. The harnesses compile on
  the pull-request path as a gating CI job, and a weekly campaign fuzzes each
  target with its corpus kept between runs. `fuzz/README.md` carries the threat
  model and the commands, `.claude/rules/fuzzing.md` the discipline and the
  crash-to-regression-test procedure.
- `veredictum::load::yaml_str_to_value` parses artifact YAML from a string under
  the same budget and duplicate-key refusal the file reader uses, and
  `veredictum::validate` exposes `citation_clauses`, `expand_braces` and
  `section_candidates`, so a consumer can read a citation the way the validator
  does.
- A published VEX record under `security/vex/`, in OpenVEX format: the
  distroless base's adjudicated OpenSSL finding as a hand-authored statement
  beside its `.trivyignore.yaml` twin, and the Rust advisories `deny.toml`
  accepts as a GENERATED document whose id set cannot drift from the gate —
  `scripts/security/vex-generate.sh` refuses on any disagreement and the CI
  guard tier regenerates and diffs on every pull request. The scheduled
  published-image scan applies the documents, and
  `scripts/security/scan-images.sh` reruns that exact scan locally.

### Changed

- **The container image is the web console now.** `ghcr.io/rubentalstra/veredictum`
  ships the new `veredictum-console` Leptos server (`app/veredictum-console`,
  a second workspace package that never publishes to crates.io) instead of the
  CLI, per the ruling recorded in `docker/Dockerfile` when the image first
  shipped: the CLI payload was a placeholder, and its no-toolchain paths are
  `cargo install veredictum` and the attested release binaries. Start the
  console with `docker run --rm -p 127.0.0.1:3000:3000 -v "$PWD:/work"
  ghcr.io/rubentalstra/veredictum:<tag>`; it binds loopback through the
  publish flag because the console has no login. The server answers
  `/healthz`, the image bakes a `HEALTHCHECK` that probes it (the binary is
  its own probe, because distroless carries no curl), and the binary drains
  in-flight requests on SIGTERM, so `docker stop` ends it gracefully. The
  image build properties are unchanged: pushed by digest, smoke-driven and
  scanned before any tag applies, SLSA provenance and an SBOM attested on
  the digest, `:latest` moving only on a release tag.
- The CKM template breadth pack is re-vendored. CKM published new asset
  versions of `ips-problem-list` and `ips-allergies-and-intolerances` on
  2026-08-19, so those two exports carry different bytes. The library is still
  305 vendored templates beside the one private-incubator template that answers
  404 without an account.

### Fixed

- Three ways a document the instrument was JUDGING could stop the instrument,
  all found by the new fuzzing lane on its first local campaign. A
  decision-table cell nesting 4000 lists deep, or chaining 4000 ordinal tuples,
  ran `Literal::from_text` off the stack; a Rust stack overflow aborts rather
  than unwinding, so a validator run died instead of reporting a finding. And a
  113-byte citation carrying 22 `{a,b}` groups in one path hint asked citation
  resolution for four million strings, hanging the run: the 32-variant ceiling
  was applied across a clause's tokens but not within one. Literal nesting is
  now bounded at `literal::MAX_NESTING` and brace expansion at
  `validate::MAX_CITATION_VARIANTS`, both refusing with a typed finding. The
  grammars' own forms are unaffected — a literal reaches three levels and an
  authored shorthand names two or three sibling documents.
- The README quoted 1107 spec-cited cases, which was the file count under
  `artifacts/schedule/`. The instrument reports 1103, because the four
  `schedule/performance/` journey definitions load as measured-workload
  definitions and are not case cores. The page now carries the number
  `validate` prints and says where that number comes from.
- Re-running `scripts/vendor/ckm-archetypes.sh` would have regressed two facts
  in the pack's `PROVENANCE.md`: the corrected mixed-licence count, and now the
  exerciser. The script emits both, so the record survives a refresh.
- The SonarQube lane no longer runs on a Dependabot pull request. `SONAR_TOKEN`
  is an Actions secret and a Dependabot run reads a separate store, so every
  such run failed on the missing secret. The lane is advisory and gates no
  merge, so skipping it costs nothing.

## [0.1.0-alpha.3] - 2026-08-26

### Fixed

- The image vulnerability gate refused to tag the `0.1.0-alpha.2` image, so that
  release published its binaries and its crate but no pullable image tag. The
  finding was real: `libssl3t64` in the distroless base, CVE-2026-14456, HIGH,
  with a Debian fix the base image has not been rebuilt against — the current
  `:nonroot` digest still carries the vulnerable version, so a base bump does not
  resolve it and a distroless image has no package manager to upgrade it in a
  layer of our own.

  It is adjudicated as unreachable rather than suppressed, on the shipped
  binary's own ELF header: its dynamic dependencies are `libgcc_s`, `libm` and
  `libc` only. TLS is rustls and the JOSE signing is aws-lc-rs, so nothing this
  project builds links OpenSSL, and the image is distroless — no shell, no
  package manager, no second executable that could load the library. The entry
  lives in a new `.trivyignore.yaml`, scoped to that one package by PURL, with
  the evidence and a three-month expiry, so it has to be re-argued rather than
  quietly becoming permanent.

### Added

- `scripts/checks/image-labels.sh`, in the ungated guard tier. The base image
  digest is declared in three places — the runtime `FROM`, the Dockerfile's
  `base.digest` label, and the release pipeline's `labels:` input, which is the
  copy the published image actually carries because it overrides the Dockerfile's
  — and an automated base bump edits only the first. Without the guard, merging
  one publishes an image whose `base.digest` names a parent it was not built on.
  It also checks that every shared OCI key agrees between the two declaration
  sites, and refuses to pass vacuously if the publishing lane it expects is
  absent.
- **`ARCHITECTURE.md`** at the repository root: the instrument's design record,
  moved here from the FerroEHR mono-repo where it was written. It is the design
  authority for the machinery — the artifact set and the case-core field
  definitions, the operation bindings, the outcome taxonomy and the ambiguity
  register, the assertion vocabulary, verdict computation — and it carries the
  population-anchored performance-class model with its journey decomposition,
  plus the evidence base and the ISO/IEC 9646 and CASCO grounding the scheme is
  built in. Names and paths were adapted to this tree; the substance is
  unchanged.
- The GHCR image-pulls badge in the README, now that the package exists.
- A Dependabot `ignore` for `rand` major bumps. The dev-dependency exists to hand
  `pgp`'s signing call an RNG, and `pgp 0.20` is on `rand_core 0.6`, so a major
  bump does not compile. Patch and minor bumps within the pin are still proposed,
  and advisory-driven updates are unaffected.

### Changed

- The container image states in its own header that its current payload is a
  placeholder: it ships the CLI today and becomes the web UI's image when that
  lands (#6). The CLI's own distribution channels are `cargo install veredictum`
  and the prebuilt binaries on each release.
- The distroless base moves to the current `:nonroot` digest
  (`sha256:a77defd6…`). This is **not** a security fix — the new digest carries
  the same `libssl3t64` version, verified by scanning it — it is base currency,
  so the image is not built on a two-month-old parent.

## [0.1.0-alpha.2] - 2026-08-26

### Added

- **The release pipeline.** A `v*` tag now publishes a release: `release.yml`
  verifies every release fact against the tagged commit before anything is
  built, creates the GitHub release as a draft, builds per-architecture Linux
  binaries (x86_64 and aarch64) each with a checksum, a CycloneDX dependency
  SBOM and Sigstore provenance and SBOM bundles, attaches a repository-wide
  SPDX SBOM, and publishes the release only once every expected asset is
  attached. The binary and image builds each live in a reusable workflow, which
  is GitHub's documented construction for SLSA Build Level 3, so a consumer can
  pin the signer with `gh attestation verify --signer-workflow`.
- The multi-architecture container image on GHCR, pushed BY DIGEST, smoke-run and
  Trivy-scanned on both architectures before any tag names it, with provenance
  and an SPDX SBOM attested on the digest. `:latest` moves on a release tag and
  never on a pre-release.
- **`docker run` needs no Rust toolchain.** `docker/Dockerfile` builds a
  distroless image that runs as uid 65532 and carries nothing but the runner:
  mount the repository at `/work` and every subcommand works, because the
  entrypoint is the instrument itself.

  ```bash
  docker run --rm -v "$PWD:/work" ghcr.io/rubentalstra/veredictum:<tag> \
      validate --root /work/artifacts --specs /work/specs/openehr
  ```

  The catalogue and the vendored specification oracle are deliberately NOT baked
  in — 347 MB, read as run-time paths, and a party may want to point at their
  own — so the image stays 55 MB and the data comes from the mount.
- A `Dockerfile lint` job in CI, gated on a change to the image tier, running
  hadolint at its warning threshold against a configuration where any
  deliberately violated rule is named with its reason.
- **The changelog guard now requires an entry**, not just a valid file shape. A
  change touching a user-visible surface with no entry under the Unreleased
  heading fails, and the path set that decides "user-visible" is declared in
  `scripts/checks/changelog-entry.sh` beside the reason for each path rather
  than inferred from a pattern in a workflow. The `no-changelog` label waives it
  and says so in the run, so a waived guard is auditable afterwards.
- Two scheduled lanes, both of which report a finding by filing or updating one
  tracking issue and keep the run green — the run goes red only when the probe
  itself cannot answer, because a red scheduled run is invisible to anyone not
  watching the Actions tab:
  - `image-scan.yml`, Mondays, Trivy over the PUBLISHED image on both
    architectures, so a CVE disclosed after a release is still found. Before the
    first release it reports that nothing is published and exits green, so
    "nothing found" and "nothing looked at" are never the same line.
  - `latest-deps.yml`, Mondays, `cargo update` then `cargo check --all-targets`,
    the Cargo book's named mitigation for a committed lockfile: a breaking
    in-range upstream release is found on a schedule instead of during an
    unrelated pull request.
- Dependabot covers the `docker` ecosystem now that a Dockerfile exists, with a
  fourteen-day cooldown — the longest of the three, because a base-image bump
  changes the bytes every user of the published image runs.
- The crate publish joins the same tag, as the last leg of the pipeline and after
  the release is otherwise complete, so the `crates-io` environment's reviewer
  approval blocks nothing else. `publish-crates.yml` stays as the out-of-band
  dry-run and recovery lane, and both lanes call one implementation,
  `scripts/release/publish-crate.sh`.
- The release procedure is written into `CLAUDE.md` and driven by this file: a
  missing or empty section for the tagged version fails the pipeline's `plan` job
  before anything is published.
- The crates.io version, crate-downloads and docs.rs badges in the README.
- **Published on crates.io** as `veredictum`, both a binary and a library:
  `cargo install veredictum --version 0.1.0-alpha.2` puts the command on your
  `PATH`, and the library target lets an integrator consume the typed artifact
  model and the published JSON Schemas rather than reimplementing the format.
  The package carries the code and the legal set; the catalogue and the vendored
  specification oracle are 347 MB of data no registry accepts, and every root is
  a path passed at run time, so both come from the repository.
- `publish-crates.yml`: the release lane for the crate, authenticating through
  crates.io Trusted Publishing so no long-lived registry token exists in this
  repository. Manual dispatch, dry run by default, the upload built from the
  checkout with no cache restored, and the registry read back before the lane
  reports success.
- **The instrument itself builds and runs from this repository:** the runner,
  the catalogue with its 1107 case cores and 247 operation bindings, the
  corpora, the ambiguity register, the party declarations, and the vendored
  openEHR specification text that is its oracle.
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
- `sonar.yml` and `sonar-project.properties`: SonarQube Cloud analysis on every
  pull request and every push to `main`, advisory under
  `.claude/rules/ai-code-review.md`, with the New Code window anchored to the
  package version so "new code" means "since the last release".
- Test coverage, measured and published. The Sonar lane runs the suite under
  `cargo-llvm-cov` and imports the merged lcov; the denominator excludes the
  test tree, the CLI entry point and the two asset renderers, each with its
  reason recorded, because a coverage percentage is only useful if every file
  counted could in principle be covered by a test. The README carries the
  coverage and quality-gate badges beside the CI, CodeQL, reliability, security,
  maintainability and duplication readings.
- Dependabot covers the `cargo` ecosystem, with a seven-day cooldown against
  the actions entry's three: a crate compiles into the published binary, so a
  compromised release reaches every downstream run rather than one CI job.
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

- The SonarQube Cloud lane, which had failed on every run since the code
  migration, the push to `main` included. `sonar.sources=.` and
  `sonar.tests=tests` overlapped, and the scanner refuses an overlap rather than
  picking a side, so one YAML fixture under `tests/fixtures/` ended the analysis
  at exit code 3 — leaving the quality gate and the coverage badge with no
  current reading at all.
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

[unreleased]: https://github.com/rubentalstra/Veredictum/compare/v0.1.0-alpha.4...HEAD
[0.1.0-alpha.4]: https://github.com/rubentalstra/Veredictum/compare/v0.1.0-alpha.3...v0.1.0-alpha.4
[0.1.0-alpha.3]: https://github.com/rubentalstra/Veredictum/compare/v0.1.0-alpha.2...v0.1.0-alpha.3
[0.1.0-alpha.2]: https://github.com/rubentalstra/Veredictum/compare/v0.0.1-alpha.1...v0.1.0-alpha.2
[0.0.1-alpha.1]: https://github.com/rubentalstra/Veredictum/releases/tag/v0.0.1-alpha.1
