#!/usr/bin/env bash
# SPDX-FileCopyrightText: Veredictum contributors
# SPDX-License-Identifier: Apache-2.0
# Vendor the official openEHR ADL 2 archetype libraries — the ADL 2.4 half of
# the two-dialect archetype corpus.
#
# WHY THIS SCRIPT EXISTS (verified 2026-08-01 — do not re-derive):
# the openEHR CKM publishes ADL **1.4** only. `GET /archetypes/{cid}/adl`
# answers `adl_version=1.4`; `/adl2`, `/adl14`, `/adl2.4`, `/opt2` and
# `/source` all 404; `?format=ADL2` / `?version=2` are silently ignored and
# return byte-identical 1.4 text. So the ADL 1.4 corpus comes from the live
# CKM (`scripts/vendor/ckm-archetypes.sh`) and the ADL 2 corpus comes from
# here — the openEHR ADL archetype repository, pinned by commit.
#
# NEVER fill the ADL 2 side by running our own ADL 1.4->2 converter
# (`openehr_adl::adl14::convert`) over CKM output: the converter has no spec
# basis (it is our own design) and feeding it its own output back as the
# oracle would test it against itself.
#
# Source: https://github.com/openEHR/adl-archetypes ("ADL test, reference and
# example archetypes"). One tree is vendored here:
#
#   Reference/CKM_2013_12_09/  -> artifacts/corpus/archetypes/adl2
#     A CKM export carrying BOTH dialects of the same archetypes side by side
#     (`*.adl` = 1.4, `*.adls` = ADL 2). The PAIRING is the point: it is
#     upstream's own 1.4/2 correspondence for real clinical archetypes, so it
#     grounds the ADL 2 wire cases of the DEFINITION API and gives the 1.4->2
#     conversion an INDEPENDENT reference (upstream's conversion, not ours).
#
# Licensing: the repository carries no top-level LICENSE file (verified
# 2026-08-04); the content is openEHR Foundation test/reference material, and
# individual archetypes carry their own `licence` field (predominantly
# CC-BY-SA 3.0 where stated). Recorded as-is, provenance retained.
#
# Usage:
#   scripts/vendor/adl2-archetypes.sh            # vendor at the pin below
#   ADL2_PIN=<sha> scripts/vendor/adl2-archetypes.sh
#   scripts/vendor/adl2-archetypes.sh --check    # report drift, write nothing
set -Eeuo pipefail

REPO="openEHR/adl-archetypes"
# The pin. Bump deliberately: a bump is a corpus change, so re-run the corpus
# gates (`cargo nextest run`) and the artifact validator
# (`cargo run -- validate --root artifacts --specs specs/openehr`) in the same
# commit.
PIN="${ADL2_PIN:-093c77ea003742b9540e3dd377d615e2b26f2996}"

CKM_PAIRS_DEST="artifacts/corpus/archetypes/adl2"

CHECK=0
[[ "${1:-}" == "--check" ]] && CHECK=1

WORK=$(mktemp -d)
trap 'rm -rf "$WORK"' EXIT
STAMP=$(date -u +%Y-%m-%dT%H:%M:%SZ)

echo "==> fetching $REPO @ ${PIN:0:12}"
curl -fsSL --proto '=https' --proto-redir '=https' "https://codeload.github.com/$REPO/tar.gz/$PIN" -o "$WORK/repo.tar.gz"
mkdir -p "$WORK/src"
tar -xzf "$WORK/repo.tar.gz" -C "$WORK/src" --strip-components=1

tree=Reference/CKM_2013_12_09
[[ -d "$WORK/src/$tree" ]] || {
  echo "::error::$tree is absent at ${PIN:0:12} — the upstream layout moved" >&2
  exit 1
}

sync_tree() {
  local src=$1 dest=$2
  if [[ $CHECK == 1 ]]; then
    echo "==> check $dest"
    diff -rq "$src" "$dest" || true
    return
  fi
  echo "==> vendor $dest"
  mkdir -p "$dest"
  # --delete so a file upstream removed disappears here too; the tree is
  # vendored verbatim and never hand-edited
  rsync -a --delete "$src/" "$dest/"
}

sync_tree "$WORK/src/Reference/CKM_2013_12_09" "$CKM_PAIRS_DEST/ckm-2013-12-09"

[[ $CHECK == 1 ]] && exit 0

