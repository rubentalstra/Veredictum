#!/usr/bin/env bash
# SPDX-FileCopyrightText: Veredictum contributors
# SPDX-License-Identifier: Apache-2.0
# Comment-style guard (.claude/rules/comments.md — RFC 505 / RFC 1574).
#
# Ported from FerroEHR at the Veredictum split (FerroEHR#2789). The awk program
# is unchanged; only the file globs (this repository has one crate, not three
# workspace directories) and the header text were adapted.
#
# Checks HAND-WRITTEN .rs files (files carrying the `@generated` marker on their
# first line are skipped — their comments are fixed in whatever emits them):
#
#   1. block comments      `/* … */` is banned; line comments only (RFC 505).
#   2. TODO form           every TODO names its issue: `TODO(#NNNN):`.
#   3. marker vocabulary   only TODO(#N)/NOTE/SAFETY are sanctioned; FIXME,
#                          HACK, XXX, WIP and the (port) forms all fail.
#   4. NOTE budget         a `// NOTE:` block is a citation + one sentence —
#                          at most $NOTE_MAX physical comment lines.
#   5. essay budget        a plain `//` comment run is at most $RUN_MAX lines;
#                          longer prose belongs in doc comments, the PR
#                          description, or the tracker — not in code.
#   6. internal citations  no `.claude/` path anywhere in a hand-written .rs
#                          file: code cites the vendored spec text or official
#                          external documentation, never an internal document
#                          (root CLAUDE.md rule 11). Whole-file rather than
#                          comment-only, because an `#[expect(reason = "…")]`
#                          string is read as justification exactly like a
#                          comment.
#   7. internal markdown   the same rule for every OTHER in-repo markdown
#                          document: an `artifacts/`, `party/`, `scripts/`,
#                          `website/`, `schemas/`, `fuzz/`, `app/` or
#                          `verification-pack/` path ending in `.md`, and the
#                          root documents by name (README, CLAUDE,
#                          ARCHITECTURE, CONTRIBUTING, CHANGELOG). Adjudicated
#                          2026-08-27 (#125): a corpus-local README is NOT a
#                          sanctioned exception — it moves and dies like any
#                          other internal document, so a comment grounds on
#                          the material itself. COMMENT LINES ONLY, unlike
#                          rule 6: the vendored spec text is full of `.md`
#                          documents that ARE legitimate citations, and a
#                          string the code EMITS may legitimately name a
#                          markdown file the tool writes.
#
# Usage:
#   scripts/checks/comment-style.sh --all               # whole tree
#   scripts/checks/comment-style.sh --diff <base> [head]  # changed files only
#   scripts/checks/comment-style.sh --files <f.rs>...   # named files (hook)
#   scripts/checks/comment-style.sh --self-test         # seeded-violation proof
#
# Exit 0 = clean, 1 = violations (listed as file:line: message), 2 = usage.

set -euo pipefail

NOTE_MAX=3
RUN_MAX=8

# The in-repo markdown documents a comment may not cite (check 7 above). The
# vendored spec trees are deliberately absent: `specs/**` IS the oracle, and an
# ITS-REST citation names a `.md` document there.
INTERNAL_DOC='(artifacts|party|scripts|website|schemas|fuzz|app|verification-pack)/[^[:space:]]*[.]md'
INTERNAL_DOC="$INTERNAL_DOC"'|(^|[^[:alnum:]_./-])(README|CLAUDE|ARCHITECTURE|CONTRIBUTING|CHANGELOG)[.]md'

cd "$(dirname "$0")/../.."

mode="${1:---all}"
files=()
case "$mode" in
--all)
  # `git ls-files` reads the index, so a file deleted from the worktree but
  # not yet staged would still be listed — skip it rather than letting awk
  # fail on a missing path.
  while IFS= read -r f; do [[ -f "$f" ]] && files+=("$f"); done \
    < <(git ls-files '*.rs')
  ;;
