<p align="center"><img src="https://raw.githubusercontent.com/rubentalstra/Veredictum/main/assets/brand/veredictum-icon.svg" width="112" alt="The Veredictum seal"></p>

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
<a href="https://github.com/rubentalstra/Veredictum/pkgs/container/veredictum"><img src="https://img.shields.io/badge/ghcr.io-veredictum-2496ED.svg?logo=docker&logoColor=white" alt="GHCR"></a>
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
<a href="https://github.com/rubentalstra/Veredictum/blob/main/LICENSE"><img src="https://img.shields.io/badge/license-Apache--2.0-46215C" alt="License: Apache-2.0"></a>
<a href="https://github.com/rubentalstra/Veredictum/blob/main/rust-toolchain.toml"><img src="https://img.shields.io/badge/rust-1.97-B7431B?logo=rust&logoColor=white" alt="Rust 1.97"></a>
</p>

<p align="center">
<strong>the released openEHR specifications, version-aware per case</strong> &nbsp;·&nbsp; <strong>1146 spec-cited cases</strong> &nbsp;·&nbsp; <strong>249 operation bindings</strong>
</p>

<!--
Every badge above is a live reading, not a claim. The Sonar badges come from the
analysis lane in .github/workflows/sonar.yml, coverage included; the two OpenSSF
badges are the Scorecard weekly analysis and Best Practices project 14252; the
registry row reads crates.io, docs.rs and the GHCR package directly, so the
version shown is whatever is actually published, the docs badge goes red if a
docs.rs build fails, and the pull count is the package's own. Several read below
their ceiling today, and the scorecard workflow's header says why check by
check — Fuzzing reads low by Scorecard's own detection (it looks for OSS-Fuzz
and the integrations it knows), while the harnesses exist under fuzz/ (#11):
CI compiles them per pull request and campaigns weekly. Those are the honest
numbers, and they
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
actually implements, what load it sustains, and how fast it answers.

It ships as two products over one engine:

- **The instrument CLI** — the `veredictum` command, installed from
  [crates.io](https://crates.io/crates/veredictum) or taken as a signed
  release binary. Every verdict this repository speaks is a run of it.
- **The web console** — the container image at
  [ghcr.io/rubentalstra/veredictum](https://github.com/rubentalstra/Veredictum/pkgs/container/veredictum),
  a browser frontend that drives the same pinned CLI underneath: connect a
  CDR, paste the vendor's claim, watch the run live, read the results and
  the verdicts. The image is the console, never the CLI — a static binary
  needs no container.

[console.veredictum.eu](https://console.veredictum.eu) is the official
conformance instrument, and a run performed there is an official run. It
drives the catalogue against the endpoint you name, records every exchange,
computes the verdicts, and submits the whole record as a pull request. CI
recomputes those verdicts from the transcript the submission carries, refuses
any mismatch, and signs the record with a key the instance never holds. The
merge publishes it.

What that establishes is threefold: the run was performed by an instrument
nobody in the exchange controls, its judgement is arithmetic anyone can
repeat, and the bytes have not moved since we repeated it. What it cannot
establish is the environment — you chose the endpoint, and the record says
so. Running the instrument yourself publishes your own claim instead, which
is a different question rather than a weaker answer, and the section below is
how. The delivery pipeline behind the hosted instrument is documented in
[deploy/hosted/README.md](deploy/hosted/README.md).

Both are pre-1.0. The 0.1.x line publishes working releases and makes no
API-stability claim yet; every claim a release does make is checked, signed
and reproducible, which is the stability that matters for a verdict.

## What it does

The instrument is one binary plus a data tree. The data tree is a
machine-readable catalogue of 1146 test cases. Each case cites the
specification section it enforces, and the released specification text is
vendored in this repository, so every citation resolves against text you can
read. The case and binding counts on this page are the line
`veredictum validate` prints over `artifacts/`, and a CI guard fails the
build when a count here disagrees with the catalogue.

One command surface answers four different questions about a server:

| Question | Commands |
|---|---|
| **Does it conform?** | `validate` checks the catalogue itself before any server is involved; zero findings is the only passing result. `run` drives the applicable cases against the server over its own REST wire and records every request and response. `verdicts` computes the verdict from those recordings, as a pure function, and renders the report and certificate documents. |
| **What class does it sustain?** | `perf` seeds the population-scale corpus and holds a class's offered load for the sustained window, open-loop, merging the measured record into the results. |
| **Where does it break?** | `stress` steps load up to the maximum sustainable throughput; `stress-compare` overlays two committed stress reports; `aql-probe` explores AQL behaviour over the seeded corpus with per-statement database attribution. All three are exploration and never produce a conformance record. |
| **How fast is it?** | `bench` runs an embedded benchmark pack against any reachable CDR from a base URL and credentials; `bench-compare` aligns committed results into one table; `bench-packs` writes the byte-deterministic description of what every pack runs. Comparative speed only, never a conformance verdict. |

Three more subcommands serve the records themselves: `verify-record` checks
a sealed bundle, `emit-schemas` writes the published JSON Schemas, and
`perf-assets` / `conformance-assets` render the published charts from
committed artifacts. `veredictum --help` is the authoritative list.

`run` and `verdicts` take `--sign-key`, which seals the documents they emit
with a SHA-256 digest manifest and a detached OpenPGP signature over it.
`verify-record` recomputes every digest and checks that signature against a
public key you supply, so a published record is tamper-evident to anyone who
has the key. The bundle is ordinary files, so `gpg --verify` and `sha256sum`
answer the same questions without this tool.

## Quick start

The fastest path installs nothing:
[console.veredictum.eu](https://console.veredictum.eu) is the official
instrument described above, and a run there produces an official record. To
publish your own claim, or to work offline, run the instrument yourself —
fastest first: the console with `docker compose up`, the CLI from cargo, the
signed bare-metal binaries. Grading a server end to end needs the catalogue,
which lives in this repository — that path closes the section.

### docker compose up — the console

```bash
curl -LO https://raw.githubusercontent.com/rubentalstra/Veredictum/main/docker/docker-compose.yml
docker compose up
```

Open <http://127.0.0.1:3210>. The image carries the console and the pinned
engine; started beside an empty directory it comes up and says what it is
missing, and started beside a checkout of this repository it reads the
catalogue, the specification oracle and the party declarations from the
mount. The console has no login, so the compose file binds it to loopback;
exposing it further is the operator's decision, behind their own gate. From
the next release onward the same file also sits in the release assets,
pinned to that release's image.
[The console chapter](https://veredictum.eu/docs/console.html) shows what it
does today.

### cargo install

```bash
cargo install veredictum
```

That puts the `veredictum` command on your `PATH`. The library target is
published with the binary, so an integrator can consume the typed artifact
model and the published JSON Schemas directly instead of reimplementing the
format.

### Bare-metal binaries

Prebuilt binaries for `x86_64` and `aarch64` Linux are attached to each
[release](https://github.com/rubentalstra/Veredictum/releases), each with a
`sha256sum`, a CycloneDX dependency SBOM and a Sigstore bundle you can check:

```bash
gh attestation verify veredictum-<tag>-<target>.tar.gz \
    -R rubentalstra/Veredictum \
    --signer-workflow rubentalstra/Veredictum/.github/workflows/release-build.yml
```

### Benchmark a CDR in one command

The benchmark needs no clone and no declaration files: the packs are
embedded in the binary, pinned by digest, and described by `bench-packs`.

```bash
# The credential is read from the environment. It never rides argv.
export VEREDICTUM_BENCH_PASSWORD=…

veredictum bench \
  --base-url https://cdr.example/openehr/v1 \
  --auth basic --user <user> \
  --pack community-vitals \
  --repetitions 3 \
  --with-baselines \
  --out ./bench \
  --label "Your CDR 1.2.3"
```

`community-vitals` reproduces the openEHR community's vital-signs harness
and then measures the same population a second way, open-loop, so a stall
shows up in the percentiles instead of quietly reducing the request count.
`--with-baselines` composes the two pinned reference CDRs, EHRbase and
FerroEHR, from image digests on your machine and drives the same pack at the
same seed against each, so the record carries one relative index per
reference — the only kind of number that means anything across machines. A
declared posture profile is checked by canaries on both sides of the
measured window, and a run whose deployment disagrees with its declaration
is refused rather than recorded.
[`benchmarks/SUBMITTING.md`](https://github.com/rubentalstra/Veredictum/blob/main/benchmarks/SUBMITTING.md)
takes the record from there to the public board.

### Run the full conformance catalogue

A conformance run reads the catalogue and the vendored specification oracle
as paths. The published crate carries the code; those two trees are over
300 MB of data no registry accepts, and this repository is where they live —
so grading a server starts from a clone:

```bash
git clone https://github.com/rubentalstra/Veredictum
cd Veredictum

# 1. Check the catalogue itself. Zero findings is the only passing result.
veredictum validate --root artifacts --specs specs/openehr

# 2. Declare your deployment: copy an example and edit the endpoints, the
#    credential variable names and the postures your server actually serves.
cp -r party/ehrbase party/mine

# 3. Drive the catalogue against your running server.
veredictum run --root artifacts --ixit party/mine/ixit.json --out out/ \
    --sut-name my-cdr --sut-version 1.2.3 --statement party/mine/statement.json

# 4. Compute the verdicts and render the submission documents.
veredictum verdicts --root artifacts --statement party/mine/statement.json \
    --results out/results.json --out out/
```

No installed binary? `cargo run -- <subcommand> …` from the clone does the
same; the toolchain pins itself from `rust-toolchain.toml`, and the only
extra tool is `cargo-nextest`, only if you intend to run the test suite.

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

## The public results registry

Published results live in this repository as one append-only tree,
conformance runs on [one board](https://veredictum.eu/conformance-board.html)
and benchmark runs on [another](https://veredictum.eu/benchmarks.html). A
submission is a pull request that adds one entry, CI validates it before
anybody reads the numbers, and the merge is the publication. Every entry
records who submitted it, their relationship to the system, the deployment
with its image digests, the instrument version, the machine, and the
artifacts it stands on by digest.

Every entry carries one of three tiers, and the tier is a property of who
performed the run. A **reproduced** entry was produced by this repository's
own workflow: it composed the deployment from a recipe committed under
`registry/topologies/`, drove the catalogue against it, and attested the
bundle. A **console** entry was produced at
[console.veredictum.eu](https://console.veredictum.eu), the official hosted
instrument, against an endpoint the submitter named that is **reachable from the
public internet**; its verdicts were re-derived here from the transcript it
submitted, and the record was signed only after they matched. A **self-reported** entry was run and signed by its
submitter; the signature proves who submitted the file and that the bytes
have not moved, and it never proves the run happened as described. The tier
is the discriminant of the entry's provenance block, so it cannot be claimed
without the evidence its variant requires.

**A deployment the internet cannot reach cannot earn a console entry.** The
hosted instrument refuses a target only it could reach (loopback, RFC 1918,
link-local, unique-local) before a socket opens, because otherwise a visitor
could point it at its own host network. That leaves a server behind a firewall,
on a laptop, or in a private subnet with two honest routes: the **reproduced**
tier, if the deployment can be composed from a recipe committed under
`registry/topologies/`, which is the stronger of the two because this repository
performs the run; or **self-reported**, which says what the submitter says it
says. The guard is not a limitation to be worked around. It is the reason the
instrument cannot be turned against the network it runs in.

**A test report is not a certificate.** An entry says what happened when a
named version of a named system was driven by a named version of this
instrument on a named machine. Certification is the openEHR Foundation's to
grant, and the registry is deliberately shaped to hand over: the rules
([`registry/RULES.md`](https://github.com/rubentalstra/Veredictum/blob/main/registry/RULES.md), versioned, changed prospectively)
are public, the entries carry their own evidence, and no step of the
pipeline is proprietary.

## Where the official instrument runs

An instrument that grades other people's products in public states its own
conditions in public.

| | |
|---|---|
| Provider | Hetzner Cloud |
| Location | Nuremberg, Germany, network zone `eu-central` |
| Machine | CPX12: 1 vCPU, 2 GB RAM, 40 GB local disk |
| Price | €13.90 per month, from the account's own record on 2026-08-31 |
| Paid by | The maintainer, out of pocket. No vendor funds the instrument that grades them |
| Concurrent runs | One |

The cap comes from the machine. An engine process loads the whole catalogue, and
it shares 2 GB with the console, the proxy and the operating system. A second
concurrent run would be the OOM killer ending a conformance run halfway
through, which looks exactly like a defect in the instrument. The
number lives in one place, `VEREDICTUM_MAX_CONCURRENT_RUNS` in the box's
environment file, beside the container's memory limit. Raising it is an
environment edit and a redeploy, never a release, so a bigger machine changes
what the instrument admits without changing a line of code.

**The host holds no signing key, in any form, at any step.** A record produced
there is signed only after this repository's CI has re-derived its verdicts from
the submitted transcript, in an environment the host cannot reach. What the box
can do is drive the catalogue, record the exchanges and open a pull request.
[`registry/RULES.md`](https://github.com/rubentalstra/Veredictum/blob/main/registry/RULES.md)
states what a console entry attests and what it cannot.

The box is disposable. It stores nothing durable, git is where a record lives,
and the whole posture is committed under
[`deploy/hosted/`](https://github.com/rubentalstra/Veredictum/tree/main/deploy/hosted):
the cloud-init that built it, the compose file, the proxy configuration. A
rebuild from that directory produces the same machine.

**Who can change the thing that judges.** A record is trusted because this
repository's CI re-derived its verdicts from the submitted transcript and then
signed them, and the lane that does both lives on `main`. So who can move `main`
is part of the trust claim, and the answer is published rather than implied.

Every change to `main` requires a pull request, with its diff, its CI run and its
audit-log entry. The maintainer holds a pull-request-only bypass, which means
they may merge their own pull request without a second reviewer, and may merge
one whose checks have not passed. They cannot push to `main` at all. Signed
commits, no force-pushes and no branch deletion are enforced with no exception
for anyone. The registry signing key sits behind a second gate: an environment
that admits only `main` and holds the job until a person releases it.

For a project with one maintainer, a second reviewer is a promise nobody could
keep. A visible object for every change is one that can be kept, and it is the
one that matters when somebody wants to audit how a verdict was produced.

**One measurement caveat, stated before anyone reads a number.** The hosted
instrument performs functional conformance runs. It does not offer the measured
instruments: `perf`, `stress` and the comparative benchmark stay local for now.
A latency measured from a fixed origin in Nuremberg carries the network path to
wherever the system under test lives, and distance is not a property of the
database being graded. Measured runs are driven by the operator, on hardware
they describe.

## What is in the box

| | |
|---|---|
| **1146 case cores** | `artifacts/schedule/` — one small isolated case per behaviour, so a red row names one defect. Grouped by chapter: EHR, composition, content, contribution, directory, query, definition, demographic, admin, messaging, security, SMART, simplified formats, system. `schedule/performance/` holds the four measured-workload journey definitions, which are their own family and are not case cores |
| **249 operation bindings** | `artifacts/bindings/` — a case core says what an operation means, in the Service Model's own vocabulary; a binding says how it reaches the wire. A case core carries no status code, header or media type, so a new protocol adds binding files, never a new catalogue |
| **The vocabularies** | `artifacts/vocab/` — the capability matrix behind the CORE, STANDARD and OPTIONS profiles, the wire surface the coverage gate enumerates, the outcome and selector grammars, and the journey catalogue the measured workload decomposes through |
| **The corpora** | `artifacts/corpus/` — payload fixtures with their adjudicated verdicts, plus breadth packs vendored verbatim from upstream clinical-model libraries. Every invalid shape is kept as its own negative case, so a lenient server that accepts it fails |
| **The ambiguity register** | `artifacts/registers/ambiguities.yaml` — every place the specification is silent or contradicts itself, each with a typed disposition and, where we reported it, the upstream issue |
| **The results registry** | `registry/` and `benchmarks/` — the versioned submission rules, the deployment topologies the reproduction lane composes, and the committed entries the public boards render from |
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
([`ARCHITECTURE.md`](https://github.com/rubentalstra/Veredictum/blob/main/ARCHITECTURE.md) §8):

- **Conformance by assertion:** the statement selects the applicable cases,
  typed assertions judge each recorded exchange, and case results roll up
  through capabilities to a profile verdict against the CORE / STANDARD /
  OPTIONS matrix. Version selection lives in the same two documents: each
  case declares the spec-version ranges it applies to, the statement declares
  the versions the product implements, and a case outside the declared
  versions is out of scope — the instrument is version-aware per case, never
  fixed to one release. `not_evidenced` and `not_claimed` are printed as
  first-class results, so a thin claim is visible instead of silently green.
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
activity statistics in [`ARCHITECTURE.md`](https://github.com/rubentalstra/Veredictum/blob/main/ARCHITECTURE.md) §8.14:

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

[`ARCHITECTURE.md`](https://github.com/rubentalstra/Veredictum/blob/main/ARCHITECTURE.md) carries the reasoning rather than a
summary of it: the testable surface and the case-core field definitions, the
per-operation wire bindings, the outcome taxonomy and the ambiguity
register, the assertion vocabulary, the verdict computation, and the
population-anchored performance-class model with its hospital-simulation
journey decomposition. It also records the evidence base for why the
instrument exists in this shape: the state of the official openEHR CNF
component, how other standards run conformance, and the ISO/IEC 9646 and
CASCO vocabulary the scheme is built in.

## Contributing

[`CONTRIBUTING.md`](https://github.com/rubentalstra/Veredictum/blob/main/CONTRIBUTING.md) has the gates and the review bar.
[`CLAUDE.md`](https://github.com/rubentalstra/Veredictum/blob/main/CLAUDE.md) is the working discipline the project holds itself
to, including the attribution law above. Security reports go through
[`SECURITY.md`](https://github.com/rubentalstra/Veredictum/blob/main/SECURITY.md), and questions through
[`SUPPORT.md`](https://github.com/rubentalstra/Veredictum/blob/main/SUPPORT.md).

If you maintain a CDR and want it graded, open an issue. A defect this
instrument finds in your server arrives with the reproduced exchange and the
citation, and a defect you find in this instrument is a first-class bug
here.

The [public roadmap board](https://github.com/users/rubentalstra/projects/5)
shows what is planned, in progress, and shipped — a view over the issue
tracker, where milestones are releases.

## Credits

The conformance work here stands on work other people did first. Each entry
below says what that work contributed to this catalogue.

- **The openEHR SEC and the CNF authors:** the
  [Conformance component](https://specifications.openehr.org/releases/CNF/development),
  whose Conformance Guide and Platform Conformance Test Schedule set the SUT
  model, the profile matrix and the certificate shape, and say which
  behaviours a platform product has to be tested for. The schedule's
  amendment record names T Beale, B Naess, I McNicoll, C Chevalley,
  H Frankel, S Iancu, B Lah and W Wagner across its revisions, beside
  P Pazos. 349 of the 1146 case cores here cite one of its Test Schedule
  chapters.
- **Pablo Pazos (CaboLabs):** the fleshed EHR, COMPOSITION, CONTRIBUTION and
  DIRECTORY chapters of that schedule, which are its usable core. The
  amendment record names him as the raiser of Test Schedule revisions 0.8.0
  (23 Nov 2021) through 0.8.6 (24 Mar 2022), and as co-author of the
  Conformance Guide's initial writing with T Beale. He wrote the original
  2019 EHRbase conformance tests at Hannover Medical School, and his
  [openEHR conformance verification framework](https://github.com/ppazos/openehr-conformance-verification)
  is the expanded continuation of that work, carrying a conformance testing
  specification of its own. He has argued the case for openEHR conformance
  testing on the community forums for years. 127 of the 1146 case cores cite
  the four chapters those revisions wrote.
- **The EHRbase and vitasystems team:** the executable battery. The 223 Robot
  files the CNF component vendored name Wladislaw Wagner (Vitasystems GmbH),
  Pablo Pazos and Jake Smolka (Hannover Medical School) in their copyright
  headers, and the team maintains that set as its
  [integration tests](https://github.com/ehrbase/integration-tests). 156 of
  the 462 corpus provenance records here name that set as the source of the
  entry's bytes or of its template skeleton, each one re-adjudicated against
  the released specifications.
- **The openEHR Foundation:** the
  [released specifications](https://specifications.openehr.org/) every
  expectation in the catalogue cites, and the machine-readable artifacts the
  bindings resolve against: the ITS-XML and ITS-JSON schema bundles and the
  ITS-REST OpenAPI documents.

The Test Schedule chapters are cited as the structural guide to which
behaviours need covering. The correctness authority is always the released
specification a case cites.

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
