# The hosted instrument (console.veredictum.eu)

console.veredictum.eu is the official conformance instrument (#388). A run
performed there is an official run and produces an official record: the console
drives the catalogue against the endpoint a submitter names, records every
exchange, computes the verdicts, and submits the whole record as a pull request
that CI re-derives and signs. Anyone may run the console themselves, and a local
run is that operator's own claim — which is the entire reason the hosted
instance exists.

It also serves the reading surfaces: the catalogue, the party statements, the
vendored specification oracle, the boards, and record verification.

## Why a box, and not a platform

A conformance run is a process that outlives the request that started it: the
console spawns the pinned engine, streams its output for minutes, and writes the
record to the container's own filesystem. An autoscaling request platform breaks
all three, and #387 measured it live — several instances answered one service,
so a poll reached one that never held the run and the page said "No run is in
flight" about a run that was still executing; an instance with no traffic for
five minutes was terminated with the engine child inside it; and each instance
had a filesystem the others could not see.

So the instrument runs on one machine that does not stop. The cost of that
choice is ownership: the firewall, the certificate, the deploy path, the log
rotation and the answer to "is it up" are all things this directory states.

## The machine

A Hetzner **CX33** — 4 vCPU x86, 8 GB RAM, 80 GB NVMe — in Falkenstein or
Nuremberg. #412 carries the reasoning, and the short version is that the cap is
two concurrent runs, each an engine process loading the whole catalogue, beside
the console, a proxy and the operating system. The cheaper 4 GB box would
probably hold it and leaves nothing for a spike, and the failure it avoids is
the OOM killer ending a conformance run halfway through — which looks exactly
like a defect in the instrument.

Not Arm: the CAX line is the more expensive half since April 2026, and the image
publishes for both architectures, so it buys nothing here.

## What is in this directory

| File | What it is |
|---|---|
| `cloud-init.yaml` | A fresh box to serving state on first boot: the `deploy` user, key-only SSH, both firewalls, unattended upgrades, Docker, capped logs, and the one command the CI key may run |
| `docker-compose.yml` | What runs on the box — the console with its healthcheck and memory limit, behind Caddy |
| `Caddyfile` | Automatic TLS, and the raised timeouts a run's streaming needs |
| `Dockerfile` | The hosted image: the published release plus the data a volumeless instance cannot mount |
| `env.example` | The environment file the box holds, including the one credential that lives there |

The box holds **no checkout of this repository**. Everything the instrument
needs is baked into the image, which CI builds and pushes; the box only pulls.

## What the instance keeps, and what it never holds

Nothing durable. A run's artifacts live long enough to be judged, shown and
submitted, and git is where a record lives. Because the box does not restart,
the console sweeps expired run directories itself on a stated policy —
`ARTIFACTS_KEPT`, currently a day — and a swept run answers through #386's
honest "this console knows nothing about that run" rather than a stack trace.

**No signing key reaches this host, in any form.** A console record is signed by
the registry key, which exists only in the `registry-signing` CI environment
(`registry/keys/README.md`). That is the point: the host a visitor can reach
must hold nothing that could forge a record.

**One credential does live here**: the console's GitHub App private key, which
opens a submission's pull request (#391). It cannot sign a record, and it sits
in an environment file the service user alone can read.

**No backups**, deliberately. There is nothing here worth backing up: the
container is rebuilt from a published image, and every published record is in
git.

## How a deploy happens

`.github/workflows/hosted-deploy.yml` is the only thing that deploys, in two
stages, on exactly three occasions:

1. **A real release.** `release.yml` calls it after the scan-and-tag leg applies
   the image tags. A pre-release moves no `:latest`, so it deploys nothing.
2. **A posture push.** A push to `main` touching `deploy/hosted/**` rebuilds the
   same release image with the new posture and the current main's data.
3. **A manual `workflow_dispatch`.**

The lane asserts by digest that `:latest` is the tagged image before it builds
anything, builds the hosted overlay and pushes it, and then deploys through
[`rubentalstra/hetzner-deploy-action`](https://github.com/rubentalstra/hetzner-deploy-action),
which was written for this. It waits for the container's own healthcheck and
then fetches the public URL **from the runner**, requiring the served engine
version — because a bare 200 is not proof, the deployment being replaced answers
200 too.

## How it is watched

Four layers, each answering a different question:

1. The container's healthcheck plus `restart: unless-stopped` — is it serving,
   and if not, is it coming back?
2. The deploy's own verification — did what we just shipped actually land?
3. `.github/workflows/hosted-watch.yml`, every fifteen minutes — is it up, and
   is it serving the release it should? It opens one issue and reuses it, then
   closes it on recovery, so an open issue always means a live problem.
4. Log rotation with a size cap, because a box that fills its disk fails in a
   way that looks like nothing at all.

## Provisioning

#403, once: buy the box with `cloud-init.yaml` as its user data, point
`console.veredictum.eu` at it, and store a deploy key restricted to one command
as `HOSTED_SSH_KEY` with the host's key as `HOSTED_KNOWN_HOSTS`. No Hetzner API
token is created for CI at all — the deploy talks to the host and nothing else,
so a key that leaks cannot destroy the server.
