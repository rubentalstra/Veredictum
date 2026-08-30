---
name: spec-defects-confirmed
description: Defects and internal divergences confirmed first-hand in the vendored released openEHR text — where they are, so they are not rediscovered
metadata:
  type: reference
---

Confirmed first-hand in the vendored released text. Each is a pointer, not an
adjudication; re-verify before acting. Related: [[aql-containment-and-folders]].

- `RM/docs/UML/classes/folder.adoc` §Invariants — `Folders_valid: not
  folders.is_empty` is stated UNCONDITIONALLY while `folders` is `0..1`.
  Sibling tables (e.g. `ehr.adoc`) use the `X /= Void implies …` guard form.
- `QUERY/docs/AQL/*.adoc` — the token `VERSION` and the grammar alternative
  `versionClassExpr` exist in `grammar/AqlParser.g4` (normative, included by
  `master07-grammar.adoc`) but `VERSION` appears ZERO times in the AQL prose
  chapters. A grammar-only class operand with no prose semantics.
- SM `RESULT_SET` (`SM/docs/UML/classes/result_set.adoc`) vs ITS-REST
  `specifications/schemas/query/ResultSet.yaml`: SM makes `columns` 1..1 and
  `rows` 0..1; ITS-REST requires `rows` and makes `columns` optional. SM also
  carries `id`, `creation_time`, `query` which the ITS-REST schema does not.
  SM's `rows` meaning reads "Rox data" (typo).
- `CNF/docs/platform_test_schedule/master11-func_tc_querying.adoc` L74 —
  a released heading reading `==== Test Case bbbb`, an unfilled placeholder.
- ITS-REST `specifications/operations/contribution_create.yaml` §Simplified
  Formats says the inner `versions[i].data` may be an `EHR_STATUS` or `FOLDER`
  in FLAT/STRUCTURED form, but the STABLE
  `ITS-REST/docs/simplified_formats/master05-rm_mapping.adoc` maps COMPOSITION
  and its content ONLY — no EHR_STATUS or FOLDER mapping exists.
- `RM/docs/UML/classes/locatable.adoc` — `archetype_node_id` is described as
  "Always in the form of an at-code, e.g. `at0005`", contradicted two
  sentences later by the archetype-root rule in the same cell.
- `RM/docs/ehr/master04-ehr_package.adoc` L104 spells `LOCATABALE`.
- CNF (stalled guide, not authority): every `.opt` under
  `CNF/tests/platform/robot/_resources/test_data_sets/` is COMPOSITION- or
  ENTRY-rooted; and the EHR_STATUS/FOLDER "valid" JSON fixtures carry a full
  archetype id in `archetype_node_id` with NO `archetype_details`, which fails
  RM LOCATABLE `Archetyped_valid`.
- `CNF/docs/platform_test_schedule/master08-func_tc_ehr_contribution.adoc`
  §EHR_STATUS accepted-cases table lists 15 rows for 16 combinations and
  repeats `false|true|…` three times.
- ITS-REST error body, TWO released shapes that share only `message`:
  `specifications/docs/overview/Requests_and_responses.md` §HTTP status codes
  illustrates `{message, code, errors[DV_CODED_TEXT]}`, while
  `specifications/schemas/others/Error.yaml` requires
  `{message, validationErrors[]}` and defines neither `code` nor `errors` —
  the release's own example is invalid against its own schema. Adjudicated as
  register AMB-217, upstream 2612.
- ITS-REST `Location` on refusals: `Requests_and_responses.md` §Location says
  "The `Location` header MUST ONLY be used for resource creation (e.g.,
  `201 Created`) or redirect responses", yet every `responses/412_*.yaml` and
  `responses/409_*_with_uid_based_id.yaml` declares a `Location` header
  (`headers/Location_deprecated.yaml`, `deprecated: true`).
- ITS-REST docs §HTTP status codes tables 401, 403, 415, 500 and 501, and
  §Authentication and authorization makes a MUST about WWW-Authenticate, but
  NO OAS response object anywhere declares any of those five codes and every
  bundle carries document-level `security: []`.
