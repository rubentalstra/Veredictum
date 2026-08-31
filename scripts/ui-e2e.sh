#!/usr/bin/env bash
# SPDX-FileCopyrightText: Veredictum contributors
# SPDX-License-Identifier: Apache-2.0
# The console E2E harness (#69): build the console, serve it over the
# repository's own mounts, start a browser endpoint, run the journeys, tear
# everything down.
#
# Usage:
#   scripts/ui-e2e.sh [FILTER]      # FILTER = a nextest -E test(...) substring
#
# The browser: ONE primary path, a digest-pinned `selenium/standalone-chromium`
# container, because it locks the Chromium and driver versions to bytes rather
# than to whatever the host happens to have installed. Its browser lives in the
# container, so the console binds every interface for the run and the journeys
# reach it at `host.docker.internal`. THE ALTERNATIVE: set UI_E2E_CHROMEDRIVER
# to a local `chromedriver` binary, which needs no container and keeps the
# console on loopback, at the cost of a host Chrome whose version has to match
# that driver. Both were verified against the journeys; the container is the
# default because CI must not depend on a runner image's browser pins.
#
# Env:
#   UI_E2E_CHROMEDRIVER   path to a local chromedriver; selects the loopback
#                         alternative described above instead of the container.
#   UI_E2E_DOCS_SHOTS     when set, the journeys also write the book's
#                         screenshots into website/book/src/console/img, and
#                         the console is served in capture mode: the facts one
#                         run stamps (its clock, the record digest, the signing
#                         time) render as fixed stand-ins, so a pass over an
#                         unchanged console rewrites no committed image. The
#                         run's own record, manifest and signature are real.
#   UI_E2E_NO_BUILD       skip the builds and reuse the binaries the last
#                         harness run copied into target/ui-e2e, plus
#                         target/site as it stands.
#   UI_E2E_KEEP_UP        skip teardown (local debugging).
#   UI_E2E_PORT           the console's port (default 3300).
#   UI_E2E_REAL_SUTS      when set, compose the two real CDRs — FerroEHR's own
#                         quickstart (its published docker-compose.yml, basic
#                         auth ferroehr/ferroehr on 8080) and EHRbase's
#                         official pairing (docker/e2e-ehrbase.yml, basic auth
#                         defaults on 8090) — and run the two-CDR journey
#                         against them (#99). Both are deliberately `:latest`
#                         (owner ruling on #99): the SUT is the thing being
#                         graded, not a supply-chain input.
#
# The journeys themselves read UI_E2E_BASE_URL, UI_E2E_WEBDRIVER_URL,
# UI_E2E_SHOTS_DIR and UI_E2E_DOCS_SHOTS, and skip with a printed reason when
# the first two are unset — so a plain `cargo nextest run` stays green.
#
# The console is served with VEREDICTUM_CLIENT_IP_HEADER set (#389), which is
# how a proxied deployment names the real client address. It is what lets the
# concurrency journey be a SECOND visitor beside the browser: the browser
# sends no such header and keeps its socket peer, while the journey's own HTTP
# calls claim an address of their own. The journeys learn the header's name
# and the console's host-side URL from UI_E2E_CLIENT_IP_HEADER and
# UI_E2E_HOST_URL, and skip the concurrency journey when either is unset.
#
# It is also served with a FICTIONAL registry App identity (#391), so the
# submission screen renders its ready state instead of the unconfigured one.
# The key is this repository's committed RSA test key, which holds no account,
# and no journey opens a submission.
set -Eeuo pipefail

FILTER="${1:-}"
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT"

# Digest-pinned, and the digest is the multi-architecture INDEX, so amd64 CI
# and an arm64 workstation resolve the same release: Selenium standalone on
# Ubuntu 24.04 carrying Chromium 151.0.7922.108 and its matching driver.
readonly SELENIUM_IMAGE="selenium/standalone-chromium@sha256:1d3d834a2ce93f26cc0d0ae3c61abd189755b32649f5c356c6c5cf9502aa397e"
readonly SELENIUM_NAME="veredictum-ui-e2e"