--diff)
  base="${2:?usage: --diff <base> [head]}"
  head="${3:-HEAD}"
  while IFS= read -r f; do
    [[ -f "$f" ]] && files+=("$f")
  done < <(git diff --name-only "$base" "$head" -- '*.rs')
  ;;
--files)
  shift
  for f in "$@"; do
    case "$f" in
    *.rs) [[ -f "$f" ]] && files+=("$f") ;;
    *) ;;
    esac
  done
  ;;
--self-test)
  # The seeded-violation proof for check 7: a guard nobody has watched fire is
  # a guard nobody knows the shape of. Each seeded line below MUST be reported,
  # and each clean line MUST NOT be.
  tmp="$(mktemp -d)"
  trap 'rm -rf "$tmp"' EXIT
  seeded="$tmp/seeded.rs"
  {
    echo '// the committed keypair (`artifacts/corpus/keys/README.md` records it)'
    echo '/// the crate CLAUDE.md doctrine'
    echo '//! see website/book/src/commands.md for the surface'
    echo '/// ITS-REST `docs/overview/Requests_and_responses.md` §HTTP status codes'
    echo 'const REPORT: &str = "coverage-report.md";'
    echo 'const PROVENANCE: &str = "artifacts/corpus/archetypes/adl2/PROVENANCE.md";'
  } >"$seeded"
  out="$("$0" --files "$seeded" 2>/dev/null || true)"
  fail=0
  for want in 1 2 3; do
    grep -q "seeded.rs:$want: internal-document citation" <<<"$out" || {
      echo "self-test: line $want was NOT reported — the guard does not fire" >&2
      fail=1
    }
  done
  for unwanted in 4 5 6; do
    if grep -q "seeded.rs:$unwanted: internal-document citation" <<<"$out"; then
      echo "self-test: line $unwanted was reported — a legitimate citation is refused" >&2
      fail=1
    fi
  done
  [[ "$fail" -eq 0 ]] || exit 1
  echo "comment-style: self-test OK (3 seeded violations caught, 3 legitimate lines passed)."
  exit 0
  ;;
*)
  echo "usage: $0 [--all | --diff <base> [head] | --files <f.rs>... | --self-test]" >&2
  exit 2
  ;;
esac

[[ "${#files[@]}" -eq 0 ]] && {
  echo "comment-style: no files to check — OK."
  exit 0
}

