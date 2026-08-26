#!/usr/bin/env bash
# SPDX-FileCopyrightText: Veredictum contributors
# SPDX-License-Identifier: Apache-2.0
# Image-metadata guard: the same OCI facts are declared in two places, and this
# makes them fail rather than drift.
#
# WHY TWO PLACES AT ALL. The Dockerfile declares the build-INDEPENDENT keys so a
# plain `docker build` produces a correctly labelled image, and the publishing
# workflow declares them too because `build-push-action`'s `labels:` input
# OVERRIDES a Dockerfile LABEL — so the workflow's copy is what the published
# image actually carries. Two declarations of one fact drift silently, and the
# drift is invisible in exactly the direction that matters: CI publishes the
# workflow's value while anyone building the Dockerfile directly gets the other
# one.
#
# THE DEFECT THAT MOTIVATED IT here, concretely: the base image digest appears in
# THREE places — the runtime stage's `FROM`, the Dockerfile's
# `base.digest` LABEL, and `release.yml`'s `labels:` input — and Dependabot's
# base-image bump edits ONLY the `FROM`. Merging such a bump unmodified publishes
# an image whose `base.digest` names a parent it was not built on, which is worse
# than declaring no parent at all.
#
# Ownership, which is what the checks below encode:
#   build-INDEPENDENT (title, description, url, documentation, source, vendor,
#     authors, licenses, base.name, base.digest) — both places, identical values
#   build-DEPENDENT (created, version, revision) — the workflow only, since a
#     Dockerfile cannot know them
#
# Usage: scripts/checks/image-labels.sh
set -euo pipefail
cd "$(dirname "$0")/../.."

readonly IMAGE=veredictum
readonly DOCKERFILE=docker/Dockerfile
readonly WORKFLOW=.github/workflows/release.yml
readonly BUILD_WORKFLOW=.github/workflows/build-image.yml

# The keys both places must declare, identically.
readonly SHARED="title description url documentation source vendor authors licenses base.name base.digest"

failures=0
report() { printf '%s\n' "$*" >&2; failures=$((failures + 1)); }

# The value of `org.opencontainers.image.<key>` in a Dockerfile LABEL block.
dockerfile_label() {
  awk -v key="org.opencontainers.image.$1=" '
    index($0, key) {
      i = index($0, key) + length(key)
      v = substr($0, i)
      sub(/^"/, "", v); sub(/"[[:space:]]*\\?[[:space:]]*$/, "", v)
      print v; exit
    }' "$DOCKERFILE"
}

# The value declared in the workflow's `labels:` block. Matched from the lane's
# `image: ghcr.io/...` opener so a second image lane added later cannot have its
# labels read as this one's.
workflow_label() {
  awk -v lane="/$IMAGE" -v key="org.opencontainers.image.$1=" '
    /image: ghcr\.io\// {
      line = $0
      sub(/^[[:space:]]*/, "", line); sub(/[[:space:]]*$/, "", line)
      inlane = (substr(line, length(line) - length(lane) + 1) == lane)
    }
    inlane && index($0, key) {
      i = index($0, key) + length(key)
      v = substr($0, i)
      sub(/^[[:space:]]*/, "", v); sub(/[[:space:]]*$/, "", v)
      print v; exit
    }' "$WORKFLOW"
}

[[ -f "$DOCKERFILE" ]] || { report "image-labels: missing $DOCKERFILE"; }
[[ -f "$WORKFLOW" ]] || { report "image-labels: missing $WORKFLOW"; }

for key in $SHARED; do
  d=$(dockerfile_label "$key" || true)
  w=$(workflow_label "$key" || true)
  if [[ -z "$d" ]]; then
    report "image-labels: $DOCKERFILE declares no org.opencontainers.image.$key"
    continue
  fi
  if [[ -z "$w" ]]; then
    report "image-labels: $WORKFLOW declares no org.opencontainers.image.$key for $IMAGE"
    continue
  fi
  # Every shared key is a literal — the Dockerfile substitutes build args only
  # into version/revision — so a plain comparison is right.
  if [[ "$d" != "$w" ]]; then
    report "image-labels: .$key disagrees —
    $DOCKERFILE: $d
    $WORKFLOW: $w"
  fi
done

# base.name/base.digest must match the runtime stage's ACTUAL FROM pin. This is
# the check that catches an unmodified Dependabot base bump.
from=$(grep -E '^FROM [^ ]+@sha256:' "$DOCKERFILE" | grep -v ' AS builder' | tail -1 || true)
if [[ -z "$from" ]]; then
  report "image-labels: $DOCKERFILE has no digest-pinned runtime FROM to check base.* against"
else
  ref=$(printf '%s' "$from" | awk '{print $2}')
  want_name=${ref%@*}
  want_digest=${ref#*@}
  got_name=$(dockerfile_label base.name || true)
  got_digest=$(dockerfile_label base.digest || true)
  [[ "$got_name" = "$want_name" ]] \
    || report "image-labels: $DOCKERFILE base.name is '$got_name', but its runtime FROM is '$want_name'"
  [[ "$got_digest" = "$want_digest" ]] \
    || report "image-labels: $DOCKERFILE base.digest is '$got_digest', but its runtime FROM pins '$want_digest'"
  # And the workflow's copy of the digest, which is the one the PUBLISHED image
  # carries, must name the same base.
  wf_digest=$(workflow_label base.digest || true)
  [[ "$wf_digest" = "$want_digest" ]] \
    || report "image-labels: $WORKFLOW base.digest is '$wf_digest', but $DOCKERFILE pins '$want_digest'"
fi

# Annotations must reach the INDEX: it is the only place GHCR reads a package
# description from, and metadata-action annotates manifests only by default. The
# mechanics live ONCE, in the reusable lane.
levels=$(grep -c 'DOCKER_METADATA_ANNOTATIONS_LEVELS: index,manifest' "$BUILD_WORKFLOW" || true)
[[ "$levels" -eq 1 ]] \
  || report "image-labels: expected one metadata step with DOCKER_METADATA_ANNOTATIONS_LEVELS: index,manifest in $BUILD_WORKFLOW, found $levels"
# shellcheck disable=SC2016 # the single quotes are correct: `${{ }}` is a
# literal to grep for, and must NOT be expanded by the shell.
annots=$(grep -c 'annotations: ${{ steps.meta.outputs.annotations }}' "$BUILD_WORKFLOW" || true)
[[ "$annots" -eq 1 ]] \
  || report "image-labels: expected one build step passing the annotations output in $BUILD_WORKFLOW, found $annots"

# A guard that finds nothing to check is not passing, it is vacuous.
lanes=$(grep -c 'uses: ./.github/workflows/build-image.yml' "$WORKFLOW" || true)
[[ "$lanes" -eq 1 ]] \
  || report "image-labels: expected 1 publishing lane calling build-image.yml in $WORKFLOW, found $lanes"

# `ref.name` is deliberately unset, recorded rather than silently skipped: the
# spec leaves it to the consumer, an image carries several tags, and a single
# ref.name would have to pick one arbitrarily. `created` comes from the action.

if [[ "$failures" -gt 0 ]]; then
  echo "image-labels: $failures problem(s) — see above." >&2
  exit 1
fi
echo "image-labels: OK."
