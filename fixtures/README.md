<!--
SPDX-FileCopyrightText: Veredictum contributors
SPDX-License-Identifier: Apache-2.0
-->

# Test fixtures

Test material, at the repository root because both crates' test suites and the
browser-journey script read it by path. **Nothing here is a declaration about
any product, and no gate reads it as one.**

## Why a declaration lives under a fixtures tree

ISO/IEC 9646-7 splits an ICS in exactly one place: "All table cells should be
completed by the ICS proforma specifier, except the support and supported
values columns, which need to be completed by the supplier of the
implementation." The form is this repository's to author, and
`artifacts/vocab/capability_matrix.yaml` is that form: one row per capability,
each with its family, tier, `required` flag, `min_cases` floor and a spec
citation.

The support columns are the supplier's. ISO/IEC 17050-1 is titled *supplier's*
declaration of conformity, so a filled-in declaration written by the test
instrument is a contradiction in terms. A real declaration therefore travels
with the submission it describes: pasted into a console run, or carried as the
`statement` artifact of a registry entry.

What the test suites still need is the SHAPE of a filled-in declaration, so the
static conformance review can be seen to fire. That is what
`declaration/statement.json` is, and its product does not exist.

| Path | What it is |
|---|---|
| `declaration/statement.json` | A filled-in ICS for a product that does not exist, claiming every row of the capability matrix. The static conformance review (`veredictum validate --statement`) runs over it. |
| `declaration/ixit.json` | The ixit beside it, exercising every deployment posture the runner reads. Every address in it is fictional. |
| `smart-test-issuer/` | The SMART lane's committed test issuer: a public, deliberately non-secret key pair. Read its own README before touching it. |
