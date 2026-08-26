---
name: migration-state
description: "The runner code still lives in FerroEHR tools/cnf-runner until the #2789 extraction lands; this repo owns the identity and the tracker"
metadata:
  node_type: memory
  type: fact
  originSessionId: 32d068af-12e7-4654-9ece-124240b2367f
  modified: 2026-08-26T00:00:00.000Z
---

State as of 2026-08-26: the Veredictum repository holds the product identity,
the agent discipline, and the tracker. **The living code is still FerroEHR
`tools/cnf-runner`.** The migration contract is
[FerroEHR#2789](https://github.com/rubentalstra/FerroEHR/issues/2789).

**What has moved:** the name and the origin story (`README.md`), the root
`CLAUDE.md`, `.claude/rules/*`, `.claude/hooks/*`, `.claude/agents/*`,
`.claude/memory/*`, `AGENTS.md`, and `scripts/checks/comment-style.sh`.

**What has not:** the runner source, the catalogue artifacts, the vendored spec
text, the corpora and their PROVENANCE trees, the ambiguity register, the party
statements and IXIT examples, the workspace scaffolding
(`Cargo.toml`, `rust-toolchain.toml`, `clippy.toml`, `deny.toml`,
`rustfmt.toml`), every CI lane, and the container image.

**How to apply:**
- A change to runner behaviour lands in FerroEHR until the extraction
  completes. Do not re-implement it here.
- A spec question is answered from a FerroEHR checkout at
  `docs/specs/openehr/` until the spec text is vendored here, and the answer
  names which checkout was read.
- A rule or hook that references machinery not yet present says so in place.
  Do not delete such a rule to make the tree self-consistent, and do not write
  prose that implies the machinery exists.
- FerroEHR keeps its committed conformance baselines: they are claims about
  that CDR, not about this instrument. After the split, FerroEHR pins a
  released Veredictum version instead of building the runner from source.
