#!/usr/bin/env bash
# SPDX-FileCopyrightText: Veredictum contributors
# SPDX-License-Identifier: Apache-2.0
# Changelog structure guard (Keep a Changelog 1.1.0 —
# https://keepachangelog.com/en/1.1.0/).
#
# Ported from FerroEHR at the Veredictum split (FerroEHR#2789). The awk program
# is unchanged; only the header text was adapted.
#
# Fails when any release section of CHANGELOG.md (including [Unreleased])
# violates the Keep a Changelog structure:
#   1. a duplicated `### <Type>` subsection inside one release section
#      (each type appears at most once per section — a new entry merges into the
#      existing subsection, it never appends a second header);
#   2. a subsection header outside the canonical type set
#      (Added / Changed / Deprecated / Removed / Fixed / Security).
#
# A duplicated header is not cosmetic: the release lane publishes the section
# verbatim, so the same release notes ship twice.
#
# Wired into the CI `guards` job, unconditionally. The other half — requiring an
# entry when a user-visible surface changes — is issue #10: it decides
# "user-visible" by matching changed paths, and those paths exist now that the
# code is here.
set -euo pipefail

file="${1:-CHANGELOG.md}"

# awk, not python: this repository ships no Python, and the check is a line scan
# over one file — exactly what awk is for. The quote character arrives as a
# variable because an awk string constant cannot portably escape it.
awk -v path="$file" -v q="'" '
  # A release heading opens a new section and resets what has been seen in it.
  /^## \[/ {
    section = $0
    sub(/^## \[/, "", section)
    sub(/\].*$/, "", section)
    delete seen
    next
  }
  # A subsection heading inside a section: check the type, then the duplicate.
  /^### / && section != "" {
    heading = $0
    sub(/^### /, "", heading)
    sub(/[[:space:]]+$/, "", heading)
    if (heading != "Added" && heading != "Changed" && heading != "Deprecated" \
        && heading != "Removed" && heading != "Fixed" && heading != "Security") {
      errors[++n] = path ":" FNR ": " q "### " heading q " in [" section \
        "] is not a Keep-a-Changelog type (allowed: Added, Changed, " \
        "Deprecated, Fixed, Removed, Security)"
    }
    if (heading in seen) {
      errors[++n] = path ":" FNR ": duplicate " q "### " heading q " in [" \
        section "] — merge the entry into the existing subsection instead of " \
        "adding a second header"
    }
    seen[heading] = 1
  }
  END {
    if (n > 0) {
      print "changelog structure check FAILED:" > "/dev/stderr"
      for (i = 1; i <= n; i++) print "  " errors[i] > "/dev/stderr"
      exit 1
    }
    print "changelog structure OK (" path ")"
  }
' "$file"
