---
name: unparsed-body-is-an-instrument-limit
description: "equivalent/field/instance_of over a body the binding negotiated as XML or text/plain is a RUNNER comparator gap, never a SUT defect — the runner says so itself"
metadata:
  type: project
---

A red row reading `equivalent: the SUT served a application/xml body, which this
runner does not parse into RM attributes … UNJUDGEABLE` attributes to the RUNNER
comparator, and the server is correct by construction: the BINDING pinned that
`Accept`.

**Why:** `app/veredictum/src/exec/driver.rs` `unjudgeable_on_unparsed_body`
routes the Field / Equivalent / ResultSet / InstanceOf / Signature families to
the inconclusive channel on a `BodyForm::Unparsed`, and its own doc comment
states the reason: "Judging the fact absent there would charge the SUT for the
instrument's own limit". The negotiated form is what the release asks for —
ITS-REST `specifications/docs/overview/Resources.md` §Data representation,
"Services MUST support at least one of the openEHR **XML** or **JSON** canonical
formats for resource representation" — and the binding
`I_DEFINITION_ADL14.get_opt.yaml` pins `Accept: application/xml` while
`I_DEFINITION_ADL2.get_artefact.yaml` pins `Accept: text/plain`.

**How to apply:** check the corpus form of BOTH sides before blaming anything —
`artifacts/corpus/MANIFEST.yaml` types these fixtures `opt-xml` and `adl2-text`,
i.e. the SAME non-JSON form the server served, so the comparison is well posed
and only the comparator is missing. This is the LIVE path and is distinct from
the replay-seam XML collapse in #346. Related: [[version-envelope-in-hand]].
