#!/usr/bin/env bash
# SPDX-FileCopyrightText: Veredictum contributors
# SPDX-License-Identifier: Apache-2.0
# Perform one tier-1 reproduction: compose a committed topology, drive the
# catalogue against it, judge the results, and write the bundle the workflow
# attests.
#
# The whole orchestration lives here rather than in the workflow so that it is
# one shellcheck-linted file a maintainer can run on a workstation, and so the
# workflow stays a thin caller with no context value reaching a shell.
#
# WHAT MAY BE COMPOSED, and why the list is closed. A reproduction runs with an
# OIDC token that can mint an attestation carrying this repository's identity,
# so it executes nothing a submitter wrote. The only deployments it stands up
# are the topologies declared under `registry/topologies/`, whose recipes are
# committed here or fetched from the upstream repository the declaration names.
# Adding one is a change to this repository, reviewed like any other.
#
# Usage:
#   scripts/registry/reproduce.sh <topology-id> <out-dir> [<sut-version>]
#
# Writes into <out-dir>:
#   deployment.json   the images the run actually composed, by digest
#   ixit.json         the declaration the run was driven under, byte for byte
#   run/results.json  the recorded catalogue run
#   judgement/…       verdicts.json and the rendered documents
#
# WHY THE IXIT TRAVELS WITH THE BUNDLE. A topology declares the principals its
# composed deployment actually has, which is narrower than a party's own
# declaration: the quickstarts stand up one clinical principal, so every case
# addressing an admin or read-only principal is recorded not-applicable at
# selection time. A reader cannot check that reason against a digest alone, so
# the bundle carries the declaration the digest was taken over and the run
# record's `ixit_digest` is re-derived from it below.
set -euo pipefail

cd "$(dirname "$0")/../.."
ROOT="$PWD"

command -v jq >/dev/null || { echo "jq is required" >&2; exit 1; }
command -v docker >/dev/null || { echo "docker is required" >&2; exit 1; }

# The runner records the leading 8 bytes of the SHA-256 over the ixit
# document, so the check below needs whichever of the two spellings this
# machine carries: coreutils on a runner, BSD on a workstation.
if command -v sha256sum >/dev/null; then
  sha256_of() { sha256sum "$1" | cut -d' ' -f1; }
elif command -v shasum >/dev/null; then
  sha256_of() { shasum -a 256 "$1" | cut -d' ' -f1; }
else
  echo "sha256sum or shasum is required" >&2
  exit 1
fi

if [[ $# -lt 2 ]]; then
  echo "usage: $0 <topology-id> <out-dir> [<sut-version>]" >&2
  exit 2
fi

TOPOLOGY="$1"
OUT="$2"
SUT_VERSION="${3:-unpinned}"

case "$TOPOLOGY" in
  *[!a-z0-9-]* | '')
    echo "::error::the topology id carries something outside [a-z0-9-]" >&2
    exit 2
    ;;
esac

DECLARATION="$ROOT/registry/topologies/$TOPOLOGY/topology.json"
if [[ ! -f "$DECLARATION" ]]; then
  echo "::error::no topology is declared at registry/topologies/$TOPOLOGY/topology.json" >&2
  exit 1
fi

mkdir -p "$OUT"
OUT="$(cd "$OUT" && pwd)"
WORK="$ROOT/target/registry-reproduce/$TOPOLOGY"
rm -rf "$WORK"
mkdir -p "$WORK"

field() { jq -r --arg name "$1" '.[$name] // ""' "$DECLARATION"; }

IXIT="$(field ixit)"
if [[ -z "$IXIT" || ! -f "$ROOT/$IXIT" ]]; then
  echo "::error::$TOPOLOGY names the ixit '${IXIT:-<none>}', which this tree does not carry" >&2
  exit 1
fi
STATEMENT="$(field statement)"
READY_URL="$(field ready_url)"
COMPOSE_FILE="$(field compose_file)"
PROJECT="veredictum-reproduce-$TOPOLOGY"

# ── The compose document ─────────────────────────────────────────────────────
# Either committed here, or the upstream quickstart the declaration names. The
# upstream form is deliberately unpinned: the deployment is the thing being
# graded, and a reproduction of somebody's published quickstart is a
# reproduction of whatever that release train currently ships. The digests the
# run actually composed are recorded below, so the record says which bytes
# answered.
if [[ -n "$COMPOSE_FILE" ]]; then
  cp "$ROOT/$COMPOSE_FILE" "$WORK/compose.yaml"
  echo "reproduce: composing the committed recipe $COMPOSE_FILE"
else
  repository="$(jq -r '.compose_from.repository // ""' "$DECLARATION")"
  path="$(jq -r '.compose_from.path // ""' "$DECLARATION")"
  if [[ -z "$repository" || -z "$path" ]]; then
    echo "::error::$TOPOLOGY declares neither compose_file nor compose_from" >&2
    exit 1
  fi
  command -v gh >/dev/null || { echo "gh is required to fetch an upstream recipe" >&2; exit 1; }
  echo "reproduce: fetching $repository/$path"
  gh api "repos/$repository/contents/$path" \
    -H "Accept: application/vnd.github.raw" > "$WORK/compose.yaml"
fi

# ── Compose, drive, tear down ────────────────────────────────────────────────
# The teardown runs whether or not the run succeeded, so a failed reproduction
# never leaves containers and volumes behind for the next one.
teardown() {
  docker compose -p "$PROJECT" -f "$WORK/compose.yaml" down -v >/dev/null 2>&1 || true
}
trap teardown EXIT

while IFS=$'\t' read -r key value; do
  [[ -n "$key" ]] || continue
  export "$key=$value"
done < <(jq -r '(.compose_env // {}) | to_entries[] | "\(.key)\t\(.value)"' "$DECLARATION")

echo "reproduce: composing $PROJECT"
docker compose -p "$PROJECT" -f "$WORK/compose.yaml" up -d --quiet-pull

# Answering means any HTTP status below 500: a 401 from basic auth is an
# answer, and waiting for 200 would wait forever on a server that is up.
echo "reproduce: waiting for $READY_URL"
for _ in $(seq 1 240); do
  code="$(curl -s -o /dev/null -w '%{http_code}' --max-time 2 "$READY_URL" || true)"
  if [[ "$code" != "000" && "$code" -lt 500 ]]; then
    break
  fi
  sleep 1
done
if [[ "${code:-000}" == "000" || "${code:-500}" -ge 500 ]]; then
  echo "::error::$TOPOLOGY never answered at $READY_URL (last status ${code:-none})" >&2
  exit 1
fi

# What actually answered, by digest. The declaration names a recipe; this names
# the bytes, and it is part of the bundle the attestation covers.
docker compose -p "$PROJECT" -f "$WORK/compose.yaml" ps --format json \
  | jq -s --arg topology "$TOPOLOGY" \
      'flatten | {topology: $topology,
        services: [ .[] | {service: .Service, image: .Image, state: .State} ]}' \
  > "$OUT/deployment.json"

