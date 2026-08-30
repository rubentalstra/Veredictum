#!/usr/bin/env bash
# SPDX-FileCopyrightText: Veredictum contributors
# SPDX-License-Identifier: Apache-2.0
# Every TODO names its tracker issue, OUTSIDE the Rust tree too (#264).
#
# comment-style.sh enforces the `TODO(#NNNN):` form over hand-written `.rs`
# files, and nothing enforced it anywhere else — a bare `# TODO:` sat in an
# operation binding until the #264 sweep. This sibling guard reads every
# committed hand-written file whose comment marker is `#` (YAML, shell, TOML)
# and refuses a TODO that names no issue. The vendored trees are excluded:
# acting on a finding inside vendored bytes is forbidden here.
#
# Usage: scripts/checks/todo-issue-refs.sh            # whole tree
#        scripts/checks/todo-issue-refs.sh --self-test # seeded-violation proof
set -euo pipefail
cd "$(dirname "$0")/../.."

scan() { # $1... = files
  local failures=0 file
  for file in "$@"; do
    [[ -f "$file" ]] || continue
    while IFS= read -r hit; do
      [[ -n "$hit" ]] || continue
      echo "${file}${hit}" >&2
      failures=$((failures + 1))
    done < <(awk '
      /^[[:space:]]*#/ || /[[:space:]]#/ {
        line = $0
        sub(/^[^#]*#/, "#", line)
        if (line ~ /^#+[[:space:]]*TODO/ && line !~ /TODO\(#[0-9]+\):/)
          printf ":%d: TODO without an issue reference — the only sanctioned form is `TODO(#NNNN):`\n", NR
      }' "$file")
  done
  return "$failures"
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
  out="$(scan "$seeded" 2>&1 >/dev/null || true)"
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
  [[ "$fail" -eq 0 ]] || exit 1
  echo "todo-issue-refs: self-test OK (2 seeded violations caught, 3 legitimate lines passed)."
  exit 0
fi

files=()
while IFS= read -r f; do [[ -f "$f" ]] && files+=("$f"); done < <(
  git ls-files '*.yml' '*.yaml' '*.sh' '*.toml' \
    ':(exclude)specs/**' ':(exclude)fuzz/corpus/**' ':(exclude)fuzz/seeds/**'
)
if scan "${files[@]}"; then
  echo "todo-issue-refs: OK (${#files[@]} files)."
else
  echo "::error::bare TODOs found — give each its tracker issue as TODO(#NNNN):" >&2
  exit 1
fi
