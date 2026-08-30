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
- LIVE since 2026-08-30 (deploy run 33320229889 green end to end: hook →
  buildah build → promote → `engine 0.1.1` served). Two lessons paid for on
  the way, both already paid once in FerroEHR:
  - The root `.dockerignore` must NOT exclude the baked trees (buildah reads
    only the root-level ignore file, never the Dockerfile-adjacent form —
    buildah#4236; the first build died at `COPY artifacts/`). One root file,
    the FerroEHR shape (#367).
  - Vercel forwards to `$PORT` whose PLATFORM default is 80 — an image-level
    `ENV PORT` changes nothing; rebind `LEPTOS_SITE_ADDR=0.0.0.0:80` and set
    `PORT=80` (#368, the FerroEHR#2947 fix). The symptom is "Application
    initialization timed out" + 500 on every route.
  Before replicating ANY Vercel container setup: read FerroEHR's merged
  `fix(sandbox)`/`ci(sandbox)` PR list first — every failure mode so far was
  already fixed there.

Related: [[release-conventions]], [[use-pr-stacks]].
