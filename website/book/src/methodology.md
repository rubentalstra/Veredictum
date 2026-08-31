# The conformance method

<!-- toc -->

A conformance verdict is only worth what its method is worth. This chapter sets
the method out.

## The oracle is the released specification text

The vendored openEHR specification text is the oracle, and it is never a
suspect. Every expectation in the catalogue cites the section it comes from, and
the only thing that can refute an expectation is a better reading of that text.
Not a maintainer's confidence, and not what every other CDR happens to do.

Two components are both oracles for a functional case. The Service Model anchors
the operation and the naming the cases use; the ITS-REST specification binds
that operation to the wire. Where the ITS-REST prose is silent, the released
OpenAPI bundle shipped with the same release grounds the expectation, cited as
the OpenAPI document and never passed off as prose. It loses on any conflict
with the prose.

Some sources look like authority and are not. The official openEHR Conformance
component never reached a stable release: its Platform Conformance Test Schedule
says which behaviours are worth testing, which is genuinely useful, but it does
not settle what the correct answer is, and in places it contradicts a released
component. Its Robot test suites are in the same position. Where any of those
disagrees with a released component, the released component wins.

## The catalogue's shape: one behaviour, one case

A case core describes one narrow conformance requirement, in the Service Model's
vocabulary, with no status code, header or media type in it. A separate operation
binding says how that operation reaches the wire on a given interface
specification. That split is what lets the same case be graded against a future
wire specification without rewriting the behaviour it tests.

Errors in a case are expressed as *kinds*, never as codes. The catalogue
distinguishes a duplicate EHR from a non-existent EHR from a missing template
from a validation failure, because those are different behaviours; the mapping
from a kind to a status code lives in the binding, since the same kind maps to
different codes on different operations.

Cases stay small on purpose. One behaviour per case means a red row names one
defect, which is the difference between a report a vendor can act on and a report
they have to reverse-engineer.

## Positive and negative testing carry equal weight

A server that accepts everything passes every positive test. That is why the
catalogue treats a refusal as a first-class behaviour with its own cases.

Every invalid shape in the corpus is kept as its own entry, marked invalid, with
the defect it carries and the specification reference that makes it invalid.
Each one has a valid twin. The valid twin proves the server accepts what it must
accept; the invalid twin pins the refusal, so a lenient server fails it. Deleting
an invalid shape would silently narrow the claim, so it never happens.

The negative surface the catalogue covers includes:

- **Content negotiation.** An `Accept` header nothing can satisfy must produce
  406; an unsupported payload media type must produce 415.
- **Identity and state conflicts.** A duplicate identifier, a stale preceding
  version, a missing version precondition, a delete of something already
  deleted.
- **Content validity.** Missing mandatory attributes, empty lists where the
  Reference Model requires at least one member, structurally wrong documents,
  and content committed against the wrong template.
- **Scope negatives.** A format applied where the specification does not define
  it.
- **Authentication.** A sweep over the route table checking that every platform
  route refuses an unauthenticated request.

An invalid payload cannot be authored through a typed model, because a typed
model will not construct it. Those fixtures are therefore raw bytes, which is
also what makes them catch encoder and decoder bugs that a
construct-then-serialize fixture cannot reach.

## When a run goes red

A red row is a statement that the specification, the catalogue and the server do
not all three agree. Which one is wrong is decided before anything is changed.

Two reflexes are both refused, and the second is why this instrument exists
separately from any server:

1. **"The catalogue must be wrong, the server is right."** This is the reflex a
   vendor brings to a failing conformance run. The catalogue answers it by
   construction: an expectation traces to a citation, so it is refuted by a
   better reading of the released text and by nothing else.
2. **"The server must be wrong, the instrument is right."** Veredictum is the
   thing people are being asked to trust, so it is a suspect on every red row,
   ahead of the server. The first live triage attributed 7 of 7 diagnosed
   defects to the instrument and none to the server under test.

Each red row is attributed to exactly one of three suspects:

