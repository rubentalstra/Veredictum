---
name: version-envelope-in-hand
description: Confirmed attribution pattern — "the served envelope carries no commit_audit.change_type" is the driver's in-hand shortcut mistaking a served COMPOSITION/PARTY for the ORIGINAL_VERSION envelope, a runner defect
metadata:
  type: project
---

A red row reading `version change_type: the served envelope carries no
commit_audit.change_type.defining_code.code_string` attributes to the RUNNER
(`app/veredictum/src/exec/driver.rs`, `version_envelope`), never to the SUT.

**Why:** `version_envelope` returns the step's own body whenever
`envelope_uid(body) == version_uid`, testing the uid ALONE. RM common
`UML/classes/version.adoc` §Attributes makes `VERSION.uid` an
`OBJECT_VERSION_ID` "in the form of an `{object_id, a version_tree_id,
creating_system_id}` triple", and ITS-REST
`specifications/docs/overview/Requests_and_responses.md` §"ETag and
Last-Modified" says "The `ETag` value is usually taken from e.g.
VERSIONED_OBJECT.uid.value, VERSION.uid.value" — so a `Prefer:
return=representation` COMPOSITION/PERSON body carries that SAME
`OBJECT_VERSION_ID` in `LOCATABLE.uid` and passes the test. But
`commit_audit` is on the VERSION (`version.adoc` §Attributes) and the
resource sits UNDER it (`original_version.adoc` §Attributes: `data: T
[0..1]`, "Data content of this Version"), so the judge finds nothing.

**How to apply:** the discriminator is the LAST step's body. A case whose
last step returns a representation of the resource fails; a sibling whose
last step is a DELETE, a 4xx, or a versioned-object read PASSES on the same
server (verified 2026-08-30 on FerroEHR 4.0.11: 30 change_type cases passed,
20 failed, and every failure's last step returned the resource). Fix: also
require the body to BE a VERSION before short-circuiting. Related:
[[uid-pattern-tautology]].
