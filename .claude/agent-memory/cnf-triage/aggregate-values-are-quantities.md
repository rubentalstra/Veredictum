---
name: aggregate-values-are-quantities
description: AQL date/time values are processed as quantities, not literal strings, so a MIN/MAX cell carries no committed lexeme
metadata:
  type: project
---

`MIN`/`MAX` over a date/time path return a computed quantity, so the returned
cell has no committed lexical identity to demand back.

**Why:** QUERY `docs/AQL/master03-syntax.adoc` §Dates and Times NOTE: "The
underlying types of date/time strings are inferred by the AQL processor from the
context … enabling them to be processed as date/time quantities rather than
literal strings by AQL engines." §MIN and §MAX fix only the TYPE ("Input values
type should be either String, Date, Time, Integer or Real, and it will also
determine the return type"), never a spelling, and
`docs/AQL/master04-result_structure.adoc` types the raw result as
`Array<Array<Any>>` while declaring the annotated result structure "not formally
defined by this specification".

**How to apply:** a case core may not read "the returned cell carries that same
type back" as "the returned cell carries the committed bytes back" — that step
is unsourced. Related: [[datetime-lexical-form]].
