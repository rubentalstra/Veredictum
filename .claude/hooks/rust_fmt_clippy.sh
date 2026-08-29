#!/usr/bin/env bash
# SPDX-FileCopyrightText: Veredictum contributors
# SPDX-License-Identifier: Apache-2.0
# .claude/hooks/rust_fmt_clippy.sh
#
# Ported from FerroEHR at the Veredictum split (FerroEHR#2789), keeping the
# rustfmt step and the comment-style guard. The three other per-edit guards
# (spec citations, default-value style, typed status) arrive with their scripts
# when the code lands.
#
# Claude Code PostToolUse hook (matcher: Write|Edit).
# Formats an edited .rs file with rustfmt. Never blocks on formatting; swallows
# all rustfmt failures (rustfmt failing to parse a draft is expected and fine).
#
# NOTE: this hook never runs clippy. A per-edit `cargo clippy` check-builds the
# crate plus its dependency cone on every file edit, thrashing the cargo cache;
# clippy is a gate the agent runs explicitly, never a per-edit hook.

set -uo pipefail

payload="$(cat)" || true

if command -v jq >/dev/null 2>&1; then
  file_path="$(printf '%s' "$payload" | jq -r '.tool_input.file_path // empty' 2>/dev/null)" || true
else
  file_path="$(printf '%s' "$payload" | sed -n 's/.*"file_path"[[:space:]]*:[[:space:]]*"\([^"]*\)".*/\1/p' | head -n1)"
fi

case "${file_path:-}" in
*.rs) ;;
*) exit 0 ;;
esac
[[ -f "$file_path" ]] || exit 0

rustfmt --edition 2024 "$file_path" >/dev/null 2>&1 || true

# Comment-style guard (.claude/rules/comments.md): block comments, TODO(#N)
# form, NOTE/essay budgets. Exit 2 feeds the findings back as a correction.
repo_root="${CLAUDE_PROJECT_DIR:-$(cd "$(dirname "$0")/../.." && pwd)}"
if [[ -x "$repo_root/scripts/checks/comment-style.sh" ]]; then
  findings="$("$repo_root/scripts/checks/comment-style.sh" --files "$file_path" 2>&1)" || {
    printf '%s\n' "$findings" >&2
    exit 2
  }
fi

exit 0
