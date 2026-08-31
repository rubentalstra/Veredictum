#!/usr/bin/env bash
# SPDX-FileCopyrightText: Veredictum contributors
# SPDX-License-Identifier: Apache-2.0
# Completing a console submission (#392): the lane seals the record and writes
# the provenance block the instrument is not allowed to write for itself.
#
# A `console` entry arrives without provenance. Everything that tier asserts is
# something this repository established rather than something the performer
# claimed, so this script — and only this script, run from the protected
# environment that holds the key — writes it:
#
#   * the instrument and the run id come from what the LANE observes (the App
#     identity that opened the submission, and the branch it arrived on), never
#     from a value the submission carries;
#   * the workflow reference, run id and attempt are this run's own;
#   * the signature is made here, over the record manifest, after the
#     re-derivation gate has already agreed with the submitted judgement.
#
# It writes files and nothing else: committing them is the caller's job, and
# the caller commits through the Git Data API so the commit is signed
# (root CLAUDE.md, hard rule 9).
#
# Usage:
#   scripts/registry/complete-console-entry.sh <entry.json>
#
# Environment:
#   VEREDICTUM_BIN            the engine binary (default: built from this tree)
#   CONSOLE_ORIGIN            the instrument that drove the run
#   CONSOLE_RUN_ID            that run's id at the instrument
#   SIGN_WORKFLOW_REF         the OIDC workflow_ref of the re-deriving lane
#   SIGN_RUN_ID               that workflow run's id
#   SIGN_RUN_ATTEMPT          which attempt of it
#   REGISTRY_SIGN_KEY         path to the armored secret key
#   REGISTRY_SIGN_PASSPHRASE  its passphrase, when it has one
#   REGISTRY_PUBLIC_KEY       the committed public half a reader verifies with
set -euo pipefail

# The tree the entry paths resolve against. The repository itself, unless a
# caller points this at a prepared tree — which is how the gate is tested
# without writing a fixture into the published registry.
cd "${REGISTRY_TREE:-$(dirname "$0")/../..}"

readonly PUBLIC_KEY_DEFAULT='registry/keys/registry-signing.pub.asc'

require() {
  local name="$1"
  if [[ -z "${!name:-}" ]]; then
    echo "::error::$name is empty — a console record is signed only from the protected environment that holds the key, and a lane that cannot sign must fail rather than publish an unsigned entry" >&2
    exit 1
  fi
}

engine() {
  if [[ -n "${VEREDICTUM_BIN:-}" ]]; then
    printf '%s' "$VEREDICTUM_BIN"
    return 0
  fi
  cargo build --locked --release -p veredictum --bin veredictum >&2
  printf '%s' "target/release/veredictum"
}

role_path() {
  jq -r --arg role "$2" \
    '[.artifacts[] | select(.role == $role) | .path] | first // ""' "$1"
}

