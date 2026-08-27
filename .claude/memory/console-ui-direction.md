---
name: console-ui-direction
description: "The owner-ratified direction for the web console's UI — FerroEHR shell, Veredictum palette, everything-in-the-UI, signed verifiable records"
metadata: 
  node_type: memory
  type: project
  originSessionId: 32ee1aff-5e0d-4b1b-a080-d604f6efa1bb
  modified: 2026-08-27T08:54:23.560Z
---

Owner direction (2026-08-27) for `app/veredictum-console`: the FerroEHR admin
console's look and feel carries over — the calm static sidebar shell, the
one-kit-per-surface component discipline (`page_header`, `stat_card`,
`data_table`, `empty_state`, `toast`+inline doctrine) — but with Veredictum's
own palette (the `--ver-*` tokens in `website/landing/style.css`: teal
#258bb0, orange #ff861c accent, warm paper surfaces) and the seal as the
identity. Everything is displayed IN the UI; downloads exist only as the last
step: a signed record bundle, a badge SVG, a self-contained report. Trust
story: detached PGP signature over a digest manifest (engine issue #62),
verified by a public `/verify` screen and a `verify-record` CLI verb.

**Why:** the console is a public-facing verifier surface; the owner wants it
"calm and proper" and tamper-evident ("trust is good but verification is
better").

**How to apply:** the screen-level design record is issue #61 (flow diagram,
S1–S9 wireframes); #52 is the architecture beneath it. Implement screens only
against ratified #61 sections; the engine signing half is #62. Related:
[[migration-state]].
