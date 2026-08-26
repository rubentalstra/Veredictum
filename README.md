<p align="center"><img src="assets/brand/veredictum-icon.svg" width="112" alt="The Veredictum seal"></p>

<h1 align="center">Veredictum</h1>

<p align="center"><em>The independent conformance instrument for openEHR clinical data repositories.</em></p>

<p align="center">
<a href="LICENSE"><img src="https://img.shields.io/badge/license-Apache--2.0-46215C" alt="License: Apache-2.0"></a>
<a href="https://doi.org/10.5281/zenodo.22113258"><img src="https://zenodo.org/badge/1347360549.svg" alt="DOI"></a>
<a href="https://www.bestpractices.dev/projects/14252"><img src="https://www.bestpractices.dev/projects/14252/badge" alt="OpenSSF Best Practices"></a>
<a href="https://scorecard.dev/viewer/?uri=github.com/rubentalstra/Veredictum"><img src="https://api.scorecard.dev/projects/github.com/rubentalstra/Veredictum/badge" alt="OpenSSF Scorecard"></a>
<a href="https://github.com/rubentalstra/FerroEHR/issues/2789"><img src="https://img.shields.io/badge/split_from-FerroEHR-B7431B" alt="Split from FerroEHR"></a>
</p>

<!--
The two OpenSSF badges are live scores, not claims. Best Practices is project
14252 and reads whatever fraction of the criteria is actually recorded; Scorecard
is published by the weekly analysis lane in .github/workflows/scorecard.yml. Both
read below their ceiling today, and that workflow's header says why check by
check — Packaging and Signed-Releases wait on the release pipeline (#12), Fuzzing
on a harness (#11). Those are the honest numbers, and they are the baseline the
next ones are measured against.
-->

Point it at a running openEHR CDR and it tells you, with citations, which parts
of the specification that server actually implements.

It executes a machine-readable catalogue of 1107 spec-cited test cases against
the server's own wire, records every exchange, and computes verdicts as pure
functions over what it recorded. Functional conformance, measured performance and
step-load stress come from one tool. The released openEHR specifications are the
only authority it accepts: every expectation in the catalogue names the section
it comes from, so it can be refuted by a better reading of the specification and
by nothing else.

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

Nothing is published from this repository yet, so it runs from a checkout. A
published release, a `cargo install` path and a `docker run` image are
[#12](https://github.com/rubentalstra/Veredictum/issues/12) and
[#5](https://github.com/rubentalstra/Veredictum/issues/5).

```bash
git clone https://github.com/rubentalstra/Veredictum
cd Veredictum

# 1. Check the catalogue itself. Zero findings is the only passing result.
cargo run -- validate --root artifacts --specs specs/openehr

# 2. Declare your deployment: copy an example and edit the endpoints, the
#    credential variable names and the postures your server actually serves.
cp -r party/ferroehr party/mine

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

## What is in the box

| | |
|---|---|
| **1107 case cores** | `artifacts/schedule/` — one small isolated case per behaviour, so a red row names one defect. Grouped by chapter: EHR, composition, contribution, directory, query, definition, demographic, admin, messaging, security, SMART, simplified formats, system, performance |
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
observed.

It began inside [FerroEHR](https://github.com/rubentalstra/FerroEHR), the Rust
openEHR CDR, as that project's conformance instrument, built independent from the
start so the CDR could never grade its own homework. It moved to its own
repository after people across the openEHR community pointed out the same thing:
an independent conformance tool is worth more than any single server, and none
existed. The code, its history and its catalogue came with it
([FerroEHR#2789](https://github.com/rubentalstra/FerroEHR/issues/2789)).

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
