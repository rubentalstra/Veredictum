# `veredictum-console` — the Leptos web console over the published instrument

A standalone full-stack Leptos 0.8 app (cargo-leptos: `ssr` server binary +
`hydrate` WASM client) and its own OCI image. It is a **frontend over the
published instrument** — no openEHR spec governs a web console (our own
design; the record is issue #52); the wire the runs it orchestrates drive is
spec-bound as ever. Adapted from FerroEHR's `app/ferroehr-admin-ui` CLAUDE.md
at the scaffold (#53); the mandates carry over, re-grounded on this product.

## The three binding mandates

1. **Rust only — zero authored JavaScript** (the wasm-bindgen bootstrap is
   generated, never touched; styling is Tailwind v4 standalone, no Node).
2. **The engine is reached ONLY through the published crate** (#52, #54):
   reads parse through the `veredictum` lib's typed pipeline API, runs spawn
   the pinned `veredictum` CLI binary as a subprocess. Never a `path =`
   dependency on the root package, never a reimplemented parser or judgement,
   and never console code speaking to a CDR itself — the spawned instrument
   is the only thing that touches the SUT — with ONE carved-out exception:
   the connect screen's reachability probe (`run_api::read::probe`), a single
   GET whose answer renders verbatim and is never judged. A diagnostic about
   the network path is not conformance traffic; everything that grades stays
   engine-only.
3. **Every `#[server]` fn is a publicly reachable HTTP endpoint**, and the
   console has no login by design: it binds `127.0.0.1` by default, wider
   exposure is the operator's decision with their own gate in front. So every
   server fn treats input as untrusted and stays under the mounted roots, and
   SUT credentials (the Basic or Bearer values the run form collects) live in
   memory and the spawned run's environment only — never in client-readable
   signals, props, or serialized resource data, never in a file, never in a
   log line.

## Discipline

- Rules file: `.claude/rules/leptos-ui.md` (hydration hard rules, `<For>`
  keys, forms, server fns) — every section applies. Leptos questions →
  `/leptos-lookup` (the book is the oracle, never memory). Gates →
  `/ui-gates`.
- Business logic lives in component-free plain-Rust modules with ordinary
  unit tests; components stay thin.
- Stack pins live in this crate's `Cargo.toml` (Leptos 0.8 stable; 0.9 is
  adopted at stable, never at beta). Thaw, leptos-use, leptos_icons and
  leptos-chartistry arrive with the first real screens, at the pins the
  design record (#52) names.
- **Views are built in `.into_any()`-erased sections** (rules §1): plain
  cargo builds have no `erase_components`, and monolithic component-library
  view trees blow rustc's layout-recursion depth in `cargo test` codegen
  (FerroEHR lesson W0).
- Shared kits, once screens exist: one data-table kit, one chart kit, one
  error-copy module. The FerroEHR console's "one kit per repeated surface"
  and "one reader per claim" laws apply here from the first screen — two
  surfaces never read the same fact through two different paths.

## Error feedback: toast vs inline (FerroEHR adjudication, kept)

- **A mutation reports success AND failure as a notification.** Every action
  that writes (start a run, write an ixit, render documents) reports both
  outcomes. The failure copy is actionable: name the object, name what went
  wrong (the instrument's own diagnostic verbatim), name the next action.
- **A detailed inline `MessageBar` may stay BESIDE the failure toast** where
  the diagnostic is worth reading line by line (a validate finding list, a
  refused ixit). Never inline-only: a transient success toast paired with a
  silent failure below the fold reads as "nothing happened".
- **Pure reads render inline errors only** — in the section whose data
  failed, never a toast. A first-class empty state (no runs yet, no results
  document in the output directory) is not an error at all.

## No console-local domain state (FerroEHR ruling, kept)

The console stores NOTHING of its own — no database, no JSON store beside
the binary, no state directory. The catalogue, specs and party trees are
read-only mounts; runs write into the mounted output directory exactly as a
terminal run would; the session's run list is in-process memory an image
restart legitimately forgets. If a grouping or preset is worth keeping, it
must derive from the mounted artifacts or not exist.

## Gates

`/ui-gates`: clippy on **native (`--features ssr`) and wasm32 (hydrate)**
targets, `cargo nextest run`, `cargo fmt` (plus `leptosfmt` when installed),
cargo-leptos build when the build surface changed. The E2E journey stage and
any visual-capture guard are deliberately absent until their machinery lands
(#6) — a gate pointing at absent machinery reports green.
