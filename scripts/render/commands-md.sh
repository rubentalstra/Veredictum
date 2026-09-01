#!/usr/bin/env bash
# SPDX-FileCopyrightText: Veredictum contributors
# SPDX-License-Identifier: Apache-2.0
# Render the book's command reference from the binary's own clap help (#466).
#
# The CLI surface has ONE truth, which is clap. The book used to restate it by
# hand — a summary line, a usage block and a flag table per subcommand — and
# `scripts/checks/cli-surface.sh` compared the subcommand NAMES to catch the
# drift. A guard over a transcription is a permanent tax, so the transcription
# is generated instead: this script writes `## <subcommand>` per command with
# `veredictum <subcommand> --help` verbatim underneath it, in clap's own
# declaration order.
#
# What stays hand-written is what clap does not know: the prose explaining why
# a flag is off by default, what a posture canary probes, why a stress report
# is never a conformance record. Those partials live in `website/book/commands`
# — `_intro.md` for the page header and `<subcommand>.md` for one command's
# notes — and are copied in after that command's help block. They sit outside
# `website/book/src` so mdbook renders the generated page and never a partial.
#
# The output is deterministic: clap is built without the `wrap_help` feature,
# so help wraps at a fixed width rather than at a terminal's, and the help text
# reaches this script through a pipe or a file, never a tty, so no colour codes
# enter it either.
#
# Usage:
#   scripts/render/commands-md.sh                 # write the committed page
#   scripts/render/commands-md.sh <out-file>      # write it elsewhere
#     Reads `target/debug/veredictum` when that binary exists (the CI test job
#     has already built it), otherwise `cargo run --quiet --locked`.
set -euo pipefail
cd "$(dirname "$0")/../.."

PARTIALS=website/book/commands
OUT="${1:-website/book/src/commands.md}"

if [[ $# -gt 1 ]]; then
  echo "usage: $0 [<out-file>]" >&2
  exit 2
fi

# Belt and braces beside the pipe: clap colours through anstream, which honours
# NO_COLOR (https://no-color.org/).
export NO_COLOR=1

engine() {
  if [[ -x target/debug/veredictum ]]; then
    target/debug/veredictum "$@"
  else
    cargo run --quiet --locked -- "$@"
  fi
}

# clap prints one subcommand per line under `Commands:`, name first, until the
# blank line before `Options:`. `help` is clap's own built-in: it documents
# nothing this page describes, and `veredictum help --help` prints the same
# text as the page header.
commands="$(engine --help | awk '/^Commands:/ { on = 1; next }
                                 on && /^[[:space:]]*$/ { exit }
                                 on { print $1 }' | grep -v '^help$')"
[[ -n "$commands" ]] || {
  echo "::error::could not read the subcommand list from \`veredictum --help\`" >&2
  exit 1
}

# A partial for a subcommand the binary no longer declares would be dropped in
# silence, which is the rot this generator exists to end, so it is an error.
for partial in "$PARTIALS"/*.md; do
  name="$(basename "$partial" .md)"
  if [[ "$name" == "_intro" ]]; then
    continue
  fi
  grep -qx "$name" <<<"$commands" || {
    echo "::error::${partial} carries notes for \`${name}\`, which the binary does not declare — delete it or restore the subcommand" >&2
    exit 1
  }
done

tmp="$(mktemp)"
trap 'rm -f "$tmp"' EXIT

cat "$PARTIALS/_intro.md" >"$tmp"
while IFS= read -r command; do
  {
    printf '\n## %s\n\n```text\n' "$command"
    engine "$command" --help
    printf '```\n'
  } >>"$tmp"
  if [[ -f "$PARTIALS/${command}.md" ]]; then
    printf '\n' >>"$tmp"
    cat "$PARTIALS/${command}.md" >>"$tmp"
  fi
done <<<"$commands"

mkdir -p "$(dirname "$OUT")"
cp "$tmp" "$OUT"
echo "commands-md: wrote ${OUT} from $(wc -l <<<"$commands" | tr -d ' ') subcommands' own --help."
