#!/usr/bin/env bash
# SPDX-FileCopyrightText: Veredictum contributors
# SPDX-License-Identifier: Apache-2.0
# Runs the documented gate battery, so "run the gates" cannot mean a subset
# somebody remembered (#466).
#
# The battery was prose in four places — the root CLAUDE.md, CONTRIBUTING.md,
# the SessionStart dump and two agent definitions — and nothing ran it. On
# 2026-08-31 five changes reached CI with a rustdoc failure the local run had
# not caught, every one of them because the person or agent ran the cargo
# commands they remembered and not the doc pass. Two of those were the session
# model's own. A list people approximate is not a gate.
#
# The rustdoc pass is the one that matters most here and the one most skipped,
# because it reads as a documentation formality and is not: the workspace doc
# build compiles the console FEATURELESS, so a doc link from an ungated item
# into an `ssr`-gated one cannot resolve, and build, test, clippy and fmt all
# pass regardless. CI runs TWO doc commands and both are below.
#
# Usage:
#   scripts/checks/gates.sh              # the guard tier and the rust tier
#   scripts/checks/gates.sh --all        # everything, including the slow lanes
#   scripts/checks/gates.sh --guards     # the guard tier alone (no cargo)
#   scripts/checks/gates.sh --console    # the console's own two targets
#   scripts/checks/gates.sh --list       # print the battery without running it
#
# Every gate is named, run in order, and its outcome printed. The script exits
# non-zero if any gate failed, and reports EVERY failure rather than stopping at
# the first: a run that tells you about one problem when there are three costs
# three round trips.
set -uo pipefail
cd "$(dirname "$0")/../.." || exit 1

declare -a NAMES=() CMDS=() TIERS=()
gate() { TIERS+=("$1"); NAMES+=("$2"); CMDS+=("$3"); }

gate guard "comment style"            "bash scripts/checks/comment-style.sh --all"
gate guard "TODO issue refs"          "bash scripts/checks/todo-issue-refs.sh"
gate guard "changelog structure"      "bash scripts/checks/changelog-structure.sh"
gate guard "hosted-instrument words"  "bash scripts/checks/hosted-instrument-language.sh"
gate guard "CI jobs gate the merge"   "bash scripts/checks/ci-conclusion-complete.sh"
gate guard "CLI surface copies"       "bash scripts/checks/cli-surface.sh"
gate guard "registry submissions"     "bash scripts/checks/registry-submission.sh"

gate rust  "build"                    "cargo build --workspace --all-targets"
gate rust  "tests"                    "cargo nextest run --workspace"
gate rust  "clippy"                   "cargo clippy --workspace --all-targets -- -D warnings"
gate rust  "fmt"                      "cargo fmt --all --check"
# BOTH doc passes, exactly as ci.yml runs them. The second one is why this
# script exists.
gate rust  "rustdoc (workspace)"      "RUSTFLAGS='-D warnings' RUSTDOCFLAGS='-D warnings' cargo doc --locked --workspace --no-deps --document-private-items"
gate rust  "rustdoc (console ssr)"    "RUSTFLAGS='-D warnings' RUSTDOCFLAGS='-D warnings' cargo doc --locked -p veredictum-console --no-deps --document-private-items --features ssr"
gate rust  "validate the catalogue"   "cargo run --quiet -- validate --root artifacts --specs specs/openehr"

gate console "clippy (ssr)"           "cargo clippy -p veredictum-console --features ssr --all-targets -- -D warnings"
gate console "clippy (wasm32 hydrate)" "cargo clippy -p veredictum-console --lib --target wasm32-unknown-unknown --no-default-features --features hydrate -- -D warnings"
gate console "tests (ssr)"            "cargo nextest run -p veredictum-console --features ssr"
gate console "leptosfmt"              "leptosfmt --check app/veredictum-console/src"

gate slow  "cargo deny"               "cargo deny check"
gate slow  "MSRV"                     "cargo hack check --rust-version --all-targets --workspace"
gate slow  "unused dependencies"      "cargo machete"

WANT="guard rust"
case "${1:-}" in
  --all)     WANT="guard rust console slow" ;;
  --guards)  WANT="guard" ;;
  --console) WANT="console" ;;
  --list)
    for i in "${!NAMES[@]}"; do printf '%-8s %-24s %s\n' "${TIERS[$i]}" "${NAMES[$i]}" "${CMDS[$i]}"; done
    exit 0 ;;
  "") ;;
  *) echo "gates: unknown option ${1}; --all, --guards, --console, --list" >&2; exit 2 ;;
esac

failed=()
for i in "${!NAMES[@]}"; do
  case " $WANT " in *" ${TIERS[$i]} "*) ;; *) continue ;; esac
  printf '\033[1m==>\033[0m %s\n' "${NAMES[$i]}"
  if eval "${CMDS[$i]}" >/tmp/gate.$$.log 2>&1; then
    printf '    ok\n'
  else
    printf '    FAILED\n'
    tail -12 /tmp/gate.$$.log | sed 's/^/    /'
    failed+=("${NAMES[$i]}")
  fi
  rm -f /tmp/gate.$$.log
done

if [ ${#failed[@]} -gt 0 ]; then
  printf '\n\033[1mgates: %d failed:\033[0m %s\n' "${#failed[@]}" "$(printf '%s; ' "${failed[@]}")" >&2
  exit 1
fi
printf '\ngates: every gate in [%s] passed\n' "$WANT"