fail=0
for f in "${files[@]}"; do
  # An emitter writes its banner as the FIRST line of a generated file, so the
  # skip anchors there. Matching the marker anywhere would let a hand-written
  # file exempt itself by merely mentioning it in prose.
  head -n 1 "$f" 2>/dev/null | grep -q '^// @generated' && continue
  out="$(awk -v NOTE_MAX="$NOTE_MAX" -v RUN_MAX="$RUN_MAX" \
    -v INTERNAL_DOC="$INTERNAL_DOC" '
    function flush_note() {
      if (note_len > NOTE_MAX)
        printf ":%d: NOTE block is %d lines (max %d) — a NOTE is a citation + one sentence; move the essay to the PR/issue\n", note_start, note_len, NOTE_MAX
      note_len = 0
    }
    function flush_run() {
      if (run_len > RUN_MAX)
        printf ":%d: comment run is %d lines (max %d) — long prose belongs in doc comments or on the PR/issue, not in code\n", run_start, run_len, RUN_MAX
      run_len = 0
    }
    function flush_doc_note() {
      if (doc_note_len > RUN_MAX)
        printf ":%d: NOTE paragraph in a doc comment is %d lines (max %d) — an adjudication essay lives on the PR/issue, not in rustdoc\n", doc_note_start, doc_note_len, RUN_MAX
      doc_note_len = 0
    }
    {
      line = $0
      sub(/^[[:space:]]+/, "", line)
      is_doc  = (line ~ /^\/\/[\/!]/)
      is_line = (!is_doc && line ~ /^\/\//)

      # 1. block comments on code lines: a `/*` at line start or after
      # whitespace, before any string literal on the line. The position
      # guard skips glob/media-type/URL text inside multi-line string
      # literals (`*/*`, `dir/*.xml`, `://***@`), which an earlier-opened
      # string puts on a quote-less line.
      if (!is_doc && !is_line) {
        bc = index($0, "/*"); q = index($0, "\"")
        if (bc > 0 && (q == 0 || bc < q) \
            && (bc == 1 || substr($0, bc - 1, 1) ~ /[[:space:]]/))
          printf ":%d: block comment — use line comments (`//`) only (RFC 505)\n", NR
      }

      # 2 + 3. marker forms. TODO is judged only as the LEADING marker of a
      # comment — prose or a verbatim spec quotation mentioning the word is
      # not a marker.
      if (is_doc || is_line) {
        if (line ~ /^\/\/+[!\/]?[[:space:]]*TODO/ && line !~ /TODO\(#[0-9]+\):/)
          printf ":%d: TODO without an issue reference — the only sanctioned form is `TODO(#NNNN):`\n", NR
        if (line ~ /PORT NOTE|PORT STATUS|TODO\(port\)|PERF\(port\)|NOTE\(port\)|FIXME|HACK:|(^|[^A-Za-z0-9_])XXX([^A-Za-z0-9_]|$)|\/\/[[:space:]]*WIP[: ]/)
          printf ":%d: unsanctioned comment marker — the only forms are TODO(#NNNN): / NOTE: / SAFETY:\n", NR

        # 7. in-repo markdown citations, comment lines only. A vendored spec
        # document under `specs/**` is the oracle and never matches.
        if (line ~ INTERNAL_DOC)
          printf ":%d: internal-document citation — cite the vendored openEHR spec text or official external documentation, never an in-repo markdown file (CLAUDE.md rule 11)\n", NR
      }

      # 4 + 5. NOTE / plain-run budgets
      if (is_line) {
        if (line ~ /^\/\/[[:space:]]*NOTE/) {
          flush_note(); flush_run()
          note_start = NR; note_len = 1
        } else if (note_len > 0) {
          note_len++
        } else {
          if (run_len == 0) run_start = NR
          run_len++
        }
        next
      }
      # 6. the NOTE budget inside doc comments (`/// NOTE:` / `//! NOTE:`) —
      # a doc-relocated essay is the same essay. The paragraph ends at a
      # blank doc line, per rustdoc paragraph semantics.
      if (is_doc) {
        if (line ~ /^\/\/[\/!][[:space:]]*NOTE/) {
          flush_doc_note()
          doc_note_start = NR; doc_note_len = 1
        } else if (doc_note_len > 0) {
          # The paragraph ends at a blank doc line (rustdoc paragraph
          # semantics) or at the next list item (a NOTE inside a list does
          # not swallow its sibling items).
          if (line ~ /^\/\/[\/!][[:space:]]*$/ \
              || line ~ /^\/\/[\/!][[:space:]]+([-*][[:space:]]|[0-9]+\.[[:space:]])/ \
              || line ~ /^\/\/[\/!][[:space:]]*#/)
            flush_doc_note()
          else doc_note_len++
        }
        flush_note(); flush_run()
        next
      }
      flush_note(); flush_run(); flush_doc_note()
    }
    END { flush_note(); flush_run(); flush_doc_note() }
  ' "$f")"
  if [[ -n "$out" ]]; then
    printf '%s\n' "$out" | sed "s|^|$f|"
    fail=1
  fi

  internal="$(grep -n '\.claude/' "$f" || true)"
  if [[ -n "$internal" ]]; then
    printf '%s\n' "$internal" \
      | sed -E "s|^([0-9]+):.*|$f:\1: internal-document citation — cite the vendored openEHR spec text or official external documentation, never a \.claude/ path (CLAUDE.md rule 11)|"
    fail=1
  fi
done

if [[ "$fail" -ne 0 ]]; then
  echo "comment-style: violations found (rules: .claude/rules/comments.md)." >&2
  exit 1
fi
echo "comment-style: OK (${#files[@]} files)."
