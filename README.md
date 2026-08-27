<p align="center"><img src="assets/brand/veredictum-icon.svg" width="112" alt="The Veredictum seal"></p>

<h1 align="center">Veredictum</h1>

<p align="center"><em>The independent conformance instrument for openEHR clinical data repositories.</em></p>

<p align="center">
<a href="https://veredictum.eu"><strong>veredictum.eu</strong></a> &nbsp;·&nbsp;
<a href="https://veredictum.eu/docs/">Documentation</a>
</p>

<p align="center">
<a href="https://github.com/rubentalstra/Veredictum/actions/workflows/ci.yml"><img src="https://github.com/rubentalstra/Veredictum/actions/workflows/ci.yml/badge.svg?branch=main" alt="CI"></a>
<a href="https://github.com/rubentalstra/Veredictum/actions/workflows/codeql.yml"><img src="https://github.com/rubentalstra/Veredictum/actions/workflows/codeql.yml/badge.svg?branch=main" alt="CodeQL"></a>
<a href="https://sonarcloud.io/summary/new_code?id=rubentalstra_Veredictum"><img src="https://sonarcloud.io/api/project_badges/measure?project=rubentalstra_Veredictum&metric=alert_status" alt="Quality gate status"></a>
<a href="https://sonarcloud.io/component_measures?id=rubentalstra_Veredictum&metric=coverage"><img src="https://sonarcloud.io/api/project_badges/measure?project=rubentalstra_Veredictum&metric=coverage" alt="Coverage"></a>
</p>

<p align="center">
<a href="https://crates.io/crates/veredictum"><img src="https://img.shields.io/crates/v/veredictum?logo=rust" alt="crates.io"></a>
<a href="https://crates.io/crates/veredictum"><img src="https://img.shields.io/crates/d/veredictum?logo=rust&label=crate%20downloads" alt="crate downloads"></a>
<a href="https://docs.rs/veredictum"><img src="https://img.shields.io/docsrs/veredictum?logo=docsdotrs" alt="docs.rs"></a>
<a href="https://github.com/rubentalstra/Veredictum/pkgs/container/veredictum"><img src="https://img.shields.io/badge/dynamic/json?url=https%3A%2F%2Fghcr-badge.elias.eu.org%2Fapi%2Frubentalstra%2FVeredictum%2Fveredictum&query=downloadCount&label=image%20pulls&logo=github" alt="Image pulls"></a>
</p>

<p align="center">
<a href="https://sonarcloud.io/component_measures?id=rubentalstra_Veredictum&metric=reliability_rating"><img src="https://sonarcloud.io/api/project_badges/measure?project=rubentalstra_Veredictum&metric=reliability_rating" alt="Reliability rating"></a>
<a href="https://sonarcloud.io/component_measures?id=rubentalstra_Veredictum&metric=security_rating"><img src="https://sonarcloud.io/api/project_badges/measure?project=rubentalstra_Veredictum&metric=security_rating" alt="Security rating"></a>
<a href="https://sonarcloud.io/component_measures?id=rubentalstra_Veredictum&metric=sqale_rating"><img src="https://sonarcloud.io/api/project_badges/measure?project=rubentalstra_Veredictum&metric=sqale_rating" alt="Maintainability rating"></a>
<a href="https://sonarcloud.io/component_measures?id=rubentalstra_Veredictum&metric=duplicated_lines_density"><img src="https://sonarcloud.io/api/project_badges/measure?project=rubentalstra_Veredictum&metric=duplicated_lines_density" alt="Duplicated lines"></a>
</p>

<p align="center">
<a href="https://scorecard.dev/viewer/?uri=github.com/rubentalstra/Veredictum"><img src="https://api.scorecard.dev/projects/github.com/rubentalstra/Veredictum/badge" alt="OpenSSF Scorecard"></a>
<a href="https://www.bestpractices.dev/projects/14252"><img src="https://www.bestpractices.dev/projects/14252/badge" alt="OpenSSF Best Practices"></a>
<a href="https://veredictum.eu/docs/installation.html"><img src="https://slsa.dev/images/gh-badge-level3.svg" alt="SLSA Build L3"></a>
<a href="https://doi.org/10.5281/zenodo.22113258"><img src="https://zenodo.org/badge/1347360549.svg" alt="DOI"></a>
<a href="LICENSE"><img src="https://img.shields.io/badge/license-Apache--2.0-46215C" alt="License: Apache-2.0"></a>
<a href="https://github.com/rubentalstra/Veredictum/blob/main/rust-toolchain.toml"><img src="https://img.shields.io/badge/rust-1.97-B7431B?logo=rust&logoColor=white" alt="Rust 1.97"></a>
</p>

