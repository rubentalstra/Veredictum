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

Scope picks the claim the run grades: a party statement (the ICS, the
document that says which profiles and capabilities the vendor claims) and an
optional case-id filter for narrow runs. The verdict later certifies exactly
this claim against the recorded evidence.

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

![The results surface with a detail open](console/img/results-light.png)

Verdicts is the same pure function the command line runs, over the same
record: the profile matrix with its coverage bounds printed, and the rendered
documents byte-for-byte.

![The verdicts surface in light mode](console/img/verdicts-light.png)
![The verdicts surface in dark mode](console/img/verdicts-dark.png)

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
