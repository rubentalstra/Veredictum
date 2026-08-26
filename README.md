<p align="center"><img src="assets/brand/veredictum-icon.svg" width="112" alt="The Veredictum seal"></p>

<h1 align="center">Veredictum</h1>

<p align="center"><em>The independent conformance instrument for openEHR clinical data repositories.</em></p>

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
-->

Point it at a running openEHR CDR and it tells you, with citations, which parts
of the specification that server actually implements.

It executes a machine-readable catalogue of 1103 spec-cited test cases against
the server's own wire, records every exchange, and computes verdicts as pure
functions over what it recorded. Functional conformance, measured performance and
step-load stress come from one tool. The released openEHR specifications are the
only authority it accepts: every expectation in the catalogue names the section
it comes from, so it can be refuted by a better reading of the specification and
by nothing else. The case and binding counts quoted on this page come from the
line `veredictum validate` prints over `artifacts/`, which step 1 below runs.

## Why an independent instrument

A server vendor's own test suite cannot answer the question a hospital is asking.
The suite and the server are written by the same people against the same reading
of the specification, and when the two disagree it is usually the suite that gets
adjusted.

Veredictum is built so that cannot happen here. The vendored specification text
is the oracle and is never a suspect. When a run goes red the failure is
attributed before anything is changed, to exactly one of three suspects, by
comparing what the specification requires against what the catalogue expects
against what the server did:

| Suspect | Fix path |
|---|---|
| **The server under test** violates the specification | a defect report to that CDR, carrying the reproduced exchange and the citation |
| **The instrument** misdrove the case or misjudged the response | fix the runner. Those rows were inconclusive, never failures |
| **The catalogue** expectation is wrong against the specification | fix the artifact, with a new cited source for the corrected expectation |

The instrument is a first-class suspect on every red row, ahead of the server.
The first live triage attributed 7 of 7 diagnosed defects to the runner and none
to the server under test. An instrument that presumes itself correct is worth
nothing to the people who are supposed to rely on its verdicts.

## Run it

Work from a clone. The published crate carries the code, and the catalogue and
the vendored specification oracle are 347 MB of data that no registry accepts —
so `veredictum` reads both as paths you pass it, and the repository is where
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

`cargo run -- --help` lists every subcommand, `perf`, `stress` and `aql-probe`
among them. The toolchain pins itself from `rust-toolchain.toml`; the only extra
tool is `cargo-nextest`, and only if you intend to run the test suite.

### Without a Rust toolchain

The container image carries the runner, so a clone plus Docker is enough. Mount
the repository at `/work` and the arguments are the ordinary subcommands — the
entrypoint is the instrument itself:

```bash
docker run --rm -v "$PWD:/work" ghcr.io/rubentalstra/veredictum:<tag> \
    validate --root /work/artifacts --specs /work/specs/openehr
```

The catalogue and the vendored specification oracle are **not** baked in: they
are 347 MB, the runner reads every root as a path passed at run time, and a
party may legitimately want to point at their own. The image is the runner, and
the data comes from the mount.

