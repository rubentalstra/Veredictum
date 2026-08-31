---
name: hosted-instrument-box
description: console.veredictum.eu runs on a Hetzner box this repository provisions (owner ruling 2026-08-31); hosting is the owner's call and I proposed migrations twice before being told
metadata:
  type: project
---

console.veredictum.eu is the **official conformance instrument** (owner ruling
#388): a run performed there is an official run and produces an official record.
Nothing about it is a demonstration, and the word "demo" appears on no surface —
`scripts/checks/hosted-instrument-language.sh` keeps it that way.

**It runs on a Hetzner box this repository provisions** (#412, #403; owner ruling
2026-08-31). `veredictum-console`, a CPX12 in Nuremberg (`eu-central`), dual-stack. Vercel is
retired and its project deleted. `deploy/hosted/` carries the whole posture as
code — the cloud-init, the compose file, the Caddyfile, the image overlay CI
builds — and the box holds no checkout.

**Why not a platform:** #387 measured it. Several instances answer one service,
so a poll reaches one that never held the run; an idle instance is stopped with
the engine child inside it; each has its own filesystem.

**Hosting is the owner's decision, and I got ahead of it twice.** I proposed
Fly.io and it was refused ("we keep vercel!!!"), then proposed it again and was
refused for a box. Do not propose a host migration. Present the constraint and
the costs, then wait.

**Why:** the bill is theirs, and a recurring charge on someone else's account is
never a technical call.

**How to apply:** the caps are environment values, not constants —
`VEREDICTUM_MAX_CONCURRENT_RUNS` in the box's `.env` plus the compose memory
limit, which move together on a resize and are never a code change. An upgrade is
already intended. No signing key ever reaches the host; see
[[console-tier-trust-model]] for what a console record's trust actually rests on.
