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
    [--sign-key <KEY>]
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
