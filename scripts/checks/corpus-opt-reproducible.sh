#!/usr/bin/env bash
# SPDX-FileCopyrightText: Veredictum contributors
# SPDX-License-Identifier: Apache-2.0
# Corpus OPT reproducibility guard (issue #241).
#
# `artifacts/corpus/templates/generate_content_opts.py` declares itself the
# SOURCE of every `cnf.tpl.*` operational template beside it; the committed
# `.opt` files are its build product. That contract has two failure modes, and
# this guard is the one place both of them fail loudly:
#
#   1. an OPT whose BYTES drifted from what the script emits, which is how a
#      hand-patch silently deletes a load-bearing constraint at the next re-run;
#   2. a `cnf.tpl.*` manifest key with no builder in the script (or a builder
#      naming a key the manifest does not carry), which leaves a committed OPT
#      unreachable from its own generator. The script exits non-zero on that one
#      itself; this guard is what makes CI read the exit code.
#
# The comparison is a digest of the `.opt` files taken before and after the
# re-run, so it reads the tree as it stands and needs no clean checkout.
# Resolve a difference by deriving which side is right from the vendored specs
# and the case that consumes the template, never by accepting the diff.
set -euo pipefail

root="$(git rev-parse --show-toplevel)"
templates="$root/artifacts/corpus/templates"

digest() {
  find "$templates" -maxdepth 1 -name '*.opt' -print0 | sort -z | xargs -0 shasum -a 256
}

before="$(digest)"
python3 "$templates/generate_content_opts.py" >/dev/null
after="$(digest)"

if [[ "$before" != "$after" ]]; then
  echo "corpus-opt-reproducible: re-running the generator changed OPTs on disk:" >&2
  diff <(printf '%s\n' "$before") <(printf '%s\n' "$after") >&2 || true
  exit 1
fi

echo "corpus-opt-reproducible: every committed OPT regenerates byte-identically."
