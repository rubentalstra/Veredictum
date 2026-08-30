---
name: non-composition-archetyping
description: Where the released text grounds (and fails to ground) archetype/template constraint of FOLDER, EHR_STATUS, EHR, CONTRIBUTION and demographic PARTY — the R8 "no templates for non-COMPOSITION" question
metadata:
  type: reference
---

Map only; re-read first-hand. Related: [[am-data-conformance-closure]],
[[spec-defects-confirmed]].

**Can an archetype constrain the class?**

- `BASE/docs/architecture_overview/master06-design_of_the_ehr.adoc` §The
  Design of the openEHR EHR — the ENUMERATION of "top-level structures"
  (COMPOSITION, EHR_ACCESS, EHR_STATUS, FOLDER, PARTY, EHR_EXTRACT). EHR and
  CONTRIBUTION are absent from it.
- `BASE/docs/architecture_overview/master10-archetypes.adoc` §Relationship of
  Archetypes and Templates to Data — "Each top-level type is always guaranteed
  to be an archetype root point"; §Archetype-enabling of Reference Model Data
  ties archetypability to LOCATABLE inheritance.
- Per-class RM prose: `RM/docs/common/master05-directory_package.adoc`
  §Overview (FOLDER archetypes), `RM/docs/ehr/master04-ehr_package.adoc`
  §EHR Status + §Folders, `RM/docs/demographic/master02-demographic_package.adoc`
  §Overview (PERSON/ORGANISATION/ROLE archetypes).
- `RM/docs/UML/classes/locatable.adoc` §Invariants — `Archetyped_valid`,
  `Archetype_node_id_valid`; `archetyped.adoc` — `template_id` 0..1 with
  "Normally … only … at the top of a top-level structure".
- `AM/docs/Identification/master03-artefact_source_id.adoc` §Human-readable
  Identifier — `rm_class` is any RM class; `rm_closure` contemplates
  `DEMOGRAPHIC`. `AM/docs/AOM2/master11-rm_adaptation.adoc`
  §archetype_parent_class / §archetype_namespace. ADL1.4 says "COMPOSITION"
  ZERO times.
- OPT 1.4 = `specs/its-xml-schemas/**/AM/Release-1.4/Template.xsd`;
  `definition` is `C_ARCHETYPE_ROOT` : `C_COMPLEX_OBJECT`, and `rm_type_name`
  is plain `xs:string` in `Archetype.xsd` — NO COMPOSITION pin anywhere.
  `AM/docs/OPT2/` is DEVELOPMENT.
- Released demographic-archetype artifact:
  `openehr/ITS-XML/examples/AOM2/openEHR-demographic-ADDRESS.address-provider.xml`.

**Is the class WIRE-BOUND to template validation?** Only where 422 is bound.
Grep the bundled OAS `specs/rest-oas/*-validation.openapi.yaml` for `'422':` —
in the STABLE ehr API it appears on composition_create/update ONLY; in the
DEVELOPMENT demographic API on all ten PARTY-subtype create/update ops.
`responses/422.yaml` carries the "underlying template … not validating" text.
ehr_status_update, directory_create/update and contribution_create have NO 422
and mention neither archetype nor template.

**The one class-generic commit-validity rule:**
`RM/docs/common/master06-change_control_package.adoc` §Version Lifecycle —
data "missing mandatory data fields with respect to its generating archetypes"
committed in `complete` "would be treated as invalid by the repository and
rejected by the API"; `incomplete` relaxes existence/cardinality lower limits
to zero. ITS-REST binds no status code to it.
