#!/usr/bin/env bash
# SPDX-FileCopyrightText: Veredictum contributors
# SPDX-License-Identifier: Apache-2.0
# Populate fuzz/seeds/<target>/ from material already committed in this
# repository: the catalogue, the party declarations, and the vendored packs.
#
# Two shapes of seed. A DOCUMENT seed is symlinked, never copied — the artifact
# tree is large and each file is provenance-stamped where it lives. A FRAGMENT
# seed is a short string harvested out of those same files (a `${…}` reference,
# a citation, a decision-table cell), written out one per file because libFuzzer
# takes one input per file and there is nothing to link to.
#
# Selection is size-bounded and deterministic — sorted, then capped. libFuzzer
# re-reads every seed on each run and derives its default input length from the
# largest one, so a handful of large documents would sink the execution rate for
# no extra coverage.
#
# The one input that is not derived from the tree is fuzz/regressions/<target>/,
# the tracked artifacts of past findings, wired in after each target's wipe.
#
# Usage: fuzz/seeds.sh            (all targets)
#        fuzz/seeds.sh <target>…  (named targets only)
#
# Reference: the cargo-fuzz book, "Corpora"
# <https://rust-fuzz.github.io/book/cargo-fuzz/tutorial.html>.
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
seeds_root="$repo_root/fuzz/seeds"
regressions_root="$repo_root/fuzz/regressions"

# Every seed source must exist: a renamed directory silently producing an empty
# seed set is the failure mode this guards.
require_dir() {
  if [[ ! -d "$repo_root/$1" ]]; then
    echo "seeds.sh: missing source directory: $1" >&2
    exit 1
  fi
}

# link_from <target> <source-dir> <max-size> <cap> <find-name-pattern>…
#
# Symlinks up to <cap> files under <source-dir> smaller than <max-size> (a find
# -size argument, e.g. 32k) into fuzz/seeds/<target>/, naming each after its
# path so two sources cannot collide.
link_from() {
  local target="$1" source="$2" max_size="$3" cap="$4"
  shift 4
  require_dir "$source"

  local find_args=()
  local pattern
  for pattern in "$@"; do
    find_args+=(-o -name "$pattern")
  done

  local dest="$seeds_root/$target"
  mkdir -p "$dest"

  local linked=0 path relative name
  while IFS= read -r path; do
    relative="${path#"$repo_root"/}"
    name="${relative//\//_}"
    ln -sf "../../../$relative" "$dest/$name"
    linked=$((linked + 1))
  done < <(
    find "$repo_root/$source" -type f \( -false "${find_args[@]}" \) \
      -size "-$max_size" | LC_ALL=C sort | head -n "$cap"
  )
  echo "  $target <- $source ($linked files)"
}

# harvest <target> <label> <prefix> <cap> — read one fragment per line from
# stdin, deduplicate, cap, and write each to its own seed file.
harvest() {
  local target="$1" label="$2" prefix="$3" cap="$4"
  local dest="$seeds_root/$target"
  mkdir -p "$dest"

  local written=0 line
  while IFS= read -r line; do
    written=$((written + 1))
    printf '%s' "$line" > "$(printf '%s/%s_%04d' "$dest" "$prefix" "$written")"
  done < <(LC_ALL=C sort -u | head -n "$cap")

  if [[ "$written" -eq 0 ]]; then
    echo "seeds.sh: harvest '$label' produced nothing — the source moved" >&2
    exit 1
  fi
  echo "  $target <- $label ($written fragments)"
}

