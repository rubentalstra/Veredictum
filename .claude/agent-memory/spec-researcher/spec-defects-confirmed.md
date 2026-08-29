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
