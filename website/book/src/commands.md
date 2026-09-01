# Command reference

<!-- toc -->

Every code block below is `veredictum <command> --help` from the binary itself:
this page is rendered by `scripts/render/commands-md.sh` out of clap's own
output, so a flag cannot land in the binary and go missing here. The sections
follow the order `veredictum --help` lists them in, and the prose under each
one is written by hand and lives in `website/book/commands/`.

Three commands make the conformance record (`validate`, `run`, `verdicts`),
three measure (`perf`, `stress`, `bench`), `replay` re-judges a recorded run
out of its own transcript, `evidence` carves the exchanges behind a red run's
rows out of that transcript, `verify-record` checks a sealed bundle, and the
rest render or explore.

## emit-schemas

```text
Write the published JSON-Schema set (byte-deterministic)

Usage: veredictum emit-schemas --out <OUT>

Options:
      --out <OUT>  Output directory (created if missing)
  -h, --help       Print help
```

Byte-deterministic. The schemas in the repository's `schemas/` directory are
this command's output, drift-tested against it, which is how the published
format and the code that reads it stay one thing. Author against these if you
are writing your own catalogue or your own harness.

## run

```text
Execute the catalogue against a live SUT (from its ixit topology) and emit results.json + the run report

Usage: veredictum run [OPTIONS] --root <ROOT> --ixit <IXIT> --out <OUT>

Options:
      --root <ROOT>
          The artifact root

      --ixit <IXIT>
          The ixit topology file (JSON)

      --out <OUT>
          Output directory for results.json + the run summary

      --sut-name <SUT_NAME>
          SUT display name
          
          [default: ferroehr]

      --sut-version <SUT_VERSION>
          SUT version label
          
          [default: dev]

      --filter <FILTER>
          Only run cases whose id contains this substring

      --statement <STATEMENT>
          The party statement (ICS) — the list ISO/IEC 9646 test selection selects from. Supplied, an option-gated case whose arm the ICS does not declare is recorded N/A at drive time; absent, NO arm of a mutually exclusive branch is selected, so every such case and every extension route is recorded N/A with its citation and the run says so once

      --sign-key <SIGN_KEY>
          An armored OpenPGP secret key — when supplied, the emitted documents are sealed with `record-manifest.json` and its detached signature, which `verify-record` and `gpg --verify` both check

      --sign-passphrase <SIGN_PASSPHRASE>
          The passphrase unlocking `--sign-key`. Supply it through the environment: a passphrase on the command line is visible to every process on the host
          
          [env: VEREDICTUM_SIGN_PASSPHRASE]

      --record-exchanges
          Persist the wire exchanges beside results.json as transcript.json.
          
          OFF by default. The artifact can carry real patient data — a SUT's response body is recorded verbatim — so it is operator-controlled output, never a log: store it where the record itself is stored. The `authorization` request header's value is withheld; everything else lands exactly as it went out and came back.

      --progress
          Print one machine-parseable line per processed case: `progress: <k>/<n> <case-id>` after `progress: 0/<n>` — a stable grammar a driver may parse. Off by default, so existing output is byte-identical without it

  -h, --help
          Print help (see a summary with '-h')
```

Drives every applicable case and records the exchange. Exits `1` if any case
failed or errored.

## validate

```text
Validate one artifact tree through every machine gate

Usage: veredictum validate [OPTIONS] --root <ROOT>

Options:
      --root <ROOT>
          The artifact root (schedule/, bindings/, vocab/, corpus/, registers/)

      --specs <SPECS>
          The vendored openEHR spec tree; enables SM-operation and spec-ref resolution

      --write-report
          Also refresh `<ROOT>/coverage-report.md` from `--specs`.
          
          OFF by default: `validate` is a check verb, and a check that mutates the working tree is a trap for read-only and fenced invocations. The pipeline scripts that publish the report pass this explicitly.

      --statement <STATEMENT>
          A submitted declaration (`statement.json`) to hold to the static conformance review, with the `ixit.json` beside it.
          
          ISO/IEC 9646-7 assigns an ICS proforma's support and supported-values columns to the supplier of the implementation, so no declaration is committed here and none is swept from the tree.

  -h, --help
          Print help (see a summary with '-h')
```

