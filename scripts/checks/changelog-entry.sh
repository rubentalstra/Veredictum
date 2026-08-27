#!/usr/bin/env bash
# SPDX-FileCopyrightText: Veredictum contributors
# SPDX-License-Identifier: Apache-2.0
# A change to a user-visible surface must carry a changelog entry.
#
# The sibling `changelog-structure.sh` checks the SHAPE of `CHANGELOG.md`. This
# is the other half: whether an entry was actually added when one was owed.
#
# The path set lives HERE rather than in the workflow, so "user-visible" is one
# declared list a reader can check against `CHANGELOG.md`'s own definition of the
# term, instead of a regular expression buried in a `run:` block that nobody
# diffs. `CHANGELOG.md` says user-visible means the CLI surface, the published
# artifact schemas, verdict semantics, the container image, or anything a party's
# published record depends on. Each entry below is one of those:
#
#   app/veredictum/src/   the runner and the CLI, including verdict semantics
#   app/veredictum-console/src/   the web console the image serves
#   artifacts/      the catalogue — a case, a binding or a register entry
#                   changes what a run reports about somebody's server
#   schemas/        the published JSON Schemas an integrator authors against
#   party/          the committed statements a published record is judged against
#   docker/         the container image, and the ignore file that decides what
#   .dockerignore   its build can even see
#   Cargo.toml      the package identity, the dependency set, and the lint tables
#   Cargo.lock      that decide what compiles
#
# Deliberately NOT in the set, because a changelog entry for them would be noise
# rather than news: the crates' `tests/`, `specs/` (vendored, refreshed only by its own
# script), `scripts/`, `.github/`, `.claude/`, and prose.
#
# Usage:  changelog-entry.sh <base-ref> <head-ref>
#
# Exits 0 when no user-visible path was touched, or when `CHANGELOG.md` gained at
# least one non-blank added line. Exits 1 otherwise, naming the paths that
# triggered it.
set -euo pipefail

cd "$(dirname "$0")/../.."

# One regular expression, assembled from the list above so the list is the thing
# a reader edits.
readonly USER_VISIBLE_PATHS=(
  'app/veredictum/src/'
  'app/veredictum-console/src/'
  'artifacts/'
  'schemas/'
  'party/'
  'docker/'
  '\.dockerignore$'
  'Cargo\.toml$'
  'Cargo\.lock$'
  'app/[^/]+/Cargo\.toml$'
)

base="${1:?usage: changelog-entry.sh <base-ref> <head-ref>}"
head="${2:?usage: changelog-entry.sh <base-ref> <head-ref>}"

pattern="$(printf '%s|' "${USER_VISIBLE_PATHS[@]}")"
pattern="^(${pattern%|})"

changed="$(git diff --name-only "$base" "$head")"

touched="$(printf '%s\n' "$changed" | grep -E "$pattern" || true)"
if [ -z "$touched" ]; then
  echo "changelog-entry: no user-visible surface touched — an entry is not required."
  exit 0
fi

echo "changelog-entry: user-visible paths in this change:"
printf '%s\n' "$touched" | sed 's/^/  /'

# `grep -c`, never `grep -q`: `-q` exits at the first match, `git diff` then dies
# of SIGPIPE, and `pipefail` turns that into a failed pipeline — so a LARGE
# changelog diff would be reported as "no entry at all". `-c` reads the whole
# diff, so the pipeline always exits cleanly.
if [ "$(printf '%s\n' "$changed" | grep -cx 'CHANGELOG.md' || true)" -gt 0 ]; then
  added="$(git diff "$base" "$head" -- CHANGELOG.md | grep -c '^+[^+].*[^[:space:]]' || true)"
  if [ "${added:-0}" -gt 0 ]; then
    echo "changelog-entry: CHANGELOG.md gained ${added} line(s) — OK."
    exit 0
  fi
fi

echo "::error::This change touches a user-visible surface but adds no CHANGELOG.md entry (Keep a Changelog 1.1.0). Add one under the Unreleased heading, or apply the 'no-changelog' label if the change is genuinely invisible." >&2
exit 1
