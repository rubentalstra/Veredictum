#!/usr/bin/env bash
# SPDX-FileCopyrightText: Veredictum contributors
# SPDX-License-Identifier: Apache-2.0
# A benchmark submission is validated by machine before a human reads it.
#
# The submission channel is a pull request that ADDS one record under
# `benchmarks/submissions/`, and the merge is the acceptance. That only works
# if the merge is preceded by a check nobody has to remember to run, so this
# script is the whole gate, in three parts:
#
#   1. Append-only. A merged record is evidence somebody published; editing or
#      deleting one rewrites a published claim. Any Modified, Deleted or Renamed
#      path under the submissions tree fails, and only additions pass.
#   2. Record content. The published bench-result schema, the embedded pack and
#      its fixture pins, the submittability arithmetic (three repetitions and at
#      least one same-machine baseline), the environment fingerprint, and the
#      file name that must digest to that fingerprint. That half is a Rust
#      integration test, because it reads the same pack definitions and the same
#      result model the engine wrote the record with; a second reimplementation
#      in shell would be a second thing to keep true.
#   3. The board is not stale. The page is generated from these records and
#      committed, so a merged submission that leaves it unchanged would publish
#      a board that omits the thing just accepted.
#
# Usage:
#   scripts/checks/bench-submission.sh                 # against origin/main
#   scripts/checks/bench-submission.sh <base> <head>   # an explicit range (CI)
#
# `VEREDICTUM_BENCH_SUBMISSION_SKIP_CARGO=1` runs parts 1 and 3 alone, for a
# caller that has already run the integration suite.
set -euo pipefail

cd "$(dirname "$0")/../.."

readonly SUBMISSIONS='benchmarks/submissions'

base="${1:-origin/main}"
head="${2:-HEAD}"

# ── 1. Append-only ───────────────────────────────────────────────────────────
# The three-dot form asks what the head added since the merge base, which is the
# question a pull request is actually about; a two-dot range would report every
# change made on the base branch since the fork point as a deletion.
if git rev-parse --verify --quiet "$base" >/dev/null; then
  touched="$(git diff --name-status --diff-filter=MDR "$base...$head" -- "$SUBMISSIONS" || true)"
  if [[ -n "$touched" ]]; then
    echo "::error::the submissions tree is append-only, and this change does not only add" >&2
    echo "$touched" | sed 's/^/  /' >&2
    echo >&2
    echo "A merged record is a published claim. Correct one by adding a new record" >&2
    echo "and saying in the pull request what the earlier one got wrong." >&2
    exit 1
  fi
  echo "bench-submission: nothing under $SUBMISSIONS was modified, deleted or renamed — OK."
else
  echo "bench-submission: $base does not resolve here, so the append-only half is skipped." >&2
fi

# ── 2. Record content ────────────────────────────────────────────────────────
if [[ "${VEREDICTUM_BENCH_SUBMISSION_SKIP_CARGO:-0}" == "1" ]]; then
  echo "bench-submission: the record-content gate is skipped by request."
else
  command -v cargo >/dev/null || { echo "cargo is required" >&2; exit 1; }
  cargo nextest run --locked -E 'test(bench_submissions)'
fi

# ── 3. The board reflects what is committed ──────────────────────────────────
bash scripts/render/bench-board.sh --check
