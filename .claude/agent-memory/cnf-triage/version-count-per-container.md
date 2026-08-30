---
name: version-count-per-container
description: assert version count reads ONE container's REVISION_HISTORY, so a case summing versions across two versioned objects is a catalogue defect
metadata:
  type: project
---

`{ assert: version, count: N }` counts the `items` of ONE served
`REVISION_HISTORY`. A case that authors N as a sum across two distinct
VERSIONED_OBJECTs (e.g. EHR_STATUS versions + COMPOSITION versions) is a
CATALOGUE defect, and the lower observed count is the spec-correct answer.

**Why:** RM common `UML/classes/revision_history_item.adoc` §Description —
"An entry in a revision history, corresponding to a version from a versioned
container." The revision history is per-container by construction, and the
driver resolves exactly one family (`version_family`, which refuses to guess
between two).

**How to apply:** on a `version count` mismatch, first add up the containers
the flow touched. If the expected number only reconciles as a cross-container
total, the case is wrong. Also check the flow BINDS `versioned_object_uid` —
the container read takes it as a path parameter from an ambient capture, and
a case that omits `versioned_object_uid: created.versioned_object_uid`
errors with `version: path param {versioned_object_uid} unresolved` rather
than failing. Related: [[version-envelope-in-hand]].
