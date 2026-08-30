---
name: access-control-is-delegated
description: an expect-forbidden row over a role/principal boundary has no released-component ground — SM delegates access control and ITS-REST puts it at SHOULD
metadata:
  type: project
---

A case whose expectation is "principal X is refused with 403 on operation Y"
has NO released-spec ground, so `expected forbidden, observed ok/not_found`
is a CATALOGUE defect (register candidate), never a SUT defect.

**Why:** SM `docs/openehr_platform/master02-overview.adoc` §Functional Style
lists "approach to access control and authorisation" among the dimensions
where "In real implementations, different choices will be made", and states
"Authentication and authorisation is assumed to have been dealt with before
any particular call has been made". ITS-REST
`specifications/docs/overview/Requests_and_responses.md` §Authentication and
authorization is SHOULD-strength ("Services SHOULD implement and support an
HTTP Authentication and Authorization framework"), and its MUST is
conditional and only about WHICH code to use ("If authentication and
authorization are required, services MUST properly use the
`WWW-Authenticate` … returning `403 Forbidden`, `401 Unauthorized` …"). The
§HTTP status codes table merely DEFINES 403 as "The service understood the
request but refuses to authorize it". No SM interface declares an
authorization error: `UML/classes/i_admin_archive.adoc` §Functions lists only
`ehr_id_does_not_exist`.

**How to apply:** this bites hardest on EXTENSION routes (AMB-33, AMB-37,
AMB-41), whose own bindings say "no openEHR spec governs this — our own
design/extension" and "the status choice is ours". Also check the case's
premise is DECLARED: a `-forbidden` row calling itself "the non-administrative
principal" needs the party to declare an administrative principal too,
otherwise the separation it asserts is unverifiable for that deployment.
