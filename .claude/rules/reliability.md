# Reliability & safety rules (instrument-grade Rust)

> Ported from FerroEHR's `.claude/rules/reliability.md` at the Veredictum split
> (FerroEHR#2789) and trimmed to the generic Rust rules. The CDR-specific
> entries (clinical payload handling, the Helm chart, the compose stack, the
> generated spec-model rules, crates.io publication posture) were dropped, and
> the enforcement column says honestly which lints and jobs arrive with the
> code.

This tool speaks verdicts about other people's products, so a silent wrong
answer is the worst thing it can do. Every rule below pairs with the check that
fails on violation. When you add a rule here, add its enforcement in the same
change: a rule without a failing check is a wish.

The principles come from the Rust API Guidelines checklist, the Rust Book's
error-handling and overflow chapters, the Clippy book, and the Cargo and
rustdoc books.

## Enforcement tiers (strongest first)

1. **Compile property** — the type system makes the violation unrepresentable
   (newtypes, `#[must_use]`, sealed traits, `forbid`, `compile_error!` feature
   guards).
2. **Lint at `deny`/`forbid`** — fails every `cargo clippy`, local and CI
   (`[lints]` in `Cargo.toml`). NOTE on `forbid`: it cannot be relaxed by ANY
   attribute — an `#[allow]` under a forbid is itself a compile error (rustc
   book, lint levels).
3. **Warn plus CI `-D warnings`** — `clippy::all` and `clippy::pedantic` both
   live here, so every pedantic lint is effectively a hard rule, including
   `missing_errors_doc` and `missing_panics_doc`.
4. **CI job** — `cargo deny check` (which subsumes cargo-audit: same RustSec
   database, plus yanked, licenses, bans, sources), the comment-style guard,
   the attribution guard, rustfmt, the rustdoc job (`cargo doc` with
   `RUSTDOCFLAGS=-D warnings`, without which the `[lints.rustdoc]` table is
   inert), the MSRV job, and the scheduled latest-deps lane.
5. **Review-enforced** (weakest, minimize): only for properties no tool can
   check. Each is marked as such below.

## The rules

- **No `unsafe`, ever** (`unsafe_code = "forbid"`, tier 2). A need for
  `unsafe` is a design defect to solve differently. The SAFETY vocabulary is
  guarded from both sides: `undocumented_unsafe_blocks` and
  `multiple_unsafe_ops_per_block` (moot but free), and
  `unnecessary_safety_comment` / `unnecessary_safety_doc`, which deny a
  `// SAFETY:` comment or a `# Safety` doc section on safe code.
- **Fail loud, never wrap.** Release builds run with `overflow-checks = true`,
  so integer overflow panics instead of silently wrapping into a wrong
  measurement or a wrong verdict. On load-bearing arithmetic (percentile
  interpolation, histogram bucket math, arrival-schedule offsets, pagination)
  prefer explicit `checked_*` or `saturating_*` with a typed error: the panic
  is the backstop, not the design. Numeric-honesty lints back this at deny
  tier: `float_cmp_const`, `lossy_float_literal`, `as_underscore`,
  `fn_to_numeric_cast_any`, `precedence_bits`, `suspicious_xor_used_as_pow`,
  `ambiguous_negative_literals`, plus `integer_division` at warn.
- **The panic strategy stays `unwind`**, pinned in `[profile.release]`. Cargo
  documents that tests IGNORE the `panic` setting
  (<https://doc.rust-lang.org/cargo/reference/profiles.html#panic>), so an
  `abort` regression is untestable by construction. Release builds carry
  `debug = "line-tables-only"` so a production panic names its file and line,
  and `strip` stays `"none"`, because rustc notes that stripping symbols makes
  traces incomprehensible.
- **No `unwrap`/`expect`/`panic!`/`unreachable!`/`unimplemented!`/`todo!` in
  production code** (deny-tier lints). Tests keep the `clippy.toml`
  `allow-*-in-tests` scoping, per the Book ch11 doctrine that a panicking
  assertion is exactly how a test fails. Recoverable failures return typed
  errors: `thiserror` in library code, `anyhow` only in the binary entry point.
- **The ONE sanctioned escape for a logically impossible `Err`/`None`** (Book
  ch9: "perfectly acceptable to call `expect` … and document the reason you
  think you'll never have an `Err` variant"): a narrowly scoped
  `#[expect(clippy::expect_used, reason = "…")]` on the smallest item, whose
  reason states the inspection proving unreachability, plus a *should*-phrased
  message. Dodging the lint with `unwrap_or_default()` instead is FORBIDDEN:
  that converts a loud impossible state into a silent wrong value, which is
  the failure class this file exists to prevent.
- **No panicking indexing on any data path** (deny tier: `indexing_slicing`,
  `string_slice`): use `.get(..)` or pattern matching over `x[i]` and
  `&s[a..b]`. `string_slice` panics on a UTF-8 boundary, and the payloads this
  tool reads are full of multi-byte clinical text. Tests are scoped out via
  `clippy.toml`; a site PROVEN in bounds uses the `#[expect]` escape above,
  never a bare index.
- **Guards are never silently dropped**: `let _ = lock/handle;` is denied
  (`let_underscore_drop`, `let_underscore_lock`). Bind guards to named
  variables that live to the end of scope. `unused_result_ok` (deny) closes the
  `.ok();` variant, which looks like a check but only silences `#[must_use]`.
  Edition-2024 corollary (review-enforced, from the Edition Guide's own
  deadlock example): a guard produced in an `if let` scrutinee is dropped
  BEFORE the `else` branch runs
  (<https://doc.rust-lang.org/edition-guide/rust-2024/temporary-if-let-scope.html>).
  Never rely on it inside `else`; rewrite as `match` when the guard must span
  both arms.
- **`#[source]` over an `Option<Arc<…>>` or `Option<Box<…>>` yields the SMART
  POINTER as the source hop, not the error inside it** (verified first-hand on
  Rust 1.96.1 against `thiserror` 2). The failure is invisible in a log,
  because `Display` forwards and the chain reads correctly, while
  `downcast_ref::<ConcreteError>()` returns `None`, which is the entire point
  of carrying a cause. A non-`Option` `Box<dyn Error + Send + Sync>` derives
  correctly; the optional form must hand-write `Display` and `Error` and return
  `self.source.as_deref()`. There is no lint for this: the only thing that
  catches it is a test that DOWNCASTS to the concrete error type rather than
  asserting `source().is_some()`, so every new source-carrying error type gets
  one.
- **`#[error(transparent)]` removes its own type from the cause chain**
  (verified first-hand on the same toolchain). Transparent forwards `Display`
  AND `source()`, so the wrapper is not a hop: a test looking for the wrapper
  fails while the chain is intact. Assert the ROOT cause, not an intermediate
  type, or the test measures thiserror's forwarding rather than our chaining.
- **`Result` to `Option` inside a chain is a DECISION, and it carries NO
  automated guard** (review-enforced, the honest no-guard record).
  `.filter_map(|x| f(x).ok())`, `.and_then(|x| f(x).ok())` and `f(x).ok()?`
  turn an error into a missing element with no trace. In this tool that shape
  is how a case silently drops out of a run, or an unparsable response field
  becomes a passing comparison. The rule: **a fallible conversion whose failure
  means "the input is DEFECTIVE" propagates a typed error; only one whose
  failure means "this input is legitimately ABSENT or not of this form" may
  become `Option`, and it carries a `// NOTE:` saying so.** There is no lint
  for it (the Clippy book lists none) and a grep gate cannot make the
  distinction the rule turns on, because the two shapes are textually
  identical. A wish honestly labelled beats a check that trains people to
  ignore it.
- **Determinism is lint-backed** (deny tier: `iter_over_hash_type`).
  HashMap and HashSet iteration order is undefined, and this tool's whole value
  is that its outputs are re-checkable: verdicts, emitted schemas, rendered
  documents, and the seeded arrival schedule all iterate ordered structures
  (`BTreeMap`, sorted vectors). Byte-determinism of every emitted artifact is a
  drift-tested property, not a preference.
- **A published record never depends on wall-clock luck.** Anything that lands
  in a results or verdict artifact is either derived from recorded data or
  explicitly stamped once. Re-running the verdict pipeline over the same
  results must produce the same bytes.
- **Never log a SUT response body at info level or above** (review-enforced).
  This instrument runs against real deployments, and a customer's CDR can hold
  real patient data even in a test EHR. Log identifiers, statuses, and shapes.
  A recorded exchange that must carry a body belongs in the run artifact the
  operator controls, never in a shared log stream.
- **Banned APIs are compile-time bans, not review notes** (`clippy.toml`
  `disallowed-methods` / `disallowed-types`). The durable entry ported here:
  `Option::as_slice`/`as_mut_slice` — on an `Option<Vec<T>>` receiver they
  yield `&[Vec<T>]`, a slice of zero-or-one *vectors* rather than `&[T]`, and
  they keep compiling after a field's shape flips between `Vec<T>` and
  `Option<Vec<T>>`. Spell it `.as_deref().unwrap_or_default()` or match on the
  `Option`; `Vec::as_slice` stays fine. The full list is instantiated in
  `clippy.toml` when the code lands (#2789). A legitimate exception site
  carries a scoped `#[expect(clippy::disallowed_methods, reason)]`.
- **Errors are types, not strings, at every boundary that branches.** A caller
  that needs to distinguish outcomes gets an enum variant, not a substring
  match. String context belongs in the display text, not the discriminant.
  This matters most at the outcome-classification seam: a classification that
  branches on a message is a classification that changes when a message is
  reworded.
- **Ids are distinct types where confusion is fatal** (C-NEWTYPE). Case ids,
  binding ids, party ids, and capability keys are newtypes, so the type system
  rejects a swapped argument at compile time (tier 1). Never add a function
  taking two adjacent bare `String` parameters that name different id kinds.
- **Every closed vocabulary is an enum or a newtype.** Outcome kinds,
  selectors, header matchers, capture sources, the `${…}` reference grammar,
  dispositions: illegal states unrepresentable, and an unknown token is a
  validate-time finding plus a loud drive-time error, never a silent fallback
  to a default. A silent fallback in a conformance instrument manufactures a
  passing row out of a typo.
- **Every public item: documented, `Debug`, with concrete `# Errors` and
  `# Panics` sections** (C-DOC, C-DEBUG, C-FAILURE). `missing_docs` requires
  the doc comment, `missing_debug_implementations` requires `Debug`, and
  `missing_errors_doc` / `missing_panics_doc` require the sections. Doc quality
  is lint-backed too: the `[lints.rustdoc]` table (broken and private
  intra-doc links, invalid codeblock attributes, bare URLs at deny) plus the CI
  doc job. Doctests are copy-paste templates and deny warnings via
  `#![doc(test(attr(deny(warnings))))]`.
- **Visibility is deliberate** (C-STRUCT-PRIVATE): private by default, scoped
  visibility only at real module boundaries, and every import names its
  defining module. `unreachable_pub` is watched at CI. Struct fields stay
  private unless the type IS a plain record.
- **Constructors and conversions follow the standard shapes** (C-CTOR, C-CONV,
  C-GETTER, C-BUILDER): `new` and `with_*` builders, `From`/`TryFrom` over
  ad-hoc `to_x()` where the conversion is total or fallible, getters without a
  `get_` prefix.
- **A field's default value lives in its struct's `Default` impl, inline.**
  Banned: the per-field `#[serde(default = "path")]` form, which lets
  `Default::default()` and a deserialized value disagree about one field;
  zero-argument `fn default_x()` constructors; and single-reader
  `const DEFAULT_X`. A constant with several consumers stays a constant and may
  be read from inside the `Default` impl. This is the shape of RFC 3681, whose
  own syntax is nightly-only
  (<https://github.com/rust-lang/rust/issues/132162>).
- **An HTTP status is compared as a `StatusCode`, never as a number.**
  `status.as_u16() == 401` discards the type the `http` crate exists to
  provide, and `403` versus `404` is a one-character typo no compiler catches.
  Rendering the number (a log field, a recorded outcome, a report column) stays
  legal; only comparison against a literal is refused. The catalogue authors
  numbers in YAML, and the boundary that parses them into `StatusCode` is
  exactly where the typing must happen, once.
- **Blocking never hides in async**: no `std::sync` locks held across `.await`
  (`await_holding_lock`), no synchronous I/O on the runtime, `spawn_blocking`
  for the rare CPU-heavy transform. In the load instrument this is not a style
  point: a blocked task delays a planned arrival and shows up as latency the
  SUT never caused.
- **Dependencies are pinned, locked, and vetted**: `cargo deny check` green at
  all times, no new dependency for what the pinned set already provides. CI
  builds run `--locked`, per the Cargo FAQ's determinism rationale, and the
  scheduled latest-deps lane is the official way to discover in-range upstream
  breakage. Mutually exclusive features carry a `compile_error!` guard, per the
  Cargo book.
- **Comment style is machine-enforced** (`comments.md`): line comments only,
  unfinished work is `// TODO(#NNNN):`, a settled decision is a `// NOTE:`
  citation plus one sentence at most 3 lines, plain `//` runs at most 8 lines,
  and `// SAFETY:` is reserved for `unsafe`. Enforcement:
  `scripts/checks/comment-style.sh` per-edit through the hook, and per-PR in
  the CI guards job.
- **The build pipeline is code, and it is analysed like code** (audited against
  the OWASP GitHub Actions Security Cheat Sheet,
  <https://cheatsheetseries.owasp.org/cheatsheets/GitHub_Actions_Security_Cheat_Sheet.html>).
  Four properties hold across every workflow: every `uses:` pinned to a full
  commit SHA with its version in a trailing comment; `permissions: {}` at
  workflow level with the minimum granted per job; `persist-credentials: false`
  on every `actions/checkout` that does not use git against the remote; and no
  context value interpolated into a `run:` block, since it arrives through
  `env:`. A lane that PUBLISHES restores no build cache unless no untrusted run
  can write its cache keys, with the proof recorded at the step. Enforcement is
  the live `zizmor` guard job at `--min-severity=low` plus CodeQL's `actions`
  language (`codeql.yml`).

## Recorded posture decisions

- **License: Apache-2.0** for this repository's own code. Section 4's license
  and NOTICE retention makes attribution travel with every copy and
  derivative, and section 3 grants the patent license. Vendored third-party
  material keeps its upstream terms, and each vendored tree carries a
  `PROVENANCE.md` naming its license with the upstream `LICENSE` beside it.
  Dependencies stay license-gated by `deny.toml`.
- **crates.io publication posture: decided and executed** — the binary and
  the library publish as one crate (#5; the console consumes the lib and
  spawns the pinned binary). The API-shape lints stay live, and the
  pre-release line makes no stability claim yet.

## When a lint fights a legitimate case

**`#[expect(lint, reason = "…")]` is the default suppression**: it self-reports
the moment the expectation stops being fulfilled
(`unfulfilled_lint_expectations`), so stale suppressions cannot accumulate.
`#[allow(lint, reason = "…")]` is reserved for a lint that fires only in SOME
configurations (cfg- or feature-dependent code, macro expansions), where an
`#[expect]` would itself warn in the quiet configuration. Both forms MUST carry
`reason = "…"` (`allow_attributes_without_reason` = deny;
`allow_attributes` = warn steers toward `#[expect]`). Scope every suppression
to the smallest item. A file-level or crate-level suppression needs sign-off in
the PR.