main() {
  local entry="${1:-}"
  if [[ -z "$entry" || ! -f "$entry" ]]; then
    echo "usage: scripts/registry/complete-console-entry.sh <entry.json>" >&2
    exit 2
  fi
  command -v jq >/dev/null || { echo "jq is required" >&2; exit 1; }
  command -v sha256sum >/dev/null || { echo "sha256sum is required" >&2; exit 1; }

  require CONSOLE_ORIGIN
  require CONSOLE_RUN_ID
  require SIGN_WORKFLOW_REF
  require SIGN_RUN_ID
  require SIGN_RUN_ATTEMPT
  require REGISTRY_SIGN_KEY
  local public_key="${REGISTRY_PUBLIC_KEY:-$PUBLIC_KEY_DEFAULT}"
  if [[ ! -f "$public_key" ]]; then
    echo "::error::$public_key is not committed — a reader checks a console record against the public half, so it lives in the tree beside the entries" >&2
    exit 1
  fi

  local results statement record
  results="$(role_path "$entry" results)"
  statement="$(role_path "$entry" statement)"
  if [[ -z "$results" || -z "$statement" ]]; then
    echo "::error::$entry carries no results or statement artifact, so nothing can be sealed" >&2
    exit 1
  fi
  record="$(dirname "$results")"

  local bin
  bin="$(engine)"

  # The seal is made over the documents the catalogue computes from the
  # submitted outcomes. The re-derivation gate has already established that
  # those outcomes follow from the recorded exchanges and that the submitted
  # verdicts are what the catalogue computes, so this writes the same bytes
  # and adds the manifest and the detached signature over them.
  # `verdicts` exits 1 when the judgement is not clean — a failed capability
  # or a static-review finding — and that is a RESULT rather than a failure to
  # compute: it belongs in the record and gets signed like any other. Only a
  # refusal to run at all (exit 2) stops the seal, and the missing-file check
  # below is what catches a seal that did not happen.
  local sealed=0
  VEREDICTUM_SIGN_PASSPHRASE="${REGISTRY_SIGN_PASSPHRASE:-}" \
    "$bin" verdicts \
      --statement "$statement" \
      --results "$results" \
      --root artifacts \
      --out "$record" \
      --sign-key "$REGISTRY_SIGN_KEY" >/dev/null || sealed=$?
  if [[ "$sealed" -gt 1 ]]; then
    echo "::error::the judgement could not be computed at all (exit $sealed), so nothing was sealed" >&2
    exit 1
  fi

  local manifest="$record/record-manifest.json"
  local signature="$record/record-manifest.json.asc"
  if [[ ! -f "$manifest" || ! -f "$signature" ]]; then
    echo "::error::the seal produced no $manifest and $signature" >&2
    exit 1
  fi

  # The identity a reader checks against is read back out of the signature
  # itself, so the entry cannot name a key that did not make it.
  local verified fingerprint
  verified="$("$bin" verify-record --record "$record" --key "$public_key")"
  fingerprint="$(printf '%s' "$verified" | sed -nE 's/.*[Ff]ingerprint[: ]+([0-9A-Fa-f]+).*/\1/p' | head -1)"
  if [[ -z "$fingerprint" ]]; then
    echo "::error::the sealed record does not verify against $public_key" >&2
    printf '%s\n' "$verified" >&2
    exit 1
  fi

  local tmp
  tmp="$(mktemp)"
  # shellcheck disable=SC2064 # the path is expanded now, on purpose
  trap "rm -f '$tmp'" EXIT

  # Every rendered document the seal produced becomes a pinned artifact, so
  # the entry stands on the exact bytes the manifest covers.
  local -a added=()
  local file role digest name
  while IFS= read -r file; do
    name="$(basename "$file")"
    case "$name" in
      verdicts.json) continue ;;
      record-manifest.json) role='record-manifest' ;;
      record-manifest.json.asc) role='signature' ;;
      *.md|*.html|*.svg|*.json) role='report' ;;
      *) continue ;;
    esac
    digest="$(sha256sum "$file" | cut -d' ' -f1)"
    added+=("$(jq -n --arg role "$role" --arg path "$file" --arg sha "$digest" \
      '{role: $role, path: $path, sha256: $sha}')")
  done < <(find "$record" -maxdepth 1 -type f | sort)

  # The verdicts artifact's digest moves with the sealed bytes.
  digest="$(sha256sum "$record/verdicts.json" | cut -d' ' -f1)"

  jq \
    --argjson added "$(printf '%s\n' "${added[@]}" | jq -s '.')" \
    --arg verdicts_path "$record/verdicts.json" \
    --arg verdicts_sha "$digest" \
    --arg origin "$CONSOLE_ORIGIN" \
    --arg run "$CONSOLE_RUN_ID" \
    --arg workflow "$SIGN_WORKFLOW_REF" \
    --arg ci_run "$SIGN_RUN_ID" \
    --argjson attempt "$SIGN_RUN_ATTEMPT" \
    --arg signature "$signature" \
    --arg manifest "$manifest" \
    --arg fingerprint "$fingerprint" \
    --arg verify "veredictum verify-record --record $record --key $public_key" \
    '
      .artifacts = (
        [ .artifacts[]
          | if .path == $verdicts_path then .sha256 = $verdicts_sha else . end
        ]
        + $added
      )
      | .artifacts = (.artifacts | unique_by(.path))
      | .provenance = {
          tier: "console",
          instrument_origin: $origin,
          console_run_id: $run,
          workflow_ref: $workflow,
          run_id: $ci_run,
          run_attempt: $attempt,
          scheme: "openpgp-detached",
          signature: $signature,
          signs: $manifest,
          identity: $fingerprint,
          verify_command: $verify
        }
    ' "$entry" > "$tmp"
  mv "$tmp" "$entry"
  trap - EXIT

  echo "complete-console-entry: sealed $record and wrote the console provenance of $entry (signed by $fingerprint)"
}

main "$@"
