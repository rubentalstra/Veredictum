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
