# Veredictum AI Statement

| | |
|---|---|
| Version | 1.0.0 |
| Effective date | 2026-08-26 |
| Status | Active |
| Author and owner | Ruben Talstra, maintainer |
| Canonical location | `AI_STATEMENT.md` at the repository root |
| Licence | Apache-2.0, like the rest of the project's own text |
| Review | at every major or minor release, and on any trigger in §13 |

**Abstract.** This document discloses how artificial-intelligence tools are used
to develop Veredictum, an independent conformance instrument for openEHR
clinical data repositories. It states what the tools do and do not touch, who is
accountable, which controls bound the work and how each is enforced, the
licensing and data posture, the rules for contributors, the uses that are
prohibited, and the limitations that survive all of it. It is a self-declaration
by the maintainer, written for the people who have to decide whether a verdict
from this instrument is worth relying on, and it changes in the same pull
request that changes the practice it describes.

The key words **shall**, **should**, and **may** are used as ISO/IEC Directives
Part 2 defines them: requirement, recommendation, permission.

## 1. Scope

This document covers the use of AI tools in developing everything in this
repository: the runner machinery, the conformance catalogue, the schemas, the
tests, the infrastructure, the documentation, and this document itself. It
covers the whole history in this repository, including the years of work that
predate its first public commit.

It does not cover an AI system in the product, because there is none.
**Veredictum ships no AI.** No model is trained, embedded, or called at run
time. A verdict is a pure function over recorded wire exchanges: the same
records produce the same verdict on any machine, forever, with no inference
anywhere in the path. That is a property a reader can check in the code, and it
is deliberate. An instrument whose judgement came from a model would be an
instrument nobody could re-derive. AI is used to *build* this software, in the
same sense a compiler is used to build it.

## 2. Which frameworks apply here, and which do not

Stated plainly, because borrowed authority is worse than none.

- **The EU AI Act imposes no obligation on this project.** The Act binds
  providers and deployers of AI *systems* (Articles 2 and 3(1)); Veredictum is
  not one. Article 50's marking duties bind the AI tool's provider, not the
  tool's user, and the European Commission's Article 50 FAQ places source code
  outside the content-marking obligation. This document is voluntary.
- **This is not a medical device, and it is not a conformity-assessment body.**
  Veredictum is software test tooling: it exercises a server's API and records
  what happened. It has no medical purpose, processes no patient data, and
  issues no certification. It is not accredited under ISO/IEC 17025 or
  ISO/IEC 17065, no notified body is involved, and a verdict from it carries no
  regulatory standing. A vendor who cites a Veredictum result inside their own
  technical documentation is making their own claim, under their own quality
  system, and this document exists partly so they can answer their own supplier
  questions about how the instrument was built.
- **No openEHR Foundation endorsement is claimed.** The Foundation's published
  specifications are the authority this instrument reads. The Foundation has
  not reviewed, approved, or accredited it.
- **ISO/IEC 42001 and the NIST AI RMF are used as vocabulary, not claimed as
  conformity.** No certification is claimed, no audit has occurred, and the
  words "certified", "audited", and "validated" appear in this document only
  inside this sentence, to say they do not apply.

## 3. Terms

This document reuses the W3C AI Content Disclosure vocabulary rather than
inventing one: **none** (entirely human-authored), **ai-assisted**
(human-authored; AI edited, refined, or filled in boilerplate),
**ai-generated** (AI-generated with human prompting and review), **autonomous**
(AI-generated without meaningful human oversight). An **agentic tool** is one
that plans and executes multi-step work, editing files and running builds and
tests, under a human's direction, as opposed to inline completion.

## 4. Accountability

One named human, the maintainer listed in [MAINTAINERS.md](MAINTAINERS.md), is
the author of and accountable for every change in this repository, whatever tool
produced the bytes. A tool **shall not** be named as an author, co-author, or
signer of anything here, because a tool cannot be responsible for accuracy,
integrity, or originality, and responsibility that cannot be borne cannot be
assigned. There is no AI-issued sign-off of any kind.

