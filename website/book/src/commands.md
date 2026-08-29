# Command reference

<!-- toc -->

Every flag below is the one the binary declares. `veredictum <command> --help`
prints the same list from the build you have installed, and that output is the
authority if the two ever disagree.

Three commands make the conformance record (`validate`, `run`, `verdicts`), two
measure (`perf`, `stress`), `verify-record` checks a sealed bundle, and the rest
render or explore.

## validate

Validate one artifact tree through every machine gate.

```bash
veredictum validate --root <ROOT> [--specs <SPECS>] [--write-report]
```

| Flag | Meaning |
|---|---|
| `--root <ROOT>` | The artifact root holding `schedule/`, `bindings/`, `vocab/`, `corpus/` and `registers/`. Required |
| `--specs <SPECS>` | The vendored openEHR specification tree. Supplying it enables Service-Model operation resolution and citation resolution |
| `--write-report` | Also refresh the wire-surface coverage report, at `<ROOT>/coverage-report.md`, from `--specs` |

Every machine check over the catalogue: identifier uniqueness, citation
resolution, binding completeness, coverage of the enumerated wire surface, and
claim completeness against the committed party statements. It prints one line
per finding and a summary line, and exits `1` if the finding count is not zero.

`--write-report` is off by default on purpose. A check verb that rewrites a file
on every run is a trap for read-only invocations, so the pipelines that publish
the coverage report ask for it explicitly. The report lands at
`<ROOT>/coverage-report.md`, beside the artifact families it measures.

## run

Execute the catalogue against a live SUT and emit `results.json` plus the run
report.

```bash
veredictum run --root <ROOT> --ixit <IXIT> --out <OUT> \
    [--sut-name <NAME>] [--sut-version <VERSION>] \
    [--filter <SUBSTRING>] [--statement <STATEMENT>] \
    [--record-exchanges] [--sign-key <KEY>]
```

| Flag | Meaning |
|---|---|
| `--root <ROOT>` | The artifact root. Required |
| `--ixit <IXIT>` | The IXIT topology file describing the deployment under test. Required |
| `--out <OUT>` | Output directory for `results.json` and the run summary. Required |
| `--sut-name <NAME>` | Display name for the system under test. Default `ferroehr` |
| `--sut-version <VERSION>` | Version label for the system under test. Default `dev` |
| `--filter <SUBSTRING>` | Only run cases whose identifier contains this substring |
| `--statement <STATEMENT>` | The party statement. When supplied, an option-gated case whose option the statement does not declare is recorded not-applicable at drive time instead of driven |
| `--sign-key <KEY>` | An armored OpenPGP secret key. Seals the emitted documents with `record-manifest.json` and its detached signature |
| `--sign-passphrase <PASSPHRASE>` | The passphrase unlocking `--sign-key`, read from `VEREDICTUM_SIGN_PASSPHRASE` |
| `--record-exchanges` | Persist the wire exchanges beside `results.json` as `transcript.json`. Off by default. The artifact records a SUT's response bodies verbatim, so it can carry real patient data: it is operator-controlled output, never a log, and belongs wherever the record itself is stored. The `authorization` request header's value is withheld. With `--sign-key` the sealed manifest covers the transcript too |
| `--progress` | Print one machine-parseable line per processed case: `progress: 0/<n>` once the selection is final, then `progress: <k>/<n> <case-id>` as each case is processed. Off by default, so existing output is byte-identical without it |

Drives every applicable case and records the exchange. Exits `1` if any case
failed or errored.

## verdicts

Compute the verdicts from a statement and a results record against an artifact
tree, and write the rendered submission documents.

```bash
veredictum verdicts --statement <STATEMENT> --results <RESULTS> \
    --root <ROOT> --out <OUT> [--sign-key <KEY>]
```

| Flag | Meaning |
|---|---|
| `--statement <STATEMENT>` | The party statement, `statement.json`. Required |
| `--results <RESULTS>` | The recorded results, `results.json`. Required |
| `--root <ROOT>` | The artifact root. Required |
| `--out <OUT>` | Output directory for the rendered documents and `verdicts.json`. Required |
| `--sign-key <KEY>` | An armored OpenPGP secret key. Seals the rendered documents with `record-manifest.json` and its detached signature |
| `--sign-passphrase <PASSPHRASE>` | The passphrase unlocking `--sign-key`, read from `VEREDICTUM_SIGN_PASSPHRASE` |

The pure step. It reaches no network and reads nothing but its inputs, which is
what makes a published verdict re-derivable by anyone who has the same four
files.

## verify-record

Verify a sealed bundle: recompute every digest its record manifest names, and
check the detached signature over that manifest.

```bash
veredictum verify-record --record <DIR> --key <KEY>
```

