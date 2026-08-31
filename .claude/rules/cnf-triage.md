---
paths: ["app/veredictum/src/**", "artifacts/**", "fixtures/**", "schemas/**"]
---

# Red-run triage (the attribution law)

> Ported from FerroEHR's `.claude/rules/cnf-triage.md` at the Veredictum split
> (FerroEHR#2789) and adapted: the paths point at this repository's
> layout, and the first suspect bin is the SUT, which is somebody else's code
> and never edited here.

When a run goes red, the failure is attributed BEFORE anything is changed.
**The vendored released openEHR spec text is always right and is never a
suspect.** Every red row is a defect in exactly one of three bins, two of
which are ours. Run the `cnf-triage` agent, or follow this protocol in
session, for every red row. Never "fix" a red run by guessing which side is
wrong.

**Two reflexes, both banned.** The second one is the reason this product
exists as a separate, independent instrument:

1. **"The catalogue must be wrong, the server is right."** This is the reflex
   a vendor brings to a failing conformance run, and the instrument answers it
   by construction: an expectation traces to a spec citation, so it is refuted
   by a better reading of the released text and by nothing else. Not by a
   maintainer's confidence, not by what every other CDR does.
2. **"The server must be wrong, the instrument is right."** Veredictum is the
   thing people are being asked to trust, so it is a suspect on every red row,
   first-class, ahead of the server. The first live triage run in FerroEHR
   attributed 7 of 7 diagnosed defects to the runner and zero to the server.
   An instrument that presumes itself correct is worth nothing to the people
   who rely on its verdicts.

The reference is the spec, every time. This instrument validates the whole
setup (the SUT AND the runner AND the catalogue) against that spec. It is not
a test suite that presumes its own correctness and questions the server.

## The three suspects (and the only fix path for each)

| Bin | What it means | Fix path |
|---|---|---|
| **SUT** | the server under test violates the spec | a defect report to that CDR's maintainers carrying the reproduced wire exchange and the spec citation. NEVER a change in this repository, and never a catalogue edit that hides it |
| **Runner machinery** | the SUT behaved correctly but `app/veredictum/src/**` misdrove the case or misjudged the response (driver, provisioning, resolver, outcome classification, comparator, verdicts) | fix the runner module; the affected rows were inconclusive, not SUT failures |
| **Catalogue artifact** | the hand-authored schedule is wrong versus the spec (case core, operation binding, corpus, vocabulary) | edit the artifact WITH a new spec-cited source for the corrected expectation |

## The protocol (per red row)

1. Read the observed wire exchange (`results.json` / transcript): what was
   actually sent, what came back, how it was classified.
2. Read the case core and the operation binding it realized through (what the
   catalogue expects, and its cited spec source). Note exactly what it claims
   to expect and why.
3. Read the governing RELEASED spec text FIRST-HAND: the ITS-REST docs text
   plus the overview `Requests_and_responses`, the SM interface, and the
   RM / QUERY / BASE / AM / TERM / ITS-XML sections that apply. The CNF
   schedule is only a GUIDE to WHICH behaviour to check. Re-derive the correct
   answer from the released component, never from the schedule's own
   assertion, the OAS, the Robot set, memory, or any CDR's behaviour.
4. Derive independently what a conformant server must return for the exchange
   that actually happened, then compare three-way: spec-required versus
   catalogue-expected versus SUT-observed. The mismatching side is the defect.
5. An attribution that names the SUT carries a reproduced exchange (`curl`
   against the running SUT) plus the spec citation, file and section, with the
   decisive sentence quoted. A verdict about somebody else's product is held
   to a higher evidence bar than a verdict about our own.
6. Spec silence or genuine ambiguity goes to `artifacts/registers/ambiguities.yaml`
   with a typed `disposition`, never a private resolution.

## Hard rules

- **Never edit the vendored spec text, and never adjust a catalogue
  expectation to match observed SUT behaviour.** Expectations trace to spec
  text only.
- **Only the RELEASED spec components are the oracle.** Adjudicate against
  RM / BASE / AM / QUERY / TERM / ITS-XML / **SM** / ITS-REST **docs text**,
  with one ordered supplement: **the vendored released OAS** is part of the
  release's own specification artifacts (the ITS-REST overview
  `Specifications.md` presents them as its computable artifacts) and grounds
  an expectation **where the docs text is SILENT**. It loses to the docs text
  on any conflict, and an OAS-only ground is always cited AS the OAS, by file
  and element, never passed off as docs text. NEVER treat as authority: the
  CNF Platform Conformance Test Schedule (openEHR CNF never released stable;
  it says which behaviour to test, not the correct answer), or the Robot
  suites and data sets (stalled and in places broken). Where any of these
  conflicts with a released component, the released component wins. An
  "ambiguity" that exists ONLY because a guide source is stale, incomplete,
  or self-contradictory, with no released-component ground, is not a spec gap:
  re-ground it on a released component, or drop it and make the case gating.
  **SM and the ITS-REST docs text are BOTH oracles** — SM anchors the
  operation and the naming the case cores use, ITS-REST binds it to the wire;
  an SM operation the released ITS-REST does not yet realize is a genuine
  SM-to-ITS realization gap (verdict N/A with citation on this ITS, plus an
  upstream alignment candidate), not a refutation of the server.
