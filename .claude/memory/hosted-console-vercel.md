---
name: hosted-console-vercel
description: The public console runs on Vercel at console.veredictum.eu, view-only by construction, deploy-hook only — the FerroEHR sandbox pattern with the 2945 lesson
metadata:
  type: project
---

The hosted console (#348, PR #349, 2026-08-30) serves the public conformance
page at **console.veredictum.eu** — the owner already holds `veredictum.eu`
(same zxcs nameservers as `ferroehr.eu`), and the repository secret
`SANDBOX_DEPLOY_HOOK_URL` is set.

**Why:** anyone should read the catalogue, party statements and the spec
oracle without running anything; the delivery replicates the FerroEHR sandbox
lane that already works.

**How to apply:**
- One Vercel container service rooted at `.` — Vercel accepts only the fixed
  `Dockerfile.vercel` name resolved against the service `root`
  (FerroEHR#2945); with one service, rooting at the repo top is what lets
  COPY reach `artifacts/`, `specs/openehr/`, `party/` (baked into `/work`;
  Vercel has no volume).
- View-only is BY CONSTRUCTION, not configuration: the image ships no engine
  binary (`VEREDICTUM_ENGINE`/PATH resolution finds nothing), so no visitor
  can start a network-touching run, and distroless nonroot cannot write the
  baked tree. Never add the engine to this image without a deliberate
  security adjudication.
- Deploys: git-triggered deploys OFF in `vercel.json`; the only trigger is
  the Deploy Hook pinged by `.github/workflows/sandbox-deploy.yml` (release
  call after scan-and-tag moves `:latest`, posture push, manual). The
  verification polls the SSR footer's `engine X.Y.Z` string, never a bare
  200 (the old deployment answers 200 too — FerroEHR 4.0.4/4.0.10 lesson).
- Remaining owner action: add the domain to the Vercel project and CNAME
  `console.veredictum.eu` at Vercel; until DNS resolves, the deploy run's
  version poll stays honestly red.

Related: [[release-conventions]], [[use-pr-stacks]].