# The forwarded-client-address header this harness trusts. Any name works; the
# hosted deployment sets Fly-Client-IP, and the console reads only the header
# the operator names.
readonly CLIENT_IP_HEADER="X-Console-Client-Ip"

PORT="${UI_E2E_PORT:-3300}"
DRIVER_PORT=9515
SHOTS_DIR="$ROOT/target/ui-e2e/screenshots"
mkdir -p "$SHOTS_DIR"

if [[ -n "${UI_E2E_CHROMEDRIVER:-}" ]]; then
  CONSOLE_ADDR="127.0.0.1:$PORT"
  CONSOLE_URL="http://127.0.0.1:$PORT"
else
  # The browser is in a container; loopback there is the container itself.
  CONSOLE_ADDR="0.0.0.0:$PORT"
  CONSOLE_URL="http://host.docker.internal:$PORT"
fi
PROBE_URL="http://127.0.0.1:$PORT"
DRIVER_URL="http://127.0.0.1:$DRIVER_PORT"

# ── Working-tree residue guard ──────────────────────────────────────────────
# A harness run must not change tracked files. The capture pass is the one
# legitimate writer (it regenerates website/book/src/console/img), so it is
# excluded from the comparison; a pre-existing dirty tree is fine, NEW residue
# is not. Off a checkout both samples come back empty and the comparison is
# trivially satisfied.
git_tree_state() {
  git -C "$ROOT" status --porcelain -- . ':(exclude)website/book/src/console/img' 2>/dev/null || true
}
TREE_STATE_BEFORE="$(git_tree_state)"

CONSOLE_PID=""
DRIVER_PID=""
SUTS_UP=""
cleanup() {
  [[ -n "${UI_E2E_KEEP_UP:-}" ]] && return 0
  [[ -n "$CONSOLE_PID" ]] && kill "$CONSOLE_PID" 2>/dev/null || true
  [[ -n "$DRIVER_PID" ]] && kill "$DRIVER_PID" 2>/dev/null || true
  docker rm -f "$SELENIUM_NAME" >/dev/null 2>&1 || true
  if [[ -n "$SUTS_UP" ]]; then
    docker compose -p veredictum-e2e-ferroehr \
      -f "$ROOT/target/ui-e2e/suts/ferroehr.yml" down -v >/dev/null 2>&1 || true
    docker compose -p veredictum-e2e-ehrbase \
      -f "$ROOT/docker/e2e-ehrbase.yml" down -v >/dev/null 2>&1 || true
  fi
  return 0
}
trap cleanup EXIT

wait_http() { # url, tries, what
  local url="$1" tries="${2:-90}" what="${3:-$1}"
  for _ in $(seq 1 "$tries"); do
    if curl -sf -o /dev/null "$url"; then return 0; fi
    sleep 1
  done
  echo "FATAL: $what never answered at $url" >&2
  return 1
}

# ── 1. The console under test ───────────────────────────────────────────────
# The server runs from a COPY, because the journeys' own `cargo nextest`
# compile rebuilds the bin target and overwrites target/debug/veredictum-console
# mid-run. The two builds are not interchangeable: a plain cargo build has no
# LEPTOS_OUTPUT_NAME in its environment, so leptos emits a bootstrap naming
# `<name>_bg.wasm` while cargo-leptos ships `<name>.<hash>.wasm`, and the only
# symptom is a page that loads and never hydrates. That is also why UI_E2E_NO_BUILD
# reuses these copies rather than target/debug: any cargo command between two
# harness runs replaces target/debug/veredictum-console with such a build.
CONSOLE_BIN="$ROOT/target/ui-e2e/veredictum-console"
ENGINE_BIN="$ROOT/target/ui-e2e/veredictum"
mkdir -p "$ROOT/target/ui-e2e"