| Suspect | What it means | What happens |
|---|---|---|
| **The server under test** | The server violates the released specification | A defect report to its maintainers, carrying the reproduced wire exchange and the citation. Nothing changes in the catalogue |
| **The instrument** | The server behaved correctly and the runner misdrove the case or misjudged the response | The runner is fixed. Those rows were inconclusive, never server failures |
| **The catalogue** | The expectation is wrong against the specification | The artifact is fixed, with a new cited source for the corrected expectation |

The derivation per row is the same every time: read what was actually sent and
received, read what the case and its binding claim to expect and why, then read
the governing released specification text first-hand and derive independently
what a conformant server must return for the exchange that actually happened.
The three-way comparison names the defect.

Two rules keep that honest. A transport fault or a step that could not be
resolved classifies as inconclusive on the instrument's side, never as a server
failure. And an attribution naming the server carries the reproduced exchange
plus the citation with the decisive sentence quoted, because a verdict about
somebody else's product is held to a higher evidence bar than a verdict about
our own.

> [!WARNING]
> A catalogue expectation is never adjusted to match observed server behaviour.
> A server response is evidence in the three-way comparison. It is not the
> reference, and treating it as one is how a conformance suite quietly becomes a
> description of whatever the last server did.

## Where the specification does not say

Sometimes the released text leaves a behaviour undefined, or two released
documents disagree. Inventing an expectation there would fail every conformant
server and teach nobody anything. Hiding it would be worse.

Instead, each such case is recorded in the **ambiguity register** with four
things: the ambiguity, the first-hand citation that establishes it, the handling
a runner must apply, and a typed disposition the pipeline branches on.

The dispositions are a closed set:

| Disposition | Effect |
|---|---|
| `loose_assert` | Assert only what the specification actually pins, and nothing more |
| `fixed_handling` | The handling is encoded directly in the case or the binding |
| `option_select` | The entry names one family per independent choice; sibling cases realize a family’s arms, and the declaration answers every family its claim reaches with exactly one arm |
| `report_only` | The result is reported and never gates a certificate. Reserved for genuinely open upstream questions |
| `statement_declared` | The party declares the behaviour in its statement |
| `editorial` | The specification text is itself defective; the catalogue encodes the reading derivable from the released text, with a citation |

Four properties make this a transparency mechanism rather than an excuse list:

- **An entry has to be proven.** A claimed ambiguity that the specification
  actually defines is a catalogue defect. The entry is removed and the case
  becomes gating.
- **Nothing is absorbed.** A `report_only` or `editorial` entry must carry a
  link to an upstream report, and that requirement is enforced by the schema
  rather than by review. A carried divergence always has an outbound report
  attached.
- **Every case still runs.** The register is not an exclusion list. It governs
  how a spec-silent expectation is derived and whether a genuinely open question
  gates a certificate.
- **A private resolution is non-conformant.** A harness that quietly decides an
  ambiguity for itself is not implementing this schedule.

`report_only` is a cited, upstream-linked suspension, and it reverts to gating
the moment the upstream question resolves. It is not a way to make red rows
disappear.

## Verdicts are computed, never asserted

A verdict is a pure function of the party's statement, the recorded results, the
catalogue and the capability matrix. The recording step and the judging step are
separate commands, so the record survives the judgement and anyone holding the
same files can re-derive the same documents.

Measured performance works the same way. A class verdict is re-derived from the
HDR histograms embedded in the record, so the stored summary is tamper-checked
rather than trusted, and latency is measured from the planned arrival instant
under open-loop offered load, which is what stops coordinated omission from
hiding a stall.

## Coverage is a mandate

A green run over a thin catalogue proves nothing, so coverage is machine-checked
rather than asserted. A gate enumerates the wire surface from the released
sources alone, the Service Model's platform interfaces crossed with their
ITS-REST branches, and fails on any operation, status-code branch, header rule,
negotiation variant or error family that has neither a covering case nor a cited
exception.

A behaviour the specification defines and the catalogue misses is a gap to close
or an honest boundary recorded in the register. Cases are added; they are never
removed to make a run go green.