# The declaration the run is about to be driven under, byte for byte, so the
# bundle answers "which principals did this deployment have" on its own.
cp "$ROOT/$IXIT" "$OUT/ixit.json"

while IFS=$'\t' read -r key value; do
  [[ -n "$key" ]] || continue
  export "$key=$value"
done < <(jq -r '(.credentials // {}) | to_entries[] | "\(.key)\t\(.value)"' "$DECLARATION")

run_args=(
  run
  --root artifacts
  --ixit "$IXIT"
  --out "$OUT/run"
  --sut-name "$TOPOLOGY"
  --sut-version "$SUT_VERSION"
)
[[ -n "$STATEMENT" ]] && run_args+=(--statement "$STATEMENT")

# The runner exits 0 clean, 1 with findings and 2 on a runner error. A
# reproduction whose rows went red is a legitimate record, so findings pass
# through; a runner error is not a result and stops the lane.
echo "reproduce: driving the catalogue"
set +e
cargo run --quiet --locked -- "${run_args[@]}"
drove=$?
set -e
if [[ "$drove" -ge 2 ]]; then
  echo "::error::the run could not be executed (exit $drove)" >&2
  exit 1
fi

if [[ ! -f "$OUT/run/results.json" ]]; then
  echo "::error::the run wrote no results.json, so there is nothing to judge" >&2
  exit 1
fi

# The bundle's own digest check, run here so a mismatch stops the lane instead
# of shipping a record nobody can resolve. This is the same derivation a reader
# performs over the carried declaration.
recorded_digest="$(jq -r '.ixit_digest // ""' "$OUT/run/results.json")"
carried_digest="$(sha256_of "$OUT/ixit.json" | cut -c1-16)"
if [[ "$recorded_digest" != "$carried_digest" ]]; then
  echo "::error::the record's ixit_digest $recorded_digest does not re-derive from the carried declaration ($carried_digest)" >&2
  exit 1
fi
echo "reproduce: ixit_digest $recorded_digest re-derives from $OUT/ixit.json"

echo "reproduce: judging the results"
judge_args=(
  verdicts
  --results "$OUT/run/results.json"
  --root artifacts
  --out "$OUT/judgement"
)
if [[ -n "$STATEMENT" ]]; then
  judge_args+=(--statement "$STATEMENT")
else
  echo "::error::$TOPOLOGY declares no statement, and a verdict is computed against one" >&2
  exit 1
fi
# The same tolerance as the run step: a judgement carrying review findings
# (exit 1) is a legitimate record whose verdicts state the findings, while a
# judge error (exit 2) is not a judgement and stops the lane.
set +e
cargo run --quiet --locked -- "${judge_args[@]}"
judged=$?
set -e
if [[ "$judged" -ge 2 ]]; then
  echo "::error::the verdicts could not be computed (exit $judged)" >&2
  exit 1
fi
if [[ ! -f "$OUT/judgement/verdicts.json" ]]; then
  echo "::error::the judgement wrote no verdicts.json" >&2
  exit 1
fi

echo "reproduce: wrote $OUT/ixit.json, $OUT/run/results.json and $OUT/judgement/verdicts.json"
