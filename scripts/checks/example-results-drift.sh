#!/usr/bin/env bash
# SPDX-FileCopyrightText: Veredictum contributors
# SPDX-License-Identifier: Apache-2.0
# The committed example results document is what its generator writes (#466).
#
# `app/veredictum/examples/results.example.json` is produced by
# `cargo run --example make_example_results`, and nothing held the committed
# copy to the generator. It carried `"version": "0.1.1"` while the crate was at
# `0.1.4`, silently, and it would drift again at every release. The published
# schema set has `schema_drift` in the Rust suite for exactly this property; a
# cargo example cannot have one, because it is a binary no integration test can
# import (the Book, ch11.3), so the equivalent gate is this script.
#
# It renders the document to a temporary path — the generator takes one optional
# argument for that — and byte-compares the committed copy, so a check never
# writes into the tree.
#
# Usage: scripts/checks/example-results-drift.sh
set -euo pipefail
cd "$(dirname "$0")/../.."

EXAMPLE=app/veredictum/examples/results.example.json

rendered="$(mktemp)"
trap 'rm -f "$rendered"' EXIT

cargo run --quiet --locked --example make_example_results -- "$rendered"

if ! diff -u "$EXAMPLE" "$rendered" --label "$EXAMPLE (committed)" --label "$EXAMPLE (generated)"; then
  echo "::error::${EXAMPLE} is stale — regenerate it with \`cargo run --example make_example_results\` and commit the result" >&2
  exit 1
fi

echo "example-results-drift: ${EXAMPLE} is byte-identical to its generator's output — OK."
