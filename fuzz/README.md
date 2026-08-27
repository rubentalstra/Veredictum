<!--
SPDX-FileCopyrightText: Veredictum contributors
SPDX-License-Identifier: Apache-2.0
-->

# Fuzzing the readers that take text the instrument did not write

libFuzzer harnesses, through [cargo-fuzz], over every reader in this crate that
parses text or bytes from outside. Each harness is a **pure parse** — no I/O, no
network, no global mutable state — so a finding reproduces from the recorded
input alone.

## The threat model, stated plainly

Veredictum is a command-line instrument, not a server, so "attacker-controlled"
needs saying rather than assuming. Three inputs come from outside:

- **A party's published record.** `veredictum verdicts` re-derives a conformance
  verdict from a `results.json` and a `statement.json` written by the
  organization whose product is being judged. Re-checking somebody else's claim
  is the entire purpose of the verdict pipeline, and the encoded HDR histograms
  inside a results record are bytes this runner did not produce.
- **A third-party catalogue.** `validate --root <their artifacts>` is an
  advertised use: anyone may author cases and run them. Every closed grammar in
  the crate reads that YAML.
- **An operator's IXIT.** It declares endpoints, credentials and file-system
  paths for a deployment this repository knows nothing about.

A parse failure on any of those is normal and expected. What must never happen
is a panic, an abort or a hang, because the instrument's answer is the product:
a run that dies mid-catalogue reports nothing, and an instrument that can be
stopped by a document it is judging is one a party can stop.

## The targets

| target | reads | entry points |
|---|---|---|
| `reference_grammar` | the `${…}` references, capture sources and identifier spaces of a case core or binding | `Template`/`ValueRef`/`CaptureValueSource::parse`, every `ids` newtype, `WireFrom`/`HeaderMatcher::parse` |
| `literal_grammar` | every decision-table cell and `violates` entry of a content chapter | `Literal::from_text`, `Literal::from_cell`, `ViolationRef::parse` |
| `citation` | the `spec_refs` of every case, binding and register entry | `citation_clauses`, `expand_braces`, `section_candidates` |
| `artifact_yaml` | a case core end to end: YAML under the budget, the published schema, the typed model | `yaml_str_to_value`, `validate_against`, `CaseCore` deserialization |
| `party_document` | the IXIT, the statement and the results record | `Ixit`/`Statement`/`Results` deserialization, `check_invariants` |
| `hdr_v2` | the encoded latency histograms a measured record publishes | `OperationMeasurement::decode_histogram`, then the quantiles `class_verdict` reads |

Three targets assert a property beyond the absence of a crash.
`reference_grammar` requires that a reference the grammar ACCEPTS renders back
to text that parses to the same reference, because the runner resolves against
the rendered form. `citation` requires that brace expansion stays under its
documented 32-variant ceiling. `party_document` requires that every declared
IXIT instance is reachable by the name it was declared under.

## Seeds

`fuzz/seeds.sh` builds `fuzz/seeds/<target>/` from material already committed
here. Documents are symlinked, never copied. Fragments — a `${…}` reference, a
citation, a decision-table cell — are harvested out of the catalogue and written
one per file, because libFuzzer takes one input per file and there is nothing to
link to.

```sh
fuzz/seeds.sh                # all targets
fuzz/seeds.sh citation       # one target
```

The script fails loud when a source directory has moved or a harvest matches
nothing, so a renamed path can never quietly degrade into an empty seed set. Two
targets seed from written shapes as well: the grammar forms no artifact happens
to carry, and — for `hdr_v2`, where this repository has no committed histogram —
the V2 cookie with plausible and implausible headers behind it.

Selection is size-bounded and deterministic. libFuzzer re-reads every seed on
each run and derives its default input length from the largest one.

## Recorded regressions

`fuzz/seeds/`, `fuzz/corpus/` and `fuzz/artifacts/` are generated and ignored.
`fuzz/regressions/<target>/` is the tracked half: every input that reproduced a
real crash, hang or leak is committed there and linked into that target's seed
directory after the wipe, so a finding is re-checked by every run from then on.

The corpus is a search, not a test suite. A fixed defect is also pinned by a
normal test in the crate — see `.claude/rules/fuzzing.md`, which carries the
procedure. Reproducing one artifact directly, by naming the file:

```sh
cargo +nightly fuzz run citation fuzz/regressions/citation/hang_11_brace_expansion
```

Never pass `fuzz/regressions/<target>/` as the FIRST corpus argument: libFuzzer
treats its first corpus directory as writable and fills it with generated
inputs. The seed directory reaches it read-only, as the second argument.

## Running

cargo-fuzz needs a nightly toolchain, which is why `fuzz/` is its own workspace:
no `cargo build`, `cargo clippy` or `cargo nextest` at the repository root ever
compiles this package, and the root stays on its pinned stable 1.97.1.

```sh
rustup toolchain install nightly
cargo install cargo-fuzz --locked

fuzz/seeds.sh

# A short run, the shape the scheduled lane uses.
cargo +nightly fuzz run literal_grammar \
  fuzz/corpus/literal_grammar fuzz/seeds/literal_grammar \
  -- -max_total_time=120 -max_len=4096 -timeout=25

# A long local campaign: several cores, no time limit, until you stop it.
cargo +nightly fuzz run --jobs 8 literal_grammar \
  fuzz/corpus/literal_grammar fuzz/seeds/literal_grammar \
  -- -max_len=4096 -timeout=25
```

`fuzz/seeds.sh` also creates `fuzz/corpus/<target>/`, because libFuzzer refuses
to start when a corpus directory it was given does not exist.

Coverage of a corpus, to see what a campaign actually reached:

```sh
cargo +nightly fuzz coverage citation fuzz/corpus/citation
```

## What the first campaign found

Three defects, all in the crate and all fixed there, on the first local run of
the lane (issue #11):

- `Literal::from_text` recursed once per `[` and once per `|` with no depth
  bound. A decision-table cell nesting 4000 deep overflowed the stack, and a
  Rust stack overflow aborts rather than unwinding, so the validator died
  instead of reporting a finding. Bounded at
  `literal::MAX_NESTING`; the grammar's own forms reach three levels.
- The same reader had the same hole through the ordinal and scale tuple
  productions, reached by `1|1|1|…` rather than by brackets.
- `expand_one_token` expanded a citation's `{a,b}` groups with no ceiling. The
  32-variant bound was applied ACROSS a clause's tokens but not WITHIN one, so a
  113-byte citation carrying 22 groups asked for four million strings and hung
  the validator. Bounded at `validate::MAX_CITATION_VARIANTS` per token.

`hdr_v2` found nothing in a million executions, and its coverage stays narrow:
the V2 cookie check refuses nearly everything before a byte is allocated, and a
header declaring a huge trackable range is rejected by the decoder rather than
honoured. That is the honest reading of a quiet target, not a claim that the
path is proven.

## CI

`.github/workflows/fuzz.yml` compiles every harness on the pull-request path and
runs a bounded campaign per target on a weekly schedule, with each corpus kept
in the Actions cache so coverage accumulates between runs. The build job exists
because a scheduled-only lane rots silently: a harness that stops compiling
because a function was renamed would not surface until the next campaign.

## Working in an IDE

`fuzz/` is a separate Cargo workspace, so an IDE that knows only the root
workspace reports "file does not belong to a known Cargo project" for the
harnesses. Attach `fuzz/Cargo.toml` as a second Cargo project (in RustRover:
*File → Link Cargo Project*). Nothing in the repository needs to change.

[cargo-fuzz]: https://rust-fuzz.github.io/book/cargo-fuzz.html
