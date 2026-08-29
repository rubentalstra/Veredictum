---
name: aql-containment-and-folders
description: Where AQL containment semantics, FOLDER/directory RM definitions and the query RESULT_SET live in the vendored spec tree, plus the confirmed FOLDER silence in QUERY
metadata:
  type: reference
---

Routing for AQL containment / FOLDER / directory questions.

- AQL containment prose: `specs/openehr/QUERY/docs/AQL/master03-syntax.adoc`
  §Query structure > FROM > Containment (~L958) and §Operators > Logical
  operators > NOT (~L428, the FROM-clause exclusion paragraph). Class operand
  rules: §Expressions > Class expressions (~L775). FROM RM-binding paragraph:
  §FROM (~L920).
- Normative AQL grammar is included, not restated: `master07-grammar.adoc`
  includes `grammar/AqlParser.g4` (+ `AqlLexer.g4`). `containsExpr` and
  `classExprOperand` are the rules to read.
- **Confirmed 2026-08-29 by exhaustive case-insensitive grep: the string
  "folder" appears ZERO times in the entire QUERY component** (docs, examples,
  grammar). Anything about FOLDER containment in AQL is spec SILENCE.
- FOLDER class table: `specs/openehr/RM/docs/UML/classes/folder.adoc`
  (included by `RM/docs/common/master05-directory_package.adoc` §Class
  Descriptions). VERSIONED_FOLDER beside it.
- EHR.directory / EHR.folders: `specs/openehr/RM/docs/ehr/master04-ehr_package.adoc`
  §Folders (~L100-129); EHR class table at `RM/docs/UML/classes/ehr.adoc`.
- Query wire shape: `specs/openehr/ITS-REST/specifications/responses/200_Query.yaml`
  -> `schemas/query/ResultSet.yaml`; prose in
  `specifications/docs/query/Response.md` §RESULT_SET response. Note the
  ITS-REST docs text lives under `specifications/docs/<group>/*.md`, NOT under
  the top-level `docs/` dir (which only holds smart_app_launch,
  simplified_data_template, simplified_formats).
- SM query service: `specs/openehr/SM/docs/openehr_platform/master08-query_service.adoc`,
  class tables in `SM/docs/UML/classes/i_query_service.adoc`,
  `result_set.adoc`. Directory ops: `SM/docs/UML/classes/i_ehr_directory.adoc`
  (ten operations), anchored by `master05-ehr_service.adoc`.
- Catalogue citation strings in use: `"ITS-REST query §200_Query (RESULT_SET)"`,
  `"ITS-REST query Response.md §RESULT_SET response"`,
  `"QUERY AQL master03-syntax §Containment, §NOT"`,
  `"SM openehr_platform §I_QUERY_SERVICE.execute_ad_hoc_query"`.
