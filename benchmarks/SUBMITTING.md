<!--
SPDX-FileCopyrightText: Veredictum contributors
SPDX-License-Identifier: Apache-2.0
-->

# Submitting a benchmark result

The public board at <https://veredictum.eu/benchmarks.html> is rendered from the
records committed under `submissions/`. There is no upload form and no account.
You run the benchmark, you open a pull request that adds the record it wrote,
CI validates the record before anybody looks at the numbers, and the merge is
the acceptance.

**A bench number is not a conformance verdict.** The board reports comparative
speed. It is not a conformance record, not a certificate, and not a
performance-class rating. A bench result may motivate a class run; it never
substitutes for one.

## 1. Run the benchmark

The board's reference pack is `community-vitals`. It reproduces the openEHR
community's vital-signs harness and then measures the same population a second
way, open-loop, so a stall shows up in the percentiles instead of quietly
reducing the request count.

```bash
# The credential is read from the environment. It never rides argv.
export VEREDICTUM_BENCH_PASSWORD=…

veredictum bench \
  --base-url https://cdr.example/openehr/v1 \
  --auth basic --user <user> \
  --pack community-vitals \
  --repetitions 3 \
  --with-baselines \
  --out ./bench \
  --label "Your CDR 1.2.3"
```

What each flag is doing, and why the gate insists on it:

- `--repetitions 3` is the floor. One repetition measures a moment rather than a
  system, and every figure the board prints is a median across repetitions.
- `--with-baselines` composes both pinned reference CDRs on **your** machine,
  EHRbase and FerroEHR, from image digests, under identical container ceilings,
  and drives the same pack at the same seed against each. The record then
  carries one relative index per reference, which is the only kind of number
  that means anything across machines. Two references rather than one, so a row
  is not a verdict about a single product. This needs a working `docker` CLI
  and it roughly triples the run time.
- `--label` names the system on the board. Give the product and its version.

Leave `--scale` and `--seed-workers` alone. Either one takes the run off the
pack's pinned configuration, the record says so, and the board marks the row.

Run it on an idle machine, and do nothing else on that machine while it runs.
The load generator, your deployment and both reference deployments share the
host, so a compile or a container build running beside them lands in every
number. Nothing in the record can detect that, which is why it is your job. A
run whose inter-quartile range is a large fraction of its median is usually
this.

The command writes `bench-result-<label>.json` into `--out`. That file is the
submission. Do not edit it: every check below reads it as the engine wrote it.

## 2. Name and place the record

```text
benchmarks/submissions/<system>/<YYYY-MM-DD>-<host>.json
```

- `<system>` is a lowercase directory naming the CDR, for example `ferroehr`
  or `ehrbase`. Reuse the existing directory when one is already there.
- `<YYYY-MM-DD>` is the calendar date the run started, which the record itself
  carries in `started_at`.
- `<host>` is the first eight hexadecimal characters this command prints. It
  digests the record's own environment block, so it distinguishes two runs the
  same person takes on two machines on one day, and the gate recomputes it from
  the record, so a copied name fails:

```bash
jq -cS '.environment' bench-result-your-cdr-1-2-3.json | shasum -a 256 | cut -c1-8
```

## 3. Open the pull request

One record per pull request. Say in the body what the deployment was: the
hardware, the container limits, the database, and anything about the
configuration a reader would need to make sense of the numbers. The record
carries the machine that offered the load, which is not always the machine that
served the requests.

## What CI checks before a human reads it

`scripts/checks/bench-submission.sh` is the whole gate, and it fails the pull
request on any of these:

| Check | What fails it |
|---|---|
| Schema | the file does not validate against `schemas/bench-result.schema.json`, or does not parse as a bench result |
| Pack pins | the pack id, the pack version, the seed or the fixture digests do not match a pack this release embeds |
| Repetitions | fewer than three |
| Baselines | no same-machine baseline block, or a baseline with no relative index derived from it |
| Submittability | the `submittable` flag disagrees with the record's own numbers |
| Fingerprint | no environment block, or no core count in it |
| Failed arrivals | any operation in any repetition, on the target or on either baseline, where every recorded arrival failed |
| File name | the date is not an ISO 8601 calendar date, or the host prefix does not digest from the record's own environment block |
| Append-only | the pull request modifies, deletes or renames a record that is already merged |
| Board freshness | `website/landing/benchmarks.html` no longer matches the committed records |

The board page is generated, so regenerate it in the same pull request:

```bash
bash scripts/render/bench-board.sh
```

Run the whole gate locally before pushing:

```bash
bash scripts/checks/bench-submission.sh
```

## The append-only rule

A merged record is a published claim with a signed commit and a date behind it.
Records are added, never edited and never removed. A record that turns out to be
wrong is corrected by submitting a new one and saying in that pull request what
the earlier one got wrong. The gate enforces this mechanically: any Modified,
Deleted or Renamed path under `submissions/` fails the check.

## Tiers

Every row on the board carries a tier badge saying how much of it anyone here
has verified.

- **self-reported.** The submitter ran the benchmark, the record passed every
  check above, and nobody here re-ran it. This is the tier every submission
  starts at, and today it is the only one in use. Read a self-reported row as a
  claim its author put their name to in a public git history.
- **reproduced.** A maintainer re-ran the same pack against the same deployment
  and got a consistent record. The submission channel does not change: the
  attestation machinery is being designed, and it will upgrade tiers on records
  that already sit in this tree.

A tier says who stood behind the measurement. It says nothing about
conformance, which is a separate instrument with a separate record.

## Records under `submissions/examples/`

That sub-tree holds records that demonstrate the submission pipe rather than
claim a place on the board. They are held to every check above except the
submittability requirement, and the board does not rank them.
