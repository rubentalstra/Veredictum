---
paths: ["app/**/*.rs"]
---

# Comments & documentation (RFC 505 + RFC 1574)

> Ported from FerroEHR's `.claude/rules/comments.md` at the Veredictum split
> (FerroEHR#2789). The rules are generic and travel verbatim; only the paths
> and the enforcement register were adapted.

The authority is the official Rust API documentation conventions — RFC 505
(https://rust-lang.github.io/rfcs/0505-api-comment-conventions.html) and
RFC 1574 (https://rust-lang.github.io/rfcs/1574-more-api-documentation-conventions.html)
— plus the rustdoc book. Comments are the fastest-rotting artifact in a
repository; essays in code are banned. This file is the single home of the
comment rules.

## The prime rule: a comment earns its lines

Code says WHAT; a comment exists only for what the code cannot show — the
spec citation, the non-obvious why, the constraint. Everything else already
has a durable home and goes there, not into the source:

| Content | Home |
|---|---|
| Adjudications, spec-conflict essays, history | the PR description / tracker issue |
| Design decisions | the root `CLAUDE.md`, `.claude/rules/*.md` |
| API usage, contracts, examples | doc comments (`///`) |
| What changed and why it is correct | the PR — never the code |

**No change-narration in comments**: "previously…", "now correctly…",
"before this consolidation…", "the former X is retired" is PR text; git
history carries it. A comment describes the code as it IS.

## Budgets (machine-enforced — `scripts/checks/comment-style.sh`)

- `// NOTE:` = a citation + ONE sentence, **max 3 physical lines**. The
  full adjudication lives on the issue/PR.
- A plain `//` comment run is **max 8 physical lines**. Longer prose is
  either API documentation (move it into the item's `///` docs) or a record
  (move it to the PR/issue).
- Block comments (`/* … */`, `/** … */`) are banned — "Avoid block comments.
  Use line comments instead" (RFC 505). `/* arg */` parameter labels become
  named locals.

## Doc comments (`///`)

- `///` documents items; `//!` is ONLY for crate- and module-level docs
  ("nothing else" — RFC 1574); above a `mod` block prefer `///` outside it.
- **Summary line**: the first line is a single short sentence, third person
  singular present indicative ("Returns…", "Creates…" — never "Return" or
  "This function returns"), properly punctuated, followed by a blank `///`
  line before any detail (`clippy::too_long_first_doc_paragraph`).
- Full sentences over fragments; American English spelling.
- Markdown with `#` top-level section headings, in this order where present:
  `# Examples` (always plural, even for one example), `# Panics`,
  `# Errors`, `# Safety` (`missing_errors_doc` / `missing_panics_doc`
  enforce presence; `unnecessary_safety_doc` bans `# Safety` on safe code).
- Code fragments in backticks (`clippy::doc_markdown`); longer examples in
  triple-backtick blocks, non-Rust blocks explicitly tagged (```text) —
  rustdoc tests every untagged block as Rust. Doctest shapes: testing.md
  §Test shapes.
- Name generic types fully (`Option<T>`, not `Option`); link with intra-doc
  links (`[`Type`]`) and reference-style links; bare URLs in `<…>`.
- Doc comments state the CURRENT contract only — no history, no
  adjudication trail. Citations name the vendored openEHR spec text or
  official external documentation, never an internal markdown file.

## Annotation vocabulary (the only sanctioned markers)

- `// TODO(#NNNN): <what is missing>` — pending work, ALWAYS with its
  tracker issue. Deferred work is a TODO, never prose ("lands later",
  "deferred to X" is banned); phase and plan markers (A5, P16, W-nn) are
  banned tracker IDs.
- `// NOTE: <citation + one sentence>` — a SETTLED decision: the spec
  citation, or the explicit flag "no openEHR spec governs this — our own
  design". If it describes something not yet done, it is a TODO.
- `// SAFETY:` — reserved for `unsafe` (forbidden anyway;
  `unnecessary_safety_comment` denies misuse).
- No other marker form exists: `FIXME`, `HACK`, `XXX`, `WIP`, and any
  bespoke vocabulary fail the guard.

## Enforcement register

- `scripts/checks/comment-style.sh` — block comments, TODO(#N) form, banned
  marker vocabulary, NOTE ≤ 3 lines, `//` runs ≤ 8 lines, and no `.claude/`
  path anywhere in a hand-written `.rs` file (root `CLAUDE.md` rule 11: code
  cites the vendored spec text or official external documentation, never an
  internal document). That last check reads the whole file rather than only
  comment lines, because an `#[expect(reason = "…")]` string is read as
  justification exactly like a comment. Runs per-edit via the
  `rust_fmt_clippy.sh` PostToolUse hook, and per-PR through the `--all` step
  in the CI guards job.
- `scripts/checks/todo-issue-refs.sh` — the `TODO(#NNNN):` form OUTSIDE the
  Rust tree (#264): every committed hand-written YAML, shell and TOML file,
  vendored trees excluded. Self-tested with seeded violations, run in the CI
  guards job beside the `.rs` guard.
- **Adjudicated 2026-08-27 (#125): a corpus-local README is NOT a sanctioned
  exception to rule 11.** A document committed beside the material it
  describes moves and dies like any other internal document, so the guard
  refuses every in-repo markdown citation — an `artifacts/`, `party/`,
  `scripts/`, `website/`, `schemas/`, `fuzz/`, `app/` or `verification-pack/`
  path ending in `.md`, and the root documents by name — and a comment grounds
  on the material itself instead (the committed test keypair's armored
  certificate carries its own user id and subkey binding, so nothing outside
  the packets has to state its identity). That check reads COMMENT LINES only,
  because `specs/**` is full of `.md` documents that ARE the oracle and a
  string the code emits may legitimately name a markdown file the tool writes.
  `scripts/checks/comment-style.sh --self-test` is its seeded-violation proof
  and runs in the CI guards job. Widened 2026-08-29 (#176) to every path form:
  the guard builds the suffix set of every committed non-spec markdown file, so
  an artifact-root-relative citation (`corpus/recipes/bp_series.md`) is refused
  exactly like the full path, while a suffix naming nothing committed and a URL
  ending in `.md` both stay legal.
- **Adjudicated 2026-08-29 (#176): manifest provenance naming a digest-pinned
  recipe contract is INSIDE the rules.** Each generated set in
  `artifacts/corpus/MANIFEST.yaml` binds its recipe through a `generated_by`
  digest, which makes the named contract file load-bearing catalogue data: edit
  the file and the digest no longer matches, so the reference fails loud instead
  of rotting silently. Rule 11 governs prose comments, and a pinned data
  reference is not prose. A comment in a `.rs` file still may not name that same
  file, because a comment carries no digest and nothing catches it going stale.
- `clippy::too_long_first_doc_paragraph` (nursery cherry-pick, CI
  `-D warnings`) — the RFC 1574 summary line.
- Doc lints, all live: `doc_markdown`,
  `missing_errors_doc`, `missing_panics_doc` (pedantic = deny),
  `unnecessary_safety_comment`, `unnecessary_safety_doc`, the
  `[lints.rustdoc]` table plus a CI doc job.
- Review-enforced (no tool can judge them): change-narration, prose
  deferrals, third-person summary phrasing, essays relocated into doc
  comments to dodge the `//` budget.
