---
name: hosted-instrument-always-on
description: console.veredictum.eu is the official conformance instrument on one always-on machine (owner ruling #388); Vercel was retired by #394 because an autoscaling request platform kills a run
metadata:
  type: project
---

console.veredictum.eu is the **official conformance instrument** (owner ruling
2026-08-30, #388): a run performed there is an official run and produces an
official record. Nothing about it is a demonstration, and the word "demo"
appears on no surface. A local run is that operator's own claim, which is the
entire reason the hosted instance exists.

**It runs as one always-on machine, and Vercel is retired** (#394, merged
2026-08-31). The reason is measured, not aesthetic (#387): an autoscaling
request platform serves one service from several instances, so a poll reaches
one that never held the run; an instance with no traffic for five minutes is
terminated with the engine child inside it; and each instance has a filesystem
the others cannot see. `deploy/hosted/` carries the posture, the image overlay
and the reasoning; `.github/workflows/hosted-deploy.yml` is the only thing that
deploys.

**Why:** a conformance run is a process that outlives the request that started
it, and the hosted instrument is what makes an official record possible at all.

**How to apply:** the instance holds nothing durable and no signing key in any
form — a console record is signed by the registry key in the `registry-signing`
CI environment, never on the host. What is left is an owner action (#403): the
app, the domain and `FLY_API_TOKEN`. See [[use-pr-stacks]] for the merge-train
lesson and [[console-ui-direction]] for the console's design record.
