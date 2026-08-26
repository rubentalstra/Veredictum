# syntax=docker/dockerfile:1
#
# The Veredictum container image.
#
# WHAT THIS IMAGE IS: the runner, and nothing else. The catalogue, the corpora
# and the vendored specification oracle are 347 MB, and the runner reads every
# one of them as a path the caller passes at run time — no source file embeds an
# artifact. Baking them in would multiply the image size to ship data that is
# already in the repository and that a party may legitimately want to override.
# So the image carries the binary and the catalogue is MOUNTED.
#
# MOUNT THE REPOSITORY AT /work, and the documented invocation works:
#
#   docker run --rm -v "$PWD:/work" ghcr.io/rubentalstra/veredictum:<tag> \
#     validate --root /work/artifacts --specs /work/specs/openehr
#
# The entrypoint IS the instrument, so the arguments are its subcommands
# (`validate`, `run`, `verdicts`, `perf`, `stress`, `aql-probe`, `emit-schemas`).
# A `run` against a CDR needs the party declaration and an output directory on
# the same mount, and `--network host` if the server under test is on the host.
#
# ONE runtime stage feeds TWO final targets, so a locally built image and a
# CI-published one can never drift
# (https://docs.docker.com/build/building/multi-stage):
#
#   runtime-from-source  the LOCAL path — compiles the binary in-image against a
#                        persistent BuildKit cache mount.
#   runtime-prebuilt     the CI path — COPY-only packaging of a context-provided
#                        native binary (bin/${TARGETARCH}/veredictum). No RUN
#                        executes, so a multi-architecture buildx needs no QEMU
#                        and finishes in seconds.
#
# The RUST_VERSION toolchain is single-sourced from rust-toolchain.toml, and the
# release pipeline cross-checks this ARG against its `channel` so the two cannot
# drift.

ARG RUST_VERSION=1.97.1

# ── builder: compile in-image against the persistent cache mounts ──────────────
# Digest-pinned rust:1.97.1-slim-trixie (Debian 13, glibc 2.41). The builder and
# the cc-debian13 runtime below share the Debian 13 generation, so the binary
# links exactly the glibc it runs against. Pinning by digest rather than by the
# mutable tag is what makes the base reproducible
# (https://docs.docker.com/build/building/best-practices — "pinning to a digest
# guarantees the same image version").
FROM rust:${RUST_VERSION}-slim-trixie@sha256:3b2879047d42784ca9403ad20c51ed3df361a50f1df96f5777d39b4e33aa65cd AS builder
WORKDIR /app
COPY . .
# cargo-auditable embeds the compressed dependency list in the binary's own
# `.dep-v0` linker section, which is allocated and therefore survives the strip
# below, letting syft/trivy/grype/osv-scanner read the crate graph from the
# shipped artifact itself (https://github.com/rust-secure-code/cargo-auditable).
RUN --mount=type=cache,target=/usr/local/cargo/registry,sharing=locked \
    cargo install cargo-auditable --version 0.7.5 --locked
# Both caches are mounts, per the official Rust guide
# (https://docs.docker.com/guides/rust): /app/target holds the compile state
# including fingerprints, so cargo's own freshness logic survives across builds —
# which is the thing an image layer can never give a workspace crate. A mount is
# not part of the image, so the binary is copied OUT in the same RUN and nothing
# under /app/target survives past this step. `sharing=locked` serializes
# concurrent builds on the one cache.
RUN --mount=type=cache,target=/app/target,sharing=locked \
    --mount=type=cache,target=/usr/local/cargo/registry,sharing=locked \
    cargo auditable build --release --locked --bin veredictum \
    && cp target/release/veredictum /usr/local/bin/veredictum \
    && strip /usr/local/bin/veredictum

