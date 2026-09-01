#!/usr/bin/env bash
# SPDX-FileCopyrightText: Veredictum contributors
# SPDX-License-Identifier: Apache-2.0
# The re-derivation gate (#392): a console entry's judgement is recomputed
# here, from the evidence the submission carries, before anything is signed.
#
# A `console` entry says that the official hosted instrument drove a run and
# reached these outcomes. Nobody has to believe it. The entry carries the
# recorded exchanges, the topology they were driven under and the claim they
# were judged against, so the whole judgement is recomputed twice over:
#
#   1. The outcomes, from the transcript. The catalogue is driven again with
#      the recording standing in for the server, through the same request
#      composition, the same response classification and the same assertion
#      evaluators the live run used. Any row whose status or row counts differ
#      from the submitted `results.json` fails the gate.
#   2. The verdicts, from those outcomes. `verdicts` is a pure function of the
#      results and the catalogue, so its output is compared byte for byte with
#      the submitted `verdicts.json`.
#
# What this establishes is that the judgement follows from the evidence. It
# does not establish the evidence: a transcript is what the instrument says it
# sent and received, which is why `registry/RULES.md` states plainly that the
# console tier cannot attest the environment.
#
# The record's own provenance decides how far step 1 can go, and the two ways
# it falls short are treated differently. A member the record does not carry
# (`selection_basis`, `statement_digest`, `tech_profile.source`) means the
# document was written before that member existed: `replay --against` reports
# the absence, says which member is missing, and recomputes anyway, because
# reading absence as a value would refuse every published record a member
# predates. Members that CONTRADICT each other are refused, because no
# re-judgement reconstructs a campaign the document describes two ways at once
# — a profile read from a declaration on a campaign no declaration selected is
# the case that exists today.
#
# NOTHING a submitter wrote is executed here. The engine is the one this
# repository builds from its own checked-out source; the submission is read as
# data, and no path from it reaches a shell.
#
# Usage:
#   scripts/checks/registry-rederive.sh <entry.json> [<entry.json> …]
#   scripts/checks/registry-rederive.sh            # every console entry added
#                                                  # against origin/main
set -euo pipefail

# The tree the entry paths resolve against. The repository itself, unless a
# caller points this at a prepared tree — which is how the gate is tested
# without writing a fixture into the published registry.
cd "${REGISTRY_TREE:-$(dirname "$0")/../..}"

# How many entries this run actually recomputed, for the summary the caller
# reads.
rederived=0

readonly ENTRIES='registry/entries'

# The engine under test is built from THIS tree, never from a submission.
engine() {
  if [[ -n "${VEREDICTUM_BIN:-}" ]]; then
    printf '%s' "$VEREDICTUM_BIN"
    return 0
  fi
  cargo build --locked --release -p veredictum --bin veredictum >&2
  printf '%s' "target/release/veredictum"
}

# One artifact path by role, or the empty string when the entry carries none.
role_path() {
  jq -r --arg role "$2" \
    '[.artifacts[] | select(.role == $role) | .path] | first // ""' "$1"
}

# The gate fires on a console SUBMISSION and on a completed console entry, and
# those two look different: a submission carries no provenance block at all,
# because the instrument is not allowed to state its own provenance and the lane
# writes the block afterwards. Reading the tier alone therefore skipped every
# real submission and signed it unrecomputed (#408). An entry naming any other
# tier is skipped, with the tier said out loud.
rederive_one() {
  local entry="$1" bin="$2"
  local tier
  tier="$(jq -r '.provenance.tier // "none"' "$entry")"
  case "$tier" in
    console|none) ;;
    *)
      echo "registry-rederive: $entry declares tier '$tier' — this gate re-derives console entries."
      return 0
      ;;
  esac

  local results verdicts transcript ixit statement
  results="$(role_path "$entry" results)"
  verdicts="$(role_path "$entry" verdicts)"
  transcript="$(role_path "$entry" transcript)"
  ixit="$(role_path "$entry" ixit)"
  statement="$(role_path "$entry" statement)"
  local role
  for role in results verdicts transcript ixit statement; do
    if [[ -z "${!role}" || ! -f "${!role}" ]]; then
      echo "::error::$entry declares no readable '$role' artifact, and a console entry is re-derived from all five" >&2
      return 1
    fi
  done

  local scratch
  scratch="$(mktemp -d)"
  # shellcheck disable=SC2064 # the path is expanded now, on purpose
  trap "rm -rf '$scratch'" RETURN

  echo "registry-rederive: re-judging $entry from its recorded exchanges"
  "$bin" replay \
    --root artifacts \
    --ixit "$ixit" \
    --transcript "$transcript" \
    --statement "$statement" \
    --out "$scratch/results.json" \
    --against "$results"

  echo "registry-rederive: recomputing the verdicts from the submitted outcomes"
  # `verdicts` exits 1 when the judgement is not clean — a failed capability or
  # a static-review finding — and that is a RESULT rather than a failure to
  # compute: an entry about a server that fails cases is exactly what this
  # registry publishes. Only a refusal to run at all stops the gate, and the
  # missing-document check below is what catches a judgement that never ran.
  local judged=0
  "$bin" verdicts \
    --statement "$statement" \
    --results "$results" \
    --root artifacts \
    --out "$scratch/judgement" >/dev/null || judged=$?
  if [[ "$judged" -gt 1 ]]; then
    echo "::error::the verdicts could not be computed from $results at all (exit $judged)" >&2
    return 1
  fi
  if [[ ! -f "$scratch/judgement/verdicts.json" ]]; then
    echo "::error::recomputing the verdicts from $results produced no verdicts.json" >&2
    return 1
  fi

  if ! diff -u "$verdicts" "$scratch/judgement/verdicts.json"; then
    echo "::error::$verdicts is not what the catalogue computes from $results" >&2
    return 1
  fi
  echo "registry-rederive: $entry re-derives to what it submitted — OK."
  rederived=$((rederived + 1))
}

main() {
  command -v jq >/dev/null || { echo "jq is required" >&2; exit 1; }
  local -a entries=()
  if [[ $# -gt 0 ]]; then
    entries=("$@")
  else
    local base="${VEREDICTUM_REGISTRY_BASE:-origin/main}"
    if ! git rev-parse --verify --quiet "$base" >/dev/null; then
      echo "registry-rederive: $base does not resolve here, so there is nothing to select." >&2
      exit 0
    fi
    while IFS= read -r file; do
      [[ -n "$file" ]] && entries+=("$file")
    done < <(git diff --name-only --diff-filter=A "$base...HEAD" -- "$ENTRIES" || true)
  fi

  if [[ ${#entries[@]} -eq 0 ]]; then
    echo "registry-rederive: no entry was added — nothing to re-derive."
    exit 0
  fi

  local bin
  bin="$(engine)"
  local entry
  for entry in "${entries[@]}"; do
    rederive_one "$entry" "$bin"
  done

  # What the caller can check. A gate is proven by having RUN, never by having
  # failed to complain: the lane asks for a re-derivation and must be able to
  # tell a clean one from a skip.
  echo "registry-rederive: re-derived ${rederived} of ${#entries[@]} entry(ies)."
  if [[ -n "${VEREDICTUM_REQUIRE_REDERIVATION:-}" && "$rederived" -eq 0 ]]; then
    echo "::error::nothing was re-derived, and this caller required it — a submission that reaches the signing step unrecomputed is the failure this gate exists to prevent" >&2
    exit 1
  fi
}

main "$@"
