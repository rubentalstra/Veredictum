#!/usr/bin/env bash
# SPDX-FileCopyrightText: Veredictum contributors
# SPDX-License-Identifier: Apache-2.0
# Render the public conformance board from the committed registry entries.
#
# The board is a static page with no backend and no state, the same doctrine
# the benchmark board follows: `website/landing/conformance-board.html` is
# generated from `registry/entries/conformance/**/*.json` plus the verdicts
# document each entry pins, and committed, so what the site serves is a file a
# reader can diff against the records it came from. `--check` regenerates into
# a temporary file and fails on any difference, and that check runs in the
# submission gate and again inside the site build.
#
# Every figure on a row comes out of the entry's own `verdicts` artifact, never
# out of a number restated in the entry, so a row cannot claim a result its
# evidence does not carry. Rows carry their tier badge, and the tier is read
# from the entry's provenance block rather than assumed.
#
# The conformance board and the benchmark board are separate surfaces on
# purpose. Ranking speed beside verdicts invites reading one as the other, and
# a fast server that fails the catalogue is a fast server that fails the
# catalogue.
#
# Usage:
#   scripts/render/conformance-board.sh            # write the page
#   scripts/render/conformance-board.sh --check    # fail if the page is stale
set -euo pipefail

cd "$(dirname "$0")/../.."

command -v jq >/dev/null || { echo "jq is required" >&2; exit 1; }

readonly ENTRIES='registry/entries/conformance'
readonly PAGE='website/landing/conformance-board.html'
readonly RULES_URL='https://github.com/rubentalstra/Veredictum/blob/main/registry/RULES.md'
readonly TREE_URL='https://github.com/rubentalstra/Veredictum/tree/main/registry/entries'

MODE="${1:-}"
if [[ -n "$MODE" && "$MODE" != "--check" ]]; then
  echo "usage: $0 [--check]" >&2
  exit 2
fi

# The model: one object per committed conformance entry, pairing the entry with
# the verdicts document it pins, ordered by path so the page is byte-identical
# on any machine.
#
# An absent entries tree is the state of a registry with nothing merged yet, so
# it renders the empty board silently. An entry whose verdicts artifact is
# missing stops the render rather than producing a row with no numbers: the
# submission gate refuses that entry, so reaching it here means the tree is
# already inconsistent.
model_of() {
  if [[ ! -d "$ENTRIES" ]]; then
    printf '[]'
    return 0
  fi
  local listing
  if ! listing="$(find "$ENTRIES" -type f -name '*.json' | sort)"; then
    echo "::error::listing $ENTRIES failed" >&2
    return 1
  fi
  local file verdicts_path model=''
  while IFS= read -r file; do
    [[ -n "$file" ]] || continue
    verdicts_path="$(jq -r '[.artifacts[] | select(.role == "verdicts") | .path] | first // ""' "$file")"
    if [[ -z "$verdicts_path" || ! -f "$verdicts_path" ]]; then
      echo "::error::$file pins no readable verdicts artifact (${verdicts_path:-none})" >&2
      return 1
    fi
    model+="$(jq -c -n --slurpfile entry "$file" --slurpfile verdicts "$verdicts_path" \
      --arg path "$file" '{path: $path, entry: $entry[0], verdicts: $verdicts[0]}')"$'\n'
  done <<<"$listing"
  printf '%s' "$model" | jq -s '.'
}