<p align="center">
<strong>openEHR ITS-REST 1.1.0</strong> &nbsp;·&nbsp; <strong>AQL 1.1</strong> &nbsp;·&nbsp; <strong>RM 1.2.0</strong> &nbsp;·&nbsp; <strong>1103 spec-cited cases</strong> &nbsp;·&nbsp; <strong>247 operation bindings</strong>
</p>

<!--
Every badge above is a live reading, not a claim. The Sonar badges come from the
analysis lane in .github/workflows/sonar.yml, coverage included; the two OpenSSF
badges are the Scorecard weekly analysis and Best Practices project 14252; the
registry row reads crates.io, docs.rs and the GHCR package directly, so the
version shown is whatever is actually published, the docs badge goes red if a
docs.rs build fails, and the pull count is the package's own. Several read below
their ceiling today, and the scorecard workflow's header says why check by
check — Fuzzing waits on a harness (#11). Those are the honest numbers, and they
are the baseline the next ones are measured against.

The SLSA badge is the one static image here, and its claim is substantiated
rather than asserted: binaries and images build inside reusable workflows
(release-build.yml, build-image.yml) per GitHub's documented SLSA Build L3
construction, each artifact carries a signed provenance attestation on its
digest, and the linked installation page shows the `gh attestation verify`
invocation that checks it.
-->

Veredictum grades openEHR servers. Point it at a running clinical data
repository (CDR) and it tells you, with a specification citation on every
finding, which parts of the released openEHR specifications that server
actually implements, and what load it sustains while doing so.

## What it does

The instrument is one binary plus a data tree. The data tree is a
machine-readable catalogue of 1103 test cases. Each case cites the
specification section it enforces, and the released specification text is
vendored in this repository, so every citation resolves against text you can
read. The case and binding counts on this page are the line
`veredictum validate` prints over `artifacts/`.

A grading run is three commands:

1. **`validate`** checks the catalogue itself before any server is involved:
   id uniqueness, citation resolution against the vendored specs, binding
   completeness, and coverage of the enumerated wire surface. Zero findings
   is the only passing result.
2. **`run`** drives the applicable cases against your server over its own
   REST wire and records every request and response.
3. **`verdicts`** computes the verdict from those recordings and renders the
   report and certificate documents.

Three further subcommands share the same catalogue and recordings
discipline: `perf` measures a hospital-simulation workload against the
performance-class thresholds, `stress` finds the knee of the throughput
curve under stepped load, and `aql-probe` explores a server's AQL behaviour.

`run` and `verdicts` take `--sign-key`, which seals the documents they emit
with a SHA-256 digest manifest and a detached OpenPGP signature over it.
`verify-record` recomputes every digest and checks that signature against a
public key you supply, so a published record is tamper-evident to anyone who
has the key. The bundle is ordinary files, so `gpg --verify` and `sha256sum`
answer the same questions without this tool.
`veredictum --help` lists everything.

## Why an independent instrument

A vendor's own test suite cannot answer the question a hospital procurement
is asking. The suite and the server come from the same people, built on the
same reading of the specification, and when the two disagree it is usually
the suite that gets adjusted.

Veredictum is built so that adjustment has nowhere to happen. The released
specifications are the only authority it accepts: every expectation in the
catalogue names the section it comes from, so it can be refuted by a better
reading of that text and by nothing else. No server's behaviour, no vendor's
documentation, and no stalled upstream test suite ever sets an expected
value. Where the released text is genuinely silent or contradicts itself,
the gap goes to the ambiguity register with a typed disposition and is
reported back upstream. A private resolution never happens.

Every failure is attributed before anything is changed. A red row has
exactly three possible causes, and the instrument itself is a suspect ahead
of the server:

| Suspect | Fix path |
|---|---|
| **The server under test** violates the specification | a defect report to that CDR, carrying the reproduced exchange and the citation |
| **The instrument** misdrove the case or misjudged the response | fix the runner; those rows were inconclusive, never failures |
| **The catalogue** expectation is wrong against the specification | fix the artifact, with a new cited source for the corrected expectation |

The first live triage attributed 7 of 7 diagnosed defects to the runner and
none to the server under test. An instrument that presumes itself correct is
worth nothing to the people who rely on its verdicts.

## Quick start

Work from a clone. The published crate carries the code; the catalogue and
the vendored specification oracle are 347 MB of data no registry accepts, so
`veredictum` reads both as paths you pass it, and this repository is where
they live.

```bash
git clone https://github.com/rubentalstra/Veredictum
cd Veredictum

# 1. Check the catalogue itself. Zero findings is the only passing result.
cargo run -- validate --root artifacts --specs specs/openehr

# 2. Declare your deployment: copy an example and edit the endpoints, the
#    credential variable names and the postures your server actually serves.
cp -r party/ehrbase party/mine

# 3. Drive the catalogue against your running server.
cargo run -- run --root artifacts --ixit party/mine/ixit.json --out out/ \
    --sut-name my-cdr --sut-version 1.2.3 --statement party/mine/statement.json

# 4. Compute the verdicts and render the submission documents.
cargo run -- verdicts --root artifacts --statement party/mine/statement.json \
    --results out/results.json --out out/
```

The toolchain pins itself from `rust-toolchain.toml`. The only extra tool is
`cargo-nextest`, and only if you intend to run the test suite.

### Without a Rust toolchain

Prebuilt binaries for `x86_64` and `aarch64` Linux are attached to each
[release](https://github.com/rubentalstra/Veredictum/releases), each with a
`sha256sum`, a CycloneDX dependency SBOM and a Sigstore bundle:

```bash
gh attestation verify veredictum-<tag>-<target>.tar.gz \
    -R rubentalstra/Veredictum \
    --signer-workflow rubentalstra/Veredictum/.github/workflows/release-build.yml
```

### The web console

The container image is the web console: a browser frontend over the same
instrument, served by its own binary. Start it against a clone and it serves
on port 3000:

```bash
docker run --rm -p 127.0.0.1:3000:3000 -v "$PWD:/work" \
    ghcr.io/rubentalstra/veredictum:<tag>
```

The catalogue and the specification oracle are deliberately not baked into
the image: the instrument reads every root as a path, and a party may
legitimately point at their own. The console has no login, so the publish
flag binds it to loopback; exposing it further is the operator's decision,
behind their own gate. The console is under construction — image tags
published before its first release still carry the CLI as the payload.

### With cargo

Installing from crates.io puts the command on your `PATH`, which is the path
to take if you already have a catalogue checkout to point it at:

```bash
cargo install veredictum --version 0.1.0-alpha.3   # pre-release: name the version
veredictum validate --root <catalogue> --specs <spec-tree>
```

The library target is published with the binary, so an integrator can
consume the typed artifact model and the published JSON Schemas directly
instead of reimplementing the format.

## What is in the box

| | |
|---|---|
| **1103 case cores** | `artifacts/schedule/` — one small isolated case per behaviour, so a red row names one defect. Grouped by chapter: EHR, composition, content, contribution, directory, query, definition, demographic, admin, messaging, security, SMART, simplified formats, system. `schedule/performance/` holds the four measured-workload journey definitions, which are their own family and are not case cores |
| **247 operation bindings** | `artifacts/bindings/` — a case core says what an operation means, in the Service Model's own vocabulary; a binding says how it reaches the wire. A case core carries no status code, header or media type, so a new protocol adds binding files, never a new catalogue |
| **The vocabularies** | `artifacts/vocab/` — the capability matrix behind the CORE, STANDARD and OPTIONS profiles, the wire surface the coverage gate enumerates, the outcome and selector grammars, and the journey catalogue the measured workload decomposes through |
| **The corpora** | `artifacts/corpus/` — payload fixtures with their adjudicated verdicts, plus breadth packs vendored verbatim from upstream clinical-model libraries. Every invalid shape is kept as its own negative case, so a lenient server that accepts it fails |
| **The ambiguity register** | `artifacts/registers/ambiguities.yaml` — every place the specification is silent or contradicts itself, each with a typed disposition and, where we reported it, the upstream issue |
| **The published schemas** | `schemas/` — JSON Schema for every artifact family, emitted by the instrument and drift-tested, so an integrator can author against the format |
| **The verification pack** | `verification-pack/` — a recorded transcript with adjudicated verdicts. A runner claiming to implement this catalogue replays it and must reproduce every verdict, so no harness, this one included, is trusted on its word |
| **The oracle** | `specs/openehr/` — the released specification text, vendored verbatim, plus the released XSD, JSON Schema and OpenAPI bundles a citation resolves against |

## How a verdict is computed

A verdict is a pure function of four inputs: the party's statement (the
capabilities the server claims), the recorded results, the catalogue, and
the capability matrix. Nothing else enters. Two independent runners given
the same four inputs must compute identical verdicts, and the verification
pack exists to check exactly that. A certificate row a human typed is a
defect.

Two verdict machineries share that discipline
([`ARCHITECTURE.md`](ARCHITECTURE.md) §8):

- **Conformance by assertion:** the statement selects the applicable cases,
  typed assertions judge each recorded exchange, and case results roll up
  through capabilities to a profile verdict against the CORE / STANDARD /
  OPTIONS matrix. `NotEvidenced` and `NoCases` are printed as first-class
  results, so a thin claim is visible instead of silently green.
- **Conformance by measurement:** a performance class is earned when every
  threshold holds in one measured run. The class verdict is re-derived from
  the HDR histograms embedded in the record, so a stored summary is
  tamper-checked rather than trusted.

Load is offered open-loop: arrivals follow a seeded schedule of planned
instants, and latency is measured from the planned instant rather than the
actual send. A stalled server therefore accumulates the delay it caused,
which is what stops coordinated omission from hiding a stall behind a
slowed-down client.

The performance classes anchor to population served rather than to a
concurrent-user guess, with the full derivation from OECD, Eurostat and NHS
activity statistics in [`ARCHITECTURE.md`](ARCHITECTURE.md) §8.14:

| Class | Population served | Corpus | Sustained arrival floor | p99 budget | Error rate |
|---|---|---|---|---|---|
| POC | demonstration | 10k EHRs | 2/s | ≤ 1 s | 0 |
| S | 100 thousand | 100k EHRs | 15/s | ≤ 1 s | 0 |
| L | 1 million | 1M EHRs | 150/s | ≤ 1 s | 0 |
| R | 10 million | 10M EHRs | 1,500/s | ≤ 1 s | 0 |

## Coverage is a mandate

A green run over a thin catalogue proves nothing, so coverage is
machine-checked rather than asserted. The `surface-coverage` gate enumerates
the wire surface from the released sources alone, the Service Model's
platform interfaces crossed with their ITS-REST branches, and fails on any
operation, status-code branch, header rule, negotiation variant or error
family that has neither a covering case nor a cited exception. A behaviour
the specification defines and the catalogue misses is a gap to close or an
honest boundary in the register.

Cases are added. They are never removed to make a run go green.

## Lineage

None of the vocabulary here is invented. ISO/IEC 9646 standardized this
architecture in 1991: a supplier's conformance statement (ICS) selects the
applicable cases from an Abstract Test Suite, the supplier's IXIT provides
the instance parameters to run them, and verdicts land in a standardized
report. ETSI, the Bluetooth SIG and USB-IF still run on it. In those terms
the catalogue is the ATS, `statement.json` is the ICS, and `ixit.json` is
the IXIT.

openEHR's own conformance component defined the right concepts and then
stalled: its last content amendment is from March 2022, its assessment layer
was never written, and it carries zero AQL test cases. That component
remains the structural guide for which behaviours need covering. It is never
the correctness authority; the released specifications are.

## Origin of the name

*Veredictum* is medieval Latin for "truly spoken", *vere dictum*, and it is
the word that became the English *verdict*. That is what this instrument
produces: it runs the catalogue against a running CDR and speaks a verdict
about what it observed. The seal above is the mark of that verdict.

## Design record

[`ARCHITECTURE.md`](ARCHITECTURE.md) carries the reasoning rather than a
summary of it: the testable surface and the case-core field definitions, the
per-operation wire bindings, the outcome taxonomy and the ambiguity
register, the assertion vocabulary, the verdict computation, and the
population-anchored performance-class model with its hospital-simulation
journey decomposition. It also records the evidence base for why the
instrument exists in this shape: the state of the official openEHR CNF
component, how other standards run conformance, and the ISO/IEC 9646 and
CASCO vocabulary the scheme is built in.

## Contributing

[`CONTRIBUTING.md`](CONTRIBUTING.md) has the gates and the review bar.
[`CLAUDE.md`](CLAUDE.md) is the working discipline the project holds itself
to, including the attribution law above. Security reports go through
[`SECURITY.md`](SECURITY.md), and questions through
[`SUPPORT.md`](SUPPORT.md).

If you maintain a CDR and want it graded, open an issue. A defect this
instrument finds in your server arrives with the reproduced exchange and the
citation, and a defect you find in this instrument is a first-class bug
here.

## License

Apache-2.0. Attribution travels with every copy and derivative through the
license and the `NOTICE` file, as its section 4 requires. The vendored
specification text and clinical models keep their upstream terms, recorded
per tree in `PROVENANCE.md` and declared machine-readably in `REUSE.toml`.

## openEHR

openEHR® is the registered trademark of the openEHR Foundation. Veredictum
is an independent, community-driven conformance instrument: it names openEHR
descriptively, to say what is being tested against, and it is not an
official openEHR Foundation product, not the Foundation's CNF program, and
not endorsed by or affiliated with the Foundation. The released openEHR
specifications are this instrument's oracle by its own choice, and every
expectation cites them — that fidelity is a design discipline here, never a
claim of official status.