if [[ -z "${UI_E2E_NO_BUILD:-}" ]]; then
  echo "── building the console (cargo leptos build)"
  # The Tailwind pin is single-sourced from the image build, so the harness
  # and the shipped bundle cannot style differently.
  TAILWIND_VERSION="$(grep -E '^ARG TAILWIND_VERSION=' docker/Dockerfile | head -1 | cut -d= -f2)"
  (cd app/veredictum-console && LEPTOS_TAILWIND_VERSION="$TAILWIND_VERSION" cargo leptos build)
  for artifact in "$ROOT/target/debug/veredictum-console" "$ROOT/target/site/pkg" \
                  "$ROOT/target/debug/hash.txt"; do
    [[ -e "$artifact" ]] || { echo "FATAL: $artifact is missing after the build" >&2; exit 1; }
  done
  cp "$ROOT/target/debug/veredictum-console" "$CONSOLE_BIN"
else
  for artifact in "$CONSOLE_BIN" "$ROOT/target/site/pkg" "$ROOT/target/debug/hash.txt"; do
    [[ -e "$artifact" ]] \
      || { echo "FATAL: $artifact is missing — run once without UI_E2E_NO_BUILD" >&2; exit 1; }
  done
fi

# The driven-run journey spawns the instrument itself, and the console only
# ever runs the engine version it PINS. The pin IS the workspace engine version
# (#179), so the harness always has an engine both sides understand — and it
# refuses to run at all rather than driving nothing when that stops holding.
echo "── the console's engine pin names this tree's engine"
bash scripts/release/check-console-pin.sh
if [[ -z "${UI_E2E_NO_BUILD:-}" ]]; then
  echo "── building the engine (cargo build -p veredictum)"
  cargo build --locked -p veredictum --bin veredictum
  [[ -e "$ROOT/target/debug/veredictum" ]] \
    || { echo "FATAL: target/debug/veredictum is missing after the build" >&2; exit 1; }
  cp "$ROOT/target/debug/veredictum" "$ENGINE_BIN"
else
  [[ -e "$ENGINE_BIN" ]] \
    || { echo "FATAL: $ENGINE_BIN is missing — run once without UI_E2E_NO_BUILD" >&2; exit 1; }
fi
# An arm64 macOS binary carries an ad-hoc signature that a copy invalidates,
# and the kernel then SIGKILLs the copy at exec. Re-signing is a no-op
# elsewhere, because `codesign` exists only on macOS.
if command -v codesign >/dev/null; then
  for bin in "$CONSOLE_BIN" "$ENGINE_BIN"; do
    codesign --force --sign - "$bin" >/dev/null 2>&1 \
      || echo "warning: could not re-sign $bin" >&2
  done
fi

# The console's output mount is this harness's own scratch, and it is emptied
# before every pass. A driven run's job directories and the bench page's
# uploaded batches both survive a pass — the bench sweep keeps a batch for an
# hour — so a second pass listed each uploaded record twice and the benchmark
# captures grew a row per run. The console's own TTL is a production
# behaviour and stays untouched; what is reset is the directory the harness
# created.
rm -rf "$ROOT/target/ui-e2e/out"
mkdir -p "$ROOT/target/ui-e2e/out"

echo "── serving the console on $CONSOLE_ADDR over the repository mounts"
# The bundle carries content-hashed names (#450) and the server reads them back
# from the hash file cargo-leptos wrote beside the binary it built. The copy
# served here runs from target/ui-e2e, so the file is named by absolute path
# rather than found next to the running executable.
# The mounts are RELATIVE and the process runs from the repository root: the
# landing renders them verbatim, and an absolute path would put whoever ran the
# capture pass into a committed documentation screenshot.
LEPTOS_SITE_ROOT="$ROOT/target/site" \
LEPTOS_SITE_ADDR="$CONSOLE_ADDR" \
LEPTOS_OUTPUT_NAME="veredictum-console" \
LEPTOS_HASH_FILES="true" \
LEPTOS_HASH_FILE_NAME="$ROOT/target/debug/hash.txt" \
VEREDICTUM_ROOT="artifacts" \
VEREDICTUM_SPECS="specs/openehr" \
VEREDICTUM_OUT="target/ui-e2e/out" \
VEREDICTUM_ENGINE="$ENGINE_BIN" \
VEREDICTUM_SIGN_KEY="artifacts/corpus/keys/cnf-signing.sec.asc" \
VEREDICTUM_VERIFY_KEY="artifacts/corpus/keys/cnf-signing.pub.asc" \
VEREDICTUM_CAPTURE_MODE="${UI_E2E_DOCS_SHOTS:-}" \
VEREDICTUM_CLIENT_IP_HEADER="$CLIENT_IP_HEADER" \
VEREDICTUM_GITHUB_APP_ID="1234567" \
VEREDICTUM_GITHUB_APP_KEY="party/smart/cnf-smart-test.key.pem" \
VEREDICTUM_GITHUB_INSTALLATION_ID="89012345" \
VEREDICTUM_REGISTRY_REPO="rubentalstra/Veredictum" \
  "$CONSOLE_BIN" &