Prebuilt binaries for `x86_64` and `aarch64` Linux are attached to each
[release](https://github.com/rubentalstra/Veredictum/releases), each with a
`sha256sum`, a CycloneDX dependency SBOM and a Sigstore bundle:

```bash
gh attestation verify veredictum-<tag>-<target>.tar.gz \
    -R rubentalstra/Veredictum \
    --signer-workflow rubentalstra/Veredictum/.github/workflows/release-build.yml
```

### With cargo

The binary is on crates.io, which is the path to take if you want the command on
your `PATH` and intend to point it at a catalogue you already have:

```bash
cargo install veredictum --version 0.1.0-alpha.3   # pre-release: name the version
veredictum validate --root <catalogue> --specs <spec-tree>
```

The library target is published with it, so an integrator can consume the typed
artifact model and the published JSON Schemas directly rather than reimplementing
the format.

## What is in the box

| | |
|---|---|
| **1103 case cores** | `artifacts/schedule/` — one small isolated case per behaviour, so a red row names one defect. Grouped by chapter: EHR, composition, content, contribution, directory, query, definition, demographic, admin, messaging, security, SMART, simplified formats, system. `schedule/performance/` holds the four measured-workload journey definitions, which are their own family and are not case cores |
| **247 operation bindings** | `artifacts/bindings/` — a case says what the operation IS, in the Service Model's own vocabulary; a binding says how it reaches the wire. A case core carries no status code, header or media type |
| **The vocabularies** | `artifacts/vocab/` — the capability matrix, the wire surface the coverage gate enumerates, the outcome and selector grammars, and the journey catalogue the measured workload decomposes through |
| **The corpora** | `artifacts/corpus/` — payload fixtures with their adjudicated verdicts, plus breadth packs vendored verbatim from upstream libraries. Every invalid shape is kept as a negative case, so a lenient server fails it |
| **The ambiguity register** | `artifacts/registers/ambiguities.yaml` — where the specification is silent or contradicts itself, with a typed disposition. Never a private resolution |
| **The published schemas** | `schemas/` — JSON Schema for every artifact family, emitted and drift-tested, so an integrator can author against the format |
| **The oracle** | `specs/openehr/` — the released specification text, vendored verbatim, plus the released XSD, JSON Schema and OpenAPI bundles a citation resolves against |

## Coverage is a mandate

A green run over a thin catalogue proves nothing, so coverage is machine-checked
rather than asserted. The `surface-coverage` gate enumerates the wire surface
from the released sources alone — the Service Model's platform interfaces crossed
with their ITS-REST branches — and fails on any operation, status-code branch,
header rule, negotiation variant or error family that has neither a covering case
nor a cited exception. A behaviour the specification defines and the catalogue
misses is a gap to close or an honest boundary in the register.

Cases are added. They are never removed to make a run go green.

## Verdicts are computed, never asserted

A verdict is a pure function of the party's statement, the recorded results, the
catalogue and the capability matrix. Performance works the same way: a class
verdict is re-derived from the HDR histograms embedded in the record, so the
stored summary is tamper-checked rather than trusted, and latency is measured
from the planned arrival instant under open-loop offered load, which is what
stops coordinated omission from hiding a stall.

## Origin of the name

*Veredictum* is medieval Latin for "truly spoken", *vere dictum*, and it is the
word that became the English *verdict*. That is what this instrument produces: it
runs the catalogue against a running CDR and speaks a verdict about what it
observed. The seal above is the mark of that verdict.

## Architecture

[`ARCHITECTURE.md`](ARCHITECTURE.md) is the design record, and it is where the
reasoning lives rather than a summary of it: the testable surface and the
case-core field definitions, the per-operation wire bindings, the outcome
taxonomy and the ambiguity register, the assertion vocabulary, how a verdict is
computed, and the population-anchored performance-class model — the POC / S / L /
R volumetric floors derived from OECD, Eurostat and NHS statistics, with the
hospital-simulation journey decomposition behind the measured runs. It also
carries the evidence base for why the instrument exists in this shape: the state
of the official CNF component, how other standards run conformance, and the
ISO/IEC 9646 and CASCO vocabulary the scheme is built in.

## Contributing

[`CONTRIBUTING.md`](CONTRIBUTING.md) has the gates and the review bar.
[`CLAUDE.md`](CLAUDE.md) is the working discipline the project holds itself to,
including the attribution law above. Security reports go through
[`SECURITY.md`](SECURITY.md), and questions through [`SUPPORT.md`](SUPPORT.md).

If you maintain a CDR and want it graded, open an issue. A defect this instrument
finds in your server arrives with the exchange and the citation, and a defect you
find in this instrument is a first-class bug here.

## License

Apache-2.0. Attribution travels with every copy and derivative through the
license and the `NOTICE` file, as its section 4 requires. The vendored
specification text and clinical models keep their upstream terms, recorded per
tree in `PROVENANCE.md` and declared machine-readably in `REUSE.toml`.
