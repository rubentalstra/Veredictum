# The web console

The console is the instrument with a browser in front of it. It ships as its
own container image, reads the catalogue and the vendored specification text
from paths you mount, and reaches the engine only through the published
`veredictum` crate: every number it shows is one the command line prints too.

Every screenshot below is captured by the console's own browser journeys
(`scripts/ui-e2e.sh` with `UI_E2E_DOCS_SHOTS=1`), in one 1440×900 browser
window, light and dark. They are refreshed in the pull request that changes the
interface, so what you see here is the interface that shipped.

## The landing

The four counts are the catalogue's own: case cores, operation bindings, party
statements, and validate findings. A findings count above zero means the
catalogue itself needs attention before any server is graded.

![The console landing in light mode](console/img/landing-light.png)
![The console landing in dark mode](console/img/landing-dark.png)

## The catalogue explorer

Chapters first, each with the number of cases it carries. The search and page
state live in the URL, so a view is shareable and survives a refresh.

![The chapter list in light mode](console/img/catalogue-light.png)
![The chapter list in dark mode](console/img/catalogue-dark.png)

One chapter lists its cases: the identifier, the kind, and the test purpose.
One behaviour per case, so a red row names one defect.

![A chapter's case listing in light mode](console/img/chapter-light.png)
![A chapter's case listing in dark mode](console/img/chapter-dark.png)

## One case in full

The case detail carries the description, the specification citations the
expectation stands on, the operation bindings that realize it on the wire, and
the corpus fixtures it uses. The citation list is the point: an expectation is
refuted by a better reading of the cited text, and by nothing else.

![A case detail in light mode](console/img/case-light.png)
![A case detail in dark mode](console/img/case-dark.png)

## The run wizard

Grading starts at Connect: the CDR base URL, the display identity, the
authentication choice from the ixit's own vocabulary, and a probe whose
answer renders verbatim before anything continues.

![The connect step in light mode](console/img/connect-light.png)
![The connect step in dark mode](console/img/connect-dark.png)

Scope takes the claim the run grades: the vendor's own statement.json (the
ICS, the document that says which profiles and capabilities are claimed)
pasted into the box, or a committed example loaded into it.

The claim can also be built on the screen. The tier row offers CORE,
STANDARD, OPTIONS and SEC-BASIC with the capabilities the capability matrix
puts in each tier and the number of catalogue cases those capabilities gate,
counted through the instrument's own matrix walk — the one the judgement
computes each profile verdict from. Composing writes that claim into the same
box, product identity taken from the Connect step, so the operator reads the
exact document the run will be graded against. Option branches stay
undeclared there: only the party running the server knows which branch it
realizes.

The document is
held to the published statement schema before anything is stored, and saving
answers with the claim overview — product, claimed tiers, capability count —
beside the selection preview, so the screen says what will run before
anything starts. An empty box is an honest no-claim run. The verdict later
certifies exactly the pasted claim against the recorded evidence.

Scope also decides whether the run keeps its wire. "Record the wire exchanges"
is off by default; ticked, the run writes `transcript.json` beside its
`results.json`, carrying every request and response it drove. The artifact
records a server's response bodies verbatim, so it can hold real patient data
from the deployment being graded: it lands in the operator's own output
directory, the sealed record covers it, and the `authorization` request
header's value is withheld.

![The scope step in light mode](console/img/scope-light.png)

The live screen renders the engine's own progress stream: the case counter,
the elapsed clock, a moving-median estimate labelled as such, and the tail of
the engine's output. When the run finishes the outcome links straight to the
record.

![A finished run on the live screen](console/img/live-light.png)

## The record: results and verdicts

Results reads the finished run's own record, red rows first. A row links to
its detail: the recorded reason beside the case's specification citations,
because a red row names a defect in exactly one of three suspects — the
server, the runner, or the catalogue — and the cited text is the reference.

The detail ends with the wire. A run driven with the recording box ticked
shows each exchange as its request and response panes, verbatim, which is
where a triage starts. A run driven without it says so in the same place.

![The results surface with a detail open](console/img/results-light.png)

Verdicts is the same pure function the command line runs, over the same
record: the profile matrix with its coverage bounds printed, and the rendered
documents byte-for-byte.

![The verdicts surface in light mode](console/img/verdicts-light.png)
![The verdicts surface in dark mode](console/img/verdicts-dark.png)

## The export: one signed record

A verdict nobody can check is an opinion. The export step on the verdicts
screen hands the run to the pinned instrument's own `verdicts --sign-key`,
which writes the rendered documents, a digest manifest over them, and a
detached OpenPGP signature over that manifest. The console adds no sealing of
its own; it supplies the key path and reads the result back.

![Preparing the export on the verdicts screen](console/img/verdicts-export-light.png)

Beside the sealed set the console renders three files a party publishes. Each
carries the record digest prefix, so the artwork names the bytes it certifies
rather than being a logo anyone could copy:

- **The seal card** is the brand's certificate master with its three slots
  filled: the product under test, the profile verdict, and the moment the
  verdict was spoken — read back from the signature's own creation time,
  because the record carries no wall clock.
- **The badge** is a compact SVG for a README, with the digest prefix in its
  title and the verify path in its source.
- **The report** is one self-contained HTML file of everything the results and
  verdicts screens show, with the full digest, the signer fingerprint and the
  signing time in its footer.

All three are pure functions of the record: the same bundle in produces the
same bytes out, which is what lets anyone re-render them and compare.

The seal card, the badge and the report sit deliberately outside the manifest.
The manifest covers what the instrument judged; these are renderings about
that judgement, and signing a rendering of a rendering would say nothing extra.

Two keys are configuration. `VEREDICTUM_SIGN_KEY` names the armored secret key
the bundle is sealed with, and `VEREDICTUM_VERIFY_KEY` names the public half.
The console asks for both because it will not print a signing time it has not
checked: after sealing, it verifies its own bundle before stating who signed
it and when. With neither mounted the section says so and offers no button.

## Verify: checking a record without trusting us

`/verify` is public. It needs no run, no server and no account: upload a
bundle, and the published library recomputes every digest the manifest names
and checks the detached signature over it.

![A bundle verifying clean](console/img/verify-light.png)
![The same page in dark mode](console/img/verify-dark.png)

The upload is a plain HTML form posting to a server route. There is no
JavaScript in it, so it works before the client bundle has loaded and works
with scripting switched off. Uploaded bundles are transient: they unpack into
a scratch directory, are checked, and are swept on a short timer.

A tampered file names itself. Change one byte of one document and the row for
that file reads `mismatched`, with the digest the bytes actually produce
beside the digest the manifest promised. Change the manifest instead and the
signature is rejected while every remaining digest still matches, which says
precisely which of the two claims failed.

Two things are permanent furniture on that page. The first is the honesty box,
which renders on every outcome including a clean one: a valid signature proves
integrity and origin since signing, and nothing else. It does not prove the
conditions the run executed under, that the system under test is what the
record says it is, or that the catalogue covered everything the specification
defines. The second is the command line that does the same job:

```text
veredictum verify-record --record <dir> --key <public-key>
```

The manifest's signature is an ordinary detached OpenPGP signature, so
`gpg --verify` accepts a bundle too. Nobody has to trust this console to check
this console.

## An address the console does not serve

A path outside the route tree answers `404` and renders it: the same chrome,
the path that missed named back to you, and two ways on. Nothing was run and
nothing was judged, which is what the page says.

![The 404 page in light mode](console/img/not-found-light.png)
![The 404 page in dark mode](console/img/not-found-dark.png)

## Two real servers, side by side

The same catalogue, the same wizard, the same judgement — driven against two
live CDRs pulled at their latest published images: FerroEHR's quickstart and
EHRbase's official pairing. The run behind each column is the EHR-service
case family. The point of the pairing is the comparison: one instrument, two
records, and every difference below traces to a case id and its citation.

| FerroEHR (latest) | EHRbase (latest) |
|---|---|
| ![FerroEHR results](console/img/results-ferroehr-light.png) | ![EHRbase results](console/img/results-ehrbase-light.png) |
| ![FerroEHR verdicts](console/img/verdicts-ferroehr-light.png) | ![EHRbase verdicts](console/img/verdicts-ehrbase-light.png) |

These captures come from the same E2E harness that gates the console
(`UI_E2E_REAL_SUTS=1 scripts/ui-e2e.sh`): the browser drives the real wizard
against the real servers, and the book shows what it photographed.