Every machine check over the catalogue: identifier uniqueness, citation
resolution, binding completeness, coverage of the enumerated wire surface, and
the per-capability case-count floors. It prints one line per finding and a
summary line, and exits `1` if the finding count is not zero.

Supplying `--statement` adds the static conformance review of ISO/IEC 9646-1
and -7 over that one declaration: a claimed capability the catalogue holds no
verdict-bearing case for, a `Signing` claim the ixit beside it declares no
posture for, and a served-extension family the catalogue's wire surface does
not carry.

`--write-report` is off by default on purpose. A check verb that rewrites a file
on every run is a trap for read-only invocations, so the pipelines that publish
the coverage report ask for it explicitly. The report lands at
`<ROOT>/coverage-report.md`, beside the artifact families it measures.

## perf

```text
Execute the performance schedule's open-loop measured run(s) against a live SUT and merge the §8.10 measurement records into an existing results.json (conformance-by-measurement)

Usage: veredictum perf [OPTIONS] --root <ROOT> --ixit <IXIT> --results <RESULTS> --class <CLASS>

Options:
      --root <ROOT>                  The artifact root
      --ixit <IXIT>                  The ixit topology file (JSON) — its environment block is mandatory for a measured run
      --results <RESULTS>            The results.json to merge the measurement records into (written by a prior `run`)
      --class <CLASS>                Select the performance case(s) of this class (POC | S | L | R)
      --seed-workers <SEED_WORKERS>  Parallel seeding workers [default: 16]
      --hours <HOURS>                The sustained-window ladder: hours to hold the offered load — 1 (default, the case's normative window) | 2 | 4 | 6 | 8 | 12. A longer window is a STRICTER demonstration and persists like any measured run; nothing shorter than the case exists [default: 1]
  -h, --help                         Print help
```

Conformance by measurement. Latency is measured from the planned arrival instant
under open-loop offered load, so a stall shows up as latency instead of
disappearing into a slowed-down request rate. A longer `--hours` window is a
stricter demonstration and persists like any measured run; nothing shorter than
the case exists.

The `environment` block is mandatory rather than optional because a latency
number is a claim about a deployment, and a claim with no deployment described
cannot be checked or reproduced.

## stress

```text
Run the step-load STRESS instrument: geometric load steps to the maximum sustainable throughput — where the system breaks (exploration only; never a conformance record, and class-free by design)

Usage: veredictum stress [OPTIONS] --root <ROOT> --ixit <IXIT> --out <OUT>

Options:
      --root <ROOT>                  The artifact root
      --ixit <IXIT>                  The ixit topology file (JSON) — its environment block is mandatory (a throughput number without the deployment described is meaningless)
      --out <OUT>                    Where to write the stress report (stress.json)
      --corpus-class <CORPUS_CLASS>  The class-scale corpus the stress runs on (POC | S | L | R — the standardized corpus selector): data volume + workload mix only; no class floor enters the stress report or chart [default: POC]
      --seed-workers <SEED_WORKERS>  Parallel seeding workers [default: 16]
      --step-secs <STEP_SECS>        Each load step's recorded hold, seconds (short + intense by design) [default: 120]
      --bisections <BISECTIONS>      Post-breach bisection refinements [default: 3]
      --max-rate <MAX_RATE>          The climb cap (arrivals/s) [default: 4096]
  -h, --help                         Print help
```

Exploration only, and class-free by design: no class floor enters the stress
report or its chart. A `stress.json` is never a conformance record, and quoting
one as if it were is a misuse of the tool.

## bench

