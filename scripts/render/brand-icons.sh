#!/usr/bin/env bash
# SPDX-FileCopyrightText: Veredictum contributors
# SPDX-License-Identifier: Apache-2.0
# Render the raster icon set from the brand originals (#84).
#
# The mark has exactly one source per variant, both under `assets/brand/`:
# `favicon.svg` (ground + V-check, the small-size master) and
# `veredictum-icon.svg` (the full seal). Every raster below is DERIVED from
# one of those two, so the mark cannot fork between the console's tab icon,
# a home-screen icon, and the seal in the sidebar. The console's `public/`
# directory symlinks the results rather than copying them, the same way
# `seal.svg` already does.
#
# The rasters are committed: a browser asks for `/favicon.ico` on every load
# whether or not one is declared, and the console's browser journeys read the
# resulting 404 as a page error. Rendering at serve time would put a
# rasterizer in the container image for four files that never change.
#
# Tools: `rsvg-convert` (librsvg) rasterizes, ImageMagick's `magick` packs the
# multi-size `.ico`. Both are developer-machine tools; nothing in CI or in the
# image runs this script.
#
# Usage:
#   scripts/render/brand-icons.sh
set -Eeuo pipefail

ROOT="$(cd "$(dirname "$0")/../.." && pwd)"
BRAND="$ROOT/assets/brand"

for tool in rsvg-convert magick; do
  if ! command -v "$tool" >/dev/null 2>&1; then
    echo "brand-icons: $tool is not installed" >&2
    exit 1
  fi
done

# The ground the seal is struck on (`tokens.css`), so a square app icon is
# full-bleed teal rather than a circle floating on the launcher's wallpaper.
readonly GROUND="#1B6E92"

render() {
  local source="$1" size="$2" out="$3"
  shift 3
  rsvg-convert --width "$size" --height "$size" "$@" \
    --output "$BRAND/$out" "$BRAND/$source"
  echo "rendered $out (${size}px)"
}

# The tab icon: the small-size master, transparent outside the disc, at the
# three sizes an `.ico` is expected to carry.
render favicon.svg 16 favicon-16.png
render favicon.svg 32 favicon-32.png
render favicon.svg 48 favicon-48.png
magick "$BRAND/favicon-16.png" "$BRAND/favicon-32.png" "$BRAND/favicon-48.png" \
  "$BRAND/favicon.ico"
echo "packed favicon.ico (16/32/48)"

# The home-screen icons: iOS composites an apple-touch icon onto white and
# rounds it itself, so this one carries the ground.
render favicon.svg 180 apple-touch-icon.png --background-color "$GROUND"
render veredictum-icon.svg 192 icon-192.png
render veredictum-icon.svg 512 icon-512.png
