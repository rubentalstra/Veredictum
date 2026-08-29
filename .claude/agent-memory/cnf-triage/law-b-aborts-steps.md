---
name: law-b-aborts-steps
description: A failing step aborts the remaining steps of a case, so later steps in a red row are UNDRIVEN and must never be reported as passed
metadata:
  type: project
---

When a case fails at step N, steps N+1.. never execute
(`app/veredictum/src/exec/mod.rs`, the `break 'steps` arms labelled "law b").
`results.json` reports only `failing_step`, and the transcript will carry no
exchange for the later steps.

**Why:** a triage report that says "steps 2 and 3 passed" when they were never
driven is a false claim about coverage, which is the exact failure class this
instrument exists to prevent.

**How to apply:** before naming which assertion failed, count the exchanges in
`transcript.json` for the case and confirm the later steps produced none. Report
them as UNDRIVEN and list them in the not-fully-adjudicated section.
