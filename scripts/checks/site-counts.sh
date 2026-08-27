#!/usr/bin/env bash
# SPDX-FileCopyrightText: Veredictum contributors
# SPDX-License-Identifier: Apache-2.0
# The website's catalogue counts cannot drift from the catalogue (#35).
#
# The landing page and the book state how many cases, bindings, party
# statements and outcome kinds the catalogue carries. Those literals are
# hand-typed, and nothing held them to the artifacts: a schedule release that
# moves a count would silently leave the public site wrong. This guard reads
# the truth from the instrument itself — the `validate` summary line for
# cases/bindings/statements, the outcome vocabulary for the kinds — and fails
# on any count-bearing phrase whose number disagrees.
#
# Usage: scripts/checks/site-counts.sh [<validate summary line>]
#   With no argument it runs `cargo run -- validate` itself (the CI test job
#   passes the line it already produced, so the binary runs once).
set -euo pipefail
cd "$(dirname "$0")/../.."

SURFACES=(website/landing/index.html website/book/src)

summary="${1:-}"
if [[ -z "$summary" ]]; then
  summary="$(cargo run --quiet --locked -- validate --root artifacts --specs specs/openehr | grep -E '[0-9]+ case\(s\)')"
fi
cases=$(grep -oE '[0-9]+ case\(s\)' <<<"$summary" | grep -oE '^[0-9]+')
bindings=$(grep -oE '[0-9]+ binding\(s\)' <<<"$summary" | grep -oE '^[0-9]+')
statements=$(grep -oE '[0-9]+ party statement\(s\)' <<<"$summary" | grep -oE '^[0-9]+')
outcomes=$(grep -cE '^[a-z_]+: ' artifacts/vocab/outcomes.yaml)
for truth in cases bindings statements outcomes; do
  [[ -n "${!truth}" ]] || { echo "::error::could not derive the ${truth} count" >&2; exit 1; }
done

failures=0
check() { # $1 = expected, $2 = extraction regex, $3 = label
  local expected="$1" regex="$2" label="$3" hit file match n
  while IFS= read -r hit; do
    [[ -n "$hit" ]] || continue
    file="${hit%%:*}"
    match="${hit#*:}"
    n=$(grep -oE '[0-9]+' <<<"$match" | head -1)
    if [[ "$n" != "$expected" ]]; then
      echo "::error::${file} states '${match}', the catalogue carries ${expected} ${label} — update the page (or the phrase joined a pattern it should not match)" >&2
      failures=$((failures + 1))
    fi
  done < <(grep -roE "$regex" "${SURFACES[@]}")
}

check "$cases" '[0-9]+ (spec-cited (test )?cases|test cases|case cores|case\(s\))' "cases"
check "$bindings" '[0-9]+ (operation bindings|binding\(s\)|bindings today)' "bindings"
# The landing page's fact cards carry the number and its label in separate
# spans, so the prose patterns above never see them — proven by mutation.
check "$cases" '"n">[0-9]+</span><span class="l">spec-cited cases' "cases (fact card)"
check "$bindings" '"n">[0-9]+</span><span class="l">operation bindings' "bindings (fact card)"
check "$statements" '[0-9]+ party statement\(s\)' "party statements"
check "$outcomes" 'There are [0-9]+ of them' "outcome kinds"

if [[ "$failures" -gt 0 ]]; then
  exit 1
fi
echo "site-counts: every count on the site matches the catalogue (${cases} cases, ${bindings} bindings, ${statements} statements, ${outcomes} outcome kinds) — OK."
