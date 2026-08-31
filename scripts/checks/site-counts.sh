#!/usr/bin/env bash
# SPDX-FileCopyrightText: Veredictum contributors
# SPDX-License-Identifier: Apache-2.0
# The website's catalogue counts cannot drift from the catalogue (#35).
#
# The landing page and the book state how many cases, bindings, capability
# matrix rows and outcome kinds the catalogue carries. Those literals are
# hand-typed, and nothing held them to the artifacts: a schedule release that
# moves a count would silently leave the public site wrong. This guard reads
# the truth from the instrument itself — the `validate` summary line for
# cases/bindings/capability rows, the outcome vocabulary for the kinds — and
# fails on any count-bearing phrase whose number disagrees.
#
# Usage: scripts/checks/site-counts.sh [<validate summary line>]
#   With no argument it runs `cargo run -- validate` itself (the CI test job
#   passes the line it already produced, so the binary runs once).
set -euo pipefail
cd "$(dirname "$0")/../.."

SURFACES=(website/landing/index.html website/book/src README.md ARCHITECTURE.md)

# Every extraction below ends in `|| true`, and that is load-bearing: under
# `set -e` a `$( … | grep )` whose grep matches nothing exits the script on the
# spot, so the diagnostic underneath was unreachable and CI failed with no
# message at all (POSIX shell rule: the pipeline's status is its last command's,
# <https://pubs.opengroup.org/onlinepubs/9699919799/utilities/V3_chap02.html>).
# The empty value is what the loop is written to catch.
summary="${1:-}"
if [[ -z "$summary" ]]; then
  summary="$(cargo run --quiet --locked -- validate --root artifacts --specs specs/openehr | grep -E '[0-9]+ case\(s\)' || true)"
fi
cases=$(grep -oE '[0-9]+ case\(s\)' <<<"$summary" | grep -oE '^[0-9]+' || true)
bindings=$(grep -oE '[0-9]+ binding\(s\)' <<<"$summary" | grep -oE '^[0-9]+' || true)
capabilities=$(grep -oE '[0-9]+ capability row\(s\)' <<<"$summary" | grep -oE '^[0-9]+' || true)
outcomes=$(grep -cE '^[a-z_]+: ' artifacts/vocab/outcomes.yaml || true)
for truth in cases bindings capabilities outcomes; do
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
check "$capabilities" '[0-9]+ capability row\(s\)' "capability rows"
check "$outcomes" 'There are [0-9]+ of them' "outcome kinds"

# The credits section counts how many case cores cite the CNF Test Schedule
# and its Pazos-raised chapters, and how many corpus records carry provenance.
# Each is derivable from the tree, so each is held to it.
schedule_citers=$(grep -rlE 'platform_test_schedule' artifacts/schedule --include='*.yaml' | wc -l | tr -d ' ')
pazos_citers=$(grep -rlE 'platform_test_schedule master(06|07|08|09)' artifacts/schedule --include='*.yaml' | wc -l | tr -d ' ')
provenance_records=$(grep -cE '^[[:space:]]+provenance: ' artifacts/corpus/MANIFEST.yaml || true)
for truth in schedule_citers pazos_citers provenance_records; do
  [[ "${!truth}" =~ ^[1-9][0-9]*$ ]] || { echo "::error::could not derive the ${truth} count" >&2; exit 1; }
done

check "$schedule_citers" '[0-9]+ of the [0-9]+ case cores here cite' "Test Schedule citers"
check "$pazos_citers" '[0-9]+ of the [0-9]+ case cores cite' "Pazos-chapter citers"
check "$provenance_records" 'the [0-9]+ corpus provenance records' "corpus provenance records"

# The install snippets name the published crate version by hand, in several
# copies; hold every copy to the workspace manifest so a release bump cannot
# leave a page installing the superseded version.
crate_version="$(grep -m1 '^version = ' app/veredictum/Cargo.toml | cut -d'"' -f2)"
while IFS= read -r hit; do
  [[ -n "$hit" ]] || continue
  file="${hit%%:*}"
  n="${hit##*--version }"
  n="${n%% *}"
  if [[ "$n" != "$crate_version" ]]; then
    echo "::error::${file} installs --version ${n}, the workspace is ${crate_version} — update the snippet" >&2
    failures=$((failures + 1))
  fi
done < <(grep -roE -- 'cargo install veredictum[^`<]*--version [0-9a-zA-Z.-]+' "${SURFACES[@]}")

if [[ "$failures" -gt 0 ]]; then
  exit 1
fi
echo "site-counts: every count on the site matches the catalogue (${cases} cases, ${bindings} bindings, ${capabilities} capability rows, ${outcomes} outcome kinds) — OK."
