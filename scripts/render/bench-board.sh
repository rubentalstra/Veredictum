#!/usr/bin/env bash
# SPDX-FileCopyrightText: Veredictum contributors
# SPDX-License-Identifier: Apache-2.0
# Render the public benchmark board from the committed submissions.
#
# The board is a static page with no backend and no state, the same doctrine as
# everything else published here: `website/landing/benchmarks.html` is generated
# from `benchmarks/submissions/**/*.json` and committed, so what the site serves
# is a file a reader can diff against the records it came from.
#
# Generated-and-committed rather than generated-at-deploy for one reason: the
# page then has a reviewable diff. A merged submission that leaves the page
# stale is caught by `--check`, which regenerates into a temporary file and
# fails on any difference. That check runs in the CI submission job and again
# inside the site build, so neither a merge nor a deploy can serve a page the
# records no longer support. This is the shape `scripts/render/zenodo-json.sh`
# already established for a generated, committed artifact.
#
# Every row carries ONE INDEX PER REFERENCE CDR, because every submission
# composes them all on its own host from pinned image digests and a board with
# a single reference would be a verdict about that one product. The reference
# set is read out of the records rather than listed here. Ordering a list still
# needs one ruler, so the rows sort by the FerroEHR index and the page says so.
# The ratios are what travel between machines; the absolute milliseconds render
# second, and never without the fingerprint of the machine that produced them.
#
# Usage:
#   scripts/render/bench-board.sh            # write the page
#   scripts/render/bench-board.sh --check    # fail if the committed page is stale
set -euo pipefail

cd "$(dirname "$0")/../.."

command -v jq >/dev/null || { echo "jq is required" >&2; exit 1; }

readonly SUBMISSIONS='benchmarks/submissions'
readonly PAGE='website/landing/benchmarks.html'
readonly GUIDE_URL='https://github.com/rubentalstra/Veredictum/blob/main/benchmarks/SUBMITTING.md'
readonly TREE_URL='https://github.com/rubentalstra/Veredictum/tree/main/benchmarks/submissions'
readonly LEGEND='./benchmark-methodology.html'

# The reference whose index decides the ROW ORDER. Every reference gets its own
# column either way; this one only breaks the tie that sorting a list is, and a
# board that ordered each row by whichever reference happened to look best would
# be sorting numbers that mean different things.
readonly ANCHOR='ferroehr'

MODE="${1:-}"
if [[ -n "$MODE" && "$MODE" != "--check" ]]; then
  echo "usage: $0 [--check]" >&2
  exit 2
fi

