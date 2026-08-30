# The hosted console (console.veredictum.eu)

The hosted console is the public face of the instrument: the catalogue, the
party statements, and the vendored specification oracle, served by the
released console image on Vercel — and a working instrument, by owner ruling
(#375): a visitor can connect a publicly reachable CDR and drive a real run
from the browser. It exists so anyone can read the conformance record, and
try the instrument, without installing anything (#348).

## What runs

One Vercel container service, `console`, rooted at the repository top
(`vercel.json`). Vercel's container entrypoint accepts only the fixed names
`Dockerfile.vercel`/`Containerfile.vercel`, resolved against the service's
`root` (<https://vercel.com/docs/functions/container-images>), so the overlay
lives at `/Dockerfile.vercel`, where `COPY` reaches the catalogue and the
specs.

The overlay is `FROM ghcr.io/rubentalstra/veredictum:latest`, the release
pointer the scan-and-tag leg moves on a real release tag. Vercel's build is a
pull plus data layers: the exact release bytes, with `artifacts/`,
`specs/openehr/` and `party/` baked into `/work` from the checkout Vercel
builds. The compose stack mounts the same paths; the hosted instance bakes
them because Vercel has no persistent filesystem.

The engine ships in the image at `/usr/local/bin/veredictum`, named by
`VEREDICTUM_ENGINE`, and the overlay leaves that inherited, which is what
makes the run screens work here. What the overlay adds for it is `/work/out`:
the baked `/work` tree is root-owned and the distroless base has no shell, so
the directory is created in a `busybox` helper stage and copied in owned by
uid 65532.

Runs write that directory on the instance's ephemeral filesystem and vanish
on the next redeploy — the sandbox's honest lifecycle. A record worth keeping
is produced on the operator's own machine with the same image.

## How a deploy happens

Vercel's git-triggered deployments are OFF (`vercel.json`
`git.deploymentEnabled: {"**": false}`). The only trigger is the project's
Deploy Hook, pinged by `.github/workflows/sandbox-deploy.yml` on exactly
three occasions:

1. **A real release.** `release.yml` calls the workflow after the
   scan-and-tag leg applies the image tags, passing the version the tag
   names. A pre-release moves no `:latest`, so it deploys nothing.
2. **A posture push.** A push to `main` touching `Dockerfile.vercel`,
   `vercel.json` or `deploy/vercel/**` redeploys the same release image with
   the new posture and the current main's baked data.
3. **A manual `workflow_dispatch`.**

The workflow verifies before and after. Before the ping, a release call
asserts `:latest` already is the tagged image, by digest, so a stale tag
fails fast instead of deploying the previous release. After the ping, the
poll asserts the served version: the console's server-rendered footer
carries `engine X.Y.Z`, and the check-console-pin guard holds that pin to
the console version, so the footer is the deployed version. A bare 200 is
never accepted as proof, because the old deployment answers 200 too. A run
with no expected version derives it from the `:latest` image's own
`org.opencontainers.image.version` label.

## One-time provisioning (owner actions)

1. Connect the Vercel project to this repository (done).
2. Create a Deploy Hook (Vercel project → Settings → Git → Deploy Hooks,
   branch `main`) and store it: `gh secret set SANDBOX_DEPLOY_HOOK_URL`
   (done).
3. Add `console.veredictum.eu` as the project domain and point the DNS
   CNAME at Vercel. The verification poll targets that host, so the first
   deploy run stays red until the domain resolves.
