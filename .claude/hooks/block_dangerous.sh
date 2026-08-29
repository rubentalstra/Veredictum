#!/usr/bin/env bash
# SPDX-FileCopyrightText: Veredictum contributors
# SPDX-License-Identifier: Apache-2.0
# .claude/hooks/block_dangerous.sh
#
# Ported from FerroEHR at the Veredictum split (FerroEHR#2789). The two
# FerroEHR-only guards (the plans-directory pointer files and the read-only
# reference/v1 ref) were dropped; the rm and force-push guards are unchanged.
#
# Claude Code PreToolUse hook (matcher: Bash). Blocks destructive commands:
#   - rm -rf / rm -fr (delete specific files, use git rm, or work under /tmp)
#   - force-pushes touching main/master/develop, and bare force-pushes
#
# Reads the tool-call JSON on stdin. Exit 2 blocks; exit 0 allows.

set -euo pipefail

payload="$(cat)"

if command -v jq >/dev/null 2>&1; then
  cmd="$(printf '%s' "$payload" | jq -r '.tool_input.command // empty' 2>/dev/null || true)"
else
  cmd="$payload"
fi
[[ -n "${cmd:-}" ]] || exit 0

# rm with both -r and -f (combined or separate flags), unless scoped to /tmp.
if printf '%s' "$cmd" | grep -qE '(^|[;&|[:space:]])rm[[:space:]]+-[a-zA-Z]*([rR][a-zA-Z]*f|f[a-zA-Z]*[rR])' ||
  printf '%s' "$cmd" | grep -qE '(^|[;&|[:space:]])rm[[:space:]]+(-[a-zA-Z]+[[:space:]]+)*-[a-zA-Z]*[rR][a-zA-Z]*([[:space:]]+-[a-zA-Z]+)*[[:space:]]+-[a-zA-Z]*f'; then
  if ! printf '%s' "$cmd" | grep -qE 'rm[[:space:]]+-[a-zA-Z]+[[:space:]]+"?(/private)?/tmp/'; then
    echo "BLOCKED: 'rm -rf' is not allowed (block_dangerous hook). Delete specific files with 'git rm' or 'rm <file>', or operate under /tmp." >&2
    exit 2
  fi
fi

# Force pushes: never to main/master/develop; bare force-pushes refused too.
# The protected names match only as WHOLE REF WORDS (delimiter-bounded, so
# `refs/heads/main`, `origin main`, `HEAD:main` all hit) — never as raw
# substrings of the command line, which falsely blocked feature branches whose
# names merely CONTAIN a protected name and pushed sessions into
# delete-then-push workarounds that defeat the lease safety this guard exists
# to encourage.
if printf '%s' "$cmd" | grep -qE 'git[[:space:]]+push[^;|&]*(--force([^-]|$)|--force-with-lease|[[:space:]]-f([[:space:]]|$)|[[:space:]]\+[[:alnum:]])'; then
  if printf '%s' "$cmd" | grep -qE '(^|[[:space:]:/+])(main|master|develop)([[:space:]]|$|["'"'"';&|])'; then
    echo "BLOCKED: force-push touching main/master/develop is forbidden (CLAUDE.md hard rule)." >&2
    exit 2
  fi
  if ! printf '%s' "$cmd" | grep -qE '(feat|fix|chore|docs|refactor|perf|test|ci|build|release|claude)/'; then
    echo "BLOCKED: bare force-push refused. Force-push (prefer --force-with-lease) only an explicit conventional-type branch (feat/, fix/, chore/, docs/, refactor/, perf/, test/, ci/, build/, release/)." >&2
    exit 2
  fi
fi

exit 0
