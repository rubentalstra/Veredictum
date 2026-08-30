---
name: am-data-conformance-closure
description: Where AM defines (and does not define) data-to-constraint correspondence — the "closed archetype" question, cardinality/existence/occurrences, and which AM documents are STABLE vs DEVELOPMENT
metadata:
  type: reference
---

Navigation for "is a data node that matches no constraint node invalid?" and for
cardinality/existence/occurrences semantics. Related:
[[spec-defects-confirmed]].

Owning sections (re-read first-hand; this file is only a map):

- `AM/docs/ADL1.4/master05-cadl.adoc` — §Attribute Constraints/Existence,
  §Single-valued Attributes (the only "matched by the data" principle),
  §Container Attributes/Cardinality, §Container Attributes/Occurrences (carries
  rule **VCOC**).
- `AM/docs/ADL2/master04.3-cadl_complex_types.adoc` — the ADL2 twin of all
  four, plus §Runtime Paths.
- `AM/docs/AOM2/master04.5-constraint_model-class_definitions.adoc` — the coded
  container-attribute rules **VACSO, VACMCU, VACMCO, VCACA, WACMCL, VSANCC**.
  ADL1.4/AOM1.4 have no coded equivalents beyond VCOC.
- `AM/docs/AOM2/master08-validation.adoc` — ARCHETYPE validation phases only,
  never data validation. Do not cite it for instance conformance.
- `AM/docs/AOM1.4/master04-constraint_model_package.adoc` §Semantics plus
  `AM/docs/UML/classes/org.openehr.am.aom14.*.adoc` for the AOM1.4 class tables
  (`c_attribute`, `c_multiple_attribute`, `c_object`, `c_defined_object`).
- `BASE/docs/architecture_overview/master10-archetypes.adoc` — §Relationship of
  Archetypes and Templates to Data, §Archetype-enabling of Reference Model
  Data, §Archetypes and Templates at Runtime/Validation during Data Capture.
  This is where the nearest "data conform to the template" prose lives.

Document status traps:

- `AM/docs/OPT2/manifest_vars.adoc` is `:spec_status: DEVELOPMENT`. OPT2 is NOT
  a released oracle. OPT **1.4** has no prose document at all — it exists only
  as the released XSD `specs/its-xml-schemas/its-xml-2.0.0-nsv2/AM/Release-1.4/
  Template.xsd` (`OPERATIONAL_TEMPLATE` complexType), which is structural only.
- ADL1.4, AOM1.4, ADL2, AOM2 and the BASE Architecture Overview are all
  `:spec_status: STABLE`.
- The `specs/openehr/README.md` component table's version labels DRIFT from
  `<component>/PROVENANCE.md`. Trust PROVENANCE.md.

RM floors for the container rows (needed with any cADL cardinality question):
`RM/docs/UML/classes/cluster.adoc` states existence only and NO non-empty
invariant; the list floor is recoverable only from the released bundles —
`specs/its-json-schemas/RM/Release-1.1.0/Data_structures/CLUSTER.json`
(`items` required, `minItems: 1`) and `specs/its-xml-schemas/
its-xml-2.0.0-nsv2/RM/Release-1.0.4/DataStructures.xsd`. `ITEM_TREE`/
`ITEM_LIST` have neither.
