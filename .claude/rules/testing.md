# Testing discipline

> Ported from FerroEHR's `.claude/rules/testing.md` at the Veredictum split
> (FerroEHR#2789) and trimmed to what the instrument needs: the never-weaken
> rule, the tooling, the test shapes, and the coverage mandate. The CDR-only
> sections (the shared PostgreSQL harness, the codegen fidelity gates, the
> admin console) were dropped, not softened.

Test discipline is non-negotiable (a standing hard rule, root `CLAUDE.md`).

## The hard rule

- **Never** silently weaken, skip, or delete an existing test to make a build
  pass.
- **Never** edit a test to route around a runtime bug it exposes. If a test
  fails and the fix is unclear, leave it failing and record a
  `// TODO(#NNNN):` naming its issue. Do not touch the test to make it green.
- Catalogue and corpus tests assert the **openEHR specifications**: cite the
  spec clause a test encodes, and never adjust an expectation to match an
  implementation's behaviour. A corpus or fixture defect is ADJUDICATED with a
  first-hand spec or schema citation, either as an expected-rejection entry in
  the owning gate or as an `artifacts/registers/ambiguities.yaml` entry for
  genuine spec silence (`.claude/rules/cnf-triage.md`). It is never routed
  around by editing the case.

## Tooling

- **Runner:** `cargo-nextest` (`cargo nextest run`), not `cargo test`.
- **Snapshots:** `insta` pins emitted output (the published JSON Schemas, the
  rendered report and certificate documents, verdict payloads). Redact
  volatile fields such as timestamps and generated identifiers before
  snapshotting. Review intentional changes with `cargo insta review`, and never
  accept a snapshot change you have not read.
- **Properties:** `proptest` where a round trip or an invariant is the claim,
  such as histogram encode and decode, or the `${…}` reference grammar.
- **HTTP:** `wiremock` for a fake SUT in unit tests. A real SUT belongs in the
  conformance pipeline, not in a unit test.
- **Benches:** `criterion` or `divan`, kept separate from correctness tests.

## Oracles

- The correctness oracle is the RELEASED openEHR spec text
  (`.claude/rules/cnf-triage.md` carries the full oracle order). The CNF
  Platform Conformance Test Schedule is the stalled structural GUIDE the
  catalogue's coverage derives from, never the correctness authority. Where the
  schedule and a released component conflict, the released component wins.
- The upstream Robot suites are stalled reference material. Their official data
  fixtures enter the corpus only as provenance-stamped re-adjudications, never
  as blind imports and never as an oracle.
- Prefer an existing golden vector over a hand-written fixture. A test that
  encodes a spec rule cites the section it asserts.

## Where tests live

Unit tests live beside the code they test, as `#[cfg(test)] mod tests` in the
same file, and only there: dedicated test FILES under `src/` are banned. A test
that drives the public surface belongs in `tests/`, laid out as
`tests/it/main.rs` plus one `mod` per topic file, not one top-level `.rs` per
topic. Cargo compiles and links every top-level `tests/*.rs` as its own crate
(<https://doc.rust-lang.org/cargo/reference/cargo-targets.html>), so one binary
saves the link waste while nextest still runs each test in its own process.
Shared helpers live in a plain module under `tests/it/`.

A binary-only crate is untestable by construction (Book ch11.3): `main.rs`
cannot be imported from `tests/`. The runner therefore keeps a thin `main.rs`
over a testable `lib.rs` run path (Book ch12.3), and its integration tests
import the lib.

## Test shapes (the Book ch11 doctrine)

- **`Result`-returning tests are the preferred shape**: `fn t() -> Result<(), E>`
  with `?` instead of unwrap chains
  (<https://doc.rust-lang.org/book/ch11-01-writing-tests.html>). The
  `clippy.toml` `allow-*-in-tests` scoping keeps assertion panics legal, while
  plumbing failures propagate with `?`.
  `clippy::panic_in_result_fn` fires on this shape and clippy offers no
  `allow-…-in-tests` knob for it
  (<https://doc.rust-lang.org/clippy/lint_configuration.html>). Adjudication:
  the Book shape wins in tests, and the lint keeps its full strength in
  production code. A `Result`-returning test that also asserts carries
  `clippy::panic_in_result_fn` in the same scoped relaxation its file already
  uses for `panic`/`unwrap`/`expect`. It is never relaxed at the workspace
  level, and never in a non-test module.
- **`#[should_panic]` always carries `expected = "…"`.** A bare `should_panic`
  passes when the code panics for the wrong reason (Book ch11.1), which is
  unacceptable in a suite that adjudicates spec behaviour. `should_panic` is
  illegal on a `Result`-returning test; assert `value.is_err()` there.
- **Assertions:** `assert_eq!`/`assert_ne!` over bare `assert!` for
  comparisons, since they print both values. Production-code asserts carry a
  message.
- **Doctests are copy-paste templates:** `?` through a hidden `# Ok::<(), E>(())`
  tail, never `unwrap`. Use `no_run` for examples that would need a live SUT,
  `text` for non-code, and never `ignore` ("almost never what you want",
  <https://doc.rust-lang.org/rustdoc/write-documentation/documentation-tests.html>).

## Catalogue coverage (breadth is a mandate, not just pass rate)

A green pipeline over a thin catalogue proves almost nothing. The catalogue
must exercise EVERYTHING the spec defines on the wire: every SM operation,
every status-code branch (200 / 201 / 204 / 400 / 404 / 409 / 412 / 422 and
the rest), every required or conditional header (`ETag`, `Location`,
`Last-Modified`, `Prefer`, `If-Match`), the content-negotiation variants (JSON
and XML, `Accept` q-values), every precondition and error family, and every RM
and AQL behaviour. Each is its own small isolated case, so a red row localizes
to one behaviour.

- **A spec-defined wire behaviour with no case is a COVERAGE GAP, never an
  acceptable omission.** Close it with a new spec-cited case, or, only where
  the spec genuinely puts a behaviour off the wire, record the honest boundary
  as a statement-declared capability or a register entry. Silence is not
  coverage.
- **Coverage only ratchets up.** Cases are added, never removed to go green.
  Narrowing coverage needs an adjudicated, spec-cited reason.
- **One behaviour per case.** Many small isolated cases beat one broad case,
  because a failure then names exactly one defect, which is also what makes
  the attribution law tractable.
- **An adjudicated spec-correct refusal always yields BOTH twins.** When
  triage attributes a red row to a defective fixture the SUT was spec-right to
  refuse, fixing the fixture is half the job: the invalid shape is preserved
  as its own corpus entry (`validity: invalid`, with the defect and its
  `spec_ref`) plus a refusal case, so the catalogue carries the valid twin
  (acceptance proven) and the invalid twin (the refusal pinned, so a lenient
  server fails it). Deleting the invalid shape silently narrows coverage.
- Vendored corpora carry the same completeness discipline: 100% exercised,
  coverage-gated, with adjudicated skips only. Never partial coverage that
  silently narrows the claim.

## Fixture construction: raw JSON only where raw is the point

1. **Refusal and negative fixtures: raw bytes, MANDATORY.** An invalid shape
   (a missing mandatory attribute, an empty `1..*` list, an undeclared key) is
   unrepresentable in a typed model, so raw bytes are the only way to author
   what the reader must reject.
2. **Wire inputs posted to a SUT: raw JSON permitted.** Independently authored
   bytes catch codec bugs that typed-then-serialized values cannot.
3. **Everything else** (expected values, in-memory construction): build the
   typed value and serialize it, so the fixture stays correct across a pin
   bump.

## Target

The standing bar is a green pipeline whose baseline only ratchets upward, and
green comes ONLY from fixing the guilty component after spec-adjudicated
attribution (`.claude/rules/cnf-triage.md`). Never from bending the catalogue
or the runner to match a server. Every change ships as a compiling, tested
increment.
