<!--
SPDX-FileCopyrightText: Veredictum contributors
SPDX-License-Identifier: Apache-2.0
-->

# The public results registry — submission rules

Rules version **1.1.0**. Every merged entry records the rules version it was
accepted under, and rules change prospectively: a published entry is never
re-scored against a later version of this document.

The registry is the committed set of published results about openEHR clinical
data repositories: conformance runs on one board, benchmark runs on another.
A submission is a pull request that adds one entry. CI validates it before
anybody reads the numbers, and the merge is the publication.

## An entry is a report

**A test report is not a certificate.** An entry says what happened when a
named version of a named system was driven by a named version of this
instrument on a named machine. It is not a certification, not a mark, and not
a statement that the system is fit for any purpose. Certification is the
openEHR Foundation's to grant, and this registry is deliberately shaped to
hand over: the rules are public, the entries carry their own evidence, and no
step of the pipeline is proprietary.

Two things follow, and both are printed on every rendered surface. A
conformance entry is a verdict about the catalogue's cases against one
deployment, and a benchmark entry is comparative speed. Neither one substitutes
for the other. A fast server that fails the catalogue is a fast server that
fails the catalogue.

## Three kinds of entry, and what separates them

A signature proves who submitted a file and that the bytes have not moved
since. It never proves the run happened as described. So every entry carries
one of three tiers, and the tier is a property of who performed the run. Two
of them are official, and they answer different questions: neither supersedes
the other.

**Reproduced.** This repository's own workflow composed a deployment from a
recipe committed here, drove the catalogue against it, and attested the
artifacts from the workflow's OIDC identity through Sigstore. The identity is
the signature. Anybody can check it:

```bash
gh attestation verify <artifact> --repo rubentalstra/Veredictum
```

No key stands behind this tier and none ever will. A stored key is one
compromised workflow away from forging every entry on the board, and the
release lanes here refuse long-lived credentials for the same reason.

**Console.** console.veredictum.eu, the official hosted instrument, drove the
catalogue against an endpoint the submitter named, recorded every exchange,
computed the verdicts, and opened the submission from its own GitHub App
identity. That identity is the only one permitted to open a `console` entry.

The endpoint has to be reachable from the public internet. The instrument
refuses a target only it could reach, before a socket opens, because a visitor
who could name a loopback or private address could point it at its own host
network. A deployment the internet cannot reach is therefore not eligible for
this tier, and its honest routes are `reproduced`, if the deployment can be
composed from a recipe committed under `registry/topologies/`, or
`self-reported`.

The instrument is not asked to be trusted. A verdict is a pure function over
the recorded exchanges, so CI recomputes the verdicts here from the transcript
the submission carries and refuses the submission unless they match. Only then
is the record signed. What a console entry attests is therefore threefold: the
run was performed by an instrument nobody in the exchange controls, its
judgement is arithmetic anyone can repeat, and the bytes have not moved since
CI repeated it.

What it cannot attest is the environment. The submitter chose the endpoint, so
the entry states what was measured and where, and claims nothing about how
that server is normally configured. That is the one thing `reproduced` adds,
and the only sense in which it is stronger.

One signing key exists in this project, and this is its whole scope: the
registry key that signs a console record. It lives in a protected CI
environment, it is used by the lane that re-derived the verdicts it signs and
by nothing else, and it never reaches the hosted instrument, this repository,
or any workflow a pull request can influence. The instrument that produced the
record holds no key in any form. The public half is committed at
`registry/keys/registry-signing.pub.asc`, so a reader checks a console record
offline:

```bash
veredictum verify-record \
  --record registry/records/<system>/<entry-id> \
  --key registry/keys/registry-signing.pub.asc
```

**Self-reported.** The submitter performed the run and signed the artifact,
with OpenPGP or with a Sigstore bundle carrying their own identity. The entry
carries the verification command, so a reader checks the signature rather than
trusting the board. Read a self-reported entry as a claim its author put their
name to in a public git history. A console run someone drives on their own
machine is this kind: the instrument is the same, the identity behind it is
theirs, and that is exactly why the hosted instance exists.

Nothing about a tier is written by hand into a `tier` field. The tier is the
discriminant of the entry's `provenance` block, so claiming `reproduced` means
carrying the workflow reference, the run id and the attestation predicate that
only this repository's lane produces. CI refuses an entry that names a
workflow outside this repository. A `console` block goes further: every field
in it is written by the re-derivation lane rather than by the instrument that
would benefit from it, so the performer cannot state its own provenance at
all.

## Where an entry lives

```text
registry/entries/<kind>/<system>/<entry-id>.json      the entry
registry/records/<system>/<entry-id>/…                a conformance entry's evidence
benchmarks/submissions/<system>/<date>-<host>.json    a benchmark entry's record
```

`<kind>` is `conformance` or `bench`. `<system>` is a lowercase id naming the
CDR, reused across that system's entries. `<entry-id>` is
`<YYYY-MM-DD>-<slug>`: the date the run started, then a lowercase slug you
choose. Ids are unique across the whole registry, which is what makes
supersede-by-reference resolvable.

