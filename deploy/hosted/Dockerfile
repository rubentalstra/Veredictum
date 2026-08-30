# SPDX-FileCopyrightText: Veredictum contributors
# SPDX-License-Identifier: Apache-2.0
#
# The hosted instrument's image: the published release, plus the data a
# deployment with no volume needs baked in (#394).
#
# console.veredictum.eu is the official conformance instrument, so what it
# runs has to be the release bytes and nothing else: this file is FROM the
# published image and adds layers of committed data. `:latest` is the release
# pointer `scan-and-tag` moves on a real release tag and never on a
# pre-release, and the only thing that deploys is
# `.github/workflows/sandbox-deploy.yml`, which a release pings after that leg
# succeeds — so the reference always resolves and no cut-time bump exists.
#
# The instance holds nothing durable. A run's artifacts live under /work/out
# long enough to be judged, shown and submitted; a redeploy ends the runs in
# flight and costs no published record, because git is where a record lives.

# The base runs as uid 65532 and the /work tree COPYed below is root-owned, so
# the output directory has to arrive already owned by that uid. distroless
# carries no shell, so no RUN can make it there: this stage has one.
FROM busybox@sha256:9db7b59979c38555a39def84a31fb98b5296952f9e3afd4f6f11f05b07adfab0 AS mkdirs
RUN mkdir -p /empty/out

# The moving tag is the mechanism rather than an oversight: `:latest` is the
# release pointer `scan-and-tag` moves on a real release tag, so the hosted
# instrument follows releases without a hand-edited version here. A digest
# would have to be bumped at every cut, in a file the cut does not touch.
# hadolint ignore=DL3007
FROM ghcr.io/rubentalstra/veredictum:latest

# The base image already runs as uid 65532 (distroless nonroot); stated
# explicitly so the posture is a property of THIS file too (trivy DS-0002).
USER 65532:65532

# The compose stack mounts the repository at /work; the hosted instance has
# no volume, so the same paths are baked from the checkout the deploy builds.
COPY artifacts/ /work/artifacts/
COPY specs/openehr/ /work/specs/openehr/
COPY party/ /work/party/

# The runs directory, owned by the console's own uid so a run can create its
# job directory. It arrives from the helper stage above and starts empty, the
# same state a fresh local mount starts from.
COPY --from=mkdirs --chown=65532:65532 /empty/ /work/

# ROOT and SPECS are already /work-anchored in the base image; PARTY and OUT
# default to paths relative to /app there, so both are pinned here.
ENV VEREDICTUM_PARTY=/work/party \
    VEREDICTUM_OUT=/work/out

# The public posture (#390): anyone may name the endpoint this instance drives,
# so a target only this container can reach is refused before a socket opens.
# The console refuses to start on any other value, so a typo here fails the
# deploy rather than dropping the guard.
ENV VEREDICTUM_POSTURE=hosted

# The console reads its bind address from LEPTOS_SITE_ADDR at startup and
# defaults to loopback, which no host outside the container can reach. The
# port is the base image's own EXPOSE, and `deploy/hosted/fly.toml` names the
# same number as the service's internal port.
ENV LEPTOS_SITE_ADDR=0.0.0.0:3000