# ── The reference and identifier grammars ──────────────────────────────────
# The `${…}` references the catalogue actually authors, the capture sources the
# bindings read, and the identifier spaces: SM operation anchors, case ids,
# corpus keys. Written rather than linked because each is a short string inside
# a YAML file, not a file of its own.
seed_reference_grammar() {
  require_dir artifacts
  grep -rhoE '\$\{[^}]*\}' "$repo_root/artifacts" --include='*.yaml' \
    | harvest reference_grammar "catalogue \${…} references" ref 400
  grep -rhoE '\bI_[A-Z0-9_]+\.[a-z0-9_]+' "$repo_root/artifacts" --include='*.yaml' \
    | harvest reference_grammar "SM operation anchors" smop 200
  grep -rhoE '^id: [^ ]+$' "$repo_root/artifacts/schedule" --include='*.yaml' \
    | sed -E 's/^id: //' \
    | harvest reference_grammar "case ids" caseid 200
  grep -rhoE '\bcnf\.[a-z0-9._-]+' "$repo_root/artifacts" --include='*.yaml' \
    | harvest reference_grammar "corpus keys" key 200

  # The degenerate separator cases, which no authored artifact carries: the
  # point of a written seed is to hand libFuzzer the SHAPE so its mutations
  # land inside the grammar instead of rediscovering that `::` and `#` matter.
  local dest="$seeds_root/reference_grammar"
  printf '%s' '${ds:cnf.set.bp-10#magnitude_ge_140_by_uid}' > "$dest/shape_ds_view"
  printf '%s' '${time:between(t1,t2)}' > "$dest/shape_time_between"
  printf '%s' '${recipe:ehr_status(row)}' > "$dest/shape_recipe"
  printf '%s' '${offset?}::${row.x}' > "$dest/shape_two_refs"
  printf '%s' 'created.version_uids[]' > "$dest/shape_list_capture"
  printf '%s' 'header ETag last-segment' > "$dest/shape_wire_from"
  printf '%s' 'pattern:^W/"[^"]+"$' > "$dest/shape_header_pattern"
  printf '%s' '${' > "$dest/shape_unterminated"
  echo "  reference_grammar <- written grammar shapes (8 fragments)"
}

# ── The decision-table literal grammar ─────────────────────────────────────
# The content chapters carry the literals: range, list, term code, ordinal and
# scale tuples, quantities, and the `violates` entries beside them.
seed_literal_grammar() {
  require_dir artifacts/schedule/content
  grep -rhoE '"[^"]*(\.\.|::|\|)[^"]*"' "$repo_root/artifacts/schedule" --include='*.yaml' \
    | sed -E 's/^"//; s/"$//' \
    | harvest literal_grammar "decision-table literals" lit 400
  grep -rhoE '(rm_schema|rm_invariant|iso8601|constraint)(\([^)]*\))?: [^"]+' \
    "$repo_root/artifacts/schedule" --include='*.yaml' \
    | harvest literal_grammar "violation entries" viol 200

  local dest="$seeds_root/literal_grammar"
  printf '%s' '[cm 5.0..10.0, m]' > "$dest/shape_unit_range_list"
  printf '%s' '1|[local::at0005]' > "$dest/shape_ordinal"
  printf '%s' '1.5|[local::at0005]' > "$dest/shape_scale"
  printf '%s' 'openehr::122 (length)' > "$dest/shape_term_rubric"
  printf '%s' '2000-01-01T00:00:00.0..2010-12-31T23:59:59.999999' > "$dest/shape_iso_range"
  printf '%s' '100 mg' > "$dest/shape_quantity"
  printf '%s' '[[[[1]]]]' > "$dest/shape_nested_list"
  echo "  literal_grammar <- written grammar shapes (7 fragments)"
}

# ── The citation reader ────────────────────────────────────────────────────
# Every `spec_refs` entry the catalogue carries, which is the largest hand-typed
# corpus in the tree and the one whose clause boundaries have already leaked.
seed_citation() {
  require_dir artifacts
  grep -rhoE '"(CNF|RM|BASE|AM|QUERY|TERM|LANG|SM|ITS-REST|ITS-JSON|ITS-XML) [^"]+"' \
    "$repo_root/artifacts" --include='*.yaml' \
    | sed -E 's/^"//; s/"$//' \
    | harvest citation "catalogue citations" cite 600

  local dest="$seeds_root/citation"
  printf '%s' 'ITS-REST OAS operations/directory_{update,delete}.yaml §requestBody' \
    > "$dest/shape_brace_group"
  printf '%s' 'RM common master06 §The "Virtual Version Tree"; BASE base_types master05 §Composite Identifiers' \
    > "$dest/shape_two_clauses"
  printf '%s' 'SM openehr_platform I_EHR_SERVICE.create_ehr §create_ehr — the operation anchor' \
    > "$dest/shape_gloss"
  echo "  citation <- written citation shapes (3 fragments)"
}

