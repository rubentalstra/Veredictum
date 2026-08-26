---
name: product-identity-and-origin
description: "Owner decision 2026-08-26 — the product is named Veredictum, split out of FerroEHR as an independent conformance instrument"
metadata:
  node_type: memory
  type: decision
  originSessionId: 32d068af-12e7-4654-9ece-124240b2367f
  modified: 2026-08-26T00:00:00.000Z
---

Owner decision 2026-08-26: the openEHR conformance instrument is a separate
product named **Veredictum**, in its own public Apache-2.0 repository at
`github.com/rubentalstra/Veredictum`.

**The name.** Medieval Latin *vere dictum*, "truly spoken", the root of the
English *verdict*. Verdicts are the instrument's core output: the pure-function
verdict pipeline and its `verdicts.json`. Checked free on crates.io and as a
GitHub name at decision time, with no software or healthcare collisions.
Rejected runner-ups: `conformis` (live US healthcare trademark, same-field
confusion risk), `keur`, `assay`, `verax`, `titer`, `kriterion`, `probatio`.

**Why it is separate.** An independent tool defends a CDR's conformance claims
precisely because its workflow cannot leak or shortcut, and no solid
independent openEHR conformance tool exists today. The community said the same
thing on the announcement thread after the Apache-2.0 relicense, and
collaboration was offered from more than one side.

**How to apply:**
- The product is spelled `Veredictum` everywhere a human reads it as a name.
  Lowercase `veredictum` is for technical identifiers: crate and binary names,
  image names, URLs, environment-variable grammar.
- The scope is full product: own README and badges, own releases, own container
  image, and later a web UI over the runner beside the CLI.
- The pointer for everything about the split is
  [FerroEHR#2789](https://github.com/rubentalstra/FerroEHR/issues/2789).
