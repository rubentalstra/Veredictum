# The hosted instrument (console.veredictum.eu)

console.veredictum.eu is the official conformance instrument (#388). A run
performed there is an official run and produces an official record: the
console drives the catalogue against the endpoint a submitter names, records
every exchange, computes the verdicts, and submits the whole record as a pull
request that CI re-derives and signs. Anyone may run the console themselves,
and a local run is that operator's own claim — which is the entire reason the
hosted instance exists.

It also serves the reading surfaces: the catalogue, the party statements, the
vendored specification oracle, the boards, and record verification.

## What runs, and why it never stops

One machine that is always on, described by `fly.toml` beside this file.

A conformance run is a process that outlives the request that started it: the
console spawns the pinned engine, streams its output for minutes, and writes
the record to the container's own filesystem. An autoscaling request platform
breaks all three, and #387 recorded it live — several instances answered one
service, so a poll reached one that never held the run and the page said "No
run is in flight" about a run that was still executing; an instance with no
traffic for five minutes was terminated with the engine child inside it. So
autostop is off, one machine is always running, and it is sized for the
concurrency cap the console enforces itself.

The image is `deploy/hosted/Dockerfile`: `FROM
ghcr.io/rubentalstra/veredictum:latest`, the release pointer the scan-and-tag
leg moves on a real release tag, plus the data a volumeless instance needs
baked into `/work` from the checkout — `artifacts/`, `specs/openehr/` and
`party/`. The compose stack mounts the same paths. The engine ships in the
base image at `/usr/local/bin/veredictum`, named by `VEREDICTUM_ENGINE`, and
the overlay leaves that inherited, which is what makes the run screens work.
What the overlay adds for it is `/work/out`: the baked `/work` tree is
root-owned and the distroless base has no shell, so the directory is created
in a `busybox` helper stage and copied in owned by uid 65532.

The overlay also sets `VEREDICTUM_POSTURE=hosted`, which is what turns on the
target guard (#390): anyone may name the endpoint this instance drives, so a
loopback, RFC 1918 private, link-local, unique-local, unspecified or multicast
target is refused before a socket opens, with the name resolved first and every
address it answers with checked. An operator's own console leaves the variable
unset and drives `localhost` as before. Any other value refuses to start, so a
typo fails the deploy instead of dropping the guard.

## What the instance keeps

Nothing durable, and no volume is attached. A run's artifacts live long enough
to be judged, shown and submitted; git is where a record lives. A redeploy ends
the runs in flight and costs no published record.

**No signing key reaches this host, in any form.** A console record is signed
by the registry key, which exists only in the `registry-signing` CI
environment (`registry/keys/README.md`). That is the whole point: the host a
visitor can reach must hold nothing that could forge a record.

## How a deploy happens

`.github/workflows/hosted-deploy.yml` is the only thing that deploys, on
exactly three occasions:

1. **A real release.** `release.yml` calls the workflow after the scan-and-tag
   leg applies the image tags, passing the version the tag names. A
   pre-release moves no `:latest`, so it deploys nothing.
2. **A posture push.** A push to `main` touching `deploy/hosted/**` redeploys
   the same release image with the new posture and the current main's baked
   data.
3. **A manual `workflow_dispatch`.**

The workflow verifies before and after. Before the deploy, a release call
asserts `:latest` already is the tagged image, by digest, so a stale tag fails
fast instead of shipping the previous release. After it, the poll asserts the
served version: the console's server-rendered footer carries `engine X.Y.Z`,
and the check-console-pin guard holds that pin to the console version, so the
footer is the deployed version. A bare 200 is never accepted as proof, because
the old deployment answers 200 too. A run with no expected version derives it
from the `:latest` image's own `org.opencontainers.image.version` label.

## One-time provisioning (owner actions)

1. Create the app and its machine from this directory:
   `flyctl launch --no-deploy --copy-config --config deploy/hosted/fly.toml`.
2. Create a deploy token for it and store it: `gh secret set FLY_API_TOKEN`.
3. Add `console.veredictum.eu` as a certificate on the app and point the DNS
   record at it. The verification poll targets that host, so the first deploy
   run stays red until the domain resolves.

## Why this host

The requirement is one process that may outlive a request, with outbound
access to whatever endpoint a submitter names, a custom domain, and no
durable state. Three candidates met it. A managed always-on machine was
chosen for having the smallest operational surface: one config file, one
token, TLS and the domain handled by the platform. A plain VPS with Docker
and a reverse proxy is cheaper and puts the TLS, the restarts and the host
patching in this repository's hands. A container platform that scales to zero
was refused outright — it is the class of host that broke the instrument in
the first place, and #387 has the measurements.
