#!/usr/bin/env bash
# SPDX-FileCopyrightText: Veredictum contributors
# SPDX-License-Identifier: Apache-2.0
# The console's engine pin is ONE value: the workspace engine version (#179).
#
# The console consumes the engine as a crates.io dependency at an EXACT version
# (`app/veredictum-console/Cargo.toml`) and displays that same version in its
# chrome (`ENGINE_PIN` in `app/veredictum-console/src/lib.rs`). The root
# manifest's `[patch.crates-io]` redirects that name to `app/veredictum` for
# every build made inside this repository, so the pin may name the version
# being cut before crates.io carries it. That removes the old two-value policy
# (#128): the pin names the workspace version and nothing else.
#
# The patch only APPLIES while the pin equals the workspace version. Diverge
# them and cargo resolves the registry copy instead, silently, with no warning
# and no error (verified on cargo 1.97.1) — the console then links one engine
# while every gate spawns another, which is the drift #179 exists to kill. So
# this check reads four facts and refuses any disagreement:
#
#   1. the console's exact dependency pin,
#   2. `ENGINE_PIN` in the console's lib,
#   3. `[package] version` of `app/veredictum`,
#   4. and `Cargo.lock`, which must carry NO registry-sourced `veredictum` —
#      the byte-level proof that the patch took.
#
# Usage: scripts/release/check-console-pin.sh [version]
#   With no argument (the every-pull-request call) the three versions must
#   agree. With <version> — the bare version being published, no leading `v` —
#   they must all equal it as well.
#
# Exit 0 = the pin satisfies the policy, 1 = it does not, 2 = usage.
set -euo pipefail
cd "$(dirname "$0")/../.."

if [[ $# -gt 1 ]]; then
  echo "usage: $0 [version]   # the bare version being published, no leading v" >&2
  exit 2
fi
version="${1:-}"

CONSOLE_MANIFEST=app/veredictum-console/Cargo.toml
CONSOLE_LIB=app/veredictum-console/src/lib.rs
ENGINE_MANIFEST=app/veredictum/Cargo.toml
ROOT_MANIFEST=Cargo.toml
LOCK=Cargo.lock

dep_pin="$(sed -nE 's/^veredictum = \{.*version = "=([^"]+)".*$/\1/p' "$CONSOLE_MANIFEST" | head -1)"
engine_pin="$(sed -nE 's/^pub const ENGINE_PIN: &str = "([^"]+)";$/\1/p' "$CONSOLE_LIB" | head -1)"
# The `[package]` table's own version, never the first `version = ` line: the
# manifest carries dependency versions too.
engine_version="$(awk -F'"' '/^\[package\]/{p=1} p && /^version = /{print $2; exit}' "$ENGINE_MANIFEST")"

if [[ -z "$dep_pin" ]]; then
  echo "::error::could not read the exact engine pin from ${CONSOLE_MANIFEST} — the seam is \`veredictum = { version = \"=X\", … }\` and this check reads that shape" >&2
  exit 1
fi
if [[ -z "$engine_pin" ]]; then
  echo "::error::could not read ENGINE_PIN from ${CONSOLE_LIB}" >&2
  exit 1
fi
if [[ -z "$engine_version" ]]; then
  echo "::error::could not read the [package] version from ${ENGINE_MANIFEST}" >&2
  exit 1
fi

if [[ "$dep_pin" != "$engine_pin" ]]; then
  echo "::error::the console's dependency pin (${dep_pin}) and ENGINE_PIN (${engine_pin}) name different engines — they are the same fact and move together" >&2
  exit 1
fi

if [[ "$dep_pin" != "$engine_version" ]]; then
  echo "::error::the console pins ${dep_pin} while ${ENGINE_MANIFEST} is at ${engine_version}. The pin is ONE value, the workspace engine version: the root manifest's [patch.crates-io] only redirects the console's dependency to app/veredictum while the two agree, and cargo falls back to the registry copy SILENTLY when they do not. Move \`veredictum = \"=X\"\` in ${CONSOLE_MANIFEST} and ENGINE_PIN in ${CONSOLE_LIB} to ${engine_version}, and run \`cargo check\` so ${LOCK} follows." >&2
  exit 1
fi

# The patch entry itself: without it every value above can agree and the build
# still resolves crates.io.
if ! grep -qE '^veredictum = \{ path = "app/veredictum" \}$' "$ROOT_MANIFEST"; then
  echo "::error::${ROOT_MANIFEST} carries no \`[patch.crates-io]\` redirect for veredictum, so the console's exact pin resolves against crates.io and an in-tree build proves nothing about the engine it links" >&2
  exit 1
fi

# The proof the patch actually took. A patched dependency is a path entry with
# no `source`; a registry-sourced entry means cargo resolved the console's pin
# against crates.io, which is the silent fallback above.
if grep -A2 '^name = "veredictum"$' "$LOCK" | grep -q '^source = "registry'; then
  echo "::error::${LOCK} carries a registry-sourced \`veredictum\` entry, so the [patch.crates-io] redirect went unused and the console links a published engine rather than this tree's. Re-run \`cargo check\` after making the pin and ${ENGINE_MANIFEST} agree." >&2
  exit 1
fi

if [[ -n "$version" && "$dep_pin" != "$version" ]]; then
  echo "::error::the console pins ${dep_pin}, but the version being published is ${version}. The tag, ${ENGINE_MANIFEST}, the console's \`veredictum = \"=X\"\` and ENGINE_PIN are one value at tag time — the cut PR moves all of them together." >&2
  exit 1
fi

if [[ -n "$version" ]]; then
  echo "check-console-pin: the console pins ${dep_pin}, which is the workspace engine version and the version being published. OK."
else
  echo "check-console-pin: the console pins ${dep_pin}, which is the workspace engine version, and ${LOCK} resolves it to this tree. OK."
fi
