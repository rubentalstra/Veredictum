# Introduction

Veredictum is a conformance instrument for openEHR clinical data repositories.
You point it at a running CDR and it tells you, with citations, which parts of
the openEHR specification that server implements.

It executes a machine-readable catalogue of 1139 spec-cited test cases against
the server's own wire, records every exchange, and computes verdicts as pure
functions over what it recorded. Functional conformance, measured performance
and step-load stress come from the one tool.

The released openEHR specifications are the only authority the instrument
accepts. Every expectation in the catalogue names the section it comes from, so
it can be refuted by a better reading of the specification and by nothing else.

## Who this is for

- **Evaluators and procurement teams.** You need a claim about a CDR that
  someone other than its vendor can check. The instrument produces a
  conformance report, a statement of claims and a certificate, all three
  derived from a recorded run rather than asserted.
- **CDR vendors and maintainers.** You want to know where your server diverges
  from the released text before a customer finds out. A failing row arrives
  with the exchange that produced it and the specification section it violates.
- **The openEHR community.** The catalogue is an openEHR conformance test suite
  in machine-readable form. Where the specification is silent or contradicts
  itself, the divergence is recorded in an ambiguity register and reported
  upstream, never resolved privately.

## What is in the box

| | |
|---|---|
| **1139 case cores** | One small isolated case per behaviour, so a red row names one defect. Grouped by chapter: EHR, composition, content, contribution, directory, query, definition, demographic, admin, messaging, security, SMART, simplified formats, system. A separate family holds the four measured-workload journey definitions |
| **249 operation bindings** | A case says what the operation is, in the openEHR Service Model's own vocabulary; a binding says how it reaches the wire. A case core carries no status code, header or media type |
| **The vocabularies** | The capability matrix, the wire surface the coverage gate enumerates, and the outcome and selector grammars |
| **The corpora** | Payload fixtures with their adjudicated verdicts, plus breadth packs vendored verbatim from upstream libraries. Every invalid shape is kept as a negative case, so a lenient server fails it |
| **The ambiguity register** | Where the specification is silent or contradicts itself, with a typed disposition and a link to the upstream report |
| **The published schemas** | JSON Schema for every artifact family, emitted and drift-tested, so you can author against the format |
| **The specification oracle** | The released specification text, vendored verbatim, plus the released XSD, JSON Schema and OpenAPI bundles a citation resolves against |

## Three commands, three stages

The pipeline splits into stages on purpose. Nothing is computed at a stage that
could hide what an earlier stage recorded.

1. `veredictum validate` checks the catalogue itself. Zero findings is the only
   passing result, and it runs before any server is involved.
2. `veredictum run` drives the catalogue against your endpoints and writes
   `results.json`, a record of exchanges and not yet a judgement.
3. `veredictum verdicts` reads that record together with your statement of
   claims and writes the report, the statement and the certificate.

[Installation](installation.md) covers getting the command. [Running the
instrument](running.md) walks the three stages against a live server.

## Coverage is machine-checked

A green run over a thin catalogue proves nothing. A coverage gate enumerates
the wire surface from the released sources alone, the Service Model's platform
interfaces crossed with their ITS-REST branches, and fails on any operation,
status-code branch, header rule, negotiation variant or error family that has
neither a covering case nor a cited exception. A behaviour the specification
defines and the catalogue misses is a gap to close or an honest boundary in the
register.

Cases are added. They are never removed to make a run go green.

> [!NOTE]
> openEHR® is the registered trademark of the openEHR Foundation. Veredictum
> is an independent, community-driven conformance instrument: it names
> openEHR descriptively, to say what is being tested, and it is not an
> official openEHR Foundation product, not the Foundation's CNF program, and
> not endorsed by or affiliated with the Foundation.
