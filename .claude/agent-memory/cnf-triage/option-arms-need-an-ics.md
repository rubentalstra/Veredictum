---
name: option-arms-need-an-ics
description: Confirmed attribution pattern — a statement-blind run drives BOTH arms of every option_select register pair, so one arm must fail; those rows are runner-side, never SUT or catalogue
metadata:
  type: project
---

A red row whose case core carries `option: <tag>` attributes to the RUNNER
whenever the run supplied no party statement: the arm the deployment does not
sit on is red by construction.

**Why:** `app/veredictum/src/run.rs` `selection_exception` gates the option arm
on `if let Some(stmt) = statement`, so with no ICS the arm is inert and both
mutually exclusive siblings drive. The register itself says neither answer is
non-conformant — `artifacts/registers/ambiguities.yaml` AMB-167: "a server MAY
offer XML … or MAY refuse it under the §XML Format 406 MUST. Neither answer is
non-conformant, so neither may be asserted unconditionally". The underlying
released text makes both 415 and 406 CONDITIONAL MUSTs, ITS-REST
`specifications/docs/overview/Resources.md` §XML Format: "If the service cannot
process the request payload as XML format, it MUST respond with HTTP status code
`415 Unsupported Media Type`" and "If the service cannot fulfill this aspect of
the request, it MUST respond with HTTP status code `406 Not Acceptable`" — a
service that CAN do it must serve, so `observed ok` where `not_acceptable` was
expected is the correct answer, not leniency.

**How to apply:** before attributing any `expected <refusal>, observed <success>`
row (or its mirror), grep the case core for `^option:` and check the twin's
status in the same `results.json`. The 100%-correlation test that settles it:
every arm the ICS declares passes and every arm it does not declare is red.
Verified 2026-08-31 on FerroEHR 4.0.13, 20 of 62 red rows, zero exceptions.
Related: [[access-control-is-delegated]].
