#!/usr/bin/env bash
# SPDX-FileCopyrightText: Veredictum contributors
# SPDX-License-Identifier: Apache-2.0
# .claude/hooks/catalogue_attribution_guard.sh
#
# Ported from FerroEHR's cnf_attribution_guard.sh at the Veredictum split
# (FerroEHR#2789), renamed for this repository and re-pointed at the planned
# artifact paths. It stays dormant until the catalogue lands, because the paths
# it matches do not exist yet.
#
# Claude Code PreToolUse hook (matcher: Write|Edit|NotebookEdit). NON-BLOCKING
# reminder fired when a catalogue EXPECTATION artifact is edited
# (artifacts/{schedule,bindings,vocab}/**). It injects the attribution law as
# additionalContext so the agent self-checks — at the exact moment of the edit —
# that it is correcting the catalogue toward the SPEC, not bending an
# expectation to match observed server behaviour.
#
# WHY: the recurring failure mode is "our expectation must be wrong, the server
# is right" → editing the catalogue to make a red row green. This guard keeps
# the spec-oracle discipline (.claude/rules/cnf-triage.md) present precisely
# where that mistake happens. It never blocks (spec-cited catalogue fixes and
# new coverage cases are normal); it only reminds.
#
# Reads the tool-call JSON on stdin; prints hookSpecificOutput.additionalContext
# and exits 0. Corpus data and the ambiguity register are deliberately NOT
# guarded (corpus = data; registers/ = the sanctioned spec-silence path).

set -euo pipefail

payload="$(cat)"

if command -v jq >/dev/null 2>&1; then
  path="$(printf '%s' "$payload" | jq -r '.tool_input.file_path // .tool_input.notebook_path // empty' 2>/dev/null || true)"
else
  path="$payload"
fi
[ -n "${path:-}" ] || exit 0

case "$path" in
  */artifacts/schedule/* | artifacts/schedule/* | \
  */artifacts/bindings/* | artifacts/bindings/* | \
  */artifacts/vocab/*    | artifacts/vocab/*) ;;
  *) exit 0 ;;
esac

msg="Attribution law (.claude/rules/cnf-triage.md): you are editing a catalogue expectation. The vendored released openEHR spec text is the oracle, and this instrument is a suspect on every red row — never presumed correct. This edit is valid ONLY if it moves the catalogue toward the SPEC with a first-hand citation; it must NOT bend an expected status/header/outcome/value to match what a server returned. If the three-way comparison (spec-required vs catalogue-expected vs SUT-observed) shows the SERVER is wrong, the outcome is a defect report to that CDR, NOT a catalogue edit. THE ORACLE ORDER: (1) the ITS-REST DOCS TEXT — QUOTE the decisive sentence that assigns THIS value; it WINS every conflict. (2) Where the docs text is SILENT — not conflicting — the RELEASED OAS grounds the value; cite it AS the OAS (file + element), never as docs text, and never read MORE into it than it states (an optional schema member is not a presence requirement). (3) Silent in BOTH -> artifacts/registers/ambiguities.yaml, never a bent expectation. 'The overview permits X generally' is the RATIONALIZATION TELL, not an assignment — if neither source assigns the value, register it. A server's response is never a source for any tier."

if command -v jq >/dev/null 2>&1; then
  jq -n --arg m "$msg" '{hookSpecificOutput: {hookEventName: "PreToolUse", additionalContext: $m}}'
else
  printf '{"hookSpecificOutput":{"hookEventName":"PreToolUse","additionalContext":"%s"}}' "$msg"
fi
exit 0