# The model: one object per committed submission, its path relative to the
# submissions tree paired with the record itself, ordered by path so the page
# is byte-identical on any machine. Records under `examples/` demonstrate the
# submission pipe and are deliberately not ranked.
model_of() {
  local file model=''
  while IFS= read -r file; do
    model+="$(jq -c --arg path "${file#"$SUBMISSIONS"/}" '{path: $path, doc: .}' "$file")"$'\n'
  done < <(find "$SUBMISSIONS" -type f -name '*.json' -not -path "$SUBMISSIONS/examples/*" | sort)
  printf '%s' "$model" | jq -s '.'
}

# The body: every section between <main> and </main>. jq owns it because every
# number on the page is derived from the records, and `@html` escapes every
# value that came out of one.
render_body() {
  jq -r --arg anchor "$ANCHOR" --arg guide "$GUIDE_URL" --arg tree "$TREE_URL" \
        --arg legend "$LEGEND" '
    def median:
      sort as $v
      | ($v | length) as $n
      | if $n == 0 then null
        elif $n % 2 == 1 then $v[(($n - 1) / 2 | floor)]
        else ($v[($n / 2 | floor) - 1] + $v[($n / 2 | floor)]) / 2
        end;

    # Microseconds to milliseconds, and a ratio to two decimals. Every figure
    # carries its unit: a bare number on a speed board invites a misreading.
    def ms: if . == null then "—" else (. / 1000 * 100 | round / 100 | tostring) + " ms" end;
    def idx: if . == null then "—" else (. * 100 | round / 100 | tostring) end;
    def bytes_gib: if . == null then "memory not disclosed" else (. / 1073741824 * 10 | round / 10 | tostring) + " GiB" end;
    def pct: if . == null then "—" else (. * 1000 | round / 10 | tostring) + "%" end;

    # Every reference CDR the committed records were measured against, in one
    # fixed order across the whole page. Derived from the records rather than
    # listed here, so a reference added to the engine appears on the board the
    # first time a submission carries it.
    def references:
      [ .[] | .doc.relative // [] | .[] | {cdr: .baseline, name: .display_name} ]
      | unique_by(.cdr) | sort_by(.cdr);

    # One reference block of one record, and the p50 ratios it carries across
    # every phase and operation.
    def block($d; $cdr): ($d.relative // []) | map(select(.baseline == $cdr)) | first;
    def ratios($d; $cdr):
      (block($d; $cdr) // {}) | (.phases // {})
      | [ to_entries[] | (.value.operations // {}) | to_entries[] | .value.metrics.p50_us.index ]
      | map(select(. != null));

    # The absolute figures the row prints beside the ratios: the median across
    # every measured operation of that cross-repetition percentile.
    def absolute($d; $metric):
      [ ($d.cross // {}) | to_entries[] | (.value.operations // {}) | to_entries[] | .value[$metric].median ]
      | map(select(. != null)) | median;

    # Arrivals that never produced an answer, over arrivals recorded. A speed
    # figure computed over a mostly failing run describes the failures, so the
    # share appears on the row and not only in the record.
    def failures($d):
      [ ($d.repetitions // [])[] | (.phases // {}) | to_entries[] | (.value.operations // {}) | to_entries[] | .value ]
      | { errors: (map(.errors) | add // 0), count: (map(.count) | add // 0) };

    def rows($refs):
      [ .[]
        | .doc as $d
        | failures($d) as $fail
        | {
            system: ($d.label // (.path | split("/") | .[0])),
            path: .path,
            version: ($d.target.sut_version // "version not disclosed"),
            pack: ($d.pack.id + "@" + $d.pack.version),
            repetitions: ($d.repetitions | length),
            measured_on: ($d.started_at | split("T") | .[0]),
            indices: [ $refs[] | { cdr: .cdr, name: .name, index: (ratios($d; .cdr) | median) } ],
            anchor_index: (ratios($d; $anchor) | median),
            p50: absolute($d; "p50_us"),
            p99: absolute($d; "p99_us"),
            failures: $fail,
            failure_share: (if $fail.count > 0 then $fail.errors / $fail.count else null end),
            environment: $d.environment,
            baselines: ($d.baselines // []),
            blocks: [ $refs[] | { cdr: .cdr, name: .name, block: block($d; .cdr) } ],
            cross: ($d.cross // {}),
            reference_configuration: $d.scale.reference_configuration,
            scale: $d.scale.factor
          }
      ]
      | sort_by(if .anchor_index == null then 1 else 0 end, .anchor_index // 0, .system);

    # The load generator host, printing an absence as an absence. The engine
    # reads no host beyond the standard library and /proc and never spawns a
    # process to learn one, so a field it could not establish is genuinely
    # unknown rather than zero.
    def fingerprint($e):
      [ ($e.cpu_model // "CPU model not disclosed"),
        (if $e.available_parallelism == null then "core count not disclosed"
         else ($e.available_parallelism | tostring) + " cores" end),
        ($e.total_memory_bytes | bytes_gib),
        ($e.os + "/" + $e.arch)
      ] | join(" · ");

    # The per-operation table inside a row disclosure: every phase, every
    # operation, the discipline that produced the figures, and one ratio column
    # per reference.
    def detail($row):
      [ $row.cross | to_entries[] as $phase
        | $phase.value.operations | to_entries[] as $op
        | {
            phase: $phase.key,
            regime: $phase.value.regime,
            op: $op.key,
            p50: $op.value.p50_us.median,
            p90: $op.value.p90_us.median,
            p99: $op.value.p99_us.median,
            throughput: ($op.value.throughput_ops_s.median // 0),
            indices: [ $row.blocks[]
                       | (.block.phases[$phase.key].operations[$op.key].metrics.p50_us.index) // null ]
          }
      ];

    def gaps($row):
      [ $row.blocks[] | .name as $name | (.block.gaps // [])[]
        | $name + ": " + .phase + " / " + .operation + " (" + .metric + "): " + .reason ];

    def row_html($rank; $row):
      "        <article class=\"board-row\">\n" +
      "          <div class=\"board-rank\" aria-hidden=\"true\">" + ($rank | tostring) + "</div>\n" +
      "          <div class=\"board-head\">\n" +
      "            <h3>" + ($row.system | @html) + " <span class=\"board-version\">" + ($row.version | @html) + "</span></h3>\n" +
      "            <p class=\"board-meta\"><span class=\"tier tier-self\">self-reported</span> " +
      "<code>" + ($row.pack | @html) + "</code> · " + ($row.repetitions | tostring) + " repetitions · measured " + ($row.measured_on | @html) +
      (if $row.reference_configuration then "" else " · scaled to " + ($row.scale | tostring) + " of the pinned population" end) + "</p>\n" +
      "          </div>\n" +
      "          <div class=\"board-indices\">\n" +
      ([ $row.indices[]
         | "            <div class=\"board-index\"><span class=\"n\">" + (.index | idx) + "</span><span class=\"l\">vs " + (.name | @html) + "</span></div>"
       ] | join("\n")) + "\n" +
      "          </div>\n" +
      "          <div class=\"board-absolute\">\n" +
      "            <p><b>" + ($row.p50 | ms) + "</b> median, <b>" + ($row.p99 | ms) + "</b> at the 99th percentile</p>\n" +
      "            <p class=\"board-machine\">" + (fingerprint($row.environment) | @html) + "</p>\n" +
      "            <p class=\"board-machine\">" + ($row.failures.errors | tostring) + " of " + ($row.failures.count | tostring) + " measured arrivals failed (" + ($row.failure_share | pct) + ")</p>\n" +
      "          </div>\n" +
      "          <details class=\"board-detail\">\n" +
      "            <summary>Per-operation detail</summary>\n" +
      "            <div class=\"table-scroll\">\n" +
      "              <table>\n" +
      "                <thead><tr><th scope=\"col\">Phase</th><th scope=\"col\">Operation</th><th scope=\"col\">Discipline</th><th scope=\"col\">p50</th><th scope=\"col\">p90</th><th scope=\"col\">p99</th><th scope=\"col\">Throughput</th>" +
      ([ $row.indices[] | "<th scope=\"col\">vs " + (.name | @html) + "</th>" ] | join("")) + "</tr></thead>\n" +
      "                <tbody>\n" +
      ([ detail($row)[]
         | "                  <tr><td>" + (.phase | @html) + "</td><td><code>" + (.op | @html) + "</code></td><td>" + (.regime | @html) + "</td><td>" + (.p50 | ms) + "</td><td>" + (.p90 | ms) + "</td><td>" + (.p99 | ms) + "</td><td>" + ((.throughput * 10 | round / 10) | tostring) + " ops/s</td>" +
           ([ .indices[] | "<td>" + (. | idx) + "</td>" ] | join("")) + "</tr>"
       ] | join("\n")) + "\n" +
      "                </tbody>\n" +
      "              </table>\n" +
      "            </div>\n" +
      "            <p class=\"board-provenance\">Reference deployments composed on the same host: " +
      ([ $row.baselines[] | (.display_name | @html) + " (" + (.images | to_entries | map(.value | split("@") | .[0]) | join(", ") | @html) + ")" ] | join("; ")) +
      ". Record: <a href=\"" + $tree + "/" + ($row.path | @html) + "\">" + ($row.path | @html) + "</a>.</p>\n" +
      (if (gaps($row) | length) > 0 then
        "            <p class=\"board-provenance\">No ratio could be formed for: " + (gaps($row) | join("; ") | @html) + ".</p>\n"
      else "" end) +
      "          </details>\n" +
      "        </article>";

    references as $refs |
    rows($refs) as $rows |
    # With nothing committed yet there is no record to read the reference names
    # out of, and naming them here would be a second copy of the engine list.
    (if ($refs | length) == 0 then "the pinned reference CDRs"
     else [ $refs[] | .name ] | join(" and ") end) as $reference_names |
    "  <section class=\"board-intro\">\n" +
    "    <div class=\"wrap\">\n" +
    "      <div class=\"head\">\n" +
    "        <span class=\"eyebrow\">Public benchmark board</span>\n" +
    "        <h1>How fast is each openEHR CDR, measured the same way</h1>\n" +
    "        <p>Every row below is a benchmark record somebody ran with the\n" +
    "          <code>veredictum bench</code> command and submitted as a pull request to this\n" +
    "          repository. The record carries the pack it drove, the seed it drove at, every\n" +
    "          repetition, the machine it ran on, and the reference deployments it was measured\n" +
    "          against on that same machine. CI validates all of that before a maintainer looks\n" +
    "          at it, and the merge is the acceptance.</p>\n" +
    "        <p><a href=\"" + $legend + "\">What each pack actually creates and measures →</a>\n" +
    "          That page is generated from the packs the instrument embeds, so it says what a\n" +
    "          row below was produced by rather than what anyone remembers it being.</p>\n" +
    "      </div>\n" +
    "      <div class=\"boundary\">\n" +
    "        <p><b>A bench number is not a conformance verdict.</b> This board reports\n" +
    "          comparative speed. It is not a conformance record, not a certificate, and not a\n" +
    "          performance-class rating; a bench result may motivate a class run, never\n" +
    "          substitute for one. A fast server that fails the catalogue is a fast server that\n" +
    "          fails the catalogue.</p>\n" +
    "      </div>\n" +
    "    </div>\n" +
    "  </section>\n" +
    "\n" +
    "  <section id=\"board\">\n" +
    "    <div class=\"wrap\">\n" +
    "      <div class=\"head\">\n" +
    "        <span class=\"eyebrow\">The board</span>\n" +
    "        <h2>Two reference baselines on every row</h2>\n" +
    "        <p>Each submission measures " + ($reference_names | @html) + " on its own machine, in the\n" +
    "          same session, with the same pack at the same seed, so every row carries one index\n" +
    "          per reference. An index is the median latency of the submission divided by the\n" +
    "          same statistic from that reference: below 1.0 is faster than it, above 1.0 is\n" +
    "          slower. Ordering a list needs a single ruler, so the rows are sorted by the\n" +
    "          FerroEHR index; the EHRbase index sits beside it on every row and is the same\n" +
    "          measurement against the other reference.</p>\n" +
    "      </div>\n" +
    (if ($rows | length) == 0 then
      "      <p class=\"after-code\">No submission has been merged yet. The first one to arrive\n" +
      "        will be the first row.</p>\n"
    else
      "      <div class=\"board-rows\">\n" +
      ([ $rows | to_entries[] | row_html((.key + 1); .value) ] | join("\n")) + "\n" +
      "      </div>\n"
    end) +
    "    </div>\n" +
    "  </section>\n" +
    "\n" +
    "  <section id=\"how-to-read\">\n" +
    "    <div class=\"wrap\">\n" +
    "      <div class=\"head\">\n" +
    "        <span class=\"eyebrow\">How to read this board</span>\n" +
    "        <h2>Written for someone who has never seen the instrument</h2>\n" +
    "      </div>\n" +
    "      <div class=\"prose\">\n" +
    "        <p><b>Why a ratio and not milliseconds.</b> A latency in milliseconds describes a\n" +
    "          system and the machine it ran on at the same time, so two numbers taken on\n" +
    "          different hardware cannot be compared. Every submission therefore measures the\n" +
    "          reference CDRs on its own machine, in the same session, with the same pack at the\n" +
    "          same seed. Dividing one median by the other cancels the machine out.</p>\n" +
    "        <p><b>Why two references and not one.</b> A single reference makes the board a\n" +
    "          verdict about that one product. Two independent ones, measured under identical\n" +
    "          container ceilings in the same session, show whether a row is fast in general or\n" +
    "          only fast against one comparison. A row whose two indices disagree sharply is\n" +
    "          telling you something the ordering alone does not.</p>\n" +
    "        <p><b>What the absolute figures are for.</b> They say what the ratio felt like on\n" +
    "          one specific machine, which is why they never appear without the fingerprint of\n" +
    "          that machine beside them. Read them as the scale of the work, never as a claim\n" +
    "          about what your deployment would do.</p>\n" +
    "        <p><b>What the references are.</b> Each submission composes " + ($reference_names | @html) + "\n" +
    "          from image digests, under identical container ceilings. They are not a standard\n" +
    "          of correctness. They are a ruler that happens to be the same length on every\n" +
    "          machine.</p>\n" +
    "        <p><b>Failed arrivals.</b> Every row states how many measured arrivals never\n" +
    "          produced an answer. A percentile computed over a run that was mostly failing\n" +
    "          describes the failures, so read the share before the milliseconds. The submission\n" +
    "          gate refuses a record in which any operation produced no successful arrival at\n" +
    "          all.</p>\n" +
    "        <p><b>Open-loop, and what that buys.</b> Arrivals fire at their planned instants\n" +
    "          regardless of whether an earlier request has come back, and every latency is\n" +
    "          measured from the planned instant. A server that stalls therefore shows the stall\n" +
    "          in its percentiles instead of quietly issuing fewer requests. Phases that are\n" +
    "          closed-loop by construction, such as the bulk load, are labelled as such and are\n" +
    "          reported as throughput, never as a latency claim.</p>\n" +
    "        <p><b>Verified and declared-only.</b> Every row today carries the\n" +
    "          <span class=\"tier tier-self\">self-reported</span> tier: the submitter ran the\n" +
    "          benchmark and the record passed CI, and nobody here re-ran it. A record a\n" +
    "          maintainer reproduces will carry a reproduced tier, on the same submission\n" +
    "          channel. Read a self-reported row as a claim its author put their name to in a\n" +
    "          public git history, and nothing more.</p>\n" +
    "        <p><b>Repetitions.</b> Three is the floor. One repetition measures a moment, so a\n" +
    "          record with fewer is rejected before it can be ranked, and each figure on the\n" +
    "          board is the median across repetitions.</p>\n" +
    "      </div>\n" +
    "    </div>\n" +
    "  </section>\n" +
    "\n" +
    "  <section class=\"quickstart\" id=\"submit\">\n" +
    "    <div class=\"wrap\">\n" +
    "      <div class=\"head\">\n" +
    "        <span class=\"eyebrow\">Submit your CDR</span>\n" +
    "        <h2>Run the pack, open a pull request</h2>\n" +
    "        <p>The reference pack for this board is <code>community-vitals</code>. Point the\n" +
    "          command at your deployment, let it compose both reference baselines on the same\n" +
    "          machine, and commit the record it writes.</p>\n" +
    "      </div>\n" +
    "      <figure class=\"code\">\n" +
    "        <figcaption>One command, one record</figcaption>\n" +
    "        <pre><code><span class=\"cmt\"># The credential is read from the environment; it never rides argv.</span>\n" +
    "<span class=\"p\">$</span> export VEREDICTUM_BENCH_PASSWORD=…\n" +
    "<span class=\"p\">$</span> veredictum bench --base-url https://cdr.example/openehr/v1 \\\n" +
    "      --auth basic --user &lt;user&gt; \\\n" +
    "      --pack community-vitals --repetitions 3 --with-baselines \\\n" +
    "      --out ./bench --label &quot;Your CDR 1.2.3&quot;</code></pre>\n" +
    "      </figure>\n" +
    "      <p class=\"after-code\">Then copy the record into\n" +
    "        <code>benchmarks/submissions/&lt;system&gt;/&lt;date&gt;-&lt;host&gt;.json</code> and open a pull\n" +
    "        request. CI checks the schema, the pack pins, the repetition count, the baselines,\n" +
    "        the failed-arrival ceiling the pack pins, the environment fingerprint and the file\n" +
    "        name, and refuses any edit to a record\n" +
    "        already merged. <a href=\"" + $guide + "\">The submission guide, in full →</a></p>\n" +
    "    </div>\n" +
    "  </section>"
  ' <<<"$1"
}

render_page() {
  local body
  body="$(render_body "$(model_of)")"
  cat <<PAGE
<!DOCTYPE html>
<html lang="en">
<head>
  <meta charset="utf-8">
  <meta name="viewport" content="width=device-width, initial-scale=1">
  <!-- GitHub Pages serves static files and sets no response headers, so the
       policy travels as a <meta> element. A meta CSP is weaker than the header
       by specification: \`frame-ancestors\`, \`report-uri\` and \`sandbox\` are
       ignored in meta form (W3C CSP3 §3.3). It still blocks an injected
       external script and an exfiltrating connection, which is the realistic
       risk for a static page.
       This page carries no <script> at all: the per-row disclosure is a
       <details> element, which needs none. style-src keeps 'unsafe-inline'
       only because the shared stylesheet's siblings use it; this page's own
       styles all live in style.css. -->
  <meta http-equiv="Content-Security-Policy" content="default-src 'self'; script-src 'self'; style-src 'self' 'unsafe-inline'; img-src 'self' data:; font-src 'self'; connect-src 'self'; object-src 'none'; base-uri 'self'; form-action 'none'; frame-ancestors 'none'">
  <meta name="referrer" content="strict-origin-when-cross-origin">
  <title>Benchmark board — Veredictum</title>
  <meta name="description" content="Comparative speed for openEHR clinical data repositories, ranked by a same-machine relative index. Every record is a community submission validated by CI before merge. A bench number is never a conformance verdict.">
  <link rel="canonical" href="https://veredictum.eu/benchmarks.html">
  <link rel="icon" type="image/svg+xml" href="./assets/favicon.svg">
  <link rel="stylesheet" href="./style.css">
  <meta property="og:type" content="website">
  <meta property="og:title" content="Benchmark board — Veredictum">
  <meta property="og:description" content="Comparative speed for openEHR clinical data repositories, ranked by a same-machine relative index.">
  <meta property="og:url" content="https://veredictum.eu/benchmarks.html">
  <meta property="og:site_name" content="Veredictum">
  <meta property="og:image" content="https://veredictum.eu/assets/veredictum-seal.svg">
  <meta property="og:image:alt" content="The Veredictum conformance seal">
  <meta name="twitter:card" content="summary">
  <meta name="twitter:title" content="Benchmark board — Veredictum">
  <meta name="twitter:description" content="Comparative speed for openEHR clinical data repositories, ranked by a same-machine relative index.">
  <meta name="twitter:image" content="https://veredictum.eu/assets/veredictum-seal.svg">
</head>
<body>

<a class="skip" href="#main">Skip to content</a>

<header class="masthead">
  <div class="wrap">
    <a class="wordmark" href="./">
      <img src="./assets/veredictum-icon.svg" alt="" aria-hidden="true">
      <span>Veredictum</span>
    </a>
    <nav aria-label="Primary">
      <a href="./docs/">Docs</a>
      <a href="./benchmarks.html" aria-current="page">Benchmarks</a>
      <a href="./benchmark-methodology.html">Methodology</a>
      <a href="https://github.com/rubentalstra/Veredictum" rel="noopener">GitHub</a>
    </nav>
  </div>
</header>

<main id="main">

$body

</main>

<footer>
  <div class="wrap">
    <a class="wordmark" href="./">
      <img src="./assets/veredictum-icon.svg" alt="" aria-hidden="true">
      <span>Veredictum</span>
    </a>
    <p>This page is generated from the records committed under
      <a href="$TREE_URL" rel="noopener">benchmarks/submissions</a> and is regenerated
      whenever one is merged. Nothing on it is typed by hand.</p>
    <div class="fine">
      <p>openEHR® is the registered trademark of the openEHR Foundation.
        Veredictum is an independent, community-driven conformance instrument:
        it names openEHR descriptively, to say what is being tested, and it is
        not an official openEHR Foundation product, not the Foundation's CNF
        program, and not endorsed by or affiliated with the Foundation.</p>
      <p>© Veredictum contributors.</p>
    </div>
  </div>
</footer>

</body>
</html>
PAGE
}

if [[ "$MODE" == "--check" ]]; then
  rendered="$(mktemp)"
  trap 'rm -f "$rendered"' EXIT
  render_page > "$rendered"
  if ! diff -u "$PAGE" "$rendered"; then
    echo "::error::$PAGE is stale — regenerate it with scripts/render/bench-board.sh" >&2
    exit 1
  fi
  echo "bench-board: $PAGE matches the committed submissions — OK."
else
  render_page > "$PAGE"
  echo "bench-board: wrote $PAGE"
fi
