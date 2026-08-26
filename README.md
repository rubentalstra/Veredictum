<p align="center"><img src="assets/brand/veredictum-icon.svg" width="112" alt="The Veredictum seal"></p>

# Veredictum

[![License: Apache-2.0](https://img.shields.io/badge/license-Apache--2.0-46215C)](LICENSE)
[![DOI](https://zenodo.org/badge/1347360549.svg)](https://doi.org/10.5281/zenodo.22113258)
[![OpenSSF Best Practices](https://www.bestpractices.dev/projects/14252/badge)](https://www.bestpractices.dev/projects/14252)
[![OpenSSF Scorecard](https://api.scorecard.dev/projects/github.com/rubentalstra/Veredictum/badge)](https://scorecard.dev/viewer/?uri=github.com/rubentalstra/Veredictum)
[![Split from FerroEHR](https://img.shields.io/badge/split_from-FerroEHR-B7431B)](https://github.com/rubentalstra/FerroEHR/issues/2789)

<!--
The two OpenSSF badges are live scores, not claims: the Best Practices entry is
project 14252 and reads whatever percentage of the criteria is actually
recorded, and the Scorecard badge is published by the weekly analysis lane in
.github/workflows/scorecard.yml. Both will read low until the code migration
lands, because most of what they measure is a build, a test suite and a release
pipeline that are still in FerroEHR. That is the honest number, and it is the
baseline the next one is measured against. The criteria adjudication itself is
deferred (FerroEHR#2789).
-->


The independent conformance instrument for openEHR clinical data repositories:
a machine-readable catalogue of spec-cited test cases, executed against any
running CDR, judged by pure-function verdicts. Functional conformance, measured
performance, and step-load stress in one tool. The released openEHR
specifications are the only authority it accepts.

> [!NOTE]
> Veredictum is being split out of [FerroEHR](https://github.com/rubentalstra/FerroEHR),
> where it was built and where it runs in production today as `tools/cnf-runner`.
> The migration is tracked in
> [FerroEHR#2789](https://github.com/rubentalstra/FerroEHR/issues/2789); until it
> completes, the living code is there and this repository carries the product's
> identity. Nothing here is ready to consume yet.

## Why "Veredictum"

*Veredictum* is medieval Latin for "truly spoken" — *vere dictum* — and it is
the word that became the English *verdict*. That is exactly what this
instrument produces: it runs a machine-readable catalogue of openEHR
conformance cases against a running CDR and speaks a verdict about what it
observed, as pure functions over the recorded wire exchanges. Nothing in it
takes the server's word for anything, and nothing in it lets the server's
vendor bend the expectation: the released openEHR specifications are the only
authority, and every expectation cites the section it comes from.

The project began inside [FerroEHR](https://github.com/rubentalstra/FerroEHR),
the Rust openEHR CDR, as its conformance instrument — built independent on
purpose, so that the CDR could never grade its own homework. It moved to its
own repository after people across the openEHR community pointed out the same
thing: an independent conformance tool is worth more than any single server,
and none existed.

## License

Apache-2.0. Attribution travels with every copy and derivative through the
license and the NOTICE file, as its section 4 requires.
