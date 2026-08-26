---
name: release-conventions
description: Release titles are the bare version (0.0.1-alpha.1), never the product name; immutable releases get enabled in Settings before the first non-alpha cut
metadata:
  type: feedback
---

A GitHub release's TITLE is the bare version string — `0.0.1-alpha.1` — never
"Veredictum 0.0.1-alpha.1". The repository name already says whose release it
is; no other software prefixes the product name there.

**Why:** owner correction 2026-08-26 on the first cut ("call it 0.0.1-alpha.1
not the full name — no other software does it like that").

**How to apply:** `gh release create <tag> --title "<bare version>"`; a
release workflow passes the tag minus the `v`. Also owner-stated the same
day: releases here are IMMUTABLE — the Settings toggle (no API) must be on
before the first non-alpha cut; the repair for a bad cut is a new version,
never a retag. Zenodo archives each release (concept DOI
10.5281/zenodo.22113258); `CITATION.cff` carries the concept DOI and the
README badge points at it.
