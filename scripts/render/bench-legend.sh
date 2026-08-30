#!/usr/bin/env bash
# SPDX-FileCopyrightText: Veredictum contributors
# SPDX-License-Identifier: Apache-2.0
# Render the benchmark legend from the emitted pack manifest.
#
# The board (scripts/render/bench-board.sh) publishes the numbers; this page
# publishes the work those numbers describe. A hand-written explanation rots
# the first time a pack version moves, so the page is GENERATED from
# `website/landing/bench-packs.json`, which the binary itself emits with
# `veredictum bench-packs --out website/landing`. A Rust integration test holds
# that document byte-identical to what the packs compile to, and `--check` here
# holds the page byte-identical to the document, so a pack edit cannot reach
# the site with either one stale.
#
# The manifest is committed rather than emitted at deploy time for the reason
# the board is: the page then has a reviewable diff, and the site build needs
# no Rust toolchain to serve it. The document is served beside the page, so a
# reader can fetch what the page was rendered from.
#
# Usage:
#   scripts/render/bench-legend.sh            # write the page
#   scripts/render/bench-legend.sh --check    # fail if the committed page is stale
set -euo pipefail

cd "$(dirname "$0")/../.."

command -v jq >/dev/null || { echo "jq is required" >&2; exit 1; }

readonly MANIFEST='website/landing/bench-packs.json'
readonly PAGE='website/landing/benchmark-methodology.html'
readonly BOARD='./benchmarks.html'
readonly GUIDE_URL='https://github.com/rubentalstra/Veredictum/blob/main/benchmarks/SUBMITTING.md'

MODE="${1:-}"
if [[ -n "$MODE" && "$MODE" != "--check" ]]; then
  echo "usage: $0 [--check]" >&2
  exit 2
fi

[[ -f "$MANIFEST" ]] || {
  echo "::error::$MANIFEST is missing — emit it with \`cargo run -- bench-packs --out website/landing\`" >&2
  exit 1
}

