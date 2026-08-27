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

![The console landing in light mode](img/landing-light.png)
![The console landing in dark mode](img/landing-dark.png)

## The catalogue explorer

Chapters first, each with the number of cases it carries. The search and page
state live in the URL, so a view is shareable and survives a refresh.

![The chapter list in light mode](img/catalogue-light.png)
![The chapter list in dark mode](img/catalogue-dark.png)

One chapter lists its cases: the identifier, the kind, and the test purpose.
One behaviour per case, so a red row names one defect.

![A chapter's case listing in light mode](img/chapter-light.png)
![A chapter's case listing in dark mode](img/chapter-dark.png)

## One case in full

The case detail carries the description, the specification citations the
expectation stands on, the operation bindings that realize it on the wire, and
the corpus fixtures it uses. The citation list is the point: an expectation is
refuted by a better reading of the cited text, and by nothing else.

![A case detail in light mode](img/case-light.png)
![A case detail in dark mode](img/case-dark.png)