## 5. Where AI is used, and at what level

The tooling is agentic AI coding assistance (currently Claude Code, by
Anthropic), operated in sessions the maintainer directs, reviews, and merges.
Levels below use the §3 vocabulary. Deliberately, no percentage appears anywhere
in this document: no defensible method exists for measuring one.

| Activity | Level | Notes |
|---|---|---|
| Runner machinery, schemas, tooling | ai-generated | written in directed sessions; reviewed and merged by the maintainer |
| Conformance cases and their expectations | ai-generated | authored against the released specification text, each expectation carrying the section it came from. §7 is why this is checkable rather than trusted |
| Tests | ai-generated | held to the same authority as the code they test |
| Documentation and this statement | ai-generated | held to the repository's own prose rules |
| Reading a specification silence, and its disposition in the register | none | adjudicated by the maintainer and recorded on the tracker |
| Attribution of a failing row to a suspect | ai-assisted | a read-only agent derives the required behaviour from the specification text and proposes the attribution with its citation; the call, and anything it obliges, is the maintainer's |
| Release decisions | none | the maintainer's |
| Contribution and review verdicts on others' work | none | prohibited use; see §11 |

**autonomous** appears in no row, and that is the point of the next section.

## 6. Human oversight

The maintainer directs the work, reads the result, and merges every change.
Nothing lands on its own authority and no merge is automated. Where the tools run
multi-step sessions, the decisions with consequences are the maintainer's and are
recorded on the tracker: what a specification silence means, what a wire
behaviour must be, which suspect a red row is attributed to, what ships in a
release. A decision that exists only inside a tool session is not a decision
this project made.

## 7. Quality controls, and what each one proves

This is the section that matters for this particular product. An
AI-authored expectation inside a conformance catalogue is a real risk: a
plausible-sounding assertion, confidently wrong, becomes a verdict about
somebody else's software. The control is not a promise about the tool. It is
that **the expectation carries its own evidence, so anyone can refute it without
the maintainer's cooperation.**

- **Every expectation cites the section it comes from.** Authoring an expectation
  means reading the governing section first-hand and quoting the sentence that
  assigns the value. A reader who disagrees reads the same sentence. This is
  what makes an AI-authored case falsifiable rather than authoritative, and it
  is enforced by review against the cited text.
- **The specification is never a suspect, and no server ever sets an
  expectation.** The attribution law
  ([`.claude/rules/cnf-triage.md`](.claude/rules/cnf-triage.md)) forbids reading
  a server's response to decide what an expectation should be, and forbids
  adjusting an expectation to match observed behaviour. Both are the failure
  modes an AI-assisted workflow would otherwise drift into fastest.
- **The instrument is a first-class suspect on every failing row**, ahead of the
  server under test. The first live triage in FerroEHR attributed 7 of 7
  diagnosed defects to the runner and none to the server. That is the shape the
  discipline is supposed to produce, and it is the number to watch: an
  instrument that keeps finding the other side at fault is the one to distrust.
- **Verdicts are pure functions over committed records.** A published result
  derives from run artifacts that ship with it, so a claim can be recomputed and
  contradicted. Nothing is a summary of a run nobody else can see.
- **Coverage ratchets up and is published with its gaps.** A behaviour with no
  case is recorded as a gap, never left silent, so the boundary of what a green
  run proves is part of the result.
- **Tests and expectations shall not be weakened to make a run pass.** A
  standing hard rule for humans and tools alike
  ([`.claude/rules/testing.md`](.claude/rules/testing.md)).
