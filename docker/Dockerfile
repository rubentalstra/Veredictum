# syntax=docker/dockerfile:1
#
# The Veredictum container image: the WEB CONSOLE (#6, design in #52).
#
# ONE image, by owner ruling (2026-08-27): the container is the web frontend
# over the published instrument, and nothing else ships as an image. The CLI's
# own distribution channels are `cargo install veredictum` and the prebuilt
# binaries attached to each release — a terminal user never needed Docker for
# a static binary. The console binary serves the UI; it carries no catalogue
# and no judgement of its own, and every run executes through the ENGINE that
# ships beside it at /usr/local/bin/veredictum, named by VEREDICTUM_ENGINE in
# the runtime stage. The console refuses a binary reporting any version other
# than its pin, so the two travel as one artifact.
#
# The catalogue, the corpora and the vendored specification oracle stay
# MOUNTED: they are 347 MB the server reads as paths, and a party may point at
# their own. Run it locally as
#
#   docker run --rm -p 127.0.0.1:3000:3000 -v "$PWD:/work" \
#     ghcr.io/rubentalstra/veredictum:<tag>
#
# The server binds 0.0.0.0:3000 INSIDE the container (set below — a
# container-loopback bind would be reachable by nothing); the `127.0.0.1:` on
# the publish flag is what keeps the no-login console off the network, per the
# posture in app/veredictum-console/CLAUDE.md. Wider exposure is the
# operator's decision, behind their own gate.
#
# ONE runtime stage feeds TWO final targets, so a locally built image and a
# CI-published one can never drift
# (https://docs.docker.com/build/building/multi-stage):
#
#   runtime-from-source  the LOCAL path — cargo-leptos compiles the server
#                        binary, the WASM bundle and the Tailwind CSS
#                        in-image against persistent BuildKit cache mounts,
#                        and cargo compiles the engine binary beside them.
#   runtime-prebuilt     the CI path — COPY-only packaging of the
#                        context-provided native server and engine binaries
#                        (bin/${TARGETARCH}/) plus the
#                        architecture-independent site bundle (site/). No RUN
#                        executes, so a multi-architecture buildx needs no
#                        QEMU.
#
# The RUST_VERSION toolchain is single-sourced from rust-toolchain.toml, and
# the release pipeline cross-checks this ARG against its `channel` so the two
# cannot drift. The build TOOLS are fetched as PREBUILT release binaries,
# never `cargo install cargo-leptos` (that compiles a vendored OpenSSL C tree
# — roughly ten minutes per cold build; a FerroEHR lesson carried over).

ARG RUST_VERSION=1.97.1
ARG CARGO_LEPTOS_VERSION=v0.3.7
ARG TAILWIND_VERSION=v4.3.3

# ── mkdirs: the one stage with a shell, for a directory nobody can COPY ───────
# The runtime writes a run's artifacts under /work/out and runs as uid 65532,
# while everything COPYed below arrives root-owned. distroless carries no shell,
# so no RUN can create that directory there: it arrives from here instead,
# already owned by the uid that has to write to it.
FROM busybox@sha256:9db7b59979c38555a39def84a31fb98b5296952f9e3afd4f6f11f05b07adfab0 AS mkdirs
RUN mkdir -p /empty/out

