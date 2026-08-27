# Governance

How decisions get made in Veredictum, who makes them, and how that changes.

This document describes the project as it actually operates. Where the honest
description is "one person decides", it says so. A governance document that
describes a committee which does not meet is worse than none, because it invites
a reader to rely on a control that is not there. That matters more here than it
would for most software: Veredictum publishes verdicts about other people's
systems, so anyone weighing one of those verdicts is entitled to know exactly
who stood behind it.

## Current structure: one maintainer, final say

Veredictum has a single maintainer ([MAINTAINERS.md](MAINTAINERS.md)) who holds
final say on what gets built, what gets merged, what gets released, and what the
project refuses to do. There is no steering committee, no technical oversight
body, no foundation, and no vote.

That is the standard structure for a project of this age, with the standard
trade-off: decisions are fast and coherent, and the project's resilience is one
person's. The second half is treated as a finding rather than a footnote; see
[MAINTAINERS.md § If the maintainer is unavailable](MAINTAINERS.md#if-the-maintainer-is-unavailable).

## The specifications decide conformance, not the maintainer

The one place the maintainer explicitly does **not** have final say is what
counts as conformant. That authority belongs to the released openEHR
specifications, and this is the property the whole product rests on.

- An expectation in the catalogue traces to a section of the released
  specification text, quoted at the point it is authored. It is refuted by a
  better reading of that text and by nothing else.
- No server's behaviour settles a question, including the behaviour of the CDR
  the maintainer also writes
  ([FerroEHR](https://github.com/rubentalstra/FerroEHR)). A response is evidence
  in a comparison; the specification is the reference.
- Where the specification text and the released OpenAPI documents are both
  silent, the behaviour goes to the ambiguity register with a typed disposition
  and, where warranted, an outbound `upstream-report` issue. A private
  resolution is not acceptable.

**On the conflict of interest, stated plainly.** The maintainer of this
instrument also maintains a CDR that this instrument grades. That is a real
conflict and it is not resolved by asserting good intent. What constrains it is
mechanical: expectations carry citations anyone can check, the runner and the
catalogue are public, run records are committed rather than summarised, and the
attribution law forbids reading a server's behaviour to decide what an
expectation should be. The first live triage in FerroEHR attributed 7 of 7
diagnosed defects to the runner and none to the server under test, which is the
shape the discipline is supposed to produce. If you find a case that favours any
CDR against the specification text, that is the report this project most wants,
and it outranks the maintainer.

## Where decisions are recorded

The tracker is the record. There is no separate design-document layer, and that
is deliberate: a design record that outlives the code it justified becomes a
false authority.

| Kind of decision | Where it lives |
|---|---|
| What to work on next | a [GitHub issue](https://github.com/rubentalstra/Veredictum/issues); the open list is the worklist |
| Why a change looks the way it does | the pull-request description that landed it, and the issue's closing comment |
| What a release contains | [`CHANGELOG.md`](CHANGELOG.md) and the `vX.Y.Z` milestone |
| A specification silence and how it was disposed of | the ambiguity register, and the `upstream-report` issue it points at |
| Standing working rules | [`CLAUDE.md`](CLAUDE.md) and [`.claude/rules/`](.claude/rules/) |

A decision that exists only in a conversation is not a decision this project
made.

## How a change gets in

1. **An issue carries the contract**: what is wrong or missing, and the
   acceptance criteria that settle it.
2. **A pull request implements it** on a conventional-type branch, declaring
   `Closes #N`, with signed commits.
3. **The gates run.** They are not advisory and there is no override. The
   battery is listed in [CONTRIBUTING.md](CONTRIBUTING.md): the guard tier, the
   Rust tier, and `validate` over the catalogue at zero findings.
4. **The maintainer merges.** A pull request from an account without write access
   additionally needs a code-owner approval
   ([`.github/CODEOWNERS`](.github/CODEOWNERS)).

**On required review, stated plainly.** The maintainer's own changes are not
independently reviewed by a second human, because there is no second human.
Requiring two approvals of oneself would be a control that reports "reviewed"
without anyone having reviewed. What stands in for review here is the
specification citation on every expectation, which any reader can check without
the maintainer's cooperation.

## Becoming a maintainer

The route is open and it is the ordinary one:

1. **Contribute.** Sustained, merged, self-directed work. The bar is the point
   at which review stops finding things, not a pull-request count.
2. **Show specification discipline.** Reading the normative text first-hand,
   citing it, refusing to resolve a question from a server's behaviour, and
   being honest when the text is silent. That is the judgement this project
   selects for.
3. **Ask, or be asked.** Either direction is normal. Open an issue, or say so on
   a pull request.

The maintainer says yes or no with a reason on the tracker rather than by
silence. A new maintainer receives write access, a row in
[MAINTAINERS.md](MAINTAINERS.md), and their handle in
[`.github/CODEOWNERS`](.github/CODEOWNERS) for the areas they own. Publishing
identities move separately and only where the identity permits a second holder;
that table is in MAINTAINERS.md and is kept truthful.

A second maintainer's arrival also re-opens the branch-protection
adjudication (SECURITY.md's Scorecard Branch-Protection section, issue #26):
the approver-class requirements (required reviews, CODEOWNERS review,
last-push approval) become satisfiable and are enabled, and the
repository-admin bypass is re-adjudicated with two admins holding it.

**A maintainer from a competing CDR is welcome.** Independence here means no
single implementation's behaviour decides an expectation, and more vendors at
the table makes that stronger. The discipline above is what keeps it safe, and
it applies to everyone identically.

## What this project will not do

Recorded here so the questions do not have to be re-litigated in each pull
request:

- **No contributor licence agreement, and no copyright assignment.** You keep
  your copyright; the licence stays Apache-2.0 for everyone including the
  maintainer.
- **No expectation without a specification citation**, and no expectation
  adjusted to match what a server did.
- **No test, gate, or case weakened to make a run green.** A red row is
  information.
- **No verdict a reader cannot re-derive.** Published numbers come from
  committed run records, performance numbers from measured runs on a declared
  environment. A claim with no artifact behind it does not get written.
- **No paid pass, no certification business, no privileged party.** Every party
  is graded by the same catalogue against the same specifications.

## Code of conduct

[`CODE_OF_CONDUCT.md`](CODE_OF_CONDUCT.md) applies to every space this project
occupies. Enforcement is the maintainer's, at the contact given there.

## Changing this document

Governance changes are pull requests against this file, like anything else, and
they take effect when they merge. If the structure described here stops being
true (a second maintainer joins, a legal entity forms, a decision body is
created), this file changes in the same pull request that makes it true, not
afterwards.