CONSOLE_PID=$!
wait_http "$PROBE_URL/healthz" 90 "the console"

# The bootstrap must name a bundle the site tree actually carries. When it does
# not, every page loads and silently never hydrates, and the journeys report a
# 60-second wait rather than the mismatch — so the mismatch is checked here,
# once, where it can name its own cause.
WASM_PATH="$(curl -sf "$PROBE_URL/" | grep -oE 'href="/pkg/[^"]+\.wasm"' | head -1 | sed -E 's/^href="//; s/"$//')"
if [[ -z "$WASM_PATH" ]] || ! curl -sf -o /dev/null "$PROBE_URL$WASM_PATH"; then
  echo "FATAL: the served bootstrap points at '${WASM_PATH:-nothing}', which the site tree does not carry." >&2
  echo "       Rebuild with 'cargo leptos build' (a plain cargo build of the binary names the bundle differently)." >&2
  exit 1
fi

# Every asset the page names must answer 200, and warming them here also keeps
# the first journey after a cold start from paying the whole debug-profile WASM
# read against its hydration budget. The names are content-hashed (#450), so a
# 404 here means the served markup and the built bundle disagree.
curl -sf "$PROBE_URL/" \
  | grep -oE '(href|src)="/pkg/[^"]+"' \
  | sed -E 's/^(href|src)="//; s/"$//' \
  | sort -u \
  | while IFS= read -r asset; do
      curl -sf -o /dev/null "$PROBE_URL$asset" \
        || { echo "FATAL: the page references $asset, which the site tree does not carry." >&2; exit 1; }
    done

# ── 2. The browser endpoint ─────────────────────────────────────────────────
# The S9 upload journey hands the browser a real file, so the browser must be
# able to READ it. A containerised browser has its own filesystem, so the
# directory is bind-mounted and the journey is told both names for it: where to
# write (host) and what to type into the file control (browser).
UPLOAD_DIR="$ROOT/target/ui-e2e/upload"
mkdir -p "$UPLOAD_DIR"

if [[ -n "${UI_E2E_CHROMEDRIVER:-}" ]]; then
  echo "── starting the local chromedriver ($UI_E2E_CHROMEDRIVER)"
  UPLOAD_DIR_REMOTE="$UPLOAD_DIR"
  command -v "$UI_E2E_CHROMEDRIVER" >/dev/null \
    || { echo "FATAL: $UI_E2E_CHROMEDRIVER is not executable" >&2; exit 1; }
  "$UI_E2E_CHROMEDRIVER" --port="$DRIVER_PORT" >"$SHOTS_DIR/chromedriver.log" 2>&1 &
  DRIVER_PID=$!
else
  echo "── starting the pinned browser container"
  docker rm -f "$SELENIUM_NAME" >/dev/null 2>&1 || true
  # --shm-size: Chromium crashes on the 64 MB default under a real page load.
  # --add-host: the host-gateway mapping is what makes CONSOLE_URL resolve
  # from inside the container on a Linux engine (Docker Desktop already
  # provides the name, and re-declaring it there is a no-op).
  UPLOAD_DIR_REMOTE="/uploads"
  docker run -d --rm --name "$SELENIUM_NAME" \
    --shm-size=2g \
    --add-host=host.docker.internal:host-gateway \
    -p "127.0.0.1:$DRIVER_PORT:4444" \
    -v "$UPLOAD_DIR:$UPLOAD_DIR_REMOTE:ro" \
    "$SELENIUM_IMAGE" >/dev/null
fi
wait_http "$DRIVER_URL/status" 120 "the browser endpoint"

# ── 2b. The real CDRs (#99, opt-in) ─────────────────────────────────────────
# Two live SUTs the two-CDR journey grades side by side. FerroEHR runs its own
# published quickstart compose (fetched fresh, so "latest" is whatever its
# release train currently pins); EHRbase runs the official image pairing from
# docker/e2e-ehrbase.yml. Answering = any HTTP status (a 401 from basic auth
# is an answer), so the wait cannot pass on a connection refused.
FERROEHR_SUT_URL=""
EHRBASE_SUT_URL=""
wait_answering() { # url, tries, what
  local url="$1" tries="${2:-180}" what="${3:-$1}" code
  for _ in $(seq 1 "$tries"); do
    code="$(curl -s -o /dev/null -w '%{http_code}' --max-time 2 "$url" || true)"
    if [[ "$code" != "000" && "$code" -lt 500 ]]; then return 0; fi
    sleep 1
  done
  echo "FATAL: $what never answered at $url (last code $code)" >&2
  return 1
}
if [[ -n "${UI_E2E_REAL_SUTS:-}" ]]; then
  echo "── composing the real CDRs (FerroEHR latest, EHRbase latest)"
  mkdir -p "$ROOT/target/ui-e2e/suts"
  gh api repos/rubentalstra/FerroEHR/contents/docker-compose.yml \
    -H "Accept: application/vnd.github.raw" > "$ROOT/target/ui-e2e/suts/ferroehr.yml"
  SUTS_UP=1
  docker compose -p veredictum-e2e-ferroehr \
    -f "$ROOT/target/ui-e2e/suts/ferroehr.yml" up -d --quiet-pull
  docker compose -p veredictum-e2e-ehrbase \
    -f "$ROOT/docker/e2e-ehrbase.yml" up -d --quiet-pull
  FERROEHR_SUT_URL="http://127.0.0.1:8080/ferroehr/rest/openehr/v1"
  EHRBASE_SUT_URL="http://127.0.0.1:8090/ehrbase/rest/openehr/v1"
  wait_answering "$FERROEHR_SUT_URL/definition/template/adl1.4" 180 "FerroEHR"
  wait_answering "$EHRBASE_SUT_URL/definition/template/adl1.4" 180 "EHRbase"
fi

# ── 3. The journeys ─────────────────────────────────────────────────────────
NEXTEST_FILTER=(-E 'test(e2e_)')
[[ -n "$FILTER" ]] && NEXTEST_FILTER=(-E "test($FILTER)")
echo "── running the journeys"
UI_E2E_BASE_URL="$CONSOLE_URL" \
UI_E2E_WEBDRIVER_URL="$DRIVER_URL" \
UI_E2E_SHOTS_DIR="$SHOTS_DIR" \
UI_E2E_DOCS_SHOTS="${UI_E2E_DOCS_SHOTS:-}" \
UI_E2E_FERROEHR_URL="$FERROEHR_SUT_URL" \
UI_E2E_EHRBASE_URL="$EHRBASE_SUT_URL" \
UI_E2E_UPLOAD_DIR="$UPLOAD_DIR" \
UI_E2E_UPLOAD_REMOTE="$UPLOAD_DIR_REMOTE" \
UI_E2E_CLIENT_IP_HEADER="$CLIENT_IP_HEADER" \
UI_E2E_HOST_URL="$PROBE_URL" \
  cargo nextest run --locked -p veredictum-console --features ssr \
    -j 1 --no-fail-fast "${NEXTEST_FILTER[@]}"

# ── 4. Nothing may have leaked into the checkout ────────────────────────────
TREE_STATE_AFTER="$(git_tree_state)"
if [[ "$TREE_STATE_AFTER" != "$TREE_STATE_BEFORE" ]]; then
  echo "FATAL: the harness left residue in the working tree:" >&2
  diff <(printf '%s\n' "$TREE_STATE_BEFORE") <(printf '%s\n' "$TREE_STATE_AFTER") >&2 || true
  exit 1
fi
echo "── working tree clean of run residue"

echo "── journeys complete; failure evidence would be in $SHOTS_DIR"