A benchmark record stays in the benchmark submissions tree, because that is
where the board reads its numbers. The registry entry points at it by path and
digest and adds what the engine-written record cannot carry: who submitted it,
what they disclose, and how it is signed.

## The mandatory disclosure

`schemas/registry-entry.schema.json` is the contract, and CI applies it before
anything else. Every entry states, and an empty value is refused:

| Field | What it must say |
|---|---|
| `submitter.name`, `submitter.contact` | who is publishing, and where the entry can be discussed |
| `submitter.relationship` | `vendor`, `integrator`, `independent` or `maintainer` |
| `subject.system`, `subject.display_name`, `subject.version` | what was measured, at which version |
| `subject.deployment` | how it was deployed, with image digests where there are any |
| `subject.deployment.reproduction_authorized` | whether this repository may drive that deployment |
| `disclosure.instrument_version` | the Veredictum version that produced the artifacts |
| `disclosure.run_started_at` | when the run started, RFC 3339 in UTC, matching the date in the entry id |
| `disclosure.environment` | the machine: operating system, architecture, how you describe the host, and the cores and memory where the platform discloses them |
| `disclosure.sut_configuration` | what was switched on behind the result: authentication, validation depth, signing, audit, tenancy |
| `disclosure.conflict_of_interest` | any interest you hold in the outcome, in words |

`conflict_of_interest` has no "not applicable". Write the sentence that is
true. FerroEHR's own entries go through this pipeline with the same field
filled in, tier-labelled the same way as everybody else's.

A conformance entry also carries the catalogue revision it ran and the party
statement its claim was judged against. A benchmark entry carries the pack, its
version, the repetition count and the posture profile the run declared.

## The artifacts

Every entry lists the files it stands on, each with its SHA-256. Nothing on a
board is a number typed into the entry: the boards read the artifacts. So the
list is complete by role.

- A **conformance** entry carries `results` and `verdicts`, and may carry a
  `transcript`, a `record-manifest`, rendered `report` documents and the
  `ixit` declaration the run was driven under.
- A **benchmark** entry carries one `bench-result`.
- A **console** entry carries the `transcript`, the `ixit` and the `statement`
  as well, because those three are what a re-derivation reads: the recorded
  exchanges, the topology they were driven under, and the claim they were
  judged against. It also carries the `signature` CI writes over one of the
  entry's own artifacts.
- A **self-reported** entry carries the `signature` file too, and the artifact
  it signs must be one of the entry's own.

## Submitting a conformance entry

```bash
veredictum run --root artifacts --ixit <your-ixit>.json \
  --sut-name <system> --sut-version <version> \
  --statement party/<system>/statement.json --out ./run

veredictum verdicts --statement party/<system>/statement.json \
  --results ./run/results.json --root artifacts --out ./judgement
```

Copy `results.json` and `verdicts.json` into
`registry/records/<system>/<entry-id>/`, sign one of them, write the entry, and
open the pull request. That is a self-reported entry: you performed the run,
and your signature is what a reader checks.

## Submitting from the hosted instrument

A run at console.veredictum.eu submits itself. Connect the endpoint, drive the
catalogue, and the finished run offers to open the pull request: the
instrument writes the entry and the record, its GitHub App identity opens the
branch, and you fill in the disclosure this document makes mandatory, the
conflict-of-interest sentence included.

You write no provenance block, and neither does the instrument. CI re-derives
the verdicts from the transcript the submission carries, refuses any mismatch,
signs the record with the registry key from its protected environment, and
writes the `console` block stating what it established. The merge is the
publication, exactly as for every other entry.

Two of that block's facts are things the lane OBSERVED rather than read: the
instrument comes from the App identity that opened the submission, and the run
id from the `console-run/<run-id>` branch it arrived on. A submission opened by
anything but that identity is refused rather than signed.

Nothing about that path removes this one. The same console run on your own
machine, sealed and signed with your own key, is a self-reported entry, and it
is published on the same board beside the rest.

## Submitting a benchmark entry

Run the benchmark and place the record exactly as `benchmarks/SUBMITTING.md`
describes. That guide's checks all still apply: the pack pins, three
repetitions, the same-machine baselines, the failed-arrival ceiling, the
posture canaries and the file name that digests to the environment block. Then
add the registry entry beside it.

## The reproduction lane

A `reproduced` entry is produced here, never submitted. The lane composes a
**reproducible topology**: a deployment recipe this repository controls end to
end, declared under `registry/topologies/<id>/topology.json`. Nothing a
submitter wrote is executed in a job that holds an OIDC token, which is why the
lane will not compose an image or a compose file that arrived in a pull
request.

The re-derivation lane behind a `console` entry holds to the same rule and is
not an exception to it: it READS the submitted transcript and recomputes
verdicts over it, and it executes nothing that arrived in the pull request.

