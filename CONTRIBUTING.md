# Contributing to Veredictum

Thank you for your interest in contributing. This document covers the practical
rules. The working discipline that every change is held to lives in the root
[`CLAUDE.md`](CLAUDE.md) and the rule files under
[`.claude/rules/`](.claude/rules/); read those before writing anything.

There is no contributor licence agreement and no copyright assignment. You keep
your copyright, and the licence stays Apache-2.0 for everyone, including the
maintainer. What goes in is what comes out.

Contributions are not limited to code. A conformance case for behaviour the
catalogue does not cover, a specification citation that refutes an expectation,
a defect report against a CDR that this instrument mis-judged, a documentation
correction, or a measurement from your own hardware all count.

## Setup

- **Install the shared git hooks once:** `bash scripts/install-hooks.sh`. It
  points `core.hooksPath` at [`.githooks/`](.githooks/), whose `commit-msg` hook
  strips any tool attribution from a commit message before the commit is
  recorded.
- **The toolchain pins itself.** `rust-toolchain.toml` names the exact channel,
  so `cargo` fetches it on first use and there is nothing to install by hand.
- **`cargo-nextest` is the test runner**, never `cargo test`:
  `cargo install cargo-nextest --locked`, or take it from
  [its releases](https://nexte.st/docs/installation/pre-built-binaries/).

## Before you start

- **The released openEHR specifications are the only authority.** Read the
  governing section first-hand and quote the sentence that assigns the value.
  The specification text is in this repository, at `specs/openehr/`. An XSD,
  JSON-Schema or OpenAPI citation resolves against the released bundles beside
  it (`specs/its-xml-schemas/`, `specs/its-json-schemas/`, `specs/rest-oas/`),
  because the documentation tree carries only prose for those components.
- **No CDR's behaviour is evidence of what is correct**, including the CDR you
  work on. A server response is evidence in a comparison against the spec, and
  the spec is the reference.
- **The CNF Platform Conformance Test Schedule is not authority either.**
  openEHR never released a stable version of it, so it says which behaviours to
  cover, not what the correct answer is.

## The gates

Run these before you push. CI runs the same ones, behind a single required
`conclusion` check, and gates the Rust tier on whether your change touched
anything it reads.

```bash
scripts/checks/gates.sh          # the documented battery, in one place
scripts/checks/gates.sh --list   # what it runs
```

That last one is the gate the catalogue lives or dies by: it is every machine
check over the artifact tree, and zero findings is the only passing result.

And for every pull request, prose and configuration included:

```shell
bash scripts/checks/comment-style.sh --all   # comment form and budgets
```

plus the attribution rule below, which the `.claude/hooks/` guards enforce
locally at the moment a command runs. The full gate list is
[`CLAUDE.md`](CLAUDE.md) § Build and test, and `veredictum validate` is the
gate that must be clean before any server is composed. A change to the web
console has its own gate set: `app/veredictum-console/CLAUDE.md` § Gates
(clippy on both targets, the E2E journeys, and the screenshot guard).

Changing a reader that parses text from outside — a grammar, the citation
splitter, an artifact or a party document — also means running its fuzz target.
The harnesses live in [`fuzz/`](fuzz/README.md), in their own nightly
workspace, and CI compiles them on every pull request that touches `app/` or
`fuzz/`:

```shell
fuzz/seeds.sh citation
cargo +nightly fuzz run citation fuzz/corpus/citation fuzz/seeds/citation \
  -- -max_total_time=120 -max_len=4096 -timeout=25
```

## Hard rules

These are the ones a pull request is most often refused for. The full set is in
[`CLAUDE.md`](CLAUDE.md).

- **Every expectation cites its specification section.** An expectation with no
  citation is not reviewable, because there is nothing to refute it with.
- **Never weaken, skip, or delete a test**, and never edit a test to route
  around a defect it exposes. If the fix is unclear, leave the test failing and
  record a `// TODO(#NNNN):` naming its issue.
- **Coverage ratchets up only.** A case is added, never removed to make a run
  green. Narrowing coverage needs an adjudicated, spec-cited reason.
- **A red row is attributed before anything is changed**, to the server under
  test, the runner machinery, or the catalogue, by comparing spec-required
  against catalogue-expected against server-observed
  ([`.claude/rules/cnf-triage.md`](.claude/rules/cnf-triage.md)). The
  instrument is a first-class suspect on every red row.
- **Comments follow RFC 505 and RFC 1574 with budgets**
  ([`.claude/rules/comments.md`](.claude/rules/comments.md)): line comments
  only, `// TODO(#NNNN):` for pending work, `// NOTE:` for a settled decision
  as a citation plus one sentence. Adjudication essays belong on the issue.
- **Cite only durable references.** The vendored spec text or official external
  documentation. Never an internal markdown file, because internal documents
  move or die.

## Pull requests

- Branch from `main` with a conventional-type branch name:
  `feat/`, `fix/`, `chore/`, `docs/`, `refactor/`, `perf/`, `test/`, `ci/`,
  `build/`, `release/`.
- Commit subjects are conventional-commit style and describe the change itself.
- **Commits are signed.** `git commit -S`, verifiable as `G` in
  `git log --format=%G?`. Every commit in this repository's history is signed,
  and the `main` ruleset refuses an unsigned one — every change reaches `main`
  through a squash-merged pull request whose CI `conclusion` check is green. See
  [SECURITY.md § Repository security settings](SECURITY.md#repository-security-settings--the-posture-of-record).
- **No AI or tool attribution anywhere.** No `Co-Authored-By` trailer of any
  kind, no "Generated with", no robot emoji, in a commit message, a commit
  trailer, a pull-request title or body, an issue, or a code comment. Commit
  and pull-request text describe the change and nothing else. If you used an AI
  tool, disclose it in the pull-request description per
  [`AI_STATEMENT.md`](AI_STATEMENT.md) § 10, which is where disclosure belongs.
- The pull-request body declares `Closes #<n>`. One `Closes` keyword per issue:
  `Closes #1, #2` closes only #1.
- A user-visible change adds an entry under `## [Unreleased]` in
  [`CHANGELOG.md`](CHANGELOG.md) in the same pull request.
- Tests accompany behaviour changes.

## Reporting issues

Use the [issue tracker](https://github.com/rubentalstra/Veredictum/issues/new/choose).
[SUPPORT.md](SUPPORT.md) has the routing for questions, defects, and reports
about a CDR rather than about the instrument.

For a suspected security vulnerability, do not open a public issue. See
[SECURITY.md](SECURITY.md).

## Code of conduct

This project follows the [Contributor Covenant](CODE_OF_CONDUCT.md).
