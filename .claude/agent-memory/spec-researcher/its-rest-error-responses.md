---
name: its-rest-error-responses
description: Where the released ITS-REST pins (and does not pin) error-response facts — the docs sections, the OAS response files, and the Error schema's only two attachment points
metadata:
  type: reference
---

Navigation only; re-read the text before acting. Related:
[[spec-defects-confirmed]].

Docs text (the tier-1 oracle) — every error-response fact lives in just two
files:
- `ITS-REST/specifications/docs/overview/Requests_and_responses.md`
  - §HTTP Methods — 501 / 405 SHOULDs
  - §Authentication and authorization — the WWW-Authenticate / Proxy-Authenticate
    conditional MUST, and the 401/403/407 trio
  - §HTTP headers → Location (MUST ONLY on creation/redirect), ETag and
    Last-Modified (the `W/` MUST), If-Match and accidental overwrites
    (412 MUST + ETag SHOULD, missing-If-Match → 400 SHOULD)
  - §HTTP status codes — the 16-row table, the "MAY return additional error
    details if `Prefer: return=representation`" sentence, and the worked
    example body
- `ITS-REST/specifications/docs/overview/Resources.md` §XML / §JSON /
  §Simplified Formats — the 415 and 406 MUSTs and the "Content-Type MUST be
  present unless the response has no content body" MUST, repeated three times
- The SMART app-launch adocs (`ITS-REST/docs/smart_app_launch/*.adoc`) say
  NOTHING about errors: a grep for "error" over the whole `docs/` adoc tree
  hits only DV_QUANTITY accuracy prose.

OAS (tier 2, cite AS the OAS): `ITS-REST/specifications/responses/*.yaml`
(source, $ref form) and the released bundles at `specs/rest-oas/*.yaml`
(self-contained, `components.responses` + `components.schemas.Error`).
- `schemas/others/Error.yaml` is the ONLY error body type in the release; it
  is attached by exactly TWO response files, `400.yaml` and
  `400_CONTRIBUTION.yaml`, and by nothing else.
- Only `412_*` and `409_*_with_uid_based_id` declare headers (ETag +
  a deprecated Location). Every other 4xx file is a bare `description`.
- No 401, 403, 407, 415, 500 or 501 response object exists anywhere in the
  OAS, and every bundle carries document-level `security: []`.

No error type exists in `specs/its-json-schemas/**` or
`specs/its-xml-schemas/**` (greps for "error" return zero definitions).

Catalogue side: the live adjudication is register **AMB-217** (`editorial`,
`upstream_issue: 2612`), not "AMB-1" — AMB-1 does not exist in
`artifacts/registers/ambiguities.yaml` although the runner's doc comments
cite it.