# ── provenance for the vendored tree ────────────────────────────────────────
# The provenance record, in shell: the counts are `find | wc -l`, the pairing is a
# `comm` over two sorted HRID lists, and the class table is `cut | sort | uniq -c`.
{
  root="$CKM_PAIRS_DEST/ckm-2013-12-09"
  # HRID = the name up to `.v<version>`, so an ADL 1.4 file and its ADL 2 twin
  # reduce to the same key.
  hrids() { find "$root" -name "$1" -print | sed 's|.*/||; s|\.v[0-9].*$||' | sort -u; }
  adl_count=$(find "$root" -name '*.adl' -print | wc -l | tr -d ' ')
  adls_count=$(find "$root" -name '*.adls' -print | wc -l | tr -d ' ')
  paired_count=$(comm -12 <(hrids '*.adl') <(hrids '*.adls') | wc -l | tr -d ' ')
  # The pairing is not total, so the record carries the shortfall in both
  # directions rather than leaving it to be inferred from the three counts.
  # An HRID is unique within each dialect, so these subtractions are exact.
  adls_only=$((adls_count - paired_count))
  adl_only=$((adl_count - paired_count))

  {
    printf '# ADL 2 archetype pack (with ADL 1.4 twins) — provenance\n\n'
    printf 'Vendored verbatim from `https://github.com/%s`\n' "$REPO"
    printf '(`Reference/CKM_2013_12_09/`) at commit `%s` by\n' "$PIN"
    printf '`scripts/vendor/adl2-archetypes.sh` on %s.\n\n' "$STAMP"
    cat <<'PROSE'
Upstream describes the tree as archetypes exported from the Clinical
Knowledge Manager (export time Mon Dec 09 15:42:23 CET 2013).

## Why this source and not CKM

The live openEHR CKM publishes **ADL 1.4 only** — `/archetypes/{cid}/adl`
returns `adl_version=1.4` and there is no ADL 2 export endpoint
(`/adl2`, `/opt2` 404; `?format=ADL2` is ignored). The ADL 1.4 corpus is
therefore vendored live (`corpus/archetypes/ckm/`, ADL 1.4) and the ADL 2
corpus comes from this pinned upstream library.

The ADL 2 side is NEVER produced by running our own ADL 1.4->2 converter
over CKM output: that converter has no spec basis (our own design) and
would then be validated against its own output.

## Licensing

The upstream repository carries no top-level LICENSE file; individual
archetypes carry their own `licence` metadata (predominantly CC-BY-SA
3.0 where stated — see the individual file). openEHR Foundation
test/reference material, vendored verbatim with metadata retained;
root reference copy: `LICENSE-CC-BY-SA-3.0`.

## Contents

PROSE
    printf -- '- ADL 2 archetypes (`*.adls`): **%s**\n' "$adls_count"
    printf -- '- ADL 1.4 twins (`*.adl`): **%s**\n' "$adl_count"
    printf -- '- archetypes present in BOTH dialects: **%s**\n\n' "$paired_count"
    cat <<'PROSE'
The dual-dialect pairing is the value here: the same clinical archetype
in 1.4 and in 2, as published upstream, which is an INDEPENDENT
reference for the conversion path.

| RM class | ADL 2 files |
|---|---|
PROSE
    # RM class = the leading dotted segment of the file name; ordered by count
    # descending then name, as the previous record was.
    find "$root" -name '*.adls' -print \
      | sed 's|.*/||; s|\..*$||' \
      | sort | uniq -c | sort -k1,1nr -k2,2 \
      | while read -r count cls; do printf '| %s | %s |\n' "$cls" "$count"; done
    printf '\n## What exercises this pack\n\n'
    cat <<'PROSE'
`tests/it/corpus_packs.rs` reads every file in the tree and pins
what this instrument can check first-hand: the two dialect counts
above, the `adl_version` each file declares, the archetype id
inside each file against the name it is stored under, and the
pairing itself. This instrument ships no ADL parser, so nothing
here reads an archetype body.

The pairing is not total, and the gate pins the exact shortfall:
PROSE
    printf -- '\n- ADL 2 files with no ADL 1.4 twin: **%s**\n' "$adls_only"
    printf -- '- ADL 1.4 files with no ADL 2 twin: **%s**\n\n' "$adl_only"
    cat <<'PROSE'
A wire battery driving the pairs through the DEFINITION API would
exercise the pack further. That is catalogue work: no case sources
a file from this tree today.
PROSE
    printf '\nNever hand-edit a vendored fixture; re-run this script and bump the pin.\n'
  } > "$CKM_PAIRS_DEST/PROVENANCE.md"

  printf '==> %s ADL 2 + %s ADL 1.4 files (%s paired) → %s\n' \
    "$adls_count" "$adl_count" "$paired_count" "$CKM_PAIRS_DEST"
}

git status --short "$CKM_PAIRS_DEST" | head -20