# The body: every section between <main> and </main>. jq owns it because every
# figure and every sentence of pack detail on the page comes out of the
# manifest, and `@html` escapes each one.
render_body() {
  jq -r --arg board "$BOARD" --arg guide "$GUIDE_URL" '
    # ── Number formatting ────────────────────────────────────────────────────
    # A population figure reads as 100,000 rather than 100000, and a rate keeps
    # at most two decimals. Every figure carries its unit at the call site.
    def commas:
      tostring as $s
      | if ($s | length) <= 3 then $s
        else (($s[:-3] | tonumber | commas) + "," + $s[-3:])
        end;
    def rate: (. * 100 | round / 100 | tostring);
    def kib: if . < 1024 then (tostring + " bytes")
             else ((. / 1024 * 10 | round / 10 | tostring) + " KiB") end;
    def plural($n; $one; $many): if $n == 1 then $one else $many end;

    # ── The phase timeline ───────────────────────────────────────────────────
    # One row per phase, drawn to the same scale. A measured phase is the only
    # phase whose length is known before the run, so it is the only one drawn
    # to a second axis: its warmup and its measured window are shown at their
    # real proportions. A closed-loop phase runs until it is finished, which
    # depends on the system under test, so its bar is drawn open-ended and
    # says so rather than inventing a duration.
    def bar_x: 168;
    def bar_w: 452;
    def row_h: 46;

    def span($phase):
      if $phase.kind == "measure" then ($phase.warmup_s + $phase.duration_s) else 0 end;

    def timeline($pack):
      ([ $pack.phases[] | span(.) ] | max) as $scale
      | ($pack.phases | length) as $rows
      | ($rows * row_h + 26) as $height
      | "        <svg class=\"timeline\" viewBox=\"0 0 640 " + ($height | tostring) +
        "\" role=\"img\" aria-label=\"The phases of the " + ($pack.id | @html) +
        " pack, in execution order\" xmlns=\"http://www.w3.org/2000/svg\">\n" +
        ([ $pack.phases | to_entries[]
           | .key as $i | .value as $p
           | ($i * row_h + 16) as $y
           | "          <text class=\"tl-name\" x=\"0\" y=\"" + (($y + 13) | tostring) + "\">" +
             ($p.name | @html) + "</text>\n" +
             (if $p.kind == "measure" and $scale > 0 then
               (($p.warmup_s / $scale * bar_w) | floor) as $ww
               | "          <rect class=\"tl-warm\" x=\"" + (bar_x | tostring) + "\" y=\"" + ($y | tostring) +
                 "\" width=\"" + ($ww | tostring) + "\" height=\"18\"/>\n" +
                 "          <rect class=\"tl-meas\" x=\"" + ((bar_x + $ww) | tostring) + "\" y=\"" + ($y | tostring) +
                 "\" width=\"" + ((bar_w - $ww) | tostring) + "\" height=\"18\"/>\n" +
                 "          <text class=\"tl-note\" x=\"" + (bar_x | tostring) + "\" y=\"" + (($y + 31) | tostring) + "\">" +
                 "warmup " + ($p.warmup_s | tostring) + "s, discarded · measured " +
                 ($p.duration_s | tostring) + "s at " + ($p.rate_per_s | rate) + " arrivals/s</text>\n"
             else
               "          <rect class=\"tl-closed\" x=\"" + (bar_x | tostring) + "\" y=\"" + ($y | tostring) +
               "\" width=\"" + ((bar_w - 40) | tostring) + "\" height=\"18\"/>\n" +
               "          <text class=\"tl-note\" x=\"" + (bar_x | tostring) + "\" y=\"" + (($y + 31) | tostring) + "\">" +
               "closed-loop, runs until it is finished</text>\n"
             end)
         ] | join("")) +
        "        </svg>";

    # ── One phase in plain words ─────────────────────────────────────────────
    def seed_words($p):
      "The <b>" + ($p.name | @html) + "</b> phase builds the population. It creates " +
      ($p.ehrs | commas) + " " + plural($p.ehrs; "EHR"; "EHRs") +
      " through the public API and commits the same composition " +
      ($p.compositions_per_ehr | commas) + " " +
      plural($p.compositions_per_ehr; "time"; "times") + " into each, on " +
      ($p.workers | tostring) + " " + plural($p.workers; "worker"; "workers") + ", leaving " +
      ($p.compositions | commas) + " compositions behind. Every later phase reads and writes " +
      "against exactly that population. The phase is closed-loop: the next request goes out " +
      "once the previous one has answered, so it reports bulk-load throughput and never a " +
      "latency claim.";

    def sweep_words($p):
      "The <b>" + ($p.name | @html) + "</b> phase walks the whole population in order, issuing " +
      ($p.requests_per_composition | tostring) + " reads against every committed composition on " +
      ($p.workers | tostring) + " " + plural($p.workers; "worker"; "workers") +
      ". That is what a single-client harness does, and it is closed-loop by construction, so " +
      "the figure it reports is the whole-loop average per request. It is the number that " +
      "compares with a published harness figure, and it is not a percentile.";

    def measure_words($p):
      "The <b>" + ($p.name | @html) + "</b> phase offers arrivals on a fixed schedule: " +
      ($p.rate_per_s | rate) + " a second for " + ($p.duration_s | tostring) + " seconds, after a " +
      ($p.warmup_s | tostring) + "-second warmup whose " +
      (($p.planned_arrivals - $p.planned_measured_arrivals) | commas) +
      " arrivals are dispatched and then discarded. " + ($p.planned_measured_arrivals | commas) +
      " arrivals are measured. They fire at their planned instants whether or not an earlier " +
      "request has come back, and every latency is measured from the planned instant, so a " +
      "server that stalls shows the stall in its percentiles instead of quietly receiving " +
      "fewer requests.";

    def phase_words($p):
      if $p.kind == "seed" then seed_words($p)
      elif $p.kind == "sweep" then sweep_words($p)
      else measure_words($p)
      end;

    # ── The mix table of one measured phase ──────────────────────────────────
    def wire_of($token): (.operations[] | select(.token == $token) | .wire);

    def mix_table($manifest; $p):
      "          <div class=\"table-scroll\">\n" +
      "            <table>\n" +
      "              <caption>What the <b>" + ($p.name | @html) + "</b> phase offers, and why each one is in the mix</caption>\n" +
      "              <thead><tr><th scope=\"col\">Operation</th><th scope=\"col\">Request</th>" +
      "<th scope=\"col\">Share</th><th scope=\"col\">Offered</th><th scope=\"col\">What it probes</th></tr></thead>\n" +
      "              <tbody>\n" +
      ([ $p.mix[]
         | .op as $token
         | "                <tr><td><code>" + ($token | @html) + "</code></td><td><code>" +
           (($manifest | wire_of($token)) | @html) + "</code></td><td>" + (.share | tostring) +
           "</td><td>" + (.rate_per_s | rate) + "/s</td><td>" + (.rationale | @html) + "</td></tr>"
       ] | join("\n")) + "\n" +
      "              </tbody>\n" +
      "            </table>\n" +
      "          </div>";

    def sweep_table($manifest; $p):
      "          <div class=\"table-scroll\">\n" +
      "            <table>\n" +
      "              <caption>The " + ($p.requests_per_composition | tostring) +
      " reads the <b>" + ($p.name | @html) + "</b> phase issues against every composition, in this order</caption>\n" +
      "              <thead><tr><th scope=\"col\">#</th><th scope=\"col\">Operation</th><th scope=\"col\">Request</th></tr></thead>\n" +
      "              <tbody>\n" +
      ([ $p.operations | to_entries[]
         | .key as $index | .value as $token
         | "                <tr><td>" + (($index + 1) | tostring) + "</td><td><code>" +
           ($token | @html) + "</code></td><td><code>" +
           (($manifest | wire_of($token)) | @html) + "</code></td></tr>"
       ] | join("\n")) + "\n" +
      "              </tbody>\n" +
      "            </table>\n" +
      "          </div>";

    def phase_block($manifest; $p):
      "        <div class=\"phase\">\n" +
      "          <p>" + phase_words($p) + "</p>\n" +
      (if $p.kind == "measure" then mix_table($manifest; $p) + "\n"
       elif $p.kind == "sweep" then sweep_table($manifest; $p) + "\n"
       else "" end) +
      "        </div>";

    def profiles_block($pack):
      "        <div class=\"phase\">\n" +
      "          <p>A run against this pack declares one <b>posture profile</b>, which says what\n" +
      "            was switched on behind the numbers. " +
      ([ $pack.profiles[] | select(.default) | "<code>" + (.name | @html) + "</code>" ] | join(", ")) +
      " is what a run takes when it names none.</p>\n" +
      "          <div class=\"table-scroll\">\n" +
      "            <table>\n" +
      "              <caption>The posture profiles <code>" + ($pack.id | @html) +
      "</code> defines, and what each one declares</caption>\n" +
      "              <thead><tr><th scope=\"col\">Profile</th><th scope=\"col\">What it switches on</th>" +
      "<th scope=\"col\">Declares</th></tr></thead>\n" +
      "              <tbody>\n" +
      ([ $pack.profiles[]
         | "                <tr><td><code>" + (.name | @html) + "</code>" +
           (if .default then " <span class=\"tier tier-self\">default</span>" else "" end) +
           "</td><td>" + (.summary | @html) + "</td><td>" +
           ([ .declares | to_entries[] | "<code>" + (.key | @html) + "</code> " + (.value | @html) ]
            | join("<br>")) + "</td></tr>"
       ] | join("\n")) + "\n" +
      "              </tbody>\n" +
      "            </table>\n" +
      "          </div>\n" +
      "        </div>";

    def fixtures_block($pack):
      "        <details class=\"pack-detail\">\n" +
      "          <summary>The bytes it offers, and where they came from</summary>\n" +
      ([ $pack.fixtures[]
         | "          <div class=\"fixture\">\n" +
           "            <p class=\"fixture-head\"><code>" + (.key | @html) + "</code> · " +
           (.kind | @html | gsub("_"; " ")) + " · " + (.bytes | kib) + " · <code>" +
           (.media_type | @html) + "</code></p>\n" +
           "            <p>" + (.provenance | @html) + "</p>\n" +
           "            <p class=\"fixture-pin\">sha256 <code>" + (.sha256 | @html) + "</code></p>\n" +
           "          </div>"
       ] | join("\n")) + "\n" +
      "          <p class=\"pack-note\">Each digest is verified when the pack loads, so a run\n" +
      "            refuses to start if a single byte of any fixture has moved. Two records that\n" +
      "            name the same pack version therefore offered the same bytes to both systems.\n" +
      "            A fixture marked <b>invalid composition</b> is never committed by a phase and\n" +
      "            never enters the measured population: the commit-validation canary offers it\n" +
      "            once before and once after the measured window, to see whether the server\n" +
      "            refuses it as the declared posture says it should.</p>\n" +
      "        </details>";

    def pack_block($manifest; $pack):
      "      <article class=\"pack\" id=\"pack-" + ($pack.id | @html) + "\">\n" +
      "        <div class=\"pack-head\">\n" +
      "          <h3><code>" + ($pack.id | @html) + "</code> <span class=\"pack-version\">version " +
      ($pack.version | @html) + "</span></h3>\n" +
      "          <p class=\"pack-meta\">seed <code>" + ($pack.seed | tostring) + "</code> · " +
      ($pack.phases | length | tostring) + " phases · failed-arrival ceiling <code>" +
      ($pack.max_failed_share | tostring) + "</code> · drive it with <code>--pack " +
      ($pack.id | @html) + "</code></p>\n" +
      "        </div>\n" +
      timeline($pack) + "\n" +
      ([ $pack.phases[] | phase_block($manifest; .) ] | join("\n")) + "\n" +
      profiles_block($pack) + "\n" +
      fixtures_block($pack) + "\n" +
      "        <details class=\"pack-detail\">\n" +
      "          <summary>The pack&#39;s own description, as every record carries it</summary>\n" +
      "          <p class=\"pack-verbatim\">" + ($pack.description | @html) + "</p>\n" +
      "        </details>\n" +
      "      </article>";

    # ── The page ─────────────────────────────────────────────────────────────
    . as $manifest |
    "  <section class=\"board-intro\">\n" +
    "    <div class=\"wrap\">\n" +
    "      <div class=\"head\">\n" +
    "        <span class=\"eyebrow\">Benchmark methodology</span>\n" +
    "        <h1>What the benchmark actually creates, and what it actually measures</h1>\n" +
    "        <p>The <a href=\"" + $board + "\">benchmark board</a> ranks openEHR clinical data\n" +
    "          repositories by speed. This page says what produced those numbers: the data each\n" +
    "          benchmark pack creates, the requests it then issues, how long it issues them for,\n" +
    "          and what each request is there to find out. It is written for someone who has\n" +
    "          never run the tool.</p>\n" +
    "        <p>Nothing below is typed by hand. A pack is versioned data compiled into the\n" +
    "          <code>veredictum</code> binary, and the binary emits its own description as\n" +
    "          <a href=\"./bench-packs.json\">bench-packs.json</a>, which this page is rendered\n" +
    "          from. A change to a pack that left this page saying something else would fail the\n" +
    "          build.</p>\n" +
    "      </div>\n" +
    "      <div class=\"boundary\">\n" +
    "        <p><b>A bench number is not a conformance verdict.</b> " +
    ($manifest.boundary_statement | @html) + " Conformance is a separate record, produced by a\n" +
    "          different part of the same tool: it drives a catalogue of spec-cited cases and\n" +
    "          judges each answer against the released openEHR specifications. Speed says nothing\n" +
    "          about whether a server is correct, and nothing on this page claims otherwise.</p>\n" +
    "      </div>\n" +
    "    </div>\n" +
    "  </section>\n" +
    "\n" +
    "  <section id=\"how-a-run-works\">\n" +
    "    <div class=\"wrap\">\n" +
    "      <div class=\"head\">\n" +
    "        <span class=\"eyebrow\">How a run works</span>\n" +
    "        <h2>Seed once, measure several times</h2>\n" +
    "        <p>Every pack follows the same shape, and each term below means the same thing\n" +
    "          further down this page, on the board, and inside every published record.</p>\n" +
    "      </div>\n" +
    "      <div class=\"prose\">\n" +
    "        <p><b>What gets created.</b> A run starts by uploading the pack&#39;s operational\n" +
    "          template, creating EHRs, and committing compositions into them, all through the\n" +
    "          same public REST API a real client would use. The tool reaches no database and\n" +
    "          reads no server-internal state. The population that load leaves behind is what\n" +
    "          every measured request then addresses, so two runs of the same pack measure work\n" +
    "          against the same amount of data.</p>\n" +
    "        <p><b>Closed-loop.</b> The next request goes out once the previous one has come\n" +
    "          back. A slow server therefore receives fewer requests, which is why a closed-loop\n" +
    "          phase reports throughput and a whole-loop average rather than percentiles. It is\n" +
    "          the discipline a single-client harness uses, and it is how the bulk load and the\n" +
    "          read walk are reported.</p>\n" +
    "        <p><b>Open-loop.</b> Arrivals fire at instants planned before the phase starts, at a\n" +
    "          rate the pack version pins, whether or not earlier requests have answered. Every\n" +
    "          latency is measured from the planned instant rather than from the moment the\n" +
    "          request went out, so a server that stalls for a second charges that second to\n" +
    "          every arrival that was due during it. This is where the percentiles come from, and\n" +
    "          it is what stops a stall from hiding behind a lower request count.</p>\n" +
    "        <p><b>Warmup.</b> The first seconds of an open-loop phase are dispatched and then\n" +
    "          thrown away. A cold cache and a just-started process describe the first minute of\n" +
    "          a deployment rather than the deployment, so those arrivals are made and their\n" +
    "          latencies are discarded. Each pack below states how long its warmup is and how\n" +
    "          many arrivals it costs.</p>\n" +
    "        <p><b>The seed.</b> " + ($manifest.seed_disclosure | @html) + "</p>\n" +
    "        <p><b>The posture.</b> " + ($manifest.posture_disclosure | @html) + " Each pack\n" +
    "          below lists the profiles it defines and what each one declares, and every record\n" +
    "          names the profile its own run was measured under.</p>\n" +
    "        <p><b>Repetitions.</b> The population is built once and the measured phases are then\n" +
    "          repeated. Every figure a record publishes is the median across repetitions, with\n" +
    "          the spread beside it, because one repetition measures a moment.</p>\n" +
    "        <p><b>The relative index.</b> " + ($manifest.relative_index | @html) + " A record\n" +
    "          earns one by composing the pinned reference deployments on the same machine, in\n" +
    "          the same session, and driving the same pack at the same seed against them. That is\n" +
    "          the figure the board sorts on, because milliseconds taken on somebody else&#39;s\n" +
    "          hardware cannot be compared with yours.</p>\n" +
    "      </div>\n" +
    "    </div>\n" +
    "  </section>\n" +
    "\n" +
    "  <section id=\"packs\">\n" +
    "    <div class=\"wrap\">\n" +
    "      <div class=\"head\">\n" +
    "        <span class=\"eyebrow\">The packs</span>\n" +
    "        <h2>" + ($manifest.packs | length | tostring) + " packs, each pinned by version</h2>\n" +
    "        <p>Two records are comparable when they name the same pack at the same version. A\n" +
    "          change to any figure below is a change to the work, so it moves the version and\n" +
    "          the older records stop being comparable with the newer ones.</p>\n" +
    "      </div>\n" +
    ([ $manifest.packs[] | pack_block($manifest; .) ] | join("\n")) + "\n" +
    "    </div>\n" +
    "  </section>\n" +
    "\n" +
    "  <section id=\"vocabulary\">\n" +
    "    <div class=\"wrap\">\n" +
    "      <div class=\"head\">\n" +
    "        <span class=\"eyebrow\">The operation vocabulary</span>\n" +
    "        <h2>Every request a pack is allowed to offer</h2>\n" +
    "        <p>A pack may only offer an operation from this closed list. A token outside it is\n" +
    "          refused when the pack loads, so a typo can never quietly become a different\n" +
    "          measurement.</p>\n" +
    "      </div>\n" +
    "      <div class=\"table-scroll\">\n" +
    "        <table>\n" +
    "          <thead><tr><th scope=\"col\">Token</th><th scope=\"col\">Request</th></tr></thead>\n" +
    "          <tbody>\n" +
    ([ $manifest.operations[]
       | "            <tr><td><code>" + (.token | @html) + "</code></td><td><code>" +
         (.wire | @html) + "</code></td></tr>"
     ] | join("\n")) + "\n" +
    "          </tbody>\n" +
    "        </table>\n" +
    "      </div>\n" +
    "      <p class=\"after-code\">A path shown with <code>{ehr_id}</code>, <code>{uid}</code>,\n" +
    "        <code>{version_uid}</code> or <code>{at_time}</code> has that value substituted per\n" +
    "        arrival from the seeded draw. The tool builds every request it sends from exactly\n" +
    "        these templates.</p>\n" +
    "    </div>\n" +
    "  </section>\n" +
    "\n" +
    "  <section class=\"quickstart\" id=\"submitting\">\n" +
    "    <div class=\"wrap\">\n" +
    "      <div class=\"head\">\n" +
    "        <span class=\"eyebrow\">Getting on the board</span>\n" +
    "        <h2>What a record has to carry before it can be ranked</h2>\n" +
    "        <p>A run against your own deployment is useful on its own. Ranking one against other\n" +
    "          people&#39;s records asks for more, and a record that misses any of it stays valid for\n" +
    "          local work while naming what it is missing.</p>\n" +
    "      </div>\n" +
    "      <div class=\"prose\">\n" +
    "        <ul>\n" +
    ([ $manifest.submission_requirements[]
       | "          <li><b>" + (.token | @html) + ":</b> " + (.statement | @html) + "</li>"
     ] | join("\n")) + "\n" +
    "        </ul>\n" +
    "        <p>Both are decided by the tool from the record itself, and CI checks them again\n" +
    "          before a maintainer reads the numbers. The full procedure, including the file\n" +
    "          naming and the append-only rule over merged records, is in\n" +
    "          <a href=\"" + $guide + "\">the submission guide</a>.</p>\n" +
    "        <p>A record for the <a href=\"" + $board + "\">board</a> comes out of one command,\n" +
    "          driving one of the pack versions on this page.</p>\n" +
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
    "      <p class=\"after-code\">To read this page as data instead, the manifest it was\n" +
    "        generated from is <a href=\"./bench-packs.json\">bench-packs.json</a>, and\n" +
    "        <code>veredictum bench-packs --out DIR</code> writes the same document from any\n" +
    "        build.</p>\n" +
    "    </div>\n" +
    "  </section>"
  ' "$MANIFEST"
}

