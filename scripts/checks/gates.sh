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

declare -a NAMES=() CMDS=() TIERS=() NEEDS=()
# gate <tier> <name> <command> [required-tool]
#
# A gate naming a tool the machine does not have is SKIPPED and says so by
# name. A silent skip and a pass print the same thing, which is the defect
# class this script exists to close, so the summary lists every skip.
gate() { TIERS+=("$1"); NAMES+=("$2"); CMDS+=("$3"); NEEDS+=("${4:-}"); }

gate guard "comment style"            "bash scripts/checks/comment-style.sh --all"
gate guard "comment style self-test"  "bash scripts/checks/comment-style.sh --self-test"
gate guard "TODO issue refs"          "bash scripts/checks/todo-issue-refs.sh"
gate guard "TODO refs self-test"      "bash scripts/checks/todo-issue-refs.sh --self-test"
gate guard "changelog structure"      "bash scripts/checks/changelog-structure.sh"
gate guard "changelog entry"          "bash scripts/checks/changelog-entry.sh"
gate guard "hosted-instrument words"  "bash scripts/checks/hosted-instrument-language.sh"
gate guard "hosted words self-test"   "bash scripts/checks/hosted-instrument-language.sh --self-test"
gate guard "hosted deploy script"     "bash scripts/checks/hosted-deploy-script.sh"
gate guard "hosted deploy self-test"  "bash scripts/checks/hosted-deploy-script.sh --self-test"
gate guard "CI jobs gate the merge"   "bash scripts/checks/ci-conclusion-complete.sh"
gate guard "gates cover CI"           "bash scripts/checks/gates-cover-ci.sh"
gate guard "CLI surface copies"       "bash scripts/checks/cli-surface.sh"
gate guard "registry submissions"     "bash scripts/checks/registry-submission.sh"
gate guard "benchmark submissions"    "bash scripts/checks/bench-submission.sh"
gate guard "the console's engine pin" "bash scripts/release/check-console-pin.sh"
gate guard "corpus OPTs reproduce"    "bash scripts/checks/corpus-opt-reproducible.sh"
gate guard "image labels"             "bash scripts/checks/image-labels.sh"
gate guard "VEX advisories"           "bash scripts/checks/vex-advisories.sh"
gate guard "the bench legend"         "bash scripts/render/bench-legend.sh --check"
gate guard "fuzz seeds"               "bash fuzz/seeds.sh"
# The three workflow and licensing checks CI runs through actions rather than
# through a `run:` line, so `gates-cover-ci.sh` cannot see them. Each names its
# tool and skips by name when it is absent.
gate guard "workflow security"        "zizmor --min-severity=low .github/workflows/ .github/actions/" zizmor
# SHELLCHECK_OPTS mirrors the workflow's own exclusion, for the reason recorded
# there: every SC2016 here is a `git log --format=` or a `jq` filter, where
# single quotes are the idiom that makes the directive reach the tool unexpanded.
gate guard "workflow correctness"     "SHELLCHECK_OPTS='-e SC2016' actionlint" actionlint
gate guard "REUSE licensing"          "reuse lint" reuse

gate rust  "build"                    "cargo build --workspace --all-targets"
gate rust  "tests"                    "cargo nextest run --workspace"
gate rust  "clippy"                   "cargo clippy --workspace --all-targets -- -D warnings"
gate rust  "fmt"                      "cargo fmt --all --check"
# BOTH doc passes, exactly as ci.yml runs them. The second one is why this
# script exists.
gate rust  "rustdoc (workspace)"      "RUSTFLAGS='-D warnings' RUSTDOCFLAGS='-D warnings' cargo doc --locked --workspace --no-deps --document-private-items"
gate rust  "rustdoc (console ssr)"    "RUSTFLAGS='-D warnings' RUSTDOCFLAGS='-D warnings' cargo doc --locked -p veredictum-console --no-deps --document-private-items --features ssr"
gate rust  "validate the catalogue"   "cargo run --quiet -- validate --root artifacts --specs specs/openehr"
gate rust  "the site's counts"        "bash scripts/checks/site-counts.sh"

gate console "clippy (ssr)"           "cargo clippy -p veredictum-console --all-targets --features ssr -- -D warnings"
gate console "clippy (wasm32 hydrate)" "cargo clippy -p veredictum-console --lib --target wasm32-unknown-unknown --no-default-features --features hydrate -- -D warnings"
gate console "tests (ssr)"            "cargo nextest run -p veredictum-console --features ssr"
gate console "leptosfmt"              "leptosfmt --check app/veredictum-console/src" leptosfmt

gate slow  "cargo deny"               "cargo deny check"
gate slow  "MSRV"                     "cargo hack check --rust-version --all-targets --workspace"
gate slow  "unused dependencies"      "cargo machete"
# The journeys and the fuzz harnesses each need something the other gates do
# not: a container runtime for the digest-pinned browser, and the nightly
# toolchain cargo-fuzz requires. Both are CI jobs of their own, so both are
# here rather than remembered.
gate slow  "console journeys"         "bash scripts/ui-e2e.sh" docker
gate slow  "fuzz harnesses build"     "cargo +nightly fuzz build --target \$(rustc -vV | sed -n 's|^host: ||p')" cargo-fuzz

WANT="guard rust"
case "${1:-}" in
  --all)     WANT="guard rust console slow" ;;
  --guards)  WANT="guard" ;;
  --console) WANT="console" ;;
  --list)
    for i in "${!NAMES[@]}"; do printf '%-8s %-24s %s\n' "${TIERS[$i]}" "${NAMES[$i]}" "${CMDS[$i]}"; done
    exit 0 ;;
  # One command per line, for the audit that holds this list to ci.yml.
  --commands)
    for i in "${!NAMES[@]}"; do printf '%s\n' "${CMDS[$i]}"; done
    exit 0 ;;
  "") ;;
  *) echo "gates: unknown option ${1}; --all, --guards, --console, --list" >&2; exit 2 ;;
esac

failed=()
skipped=()
for i in "${!NAMES[@]}"; do
  case " $WANT " in *" ${TIERS[$i]} "*) ;; *) continue ;; esac
  printf '\033[1m==>\033[0m %s\n' "${NAMES[$i]}"
  need="${NEEDS[$i]}"
  if [[ -n "$need" ]] && ! command -v "$need" >/dev/null 2>&1; then
    printf '    SKIPPED: %s is not installed\n' "$need"
    skipped+=("${NAMES[$i]} (no ${need})")
    continue
  fi
  if eval "${CMDS[$i]}" >/tmp/gate.$$.log 2>&1; then
    printf '    ok\n'
  else
    printf '    FAILED\n'
    tail -12 /tmp/gate.$$.log | sed 's/^/    /'
    failed+=("${NAMES[$i]}")
  fi
  rm -f /tmp/gate.$$.log
done

if [ ${#skipped[@]} -gt 0 ]; then
  printf '\n\033[1mgates: %d skipped:\033[0m %s\n' "${#skipped[@]}" "$(printf '%s; ' "${skipped[@]}")"
fi
if [ ${#failed[@]} -gt 0 ]; then
  printf '\n\033[1mgates: %d failed:\033[0m %s\n' "${#failed[@]}" "$(printf '%s; ' "${failed[@]}")" >&2
  exit 1
fi
if [ ${#skipped[@]} -gt 0 ]; then
  printf '\ngates: every gate RUN in [%s] passed; %d skipped above\n' "$WANT" "${#skipped[@]}"
  exit 0
fi
printf '\ngates: every gate in [%s] passed\n' "$WANT"
