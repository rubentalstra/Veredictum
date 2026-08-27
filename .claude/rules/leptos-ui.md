---
paths: ["app/veredictum-console/**"]
---

# Leptos console rules (`app/veredictum-console` — and any Leptos code)

Ported from FerroEHR's `.claude/rules/leptos-ui.md` at the console scaffold
(#53). That file was authored from a full read of the official Leptos book
(leptos-rs/book `main`, targets Leptos 0.8) and then hardened by live
failures in FerroEHR's admin console; the lessons carry over in substance,
with the FerroEHR-specific machinery renamed or marked. Citations are book
chapters (`view/04_iteration`, `ssr/24_hydration_bugs`, …). The stack pins
live in the console's own `Cargo.toml` until a second consumer justifies a
`[workspace.dependencies]` table. Where a rule names a FerroEHR file, that
is the precedent to imitate, not a path in this tree.

## 0. Owner mandates (absolute)

- **Rust only — zero hand-written JavaScript.** No authored `.js` files, no
  inline `<script>` bodies, and no plain HTML `onxxx="..."` attributes
  containing JS strings (the book's `oninput="this.form.requestSubmit()"`
  trick in `router/20_form` is explicitly FORBIDDEN here — use an `on:` Rust
  listener instead). The only JS in the product is the `wasm-bindgen`
  bootstrap the toolchain generates. JS-wrapping crates (ECharts/Plotly
  bindings etc.) are banned; charts are `leptos-chartistry` (pure Rust+SVG).
- **Engine boundary (#52):** the console reaches the instrument only through
  the published `veredictum` crate — the lib's typed pipeline API for reads,
  the pinned CLI binary as a subprocess for runs (#54). It never depends on
  the root package by `path =`, and it never reimplements parsing, driving,
  or judgement. The SUT is reached only by the spawned instrument; console
  code never speaks to a CDR itself.
- **Server functions are a public HTTP API** (`server/25_server_functions`
  security warning). The console has no login by design (#52): it binds
  `127.0.0.1` by default and the operator owns any wider exposure. That
  makes the discipline stricter, never looser — every `#[server]` fn treats
  its input as untrusted, reads and writes only under the mounted roots, and
  SUT credentials live in memory and the spawned run's environment only:
  never in client-readable signals, props, serialized resource data, files,
  or log lines.

## 1. Crate & feature discipline (`ssr`/`hydrate`)

- Two compilation targets, one crate: `ssr` (server) and `hydrate` (browser)
  features per the `cargo-leptos` model (`ssr/21`, `ssr/22`). Server-only
  dependencies (axum, tokio, anything with `mio`/fs) are `optional = true`
  and enabled only in the `ssr` feature (`ssr/24`). Keep
  `wasm32-unknown-unknown` compiling for the lib at all times: the gate is
  clippy on BOTH targets (§10).
- WASM is 32-bit: **never `usize`/`isize` in server-fn arguments or return
  types, or in any serialized shared type** — fixed-size ints only
  (`server/25` quirks).
- Signals hold `Send + Sync` values (multithreaded tokio SSR). The `_local`
  variants (`signal_local`, `LocalResource`, `Action::new_local`) are ONLY
  for genuinely `!Send` browser types — using `LocalResource` where a
  serializable `Resource` would work is a deoptimization
  (`reactivity/working_with_signals`, `ssr/23`).
- Binary size: `[profile.wasm-release]` (root manifest) with `opt-level='z'`,
  `lto`, `codegen-units=1` + `lib-profile-release` (`deployment/binary_size`);
  serve compressed WASM; avoid `regex` and generics-heavy code in
  client-compiled paths (monomorphization bloat — factor a concrete inner
  fn). `--cfg=erase_components` in dev only (cargo-leptos ≥0.2.40 does it
  automatically), never release.
- **Section-boundary type erasure (FerroEHR lesson W0, 2026-07-17):** plain
  `cargo build`/`cargo test` runs have NO `erase_components`, and deeply
  nested component-library view trees then blow rustc's layout-recursion
  depth at codegen (thaw main needs `#![recursion_limit = "256"]` for its
  own `Layout` alone). Every screen therefore breaks its view into sections
  bound to locals erased with `.into_any()` — never one monolithic `view!`
  tree — so `cargo nextest`/CI builds stay compilable regardless of cfg.

## 2. Reactivity

- Component functions are **setup functions — they run once**. Anything
  dynamic in a view must be a signal or a closure reading signals
  (`reactivity/interlude_functions`).
- Access discipline (`reactivity/working_with_signals`): `.get()`/`.set()`
  for cheap `Copy`-ish values; `.read()`/`.write()` guards or
  `.with()`/`.update()` for collections/large values — never
  `sig.get().is_empty()` (clones the whole value). Never hold a `.read()`
  guard across a write or a `.write()` guard across a read (runtime panic).
- Signal-depends-on-signal: derived closure (`move || a.get() * 2`) or
  `Memo`. **Writing one signal from an `Effect` that reads another is
  forbidden** — the book calls it out as officially discouraged reactive
  spaghetti (`working_with_signals` §4, `14_create_effect`). Effects exist
  ONLY to sync with the non-reactive outside world (DOM APIs, logging,
  storage); most "I need an effect" cases are really event listeners or a
  `leptos-use` primitive — check there first.
- The two sanctioned async-answer-into-signal shapes (FerroEHR adjudication
  2026-08-22): an **Action dispatch's answer** is written in the action's
  own async continuation — the dispatch is the user event, no Effect needed.
  A **Resource seeding editable local state** stays a one-shot
  resource-reading `Effect` WITH a written justification whenever any seed
  target renders outside the resource's own `Suspend`: seeding inside the
  `Suspend` writes those signals during the server pass and again during
  hydration replay, and the mid-walk divergence is a tachys
  unrecoverable-hydration panic (reproduced live in FerroEHR). Seed inside
  the `Suspend` only when every target renders inside it.
- Corollary for **seed-gated `disabled`** (inert-until-seeded forms): the live
  state goes on `prop:disabled`, with a STATIC `disabled` attribute beside it
  for the server HTML (inert from first paint). An attribute BINDING is not
  enough: the seed can flip the signal during hydration replay, before the
  binding exists, and hydration trusts the serialized attribute — the control
  then stays disabled until a change that never comes. Same doctrine as
  `prop:value`/`prop:checked` (leptos COMMON_BUGS: attributes set initial
  state; properties carry live state).
- Effects don't run on the server (that's a feature — see §8);
  `Effect::new_isomorphic` only with a written justification.
  `Effect::watch` for explicit-dependency cases.
- Prefer local component state; escalate in this order: **URL (router) →
  context signal → `Store`** (`15_global_state`). Context values use the
  newtype pattern (`struct Selected(ReadSignal<usize>)`) to keep types
  unambiguous; `use_context` is a runtime lookup — `expect_context` only
  where provision is structurally guaranteed.
- Toolchain trap (pinned 1.97, FerroEHR bisection 2026-08-23): calling
  `use_query_map().with_untracked(…)` directly in a `#[component]` fn body
  makes `clippy::must_use_candidate` stop firing on that fn — so the
  crate-idiom `#[expect(clippy::must_use_candidate)]` on the component turns
  into an `unfulfilled_lint_expectations` build failure. Put the URL read in
  a private helper fn instead (FerroEHR's `find_from_url`/`pane_view_from_url`
  are the precedents).

## 3. Components & props

- Props that change over time are signal types, not plain values
  (`view/03_components`). Reusable component APIs take
  `#[prop(into)] progress: Signal<T>` so callers can pass `ReadSignal`,
  `Memo`, or `Signal::derive(closure)`. Use `#[prop(optional)]` /
  `#[prop(default = …)]` deliberately (optional = `Default::default()`).
- Doc-comment every component and every prop (the macro turns these into
  real docs — `view/03`); `missing_docs` is workspace-inherited anyway.
- Child→parent: prefer a callback prop or an `on:` DOM listener on the
  component tag; pass a `WriteSignal` down only when genuinely needed and
  never pass write halves around promiscuously (`view/08_parent_child`).
  Context to skip prop drilling; remember it trades away compile-time
  checking.
- Attribute spreading (`view/03`): `attr:`/`{..}` for plain HTML attrs on
  components; `AttributeInterceptor` when the target isn't every top-level
  element.

## 4. Views: iteration & control flow

- Dynamic lists use `<For each key children>` with a **stable, unique,
  data-derived key — never an index** (`view/04_iteration`). Signals stored
  in dynamic rows are `ArcRwSignal` (deallocates with the row), converted
  `RwSignal::from(arc)` in `children`.
- Fine-grained row updates (`view/04b_iteration`): key-includes-value is the
  least efficient option (full row re-render) — prefer nested signals or a
  `reactive_stores` `Store` with `#[store(key…)]`. NEVER
  `each=… .enumerate()` + a `Memo` capturing the plain index inside `<For>`
  (stale-index duplicates); use `<ForEnumerate>`'s index *signal* and guard
  `data.get(i)` against removal races.
- Cheap conditional text/class → `move || if …` or `class:x=cond`;
  expensive branches → `<Show when fallback>` (memoized, renders each branch
  once — `view/06_control_flow`). Divergent branch types → `Either`/
  `EitherOf3…` or `.into_any()`.
- Errors must never silently render as nothing — but in SSR'd data
  sections do NOT reach for `<ErrorBoundary>` inside `<Suspense>`:
  **hydrating a server-rendered ErrorBoundary fallback mismatches in
  Leptos 0.8** (proven live by FerroEHR's E2E gate, 2026-07-17). The
  standing pattern: resolve the `Result` INSIDE the `Suspend` and render
  content-or-inline-error as one `.into_any()`-erased view.
  `<ErrorBoundary>` remains fine for non-suspense render-time `Result`s
  (`view/07_errors`).
- **Never create Resources (or render `<Outlet/>`/any resource-owning
  subtree) inside a `Suspend` closure** (found live 2026-07-18 by FerroEHR's
  E2E gate). A `Suspend` closure re-runs on every notification of the
  resources it awaits, and each re-run RE-CREATES everything inside it.
  Resource ids are allocated in creation order and serialized by id — when
  the server and client re-run a different number of times, their id
  spaces diverge and hydration reads the wrong serialized slots
  ("expected a text node" crashes, timing-dependent). Layout guards (the
  shell) render the chrome + `<Outlet/>` exactly ONCE outside the
  Suspense; the Suspend renders only the decision (redirect / small
  resource-free fragments like identity text). Corollary: an `Err` arm
  inside a Suspend must render an explicit view — never `unwrap_or` a
  value that produces a structurally different branch set than the server
  may have rendered.
- **Anchors to server-owned axum routes need `rel="external"`** (downloads,
  anything not in the Leptos route tree): after hydration the client router
  intercepts same-origin anchors and 404s routes it doesn't own — flakily,
  depending on click timing vs WASM load (found live 2026-07-17).

## 5. Forms

- Controlled inputs: `prop:value` (the `value` *attribute* only sets the
  initial value) + `on:input:target`, or the `bind:value`/`bind:checked`/
  `bind:group` sugar (`view/05_forms`). `<textarea>` needs child text +
  `prop:value`; `<select>` is driven by `prop:value` on the select.
- Uncontrolled: `NodeRef<html::X>` + `on:submit` with `ev.prevent_default()`.
- Parse user input yourself; browser `type="number"` etc. is not validation
  (`view/07`).
- Mutating forms are `<ActionForm>` bound to a `ServerAction<T>` — they work
  without WASM loaded (progressive enhancement) and expose
  `.pending()/.value()/.input()` for UI state
  (`progressive_enhancement/action_form`). Client-side validation via
  `on:submit:capture` + `FromFormData`. Keep server-fn args
  `<ActionForm>`-compatible (default URL-encoded POST; nested structs use
  `name="arg[field]"` indexing; mind `serde_qs` Option/enum quirks —
  `server/25`).

## 6. Async data (the server-function pattern)

- Loading = `Resource::new(source, fetcher)` calling a `#[server]` fn;
  mutation = `Action`/`ServerAction` dispatching one (`async/10`,
  `async/13`, `server/25`). Reactive inputs go in the **source** (tracked);
  the fetcher is untracked by design. `OnceResource`/`<Await>` for
  load-once. `refetch()` for manual reload; after a successful mutating
  action, refetch the affected resources (action `.version()`/value as the
  trigger).
- Read resources under `<Suspense>`; use `Suspend::new(async move { … })`
  to `.await` several resources without nested `Option`-matching
  (`async/11`). Lists/tables that reload on filter changes use
  `<Transition>` to keep old data visible instead of flashing the fallback
  (`async/12`) — the console default for search/pagination.
- Never fetch in an `Effect` + signal-write; that's what resources are for.
  `spawn_local` only for true fire-and-forget.

## 7. Server functions & the axum side

- `#[server]` fns are thin: validate input, pull typed context, call the
  engine seam (the published lib, or the subprocess supervisor), map errors.
  Business logic lives in ordinary testable modules the server fn calls.
- Errors: a console-wide error enum implementing `FromServerFnError`
  (`server/25`) so the UI can render domain errors (a validate finding, a
  spawn failure, an exit status) — don't stringify everything into
  `ServerFnError::ServerError`.
- Request/response access: `leptos_axum::extract()` /
  `extract_with_state()`; shared server state (the run supervisor, the
  mounted root paths) is provided via `leptos_routes_with_context` +
  `provide_context`, consumed with `expect_context` (`server/26`). Set
  status/headers via `ResponseOptions`; navigation after mutation via
  `leptos_axum::redirect` (degrades correctly under `<ActionForm>` —
  `server/27`).
- Generic-state workaround: concrete `#[server]` wrapper delegating to a
  generic inner fn (`server/26`).

## 8. SSR & hydration correctness (hard rules — `ssr/22`–`24`)

- Default rendering mode: **out-of-order streaming** (the framework
  default). Deviations (async/in-order) per route need a written reason.
- The app body runs on BOTH server and browser. Therefore:
  - Never branch view **structure** on `cfg!(target_arch = "wasm32")` /
    `#[cfg(feature = …)]` — server HTML and client view must be identical.
  - Browser-only calls (`window`, storage, timers) go inside `Effect::new`
    (client-only by design) or a `leptos-use` wrapper; server-only code goes
    in `#[server]` fns. HTTP from shared code uses isomorphic crates only.
  - Views emit **valid HTML**: no block elements inside `<p>`; `<table>`
    ALWAYS gets an explicit `<tbody>` (browsers insert one, breaking DOM↔view
    correspondence). 0.7+ hydration walks the DOM — invalid HTML = hydration
    error.
  - No non-determinism in initial render (random ids, `now()` timestamps)
    that differs between the server pass and client hydration.
- `leptos_meta` (`<Title formatter=…>`, `<Stylesheet>`, `<Meta>`) is used
  from component bodies, never by hand-editing `<head>` in the shell
  (`metadata`). Every routed page sets a `<Title>`.

## 9. Router

- One `<Router>` at the root; `<Routes fallback=…>` with a real 404
  (`router/16`). Nested routes for the master-detail screens (catalogue →
  chapter → case; results → case → exchange).
- Params/queries are `Memo<Result<T,_>>` via typed `use_params::<T>()` /
  `use_query::<T>()` with `#[derive(Params)]` (stable: fields are
  `Option<T>`) — handle the `Err`/`None` cases; they are user input
  (`router/18`).
- **Filter/search/pagination state lives in the URL**, driven by
  `<Form method="GET">` → `use_query_map` → resource source
  (`router/20`, `15_global_state`): shareable, refresh-safe, WASM-optional.
- Navigation uses `<A>`/router APIs, never `window.location`. A plain
  same-origin `<a href>` is equally safe — the router's window-level click
  handler intercepts every same-origin anchor (verified in leptos_router 0.8
  `location/mod.rs`), so `<A>` differs only in active-class handling; what
  the rule bans is imperative `window.location` writes. (Anchors to
  server-owned axum routes still need `rel="external"` — §4.)

## 10. Testing & quality gates

- Business logic lives OUT of components in plain types with unit tests
  (`testing`). Components stay thin.
- Component/browser tests: `wasm-bindgen-test` with `mount_to` — remember
  updates are async: `tick().await` before asserting.
- **E2E doctrine, for when the harness lands with the screens (#6):**
  Rust-native only — `thirtyfour` (WebDriver, built on `fantoccini`)
  driving headless Chromium; journeys are plain `#[tokio::test]`s,
  skip-with-reason when the base-URL variable is unset. Every journey
  fails on any browser-console hydration error or panic. Explicit waits on
  elements/conditions, never `sleep`; a flaky journey is fixed, never
  `#[ignore]`d or retried-by-default. NOT Playwright/JS (the no-JS mandate
  covers the test suite). The harness does not exist yet — a claim of E2E
  coverage before it lands is false.
- Gates for every console change: `/ui-gates` — clippy on native
  (`--features ssr`) **and** wasm32 (`hydrate`, lib), `cargo nextest run`,
  `leptosfmt` + `cargo fmt`, and `cargo leptos build` when the build surface
  changed. FerroEHR's screenshot-guard CI job is deliberately NOT ported:
  no docs-site screenshot set exists here yet.
- `console_error_panic_hook` is set in the hydrate entry point (real stack
  traces in the browser — `getting_started/leptos_dx`).

## 11. Islands (deferred option — do not use yet)

`#[island]` mode (`islands`) can cut WASM size ~50% by shipping only
interactive islands, and lets `#[component]` bodies run server-only. It is
NOT enabled for the console v1: full hydration is the safer start while the
widget kit and the interactive screens are young. Revisit as a measured
optimization; if enabled, islands must stay small (pass server-rendered
`children` into them — "donut" pattern) and props must be serializable.
