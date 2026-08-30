#!/usr/bin/env bash
# SPDX-FileCopyrightText: Veredictum contributors
# SPDX-License-Identifier: Apache-2.0
# Commits the sealed record onto the submission branch, through the Git Data
# API (#392).
#
# The API path is not a preference. A commit written through the contents API
# lands UNVERIFIED, and an unverified commit is not an acceptable way to write
# to this repository (root CLAUDE.md, hard rule 9); a commit built
# blob → tree → commit → ref is signed by GitHub as the acting identity. And
# `git commit` is out for the same reason: the runner holds no signing key,
# and the one key this lane touches signs records rather than commits.
#
# Everything it writes was produced by `complete-console-entry.sh` in the
# working tree: the sealed documents, the manifest, the detached signature, the
# entry's provenance block, and the regenerated board.
#
# Environment:
#   GH_TOKEN    a token that may push to the submission branch
#   REPO        owner/name
#   PR          the submission pull request number
#   HEAD_SHA    the commit the submission is at, which this commit's parent is
set -euo pipefail

cd "$(dirname "$0")/../.."

: "${GH_TOKEN:?a token is required}"
: "${REPO:?owner/name is required}"
: "${PR:?the pull request number is required}"
: "${HEAD_SHA:?the submission head sha is required}"

readonly MESSAGE='chore(registry): seal the console record and write its provenance'

mapfile -t changed < <(git status --porcelain -- registry website | awk '{print $2}')
if [[ ${#changed[@]} -eq 0 ]]; then
  echo "::error::the seal produced no change at all, which means nothing was signed" >&2
  exit 1
fi

branch="$(gh api "repos/$REPO/pulls/$PR" --jq '.head.ref')"
base_tree="$(gh api "repos/$REPO/git/commits/$HEAD_SHA" --jq '.tree.sha')"

# One blob per changed file, then one tree over them. Base64 because a record
# carries an armored signature and rendered documents, and the API's `utf-8`
# encoding would mangle anything that is not text.
tree_entries='[]'
for file in "${changed[@]}"; do
  blob="$(base64 < "$file" | tr -d '\n' \
    | gh api "repos/$REPO/git/blobs" -f encoding=base64 -f content=@- --jq '.sha')"
  tree_entries="$(jq -c --arg path "$file" --arg sha "$blob" \
    '. + [{path: $path, mode: "100644", type: "blob", sha: $sha}]' <<<"$tree_entries")"
  echo "sealed-record: $file -> blob $blob"
done

tree="$(jq -n --arg base "$base_tree" --argjson tree "$tree_entries" \
  '{base_tree: $base, tree: $tree}' \
  | gh api "repos/$REPO/git/trees" --input - --jq '.sha')"

commit="$(jq -n --arg message "$MESSAGE" --arg tree "$tree" --arg parent "$HEAD_SHA" \
  '{message: $message, tree: $tree, parents: [$parent]}' \
  | gh api "repos/$REPO/git/commits" --input - --jq '.sha')"

gh api "repos/$REPO/git/refs/heads/$branch" -X PATCH -F sha="$commit" -F force=false >/dev/null

verified="$(gh api "repos/$REPO/git/commits/$commit" --jq '.verification.verified')"
if [[ "$verified" != "true" ]]; then
  echo "::error::$commit landed unverified — a commit this repository accepts is signed, and the Git Data API path is what signs it" >&2
  exit 1
fi
echo "sealed-record: $branch now carries $commit (verified)"
