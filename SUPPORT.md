# Getting help

Destinations, and they are not interchangeable. Picking the right one is the
difference between an answer and a thread nobody is paged for.

## Read first

- [`README.md`](README.md): what the instrument is and where the name comes
  from.
- [`CLAUDE.md`](CLAUDE.md): the working rules the product's trust story rests
  on, including the specification authority, the three-way attribution law, and
  the coverage mandate.
- [`.claude/rules/cnf-triage.md`](.claude/rules/cnf-triage.md): how a failing
  row is attributed, and what each attribution obliges.

The [command reference](https://veredictum.eu/docs/commands.html) documents
every subcommand. It is rendered from the binary's own `--help`, so
`veredictum <command> --help` from the build you have installed says the same
thing and is the authority if the two ever disagree.

## I have a question

[GitHub Discussions](https://github.com/rubentalstra/Veredictum/discussions) is
enabled, and its Q&A category is the right place for a question that is not yet
a defect or a request. If the answer turns out to be work, it becomes an issue
from there. A question you would rather file directly is
[an issue](https://github.com/rubentalstra/Veredictum/issues/new/choose) with
the `question` label.

There is no commercial support offering, no service-level agreement, and no paid
tier. Answers come when the maintainer is at a keyboard, and
[MAINTAINERS.md](MAINTAINERS.md) is honest about how many keyboards that is.

## My server failed a case and I think the case is wrong

This is the report this project most wants, and it has one requirement: **cite
the specification section that supports your reading.** Every expectation in the
catalogue carries the section it came from, so a disagreement is settled by
reading that section against yours, and by nothing else.

What a report needs:

- the case id and the version of the instrument that ran it;
- the recorded exchange: the request the runner sent and the response your server
  returned, verbatim;
- the openEHR section you read, by component, document, and heading, with the
  sentence you are relying on quoted.

What will not settle it: what another CDR does, what the CNF Robot suites do,
what a client library expects, or what the behaviour has always been. Those are
prior art. The released specification text is the reference.

If the reading holds, the catalogue is corrected with a new cited source and the
run is re-judged. If it does not, you have a defect in your server, and the case
stays. Either outcome is a good outcome for the instrument.

## I think the instrument itself misbehaved

Also an issue, with the `bug` label. The instrument is a first-class suspect on
every failing row, ahead of the server, and it has earned that: the first live
triage attributed 7 of 7 diagnosed defects to the runner and none to the server
under test. Reports of this class carry the run artifacts if you can attach
them.

## I found a vulnerability

**In Veredictum:** do not open a public issue. Follow
[SECURITY.md](SECURITY.md).

**In a CDR that Veredictum tested:** that belongs to the vendor of that CDR, on
their disclosure timeline, not here. [SECURITY.md § Two different things, two
different processes](SECURITY.md#two-different-things-two-different-processes)
has the reasoning.

## I want to change something

[CONTRIBUTING.md](CONTRIBUTING.md) is the practical guide: where the code
currently lives, the gates, and the hard rules.
[GOVERNANCE.md](GOVERNANCE.md) is how a decision gets made and how someone
becomes a maintainer, including someone from a competing implementation.

## What a verdict from this instrument is, and is not

Worth stating here because it is the question behind most support requests.

A Veredictum run produces a record of what a server did, judged by pure
functions against expectations that each cite a released openEHR specification
section. It is re-checkable: the artifacts are committed, and anyone can read
the citation and disagree.

It is **not** an openEHR Foundation certification, and this project has no
authority to issue one. It does not certify a deployment, only the software
build and the environment the run declares. It proves the behaviours its
catalogue covers and nothing beyond them, which is why coverage is published
alongside every result and why a gap is recorded rather than left silent.

## What you are entitled to

Nothing, and that is worth saying plainly. Veredictum is Apache-2.0 software
provided as-is, with no warranty. Read the [LICENSE](LICENSE), which says
exactly that in the language that binds. Everything above describes what the
project intends to do, and the intent is sincere; none of it is a contractual
commitment, and only the report windows in [SECURITY.md](SECURITY.md) are stated
as promises at all.
