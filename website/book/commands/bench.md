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
