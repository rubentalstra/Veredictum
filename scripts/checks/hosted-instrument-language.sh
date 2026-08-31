#!/usr/bin/env bash
# SPDX-FileCopyrightText: Veredictum contributors
# SPDX-License-Identifier: Apache-2.0
# The hosted instrument is not a demo, and no surface may call it one (#395).
#
# Owner ruling (#388): a run performed at console.veredictum.eu is an official
# conformance run and produces an official record. "Demo" and "sandbox" describe
# something a reader may disregard, and a reader who disregards the instrument
# disregards the record it produced — so the words are refused on every surface
# a reader meets, and this guard is what keeps them out after the prose is
# written.
#
# The refusal is narrow on purpose. `demonstration` is a real word this project
# uses correctly (the POC performance class is a demonstration floor), and
# `sandbox` is a Content-Security-Policy directive name. So the guard matches
# the WHOLE words `demo`, `demos`, `sandbox` and `sandboxes`, and carries an
# allowlist of the adjudicated legitimate lines.
#
# Usage:
#   scripts/checks/hosted-instrument-language.sh
#   scripts/checks/hosted-instrument-language.sh --self-test
set -euo pipefail

cd "$(dirname "$0")/../.."

# The surfaces a reader meets. The vendored specification trees are never
# edited and never scanned; neither is the changelog, which is a record of what
# the words used to be.
readonly SURFACES=(
  README.md
  registry/RULES.md
  deploy/hosted
  website/landing
  website/book/src
  app/veredictum-console/src
)

readonly PATTERN='\b([Dd]emos?|[Ss]andboxe?s?)\b'

# The adjudicated legitimate uses, matched against the whole `path:line` text.
allowed() {
  local hit="$1"
  # A CSP directive name, in the policy comment every landing page carries.
  case "$hit" in
    *frame-ancestors*) return 0 ;;
  esac
  return 1
}

scan() {
  local root="$1"
  local -a hits=()
  local line
  while IFS= read -r line; do
    [[ -n "$line" ]] || continue
    allowed "$line" || hits+=("$line")
  done < <(grep -rInE "$PATTERN" "${SURFACES[@]/#/$root/}" 2>/dev/null || true)
  # `set -u` and an empty array do not get along; nothing found prints nothing.
  if [[ ${#hits[@]} -gt 0 ]]; then
    printf '%s\n' "${hits[@]}"
  fi
}

if [[ "${1:-}" == "--self-test" ]]; then
  # A seeded violation must be caught, and a legitimate word must not be.
  scratch="$(mktemp -d)"
  # shellcheck disable=SC2064 # the path is expanded now, on purpose
  trap "rm -rf '$scratch'" EXIT
  mkdir -p "$scratch/website/book/src"
  printf 'The hosted demo drives a run.\n' > "$scratch/website/book/src/seeded.md"
  printf 'A demonstration floor, not population-derived.\n' >> "$scratch/website/book/src/seeded.md"
  found="$(scan "$scratch" | grep -c . || true)"
  if [[ "$found" != "1" ]]; then
    echo "::error::the self-test expected exactly one seeded hit and counted ${found}" >&2
    scan "$scratch" >&2
    exit 1
  fi
  echo "hosted-instrument-language: self-test OK (the seeded 'demo' is caught, 'demonstration' is not)."
  exit 0
fi

found="$(scan . || true)"
if [[ -n "$found" ]]; then
  echo "::error::a reader-facing surface calls the instrument a demo or a sandbox" >&2
  printf '%s\n' "$found" | sed 's/^/  /' >&2
  echo >&2
  echo "console.veredictum.eu is the official conformance instrument (#388): a run" >&2
  echo "performed there is an official run and produces an official record. Say what" >&2
  echo "it is." >&2
  exit 1
fi
echo "hosted-instrument-language: no surface calls the instrument a demo — OK."
