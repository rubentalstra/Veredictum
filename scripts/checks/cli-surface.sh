#!/usr/bin/env bash
# SPDX-FileCopyrightText: Veredictum contributors
# SPDX-License-Identifier: Apache-2.0
# The CLI surface has one truth and three hand-maintained copies (#76).
#
# clap's own `--help` is the surface. Three documents restate it, and nothing
# held them together: the binary's header table in
# `app/veredictum/src/bin/veredictum.rs`, the root `CLAUDE.md` parenthetical
# that calls itself "the instrument's own canonical CLI table", and the book's
# command reference at `website/book/src/commands.md`. A subcommand added to
# clap and forgotten in a copy leaves an operator reading a surface that does
# not exist; a subcommand removed leaves them invoking one that is gone. This
# guard derives the subcommand set from the binary and fails on either
# direction, per copy.
#
# It compares NAMES, not flags: a flag table is prose the binary's own
# `veredictum <command> --help` settles, and holding every flag row to clap
# would make the guard a second parser. The set of subcommands is the part a
# reader navigates by, and the part that silently rots.
#
# Usage: scripts/checks/cli-surface.sh
#   Reads `target/debug/veredictum --help` when that binary exists (the CI test
#   job has already built it), otherwise `cargo run --quiet --locked -- --help`.
set -euo pipefail
cd "$(dirname "$0")/../.."

BIN_DOC=app/veredictum/src/bin/veredictum.rs
BOOK=website/book/src/commands.md
ROOT_DOC=CLAUDE.md

if [[ -x target/debug/veredictum ]]; then
  help="$(target/debug/veredictum --help)"
else
  help="$(cargo run --quiet --locked -- --help)"
fi

# clap prints one subcommand per line under `Commands:`, name first, until the
# blank line before `Options:`. `help` is clap's own built-in and no document
# describes it.
declared="$(awk '/^Commands:/ { on = 1; next }
                 on && /^[[:space:]]*$/ { exit }
                 on { print $1 }' <<<"$help" | grep -v '^help$' | sort)"
[[ -n "$declared" ]] || { echo "::error::could not read the subcommand list from --help" >&2; exit 1; }

# The header table: every line opening a subcommand entry.
header="$(grep -oE '^//! veredictum [a-z][a-z-]*' "$BIN_DOC" | awk '{ print $3 }' | sort -u)"

# The book: one `## <subcommand>` section per command.
book="$(grep -oE '^## [a-z][a-z-]*$' "$BOOK" | awk '{ print $2 }' | sort -u)"

# The root document's parenthetical, which names itself the canonical table.
# Read across newlines, since the list wraps.
root="$(tr '\n' ' ' <"$ROOT_DOC" \
  | grep -oE 'canonical CLI table \([^)]*\)' \
  | grep -oE '`[a-z][a-z-]*`' | tr -d '`' | sort -u)"

failures=0
compare() { # $1 = the copy's list, $2 = label, $3 = where to fix it
  local have="$1" label="$2" where="$3" name
  while IFS= read -r name; do
    [[ -n "$name" ]] || continue
    grep -qx "$name" <<<"$have" || {
      echo "::error::${where} does not describe \`${name}\`, which the binary declares — add it to the ${label}" >&2
      failures=$((failures + 1))
    }
  done <<<"$declared"
  while IFS= read -r name; do
    [[ -n "$name" ]] || continue
    grep -qx "$name" <<<"$declared" || {
      echo "::error::${where} describes \`${name}\`, which the binary does not declare — remove it from the ${label}" >&2
      failures=$((failures + 1))
    }
  done <<<"$have"
}

compare "$header" "header table" "$BIN_DOC"
compare "$book" "command reference" "$BOOK"
compare "$root" "canonical CLI table" "$ROOT_DOC"

if [[ "$failures" -gt 0 ]]; then
  exit 1
fi
echo "cli-surface: all three copies describe the $(wc -l <<<"$declared" | tr -d ' ') declared subcommands — OK."
