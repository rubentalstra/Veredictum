---
name: admin-405-is-permitted
description: A 405 on any /admin/** route is spec-permitted, so "status 405 maps to no outcome" is never a SUT defect — the Admin API is DEVELOPMENT-stage and ITS-REST Conformance is "tbd."
metadata:
  type: project
---

`status 405 maps to no outcome of the operation's binding (inconclusive)` on an
admin route is NEVER a SUT defect. Nothing released requires a server to serve
the Admin API.

**Why:** ITS-REST `specifications/docs/overview/Requests_and_responses.md`
§HTTP Methods: "If a method is recognized but not allowed for the target
resource, the response SHOULD be `405 Method Not Allowed` status code", and the
§HTTP status codes 405 row defines it as "The method received in the request-line
is known by the origin service but not supported by the target resource".
`specifications/docs/admin/Description.md` §Status: "This specification is in the
`DEVELOPMENT` state", and `docs/overview/Preface.md` §Conformance reads "tbd.".
For the bulk route the release says it outright —
`specifications/operations/admin_ehr_delete_all.yaml`: "This functionality is
intended primarily for **development** or **testing** purposes and may be
disabled in **production** environments, in which case server may respond with
`405 Method Not Allowed`" — and that operation DECLARES a `'405'` response, which
the binding `I_ADMIN_SERVICE.physical_ehr_delete-delete_all.yaml` deliberately
omits (a catalogue gap).

**How to apply:** the released ADMIN OAS declares exactly TWO operations,
`admin_ehr_delete` and `admin_ehr_delete_all`; every other `I_ADMIN_*` binding is
an EXTENSION route no openEHR spec governs, so its 405 rows are unenforceable
and belong to the ICS-keyed extension arm (`run.rs` `unserved_extension`).
An unauthenticated `GET /admin/ehr/all` answering `401` with `allow: DELETE`
proves the route exists and auth precedes method dispatch (verified 2026-08-31
on sandbox.ferroehr.eu, admin API disabled by the operator).