# ── The artifact front-end ─────────────────────────────────────────────────
# Real case cores, the document family the harness carries end to end.
seed_artifact_yaml() {
  link_from artifact_yaml artifacts/schedule 32k 400 '*.yaml'
  link_from artifact_yaml artifacts/vocab 32k 40 '*.yaml'
  link_from artifact_yaml artifacts/bindings 32k 100 '*.yaml'
}

# ── The party documents ────────────────────────────────────────────────────
# The committed statements and IXIT declarations, plus the committed example
# results document (#48, examples/results.example.json — generated by the
# crate's own machinery via `cargo run --example make_example_results`), so
# the results half of the harness starts from a real, schema-valid record
# rather than from mutations of a statement.
seed_party_document() {
  link_from party_document party 256k 40 '*.json'
  link_from party_document app/veredictum/examples 256k 5 '*.json'
}

# ── The HDR histogram V2 decoder ───────────────────────────────────────────
# The one committed real encoding — the example results document's embedded
# histogram (#48), decoded from its base64 field — plus written headers: the
# V2 cookie (0x1C849303, HdrHistogram's own encoding constant) followed by
# plausible and implausible bytes. The harness base64-encodes whatever it is
# given, so a seed is RAW BYTES.
seed_hdr_v2() {
  local dest="$seeds_root/hdr_v2"
  mkdir -p "$dest"
  local example="$repo_root/app/veredictum/examples/results.example.json"
  if [[ ! -f "$example" ]]; then
    echo "seeds.sh: missing $example (cargo run --example make_example_results)" >&2
    exit 1
  fi
  jq -r '.measurements[0].operations[0].hdr_v2_base64' "$example" \
    | base64 -d > "$dest/example_real_encoding"
  # cookie only
  printf '\x1c\x84\x93\x03' > "$dest/cookie_only"
  # cookie + a zero-length payload + a minimal, self-consistent header
  printf '\x1c\x84\x93\x03\x00\x00\x00\x00\x00\x00\x00\x03\x00\x00\x00\x00\x00\x00\x00\x01\x00\x00\x00\x00\x00\x00\x27\x10\x00\x00\x00\x00\x00\x00\x00\x01' \
    > "$dest/empty_payload"
  # cookie + a payload length nothing follows: the truncation case
  printf '\x1c\x84\x93\x03\x7f\xff\xff\xff\x00\x00\x00\x03' > "$dest/huge_declared_length"
  printf '' > "$dest/empty"
  printf '\x00\x00\x00\x00' > "$dest/wrong_cookie"
  echo "  hdr_v2 <- the example document's real encoding + written headers (6 seeds)"
}

# Recorded finding artifacts: the exact inputs that reproduced a crash, a leak
# or a timeout. They live in fuzz/regressions/<target>/, which is TRACKED —
# unlike fuzz/seeds, which is generated and ignored — so every run re-checks
# every finding this lane has ever had. The directory is optional per target,
# and it is wired AFTER the wipe and the corpus links so a recorded artifact
# wins a name clash rather than losing one.
link_regressions() {
  local target="$1"
  local dir="$regressions_root/$target"
  [[ -d "$dir" ]] || return 0

  local dest="$seeds_root/$target"
  mkdir -p "$dest"

  local linked=0 path name
  while IFS= read -r path; do
    name="$(basename "$path")"
    ln -sf "../../../fuzz/regressions/$target/$name" "$dest/$name"
    linked=$((linked + 1))
  done < <(find "$dir" -type f | LC_ALL=C sort)
  echo "  $target <- fuzz/regressions/$target ($linked recorded artifacts)"
}

targets=("$@")
if [[ ${#targets[@]} -eq 0 ]]; then
  targets=(reference_grammar literal_grammar citation artifact_yaml party_document hdr_v2)
fi

for target in "${targets[@]}"; do
  if [[ "$(type -t "seed_$target")" != function ]]; then
    echo "seeds.sh: unknown target: $target" >&2
    exit 1
  fi
  rm -rf "${seeds_root:?}/${target:?}"
  # libFuzzer refuses to start when a corpus directory it was given does not
  # exist, and the writable corpus is empty on a first run or a CI cache miss.
  mkdir -p "$repo_root/fuzz/corpus/$target"
  echo "$target:"
  "seed_$target"
  link_regressions "$target"
  echo "  total: $(find "$seeds_root/$target" \( -type l -o -type f \) | wc -l | tr -d ' ') seeds"
done
