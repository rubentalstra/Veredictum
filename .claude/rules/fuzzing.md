---
paths:
  - fuzz/**
  - .github/workflows/fuzz.yml
---

# Fuzzing (`fuzz/**`)

libFuzzer harnesses over every reader that parses text or bytes the instrument
did not write. No openEHR specification governs fuzzing — this is our own
verification design; the authority is the
[Rust Fuzz Book](https://rust-fuzz.github.io/book/cargo-fuzz.html) and the
documentation of `cargo-fuzz` and `libfuzzer-sys`.

`fuzz/README.md` is the reference material: the threat model, the target table,
the seed sources, the local commands. This file is the standing discipline —
what a harness must be, what a crash means, and what happens after one.

## Why it exists beside the artifact gates

The integration suite exercises the VALID space: a real catalogue loads, a
seeded defect is refused, a recorded transcript reproduces its verdicts. The
defects this lane catches live in the malformed space, and the first campaign
found three of them in one evening — two unbounded recursions in the
decision-table literal reader and an unbounded brace expansion in the citation
reader. Each turned a document the instrument was JUDGING into a way to stop the
instrument.

That is the bar for adding a target: **can input from outside kill the process,
hang it, or make it answer wrongly?** If yes, it belongs here.

## What a harness must be

- **A pure parse.** No I/O, no network, no global mutable state. A finding must
  reproduce from the recorded input alone, because that is what makes
  `cargo fuzz run <target> <artifact>` a complete bug report.
- **Deterministic.** No clock, no RNG, no thread scheduling in the path. A
  non-deterministic harness turns a corpus into noise.
- **Panic-on-defect, not panic-on-invalid.** Malformed input is the point: the
  reader is expected to refuse it with a typed error. The harness asserts the
  absence of panics, aborts and hangs, plus any invariant the reader itself
  documents — never an invariant the harness invented, which produces a finding
  that is a harness defect wearing a crash's clothes.
- **Cheap per execution.** libFuzzer needs a high execution rate. Compile a
  schema or any other fixed input once in a `LazyLock`, never per execution.

## A crash is fixed in the crate, never in the harness

The same law as `cnf-triage.md`, applied here. When a target crashes, the bug is
in the reader until proven otherwise:

- **Never widen a bound to make a crash go away.** If a nesting limit is hit,
  the question is whether the limit is right, not whether the input is unfair.
- **Never delete or narrow a target** to go green, and never disable the
  sanitizer. Coverage ratchets up only.
- **Never move the fix into the harness.** A crash that turns out to be a
  harness defect — a wrong invariant, a non-deterministic path — is fixed as
  one, and the commit says which it was. The attribution matters as much as the
  fix, exactly as it does on a red conformance row.

## The crash-to-regression-test procedure

Fixed, in this order. A finding that is only fixed comes back.

1. **Minimize** the artifact:
   `cargo +nightly fuzz tmin <target> fuzz/artifacts/<target>/<input>`.
2. **Attribute** it. Which reader, and what does the released grammar or the
   documented contract actually say the bound is? A bound invented to silence a
   crash is not a fix.
3. **Fix the reader** in `app/veredictum/src/`, with the bound named as a constant and its doc
   comment saying what the legitimate depth or width is, so the next reader can
   tell a real limit from a guess.
4. **Pin it as a normal test** in the module that owns the reader — a
   `#[cfg(test)]` case that asserts the typed REFUSAL, plus one authored form
   that must still parse, so a later widening breaks a test rather than
   reopening the hole.
5. **Record the artifact** under `fuzz/regressions/<target>/`, named
   `<kind>_<issue>_<slug>` (`hang_11_brace_expansion`). That directory is
   tracked, and `fuzz/seeds.sh` links it into the seed set after the wipe, so
   every run from then on re-checks every finding this lane has ever had.
6. **Replay** it: `cargo +nightly fuzz run <target> fuzz/regressions/<target>/*`
   must exit clean.
7. **Changelog and issue.** A bound that changes what the instrument accepts is
   user-visible; it gets a `CHANGELOG.md` entry under Unreleased in the same
   pull request.

`fuzz/artifacts/` is scratch and is not tracked. `fuzz/regressions/` is the
record.

## The traps this lane has already hit

- **`fuzz/` is its own workspace on purpose.** cargo-fuzz needs nightly; this
  repository is pinned to stable 1.97.1. Never add the package to the root
  workspace, and never let a root `cargo build`, `clippy` or `nextest` reach it.
- **`--target` is mandatory in CI.** cargo-fuzz defaults the build target to the
  triple IT was built for rather than the runner's, and `install-action`
  resolves it through a binstall fallback to the musl asset — whose static libc
  cannot carry a sanitizer (`error: sanitizer is incompatible with statically
  linked libc`). Both jobs name `x86_64-unknown-linux-gnu` explicitly. Do not
  remove it, and never answer a recurrence by disabling the sanitizer: the
  sanitizer is the instrument.
- **A scheduled-only lane rots silently.** Nothing on the pull-request path
  would compile the harnesses, so a renamed function would break the lane and
  nobody would learn until the next campaign. The `build` job exists for that
  and must keep firing on changes to `fuzz/**` or `app/**`.

## Adding a target

1. Add the harness under `fuzz/fuzz_targets/` and its `[[bin]]` in
   `fuzz/Cargo.toml`.
2. Add it to the matrix in `.github/workflows/fuzz.yml` with a `max_len` chosen
   for the format, and to the table in `fuzz/README.md`.
3. Give it seeds in `fuzz/seeds.sh`, from material already committed here.
4. Run it locally past the trivial inputs before claiming it works. A target
   that has never left the seed corpus has proven nothing.

## Enforcement register

| Property | Check |
|---|---|
| The harnesses compile | the `build` job, on every pull request touching `fuzz/**` or `app/**` |
| Crashes surface | the weekly campaign; a crash uploads its artifact and fails the job |
| Every past finding is re-checked | `fuzz/regressions/` is tracked and seeded into every run |
| Every fixed crash is pinned by a test | the test suite, plus **review** |
| Corpus accumulates | the Actions cache, keyed per target |
| Harness purity and determinism | **review-enforced** — no tool can judge it |
| A crash fixed in the crate, not the harness | **review-enforced** |