| Flag | Meaning |
|---|---|
| `--record <DIR>` | The bundle directory holding the emitted documents, `record-manifest.json` and `record-manifest.json.asc`. Required |
| `--key <KEY>` | The armored OpenPGP public key the signature is checked against. Required |

Prints the signer fingerprint, the signing time, and one line per file with its
digest verdict. Zero findings is the only passing result: a digest mismatch, a
file the manifest names but the bundle does not carry, or a signature no
component of the supplied key verifies, each exits `1` naming what failed.

The bundle is ordinary files, so the check does not depend on this tool.
`gpg --verify record-manifest.json.asc record-manifest.json` establishes the
same signature, and `sha256sum` re-derives the same digests.

A verified bundle is one link in the chain and not the whole of it. A valid
signature proves integrity and origin since signing, and says nothing about the
conditions the run executed under. The published instrument, the verification
pack and the citation-carrying record are the rest, which is why that sentence
prints on every verification.

## perf

Execute the performance schedule's open-loop measured run against a live SUT and
merge the measurement records into an existing `results.json`.

```bash
veredictum perf --root <ROOT> --ixit <IXIT> --results <RESULTS> --class <CLASS> \
    [--seed-workers <N>] [--hours <H>]
```

| Flag | Meaning |
|---|---|
| `--root <ROOT>` | The artifact root. Required |
| `--ixit <IXIT>` | The IXIT topology file. Its `environment` block is mandatory for a measured run. Required |
| `--results <RESULTS>` | The `results.json` written by a prior `run`, to merge the measurement records into. Required |
| `--class <CLASS>` | Which performance case to select: `POC`, `S`, `L` or `R`. Required |
| `--seed-workers <N>` | Parallel seeding workers. Default `16` |
| `--hours <H>` | The sustained window: `1` (the case's normative window, the default), `2`, `4`, `6`, `8` or `12` |

Conformance by measurement. Latency is measured from the planned arrival instant
under open-loop offered load, so a stall shows up as latency instead of
disappearing into a slowed-down request rate. A longer `--hours` window is a
stricter demonstration and persists like any measured run; nothing shorter than
the case exists.

The `environment` block is mandatory rather than optional because a latency
number is a claim about a deployment, and a claim with no deployment described
cannot be checked or reproduced.

## stress

Run the step-load stress instrument: geometric load steps up to the maximum
sustainable throughput.

```bash
veredictum stress --root <ROOT> --ixit <IXIT> --out <OUT> \
    [--corpus-class <CLASS>] [--seed-workers <N>] \
    [--step-secs <S>] [--bisections <N>] [--max-rate <R>]
```

| Flag | Meaning |
|---|---|
| `--root <ROOT>` | The artifact root. Required |
| `--ixit <IXIT>` | The IXIT topology file. Its `environment` block is mandatory. Required |
| `--out <OUT>` | Where to write the stress report, `stress.json`. Required |
| `--corpus-class <CLASS>` | The class-scale corpus the stress runs on: `POC`, `S`, `L` or `R`. Data volume and workload mix only. Default `POC` |
| `--seed-workers <N>` | Parallel seeding workers. Default `16` |
| `--step-secs <S>` | Each load step's recorded hold, in seconds. Default `120` |
| `--bisections <N>` | Post-breach bisection refinements. Default `3` |
| `--max-rate <R>` | The climb cap, in arrivals per second. Default `4096` |

Exploration only, and class-free by design: no class floor enters the stress
report or its chart. A `stress.json` is never a conformance record, and quoting
one as if it were is a misuse of the tool.

## aql-probe

Run the AQL optimization probe against a live, freshly seeded SUT.

```bash
veredictum aql-probe --root <ROOT> --ixit <IXIT> --out <OUT> \
    [--corpus-class <CLASS>] [--seed-workers <N>] [--requests <N>]
```

| Flag | Meaning |
|---|---|
| `--root <ROOT>` | The artifact root. Required |
| `--ixit <IXIT>` | The IXIT topology file. Its `containers` block enables database-side attribution and maintenance settling. Required |
| `--out <OUT>` | Where to write the probe report, `aql-probe.json`. Required |
| `--corpus-class <CLASS>` | The class-scale corpus the probes run against: `POC`, `S`, `L` or `R`. Default `POC` |
| `--seed-workers <N>` | Parallel seeding workers. Default `16` |
| `--requests <N>` | Requests fired per probe. Default `20` |

Fires the measurement machinery's AQL vocabulary, records wire percentiles per
probe, and attributes the database-side cost through `pg_stat_statements`. This
is evidence for someone optimizing a server, and it is never a conformance
record.

## bench

Run the universal speed benchmark against any reachable CDR.

```bash
veredictum bench --base-url <URL> --out <OUT> \
    [--auth none|basic|bearer] [--user <USER>] [--pack <PACK>] \
    [--repetitions <N>] [--scale <F>] [--seed-workers <N>] \
    [--with-baselines] [--label <LABEL>]
```

| Flag | Meaning |
|---|---|
| `--base-url <URL>` | The system's base URL, up to and including the openEHR REST base. Required |
| `--out <OUT>` | Output directory for the result document and its summary. Required |
| `--auth <MODE>` | How the client presents itself: `none`, `basic` or `bearer`. Default `none` |
| `--user <USER>` | The user `--auth basic` presents |
| `--pack <PACK>` | The embedded pack to drive: `smoke` or `community-vitals`. Default `smoke` |
| `--repetitions <N>` | How many times to repeat the measured phases. Default `3` |
| `--scale <F>` | Multiply the pack's EHR count by this factor, for a shorter run. Default `1.0` |
| `--seed-workers <N>` | Override the worker count every seed phase declares. Omit to run the pack's own value |
| `--with-baselines` | Also measure the pinned reference CDRs on this host, and record the relative index |
| `--label <LABEL>` | A label for this run, which names its column in a comparison |

A bench run needs no artifact root, no IXIT and no party statement. The pack is
compiled into the binary with a sha256 pin on every fixture, and the pins are
recorded in the result.

### The embedded packs

`smoke` proves the engine: one blood-pressure template, a small EHR corpus, and
one mixed open-loop phase over the read, write and query surface.

`community-vitals` reproduces the openEHR community's vital-signs benchmark
harness ([thread 17224](https://discourse.openehr.org/t/17224)) and measures the
same work a second way. Its write phase creates 100 EHRs and commits the same
Vital signs composition 1,000 times into each with `Prefer: return=identifier`,
on one worker, reporting bulk-load throughput plus the whole-loop
milliseconds-per-composition average the thread quotes. Its read phase then runs
twice over that population. `read_walk` is the sequential walk the harness
performs, seven GETs against every committed composition: the latest version,
the same at an instant, the `VERSIONED_COMPOSITION`, its latest version, its
version at that instant, one version by id, and the revision history. It reports
the whole-loop microseconds-per-request average. `read_open_loop` offers the same
seven reads as an arrival schedule pinned at 200/s for 60s after a 15s warmup,
which is where the coordinated-omission-free percentiles come from. The pinned
rate is part of the pack version.

Every number in the record carries the discipline that produced it: a
`closed-loop` whole-loop average and an `open-loop` percentile answer different
questions and are never read against one another. The two fixtures are embedded
byte-identically, the operational template from the vendored CKM export for
template id `Vital signs` and the composition from the attachment on post 8 of
that thread, both pinned by sha256 and verified at load.

`--scale` shrinks the EHR count for a quick run and changes nothing else.
Anything but `1.0`, or a `--seed-workers` override, takes the run off the pack's
pinned configuration; the record says so in its `scale` block, the summary says
so in prose, and `bench-compare` names it in the header.

Credentials never ride the command line: `--auth basic` reads its password from
`VEREDICTUM_BENCH_PASSWORD` and `--auth bearer` reads its token from
`VEREDICTUM_BENCH_TOKEN`.

Before anything is measured, a preflight reads the template list, uploads the
pack's template, then creates one scratch EHR, commits a composition into it and
reads that composition back. A failure at any of those refuses the run and names
the exchange, so a half-measured document never exists.

The run then seeds its corpus once and repeats the measured phases. Measured
phases are open-loop: arrivals fire at their planned instants whatever the
system is doing, and every latency is measured from the planned instant, so a
stall shows up in every arrival queued behind it.

### Same-machine baselines

`--with-baselines` anchors the run. After the target's own measurement, the
instrument composes each pinned reference CDR on this host, drives the same
pack at the same seed for the same number of repetitions against it, and tears
the stack down with its volumes so the next baseline starts from an empty
database. The record then carries a baseline block per reference, each a full
per-operation summary exactly like the target's, beside the digest-pinned
images, the upstream deployment recipe the topology follows, and the container
ceilings both stacks ran under. Every image is pinned by digest rather than by
tag, so two submitters measure the same bytes.

The flag needs the `docker` CLI. On a host where it does not answer the run is
refused before the target is touched, with the missing binary named; a run
without the flag needs no container runtime at all.

### The relative index

An absolute millisecond describes the system and the machine together, so two
records taken on different hosts cannot be read against one another. The
relative index can. For every phase, operation and metric it is the target's
cross-repetition median divided by the baseline's, both measured on the same
host in the same session, so the machine cancels. On a latency metric a value
below `1.0` means the target answered faster than the baseline and above `1.0`
slower; on throughput the sense inverts, because there a larger number is the
faster system. Each ratio is serialized with the two medians it came from.

Where no ratio can be formed the record says so and why: an operation only one
side measured, a phase only one side ran, or a baseline median of zero. A gap
is recorded rather than omitted, because a missing row in a comparison reads as
agreement.

### Submittability

A record is submittable when it carries at least three repetitions and at least
one same-machine baseline. A record that misses either stays valid for local
exploration and names the requirements it misses, in the `submittable_unmet`
list, in the rendered summary and in every `bench-compare` column header. The
environment fingerprint prints on the summary header and in every comparison
column, so no number is read without the machine it came from.

A bench result is a benchmark record for comparative speed. It is not a
conformance record, not a certificate, and not a performance-class rating; a
bench result may motivate a class run, never substitute for one.

## bench-compare

Align two or more committed bench results into one table.

```bash
veredictum bench-compare --result <FILE> --result <FILE> [--result <FILE>] \
    --out <OUT>
```

| Flag | Meaning |
|---|---|
| `--result <FILE>` | A committed bench result. Repeat the flag once per file; at least two are needed. Required |
| `--out <OUT>` | Output directory for the rendered comparison. Required |

One column per file, one row per phase, operation and metric. Each cell carries
the cross-repetition median with the inter-quartile range beside it, so a reader
sees the spread as well as the number.

Every column header carries the machine the run was generated on, and a column
that is not submittable names the requirements it misses rather than printing
one bare `false`. Where the columns carry a relative index, it gets its own
table below the header, at `p50` and `p99` per baseline: that is the part which
survives a change of host.

A mismatch is stated above the table, never under it: columns that ran different
pack versions, columns generated from different hosts, columns that ran at
different scale factors or off the pack's pinned configuration, and columns whose
runs are not submittable are all named in the header, and the command exits `1`
when any of them applies. Columns from different hosts where at least one
carries no relative index are named too, because nothing in that table is
comparable across them. Each row also names the discipline its numbers came
from.

## stress-compare

Render the cross-SUT stress overlay from two committed stress reports.

```bash
veredictum stress-compare --left <LEFT> --left-label <LABEL> \
    --right <RIGHT> --right-label <LABEL> --out <OUT>
```

| Flag | Meaning |
|---|---|
| `--left <LEFT>` | The primary SUT's committed `stress.json`. Required |
| `--left-label <LABEL>` | The primary SUT's display label. Required |
| `--right <RIGHT>` | The comparison SUT's committed `stress.json`. Required |
| `--right-label <LABEL>` | The comparison SUT's display label. Required |
| `--out <OUT>` | Where to write the overlay SVG. Required |

Deterministic, and both directions on equal footing: the two curves are drawn by
the same code from the same kind of file, so neither side gets a rendering
advantage.

## perf-assets

Render the published performance SVG assets from a committed `results.json`.

```bash
veredictum perf-assets --root <ROOT> --results <RESULTS> --out <OUT> \
    [--summary <PATH>] [--stress <STRESS>]
```

| Flag | Meaning |
|---|---|
| `--root <ROOT>` | The artifact root, for the class-ladder floors. Required |
| `--results <RESULTS>` | The committed `results.json` carrying the measurement records. Required |
| `--out <OUT>` | Output directory for the SVG files. Required |
| `--summary <PATH>` | Also write the generated Markdown summary, the class ladder plus the measured detail, to this path |
| `--stress <STRESS>` | A committed `stress.json` to render the latency-throughput curve from, when one exists |

## conformance-assets

Render the capability heat grid and the per-chapter outcome bars from committed
party artifacts.

```bash
veredictum conformance-assets --root <ROOT> --results <RESULTS> \
    --verdicts <VERDICTS> --out <OUT> [--suffix <SUFFIX>]
```

| Flag | Meaning |
|---|---|
| `--root <ROOT>` | The artifact root, for the capability matrix. Required |
| `--results <RESULTS>` | The committed `results.json`. Required |
| `--verdicts <VERDICTS>` | The committed `verdicts.json`. Required |
| `--out <OUT>` | Output directory for the SVG files. Required |
| `--suffix <SUFFIX>` | A suffix appended to the SVG file stems, so a comparison SUT's copies sit beside the primary set. Default empty |

Both renderers are deterministic over files already committed, so a build job
can regenerate them and diff the result. A hand-drawn number in a published
chart is a build failure rather than a review comment.

## emit-schemas

Write the published JSON-Schema set.

```bash
veredictum emit-schemas --out <OUT>
```

| Flag | Meaning |
|---|---|
| `--out <OUT>` | Output directory, created if missing. Required |

Byte-deterministic. The schemas in the repository's `schemas/` directory are
this command's output, drift-tested against it, which is how the published
format and the code that reads it stay one thing. Author against these if you
are writing your own catalogue or your own harness.