render_page() {
  local body
  body="$(render_body)"
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
       This page carries no <script> at all: every expansion is a <details>
       element and the phase diagrams are inline SVG, neither of which needs
       one. style-src keeps 'unsafe-inline' only because the shared stylesheet's
       siblings use it; this page's own styles all live in style.css. -->
  <meta http-equiv="Content-Security-Policy" content="default-src 'self'; script-src 'self'; style-src 'self' 'unsafe-inline'; img-src 'self' data:; font-src 'self'; connect-src 'self'; object-src 'none'; base-uri 'self'; form-action 'none'; frame-ancestors 'none'">
  <meta name="referrer" content="strict-origin-when-cross-origin">
  <title>Benchmark methodology — Veredictum</title>
  <meta name="description" content="What each Veredictum benchmark pack creates, what it measures, and why. The population, the phases and their load discipline, every request in the mix with what it probes, and the pinned fixtures with their provenance. Generated from the packs the binary embeds.">
  <link rel="canonical" href="https://veredictum.eu/benchmark-methodology.html">
  <link rel="icon" type="image/svg+xml" href="./assets/favicon.svg">
  <link rel="stylesheet" href="./style.css">
  <meta property="og:type" content="website">
  <meta property="og:title" content="Benchmark methodology — Veredictum">
  <meta property="og:description" content="What each openEHR benchmark pack creates, what it measures, and why — generated from the packs the instrument embeds.">
  <meta property="og:url" content="https://veredictum.eu/benchmark-methodology.html">
  <meta property="og:site_name" content="Veredictum">
  <meta property="og:image" content="https://veredictum.eu/assets/veredictum-seal.svg">
  <meta property="og:image:alt" content="The Veredictum conformance seal">
  <meta name="twitter:card" content="summary">
  <meta name="twitter:title" content="Benchmark methodology — Veredictum">
  <meta name="twitter:description" content="What each openEHR benchmark pack creates, what it measures, and why.">
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
      <a href="./conformance-board.html">Conformance</a>
      <a href="./benchmarks.html">Benchmarks</a>
      <a href="./benchmark-methodology.html" aria-current="page">Methodology</a>
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
    <p>This page is generated from <a href="./bench-packs.json">bench-packs.json</a>, which
      the <code>veredictum</code> binary emits from the packs it embeds.</p>
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
    echo "::error::$PAGE is stale — regenerate it with scripts/render/bench-legend.sh" >&2
    exit 1
  fi
  echo "bench-legend: $PAGE matches the emitted pack manifest — OK."
else
  render_page > "$PAGE"
  echo "bench-legend: wrote $PAGE"
fi