```text
Run the universal SPEED benchmark: an embedded pack against any reachable CDR over its base URL, seeded once and measured N times open-loop (comparative speed only; never a conformance record, a certificate, or a performance-class rating)

Usage: veredictum bench [OPTIONS] --base-url <BASE_URL> --out <OUT>

Options:
      --base-url <BASE_URL>
          The system's base URL, up to and including the openEHR REST base (for example `https://cdr.example/openehr/v1`)

      --auth <AUTH>
          How the client presents itself: `none`, `basic` or `bearer`.
          
          The secret never rides argv: `basic` reads `VEREDICTUM_BENCH_PASSWORD` and `bearer` reads `VEREDICTUM_BENCH_TOKEN`.
          
          [default: none]

      --user <USER>
          The user `--auth basic` presents

      --pack <PACK>
          The embedded pack to drive
          
          [default: smoke]

      --posture <POSTURE>
          The posture profile to declare, out of the set the pack defines. Omit to take the pack's first, which is always `minimal`.
          
          The run's canaries check the declaration against the running system before and after the measured window, and a contradiction refuses the run.

      --repetitions <REPETITIONS>
          How many times to repeat the measured phases. A result with fewer than three is recorded as not submittable, and names that as one of its unmet requirements
          
          [default: 3]

      --scale <SCALE>
          Multiply the pack's EHR count by this factor, for a shorter run. Anything but `1.0` takes the run off the pack's pinned configuration, and the record says so
          
          [default: 1]

      --seed-workers <SEED_WORKERS>
          Override the worker count every seed phase declares. Omit to run the pack's own value, which is what its reference figures describe

      --with-baselines
          After the target's run, compose each pinned reference CDR on this host from its own digest-pinned images, drive the same pack at the same seed against it, and record the relative index. Needs the docker CLI; a record without a baseline is not submittable

      --out <OUT>
          Output directory for the result document and its summary

      --label <LABEL>
          A label for this run, which names its column in a comparison and distinguishes its file name in a shared output directory

  -h, --help
          Print help (see a summary with '-h')
