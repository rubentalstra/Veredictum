# Authoring the catalogue

<!-- toc -->

The catalogue is data, not code. You can read it, diff it, and add to it without
touching the runner, and a harness written in another language can execute the
same files. This chapter is an overview of its shape and the rules an author
works under.

The full grammar is the published JSON Schema set, in
[`schemas/`](https://github.com/rubentalstra/Veredictum/tree/main/schemas). Those
files are emitted by `veredictum emit-schemas` and drift-tested against it, so
the published format and the code that reads it are one thing. Author against
them; this page will not repeat every field.

## The artifact families

Everything lives under one artifact root, which is `artifacts/` in the
repository.

| Directory | What it holds |
|---|---|
| `schedule/<chapter>/` | Case cores, one file per case. The abstract test suite |
| `bindings/<its>/` | One file per Service-Model operation, saying how that operation reaches the wire on a given interface specification |
| `vocab/` | The closed vocabularies: the capability matrix, the enumerated wire surface, the outcome kinds, the selector grammar, and the journey catalogue the measured workload decomposes through |
| `corpus/` | Payload fixtures and their adjudicated verdicts, the scale-class corpora, and the breadth packs vendored from upstream libraries |
| `registers/` | The ambiguity register |

Chapters under `schedule/` follow the openEHR service components: `ehr`,
`composition`, `contribution`, `directory`, `query`, the three `definition_*`
chapters, `demographic`, `admin`, `messaging`, `security`, `smart`,
`simplified_formats`, `system` and `content`. A separate `performance` family
holds the measured-workload journey definitions, which are not case cores and do
not carry capabilities.

## A case core

One file, one behaviour. The fields that shape every case:

- **`id`** is a global identifier and is never reused. A retired case keeps its
  identifier with `status: retired`.
- **`test_purpose`** is the one narrow conformance requirement the case exists
  to check, in prose. If you cannot write it as one requirement, the case is
  really two cases.
- **`spec_refs`** are the citations, by component, document and section. They are
  resolved against the vendored specification tree by `validate`, so a citation
  that does not resolve is a finding rather than a comment.
- **`capabilities`** are the capability names whose verdict this case bears. Keep
  the list minimal: a failure marks every capability listed. Capabilities the
  case merely touches go in `exercises`, which is informative and bears no
  verdict.
- **`requires`** is typed prerequisite state, not prose. An empty server, a
  provisioned template, an EHR with no commits, a folder tree. Each provisioned
  object mints a named handle the flow can reference.
- **`flow`** is the ordered steps of a functional case: which operation, with
  what arguments, what outcome kind is expected, and what to capture from the
  response for a later step to use.
- **`parameters`** is the data-set dimension. A test is one case run against one
  data set, so a value matrix in a case produces one test per row, with
  preconditions re-established around each row by default.
- **`postconditions`** are typed assertions evaluated after the flow.

Two more fields carry the honesty:

- **`applies`** and **`guards`** state when a case is applicable at all, by
  specification version range or by a cited run condition. A failed guard
  produces not-applicable with its citation, never a silent skip.
- **`ambiguities`** lists the register entries the case is subject to.

What a case core deliberately does *not* contain: any status code, header name,
or media type. Those live in the binding.

## Operation bindings

A binding is the wire layer for one Service-Model operation on one interface
specification. It maps each outcome kind the case can expect to the wire result
that realizes it, and it says what to capture from the response.

The mapping is per operation, because the same kind lands differently in
different places. A validation failure is one status code on a composition
operation and a different one on EHR creation, and the released specification is
what decides which. A kind a binding cannot map is a validation finding.

There are 249 bindings today, and `validate` checks that every operation a case
calls has one.

## The closed vocabularies

Three of the vocabularies are closed enumerations, and that is what makes the
catalogue machine-checkable.

- **Outcome kinds.** A case says `created`, `not_found`,
  `precondition_failed`, `validation_failed` and so on. There are 26 of them,
  extensible only by a schedule release, and a case that speaks anything else
  fails validation. Codes never appear in a case.
- **Selectors.** How an assertion addresses part of a response body or a header,
  including the ignore-sets that let a case compare a document while ignoring
  the fields a server legitimately assigns.
- **The capability matrix.** The capability, family and profile-tier structure
  the verdict machinery computes over. This is the openEHR Platform Profiles
  book's capability tables as data.

The wire surface under `vocab/` is the fourth machine-readable list, and it is
the input to the coverage gate: the operations, status-code branches, header
rules, negotiation variants and error families the released sources define. A
behaviour in that list with no covering case and no cited exception fails the
gate.

## The corpus, and why invalid fixtures stay

A corpus entry carries its own adjudicated verdict: valid, or invalid with the
defect it carries and the specification reference that makes it invalid. Every
invalid shape has a valid twin and is authored as raw bytes, for the reasons
[the conformance method](methodology.md) sets out. Removing an invalid fixture
narrows the claim without changing any visible count, which is why it does not
happen.

Vendored breadth packs carry the same discipline. A pack is exercised in full,
with any skip adjudicated and recorded, so a pack never sits in the tree
implying coverage it does not have.

## The rules an author works under

1. **Cite the released specification.** An expectation without a resolving
   citation is a finding. The openEHR Conformance component's test schedule is a
   guide to which behaviours are worth covering, and it is not authority for what
   the correct answer is.
2. **One behaviour per case.** If a failure of your case could mean two
   different defects, split it.
3. **Never encode a status code in a case core.** That is the binding's job, and
   keeping the split intact is what lets the behaviour outlive the wire
   specification.
4. **Add cases; never remove one to go green.** A removal that makes a run pass
   is a narrowed claim, not a fix.
5. **Never adjust an expectation to match an observed response.** Change it only
   with a new citation that says the old reading was wrong.
6. **Record a specification silence, do not resolve it.** It goes in the
   ambiguity register with a typed disposition and an upstream report, and a
   private resolution makes a harness non-conformant.
7. **Keep the invalid twin.** See above.

## Checking your work

```bash
veredictum validate --root artifacts --specs specs/openehr
```

Zero findings is the only passing result, and the gates behind that line are in
[the command reference](commands.md#validate).

Run it before you drive anything against a server. A catalogue defect found by a
server run costs a great deal more to diagnose than the same defect found by
`validate` in a second.
