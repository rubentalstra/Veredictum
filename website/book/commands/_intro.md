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
