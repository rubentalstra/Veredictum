#!/usr/bin/env bash
# SPDX-FileCopyrightText: Veredictum contributors
# SPDX-License-Identifier: Apache-2.0
# .claude/hooks/no_attribution_guard.sh
#
# Ported from FerroEHR at the Veredictum split (FerroEHR#2789); logic unchanged.
#
# Claude Code PreToolUse hook (matcher: Bash).
# It blocks a `git commit` (or PR-creating command) BEFORE it runs if the
# command text carries any AI/Claude attribution, so the attribution is removed
# rather than rewritten after the fact.
#
# Reads the tool-call JSON on stdin. Exit 2 blocks the tool call and returns
# the stderr text to Claude. Exit 0 allows it.

set -euo pipefail

payload="$(cat)"

# Only concern ourselves with git commit / PR creation commands.
if ! printf '%s' "$payload" | grep -qiE 'git[[:space:]]+commit|gh[[:space:]]+pr[[:space:]]+create|git[[:space:]]+push'; then
  exit 0
fi

# Attribution tokens we refuse to let through. The generated-with/by shape
# requires claude ADJACENT to the phrase (allowing markdown-link punctuation),
# because a same-line `.*` blocked "regenerated with `cargo …` … CLAUDE.md"
# as attribution (#527); the claude-code alternative catches that phrase
# anywhere on its own.
if printf '%s' "$payload" | grep -qiE 'co-authored-by:.*(claude|anthropic|\[bot\])|generated[[:space:]]+(with|by)[[:space:]]*[^a-zA-Z0-9[:space:]]{0,2}[[:space:]]*claude|claude[[:space:]]+code|🤖|assisted-by:.*claude|anthropic[[:space:]]+claude'; then
  echo "BLOCKED: this commit/PR command contains AI/Claude attribution (Co-authored-by, 'Generated with Claude', 🤖, etc.). Remove all such lines from the message/description and retry. Commit and PR text must describe only the change. This is a hard project rule." >&2
  exit 2
fi

exit 0
