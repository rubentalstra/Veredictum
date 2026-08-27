---
name: ui-gates
description: >
  Runs the full console quality-gate battery for app/veredictum-console:
  clippy on native AND wasm32 targets, nextest, formatting, and a
  cargo-leptos build. Use before committing any console change, when the
  user asks to "check the UI", or as the done-gate a ui-implementer task
  must pass.
allowed-tools: [Bash, Read, Grep, Glob]
---

# /ui-gates

Run every gate the console must pass (defined in
`.claude/rules/leptos-ui.md` §10). Stop and report on the first hard
failure; run the cheap gates first.

## Preconditions

- `app/veredictum-console` must exist; if it doesn't, say so and stop.
- Tooling presence: `rustup target list --installed | grep wasm32` (install
  with `rustup target add wasm32-unknown-unknown` if missing);
  `cargo leptos --version`; `leptosfmt --version` (report if missing —
  install is `cargo install --locked cargo-leptos leptosfmt`, ask before
  installing).

## The battery (in order)

```bash
# 1. Format (fast, catches drift) — leptosfmt only when installed; report
#    SKIPPED(not installed) otherwise, never silently
cargo fmt --all --check
leptosfmt --check app/veredictum-console/src

# 2. Clippy — BOTH compilation targets, in the EXACT CI feature shapes
#    (the featureless crate ships nowhere: neither ssr nor hydrate; the
#    wasm pass catches server-only deps leaking past the ssr feature gate)
cargo clippy -p veredictum-console --all-targets --features ssr
cargo clippy -p veredictum-console --lib --target wasm32-unknown-unknown --no-default-features --features hydrate

# 3. Tests — the workspace suite, plus the ssr shape once ssr-gated test
#    modules exist (a featureless run silently skips every
#    #[cfg(feature = "ssr")] module)
cargo nextest run

# 4. Full build (server bin + WASM + assets) — only when the change
#    touches the build surface (Cargo.toml, styles, assets, features);
#    otherwise report it as skipped-with-reason
cargo leptos build
```

Deliberately absent, named so a green report cannot overclaim: the E2E
journey stage (the harness lands with the screens, #6) and FerroEHR's
screenshot guard (no docs-site capture set exists here). When those land,
this file gains their stages in the same change.

Adjust the exact feature flags to the crate's `Cargo.toml` (read it first —
the `ssr`/`hydrate` feature names are the convention, not a guess).

## Report

One line per gate: PASS / FAIL / SKIPPED(reason), with the failing output
excerpted verbatim on failure. Never mark a gate green you did not run.
A FAIL is never fixed by weakening the gate (removing a lint, deleting a
test, dropping the wasm pass) — fix the code.