- **Static and supply-chain gates.** No `unsafe`, deny-tier lints on panicking
  shortcuts, typed errors, machine-checked comment style, `cargo deny` policy,
  and workflow security audits — all running in this repository's CI. Signed
  release artifacts ship through `release.yml` (#12): per-architecture
  tarballs with checksums, a CycloneDX SBOM and a Sigstore provenance
  attestation on each digest, built in the SLSA Build L3 lane.

What these controls do **not** prove is stated in §12.

## 8. Licensing and provenance of AI output

The project is Apache-2.0. The position taken here follows the Apache Software
Foundation's and LLVM's published reasoning rather than wishful shortcuts: an AI
tool's output does not launder anyone's copyright, the full provenance of
generated text is generally not knowable, and prompting alone is not treated as
authorship. In practice: contributions of substantially copied third-party
material are refused however they were produced; generated code is held to the
same originality expectations as human code, under the same review; and if
identifiable third-party material is found in the tree, it is removed or
licensed properly, exactly as it would be for a human-introduced copy. The tools
are used under terms that do not restrict the output's use in Apache-2.0
software.

Two points specific to this repository. The vendored openEHR specification text
and the vendored corpora are **redistributed verbatim under their own upstream
terms**, declared in [`REUSE.toml`](REUSE.toml) and in each tree's
`PROVENANCE.md`; no AI tool edits them, and hand-editing a vendored tree is
forbidden by the project's own corpus discipline. Quoting a specification
sentence inside a case citation is fair quotation of the authority the case is
measured against, and the citation names the source every time.

## 9. Data

No patient data, no personally identifiable health information, and no customer
data exists anywhere in this project. Not in the repository, not in the corpora,
not in telemetry, and therefore not in any prompt. Test data is synthetic or
comes from the published openEHR corpora and CKM clinical models, which are
modelling artifacts rather than records about people. This is a structural
property a reader can check against the tree.

One class deserves naming because the instrument's job creates it: a run holds
**credentials for the system under test**, declared in that party's IXIT. Those
are the operator's secrets, they never enter a prompt, and keeping them out of
published artifacts is treated as a security property with its own scope note in
[SECURITY.md](SECURITY.md). Vendor-side data handling for the AI tools is
governed by the tool vendor's terms; this document makes no claim on the
vendor's behalf, because such claims go stale silently.

## 10. Rules for contributors

Contributors **may** use AI tools. A contribution with **ai-generated** content
per §3 **shall** say so in the pull-request description: which tool, and what it
did. Disclosure lives in the pull-request description and never in commit
trailers, for two reasons stated openly. This repository's standing rule keeps
tool attribution out of commit metadata, and one maintained disclosure beats ten
thousand trailer lines; and the wider ecosystem has no agreed trailer anyway,
since the same trailers some communities recommend, others forbid.

For a contribution that touches the catalogue, the bar is higher and it is not
about the tool: **an expectation arrives with the specification section it came
from, quoted.** A case without a citation is refused regardless of who or what
wrote it. The contributor remains responsible for the submission in full, under
the same [CONTRIBUTING.md](CONTRIBUTING.md) bar as any other work: understood,
explained on request, tested, and honest.

## 11. Prohibited uses

In this project, AI **shall not**: merge anything; adjudicate, score, or answer
reviews of contributions; sign anything; decide what a specification silence
means; set or adjust an expectation from a server's observed behaviour; issue
the final attribution of a failing row; or weaken a test, an expectation, or a
gate to make something pass.

## 12. Limitations and residual risks

This section exists because a disclosure without one is marketing.

- **The catalogue proves what it covers.** A green run demonstrates the
  behaviours its cases exercise. Coverage is broad, ratchets upward, and is
  published with its gaps, and it is still a boundary.
- **A citation proves the source, not the reading.** Every expectation names the
  section it came from, which makes a wrong reading findable. It does not make
  one impossible, and a wrong reading that nobody has checked yet is the most
  likely defect class in this repository.
- **Review depth is one person's.** A single maintainer, who also maintains one
  of the CDRs this instrument grades
  ([GOVERNANCE.md](GOVERNANCE.md) states that conflict and what constrains it).
  The honest claim is that the maintainer understands and can explain every
  merged change. "Every expectation was independently re-derived by a second
  reader" would not be.
- **Release-artifact signing is not in place.** The static and supply-chain
  gates run here; the signing half waits on a release pipeline (#12), and §7
  says so rather than implying a control that has not landed.
- **Retroactivity.** Commits predating this statement carry no disclosure
  markers. This document describes the practice, not a per-commit audit trail,
  and no such trail is claimed.
- **Provenance uncertainty survives.** Whether any generated fragment echoes
  unlicensed training material is not fully knowable with current tools. §8
  states the handling, not a guarantee.
- **The legal ground is unsettled.** Copyright in AI output is an open question
  in most jurisdictions. This document records positions, and positions may have
  to change. §13 names the triggers.
- **This is a self-declaration.** No third party has audited it. The checkable
  artifacts in §7 are the counterweight: they can disagree with this document,
  and if they do, the document is wrong.

## 13. Review and change

This statement is reviewed at every major or minor release, and revised
off-cycle when any of these fires: the tooling changes materially, a tool
vendor's terms change in a way §8 or §9 relies on, a binding rule emerges (EU AI
Act guidance touching this use, a foundation policy this project follows, a
court decision on AI output and copyright), or a claim in this document stops
being true. The release pipeline landing (#12) is itself a trigger, because §7's
one pending row becomes real then. The maintainer owns the review; the change lands
as a pull request like everything else, and the version and change log update in
the same one.

## 14. Reporting

A suspected provenance, licensing, or quality problem in this repository,
including a claim in this document that does not survive checking, is a report
this project wants. Open an issue and cite this file. For anything
security-sensitive, use the private route in [SECURITY.md](SECURITY.md). The
handling commitment is the same as for any defect: attributed, answered on the
tracker, and never silently absorbed.

## 15. References

**Normative for this project**, the documents that bind the practice described
here: the [Apache-2.0 licence](LICENSE); the released openEHR specifications
(vendored in this repository at `specs/openehr/`); this repository's rule set
([`.claude/rules/`](.claude/rules/), in particular `cnf-triage.md`,
`testing.md`, `reliability.md`, `comments.md`, `writing-style.md`);
[GOVERNANCE.md](GOVERNANCE.md), [MAINTAINERS.md](MAINTAINERS.md),
[CONTRIBUTING.md](CONTRIBUTING.md), [SECURITY.md](SECURITY.md).

**Informative**, the sources this document's structure and positions draw on:
the W3C AI Content Disclosure vocabulary; the ISO/IEC Directives Part 2 document
conventions and verbal forms; ICMJE's AI-authorship position; the Apache
Software Foundation's generative-tooling guidance; the Linux Foundation's
generative-AI policy; the Fedora Council's AI-assisted-contributions policy; the
Linux kernel, LLVM, Kubernetes, NumPy, Mozilla, QEMU, curl, and Gentoo
positions; the OpenSSF security guidance for AI code assistants; NIST AI RMF and
ISO/IEC 42001 as vocabulary; EU AI Act Articles 2, 3, and 50 with the European
Commission's Article 50 FAQ. The survey behind this structure was performed for
a sibling project's statement of 2026-08-24 and is recorded on its tracker;
this document draws on it and states its own positions.

## Annex A. Change log

| Version | Date | Change |
|---|---|---|
| 1.0.0 | 2026-08-26 | First issue. |

## Annex B. Machine-readable summary

Levels per the W3C AI Content Disclosure vocabulary (§3); the prose above is
authoritative where the two could ever disagree.

```yaml
ai-statement:
  version: 1.0.0
  last-updated: 2026-08-26
  vocabulary: w3c-ai-content-disclosure
  disclosure-default: ai-generated
  tools:
    - name: Claude Code
      provider: Anthropic
  processes:
    design: ai-assisted
    implementation: ai-generated
    catalogue-authoring: ai-generated
    testing: ai-generated
    documentation: ai-generated
    review: none
    adjudication: none
    attribution: ai-assisted
    release-decisions: none
  ships-ai-system: false
  verdicts-use-inference: false
  autonomous-use: none
```
