---
name: lifecycle-state-coded-term
description: version lifecycle_state is compared as the full terminology::code|rubric| term, so a bare code in a case is a catalogue defect
metadata:
  type: project
---

A red row reading `version lifecycle_state: served "openehr::523|deleted|",
expected "523"` is a CATALOGUE defect: the case authored the bare code where
every sibling authors the full coded term.

**Why:** RM common `UML/classes/original_version.adoc` §Attributes types
`lifecycle_state` as a `DV_CODED_TEXT` "coded by openEHR vocabulary `version
lifecycle state`", so the comparison is over the whole term
(`terminology::code|rubric|`), never the rubric or the code alone —
`exec/versioned.rs::eval_lifecycle_state` implements exactly that.

**How to apply:** grep `lifecycle_state:` across `artifacts/schedule/**` and
check the hit is inside an `assert:` (full term required) and not inside a
`with:` request block (where the bare code is the wire parameter).