A topology declares the principals its composed deployment actually has, which
is narrower than the party's own declaration: the quickstarts stand up one
clinical principal, so every case addressing an admin or read-only principal is
recorded not-applicable at selection time. The bundle therefore carries that
declaration as `ixit.json`, attested beside the results, and the run record's
`ixit_digest` is the leading 8 bytes of the SHA-256 over its bytes. Anybody
re-derives it with `sha256sum ixit.json | cut -c1-16`.

The lane runs on a pull request that touches the registry, and on demand. It
selects entries whose deployment names a committed topology, composes it,
drives the catalogue, attests the artifacts, and uploads them. A maintainer
commits the resulting tier-1 entry. When a pull request comes from a fork the
lane reports what it would have done and performs no run, because a fork's
workflow cannot hold the identity that makes the attestation worth anything.

**Driving a hosted endpoint needs the operator's standing authorization, and
the submission pull request is that authorization.** Set
`subject.deployment.reproduction_authorized` to `true` to give it, and expect
the run to create and delete data. A topology for a system whose deployment
recipe this repository does not carry is a change to this repository, reviewed
like any other.

## What CI checks before a human reads it

`scripts/checks/registry-submission.sh` is the whole gate, and it fails the
pull request on any of these.

| Check | What fails it |
|---|---|
| Schema | the entry does not validate against `schemas/registry-entry.schema.json`, or does not parse |
| Disclosure | any mandatory field empty, or a run timestamp that is not UTC |
| Naming | the file is not at `registry/entries/<kind>/<system>/<entry-id>.json`, or the id's date and the run's date disagree |
| Uniqueness | an entry id already used anywhere in the registry |
| Artifacts | a required role missing, a path outside the tree it belongs in, or a digest that does not match the committed bytes |
| Signature | a self-reported entry whose signature covers something it does not carry, or whose signature file it does not pin |
| Tier | a `reproduced` entry naming a workflow outside this repository, or a deployment the lane cannot compose; a `console` entry naming a foreign re-derivation workflow, or a deployment the hosted instrument never drives |
| Re-derivation | a `console` entry whose verdicts do not recompute from the transcript it carries |
| Identity | a `console` submission opened by anything but the hosted instrument's own App |
| Topology | a deployment naming a topology no committed file declares |
| Supersede | a superseded id nothing in the registry carries, an entry superseding itself, or a supersede with no reason |
| Bench pairing | a committed benchmark record with no entry pointing at it, or an entry pointing at a record that is not committed |
| Append-only | the pull request modifies, deletes or renames anything already merged under the registry |
| Board freshness | a rendered board no longer matches the entries it was generated from |

Run it locally before pushing:

```bash
bash scripts/checks/registry-submission.sh
bash scripts/render/conformance-board.sh
bash scripts/render/bench-board.sh
```

The boards are generated and committed, so regenerate them in the same pull
request.

## Append-only, and supersede by reference

A merged entry is a published claim with a signed commit and a date behind it.
Entries are added. They are never edited and never removed, and the gate
enforces that mechanically.

A correction is a **new** entry that names the old one in `supersedes` and says
why in `supersede_reason`. The superseded entry stays visible, marked as
superseded, with a link to what replaced it. The pointer travels forward only,
because writing a `superseded_by` field into the old entry would be an edit to
a published claim.

## Disputes

If you disagree with an entry about your product, open a pull request. Two
remedies exist, and both add rather than remove.

- Ask for a fresh reproduction run. If your deployment has a committed
  topology, the lane produces a tier-1 entry that supersedes the disputed one.
- Submit a superseding entry of your own, with the disclosure this document
  requires.

Disagreements about the rules themselves — what a pack measures, what a tier
means, what the gate should refuse — belong in a pull request against this
document. Rule changes apply from the version that carries them. Published
entries are never re-scored.

## How a version leaves the readable set

Each release declares the set of rules versions and entry format versions it
can read, and that set is wider than the newest version of each. An entry
declaring any member of it is accepted as it stands, which is what keeps a
merged entry publishable while this document moves on.

A version leaves the set only when a field an entry at that version carries no
longer means here what it meant when the entry was accepted. That is the one
event that can invalidate an already-published entry, so dropping a version is
its own pull request: it names the field whose meaning moved, and for every
affected entry it either supersedes that entry with a re-derived one or states
why the entry can no longer be read. A version is never dropped to tidy the
set.

**A version is added when what a submission must satisfy changes.** That is the
test, and it decides both directions: an edit to this document that changes no
criterion an entry is scored against is not a rules change and carries no new
version, while a new obligation, a changed threshold or a new refusal is one and
carries its own. So the wording of a rule may be clarified without re-versioning
every published entry, and a rule that actually moves cannot be slipped in as a
clarification.

## What this registry does not do

It does not rank a conformance entry against a benchmark entry, and it never
mixes the two boards. It does not compare benchmark rows taken under different
posture profiles. It does not grade a system on anything it did not measure.
And it never claims that a passing run is a certificate.