```

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

`aql-mix` measures query speed over that same population, from the same two
pinned fixtures, so a query figure and a read figure describe the same corpus.
Its seed phase creates 50 EHRs and commits the composition 20 times into each,
on a pool of 8 workers. The pack version pins that population, and it is sized
for query shapes: large enough that a query has to choose an access path, small
enough to load before a measured window opens. The measured phase is open-loop
at 24 arrivals a second for 60s after a 15s warmup, over six query classes at
equal share, so each class is offered at 4 arrivals a second and every class
returns the same number of samples.

| Class | What it probes |
|---|---|
| `adhoc_query_point_lookup` | The indexed-read floor: one composition addressed by its own uid inside one EHR, the cheapest query a server can answer |
| `adhoc_query_ehr_scan` | The loaded-database shape: every composition in one EHR projected by uid, so the cost follows how much that EHR holds |
| `adhoc_query_filtered` | The value index: a systolic magnitude threshold over the observation leaves of one EHR |
| `adhoc_query_population` | The cross-EHR planner: the same threshold with no EHR scope and a `fetch` bound, so the server picks an access path over the whole population |
| `adhoc_query_aggregate` | The columnar shape: one `COUNT` over the population that threshold matches, which returns a single row and reads every value behind it |
| `adhoc_query_ordered_page` | Sorting and pagination: an `ORDER BY` over composition start time read through a moving `fetch` window |

Each class posts its own AQL statement to `/query/aql`, accepts only `200`, and
counts every other answer in its own error class, so a server that refuses one
shape leaves the other five classes' percentiles alone. The systolic threshold,
the page offset, and the EHR or composition each arrival addresses all draw
from the run's seeded streams, so no arrival repeats the previous one's result
set and the whole draw is reproducible from the seed the record discloses. Why
each class exists travels with the pack definition, so a rendered view explains
a column without knowing anything about the pack's internals.

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

### Posture profiles

A CDR running without an audit trail and unsigned versions is a different
system from the same CDR with both switched on, and a speed number that does
not say which one it measured cannot be compared with anything. Every pack
therefore defines named posture profiles, and a run declares exactly one.

`minimal` is the bare spec-conformant surface, and every pack defines it:
no audit trail, unsigned versions, commits validated against the operational
template, uncompressed responses, one tenant. Validation sits at `template`
rather than at nothing because the specification puts it there: ITS-REST
`specifications/responses/422.yaml` defines the commit refusal as the case
where the template "is not validating the supplied resource". A server that
accepts anything is below the floor rather than lightly configured.
`community-vitals` also defines `clinical-default`, which is the same surface
with an audit trail written.

The record's `posture` block carries the profile, its summary, and one line per
disclosed item: the audit sink, the version-signing scheme, the
commit-validation depth, the authentication mode, TLS, response compression and
tenancy. Each item is a closed vocabulary, so an unknown token is refused rather
than read as a default.

### Posture canaries

A declaration is a promise, so each item is also probed black-box and labelled
`verified` or `declared-only`, with the exchange behind the label recorded
beside it.

| Item | How it is checked |
|---|---|
| `version_signing` | Versions committed by the run's OWN seed traffic are read back and their `signature` inspected. Sampling the measured population means a scheme switched on around a probe never reaches it. The openPGP armor header separates `pgp` from `digest` |
| `commit_validation` | The pack's pinned invalid twin (that pack's own composition with the mandatory `COMPOSITION.composer` removed) is committed inside the run window, and the answer read |
| `authn` | One read with no `Authorization` header at all, which is the only way to see whether the declared mode is enforced |
| `compression` | One read stating `Accept-Encoding` explicitly, over a client that does not decompress, so `Content-Encoding` survives to be read |
| `tls` | The recorded base URL's own scheme |
| `audit`, `tenancy` | Declared-only. Released ITS-REST surfaces no read resource for either, so the record carries the claim and says it is one |

The canaries run before the measured window and again after it. A reading that
contradicts the declaration refuses the run, naming the item, the declared
value, the observed one and the exchange. So does a pair of brackets that
disagree with each other, because a posture that moved mid-run leaves the
numbers straddling two systems. Neither is recorded as a footnote beside a
published figure.

Baselines run under the profile the target declared and carry their own
verified block, so both sides of a ratio disclose what was switched on behind
them. `bench-compare` states a posture disagreement between columns in the
header, above the numbers.

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

A record is submittable when it carries at least three repetitions, at least one
same-machine baseline, and no operation that lost more of its arrivals than the
pack's ceiling allows. That ceiling is part of the versioned pack definition and
the record discloses it as `pack.max_failed_share`; every embedded pack pins
`0.01`. The check reads every repetition, phase and operation, on the target and
on every baseline block, because an index divides by a baseline median and a
divisor taken from arrivals that failed describes the failure. A record that
misses any requirement stays valid for local exploration and names the ones it
misses, in the `submittable_unmet` list, in the rendered summary and in every
`bench-compare` column header.

The rendered summary carries a failed-arrival table: one row per side,
repetition and phase, with the phase's own share and the worst operation inside
it, followed by a sentence per reading above the ceiling naming where it went.
The environment fingerprint prints on the summary header and in every comparison
column, so no number is read without the machine it came from.

A bench result is a benchmark record for comparative speed. It is not a
conformance record, not a certificate, and not a performance-class rating; a
bench result may motivate a class run, never substitute for one.

## bench-compare

```text
Align two or more committed bench results into one table, one column per file, with every pack or host mismatch stated in the header

Usage: veredictum bench-compare --result <RESULTS> --out <OUT>

Options:
      --result <RESULTS>  A committed bench result. Repeat the flag once per file; at least two are needed
      --out <OUT>         Output directory for the rendered comparison
  -h, --help              Print help
