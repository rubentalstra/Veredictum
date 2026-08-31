#!/usr/bin/env bash
# SPDX-FileCopyrightText: Veredictum contributors
# SPDX-License-Identifier: Apache-2.0
# The local battery must cover the CI battery (#466).
#
# `scripts/checks/gates.sh` exists so "run the gates" cannot mean a subset
# somebody remembered. That only holds while its list matches what ci.yml
# actually runs, and on 2026-08-31 it did not: ci.yml ran 22 script
# invocations and gates.sh ran 7 of them. Two of the four failures on that
# day's pull request were commands in the gap.
#
# So the list is held to its source. Every `bash scripts/…` and `bash fuzz/…`
# invocation in ci.yml must appear in `gates.sh --commands`, and so must every
# cargo command, compared after normalization. What normalization drops, and
# why each is not a difference in what gets checked: `--locked` and
# `--profile ci` (reproducibility and CI reporting), `--quiet`, a leading
# environment assignment (the doc passes set RUSTFLAGS inline locally and
# through `env:` in CI), `+nightly` (cargo-fuzz needs it and CI's toolchain is
# already nightly there), and `--target <triple>`, which is host-specific by
# construction — the fuzz lane must name it explicitly, and a local run's host
# is not the runner's.
#
# What it deliberately does NOT check: that a gate's flags are otherwise
# identical, or that a gate runs in the same environment. A normalization that
# tried to prove equivalence would grow its own bugs; this one answers the
# question that actually went wrong, which is whether the command is present
# at all.
set -uo pipefail
cd "$(dirname "$0")/../.."

WORKFLOW=.github/workflows/ci.yml

# Strip the flags CI adds for its own reasons, collapse whitespace, and drop a
# leading environment assignment (the doc passes set RUSTFLAGS inline locally
# and through `env:` in CI).
normalize() {
  sed -E \
    -e "s/^(([A-Z_]+=('[^']*'|\"[^\"]*\"|[^ ]*) +)+)//" \
    -e 's/ \+nightly / /' \
    -e 's/--locked ?//g' \
    -e 's/--quiet ?//g' \
    -e 's/--profile ci ?//g' \
    -e 's/--target ("[^"]*"|\$\([^)]*\)|[^ ]+) ?//g' \
    -e 's/  +/ /g' \
    -e 's/ +$//'
}

ci_cmds=()
while IFS= read -r line; do
  [[ -n "$line" ]] && ci_cmds+=("$line")
done < <(
  {
    grep -oE 'bash (scripts|fuzz)/[^ ]+\.sh( --[a-z-]+)?' "$WORKFLOW"
    grep -oE 'run: cargo [^|]+$' "$WORKFLOW" | sed -E 's/^run: //'
  } | normalize | LC_ALL=C sort -u
)

gate_cmds=()
while IFS= read -r line; do
  [[ -n "$line" ]] && gate_cmds+=("$line")
done < <(bash scripts/checks/gates.sh --commands | normalize | LC_ALL=C sort -u)

missing=()
for cmd in ${ci_cmds[@]+"${ci_cmds[@]}"}; do
  [[ -n "$cmd" ]] || continue
  found=0
  for have in ${gate_cmds[@]+"${gate_cmds[@]}"}; do
    if [[ "$have" == "$cmd" ]]; then found=1; break; fi
  done
  if [[ "$found" -eq 0 ]]; then missing+=("$cmd"); fi
done

if [[ ${#missing[@]} -gt 0 ]]; then
  echo "::error::${WORKFLOW} runs ${#missing[@]} command(s) that scripts/checks/gates.sh does not, so a local 'gates green' says nothing about them:" >&2
  printf '  %s\n' "${missing[@]}" >&2
  echo "Add each as a gate() line, or, where a command cannot run outside CI, add it with its required tool so it SKIPS by name instead of vanishing." >&2
  exit 1
fi

echo "gates-cover-ci: every ci.yml command has a gate (${#ci_cmds[@]} checked) — OK."
