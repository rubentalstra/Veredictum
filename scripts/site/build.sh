#!/usr/bin/env bash
# SPDX-FileCopyrightText: Veredictum contributors
# SPDX-License-Identifier: Apache-2.0
#
# Assemble the Pages site tree into ./_site, exactly as the Docs workflow does,
# so a local build and the deployed layout are the same thing: landing page at
# the root, the mdBook under /docs/, plus CNAME, sitemap and robots.
#
# Preview it with any static server, e.g. `npx --yes serve _site`.
#
# There is no per-release version freeze here. The site serves one documentation
# tree, because the instrument has no released docs history worth pinning yet.
# Version freezing arrives with a real docs-versioning need, and it arrives as
# its own change rather than as machinery standing idle.
set -euo pipefail
cd "$(dirname "$0")/../.."
ROOT="$PWD"
OUT="$ROOT/_site"

# The site is served from the veredictum.eu apex (the GitHub Pages custom
# domain), so it lives at the domain ROOT and SITE_BASE is empty. Both stay
# overridable so a sub-path build (a fork's project-pages URL) is one env var
# away.
SITE_ORIGIN="${SITE_ORIGIN:-https://veredictum.eu}"
SITE_BASE="${SITE_BASE:-}"
SITE_DOMAIN="${SITE_ORIGIN#https://}"
SITE_DOMAIN="${SITE_DOMAIN#http://}"

log() { printf '\033[1;36m[build-site]\033[0m %s\n' "$*"; }

# 1. Clean and recreate _site.
rm -rf "$OUT"
mkdir -p "$OUT"

# 2. The benchmark board is generated from the committed submissions and
#    committed itself, so the deploy copies a reviewed file rather than a file
#    that first exists at deploy time. What the build owes is the guarantee
#    that the committed page still matches the records: --check regenerates and
#    refuses any difference, so a merged submission cannot be served stale.
log "benchmark board (committed page vs the committed records)"
bash "$ROOT/scripts/render/bench-board.sh" --check

# 3. The landing page at the site root. Its HTML uses relative URLs only, so
#    there is no base-path rewriting to do.
log "landing -> $OUT"
cp -R "$ROOT/website/landing/." "$OUT/"

# 4. The book at /docs/.
log "book -> $OUT/docs  (site-url ${SITE_BASE}/docs/)"
MDBOOK_OUTPUT__HTML__SITE_URL="${SITE_BASE}/docs/" \
  mdbook build "$ROOT/website/book" -d "$OUT/docs" >/dev/null

# 5. CNAME. GitHub Pages reads this file out of the DEPLOYED ARTIFACT, so the
#    custom domain has to be written by the build; a CNAME that exists only in
#    the repository is never seen by a workflow deployment.
if [[ -n "$SITE_BASE" ]]; then
  log "SITE_BASE is set ($SITE_BASE) — a sub-path build serves no apex domain, skipping CNAME"
else
  log "CNAME -> $SITE_DOMAIN"
  printf '%s\n' "$SITE_DOMAIN" > "$OUT/CNAME"
fi

# 6. sitemap.xml — the landing page, the benchmark board, and every built book
#    page.
log "sitemap.xml"
{
  echo '<?xml version="1.0" encoding="UTF-8"?>'
  echo '<urlset xmlns="http://www.sitemaps.org/schemas/sitemap/0.9">'
  host="${SITE_ORIGIN}${SITE_BASE}"
  echo "  <url><loc>${host}/</loc></url>"
  echo "  <url><loc>${host}/benchmarks.html</loc></url>"
  # print.html is the book's single-page render of pages already listed, and
  # 404.html is not a destination; both stay out of the sitemap.
  find "$OUT/docs" -name '*.html' \
    ! -name 'print.html' ! -name '404.html' | sort | while IFS= read -r f; do
    rel="${f#"$OUT"}"
    echo "  <url><loc>${host}${rel}</loc></url>"
  done
  echo '</urlset>'
} > "$OUT/sitemap.xml"

log "site assembled at $OUT"
