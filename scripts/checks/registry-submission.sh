#!/usr/bin/env bash
# SPDX-FileCopyrightText: Veredictum contributors
# SPDX-License-Identifier: Apache-2.0
# A registry submission is validated by machine before a human reads it.
#
# The submission channel is a pull request that ADDS one entry under
# `registry/entries/`, with the evidence it stands on, and the merge is the
# publication. That only works if the merge is preceded by a check nobody has
# to remember to run, so this script is the whole gate, in four parts:
#
#   1. Append-only. A merged entry is a published claim about somebody's
#      product; editing or deleting one rewrites it. Any Modified, Deleted or
#      Renamed path under the entries and records trees fails, and only
#      additions pass. The rules document and the reproducible topologies are
#      deliberately outside that rule: rules change prospectively, and a
#      topology is a recipe this repository maintains.
#   2. Entry content. The published registry-entry schema, the submission
#      rules the entry itself declares, the path its own fields name, id
#      uniqueness across the tree, the digest of every artifact it pins, the
#      supersede edges, the tier against the evidence its variant requires,
#      and the pairing with the benchmark board's records. That half is a Rust
#      integration test, because it reads the same model the engine writes
#      entries with; a second reimplementation in shell would be a second thing
#      to keep true.
#   3. No signing secret in the tree. The reproduced tier is an attestation
#      from the workflow identity, and the one key this project holds — the
#      registry key that signs a console record — lives in a protected CI
#      environment and nowhere else. A committed key would be the single point
#      of forgery the whole design exists to remove, so any armored private key
#      under the registry fails the gate on sight.
#   4. The boards are not stale. Both pages are generated from what is
#      committed, so a merged submission that leaves one unchanged would
#      publish a board that omits the thing just accepted.
#
# Usage:
#   scripts/checks/registry-submission.sh                 # against origin/main
#   scripts/checks/registry-submission.sh <base> <head>   # an explicit range (CI)
#
# `VEREDICTUM_REGISTRY_SKIP_CARGO=1` runs parts 1, 3 and 4 alone, for a caller
# that has already run the integration suite.
set -euo pipefail

cd "$(dirname "$0")/../.."

readonly ENTRIES='registry/entries'
readonly RECORDS='registry/records'

base="${1:-origin/main}"
head="${2:-HEAD}"

# ── 1. Append-only ───────────────────────────────────────────────────────────
# The three-dot form asks what the head added since the merge base, which is
# the question a pull request is actually about; a two-dot range would report
# every change made on the base branch since the fork point as a deletion.
if git rev-parse --verify --quiet "$base" >/dev/null; then
  touched="$(git diff --name-status --diff-filter=MDR "$base...$head" -- "$ENTRIES" "$RECORDS" || true)"
  if [[ -n "$touched" ]]; then
    echo "::error::the registry is append-only, and this change does not only add" >&2
    echo "$touched" | sed 's/^/  /' >&2
    echo >&2
    echo "A merged entry is a published claim. Correct one by adding a new entry that" >&2
    echo "names the old one in \`supersedes\` and says why in \`supersede_reason\`." >&2
    exit 1
  fi
  echo "registry-submission: nothing under $ENTRIES or $RECORDS was modified, deleted or renamed — OK."
else
  echo "registry-submission: $base does not resolve here, so the append-only half is skipped." >&2
fi

# ── 2. Entry content ─────────────────────────────────────────────────────────
if [[ "${VEREDICTUM_REGISTRY_SKIP_CARGO:-0}" == "1" ]]; then
  echo "registry-submission: the entry-content gate is skipped by request."
else
  command -v cargo >/dev/null || { echo "cargo is required" >&2; exit 1; }
  cargo nextest run --locked -E 'test(registry_submissions)'
fi

# ── 3. No signing secret anywhere in the registry ────────────────────────────
# The reproduced tier is an attestation issued to the workflow identity, and the
# console tier is signed from a protected environment no submitted branch can
# reach. A committed private key would quietly turn either into something a
# compromised workflow could forge, so it is refused here as well as by review.
registry_files=()
while IFS= read -r -d '' file; do
  registry_files+=("$file")
done < <(git ls-files -z registry)

if [[ ${#registry_files[@]} -gt 0 ]]; then
  # grep answers 0 for a match, 1 for none, and anything else for a failure to
  # look. The three are separated here rather than folded into an `if`, because
  # a scan that could not run must fail the gate instead of reading as clean.
  set +e
  carriers="$(grep -lI -- '-----BEGIN .*PRIVATE KEY' "${registry_files[@]}")"
  scan=$?
  set -e
  case "$scan" in
    0)
      echo "::error::a private key is committed under registry/, and no signing secret exists in this repository" >&2
      echo "$carriers" | sed 's/^/  /' >&2
      exit 1
      ;;
    1) ;;
    *)
      echo "::error::the private-key scan over registry/ could not run (grep exit $scan)" >&2
      exit 1
      ;;
  esac
fi
echo "registry-submission: no private key material under registry/ — OK."

# ── 4. The boards reflect what is committed ──────────────────────────────────
bash scripts/render/conformance-board.sh --check
bash scripts/render/bench-board.sh --check
