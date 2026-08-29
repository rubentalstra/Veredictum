---
name: datetime-lexical-form
description: Confirmed attribution pattern — a queried DV_DATE_TIME lexical form (Z vs +00:00) is a SHOULD, so asserting the committed spelling is a catalogue defect, never a SUT defect
metadata:
  type: project
---

A red row whose only difference is the UTC spelling of a date/time cell
(`2026-01-01T00:00:00Z` vs `2026-01-01T00:00:00+00:00`) attributes to the
CATALOGUE, not the SUT and not the comparator.

**Why:** ITS-REST `specifications/docs/overview/Resources.md` §Datetime format
splits the two directions deliberately, and `docs/overview/Preface.md`
§Requirements binds RFC 2119/8174 keywords: the write path is a plain
statement of fact ("Any date, datetime or time value provided as part of the
HTTP message body … will be preserved as it was sent by the client, and passed
to the underlying backend engine as is"), while the read path is only a SHOULD
("Retrieval or querying those resources SHOULD return date, datetime, or time
values in the (original) format provided by underlying backend engine, avoiding
any format change"). Nothing else narrows it: BASE
`UML/classes/iso8601_timezone.adoc` §Description states "`Z` is a literal
meaning UTC … i.e. timezone `+0000`"; BASE `UML/classes/iso8601_date_time.adoc`
§Description admits `[Z | ±hh[:mm]]` as alternatives; ITS-REST
`schemas/query/ResultSetRow.yaml` types a cell as untyped `ANY`; SM
`UML/classes/result_set_row.adoc` types `values` as `List<Any>`; ITS-JSON
`DV_DATE_TIME.json` gives `value` a bare `{"type":"string"}` with no pattern.

**How to apply:** assert the instant, never the spelling, in a `result_set`
row. Do NOT "fix" this in the comparator — `exec/resultset.rs` compares string
cells by exact lexeme on purpose, and a semantic datetime comparator would also
erase the ability to test the write-path preservation sentence, which carries no
SHOULD. Related: [[aggregate-values-are-quantities]], [[law-b-aborts-steps]].
