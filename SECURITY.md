# Security Policy

Veredictum is a test instrument that is pointed at other people's servers. It
is handed credentials for a system under test, it writes data into that system,
and the records it produces are used as evidence about that system. Reports
against it are taken seriously and handled with priority.

## Two different things, two different processes

Read this section before reporting, because the routing is the part people get
wrong.

**A vulnerability in Veredictum** goes through the process below: the runner,
the catalogue, the schemas, the container image, the release artifacts, the
workflows, anything published from this repository.

**A vulnerability that Veredictum finds in a CDR under test belongs to that
CDR's vendor, not here.** The instrument exists to reveal how a server behaves,
so a run turning up an authentication bypass, an access-control hole, or data
leaking across EHRs in the server is a finding about that product. Report it
through that vendor's own disclosure process, on their timeline. This repository
publishes no advisory about someone else's software, and opening an issue here
about an unpatched third-party server would be a public disclosure of their
defect through the wrong door. The one thing this project will do is fix the
instrument if it mis-reported what it saw, and add the case that pins the
behaviour once the vendor's disclosure is out.

## Supported versions

**The supported version is the most recent release**, and every release is
published by the same tag-driven pipeline: prebuilt `x86_64` and `aarch64` Linux
binaries with Sigstore bundles, a multi-architecture container image on GHCR, and
the crate on crates.io. Report against the newest release, or against `main` if
you reproduced there — either way, state the version or the commit.

The policy is the one this project can actually keep:
**only the most recent release is supported.** No maintenance branches, no
long-term-support line, no backports. A fix lands on `main` and ships in the
next tagged release. If you are not on the newest release, you are receiving no
security fixes, and the action is to upgrade.

That has a consequence worth stating for anyone who publishes a conformance
record: the version that produced your verdict stays fixed and re-checkable
forever, and it stops receiving fixes the moment a newer one exists. Pin
deliberately, and record the version alongside the verdict.

## Reporting a vulnerability

**Please do not open a public issue for a suspected vulnerability.**

Report privately through
[GitHub private vulnerability reporting](https://github.com/rubentalstra/Veredictum/security/advisories/new)
("Report a vulnerability" on the repository's Security tab).


Include what you can: the affected component, the version or commit,
reproduction steps or a proof of concept, an impact assessment, and any
suggested fix.

### What you can expect

- **An acknowledgement within 5 working days.** If you have heard nothing by
  then, the report has not reached anyone. Escalate by opening a public issue
  saying only that a private report is awaiting acknowledgement, with no
  details.
- **An assessment with a severity and an intended fix window within 14 calendar
  days** of the acknowledgement.
- **Coordinated disclosure.** A date is agreed with you rather than imposed on
  you, and you are told when the fix ships.

These are commitments to you, not conditions on you. If they are missed,
publishing is your call.

### Safe harbour

No legal action will be pursued or supported against anyone who reports a
vulnerability in good faith and follows this policy. In practice that means: you
tested against your own deployment or a test instance you control, you did not
access, modify, or retain anyone else's data, you did not degrade service for
others, and you gave the window above before publishing. If you are unsure
whether something is in scope, ask first. A question is always in good faith.

**Never run this instrument against a live clinical deployment you do not own.**
The runner writes into the system it tests: it creates EHRs, commits
compositions, uploads templates, and deliberately sends malformed and refused
requests to check that they are refused. It is built for a test instance, and
using it anywhere else is on you.

### Credit

Reporters are named in the advisory and the changelog by default, using whatever
name and link you give. Say so if you would rather not be named; declining
credit costs you nothing and changes nothing about how the report is handled.

## Scope notes

- **Credentials for the system under test are in scope.** A party's IXIT
  declaration carries the endpoint and the credentials the run authenticates
  with. Anything that leaks them into a published artifact, a log, a recorded
  exchange, or an error message is a vulnerability in this instrument, and it is
  the class most worth hunting here.
- **Integrity of a verdict is in scope.** A path that lets a party influence its
  own result without the catalogue and the specifications saying so is a
  vulnerability in the strongest sense the product has, whether that is
  tampering with a recorded run, a schema that accepts a forged record, or an
  input a server controls being trusted where it should not be.
- **Release-artifact integrity is in scope**: the Sigstore signatures and
  provenance attestations, the SBOMs, the checksums, the container digest, and
  anything else that lets a consumer be handed bytes this project did not build.
  A verification path that appears to pass on a substituted artifact is a report
  this project wants urgently.
- **The corpora are test data, and they are supposed to contain hostile
  shapes.** A malformed archetype or an invalid composition in
  `artifacts/corpus` is an asserted negative test, not a defect. What would be a
  defect is such an input escaping the instrument's own parsing safely.
- **No patient data exists anywhere in this project.** The corpora are
  modelling artifacts and synthetic instances. A report that something in the
  tree looks like real clinical data is a report this project wants.

## Repository security settings — the posture of record

Settings live in GitHub, not in the tree, so they can change without a commit
and reset without anyone noticing. This table records what the posture is
supposed to be. Read it back with
`gh api repos/rubentalstra/Veredictum --jq '.security_and_analysis'` and
`gh api repos/rubentalstra/Veredictum/rulesets`, and treat a divergence as a
finding.

| Setting | Expected | State on 2026-08-26 | Why |
|---|---|---|---|
| Secret scanning | enabled | enabled | the baseline detector |
| Push protection | enabled | enabled | refuses the commit rather than filing an alert afterwards |
| Secret scanning, non-provider patterns | enabled | **disabled** | the credential class this repository is most likely to leak is not a provider token: an IXIT declaration carries a server URL with embedded basic-auth credentials, and the SMART lane carries a test signing key |
| Secret scanning, validity checks | enabled | **disabled** | the difference between "rotate this eventually" and "this credential is live right now" |
| Private vulnerability reporting | enabled | enabled | the reporting route this document points at |
| Dependabot security updates | enabled | enabled | advisory-driven bumps, exempt from the update cooldowns in [`.github/dependabot.yml`](.github/dependabot.yml) |
| Ruleset on `main` | active: no deletion, no force-push, signed commits, pull request required | **active** (ruleset 21570979: squash-only, the `conclusion` status check required; repository-admin bypass for recovery) | every commit is now enforced-signed, and every change reaches `main` through a pull request |
| Immutable releases | enabled: published assets and tags frozen; a bad cut is repaired by a new version, never a retag | **enabled** (owner, 2026-08-26) | the v0.0.1-alpha.1 release predates the toggle and stays mutable-metadata; everything after is frozen at publish |
| Ruleset on `refs/tags/v*` | active: no tag deletion, no non-fast-forward update, signatures required | **active** (ruleset 21571001, no bypass) | the release pipeline publishes off a raw tag push, so the window in which a tag drives a build is protected: the tag cannot be moved or deleted, and it has to be signed |
| `crates-io` environment | reviewer approval required; only refs that may publish | **active**: reviewer `rubentalstra`, `main` and `v*` tags | the crate publish is the one irreversible leg — a crates.io version can be yanked, never replaced — so it waits for a human and runs last |

Six of those rows do not match yet. They are listed with their real state rather
than as intentions, and each is closed by the migration work that makes it
meaningful. The two secret-scanning sub-settings are accepted-and-ignored by
`PATCH /repos/{owner}/{repo}` (the request returns `200` and changes nothing),
so they have to be switched on in Settings, Code security, Secret Protection.
