#!/usr/bin/env bash
# SPDX-FileCopyrightText: Veredictum contributors
# SPDX-License-Identifier: Apache-2.0
# The crates.io upload and its read-back, as ONE implementation.
#
# Two workflows publish this crate — `publish-crates.yml` on a manual dispatch
# and the `publish-crate` leg of `release.yml` on a tag — and they cannot share
# a reusable WORKFLOW, because crates.io Trusted Publishing matches the OIDC
# `workflow_ref` claim, which names the CALLING workflow rather than the called
# one for a job inside a reusable workflow
# (https://docs.github.com/en/actions/concepts/security/openid-connect and
# https://crates.io/docs/trusted-publishing). Moving the publish job into a
# reusable workflow would therefore present a different identity than the one
# either publisher configuration names. So the shared thing is this script, and
# each workflow keeps its own job — and its own identity.
#
# Usage:
#   publish-crate.sh publish          # cargo publish, treating "exists" as done
#   publish-crate.sh verify           # read the registry back, with retries
#   publish-crate.sh version          # print the manifest version
#
# Requires: cargo, curl, jq. `publish` additionally requires
# CARGO_REGISTRY_TOKEN in the environment.
set -euo pipefail

readonly CRATE=veredictum

manifest_version() {
  # The `[package]` table's own `version`, never the first `version = ` line in
  # the file: this manifest carries dependency versions too, and a `[workspace]`
  # table above `[package]`.
  awk -F'"' '/^\[package\]/{p=1} p && /^version = /{print $2; exit}' Cargo.toml
}

do_publish() {
  # "already exists" is treated as DONE rather than as failure, which is what
  # makes the lane safe to re-run after a transient network failure late in an
  # upload. Matching happens against ANSI-STRIPPED text: cargo colours the
  # status word alone, so the bytes are `Uploaded<RESET> veredictum` and a
  # literal "Uploaded veredictum" would never match a perfectly successful
  # publish.
  local out plain status=0
  out="$(cargo publish --locked -p "${CRATE}" 2>&1)" || status=$?
  printf '%s\n' "$out"
  plain="$(printf '%s' "$out" | sed -E 's/\x1b\[[0-9;]*m//g')"
  case "$plain" in
    *"already exists on crates.io index"* | *"already uploaded"*)
      echo "publish-crate: already published at this version — nothing to do"
      return 0
      ;;
  esac
  if printf '%s' "$plain" | grep -q "Uploaded ${CRATE}"; then
    echo "publish-crate: uploaded"
    return 0
  fi
  echo "::error::the publish did not report an upload (cargo exit ${status})"
  return 1
}

do_verify() {
  # The registry is the ground truth, not the exit code of the step above. The
  # index is eventually consistent right after an upload, so a miss is retried
  # rather than believed — and a FAILED REQUEST is not a miss either: without
  # --fail, curl hands an error body to jq, jq dies on the absent `.versions`,
  # and `pipefail` ends the step, so one transient 429 would report a missing
  # version that is actually there.
  local want got="" body
  want="$(manifest_version)"
  echo "publish-crate: expecting ${CRATE} ${want} on the registry"
  for _ in 1 2 3 4 5 6; do
    if body="$(curl -sSL --fail -H "User-Agent: ${CRATE}-publish-verify" \
                 "https://crates.io/api/v1/crates/${CRATE}/versions" 2>/dev/null)"; then
      got="$(printf '%s' "$body" \
             | jq -r --arg v "$want" '.versions[]? | select(.num == $v) | .num' \
             | head -1)" || got=""
    fi
    [ -n "$got" ] && break
    sleep 10
  done
  if [ -z "$got" ]; then
    echo "::error::version ${want} is not on the registry"
    return 1
  fi
  echo "publish-crate: confirmed on crates.io — ${CRATE} ${got}"
}

case "${1:-}" in
  publish) do_publish ;;
  verify) do_verify ;;
  version) manifest_version ;;
  *)
    echo "publish-crate: expected 'publish', 'verify' or 'version', got '${1:-<none>}'" >&2
    exit 2
    ;;
esac
