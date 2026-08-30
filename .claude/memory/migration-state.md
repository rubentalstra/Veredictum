---
name: migration-state
description: "Historical: the instrument was built inside FerroEHR and moved to this repository on 2026-08-26; what that move left behind that still matters"
metadata:
  node_type: memory
  type: fact
  originSessionId: 32d068af-12e7-4654-9ece-124240b2367f
  modified: 2026-08-30T00:00:00.000Z
---

Veredictum was built inside
[FerroEHR](https://github.com/rubentalstra/FerroEHR) as a workspace member and
moved to this repository on 2026-08-26 under
[FerroEHR#2789](https://github.com/rubentalstra/FerroEHR/issues/2789). That move
is finished. The runner, the catalogue, the corpora, the ambiguity register, the
party declarations and the vendored spec oracle live here, releases ship from
here, and the crate is on crates.io. This file is the origin record.

## What the move left behind that still matters

- `git filter-repo` carried 611 commits out of the mono-repo and re-rooted them.
  A rewrite cannot preserve signatures, so the carried history is UNSIGNED.
  Signing resumes with the merge commit that landed it.
- The package, the binary and the library are `veredictum`, the bin source is
  `app/veredictum/src/bin/veredictum.rs`, and the debug switch is
  `VEREDICTUM_DEBUG_EXCHANGES`. No trace of the old crate name remains outside
  the vendored spec text.
- Three released machine-readable bundles were vendored separately, outside the
  filter-repo path set, so they carry no history here:
  `specs/its-xml-schemas/`, `specs/its-json-schemas/`, `specs/rest-oas/`. They
  are load-bearing: removing the XSD bundle alone produces 14 `spec-ref`
  findings.
- `clippy.toml` and `deny.toml` were adapted rather than copied. The method bans
  drop `std::env::var`, because the instrument reads credentials from the
  environment by design and the ixit declares only the variable name, and
  `uuid::Uuid::new_v4`, because there are no database keys here.
- The OpenSSF Best Practices criteria adjudication stays DEFERRED by owner
  decision 2026-08-26. Do not propose statuses until the owner picks it up.

## How to apply

- A change to instrument behaviour lands here, and a release ships from here.
- A spec question is answered from `specs/openehr/` in this repository,
  first-hand. XSD, JSON-Schema and OpenAPI citations resolve against the three
  bundles beside it, because the docs tree carries only prose for those
  components.
- Cite FerroEHR#2789 as the origin of a decision that traces to the move. It is
  not an open contract on this side: whether FerroEHR's own pipeline pins the
  published crate is tracked in FerroEHR.
- Prose that describes absent machinery is a defect. When something lands, the
  claim about it is corrected in the same change; when something is genuinely
  missing, it says so and names its issue rather than promising a future.