- **A red run is not presumptive evidence of a SUT bug, nor of a runner bug.**
  Every row gets the full derivation. No attribution without a citation.
- Transport faults and step-resolution failures classify as inconclusive
  runner-side rows, never SUT failures.
- **Ambiguity-register entries are spec-PROVEN, never assumed.** Every entry
  is confirmed first-hand against the released text; the register is a
  suspect like any other catalogue artifact. Re-adjudicate before trusting one
  or attaching an upstream report. A claimed ambiguity the spec actually
  DEFINES is a catalogue defect: remove the entry and make the case gating,
  and do not report it upstream. `report_only` and `editorial` entries MUST
  carry an `upstream_issue` (schema-enforced) so a carried divergence is
  always reported to openEHR, never silently absorbed.
- Standing test discipline applies: never weaken a test or an expectation to
  go green (`.claude/rules/testing.md`).

## Upstream reports

An outbound report of a released-spec defect, contradiction, or silence is a
**GitHub issue labeled `upstream-report`**, never a markdown ledger file. One
issue per defect; the register entry points at it via `upstream_issue: <number>`;
the narrative lives only on the issue.

- **Shape**: a plain opening summary, then `## What the released spec says`
  with citations and quotes, `## What this implementation does` with the
  register disposition, `## Resolution sought upstream`. Never ticket-draft
  framing, and no Channel/Status/Ask fields.
- **Grounding**: docs text first. The released OAS is citable only where the
  docs text is silent, is always cited as the OAS, and loses every conflict. A
  behaviour the OAS DEFINES is not a reportable silence, and a "defect" that
  exists only because a stalled guide source is wrong has no
  released-component ground and is not reportable.
- **Lifecycle — verification is TERMINAL.** A new report is created UNVERIFIED
  and enters the current verification milestone. Once re-verified first-hand
  as genuine it gains `upstream-confirmed`, and once its divergence is fully
  adjudicated here, with nothing further pending on our side, it is CLOSED as
  the standing outbound record: the closed issue stays the durable, linkable
  target of the register's `upstream_issue`, so nothing is silently absorbed
  by closing, and the open set stays near zero by design. A confirmed report
  stays open only while something here is genuinely blocked on it, expressed
  as a native `blocked-by` edge. When it turns out to be a docs misreading it
  closes as refuted, its register entry is removed or re-grounded, and the
  affected case becomes gating. When a report is filed on an openEHR channel
  (Jira, or the spec repository), the returned key is recorded on the issue
  without reopening it; when upstream later resolves one, file the inbound
  `spec-update` issue with the closed report as its provenance, and reopen
  only if the resolution requires changes here.
- **Labels**: `upstream-report` plus `spec:<component>`, and
  `upstream-confirmed` once verified. A work item here waiting on a reported
  defect adds a native `blocked-by` edge to the report issue.