```

One column per file, one row per phase, operation and metric. Each cell carries
the cross-repetition median with the inter-quartile range beside it, so a reader
sees the spread as well as the number.

Every column header carries the machine the run was generated on, the worst
failed-arrival share the run recorded beside the ceiling its pack pins, and a
column that is not submittable names the requirements it misses rather than
printing one bare `false`. Where the columns carry a relative index, it gets its
own table below the header, at `p50` and `p99` per baseline: that is the part
which survives a change of host.

A mismatch is stated above the table, never under it: columns that ran different
pack versions, columns generated from different hosts, columns that ran at
different scale factors or off the pack's pinned configuration, and columns whose
runs are not submittable are all named in the header, and the command exits `1`
when any of them applies. Columns from different hosts where at least one
carries no relative index are named too, because nothing in that table is
comparable across them. Each row also names the discipline its numbers came
from.

## bench-packs

```text
Write the embedded benchmark pack manifest: every pack's phases with their load discipline, its operation mix with what each entry probes, its posture profiles, its fixture pins with their provenance, its seed, and the submission requirements (byte-deterministic)

Usage: veredictum bench-packs --out <OUT>

Options:
      --out <OUT>  Output directory (created if missing)
  -h, --help       Print help
```

A pack is versioned data compiled into the binary, so the binary is the only
honest source for a description of one. This command writes that description:
per pack the id, the version, the seed every arrival stream draws from, the
failed-arrival ceiling a record is judged against, each phase with its load
discipline and its counts, each measured phase's operation
mix with the share and the probe rationale of every entry, each posture profile
the pack defines with what it declares item by item, and each embedded fixture
with its sha256 pin, its size and where the bytes came from. The document also
carries the boundary statement, the methodology, how a relative index is
derived, what the seed and the posture canaries govern, and the requirements a
record meets before it may be ranked.

Emission is byte-deterministic and every collection is ordered, so regenerating
the file and diffing it is a build gate. The public page at
[veredictum.eu/benchmark-methodology.html](https://veredictum.eu/benchmark-methodology.html)
is generated from the committed copy of this document, and CI refuses a pack
change that leaves either of them stale.

## aql-probe

```text
Run the AQL optimization probe: fire the measurement machinery's AQL vocabulary against a live, freshly seeded SUT, record wire percentiles, and attribute the DB-side cost per probe via `pg_stat_statements` (exploration evidence for the optimization loop — never a conformance record)

Usage: veredictum aql-probe [OPTIONS] --root <ROOT> --ixit <IXIT> --out <OUT>

Options:
      --root <ROOT>                  The artifact root
      --ixit <IXIT>                  The ixit topology file (JSON) — the `containers` block enables DB-side attribution and maintenance settling
      --out <OUT>                    Where to write the probe report (aql-probe.json)
      --corpus-class <CORPUS_CLASS>  The class-scale corpus the probes run against (POC | S | L | R) [default: POC]
      --seed-workers <SEED_WORKERS>  Parallel seeding workers [default: 16]
      --requests <REQUESTS>          Requests fired per probe [default: 20]
  -h, --help                         Print help
