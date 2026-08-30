#!/usr/bin/env bash
# SPDX-FileCopyrightText: Veredictum contributors
# SPDX-License-Identifier: Apache-2.0
# Every TODO names its tracker issue, OUTSIDE the Rust tree too (#264).
#
# comment-style.sh enforces the `TODO(#NNNN):` form over hand-written `.rs`
# files, and nothing enforced it anywhere else — a bare `# TODO:` sat in an
# operation binding until the #264 sweep. This guard reads every committed
# hand-written file whose comment marker is `#` (YAML, shell, TOML) and
# refuses a TODO that names no issue. The vendored trees are excluded:
# acting on a finding inside vendored bytes is forbidden here.
#
# One awk process scans the whole file list (#333): the original
# awk-per-file loop died with SIGBUS on macOS part-way through
# `artifacts/schedule/`, and a per-file `return $count` would overflow a
# shell return code past 255 findings anyway. The tally is printed, never
# returned.
#
# Usage: scripts/checks/todo-issue-refs.sh            # whole tree
#        scripts/checks/todo-issue-refs.sh --self-test # seeded-violation proof
set -euo pipefail
cd "$(dirname "$0")/../.."

scan() { # stdin: NUL-separated file list; prints findings, then a count line
  xargs -0 awk '
    /^[[:space:]]*#/ || /[[:space:]]#/ {
      line = $0
      sub(/^[^#]*#/, "#", line)
      if (line ~ /^#+[[:space:]]*TODO/ && line !~ /TODO\(#[0-9]+\):/) {
        printf "%s:%d: TODO without an issue reference — the only sanctioned form is `TODO(#NNNN):`\n", FILENAME, FNR
        findings++
      }
    }
    END { printf "count=%d\n", findings }
  '
}

if [[ "${1:-}" == "--self-test" ]]; then
  tmp="$(mktemp -d)"
  trap 'rm -rf "$tmp"' EXIT
  seeded="$tmp/seeded.yaml"
  {
    echo '# TODO: a bare marker, refused'
    echo 'key: value   # TODO fix this later, refused'
    echo '# TODO(#123): carries its issue, passes'
    echo 'path: "a#TODO-in-a-string-is-not-a-comment-start"'
    echo '# a TODO mentioned mid-sentence passes: the marker is leading-only'
  } >"$seeded"
  out="$(printf '%s\0' "$seeded" | scan)"
  fail=0
  for want in 1 2; do
    grep -q "seeded.yaml:$want: TODO without" <<<"$out" || {
      echo "self-test: line $want was NOT reported — the guard does not fire" >&2
      fail=1
    }
  done
  for unwanted in 3 4 5; do
    if grep -q "seeded.yaml:$unwanted: TODO without" <<<"$out"; then
      echo "self-test: line $unwanted was reported — a sanctioned form is refused" >&2
      fail=1
    fi
  done
  grep -q '^count=2$' <<<"$out" || {
    echo "self-test: the tally is not 2:" >&2
    grep '^count=' <<<"$out" >&2
    fail=1
  }
  [[ "$fail" -eq 0 ]] || exit 1
  echo "todo-issue-refs: self-test OK (2 seeded violations caught, 3 legitimate lines passed)."
  exit 0
fi

# The guard excludes itself: the self-test above seeds literal bare TODOs,
# which are test fixtures, not pending work.
list="$(mktemp)"
trap 'rm -f "$list"' EXIT
git ls-files -z '*.yml' '*.yaml' '*.sh' '*.toml' \
  ':(exclude)specs/**' ':(exclude)fuzz/corpus/**' ':(exclude)fuzz/seeds/**' \
  ':(exclude)scripts/checks/todo-issue-refs.sh' >"$list"
out="$(scan <"$list")"
# xargs may split a long list into several awk invocations, one count line
# each; the tally is their sum.
count="$(grep '^count=' <<<"$out" | cut -d= -f2 | awk '{ sum += $1 } END { print sum + 0 }')"
grep -v '^count=' <<<"$out" >&2 || true
files="$(tr -cd '\0' <"$list" | wc -c | tr -d ' ')"
if [[ "$count" -ne 0 ]]; then
  echo "::error::${count} bare TODO(s) found — give each its tracker issue as TODO(#NNNN):" >&2
  exit 1
fi
echo "todo-issue-refs: OK (${files} files)."