# ── builder: toolchain + prebuilt tools, then cargo-leptos under the cache ────
# Digest-pinned rust:1.97.1-slim-trixie (Debian 13, glibc 2.41). The builder
# and the cc-debian13 runtime below share the Debian 13 generation, so the
# binary links exactly the glibc it runs against. Pinning by digest rather
# than by the mutable tag is what makes the base reproducible
# (https://docs.docker.com/build/building/best-practices — "pinning to a
# digest guarantees the same image version").
FROM rust:${RUST_VERSION}-slim-trixie@sha256:3b2879047d42784ca9403ad20c51ed3df361a50f1df96f5777d39b4e33aa65cd AS builder
ARG CARGO_LEPTOS_VERSION
ARG TAILWIND_VERSION
# amd64 | arm64 — provided by buildx; the from-source target builds natively.
ARG TARGETARCH
# curl + ca-certificates fetch the pinned tool binaries below. Unversioned on
# purpose (hadolint DL3008 is adjudicated in .hadolint.yaml): Debian rolls
# security point releases under one package name, and the artifact's real pins
# are the digest-pinned base, the tool versions above, and Cargo.lock.
RUN apt-get update && apt-get install -y --no-install-recommends \
      curl ca-certificates \
    && rm -rf /var/lib/apt/lists/*
RUN rustup target add wasm32-unknown-unknown
# `sh` on Debian is dash, which has no `pipefail`; the fetches below must fail
# the build on a failed download, never hand a truncated file to the next step.
SHELL ["/bin/bash", "-o", "pipefail", "-c"]
# cargo-leptos arrives as the upstream prebuilt tarball, checksum-verified
# against the .sha256 the same release publishes. Tailwind v4 is the
# STANDALONE binary (no Node anywhere in this build,
# https://tailwindcss.com/blog/standalone-cli), fetched by pinned version over
# TLS; upstream publishes no per-file checksum for it. Both land on PATH,
# where cargo-leptos resolves them before its own downloader; the matching
# wasm-bindgen CLI is resolved by cargo-leptos itself from Cargo.lock's
# wasm-bindgen version, so the two cannot drift.
WORKDIR /tmp
RUN set -eu; \
    case "${TARGETARCH}" in \
      amd64) triple=x86_64-unknown-linux-gnu; tw=linux-x64 ;; \
      arm64) triple=aarch64-unknown-linux-gnu; tw=linux-arm64 ;; \
      *) echo "unsupported TARGETARCH: ${TARGETARCH}" >&2; exit 1 ;; \
    esac; \
    curl --proto '=https' --tlsv1.2 -fsSLO "https://github.com/leptos-rs/cargo-leptos/releases/download/${CARGO_LEPTOS_VERSION}/cargo-leptos-${triple}.tar.gz"; \
    curl --proto '=https' --tlsv1.2 -fsSLO "https://github.com/leptos-rs/cargo-leptos/releases/download/${CARGO_LEPTOS_VERSION}/cargo-leptos-${triple}.tar.gz.sha256"; \
    sha256sum -c "cargo-leptos-${triple}.tar.gz.sha256"; \
    tar -xzf "cargo-leptos-${triple}.tar.gz"; \
    install -m 0755 "$(find /tmp -type f -name cargo-leptos | head -1)" /usr/local/bin/cargo-leptos; \
    curl --proto '=https' --tlsv1.2 -fsSLo /usr/local/bin/tailwindcss \
      "https://github.com/tailwindlabs/tailwindcss/releases/download/${TAILWIND_VERSION}/tailwindcss-${tw}"; \
    chmod 0755 /usr/local/bin/tailwindcss; \
    rm -rf /tmp/cargo-leptos*
WORKDIR /app
COPY . .
# Both caches are mounts, per the official Rust guide
# (https://docs.docker.com/guides/rust): /app/target holds the compile state
# including fingerprints, so cargo's own freshness logic survives across
# builds. A mount is not part of the image, so the binary and the site bundle
# are copied OUT in the same RUN and nothing under /app/target survives past
# this step. `sharing=locked` serializes concurrent builds on the one cache.
RUN --mount=type=cache,target=/app/target,sharing=locked \
    --mount=type=cache,target=/usr/local/cargo/registry,sharing=locked \
    cargo leptos build --release \
    && cargo build --release --locked -p veredictum --bin veredictum \
    && install -m 0755 target/release/veredictum-console /usr/local/bin/veredictum-console \
    && install -m 0755 target/release/veredictum /usr/local/bin/veredictum \
    && strip /usr/local/bin/veredictum-console /usr/local/bin/veredictum \
    && mkdir -p /srv \
    && cp -r target/site /srv/site

# ── runtime-base: the SHARED final base (digest-pinned distroless, non-root) ──
# gcr.io/distroless/cc-debian13:nonroot — glibc, libgcc_s and CA certificates,
# running as uid 65532. `cc` rather than the smaller base-nossl is REQUIRED: a
# Rust binary dynamically links libgcc_s.so.1 for stack unwinding, and
# base-nossl does not ship it, so the image would fail at load. Both final
# targets inherit this stage's user, environment, entrypoint and labels, so
# they differ by exactly three layers: the console, the engine, the site.
FROM gcr.io/distroless/cc-debian13:nonroot@sha256:a77defd6fedbb3392b175ba8ea3d1c22be963c1597c248c3ba987ddd80bfb512 AS runtime-base
# The `:nonroot` tag already runs as uid 65532, but a scanner reading this
# file cannot see a base image, and neither can a reader. NUMERIC, not
# `nonroot:nonroot`: an orchestrator cannot resolve a username against an
# image it does not read (hadolint DL3066; OWASP Docker Security Cheat Sheet
# rule 2).
USER 65532:65532
WORKDIR /app
# The standard cargo-leptos deployment environment (the Leptos book,
# deployment chapter): where the server finds the site bundle and what it
# binds. 0.0.0.0 because a container-loopback bind is reachable by nothing;
# the operator's publish flag decides actual exposure (header comment).
# VEREDICTUM_ENGINE names the in-image engine by absolute path rather than
# leaning on PATH resolution, so an overlay can repoint or disable it with one
# variable and the console never resolves a different binary by accident.
# VEREDICTUM_VERIFY_KEY names the verifying key COPYed below, so /verify
# checks a published record with no operator action at all.
ENV LEPTOS_OUTPUT_NAME=veredictum-console \
    LEPTOS_SITE_ROOT=/app/site \
    LEPTOS_SITE_PKG_DIR=pkg \
    LEPTOS_SITE_ADDR=0.0.0.0:3000 \
    VEREDICTUM_ENGINE=/usr/local/bin/veredictum \
    VEREDICTUM_ROOT=/work/artifacts \
    VEREDICTUM_SPECS=/work/specs/openehr \
    VEREDICTUM_PARTY=/work/party \
    VEREDICTUM_OUT=/work/out \
    VEREDICTUM_VERIFY_KEY=/app/keys/registry-signing.pub.asc

# ── The data the instrument judges against, in the image ──────────────────────
# The catalogue, the vendored specification oracle and the party declarations
# ship WITH the release rather than being mounted (#420). Two reasons, and the
# second is the one that matters: an instance with no volume cannot mount them
# at all, and a record has to name a catalogue revision that belongs to a
# published version. An image whose data came from whatever branch built it
# would pair a released engine with an unreleased catalogue, which is exactly
# the unpinned pairing this instrument refuses to accept from anyone else.
#
# An operator who wants their own trees bind-mounts over these paths, which
# takes precedence over image content
# (https://docs.docker.com/engine/storage/bind-mounts — a bind mount obscures
# what the image has at the target). `docker/docker-compose.yml` does exactly
# that, so the local development loop reads the working tree as before.
COPY artifacts/ /work/artifacts/
COPY specs/openehr/ /work/specs/openehr/
COPY party/ /work/party/
# The SECOND root a citation resolves against. `validate` derives it from the
# specs root's PARENT (`bundle_root`: `/work/specs/openehr` to `/work/specs`),
# so these three have to sit beside the docs tree rather than inside it. Without
# them every citation that can only resolve against a released machine-readable
# bundle becomes a finding, and the catalogue in the image fails its own gate.
# 0.8 MB compressed for all three, measured.
COPY specs/its-xml-schemas/ /work/specs/its-xml-schemas/
COPY specs/its-json-schemas/ /work/specs/its-json-schemas/
COPY specs/rest-oas/ /work/specs/rest-oas/
COPY --from=mkdirs --chown=65532:65532 /empty/ /work/

# ── The verifying key, in the image ───────────────────────────────────────────
# The PUBLIC half of the registry signing key: what a reader checks a published
# record against on /verify. It is release data exactly like the catalogue
# above, because the release that publishes a record is the release that has to
# be able to check one — so a fresh instance verifies with no clone, no key
# hunting and no operator step (#430). Shipping a public key in an image
# weakens nothing; the secret half lives in one GitHub environment and reaches
# no host.
#
# /app, not /work: an operator bind-mounts their own trees over the /work paths
# above, and a mount there would shadow the key. Nothing mounts over /app.
COPY registry/keys/registry-signing.pub.asc /app/keys/registry-signing.pub.asc

# ── The hosted posture, in the image ──────────────────────────────────────────
# What the hosted instance RUNS — its compose file and its Caddyfile — travels
# in the image too (#423). The deploy key on that host may run exactly one
# command, and that command extracts these two files out of the image it just
# pulled, which is the box's only way to receive them: it holds no checkout and
# fetches nothing from this repository. So a posture change arrives the way a
# catalogue change does, at a release, with the provenance of the artifact it
# travelled in.
#
# /app, not /work, for the reason the key above is there: an operator
# bind-mounts their own trees over the /work paths, and a mount there would
# shadow these. An operator image simply never reads them.
COPY deploy/hosted/docker-compose.yml /app/posture/docker-compose.yml
COPY deploy/hosted/Caddyfile /app/posture/Caddyfile

EXPOSE 3000
ENTRYPOINT ["/usr/local/bin/veredictum-console"]
# The binary is its own probe (`healthcheck` mode GETs /healthz and exits by
# the outcome), because a distroless image carries no shell and no curl —
# exec form, for the same reason. `start-period` gives the server its startup
# window before failures count; the interval/timeout/retries triple follows
# Docker's own reference defaults, stated explicitly so they are a decision
# (https://docs.docker.com/reference/dockerfile/#healthcheck). Orchestrators
# that probe HTTP themselves read /healthz directly and ignore this.
HEALTHCHECK --interval=30s --timeout=5s --start-period=10s --retries=3 \
  CMD ["/usr/local/bin/veredictum-console", "healthcheck"]
# Graceful shutdown is the binary's duty, not this file's: as PID 1 of an
# exec-form entrypoint it receives `docker stop`'s SIGTERM directly (no
# STOPSIGNAL needed — SIGTERM is the default) and drains in-flight requests
# before exiting; the SIGKILL at the grace deadline is the backstop.
#
# The OCI predefined annotation keys (image-spec annotations.md), split by who
# can know the value: everything build-INDEPENDENT is declared here so a
# direct `docker build` produces a correctly labelled image, and the
# build-DEPENDENT ones (created, version, revision) arrive from the publishing
# workflow, which is the only place they exist. base.name and base.digest name
# the IMMEDIATE parent — the `FROM` of this runtime stage.
ARG VERSION="0.0.0"
ARG REVISION=""
LABEL org.opencontainers.image.title="Veredictum console" \
      org.opencontainers.image.description="The web console of the independent conformance instrument for openEHR clinical data repositories" \
      org.opencontainers.image.url="https://github.com/rubentalstra/Veredictum" \
      org.opencontainers.image.documentation="https://github.com/rubentalstra/Veredictum#readme" \
      org.opencontainers.image.source="https://github.com/rubentalstra/Veredictum" \
      org.opencontainers.image.vendor="Veredictum" \
      org.opencontainers.image.authors="Ruben Talstra <https://github.com/rubentalstra>" \
      org.opencontainers.image.licenses="Apache-2.0" \
      org.opencontainers.image.base.name="gcr.io/distroless/cc-debian13:nonroot" \
      org.opencontainers.image.base.digest="sha256:a77defd6fedbb3392b175ba8ea3d1c22be963c1597c248c3ba987ddd80bfb512" \
      org.opencontainers.image.version="${VERSION}" \
      org.opencontainers.image.revision="${REVISION}"
# Not an OCI predefined key, because none of them means this: the image carries
# the catalogue, the oracle and the party declarations at /work. The hosted
# deploy REQUIRES it, so an image built before #420 cannot be deployed to an
# instance that mounts nothing and would find /work empty.
LABEL eu.veredictum.image.carries-catalogue="true"

# ── runtime-from-source: the local path (COPY the in-image build) ─────────────
FROM runtime-base AS runtime-from-source
COPY --from=builder /usr/local/bin/veredictum-console /usr/local/bin/veredictum-console
COPY --from=builder /usr/local/bin/veredictum /usr/local/bin/veredictum
COPY --from=builder /srv/site /app/site

# ── runtime-prebuilt: the CI path (COPY the context-provided artifacts) ───────
# The server and engine binaries are compiled natively per architecture on the
# CI runners, inside rust:*-slim-trixie so they target the same Debian 13 glibc
# this distroless base ships, and land in the build context as
# bin/<arch>/veredictum-console and bin/<arch>/veredictum. The site
# bundle (WASM + CSS + the generated bootstrap) is architecture-independent
# and lands once as site/. Because no RUN executes, a multi-architecture
# buildx of this target needs no QEMU.
FROM runtime-base AS runtime-prebuilt
# amd64 | arm64 — supplied by buildx per platform.
ARG TARGETARCH
COPY --chmod=0755 bin/${TARGETARCH}/veredictum-console /usr/local/bin/veredictum-console
COPY --chmod=0755 bin/${TARGETARCH}/veredictum /usr/local/bin/veredictum
COPY site/ /app/site/