# ── runtime-base: the SHARED final base (digest-pinned distroless, non-root) ───
# gcr.io/distroless/cc-debian13:nonroot — glibc, libgcc_s and CA certificates,
# running as uid 65532. `cc` rather than the smaller base-nossl is REQUIRED: a
# Rust binary dynamically links libgcc_s.so.1 for stack unwinding, and
# base-nossl does not ship it, so the image would fail at load with
# "libgcc_s.so.1: cannot open shared object file". Both final targets inherit
# this stage's user, entrypoint and labels, so they differ by exactly one layer.
FROM gcr.io/distroless/cc-debian13:nonroot@sha256:d97bc0a941b8d4be647dc0ee75b264ddbb772f1ac5ba690a4309c00723b23775 AS runtime-base
# The `:nonroot` tag already runs as uid 65532, but a scanner reading this file
# cannot see a base image, and neither can a reader. Stating it makes the posture
# a property of this file rather than of a tag someone could change (OWASP Docker
# Security Cheat Sheet rule 2).
#
# NUMERIC, not `nonroot:nonroot`: an orchestrator cannot resolve a username
# against an image it does not read, so `runAsNonRoot: true` with a named USER is
# refused with "cannot verify user is non-root" (hadolint DL3066). 65532 is what
# the distroless `:nonroot` tag sets, so this states the base's uid rather than
# choosing one.
USER 65532:65532
# The documented mount point. Created at build time, because a distroless image
# has no shell to create it at run time, and it is what makes a relative
# `--root artifacts` work as well as the absolute form.
WORKDIR /work
ENTRYPOINT ["/usr/local/bin/veredictum"]
# No EXPOSE and no HEALTHCHECK: this image runs a catalogue and exits. It serves
# nothing, so a port would be fiction and a healthcheck would have nothing to
# probe.
#
# The OCI predefined annotation keys (image-spec annotations.md), split by who
# can know the value: everything build-INDEPENDENT is declared here so a direct
# `docker build` produces a correctly labelled image, and the build-DEPENDENT
# ones arrive from the publishing workflow, which is the only place they exist.
# base.name and base.digest name the IMMEDIATE parent — the `FROM` of this
# runtime stage, not an intermediate build stage.
ARG VERSION="0.0.0"
ARG REVISION=""
LABEL org.opencontainers.image.title="Veredictum" \
      org.opencontainers.image.description="The independent conformance instrument for openEHR clinical data repositories" \
      org.opencontainers.image.url="https://github.com/rubentalstra/Veredictum" \
      org.opencontainers.image.documentation="https://github.com/rubentalstra/Veredictum#readme" \
      org.opencontainers.image.source="https://github.com/rubentalstra/Veredictum" \
      org.opencontainers.image.vendor="Veredictum" \
      org.opencontainers.image.authors="Ruben Talstra <https://github.com/rubentalstra>" \
      org.opencontainers.image.licenses="Apache-2.0" \
      org.opencontainers.image.base.name="gcr.io/distroless/cc-debian13:nonroot" \
      org.opencontainers.image.base.digest="sha256:d97bc0a941b8d4be647dc0ee75b264ddbb772f1ac5ba690a4309c00723b23775" \
      org.opencontainers.image.version="${VERSION}" \
      org.opencontainers.image.revision="${REVISION}"

# ── runtime-from-source: the local path (COPY the in-image build) ──────────────
FROM runtime-base AS runtime-from-source
COPY --from=builder /usr/local/bin/veredictum /usr/local/bin/veredictum

# ── runtime-prebuilt: the CI path (COPY the context-provided native binary) ────
# The binaries are compiled natively per architecture on the CI runners, inside
# rust:*-slim-trixie so they target the same Debian 13 glibc this distroless base
# ships, and land in the build context as bin/amd64/veredictum and
# bin/arm64/veredictum. Because no RUN executes, a multi-architecture buildx of
# this target needs no QEMU.
FROM runtime-base AS runtime-prebuilt
# amd64 | arm64 — supplied by buildx per platform.
ARG TARGETARCH
COPY --chmod=0755 bin/${TARGETARCH}/veredictum /usr/local/bin/veredictum