# The body: every section between <main> and </main>. jq owns it because every
# number on the page is derived from the committed documents, and `@html`
# escapes every value that came out of one.
render_body() {
  jq -r --arg rules "$RULES_URL" --arg tree "$TREE_URL" '
    def pct($n; $d): if $d == 0 then "—" else ($n / $d * 1000 | round / 10 | tostring) + "%" end;

    # Every id some entry declares it replaces. The pointer travels forward
    # only, so the backward edge is derived here rather than written into a
    # published entry nobody may edit.
    def superseded_by:
      . as $all
      | reduce ($all[] | .entry as $e | ($e.supersedes // [])[] | {old: ., new: $e.entry_id})
          as $edge ({}; .[$edge.old] = $edge.new);

    def rows($replaced):
      [ .[]
        | .entry as $e
        | .verdicts as $v
        | ($v.coverage // {}) as $c
        | {
            id: $e.entry_id,
            path: .path,
            system: $e.subject.display_name,
            version: $e.subject.version,
            tier: $e.provenance.tier,
            submitter: $e.submitter.name,
            relationship: $e.submitter.relationship,
            contact: $e.submitter.contact,
            deployment: $e.subject.deployment.kind,
            configuration: $e.disclosure.sut_configuration,
            conflict: $e.disclosure.conflict_of_interest,
            instrument: $e.disclosure.instrument_version,
            catalogue: $e.result.catalogue_revision,
            statement: $e.result.statement,
            measured_on: ($e.disclosure.run_started_at | split("T") | .[0]),
            host: $e.disclosure.environment.host_class,
            selected: ($c.selected // 0),
            driven: ($c.driven // 0),
            passed: ($c.passed // 0),
            failed: ($c.failed // 0),
            inconclusive: ($c.inconclusive // 0),
            profiles: [ ($v.profiles // [])[] | {tier: .[0], verdict: .[1]} ],
            security: ($v.security // null),
            review: [ ($v.review // [])[] | .message // (. | tostring) ],
            artifacts: [ $e.artifacts[] | {role: .role, path: .path, sha256: .sha256} ],
            verify: $e.provenance.verify_command,
            identity: ($e.provenance.identity // $e.provenance.workflow_ref),
            supersedes: ($e.supersedes // []),
            supersede_reason: ($e.supersede_reason // ""),
            superseded_by: ($replaced[$e.entry_id] // null)
          }
      ];

    # The two official tiers first, then the self-reported claim, then by
    # system, then newest first: who performed a run is the first thing a
    # reader needs about a row, so it orders the page. No apostrophe may enter
    # this comment: the whole program is one single-quoted shell argument.
    def ranked:
      sort_by(if .tier == "reproduced" then 0 elif .tier == "console" then 1 else 2 end,
              .system, (.id | explode | map(-.)));

    def badge($tier):
      "<span class=\"tier tier-" + ($tier | @html) + "\">" + ($tier | @html) + "</span>";

    def profile_html($row):
      ([ $row.profiles[]
         | "<span class=\"profile profile-" + (.verdict | @html) + "\"><b>" + (.tier | @html) + "</b> " + (.verdict | @html) + "</span>"
       ] | join(" "))
      + (if $row.security == null then ""
         else " <span class=\"profile profile-" + ($row.security | @html) + "\"><b>SEC-BASIC</b> " + ($row.security | @html) + "</span>"
         end);

    def row_html($row):
      "        <article class=\"board-row" + (if $row.superseded_by == null then "" else " board-row-superseded" end) + "\">\n" +
      "          <div class=\"board-head\">\n" +
      "            <h4>" + ($row.system | @html) + " <span class=\"board-version\">" + ($row.version | @html) + "</span></h4>\n" +
      "            <p class=\"board-meta\">" + badge($row.tier) + " catalogue <code>" + ($row.catalogue | @html) + "</code> · instrument <code>" + ($row.instrument | @html) + "</code> · run " + ($row.measured_on | @html) + "</p>\n" +
      "            <p class=\"board-posture\">Submitted by " + ($row.submitter | @html) + " (" + ($row.relationship | @html) + ") · deployment <code>" + ($row.deployment | @html) + "</code></p>\n" +
      (if $row.superseded_by == null then ""
       else "            <p class=\"board-superseded\">Superseded by <code>" + ($row.superseded_by | @html) + "</code>. This entry stays published exactly as it was accepted.</p>\n"
       end) +
      "          </div>\n" +
      "          <div class=\"board-indices\">\n" +
      "            <div class=\"board-index\"><span class=\"n\">" + pct($row.passed; $row.driven) + "</span><span class=\"l\">of driven cases passed</span></div>\n" +
      "            <div class=\"board-index\"><span class=\"n\">" + ($row.driven | tostring) + " / " + ($row.selected | tostring) + "</span><span class=\"l\">driven of selected</span></div>\n" +
      "          </div>\n" +
      "          <div class=\"board-absolute\">\n" +
      "            <p>" + profile_html($row) + "</p>\n" +
      "            <p class=\"board-machine\">" + ($row.passed | tostring) + " passed · " + ($row.failed | tostring) + " failed · " + ($row.inconclusive | tostring) + " inconclusive</p>\n" +
      "            <p class=\"board-machine\">" + ($row.host | @html) + "</p>\n" +
      "          </div>\n" +
      "          <details class=\"board-detail\">\n" +
      "            <summary>Disclosure and evidence</summary>\n" +
      "            <p class=\"board-provenance\"><b>What was switched on.</b> " + ($row.configuration | @html) + "</p>\n" +
      "            <p class=\"board-provenance\"><b>Declared interest.</b> " + ($row.conflict | @html) + "</p>\n" +
      "            <p class=\"board-provenance\"><b>Statement judged against.</b> <code>" + ($row.statement | @html) + "</code></p>\n" +
      "            <p class=\"board-provenance\"><b>Signed by.</b> <code>" + ($row.identity | @html) + "</code> — check it with <code>" + ($row.verify | @html) + "</code></p>\n" +
      (if ($row.review | length) == 0 then ""
       else "            <p class=\"board-provenance\"><b>Claim review.</b> " + ($row.review | join("; ") | @html) + "</p>\n"
       end) +
      (if ($row.supersedes | length) == 0 then ""
       else "            <p class=\"board-provenance\"><b>Supersedes.</b> " + ($row.supersedes | join(", ") | @html) + " — " + ($row.supersede_reason | @html) + "</p>\n"
       end) +
      "            <div class=\"table-scroll\">\n" +
      "              <table>\n" +
      "                <thead><tr><th scope=\"col\">Artifact</th><th scope=\"col\">Path</th><th scope=\"col\">SHA-256</th></tr></thead>\n" +
      "                <tbody>\n" +
      ([ $row.artifacts[]
         | "                  <tr><td>" + (.role | @html) + "</td><td><code>" + (.path | @html) + "</code></td><td><code>" + (.sha256 | @html) + "</code></td></tr>"
       ] | join("\n")) + "\n" +
      "                </tbody>\n" +
      "              </table>\n" +
      "            </div>\n" +
      "            <p class=\"board-provenance\">Entry: <a href=\"" + $tree + "/conformance\">" + ($row.path | @html) + "</a>.</p>\n" +
      "          </details>\n" +
      "        </article>";

    superseded_by as $replaced |
    (rows($replaced) | ranked) as $rows |
    "  <section class=\"board-intro\">\n" +
    "    <div class=\"wrap\">\n" +
    "      <div class=\"head\">\n" +
    "        <span class=\"eyebrow\">Public conformance board</span>\n" +
    "        <h1>What each openEHR CDR did when the catalogue was driven against it</h1>\n" +
    "        <p>Every row below is a registry entry somebody added by pull request. The entry\n" +
    "          carries the catalogue revision that ran, the statement the claim was judged\n" +
    "          against, the machine, what was switched on, and the artifacts the numbers come\n" +
    "          out of. CI validates all of that before a maintainer looks at it, and the merge\n" +
    "          is the publication.</p>\n" +
    "        <p><a href=\"" + $rules + "\">The submission rules, in full →</a></p>\n" +
    "      </div>\n" +
    "      <div class=\"boundary\">\n" +
    "        <p><b>An entry is a report, never a certificate.</b> It says what happened when one\n" +
    "          version of one system was driven by one version of this instrument. It is not a\n" +
    "          certification, not a mark, and not a statement of fitness for any purpose.\n" +
    "          Certification belongs to openEHR governance, and this registry is shaped to hand\n" +
    "          over: public rules, self-carrying evidence, no proprietary step.</p>\n" +
    "      </div>\n" +
    "    </div>\n" +
    "  </section>\n" +
    "\n" +
    "  <section id=\"board\">\n" +
    "    <div class=\"wrap\">\n" +
    "      <div class=\"head\">\n" +
    "        <span class=\"eyebrow\">The board</span>\n" +
    "        <h2>Three kinds of row, labelled on every one</h2>\n" +
    "        <p>A <span class=\"tier tier-reproduced\">reproduced</span> row was produced by the\n" +
    "          workflow in this repository. It composed the deployment from a recipe committed\n" +
    "          here, drove the catalogue, and attested the artifacts from that workflow identity\n" +
    "          through Sigstore, so no key stands behind it for anyone to steal.</p>\n" +
    "        <p>A <span class=\"tier tier-console\">console</span> row was produced at\n" +
    "          console.veredictum.eu, the official hosted instrument, against an endpoint the\n" +
    "          submitter named. Its verdicts were recomputed here from the transcript it\n" +
    "          submitted, and the record was signed only after they matched. The instrument\n" +
    "          holds no key, and it writes none of the evidence in that block.</p>\n" +
    "        <p>A <span class=\"tier tier-self-reported\">self-reported</span> row was\n" +
    "          produced by the submitter and signed with their own key or identity, and the row\n" +
    "          prints the command that checks it.</p>\n" +
    "        <p>Percentages are taken over cases that were actually driven. A case the claim did\n" +
    "          not select, or that the deployment could not offer, is neither a pass nor a\n" +
    "          failure, so the row prints driven against selected beside the share.</p>\n" +
    "      </div>\n" +
    (if ($rows | length) == 0 then
      "      <p class=\"after-code\">No conformance entry has been merged yet. The first one to\n" +
      "        arrive will be the first row.</p>\n"
    else
      "      <div class=\"board-rows\">\n" +
      ([ $rows[] | row_html(.) ] | join("\n")) + "\n" +
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
    "        <p><b>What a verdict is.</b> The catalogue is a set of small cases, each citing the\n" +
    "          released openEHR section that assigns its expected answer. A run drives them\n" +
    "          against a live deployment and records the exchanges; the verdicts are pure\n" +
    "          functions over those recordings. A failed case names one behaviour, which is what\n" +
    "          makes a red row worth reading.</p>\n" +
    "        <p><b>Inconclusive is not failure.</b> An exchange that errored in transport, or a\n" +
    "          step that could not be resolved, proves nothing about the behaviour under test.\n" +
    "          Those rows are counted separately and never folded into the failure count.</p>\n" +
    "        <p><b>What a profile verdict means.</b> CORE, STANDARD and OPTIONS are capability\n" +
    "          tiers. A tier passes when every capability it requires passed. A tier the party\n" +
    "          never claimed reads <code>not_claimed</code> rather than failing.</p>\n" +
    "        <p><b>Why the disclosure matters.</b> Two runs of the same catalogue against the\n" +
    "          same product can differ because the deployments differ. Every row states what was\n" +
    "          switched on, which statement the claim was judged against, and which interest the\n" +
    "          submitter holds in the outcome.</p>\n" +
    "        <p><b>Superseded rows stay.</b> Entries are append-only. A correction is a new entry\n" +
    "          naming the one it replaces, and the replaced row keeps its place with a pointer\n" +
    "          forward, so the record of what was published never quietly changes.</p>\n" +
    "      </div>\n" +
    "    </div>\n" +
    "  </section>\n" +
    "\n" +
    "  <section class=\"quickstart\" id=\"submit\">\n" +
    "    <div class=\"wrap\">\n" +
    "      <div class=\"head\">\n" +
    "        <span class=\"eyebrow\">Submit your CDR</span>\n" +
    "        <h2>Run the catalogue, open a pull request</h2>\n" +
    "        <p>Point the instrument at your deployment through an IXIT that describes its\n" +
    "          topology, judge the results against your statement, and commit the entry beside\n" +
    "          the artifacts it stands on.</p>\n" +
    "      </div>\n" +
    "      <figure class=\"code\">\n" +
    "        <figcaption>One run, one judgement, one entry</figcaption>\n" +
    "        <pre><code><span class=\"p\">$</span> veredictum run --root artifacts --ixit ./my-ixit.json \\\n" +
    "      --sut-name my-cdr --sut-version 1.2.3 \\\n" +
    "      --statement ./my-statement.json --out ./run\n" +
    "<span class=\"p\">$</span> veredictum verdicts --statement ./my-statement.json \\\n" +
    "      --results ./run/results.json --root artifacts --out ./judgement</code></pre>\n" +
    "      </figure>\n" +
    "      <p class=\"after-code\">Then copy <code>results.json</code> and <code>verdicts.json</code>\n" +
    "        into <code>registry/records/&lt;system&gt;/&lt;entry-id&gt;/</code>, sign one of them, write\n" +
    "        the entry under <code>registry/entries/conformance/&lt;system&gt;/</code> and open a pull\n" +
    "        request. <a href=\"" + $rules + "\">The submission rules, in full →</a></p>\n" +
    "    </div>\n" +
    "  </section>"
  ' <<<"$1"
}

render_page() {
  # The model is taken in its own step, because bash does not propagate the
  # failure of a command substitution NESTED inside another one: an entry whose
  # evidence is missing would otherwise render an empty board instead of
  # stopping.
  local model body
  model="$(model_of)"
  body="$(render_body "$model")"
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
       <details> element, which needs none. -->
  <meta http-equiv="Content-Security-Policy" content="default-src 'self'; script-src 'self'; style-src 'self' 'unsafe-inline'; img-src 'self' data:; font-src 'self'; connect-src 'self'; object-src 'none'; base-uri 'self'; form-action 'none'; frame-ancestors 'none'">
  <meta name="referrer" content="strict-origin-when-cross-origin">
  <title>Conformance board — Veredictum</title>
  <meta name="description" content="Published conformance results for openEHR clinical data repositories, each entry carrying its own evidence and its verification tier. An entry is a report, never a certificate.">
  <link rel="canonical" href="https://veredictum.eu/conformance-board.html">
  <link rel="icon" type="image/svg+xml" href="./assets/favicon.svg">
  <link rel="stylesheet" href="./style.css">
  <meta property="og:type" content="website">
  <meta property="og:title" content="Conformance board — Veredictum">
  <meta property="og:description" content="Published conformance results for openEHR clinical data repositories, each entry carrying its own evidence and its verification tier.">
  <meta property="og:url" content="https://veredictum.eu/conformance-board.html">
  <meta property="og:site_name" content="Veredictum">
  <meta property="og:image" content="https://veredictum.eu/assets/veredictum-seal.svg">
  <meta property="og:image:alt" content="The Veredictum conformance seal">
  <meta name="twitter:card" content="summary">
  <meta name="twitter:title" content="Conformance board — Veredictum">
  <meta name="twitter:description" content="Published conformance results for openEHR clinical data repositories, each entry carrying its own evidence and its verification tier.">
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
      <a href="./conformance-board.html" aria-current="page">Conformance</a>
      <a href="./benchmarks.html">Benchmarks</a>
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
    <p>This page is generated from the entries committed under
      <a href="$TREE_URL" rel="noopener">registry/entries</a> and is regenerated
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
    echo "::error::$PAGE is stale — regenerate it with scripts/render/conformance-board.sh" >&2
    exit 1
  fi
  echo "conformance-board: $PAGE matches the committed entries — OK."
else
  render_page > "$PAGE"
  echo "conformance-board: wrote $PAGE"
fi
