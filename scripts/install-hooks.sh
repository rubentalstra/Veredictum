#!/usr/bin/env bash
# SPDX-FileCopyrightText: Veredictum contributors
# SPDX-License-Identifier: Apache-2.0
# scripts/install-hooks.sh
#
# Installs the repository's tracked git hooks by pointing git at .githooks/.
# Run once after cloning:  bash scripts/install-hooks.sh
#
# core.hooksPath is used rather than .git/hooks so the hooks are version
# controlled and shared by everyone who clones.

set -euo pipefail

repo_root="$(git rev-parse --show-toplevel)"
cd "$repo_root"

chmod +x .githooks/* 2>/dev/null || true
git config core.hooksPath .githooks

echo "core.hooksPath set to .githooks"
echo "Installed hooks:"
ls -1 .githooks
