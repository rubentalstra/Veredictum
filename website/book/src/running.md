# Running the instrument

<!-- toc -->

A conformance campaign is four steps. Each writes a file the next one reads, so
you can stop after any of them, inspect what was produced, and resume.

## 1. Check the catalogue

Before a server is involved, check the catalogue you are about to grade with:

```bash
veredictum validate --root artifacts --specs specs/openehr
```

Zero findings is the only passing result, and the command exits `1` when there
is even one. [The command reference](commands.md#validate) lists the gates it
runs.

Pass `--specs` every time. Without it the citation and Service-Model
resolution gates do not run, and the case count still prints, which looks like
a pass over a catalogue that was never fully checked.

## 2. Declare your deployment

The instrument needs to know where your server is and how to authenticate to
it. That declaration is the IXIT file, and copying an example is the fastest
way to a correct one:

```bash
cp -r party/ehrbase party/mine
```

The directory holds two files, and the split between them matters:

- **`ixit.json`** describes the deployment. Endpoints per instance, the
  authentication mode, and for a measured run an `environment` block naming the
  hardware and topology the numbers were produced on. **Credentials are named,
  never carried:** the file holds the *names* of the environment variables the
  instrument reads the user and password from, so no secret ever enters an
  artifact you might publish.
- **`statement.json`** is your declaration of claims. It names the product and
  version, the specification versions it targets, and the capabilities it claims
  to implement. The verdict machinery reads it as the thing being tested against
  the record.

A typical `ixit.json` declares three instances, because a full run needs to
speak to the server as three different callers: an ordinary clinical user, an
administrator, and no one at all. The unauthenticated instance is what lets the
security cases check that a route refuses an anonymous request.

## 3. Drive the catalogue

```bash
veredictum run --root artifacts --ixit party/mine/ixit.json --out out/ \
    --sut-name my-cdr --sut-version 1.2.3 --statement party/mine/statement.json
```

The command drives every applicable case against your endpoints and writes
`out/results.json`, a record of what was sent, what came back, and how it was
classified. It also writes `out/run-exceptions.json`, which lists the cases the
interpreter could not drive at all.

`results.json` is a record, not a judgement. That separation is the point: the
recorded exchange stays available for anyone to re-read, and the judgement is
computed from it in a separate step that touches no network.

Passing `--statement` here changes what runs. A case gated on an option your
statement does not declare is recorded as not-applicable at drive time rather
than driven, which is the test-selection discipline ISO/IEC 9646 describes. Omit
the flag and everything is driven, which is what you want when you are
exploring an unfamiliar server rather than grading a declared one.

`--filter` takes a substring matched against case identifiers, which is how you
re-drive one chapter while working on a fix. The resulting `results.json` holds
only the cases that ran, so a filtered run is a working tool and never the
record you submit.

The command exits `1` if any case failed or errored, so a shell script can gate
on it.

> [!NOTE]
> A functional run never re-measures. If a `results.json` already exists at the
> `--out` path, its measurement records are carried forward, so running the
> functional catalogue again after a measured run does not discard the
> performance evidence. A file that is present but unreadable stops the run
> instead, because carrying zero measurements past it would drop that evidence
> silently.

## 4. Compute the verdicts

```bash
veredictum verdicts --root artifacts --statement party/mine/statement.json \
    --results out/results.json --out out/
```

This step is a pure function of the statement, the recorded results, the
catalogue and the catalogue's capability matrix. Run it twice on the same files,
on any machine, and you get the same bytes out. It writes:

| File | What it is |
|---|---|
| `verdicts.json` | The machine-readable verdict set, per capability and per profile tier |
| `CONFORMANCE_REPORT.md` | The full record: every case, its outcome, and the citation behind the expectation |
| `CONFORMANCE_STATEMENT.md` | The rendered declaration of claims, with each claim marked against the evidence |
| `CONFORMANCE_CERTIFICATE.md` | The summary document, functional tiers plus any measured performance class |
| `badge.json` and siblings | Shields endpoint files, so a repository badge and the certificate beside it come from one rule |

Nothing in these documents is asserted by hand. A number that appears in them
was computed from the record in the same run that printed it.

## Measured performance, stress and speed

Those four steps cover functional conformance. Three more instruments produce
the other kinds of evidence.

- `veredictum perf` earns a volumetric class (POC, S, L or R) with an open-loop
  measured run and merges the measurement record into an existing
  `results.json`. The normative window is one hour; `--hours` extends it to 2,
  4, 6, 8 or 12, which is a stricter demonstration. Nothing shorter than the
  case exists.
- `veredictum stress` climbs geometric load steps to find where the deployment
  breaks and writes `stress.json`. It is exploration only and is never a
  conformance record, which is why it is class-free by design.
- `veredictum bench` measures comparative speed against any reachable CDR and
  writes a `bench-result` document. It needs no artifact root, no IXIT and no
  statement, and it is a speed record and never a conformance record. The
  [benchmark board](https://veredictum.eu/benchmarks.html) ranks the submitted
  ones.

`perf` and `stress` both need the `environment` block in your IXIT file filled
in, because a throughput number without the deployment described says nothing.
All three want an idle machine and a deployment whose resource limits match the
envelope you are claiming. A measured run on a laptop that is also running a
browser measures the browser.

## Where to look when something goes wrong

A red row is not presumptive evidence of a bug in your server. It is evidence
that the specification, the catalogue and the server do not all three agree, and
which one is wrong is a question that gets answered before anything is changed.
[The conformance method](methodology.md) sets out how, and what evidence an
attribution has to carry.

The [command reference](commands.md) lists every subcommand with its real flags.
