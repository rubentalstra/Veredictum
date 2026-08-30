# Memory index

- [Datetime lexical form](datetime-lexical-form.md) — Z vs +00:00 in a queried cell is a SHOULD; asserting the committed spelling is a catalogue defect
- [Aggregate values are quantities](aggregate-values-are-quantities.md) — AQL MIN/MAX date/time cells are computed quantities with no committed lexeme
- [Law b aborts steps](law-b-aborts-steps.md) — a failing step aborts the rest; later steps are UNDRIVEN, never "passed"
- [Version envelope in hand](version-envelope-in-hand.md) — "no commit_audit.change_type" is the driver mistaking a served COMPOSITION for the VERSION envelope
- [Version count is per container](version-count-per-container.md) — REVISION_HISTORY counts ONE container; a cross-container sum is a catalogue defect
- [Lifecycle state is a coded term](lifecycle-state-coded-term.md) — assert the full terminology::code|rubric|, never the bare code
- [Access control is delegated](access-control-is-delegated.md) — an expect-forbidden role boundary has no released ground; SM hands the choice over