```

Fires the measurement machinery's AQL vocabulary, records wire percentiles per
probe, and attributes the database-side cost through `pg_stat_statements`. This
is evidence for someone optimizing a server, and it is never a conformance
record.

## stress-compare

```text
Render the cross-SUT stress overlay (both systems' latency-throughput curves on one canvas) FROM two committed stress reports — deterministic, both directions on equal footing

Usage: veredictum stress-compare --left <LEFT> --left-label <LEFT_LABEL> --right <RIGHT> --right-label <RIGHT_LABEL> --out <OUT>

Options:
      --left <LEFT>                The primary SUT's committed stress.json
      --left-label <LEFT_LABEL>    The primary SUT's display label
      --right <RIGHT>              The comparison SUT's committed stress.json
      --right-label <RIGHT_LABEL>  The comparison SUT's display label
      --out <OUT>                  Where to write the overlay SVG
  -h, --help                       Print help
```

Deterministic, and both directions on equal footing: the two curves are drawn by
the same code from the same kind of file, so neither side gets a rendering
advantage.

## perf-assets

```text
Render the published performance SVG assets FROM a committed results.json (deterministic; CI regenerates and diffs — hand-drawn numbers are a build failure)

Usage: veredictum perf-assets [OPTIONS] --root <ROOT> --results <RESULTS> --out <OUT>

Options:
      --root <ROOT>        The artifact root (for the class-ladder floors)
      --results <RESULTS>  The committed results.json carrying the measurement records
      --out <OUT>          Output directory for the SVG files
      --summary <SUMMARY>  Also write the generated Markdown summary (class ladder + measured detail) to this path — the book's build-time include
      --stress <STRESS>    A committed stress report (stress.json) to render the latency-throughput curve from, when one exists
  -h, --help               Print help
```

## conformance-assets

```text
Render the conformance visuals (the capability heat grid + the per-chapter outcome bars) deterministically FROM the committed party artifacts — the perf-assets pattern for functional conformance

Usage: veredictum conformance-assets [OPTIONS] --root <ROOT> --results <RESULTS> --verdicts <VERDICTS> --out <OUT>

Options:
      --root <ROOT>          The artifact root (for the capability matrix)
      --results <RESULTS>    The committed results.json
      --verdicts <VERDICTS>  The committed verdicts.json
      --out <OUT>            Output directory for the SVG files
      --suffix <SUFFIX>      A suffix appended to the SVG file stems (`-ehrbase` for the comparison SUT's copies) [default: ""]
  -h, --help                 Print help
```

Both renderers are deterministic over files already committed, so a build job
can regenerate them and diff the result. A hand-drawn number in a published
chart is a build failure rather than a review comment.

## verdicts

```text
Compute the verdicts from a statement + results against an artifact tree (the pure pipeline) and write the rendered submission documents

Usage: veredictum verdicts [OPTIONS] --statement <STATEMENT> --results <RESULTS> --root <ROOT> --out <OUT>

Options:
      --statement <STATEMENT>
          The party statement (`statement.json`)
      --results <RESULTS>
          The party results (`results.json`)
      --root <ROOT>
          The artifact root (schedule/, vocab/, registers/)
      --out <OUT>
          Output directory for the rendered documents + verdicts.json
      --sign-key <SIGN_KEY>
          An armored OpenPGP secret key — when supplied, the rendered documents are sealed with `record-manifest.json` and its detached signature, which `verify-record` and `gpg --verify` both check
      --sign-passphrase <SIGN_PASSPHRASE>
          The passphrase unlocking `--sign-key`. Supply it through the environment: a passphrase on the command line is visible to every process on the host [env: VEREDICTUM_SIGN_PASSPHRASE]
  -h, --help
          Print help
```

The pure step. It reaches no network and reads nothing but its inputs, which is
what makes a published verdict re-derivable by anyone who has the same four
files.

## replay

```text
Re-judge a recorded run from its transcript, answering every composed request out of the recording instead of a server, and optionally refuse a results document the recorded exchanges do not support

Usage: veredictum replay [OPTIONS] --root <ROOT> --ixit <IXIT> --transcript <TRANSCRIPT>

Options:
      --root <ROOT>              The artifact root (schedule/, bindings/, corpus/, vocab/)
      --ixit <IXIT>              The ixit topology the recorded run was driven under
      --transcript <TRANSCRIPT>  The transcript of that run (`transcript.json`)
      --statement <STATEMENT>    The party statement ISO/IEC 9646 test selection re-applies: it decides which option arm, extension route, claimed capability and release floor the re-judgement selects. Omitting it re-derives a whole-catalogue sweep stamped `selection_basis: statement_blind`, which `--against` refuses to hold against a record an ICS selected
      --filter <FILTER>          Re-judge only cases whose id contains this substring
      --out <OUT>                Where the re-judged `results.json` is written
      --against <AGAINST>        The submitted `results.json` the re-judgement is held against: any row whose status or row counts differ fails the command
      --progress                 Print `progress: <k>/<n> <case>` lines while re-judging
  -h, --help                     Print help
```

Only the transport changes. The catalogue is driven again through the same
request composition, the same response classification and the same assertion
evaluators the live run used, with the recorded response standing in for the
server's. A case whose recording runs out, or whose replay composes a request
the recording does not carry, records a transport failure: a verdict is never
reproduced over evidence nobody has.

With `--against`, every row is compared on its status and its two row counts,
and any disagreement exits `1` naming the case. The reason text is not
compared, because a replay reaches a recording rather than a server and
identical judgements can carry different words.

Omitting `--statement` re-derives a sweep of the whole catalogue. The replay
says so on stderr, in the words a live run uses, and stamps
`selection_basis: statement_blind` on the document it writes. With `--against`
the selection facts a `results.json` records are compared before any row is: a
record an ICS selected, re-judged blind, under a statement the record does not
name, or under one declaring different its-rest formats, exits `2` rather than
reporting agreement, because a re-derivation under another claim re-derives
another campaign. The statement is named by `statement_digest`, the leading 8
bytes of the SHA-256 over the declaration's own bytes, so the refusal prints
the recorded value and the applied one and a reader checks either with
`sha256sum statement.json | cut -c1-16`. A record written before
`selection_basis` or `statement_digest` existed identifies nothing about what
selected it, and the replay reports that instead of refusing.

What this establishes is that the judgement follows from the evidence. It does
not establish the evidence: a transcript is what the instrument says it sent
and received.

## evidence

```text
Export a finished run's recorded exchanges for a named set of cases: the triage input, carved out of the run's own transcript. No statement is read, and a bundle that would carry nothing is refused

Usage: veredictum evidence [OPTIONS] --transcript <TRANSCRIPT> --out <OUT> <--failing|--only <ONLY>|--filter <FILTER>>

Options:
      --transcript <TRANSCRIPT>  The run's `transcript.json` (written by `run --record-exchanges`)
      --results <RESULTS>        The run's `results.json`. Required by `--failing`, and otherwise optional: supplying it puts each exported case's outcome row beside its exchanges
      --failing                  Export the red rows the results record names — every `failed` and every `errored` case. The one-command triage input
      --only <ONLY>              Export this case, by id. Repeat the flag once per case
      --filter <FILTER>          Export cases whose id contains this substring
      --out <OUT>                Where the bundle is written
  -h, --help                     Print help
```

At least one of `--failing`, `--only` and `--filter` is required, and the three
union: a case is exported when any of them names it. The unfiltered document is
the transcript itself.

The red rows of a run become a triage input in one command:

```bash
veredictum evidence --transcript run/transcript.json \
    --results run/results.json --failing --out run/evidence.json
```

**No statement is read.** Sealing a record needs a claim; reading the exchanges
a run recorded does not, and a run that went red is exactly when they are
needed.

**An export that would carry nothing is refused**, exit `2`, with no file
written. A selection matching no recorded case names what was asked for and
what the transcript actually carries; a selection whose every case recorded no
exchange names those cases and says that recording is opt-in. A selection that
half-matched still exports, and the bundle's `without_exchanges` names every
case it could not carry, so a partial answer never reads as a complete one.

The `authorization` request header's value is withheld by the export itself,
whatever the transcript held. Response bodies are the wire's own bytes and can
carry real patient data, so the bundle is operator-controlled output like the
transcript it comes from.

## verify-record

```text
Verify a sealed bundle: recompute every digest its record manifest names, and check the detached signature over that manifest

Usage: veredictum verify-record --record <RECORD> --key <KEY>

Options:
      --record <RECORD>  The bundle directory holding the emitted documents, the record manifest and its detached signature
      --key <KEY>        The armored OpenPGP public key the signature is checked against
  -h, --help             Print help
```

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
