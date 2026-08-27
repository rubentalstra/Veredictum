#!/usr/bin/env bash
# SPDX-FileCopyrightText: Veredictum contributors
# SPDX-License-Identifier: Apache-2.0
# The console's engine pin, checked against the version being published (#128).
#
# The console consumes the engine as a crates.io dependency at an EXACT version
# (`app/veredictum-console/Cargo.toml`), and displays that same version in its
# chrome (`ENGINE_PIN` in `app/veredictum-console/src/lib.rs`). Nothing held
# either to the release: main shipped v0.1.0-alpha.6 while both still read
# alpha.4, so the console consumed a two-release-old engine across two cuts and
# nothing failed.
#
# THE POLICY, and why it is two values rather than one: an exact pin can only
# move to a version that EXISTS on crates.io, and a version exists only after
# its own tag published it. So the pin cannot be the version being published at
# the moment the tag is cut, unless the cut PR pre-bumped it in the same tree.
# Both orderings are legitimate and both are checked:
#
#   1. the pin equals the version being published — a cut that pre-bumped it,
#      the way a content PR pre-bumps the crate version for the release its
#      change exists to ship; or
#   2. the pin equals the immediately previous published version — the normal
#      ordering, where the bump lands in the first PR after publication.
#
# Any third value means the pin trails by more than one release, which is
# exactly the silent staleness this check exists to refuse. The previous
# version is read from `CHANGELOG.md`, the release spine `plan` already
# verifies, so the check needs no tag history and no network.
#
# Usage: scripts/release/check-console-pin.sh <version>
#   <version> is the bare version being published (no leading `v`).
#
# Exit 0 = the pin satisfies the policy, 1 = it does not, 2 = usage.
set -euo pipefail
cd "$(dirname "$0")/../.."

version="${1:-}"
if [[ -z "$version" ]]; then
  echo "usage: $0 <version>   # the bare version being published, no leading v" >&2
  exit 2
fi

CONSOLE_MANIFEST=app/veredictum-console/Cargo.toml
CONSOLE_LIB=app/veredictum-console/src/lib.rs

dep_pin="$(sed -nE 's/^veredictum = \{.*version = "=([^"]+)".*$/\1/p' "$CONSOLE_MANIFEST" | head -1)"
engine_pin="$(sed -nE 's/^pub const ENGINE_PIN: &str = "([^"]+)";$/\1/p' "$CONSOLE_LIB" | head -1)"

if [[ -z "$dep_pin" ]]; then
  echo "::error::could not read the exact engine pin from ${CONSOLE_MANIFEST} — the seam is \`veredictum = { version = \"=X\", … }\` and this check reads that shape" >&2
  exit 1
fi
if [[ -z "$engine_pin" ]]; then
  echo "::error::could not read ENGINE_PIN from ${CONSOLE_LIB}" >&2
  exit 1
fi
if [[ "$dep_pin" != "$engine_pin" ]]; then
  echo "::error::the console's dependency pin (${dep_pin}) and ENGINE_PIN (${engine_pin}) name different engines — they are the same fact and move together" >&2
  exit 1
fi

if ! grep -q "^## \[${version}\]" CHANGELOG.md; then
  echo "::error::CHANGELOG.md has no '## [${version}]' heading, so the version published before it cannot be read. Releases are changelog-driven: add the section in the cut PR." >&2
  exit 1
fi

# The version published immediately before this one: the first release heading
# under this release's own, skipping `[Unreleased]`.
previous="$(awk -v ver="$version" '
  $0 ~ "^## \\[" ver "\\]" { found = 1; next }
  found && /^## \[Unreleased\]/ { next }
  found && match($0, /^## \[([^]]+)\]/) { print substr($0, RSTART + 4, RLENGTH - 5); exit }
' CHANGELOG.md)"

if [[ "$dep_pin" == "$version" ]]; then
  echo "check-console-pin: the console pins ${dep_pin}, the version being published — the cut pre-bumped it. OK."
  exit 0
fi
if [[ -n "$previous" && "$dep_pin" == "$previous" ]]; then
  echo "check-console-pin: the console pins ${dep_pin}, the version published immediately before ${version} — the bump lands after publication. OK."
  exit 0
fi

if [[ -z "$previous" ]]; then
  echo "::error::the console pins ${dep_pin}, but ${version} is the first release in CHANGELOG.md, so no earlier version exists on crates.io and the only value the policy allows is ${version} itself. Pre-bump the console's \`veredictum = \"=X\"\` and ENGINE_PIN in the cut PR." >&2
else
  echo "::error::the console pins ${dep_pin}, which is neither ${version} (a cut that pre-bumped the pin) nor ${previous} (the version published immediately before it). An exact pin can only move to a version crates.io already carries, so at tag time it is one of those two and nothing else — a third value means the console is consuming an engine more than one release old, which is how alpha.4 survived two cuts. Bump \`veredictum = \"=X\"\` in ${CONSOLE_MANIFEST} and ENGINE_PIN in ${CONSOLE_LIB} to ${previous} or ${version}." >&2
fi
exit 1
