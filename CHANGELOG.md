# Changelog

All notable changes to Veredictum are recorded here. The format follows
[Keep a Changelog 1.1.0](https://keepachangelog.com/en/1.1.0/) and the
versioning follows [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

Every pull request with user-visible changes adds an entry under the Unreleased
heading below, in the same pull request. User-visible here means the CLI surface,
the published artifact schemas, verdict semantics, the container image, or
anything a party's published conformance record depends on. The heading is named
in prose rather than quoted verbatim on purpose: a release cut rewrites the first
literal occurrence of it, and quoting it here is what turned this paragraph into
a stray release heading at the v0.0.1-alpha.1 cut.

Releases before `0.1.0-alpha.1` carried the repository's identity and its
discipline; the instrument itself builds and runs from this repository from that
version on.

## [Unreleased]

### Added
- **A results document names the statement that selected it (#490).** New
  `statement_digest` member in `results.json`: the leading 8 bytes of the
  SHA-256 over the party statement's bytes as they sit on disk, 16 lowercase
  hex characters, the same shape `ixit_digest` already published. A reader
  holding the declaration a record was driven under recomputes it with
  `sha256sum statement.json | cut -c1-16`. `run` and `replay` both write it,
  and `schemas/results.schema.json` publishes it.

  What it closes: a record identified the claim it was selected under by
  `selection_basis` and the declared its-rest format list, so two different
  statements declaring the same formats were one value. `replay --against` now
  refuses a re-judgement handed a statement the record does not name, and the
  refusal prints both digests. The console tier's whole claim is that CI
  re-derives the verdicts from the recorded exchanges, and a re-derivation
  under somebody else's claim is not a re-derivation of that record.

  Absence is unknown, never a match. A campaign no statement selected writes no
  digest, which `selection_basis: statement_blind` already says out loud, and a
  document written before the member existed carries none either. A
  re-derivation of such a record is reported and recomputed rather than
  refused, so `scripts/checks/registry-rederive.sh` keeps re-deriving every
  record published before this release.
- **A red run exports the evidence behind its own rows (#463).** New subcommand
  `veredictum evidence`, which carves a named set of a finished run's recorded
  exchanges out of its `transcript.json` and writes them as one bundle. It reads
  no statement: sealing a record needs a claim, and reading the exchanges a run
  recorded does not, which is exactly the case when a run has gone red. One
  command turns the red rows into a triage input:

  ```bash
  veredictum evidence --transcript run/transcript.json \
      --results run/results.json --failing --out run/evidence.json
  ```

  `--failing` selects every `failed` and `errored` case the results record
  names; `--only <CASE>` (repeatable) and `--filter <SUBSTRING>` name a set
  directly, and the three union. Each exported case carries the outcome row the
  run recorded beside its requests and responses, so a reader holds one
  document rather than two.

  **An export that would carry nothing is refused**, exit `2`, with no file
  written. A selection matching no recorded case names what was asked for and
  what the transcript actually carries; a selection whose every case recorded
  nothing names those cases. A selection that half-matched still exports, and
  the bundle's `without_exchanges` names every case it could not carry, so a
  partial answer never reads as a complete one. This is what the first live
  triage's hand-written extraction got wrong: it produced 130,761 empty objects
  and zero scalar values, valid JSON of the right shape and size with nothing
  in it, and nothing said so.

  The `authorization` request header's value is withheld by the export itself,
  whatever the transcript held, pinned by a test over a run driven with a
  credential. Response bodies are the wire's own bytes and can carry real
  patient data, so the bundle is operator-controlled output like the transcript
  it comes from.

  The console offers the same document on a red run: the results screen's
  **Evidence for a triage** section downloads it from `/export/evidence.json`,
  and says which of the two reasons applies when it cannot — no row went red, or
  the run was driven without recording its wire.

  `schemas/evidence-bundle.schema.json` is the published shape, drift-guarded
  like every other emitted schema.

### Changed
- **The proforma stays, the answers leave (#465).** ISO/IEC 9646-7 splits an ICS
  in one place: every cell of the proforma belongs to its specifier except the
  support and supported-values columns, which belong to the supplier of the
  implementation, and ISO/IEC 17050-1 is titled a *supplier's* declaration of
  conformity. This repository was committing two filled-in declarations, one of
  them about EHRbase, a product nobody here speaks for. Both are gone. The
  proforma stays exactly where it was: `artifacts/vocab/capability_matrix.yaml`,
  43 rows with a specification citation each.

  What this changes for anyone driving the instrument. `validate` sweeps no
  `party/` directory any more and gains `--statement <FILE>`, which runs the
  static conformance review of ISO/IEC 9646-1 and -7 over one submitted
  declaration: a claimed capability with no verdict-bearing case, a `Signing`
  claim whose ixit declares no posture, a served-extension family the wire
  surface does not carry. Every one of those checks survives unchanged; only its
  input moved from a committed fixture to a supplied document. The summary line
  now counts capability rows where it counted party statements, and the
  console's instrument page does the same. `workload-coverage` and the
  hollow-battery half of `claim-completeness` are re-grounded on the matrix, so
  the catalogue side of both is enforced over all 43 rows whatever anybody
  claims — measured by experiment before anything was removed: emptying one
  capability's battery is still caught, by `capability-depth`, naming the row
  and the shortfall.

  The console stops offering a list of committed declarations to attach. The
  claim is the submitter's, so the paste box and the compose-from-tiers button
  are the only ways one enters, and the endpoint that read a statement from a
  client-supplied path is gone with the tree it read from. The published image
  no longer carries `/work/party` and `VEREDICTUM_PARTY` is retired.

  The `reproduced` tier's question is answered in `registry/RULES.md` rather
  than left open: without a supplier declaration there is no claim and so no
  conformance verdict, so a reproduction either cites a declaration the supplier
  published or publishes a survey labelled as an observation. Neither committed
  topology cites one, so both dropped their `statement`, and the lane refuses to
  bundle a reproduction rather than inventing a claim. The submission criteria
  an entry is scored against are unchanged, so the rules version is unchanged.

  One filled-in declaration survives as a named test fixture,
  `fixtures/declaration/`, for a product that does not exist. The SMART lane's
  committed test issuer moved beside it as `fixtures/smart-test-issuer/`,
  contents and warnings untouched.

### Fixed
- **A case declaring no capability is refused at authoring time (#500).** The
  two readers of a case's `capabilities` list disagreed about the empty one:
  the runner drove such a case against a server, and the verdict pipeline
  deselected it because no claim intersects an empty list. The row burned a
  server's time and bore no verdict. An empty list is now illegal: the
  published `case-core` schema carries `minItems: 1` on `capabilities`, so
  `validate` reports the file that declares one, and the runner excuses the
  case under a statement as the drive-time backstop. Every one of the 1146
  committed case cores already carries a non-empty list, so no catalogue
  artifact changed.
- **The console's documentation capture mode pins every fact a re-render moves
  (#480).** A capture pass over an unchanged console rewrote most of the
  committed screenshots, so the `ui-screenshot-guard` job could not tell a real
  visual change from a re-run. Two facts were reaching a browser unpinned: the
  connect probe's measured round trip, and the run directory named by the id a
  run mints, which the live screen prints inside the engine's output tail and
  the results path beside it. Both are stand-ins in capture mode now, beside
  the address the harness's fixture server bound, which the scope screen shows
  verbatim. What a run RECORDED is still never pinned: the case counter, the
  case driving, the engine's own words and the finished tally stay the run's
  own. The browser is pinned to one architecture as well — the image digest
  names a multi-architecture manifest list, so an arm64 workstation and an
  amd64 CI runner were rendering the same pages with two different Chromium
  builds.
- **A malformed call to a console endpoint answers 400 with a sentence, not
  500 with a serializer's phrasing (#484).** Every `#[server]` function is a
  publicly reachable HTTP endpoint, and a call whose arguments will not decode
  never reaches its handler: `server_fn` builds that response itself and
  documents the status it uses, 500, for every error it carries
  (<https://docs.rs/leptos/latest/leptos/server_fn/response/trait.Res.html>).
  A caller's mistake therefore read as the server breaking, and the body was
  `Args|missing field \`postures\``. A decoding failure is now 400, and the
  body names the argument and says that every argument is required because a
  value that is not being declared has its own spelling — an empty string, or
  the `Undeclared` member of its vocabulary — so an omitted argument can never
  read as a declared absence. No argument was made optional to achieve it.
- **A declaration that answers an option family with no arm is a finding, not a
  silent deselection (#462).** The arms of an `option_select` register branch
  are mutually exclusive, so a party declares exactly one. Nothing required the
  declaration to answer at all: a family with no arm declared removed every one
  of its rows from the run and satisfied the verdict review, so a vendor could
  pass a family by declaring nothing about it. The register now groups its arms
  into named FAMILIES (`options` is a mapping of family name to arms, and
  AMB-167's twenty arms are the ten independent choices they always were), and
  three places hold a declaration to answering each family the claim reaches
  with exactly one arm: `validate --statement` reports
  `option-family-selection`, the verdict pipeline's static review reports the
  same sentence per family instead of once per entry, and a run records each
  row of an unanswered family not-applicable naming the family, counts the fact
  at run level, and prints a run-level warning before it drives anything. A
  declaration naming several arms of one family, and an `option:` tag no
  register family declares, refuse the run outright: neither has an honest
  outcome. AMB-167's ICS-completeness caveat, which described the defect as a
  known limitation, is re-grounded on the fix.
- **The console's bundle is served under content-hashed names, and the hosted
  instance caches them for a year (#450).** `veredictum-console.<hash>.js` and
  its `.wasm` now carry the content hash cargo-leptos computed, so a browser
  holding one release's JavaScript cannot pair it with the next release's wasm:
  the old name is unreachable rather than mismatched, and the `LinkError` that
  left the page rendered and dead after the v0.1.4 deploy has no way to happen.
  The served markup reads those names from the build's own hash file, so it can
  only ever name files the build emitted. `Cache-Control: no-cache` stays the
  floor over the whole of `/pkg`, and hashed names are served
  `max-age=31536000, immutable` on top of it, which is what a name that never
  changes content is worth: the 2.4 MB wasm stops revalidating on every load.
- **A blind `replay` says so, and refuses to call itself a re-derivation of
  somebody else's claim (#471).** `veredictum replay` had no warn channel at
  all, so a run without `--statement` stamped `selection_basis:
  statement_blind` on the document it wrote and printed nothing, in the one
  command whose whole job is re-deriving a published record. The advisory now
  reaches `replay` in the same words `run` uses, because both commands render
  one `RunWarning`, and the stamped basis and the advisory are read off one
  derived selection posture so they cannot disagree. With `--against`, a
  record an ICS selected that is re-judged blind, or under a statement
  declaring different its-rest wire formats, exits `2` instead of reporting
  agreement; a record written before `selection_basis` existed identifies
  nothing about what selected it, and that is reported rather than refused.
  `--statement`'s help on `replay` now says what the flag decides.
- **A case needing an undeclared signing posture is excused before it writes
  anything to the server (#456).** Four `SIG-VERSION` cases asking for
  `signature: verifiable` were driven to completion — committing real
  compositions to the server under test — and only then reported unjudgeable,
  because the drive-time selection law had no arm for the `signing` posture
  while every sibling missing-ixit fact resolved not-applicable at selection
  time. Signing is conditional on deployment infrastructure (RM common
  `master06-change_control_package.adoc` §Digital Signature: "If public key or
  equivalent infrastructure is in place so that users are able to sign
  content, a digital signature can be created"), so the mode is an ixit
  declaration. An undeclared posture now records the case `not_applicable`
  with that citation before a single request is sent; a declared one drives
  and judges the case exactly as before.
- **The console composes the deployment postures it used to drop (#456).** The
  ixit it wrote carried three instances and nothing else, so a console-driven
  run judged 31 rows of the first live public run not-applicable for facts the
  operator was never asked for. The Scope step now collects the system
  identifier, the dump location, the version-signing posture (digest with its
  encoding and prefix, or an openPGP public key) and the openEHR generation
  set, and it states in the interface which postures it cannot supply — the
  SMART lane, the terminology topology, the exclusive-server flag, and any
  second principal or second deployment — with what each omission costs a run.
  An undeclared posture composes no key at all, so the engine's own citation
  is what the row carries. A declaration the run could not use, such as an
  openPGP mode with no key, is refused by name at the save rather than
  silently dropped.
- **The verification pack judges its bindings' header matchers (#473).** The
  pack player classified the status, bound captures and evaluated assertions,
  and never ran a single header matcher, so an entry could reproduce `passed`
  over a recording whose served headers violated its own binding. Turning them
  on failed six of the pack's ten tests, and every failure was real: three
  recorded `201` responses omitted the `Last-Modified` the `create_ehr`
  expectation declares, and no recording carried the request-side ask at all,
  which left every `negotiated` matcher unjudgeable while the evaluator answered
  "no failure" for an absent ask. A recorded request now carries the negotiated
  `accept`, and an entry declaring a `negotiated` matcher without one is refused
  by name rather than passed.
- **`guards:` stops promising a selection it never performed (#460).** 377 case
  cores carry a `guards:` line, 618 entries between them, and nothing in the run
  path has ever read the field. All 56 Admin API cases said "guarded until the
  Admin API stabilises", so a reader of the catalogue concluded those cases were
  held back while they drove as gating cases, and 34 of them went red on the
  first live run. Reading all 618 entries settled which way to fix it: they are
  provenance and scope prose, and every one of the 85 that named a
  machine-decidable condition named one the runner already decides from a typed
  field (`capabilities`, `option`, `requires`, `applies`, or a `${ixit:…}`
  read). The field is now what it already was in the other 533 entries: cited
  prose about the case, selecting nothing. Those 85 entries are rewritten to
  state their fact, a new `guard-condition` validate gate refuses "applies
  only", "not-applicable", "guarded until", "skip where" and their siblings, and
  the published `case-core` schema, `ARCHITECTURE.md`, the book's catalogue
  chapter and the console's case page all say so. No case changed selection. A
  catalogue that means to hold a case back does it with `status: draft`, which a
  run reports as its own exception and no verdict rests on.
- **The bulk-delete binding declares the 405 its own released source declares
  (#458).** `admin_ehr_delete_all.yaml` anticipates the branch in as many words,
  "may be disabled in production environments, in which case server may respond
  with `405 Method Not Allowed`", and its `responses` map carries a `'405'`. The
  binding omitted it on the premise that no case could reach the branch, which a
  live run against a deployment with the admin surface switched off refuted. The
  kind is declared, the wire-surface exception records why it stays unforceable
  (the trigger is a deployment setting the ixit does not model, not a request
  shape), and a statement claiming `AdminApi` over such a deployment now fails
  those cases while one that does not has them excused at selection.
- **A run with no party statement no longer publishes the losing arm of every
  mutually exclusive option pair as a failure (#455).** `--statement` is
  optional, and five arms of the drive-time selection law were gated on a
  statement being present, so a statement-blind run drove BOTH halves of each
  `option_select` register branch. The arms are mutually exclusive, so one half
  had to go red whatever the server did: the first live public run reported 46
  such rows, none of them the server's. Two of those arms now hold without a
  statement, because the missing declaration is what manufactures their
  failure. An `option:` case with no arm selected, and a case driving an
  extension route no released openEHR text governs, are each recorded
  `not_applicable` with a citation naming the fact that was missing, so the row
  reads as neither a pass nor a failure. The other three arms keep widening the
  sweep rather than excusing cases: a deployment claiming every capability at
  the latest release passes those, and `veredictum verdicts --statement`
  re-applies both filters at judgement time.
- **`results.json` records whether selection had a statement (#455).** The new
  `selection_basis` member reads `statement` or `statement_blind`, so a reader
  tells a party-scoped record from a whole-catalogue sweep from the document
  alone. `schemas/results.schema.json` carries it as an optional enum; a
  document written before the member existed reads as unknown, never as either
  basis. A blind `veredictum run` also prints one run-level advisory naming
  every selection fact it could not establish and how many cases each excused.
- **`equivalent` judges the XML and the plain-text form its own binding asked
  for (#457).** Three retrieval rows reported that the instrument could not
  judge a body it had itself negotiated: `I_DEFINITION_ADL14.get_opt` sends
  `Accept: application/xml` and `I_DEFINITION_ADL2.get_artefact` sends
  `Accept: text/plain`, so `get_opt-retrieve_single`,
  `get_opt-retrieve_specific_version` and `get_artefact-retrieve` could not pass
  on any server. The comparator now reads both forms whenever the corpus entry's
  declared `format` and the served media type agree on one, so those rows judge
  retrieval fidelity as the register's AMB-111 ruling pins it. XML equivalence is
  information-set equality: attribute order, the namespace prefix, whitespace
  inside a tag, `<a/>` against `<a></a>`, a CDATA section against the characters
  it stands for, and the XML declaration are not differences, while element
  order, expanded names, attribute values and character data are. Text
  equivalence tolerates the line-break spelling HTTP grants a text body and
  nothing else. A form the comparator cannot read, and any disagreement between
  the two sides, stays in the inconclusive channel with a refusal naming both
  sides; the other four assertion families keep the standing refusal, since each
  needs its own derivation.
- **A transcript replay judges `equivalent` and `signature` instead of refusing
  them (#469).** Three assertion families shared one classification saying a
  recorded exchange carries no ground for them, and for two of them that had
  stopped being true. A served document is recorded verbatim and its corpus
  fixture comes from the catalogue the replay is given, so an `equivalent`
  assertion naming a `${ds:…}` fixture now runs through the same form agreement
  and the same comparator the live driver uses. A `signature` assertion whose
  facts are `present`, `equals` or `distinct_from` over a literal or a `${ds:…}`
  comparand runs through the same evaluator, which the live driver now calls too
  rather than carrying its own copy. `version` stays refused, and the refusal
  says why per family instead of one sentence for all three: every fact it
  judges is read off a VERSION envelope the assertion fetches itself — the
  `ORIGINAL_VERSION` envelope for `change_type` and `lifecycle_state`, the
  `REVISION_HISTORY` for `count` — and it names its target through a `${…}`
  reference over row state, so the step's own recorded exchange decides neither.
  A `signature` asking `verifiable` stays refused for the same kind of reason:
  the signing posture is a deployment fact the party declares and no exchange
  carries it. The verification pack gains its first `equivalent` entry, so the
  family is now proven reproducible rather than merely classified.

## [0.1.4] - 2026-08-31

This release builds the hosted instrument and everything that judges what it
produces: console.veredictum.eu on a box this repository provisions, several
people driving the catalogue at once, a submission opened by the instrument's own
App identity, CI that re-derives a submitted record's verdicts before anything is
signed, and a signing key that exists only in a protected environment.

**No record has travelled that path yet.** Nothing has been driven at
console.veredictum.eu, submitted, re-derived, signed and verified offline as one
sequence. Every part of it is tested on its own and the parts have not been run
in series, so read this as the pipeline arriving rather than as a result. The
first record is what v0.1.5 exists to produce.

### Added
- **One image, carrying the data it judges against (#420).** The published
  image now holds the catalogue, the vendored specification oracle and the party
  declarations at `/work`, so `docker compose up` grades a server in an empty
  directory and the hosted instrument mounts nothing. It replaces a second image
  whose only content was those three directories copied over the release, and
  whose data came from whichever branch last built it — so the official
  instrument was pairing a released engine with an unreleased catalogue. An
  operator who wants their own trees bind-mounts over the baked paths, which
  still takes precedence. The image declares
  `eu.veredictum.image.carries-catalogue`, and the hosted deploy refuses an
  image that does not, so an older release cannot reach an instance that would
  find `/work` empty.
- **The registry signing key exists, and its public half is committed
  (#403).** `registry/keys/registry-signing.pub.asc` is what a reader checks a
  published console record against, with the instrument through
  `veredictum verify-record` or without it through `gpg --verify`. The primary
  key certifies and an ed25519 subkey signs, so a verified record names the
  subkey fingerprint. The secret half is in the `registry-signing` environment
  and nowhere else: it was generated in a throwaway keyring, never entered a
  personal one, and that environment carries required reviewers plus a branch
  policy naming `main` alone. Before it was used, the engine's own signing and
  verification code was run over this exact key material, because the committed
  test keypair is RSA and an EdDSA key would otherwise have reached production
  on a path nothing here had exercised.
- **A finished run submits itself to the registry (#391).** The console's
  `/run/submit` screen states what the run knows — the endpoint it drove, when
  it started, the catalogue revision, the engine version — and collects the
  disclosure the submission rules make mandatory, the conflict-of-interest
  sentence included. An empty mandatory value is refused by name before
  anything is opened. The instrument then writes the entry and the five record
  files through its own GitHub App identity: blob, then tree, then commit, then
  ref, so the commit GitHub signs is the one that lands, and a commit reported
  unverified pushes no branch at all. The submission arrives on
  `console-run/<run-id>` and carries no provenance block, because the
  re-derivation lane writes that after it has recomputed the judgement. The App
  identity is `VEREDICTUM_GITHUB_APP_ID`, `VEREDICTUM_GITHUB_APP_KEY`,
  `VEREDICTUM_GITHUB_INSTALLATION_ID` and `VEREDICTUM_REGISTRY_REPO`; any of
  them unset is a first-class state that explains what to configure and offers
  no button. No credential the run was driven under reaches the branch, pinned
  by a gate over a run driven with one.
- **A recorded run can be re-judged from its own transcript: `veredictum
  replay` (#392).** The transport is now the only seam between the driver and
  the wire, so a replay answers every composed request out of the recording and
  reaches its outcomes through the same request composition, response
  classification and assertion evaluators the live run used. With `--against`
  it holds a submitted `results.json` to what the recorded exchanges support and
  names every row that differs. A case whose recording runs out, or whose replay
  composes a request the recording does not carry, records a transport failure:
  a verdict is never reproduced over evidence nobody has.
- **The console tier's lane (#392).** `scripts/checks/registry-rederive.sh`
  re-derives a `console` submission's outcomes from its transcript and its
  verdicts from its outcomes; `.github/workflows/registry-console.yml` runs
  that gate, then seals the record from a protected environment and writes the
  provenance block the instrument is not allowed to write for itself. A record
  altered after the run fails the gate, which is pinned by a test.
- **A conformance entry may carry the `statement` it was judged against**, so a
  claim that lives nowhere else is still one anybody can recompute against. A
  `console` entry must carry it, with the transcript and the ixit.
- **Two people can drive the hosted console at once (#389).** The one job slot
  is a map keyed by the run id, so several runs execute side by side and every
  seam addresses a named run instead of asking whether one exists. Two drive at
  once, a start past that ceiling is accepted and QUEUED with its place and an
  estimated wait, one address gets one run in flight and a second start is
  answered with the run it already has, a run past thirty minutes of wall clock
  is ended by the console with its partial record discarded, and finished runs
  are evicted from memory oldest-first while their artifacts stay where the run
  wrote them. The connection draft is per submitter too, so two visitors
  composing a connection no longer overwrite each other. The caps are named in
  one place and are starting values to re-measure on the chosen host.
- **`VEREDICTUM_CLIENT_IP_HEADER` (#389).** Behind a proxy the peer address is
  the proxy, so the console reads a forwarded client address only from the
  header the operator names; unset, it uses the socket peer and reads no
  forwarded header at all.
- **`VEREDICTUM_POSTURE`, and the targets a public instance refuses (#390).**
  A hosted console drives whatever endpoint a visitor names, so it can be
  pointed at addresses only it can reach. Set the variable to `hosted` and the
  console refuses loopback, RFC 1918 private, link-local, unique-local,
  unspecified, multicast, RFC 6598 shared and broadcast targets in both address
  families, the IPv4-mapped IPv6 forms included, before any socket opens; the
  name is resolved first and every address it answers with is checked, because
  a hostname under the visitor's control resolving to a private address is the
  whole attack. The
  refusal names the address, the family and the RFC that defines it, and it
  reaches the visitor as a notification. Both seams that reach a
  visitor-named endpoint are covered: the reachability probe, and the run
  start before the engine is spawned. Unset or `local` refuses nothing, so an
  operator keeps driving a CDR at `localhost`; any other value refuses to
  start, because a public instance that read a typo as `local` would drive
  whatever a visitor named.
- **Per-address rate limits on the probe and the run start (#390).** They read
  the same submitter identity the concurrency caps use, and a refusal states
  when the visitor may try again.
- **The registry publishes a third kind of entry: `console` (#393).** A run
  performed at console.veredictum.eu, the official hosted instrument, against
  an endpoint the submitter named. Its verdicts are re-derived here from the
  transcript the submission carries, and the record is signed only after they
  match, with a key held in a protected CI environment that the instrument
  cannot reach. Every field of a `console` provenance block is written by that
  lane rather than by the instrument, so a performer cannot state its own
  provenance. `registry/RULES.md` states what the kind attests and what it
  cannot — it cannot attest the environment, because the submitter chose the
  endpoint — and the conformance board labels and orders the new rows.
- **The hosted posture travels inside the image (#423).** The published image
  now carries `deploy/hosted/docker-compose.yml` and `deploy/hosted/Caddyfile`
  at `/app/posture/`, and the one command the hosted box's deploy key may run
  extracts them from the image it just pulled and installs them. Before this,
  a committed change to either file reached the box only when somebody copied it
  there by hand. A posture change now arrives the way a catalogue change does,
  at a release, with the provenance of the artifact it travelled in, and the
  deploy key gains nothing: it still runs one script and writes no arbitrary
  file. An extraction that comes out empty stops the deploy, a candidate compose
  file `docker compose config` refuses to parse is never installed, each
  replaced file is kept as `.prev`, and a changed Caddyfile restarts the caddy
  service — a bind-mounted file's contents changing does not recreate the
  container that mounts it, so without that restart a proxy change served
  nothing.

### Changed
- **Every reader-facing page describes the hosted instrument as what it is
  (#395).** console.veredictum.eu is the official conformance instrument: a run
  performed there is an official run, and the record it produces is re-derived
  and signed here. The README, the landing page and the book say that, along
  with the one thing such a record cannot attest — the submitter chose the
  endpoint. Running the instrument yourself publishes your own claim, which the
  same pages now describe as a different question rather than a lesser one.
  `scripts/checks/hosted-instrument-language.sh` refuses the words "demo" and
  "sandbox" on every surface a reader meets, as whole words, so `demonstration`
  and the CSP directive name are untouched.
- **The hosted instrument runs on a box this repository provisions, deploys to
  and watches (#394, #412).** A conformance run outlives the request that started
  it, and #387 measured what an autoscaling request platform does to that:
  several instances answer one service, an idle one is stopped with the engine
  child inside it, and each has its own filesystem. So `deploy/hosted/` carries
  the whole posture as code — a `cloud-init` that brings a fresh box to serving
  state, the compose file with its healthcheck and memory limit, a Caddyfile for
  automatic TLS, and the image overlay CI builds and the box only pulls, because
  the box holds no checkout. `Dockerfile.vercel` and `vercel.json` are gone. The
  deploy goes through `rubentalstra/hetzner-deploy-action`, written for this,
  which waits for the container's own healthcheck and then requires the public
  URL to serve the expected engine version — a bare 200 is not proof, since the
  deployment being replaced answers 200 too. A scheduled watch asks every fifteen
  minutes whether the instrument is up and serving the right release, opening one
  issue and reusing it until it recovers. The deploy key and the host key are
  ENVIRONMENT secrets in an environment the deploy job names, so nothing else in
  this repository can reach them, and no Hetzner API token exists at all — the
  deploy talks to the host and nothing else, so a leaked deploy key cannot
  destroy the server.
- **A host states how many runs it can drive (#412).**
  `VEREDICTUM_MAX_CONCURRENT_RUNS` overrides the reasoned default, because #388
  called that number a starting value to re-derive by measuring on the chosen
  host — and the chosen host turned out to be a 2 GB box that drives one run
  rather than two. A value that is not a positive integer refuses to start: a
  cap is a safety property, and falling back to a larger default on a typo lets
  a box admit work it cannot hold, which the OOM killer then resolves halfway
  through somebody's run. Resizing the box is now an environment edit rather
  than a release.
- **The console sweeps its own run artifacts (#412).** A disposable filesystem
  discarded them every few hours; a box that does not restart would let them grow
  until the disk is gone. Directories older than `ARTIFACTS_KEPT` go hourly,
  never one a run in the map still names, and never anything outside the
  `console-job-<uuid>` shape — an operator's own files under the output mount are
  left alone. A swept run answers through the honest "this console knows nothing
  about that run" rather than a failure.
- **The registry entry format and the submission rules are both at 1.1.0
  (#393).** `schemas/registry-entry.schema.json` carries the third provenance
  branch, and an entry declares the versions it was accepted under as before.

### Fixed
- **A published registry entry is no longer re-scored against a later rules
  version (#397).** `registry/RULES.md` promises that rules change
  prospectively, and the gate contradicted it: an entry was refused unless both
  its entry format version and its rules version equalled the newest constant,
  so the next rules change would have turned every merged entry red. A release
  now declares the set of versions it can read (`1.0.0` and `1.1.0`, since the
  1.1.0 move only added the console provenance kind and changed no field's
  meaning), and an entry declaring any member of that set is accepted unedited.
  An entry naming a version outside the set is still refused, by the gate and
  by the published `registry-entry` schema, and the diagnostic names every
  version a submitter may declare instead. `RULES.md` states how a version
  leaves the readable set, because that is the one event that can invalidate a
  published entry.
- **The official instrument could not check a signature (#430).** `/verify` is
  where somebody who performed no run checks a published record, and on the
  hosted console it rendered its own unconfigured hint instead: the image
  carried no public key, so `VEREDICTUM_VERIFY_KEY` had nothing to point at.
  The image now bakes the registry signing public half at
  `/app/keys/registry-signing.pub.asc` and sets that variable to it, so a fresh
  instance and a local `docker compose up` both verify a published record with
  no operator action. The key is release data like the catalogue: the release
  that publishes a record ships the key that record is checked with. It lives
  under `/app` because an operator bind-mounts their own trees over the `/work`
  paths, and a mount there would shadow it. Naming another key still overrides
  the default.
- **The re-derivation gate skipped every real console submission (#408).** It
  chose what to do by reading `.provenance.tier`, and a submission carries no
  provenance block at all — that is the property the console tier rests on,
  since the instrument may not state its own provenance and the lane writes the
  block afterwards. So the gate read an empty tier, said "nothing to re-derive",
  and exited 0, and the signing step sealed a record whose verdicts were never
  recomputed. It now re-derives an entry that carries no provenance, reports how
  many entries it recomputed, and fails when a caller that required a
  re-derivation got none. Two defects it was hiding are fixed with it: the
  replay drove the whole catalogue instead of the rows the record claims, and an
  unclean judgement — a real result — aborted the gate.
- **A run in flight became unfollowable (#386).** A run's id was a counter that
  restarted with the console process, and `/run/live` carried no run identity at
  all, so the page could only ask whether this process held a job right now. On
  a deployment that serves one service from several instances, a later request
  reached an instance whose slot was empty and the page said "No run is in
  flight" about a run that was still executing. A run id is now a UUID, minted
  once, used as the run's job-directory name and carried by the URL as
  `/run/live/{run_id}`. The live screen has four honest answers: this process is
  driving the run, the run's own artifacts say this, this console knows nothing
  about that run, and no run was named. A reload mid-run rejoins the same run, a
  link to a finished run's id shows that run, and the screen prints the run's own
  address as a copyable permalink. Cancel names the run it cancels. The screen
  also states what the instance keeps: the artifacts live long enough to be
  judged, shown and submitted, and a redeploy ends the runs in flight.

## [0.1.3] - 2026-08-30

### Changed
- **The console speaks about the console (#382).** Every screen that printed a
  command line or pointed at a terminal now states what it did itself: the
  verify page's "check it without this page" box, the benchmark pages' three
  command boxes, the empty-state hint that told a reader to run `bench`, and
  the verdicts line about "the bodies the command line writes". Documents that
  leave the console keep their verification instructions, because their reader
  is holding a record rather than looking at a screen.
### Fixed
- **Every server function answered 404 in the published image (#381).** A
  server-function URL is `xxh64` over the crate's absolute manifest directory
  unless `SERVER_FN_OVERRIDE_KEY` says otherwise, and the release pipeline
  builds the two halves in different places: the server binary inside a job
  container, the WASM bundle on the host runner. The 0.1.2 image therefore
  registered one set of paths and requested another, so every call the
  hydrated console made fell through to the not-found page. A workspace
  `.cargo/config.toml` fixes the key for every build, and the image smoke now
  diffs the shipped binary's path set against the shipped bundle's and
  refuses any difference.

## [0.1.2] - 2026-08-30

### Added
- The hosted console is documented where readers look (#374): the README,
  the landing page and the book's entry pages name
  <https://console.veredictum.eu> as the no-install path, with its ephemeral
  lifecycle stated plainly.
- The two 409 delete-conflict `ETag` outcomes assert the `W/` weakness
  indicator behind `applies: { its_rest: ">=1.1.0" }`, the same sentence and
  gate as the thirteen 412 outcomes #360 ratcheted (#361).
- **The error-response surface the released ITS-REST actually pins gains its
  coverage (#340).** A new `present-with-body` header matcher carries the
  `Resources.md` §JSON Format MUST in its own shape — the header is asserted
  when the response carries a content body and carries no criterion when it
  does not — and the demographic create binding's 400 outcome now declares it
  for `Content-Type`, with the media type itself grounded on the OAS
  (`specifications/responses/400.yaml`, `content: application/json`) because
  the docs text names no media type for an error body. Register AMB-217 is
  untouched: the error body's members stay unpinned. The new
  `create_party-error_body_content_type` case and the
  `error-body-content-type` wire-surface element name the behaviour, so a red
  row localizes to it.
- **One header may declare several expectations.** The `outcomes.*.headers`
  value now accepts a sequence beside the single form, so two rules of
  different strength or different dating on the same header each keep their own
  ground. Every `412` outcome that expects the latest `version_uid` in `ETag`
  now also asserts the `W/` weakness indicator behind
  `applies: { its_rest: ">=1.1.0" }`, which ratchets that MUST from one
  binding to thirteen.
- **The archetype-root invariants reach every top-level class the release
  makes a root (#339).** Two refusal cases with their invalid corpus twins:
  an EHR_STATUS whose root `archetype_node_id` contradicts its
  `archetype_details.archetype_id` (RM `EHR_STATUS` is unconditionally an
  archetype root), and a directory FOLDER whose `archetype_node_id` is empty
  (`Archetype_node_id_valid` binds every LOCATABLE). Template-level
  validation of EHR_STATUS and FOLDER commits has no released wire binding —
  the only released 422/template sentence reaches COMPOSITION and the
  DEVELOPMENT demographic surface — so that boundary is register entry
  AMB-230, reported upstream (#355), never an invented expectation.
- **The hosted console (#348).** The released container image serves the
  public reading surface at `console.veredictum.eu` on Vercel: the catalogue,
  the party statements and the specification oracle, baked into the image
  from the checkout. The posture is view-only by construction, because the
  image ships no engine binary. Deploys ride the project's Deploy Hook only —
  a real release pings it after the image tags apply, a posture push and a
  manual dispatch redeploy the same image — and the verification polls the
  served `engine X.Y.Z` footer, never a bare 200.
- The dump/load authorization refusals drive without a declared
  `dump_location`: the refusal is a role or authentication decision taken
  before any path is consulted, so a literal placeholder location suffices
  and the boundary is testable on every party that declares the admin split
  (#286).
- **Signing-on posture profiles (#335).** Every bench pack defines
  `minimal-signed-digest` and `minimal-signed-pgp` beside `minimal`, so a CDR
  that signs versions out of the box benches without switching a shipped
  feature off; the canaries keep refusing any declaration the deployment
  contradicts. Pack versions move with the definition (`community-vitals`
  and `aql-mix` to 1.1.0, `smoke` to 1.2.0) and the generated pack manifest
  and methodology page follow.
- `bench` warns once when a credential rides a plain-`http` base URL that is
  not loopback; the run proceeds, because the operator names the transport
  and a local quickstart is legitimately `http://localhost` (#296).

### Fixed
- **The image carries the engine (#375).** Every published image so far held
  only `/usr/local/bin/veredictum-console`, and the console spawns the pinned
  `veredictum` binary to drive a run — so the compose quickstart's console
  served its screens and failed every run it started. Both build paths now
  ship the engine beside the console at `/usr/local/bin/veredictum`, the
  runtime names it in `VEREDICTUM_ENGINE`, and the release smoke asks the
  pulled image for `veredictum --version` and refuses anything but the
  released version. Proving that on the from-source path exposed a second
  break in the same build: `.dockerignore` excluded `assets/`, which the
  console compiles against twice over — its `public/` entries are symlinks
  into `assets/brand/`, and its export module reads the seal-card master
  through `include_str!` — so `--target runtime-from-source` had been failing
  to compile at all. `assets/brand/` is re-included. The hosted console at
  `console.veredictum.eu` gains the engine with it (owner ruling): a visitor
  can drive a real run against a CDR they control, writing into a `/work/out`
  the overlay creates for the console's own uid, and that output is gone at
  the next deploy.
- **The published image hydrates (#369).** The per-architecture ssr binary
  was compiled without `LEPTOS_OUTPUT_NAME` in the compiler's environment, so
  leptos's `option_env!` probe baked a `_bg` wasm URL into it while the site
  bundle ships the cargo-leptos-named `veredictum-console.wasm` — every
  published image to date served its pages SSR-only and 404'd its own client
  bundle on each load. The compile step now carries the name, and the image
  smoke fetches every `/pkg/` asset the served page references, so the class
  cannot ship green again.
- **One judgement per sealed record (#243).** Preparing the console's export
  ran the full judgement five times over the same run — once per fact the
  summary, the seal card, the badge and the HTML report each asked for — and
  reading the export section ran it twice more. One judgement of a
  1,147-outcome campaign costs about 2.7 seconds on an unloaded developer
  machine and far more on a busy one, so the repeats were most of the wait
  between the engine's manifest and the rendered files. The judgement now runs
  once and feeds all of them. What the bundle contains is unchanged: the same
  manifest, the same signature, the same three presentation files.
- **The book's console screenshots stop changing on every capture pass
  (#243).** A capture pass now serves the console in capture mode, where the
  run clock, the record digest and the signing time render as fixed stand-ins,
  so six images no longer churn with no interface change behind them. The mode
  changes what the screen displays and nothing that is written, sealed or
  signed, and it is off in every ordinary run.
- **An XML response body is refused as unjudgeable instead of read as an empty
  document (#285).** The driver collapsed every body it could not parse as JSON
  into a plain string, so an XML-negotiated read reached the `field`,
  `equivalent`, `instance_of`, `result_set` and `signature` families as a value
  with no members and each one reported the asserted fact absent — a failed row
  charged to a server that answered exactly as it was asked to. Canonical XML
  and canonical JSON are separate bound document forms (ITS-REST
  `specifications/docs/overview/Resources.md` §Data representation) and this
  runner parses the JSON binding only, so those families now take the
  inconclusive channel with the served media type named, and the `version`
  family refuses the same way when its `ORIGINAL_VERSION` envelope read comes
  back unparsed. Nothing narrows on the JSON path, `xml_root` and `returns`
  keep grading the served text, and a `uid_pattern` judged off the resolved
  identity still gates.
- **A `lifecycle_state` assert reads an `IMPORTED_VERSION` through the version
  it wraps (#322).** The judge read the property off the top level of the
  served envelope, which an imported version cannot carry: released ITS-JSON
  `components/RM/Release-1.1.0/Common/IMPORTED_VERSION.json` requires
  `contribution`, `commit_audit` and `item` under
  `additionalProperties: false`, and RM common
  `UML/classes/imported_version.adoc` §Functions effects `lifecycle_state ()`
  from `_item.lifecycle_state_`. A conformant server was charged a failed row
  for answering exactly as the release binds it. The row now resolves the fact
  through `item`, a wrong term still fails, and `change_type` keeps reading the
  wrapper's own `commit_audit`, which is a real property of the class.
- **The measured path stops substituting silently for a defect (#293).** A
  workload stage naming an operation the vocabulary does not carry is now a
  named finding at the schedulable filter, so a typo'd `op` fails loud
  instead of dropping its journey from the mix and quietly shrinking the
  offered load. The corpus stride forms its product in `u128`, where the
  widest operand pair needs 96 bits, so the overflow fallback that stopped
  striding past ~6.9e9 arrivals is gone rather than proven unreachable. The
  capture store's cleanup path recovers a poisoned shard the way its read
  path already did, so a worker that panics mid-window cannot leak every
  instance hashing to that shard.
- **The `latest-version-uid` ETag comparison names the object it is about
  (#235).** The driver kept one `last_version_uid` slot for the whole row, so
  a row that wrote object A and then provoked an error on object B graded B's
  entity tag against A's uid. The slots are now per versioned object, keyed by
  the `object_id` of the committed `OBJECT_VERSION_ID` (BASE `base_types`
  master05 §Syntaxes), and the object a step addresses is read from the path
  parameters the request was built from, falling back to the `If-Match` the
  request sent for the `directory` and `ehr_status` routes that carry no uid
  segment. A cross-object regression test pins it.
- **`validate` recomputes every recipe digest the corpus manifest pins
  (#235).** The new `recipe-digest` gate hashes each `generated_by` and
  `recipes` contract and fails on a mismatch or on a digest algorithm it
  cannot compute, so a generated set's provenance claim is worth the pin
  behind it. The gate immediately caught the `scale_ladder` pin, which had
  never matched its committed contract, and the `bp_series` contract's "all
  other fields fixed" claim while the generator varies the COMPOSITION name.
- `CONT-DV_URI-validate_list` drives the list constraint it declares: the case
  rode the pattern-baking template, so both rows passed under the pattern by
  coincidence; a script-generated `cnf.tpl.dv_uri_list` twin (the
  `dv_ehr_uri_list` split, applied one family over) now bakes the case's own
  `C_STRING.list`, and the generator's key exemption for the deliberately
  invalid twins is stated instead of implicit (#267).
- **The unmatched-container-member posture becomes a declared option
  (#283).** No released AM sentence decides whether a data member matching no
  constraint node is invalid (reported upstream as #307), so the two
  type-narrowing refusal rows move to option-tagged sibling cases under
  register AMB-229 and the matched halves stay gating. The structural
  synthesizer stops emitting VCACA-illegal templates on `CLUSTER.items`:
  `any` restates the RM's own 1..* (`C_MULTIPLE_ATTRIBUTE.cardinality` is
  1..1, so one is always stated; same-as-RM is VCACA-legal) and `opt`
  refuses loudly, with the four unauthorable rows dropped under that
  citation; the ADL1.4/AOM2 divergence on open occurrences bounds is
  reported upstream as #308.
- **A role-boundary premise is a declaration, never a presumption (#281).**
  The IXIT instance block gains an `administrative` posture (SM delegates
  access control, so nothing on the wire discloses a principal's roles), the
  five role-boundary refusal cases require `sut` declared non-administrative,
  and selection records them not-applicable with register AMB-228's citation
  wherever the posture is undeclared or opposite. The first reproduction
  charged exactly those five rows against a server whose posture nothing had
  declared.
- **Three catalogue-grammar guards close the #264 gaps.** A `version`
  assertion's `lifecycle_state` outside the `terminology::code|rubric|` term
  grammar is refused at the invariant; a `version` fact whose container read
  is addressed by a path parameter no flow step captures and no requires
  mints is a validate finding instead of a drive-time inconclusive (the gate
  immediately surfaced and fixed five more cases of the #280 class); and
  `todo-issue-refs.sh` holds the `TODO(#NNNN):` form over every hand-written
  YAML, shell and TOML file, self-tested, in CI beside the `.rs` guard.
- **`docker compose up` starts the console with no clone and no build
  (#297).** The operator compose file `docker/docker-compose.yml` pins the
  console image to the workspace version, binds loopback port 3210, and
  mounts the working directory; every release from the next cut attaches it
  beside the binaries, `check-console-pin.sh` holds its image tag to the one
  engine value, and the release pipeline refuses to publish without it.
- **A reproduction carries the IXIT it ran under, and its digest is
  re-derivable (#284).** `ixit_digest` is now the leading 8 bytes of the
  SHA-256 over the declaration's bytes, lowercase hex, so a reader checks a
  published record with `sha256sum ixit.json | cut -c1-16`; the previous value
  came from `DefaultHasher`, whose algorithm the standard library leaves
  unspecified across releases, so nothing outside one build could reproduce
  it. The reproduce lane copies the topology's ixit into the bundle as
  `ixit.json`, attests it beside `results.json`, and fails the run when the
  recorded digest does not re-derive from the carried bytes. Registry entries
  gain an `ixit` artifact role so a committed entry pins that declaration, and
  the results schema pins `ixit_digest` to 16 lowercase hex characters. The
  first reproduction recorded `186989ede4f387fc` with no way to resolve it,
  which left 40-odd admin rows not-applicable for a reason no reader could
  check.
- **The 429 publish guard is pinned in both directions, the driver's exchange
  names its URL, and a failed statement reset no longer poisons a probe's
  attribution (#145).** The rule that a rate-limited window never becomes a
  published measurement now lives in one `refuse_rate_limited_record` seam
  that the measured and stress instruments both call, with a test for each
  direction: a latched 429 withholds the record, a clean window publishes.
  The recorded exchange's field is `url`, which is what it has always held
  and what the transcript already publishes. `aql-probe` reports a failed
  `pg_stat_statements_reset` and withholds that probe's statement rows,
  where it used to drop the failure and charge one probe with the previous
  probe's cost.
- **An undeclared signing posture no longer reds the row against the server
  (#279).** A `signature` assertion asking for `verifiable` where the ixit
  declares no `signing` at party or instance level now records the row
  inconclusive with the citation: the mode is a deployment fact the wire never
  discloses, so a declaration the party did not make is not evidence of a
  violation. `validate` gained the other half — a statement claiming the
  `Signing` capability beside an ixit that declares no posture is a
  claim-completeness finding, before any SUT is composed.
- **The two mixed-change-set version counts read one container, and the
  contribution delete case authors the full lifecycle term (#282).** A
  revision history belongs to a single versioned container (RM
  `revision_history_item.adoc` §Description), so the counts that summed
  EHR_STATUS and COMPOSITION versions now state what the EHR_STATUS container
  holds: 2 for the deactivating case, 3 for the reactivating one. The
  COMPOSITION-container half moves to two new cases that read that container's
  own revision history by uid. `commit_contribution-delete` compares
  `lifecycle_state` against `openehr::523|deleted|`, the term RM
  `original_version.adoc` §Attributes types the attribute as, instead of the
  bare code `523`. The catalogue carries 1141 case cores.
- Three bench error variants (`FixturePin`, `UnknownProfile`, `NoProfiles`)
  carry the `PackId`/`FixtureKey` newtypes instead of bare strings, the
  posture-contradiction payload is boxed with the error-size posture recorded
  on the enum, and the template-linkage test asserts the structural read
  (#218).
- **`uid_pattern` judges the version identity the row resolved (#263).** The
  assertion compared its pattern against a version envelope read that is
  addressed by the same uid, so a conformant server echoed the value back and
  the comparison could never bite; it now judges the `ETag` or commit-body uid
  directly, which also makes the fact judgeable for the directory delete, the
  two demographic party-relationship rows, and a contribution spanning two
  containers.
- **The measured path fails loud instead of substituting a value (#253).** A
  failed integer conversion no longer collapses corpus addressing onto one
  entry or maps a journey onto shard 0: the corpus stride and the capture
  shard are reduced before they narrow, so every arrival addresses what it
  planned to. A workload share naming a journey the catalogue does not carry
  is now a named finding at the schedulable filter, so the operator reads
  which name failed. One principal set drives both what a measured window
  plans and what it fires, which is why `run_window` and `run_stress` no
  longer take one separately.
- Six `create_composition` roundtrip cases now bind the container uid their
  version assert reads, so the revision-history read resolves instead of
  erroring on an unbound path parameter (#280).
- `validate` now finds a step-level `equivalent to: committed` assertion whose
  flow commits no payload at or before the asserting step; the shape was a
  drive-time failure charged against the row instead of a catalogue finding
  (#256).
- The pack-preflight diagnostic for an invalid template example prints as one
  normally spaced sentence; a lost line continuation had left runs of
  mid-sentence spaces in the operator-facing message (#174).
- The transcript replay refuses a pack case that reads a provisioned
  `requires` handle, instead of binding one hard-coded EHR id every entry
  shared; the design record now states the replay's judge-or-refuse contract
  on both seams (#261).
- **A version envelope in hand must BE a version (#278).** The driver reused
  the step's own response body as the `ORIGINAL_VERSION` envelope whenever its
  `uid.value` matched the version under assertion. A `COMPOSITION` or `PERSON`
  served under `Prefer: return=representation` repeats that same
  `OBJECT_VERSION_ID` and carries no `commit_audit`, so 20 rows of the first
  registry reproduction reported "carries no commit_audit.change_type" against
  a server that serves the audit correctly. The shortcut now also requires the
  `_type` the released ITS-JSON binds a `VERSION` to, and any other body falls
  through to the family's envelope read.

### Removed
- The `content_generation` registered-exception kind. Content cases execute
  through the synthesized functional flow, so nothing could raise it and no
  published record ever carried it (#248).

## [0.1.1] - 2026-08-30

### Added
- **A public results registry with signed submissions and two labelled tiers
  (#158).** Published results now live in one append-only tree, conformance and
  benchmark alike, and every entry carries who submitted it, what they
  disclosed, the artifacts it stands on by digest, and how far anyone here
  verified it.
  - `schemas/registry-entry.schema.json` is the versioned entry format, emitted
    from the code like every other published schema. The disclosure is
    mandatory: the submitter and their relationship to the system, the
    deployment with its image digests, the instrument version, the machine, what
    was switched on behind the result, and the interest the submitter holds in
    the outcome. `schemas/registry-topology.schema.json` is the second new
    schema, for the deployments the reproduction lane may compose.
  - The tier is the discriminant of the entry's provenance block, so it cannot
    be claimed without the evidence its variant requires. `reproduced` names a
    workflow of this repository, its run, and the attestation predicate;
    `self-reported` names the scheme, the signature, the artifact it covers and
    the command that checks it. **No signing key exists in this repository or in
    its Actions**, and the gate refuses one committed under the registry.
  - `.github/workflows/registry-reproduce.yml` is the tier-1 lane: it composes a
    topology declared under `registry/topologies/`, drives the catalogue, and
    attests the run, the judgement and the images that actually answered from
    the workflow's own OIDC identity through Sigstore. It composes nothing a
    submitter wrote, and it does not gate a merge.
  - `scripts/checks/registry-submission.sh` is the submission gate, wired into
    CI as its own tier: append-only over the entries and their evidence, the
    schema and the rules, id uniqueness, every artifact digest recomputed, the
    supersede edges resolved, the pairing with the benchmark board's records,
    and both boards held to what is committed.
  - Two rendered boards, kept separate on purpose. `conformance-board.html` is
    new and reads its numbers out of each entry's own `verdicts.json`;
    `benchmarks.html` now takes its tier badge from the registry instead of
    printing one constant. Both carry the standing boundary: an entry is a
    report, never a certificate.
  - `registry/RULES.md` is the published submission contract, covering the two
    tiers, the mandatory disclosure, append-only with supersede-by-reference,
    the dispute path, and the standing authorization a hosted-endpoint
    reproduction needs.
- **The Robot-battery coverage gaps close on released ground (#220).** Eleven
  cases and one binding variant, each derived from the released text rather
  than from the foreign suite's expectation.
  - AQL `LIKE` gains its escape row: master03 §LIKE states "To match a literal
    `?` or `*`, the respective character in a pattern must be escaped by using
    the backslash `\` character", so an escaped wildcard selects only the
    literal character while the unescaped twin still matches the whole set.
  - `ORDER BY` and the aggregates gain their non-numeric rows: a DV_DATE_TIME
    and a DV_TEXT path sort ascending by default, descending on `DESC`, and
    resolve a tie on the leftmost expression with the next one; `MIN`/`MAX`
    return the edges of a non-numeric ordered set in the input's own type and
    `COUNT` returns an Integer over the same argument.
  - A `result_set` assertion may declare `cells: instant`, and the two
    non-numeric query cases above declare it on their date/time rows. ITS-REST
    `docs/overview/Resources.md` §Datetime format assigns the query path only
    a SHOULD ("Retrieval or querying those resources SHOULD return date,
    datetime, or time values in the (original) format provided by underlying
    backend engine"), and BASE `iso8601_timezone.adoc` §Description makes `Z`
    "a literal meaning UTC …, i.e. timezone `+0000`", so a served `+00:00` is
    the same fact as a committed `Z`. Under the mode the row gates on the
    instant and the run prints the tolerated respelling as an `observed:` line
    beside the tally. The default is unchanged exact-lexeme comparison, which
    is what tests the unconditional write-path sentence one line above the
    SHOULD; `match: count` refuses the modifier, because it compares no cell.
  - `SUM` and `AVG` declare Integer or Real input and assign nothing outside
    it, so register entry AMB-227 (option_select, upstream #229) records the
    silence and twin cases pin the two branches a party may declare,
    `aql-nonnumeric-aggregate-refused` and `aql-nonnumeric-aggregate-executed`.
    No case asserts a value the released text leaves unassigned.
  - The `is_modifiable` refusal reaches the update and delete routes. The
    composition and directory PUT and DELETE are refused on a deactivated EHR
    under register AMB-82, each row verifying through a read that the refused
    write committed nothing, and the positive twin proves the RM carve-out:
    the EHR_STATUS of a deactivated EHR is still writable, which is the only
    route by which an EHR is reactivated.
  - RESULT_SET metadata is judged exactly as far as the released ground
    reaches. The docs text calls every member "optional (implementation
    dependent)" and the released schema permits additional members, so nothing
    is required; what a service does serve is held to its declared shape by a
    new `absent_or_matches` field predicate, which passes on an absent member
    and judges a present one.
  - The catalogue carries 1130 cases and 249 bindings; the `AqlBasic`,
    `CompositionOps`, `DirectoryOps` and `EhrStatus` depth floors move to 52,
    63, 95 and 52.
- **The content-validation chapter reaches inside the data structure (#220).**
  Nine cases close the constraint axes the chapter never carried. A dedicated
  `CONT-DV_TEXT-validate_pattern` runs a real regular expression through
  `C_STRING.pattern` on a DV_TEXT, the sibling the DV_URI and DV_EHR_URI cases
  already had. Four cardinality cases constrain the member container of each RM
  container class — `ITEM_TREE.items`, `ITEM_LIST.items`, `ITEM_TABLE.rows` and
  `CLUSTER.items` — over all six intervals and member counts 0, 1, 3 and 6;
  `CLUSTER.items` is 1..1 in the RM, so a zero-member CLUSTER is refused even
  under an unbounded cardinality, which is the one place the four classes
  differ. `CONT-ELEMENT-value_null_flavour_existence` puts
  `C_ATTRIBUTE.existence` on both ELEMENT attributes and pins the invariant
  that ties them: exactly one of `value` and `null_flavour` is present, so
  committing both or neither is refused. Three `CONT-ITEM-type_*` cases narrow
  an ITEM_TREE member slot through `C_OBJECT.rm_type_name` across the abstract
  ITEM class and its CLUSTER and ELEMENT subtypes. The catalogue carries 1128
  cases, and the `ArchetypeValidation` depth floor moves to 134.
- **A client body `uid` never survives a content-object commit (#202).**
  Register entry AMB-226 (fixed_handling, upstream #221) records the silence
  the released text leaves on `composition_create`, `directory_create` and
  `ehr_status_update`: none of the three says what a service does with a
  client-supplied body `uid`, and none declares a conflict response to refuse
  with, where the CONTRIBUTION create states the rule and the COMPOSITION
  update states a match rule against the URL identifier. The commit succeeds
  and the served identity is the server-minted `OBJECT_VERSION_ID`. Three
  cases pin it — `I_EHR_COMPOSITION.create_composition-client_supplied_uid`,
  `I_EHR_STATUS.clear_ehr_queryable-client_supplied_uid` and
  `I_EHR_DIRECTORY.create_directory-client_supplied_uid` — each asserting the
  commit is accepted, that the version identity the response names is not the
  client's value, and that the client's own identifier addresses nothing
  afterwards.
- **The web console reads bench records (#166).** A `/benchmarks` surface
  lists every `bench-result*.json` under the mounted output directory, and
  takes uploaded ones through a plain HTML form that needs no JavaScript. An
  uploaded batch is transient and swept on a timer, because the console stores
  nothing of its own. A record opens in full: the pack and the machine that
  offered the load, the posture block with every item labelled `verified` or
  `declared-only`, whether the record is submittable and which requirement it
  misses, the cross-repetition percentiles in microseconds with the millisecond
  reading beside them, the failed-arrival reading of every repetition and phase
  on the target and on each baseline, the same-machine baselines with their
  pinned images and recipe, and the relative index the record derived. Two or
  more records align side by side, and the pack, host, posture, scale and
  submittability mismatches are stated above the numbers rather than left for a
  reader to notice. Every figure carries the discipline that produced it, and
  the benchmark-versus-conformance boundary statement renders verbatim from the
  record on every one of those views. The per-operation `HdrHistogram` V2
  encodings are tabulated rather than drawn: decoding one is the engine's own
  histogram reader, which the console reaches once its engine pin carries the
  bench module (#179).
- **`is_modifiable` judges the whole CONTRIBUTION, in any member order
  (#201).** No released text says when `EHR_STATUS.is_modifiable` is evaluated
  relative to a commit, so register entry AMB-225 records the atomic-set
  reading and the schedule gates on it: a content member is refused exactly
  when the EHR is deactivated and the change set carries no `EHR_STATUS`
  member setting `is_modifiable = true`, whatever position the members hold.
  Two new cases close the open corners — a mixed set whose status member keeps
  `is_modifiable = false` is refused whole, and an `EHR_STATUS`-only set is
  accepted against a deactivated EHR because the object is always modifiable.
  The catalogue carries 1116 cases, and the `ChangeSets` depth floor moves to
  111. Reported upstream as #215.
- Register entries AMB-223 and AMB-224 (#206): the BMM type-conformance
  algorithm's missing simple-descendant-of-generic-ancestor rule (report_only,
  upstream #211) and ADL 1.4's stated {1..1} existence default that the
  published corpus contradicts (fixed_handling — an unstated existence defers
  to the RM's effective existence; upstream #212).
- **The board and the gate speak posture (#204).** The public board groups its
  rows by the posture profile each run declared and ranks only inside a group,
  so a `minimal` row and a `clinical-default` row never share a ranking, and the
  page states that rule in plain words. Every row prints its profile with the
  items the canaries verified and the items that stay declared-only, and its
  disclosure carries the full posture table plus any reference deployment that
  ran a different posture.

  The submission gate refuses a record whose observable canaries did not verify.
  Version signing, commit validation, authentication, TLS and compression each
  have an observable that always exists, so each is `verified` on the target and
  on every baseline or the submission is refused with the item named; a block
  claiming audit or tenancy was verified is refused the same way, because
  released ITS-REST discloses neither.

  Each baseline pin records the posture its upstream recipe actually configures,
  read first-hand at the pinned tag. EHRbase 2.35.1 configures no audit trail
  and no version signing, validates commits against the operational template,
  switches no response compression on, and enforces Basic authentication.
  FerroEHR 4.0.11 configures the same, except that its recipe leaves version
  signing on in digest mode. Where a pin disagrees with the profile the target
  declared, the baseline runs and declares the pin's value, so a canary never
  fails on a declaration the instrument itself manufactured, and the new
  `posture.comparability` block in the published bench-result schema names each
  such item with the recipe element it was read from.
- **Bench posture profiles, with bracketed canaries (#165).** Two speed numbers
  are comparable only when the same features were switched on behind them, so
  every embedded pack now defines named posture profiles and a run declares
  exactly one with `bench --posture <NAME>`. Every pack defines `minimal`, the
  bare spec-conformant surface, which is also the default; `community-vitals`
  also defines `clinical-default`. The result document's `posture` block, until
  now a reserved null, carries the profile, its summary, and one line per
  disclosed item: audit sink, version-signing scheme, commit-validation depth,
  authentication mode, TLS, response compression and tenancy, each a closed
  vocabulary whose unknown token is a loud error.

  Each item is then checked black-box and labelled `verified` or
  `declared-only`, with the canary evidence recorded beside it. Signing samples
  versions committed by the run's OWN seed traffic and inspects their
  `signature`, so a scheme switched on for a probe alone never reaches them.
  Validation commits the pack's pinned invalid twin — that pack's own
  composition with the mandatory `COMPOSITION.composer` removed — and reads the
  answer. Authentication offers one uncredentialed read, compression reads
  `Content-Encoding` back over a client that does not decompress, and TLS comes
  from the recorded base URL's scheme. Audit and tenancy stay honestly
  declared-only, because released ITS-REST surfaces no read resource for
  either.

  The canaries run BEFORE and AFTER the measured window. A reading that
  contradicts the declaration, and a pair of brackets that disagree with each
  other, both refuse the whole run with a typed error naming the item; neither
  is ever a footnote on a published figure. Same-machine baselines run under
  the profile the target declared and carry their own verified block, and
  `bench-compare` states a posture disagreement in the header, above the
  numbers, beside the pack and host mismatches it already reported.

- **The benchmark legend, generated from the binary (#189).** A new
  `bench-packs --out DIR` subcommand writes `bench-packs.json`: every embedded
  pack's id, version, seed and phases, each phase's load discipline and counts,
  each measured phase's operation mix with the share, the offered rate and the
  probe rationale of every entry, each posture profile with what it declares
  item by item, each fixture's sha256 pin with its size and its provenance, the
  closed operation vocabulary with the request each token puts on the wire, and
  the requirements a record meets before it may be ranked. Emission is byte-deterministic and `schemas/bench-packs.schema.json`
  publishes its shape. The public page at
  `veredictum.eu/benchmark-methodology.html` is rendered from the committed
  copy of that document by `scripts/render/bench-legend.sh`, so a page that
  described a pack the binary no longer runs cannot exist: the integration
  suite holds the document to the packs, and `--check` holds the page to the
  document, in the site build and in CI. The board links the legend and the
  legend links back to the board and to the submission guide.

- Multi-valued-predicate coverage (#178): register entry AMB-222 pins the
  any-element reading of a WHERE predicate over a multi-valued path (upstream
  report #195), and three QUERY cases commit compositions whose SECOND link or
  participation carries the queried literal — the first-element-only
  evaluation class FerroEHR#2919 fixed goes red on contact — plus the
  zero-row negative twin.

- **The `aql-mix` bench pack (#188).** `bench --pack aql-mix` measures AQL
  query speed over the same Vital signs population `community-vitals` seeds,
  from the same two sha256-pinned fixtures, so a query figure and a read figure
  describe the same corpus. The seed phase creates 50 EHRs and commits the
  composition 20 times into each, a population the pack version pins and sizes
  for query shapes. The measured phase is open-loop at 24
  arrivals a second for 60s after a 15s warmup, over six query classes at equal
  share: a uid point lookup, an EHR-scoped scan, a filtered magnitude
  predicate, the same predicate unscoped under a fetch bound, a `COUNT`
  aggregate, and an ordered page read through a moving fetch window. Each class
  posts its own AQL statement to `/query/aql`, accepts only 200, and counts
  every other answer in its own error class, so the record carries one set of
  percentiles per class and a server that refuses one shape never contaminates
  another. Thresholds, page offsets and targets draw from the run's seeded
  streams, and the seed is disclosed in the record. Each class states in the
  versioned pack definition which storage behaviour it probes, and the
  baselines, the relative index and `bench-compare` cover the pack unchanged.
- **The public benchmark board (#187).** `website/landing/benchmarks.html`
  carries one relative index per reference CDR on every row, EHRbase and
  FerroEHR both, because each submission composes them all on its own host and
  a board with a single reference would be a verdict about that one product.
  Rows sort by the FerroEHR index and the page says so. Each row also states
  the tier badge, the pack and version, the repetition count, the absolute
  median and 99th percentile, the fingerprint of the machine those
  milliseconds came from, and how many measured arrivals failed. A `<details>`
  disclosure carries the per-operation table with one index column per
  reference, the composed image digests and every gap where no ratio could be
  formed; the page loads no script. The boundary statement renders at the top,
  and an explainer says why the board ranks by a ratio, why there are two
  references, and what a self-reported tier does and does not claim. The page
  is generated by `scripts/render/bench-board.sh` from
  `benchmarks/submissions/**` and committed, and `--check` refuses a page that
  no longer matches those records, from both the site build and CI.
- **Benchmark submissions arrive as pull requests (#187).** `benchmarks/`
  carries the append-only submissions tree and `benchmarks/SUBMITTING.md`, which
  gives the exact `bench --pack community-vitals --with-baselines` command, the
  `<system>/<date>-<host>.json` naming convention with the one-liner that
  derives the host prefix, and the tier meaning.
  `scripts/checks/bench-submission.sh` is the gate a submission passes before a
  human reads it: the published bench-result schema, the pack id, version, seed
  and fixture pins against the embedded pack, three repetitions, a same-machine
  baseline with its relative index derived, the environment fingerprint, a file
  name that digests from that fingerprint, a refusal of any operation whose
  every recorded arrival failed on either the target or a baseline (#197: the
  engine stamps `submittable: true` on such a run, because submittability
  counts repetitions and baselines and never reads an error count), and a
  refusal of any modification, deletion or rename of a record already merged.
  The `benchmark submissions` CI job runs it on every change under
  `benchmarks/`, and gates the merge.
- **Bench baselines and the relative index (#184).** `bench --with-baselines`
  measures the target, then composes each pinned reference CDR on the same
  host, drives the same pack at the same seed for the same repetitions against
  it, and tears the stack down with its volumes. EHRbase 2.35.1 and FerroEHR
  4.0.11 are pinned by image digest, with their upstream deployment recipes
  named at an immutable tag and the same container ceilings applied to both.
  Every baseline lands in the record as a full per-operation summary beside its
  digests, its recipe reference and those ceilings. From target and baseline the
  record derives the relative index: per phase, operation and metric, the
  target's cross-repetition median divided by the baseline's, dimensionless and
  serialized with both inputs. Where no ratio exists — an operation only one
  side measured, a phase only one side ran, a zero baseline median — the record
  carries a typed gap rather than an omitted row. `--with-baselines` refuses on
  a host whose `docker` CLI does not answer, naming the binary, before the
  target is touched; a run without the flag needs no container runtime.
- Submittability now states its reasons. A record is submittable with at least
  three repetitions AND at least one same-machine baseline; the `submittable`
  boolean keeps its meaning and a new `submittable_unmet` list names each
  requirement a record misses, printed on the bench summary and in every
  `bench-compare` column header. The environment fingerprint renders on the
  summary header, in the CLI output and in every comparison column, so no
  number appears without the machine it came from. `bench-compare` gains a
  relative-index table and warns when columns from different hosts carry no
  index to compare across them. `schemas/bench-result.schema.json` gains
  `baselines`, `relative` and `submittable_unmet`.

- AQL folder-containment coverage (#156): a provisioned directory-tree fixture
  over the run's own committed compositions, eight new QUERY cases
  (FOLDER↔COMPOSITION and FOLDER↔FOLDER pairs, name scoping, NOT CONTAINS,
  ORDER BY row-set invariance, the cartesian-product detectors, and the
  option-gated undefined-pair twins), register entries AMB-218/AMB-219/AMB-220
  with upstream reports #159/#160/#161, and driver support for rendering
  precondition fixtures against the committed set.
- Register entry AMB-221 records the SM/ITS-REST RESULT_SET required-set
  divergence (upstream report #169); the wire assertions keep the ITS floor.
- **Bench pack one: the community vital-signs harness (#164).** `bench --pack
  community-vitals` reproduces the openEHR community's own benchmark harness
  (<https://discourse.openehr.org/t/17224>) and measures the same work a second
  way. The write phase creates 100 EHRs and commits the same Vital signs
  composition 1,000 times into each with `Prefer: return=identifier`, on one
  worker, and reports bulk-load throughput plus the whole-loop
  milliseconds-per-composition average the thread quotes. The read phase then
  runs twice over that population: `read_walk` is the sequential walk the
  harness performs, seven GETs against every committed composition, reporting
  the whole-loop microseconds-per-request average; `read_open_loop` offers the
  same seven reads as an arrival schedule pinned at 200/s for 60s after a 15s
  warmup, which is where the coordinated-omission-free percentiles come from.
  The pinned rate is part of the pack version.

  Both fixtures are embedded byte-identically and pinned by sha256, verified
  when the pack loads: the operational template from the vendored CKM export
  for template id `Vital signs`, the composition from the attachment on post 8
  of that thread.

  Every number in the record now carries the discipline that produced it. Bench
  results gain a closed-loop `sweeps` block per repetition, a `regime` on every
  cross-repetition phase summary, `whole_loop_ms_per_composition` on a seed
  phase, and a `scale` block saying whether the run matched the pack's pinned
  configuration. `bench-compare` prints the discipline per row and names a
  scale or configuration mismatch in the header. Two new flags: `--scale`
  shrinks the EHR count for a quick run, `--seed-workers` overrides the worker
  count a seed phase declares, and either one takes the run off the reference
  configuration, which the record, the rendered summary and the comparison all
  state. The operation vocabulary gains the six composition reads the harness
  exercises, each with its wire realization and its own error classing.

- **The universal-benchmark engine (#163).** Two new subcommands measure
  comparative speed against any reachable openEHR CDR, with no artifact root,
  no IXIT and no party statement. `bench --base-url <URL>` drives an embedded
  pack: a seed phase bulk-loads a fixed corpus through the public API, then the
  measured phases offer a seeded open-loop arrival schedule over a closed
  operation vocabulary, with every latency taken from the planned arrival
  instant. The run seeds once and repeats the measured phases, three times by
  default; a result carrying fewer than three is recorded as not submittable.
  `bench-compare --result <FILE> --result <FILE>` aligns two or more committed
  results into one table, one column per file, and names every pack-version or
  host mismatch in the header before any number.

  Credentials never ride the command line: `--auth basic` reads
  `VEREDICTUM_BENCH_PASSWORD` and `--auth bearer` reads
  `VEREDICTUM_BENCH_TOKEN`. Before anything is measured, a preflight reads the
  template list, uploads the pack's template, then creates one scratch EHR,
  commits a composition into it and reads it back; a failure at any of those
  refuses the run and names the exchange, so a half-measured document never
  exists.

  Packs are compiled into the binary with a sha256 pin on every fixture,
  verified at load and recorded in the result. A bench result is a benchmark
  record for comparative speed. It is not a conformance record, not a
  certificate, and not a performance-class rating; a bench result may motivate
  a class run, never substitute for one. That sentence is a schema-required
  constant in the artifact and is printed with every rendered view.

- **`schemas/bench-result.schema.json`**, the published schema for the new
  artifact family: pack identity with its fixture pins and seed, the target
  with its userinfo stripped, the generator's host fingerprint, per-repetition
  per-operation counts, error counts by class, percentiles in microseconds and
  the re-checkable `HdrHistogram` V2 encoding, the cross-repetition median and
  inter-quartile range, and the methodology block. The `posture` object is
  reserved and always absent.

### Changed

- **Every unjudgeable `version` assertion is adjudicated per case (#238).**
  #225 made the assertion family judge at drive time and #245 routed an
  unjudgeable one to the inconclusive channel, which left the catalogue
  carrying sites no released read can settle. Each was adjudicated first-hand
  against the vendored ITS-REST text, and no expectation was bent to reach a
  verdict.
  - Nineteen contribution and security cases now bind
    `versioned_object_uid` from the commit's own
    `first_versioned_object_uid` capture, so the released
    `versioned_composition_revision_history` /
    `versioned_party_revision_history` read and the released VERSION envelope
    read can address the container the assertion judges.
  - The three multi-member commit cases move their change-set membership onto
    `contribution_get` (`200_CONTRIBUTION.yaml` over
    `schemas/common/Contribution.yaml`, whose `versions` array carries one
    `OBJECT_REF` per committed VERSION). `assert: version, count` counts one
    container's versions, and those commits create two containers, which the
    released ITS publishes no read for.
  - Fifty-six refusal cases stop authoring `count: 0`. The refused write minted
    no container identity, and `specifications/ehr.openapi.yaml` and
    `specifications/demographic.openapi.yaml` address every version read by
    `{versioned_object_uid}` or `{version_uid}`, so no released operation
    counts the versions a service holds. The refusal outcome the flow step
    asserts is what the wire discloses, and it still gates. Each case records
    the boundary in its header.
  - The `VERSIONED_FOLDER` and `PARTY_RELATIONSHIP` families stop authoring
    version facts, with the realization gap cited on each case (register
    AMB-24 and AMB-32). `specifications/ehr.openapi.yaml` carries
    `/ehr/{ehr_id}/directory` and `/ehr/{ehr_id}/directory/{version_uid}` and
    nothing versioned, and `directory_get_by_version_id` answers
    `200_FOLDER_retrieved.yaml`; `specifications/demographic.openapi.yaml`
    declares no `/demographic/party_relationship` path at all. Where a case
    lost its only in-flow verification it gained a `verified_by` naming the
    sibling case that reads the fact back over a released route.

- **The console's engine pin names the version being released (#179).** The
  pin could only ever name a version crates.io already carried, so the console
  normally shipped against the previous engine and every window between a tag
  and the following bump ran the console's current code against an engine that
  did not carry its flags. The workspace root now redirects the console's exact
  crates.io pin to `app/veredictum` with `[patch.crates-io]`, so the pin is one
  value — the workspace engine version — while the console's own manifest still
  names the engine by an exact registry version and a console release stays
  reproducible from published artifacts. `scripts/release/check-console-pin.sh`
  holds the manifest pin, `ENGINE_PIN`, the engine's version and the tag to that
  one value, refuses a `Cargo.lock` that resolved the pin against the registry,
  and runs on every pull request as well as at tag time. The release graph is
  unchanged: the crates.io upload stays last, after the image is built, smoked
  and scanned.

  Everything that skipped for the length of a cut window now drives or fails.
  The console-versus-CLI document gate, the sealed-export gate, the scope and
  record gates and the two driven browser journeys carried a version-drift arm
  that printed a reason and returned success; those arms are gone, and a
  mismatched engine fails the gate with the version it reported. `scripts/ui-e2e.sh`
  carries no cut-window branch and always builds and drives the engine.

### Fixed

- **The structural content families refuse an unknown token instead of baking a
  permissive default (#241).** `cardinality`, the four `*_existence` axes and
  the two `slot_type` axes were read with a fallback: an unrecognized
  `cardinality` cell synthesized an unbounded `0..*` container, an
  unrecognized existence cell synthesized `0..1`, and an absent cell did the
  same. A mistyped `3to5` therefore graded the server against a constraint no
  row declares, and the row passed. Each axis is now a closed vocabulary
  (`any|1plus|3plus|opt|mand|3to5`; `optional|mandatory`; the RM `EVENT` and
  `ITEM_STRUCTURE` class hierarchies), an unknown or absent cell is a typed
  refusal at drive time, and the new `content-synthesis` validate gate
  synthesizes every row of every varying-constraint content case, so the typo
  is a finding before any server is composed. Every token the committed
  catalogue spells is unchanged.
- **A `default` ordinal cell bakes the mild/severe pair it names (#241).** The
  reserved token was accepted by the droppable-member pre-flight and then
  parsed into an EMPTY entry list, so the emitted OPT constrained that interval
  bound to a value set matching nothing. It now yields the fixed
  `at0005`/`at0006` pair the corpus template carries, which retires the
  unreachable fallback that used to hold it.
- **`CONT-DV_URI-validate_pattern` applies the pattern it declares (#241).**
  The case's `C_STRING.pattern` column read `https://.*` while the baked
  template applied `https?://.*`, so the declared cell was documentation. The
  case now declares `constraint_columns: ["C_STRING.pattern"]` and the runner
  synthesizes one OPT per row from that cell. Both rows keep their expected
  outcomes.
- **The corpus OPT generator's reproducibility contract is enforced (#241).**
  `generate_content_opts.py` printed a warning for two manifest keys
  (`cnf.tpl.dv_coded_text_binding_sct`, `cnf.tpl.dv_coded_text_binding_tsdown`)
  whose committed OPTs no builder could produce, contradicting the script's own
  header. Both builders exist now, carrying the `component_ontologies` block
  that defines and binds `ac0001`; the key-set mismatch exits non-zero instead
  of warning; and `scripts/checks/corpus-opt-reproducible.sh` re-runs the
  generator in the CI guard tier. The two regenerated templates differ from
  their committed bytes only in `<uid>`, which now follows the same
  deterministic namespace every other generated OPT uses.
- **A measured window fails on an arrival the generator could not fire
  (#233).** The collector counted a generator fault on a `u64::MAX` latency
  sentinel that the only producer of a completion could never emit, so the
  count was always zero and the run-failing arm behind it never ran. An
  arrival now reports either a measured completion or a typed fault. The one
  fault the dispatcher can hit is an arrival whose principal the driving ixit
  declares no instance for, and it stops the window with the count and the
  faulting operations named, where it used to be recorded as a wire error
  against the server. `perf` and `stress` runs are unaffected while every
  arrival fires; a run that loses arrivals to the instrument now says so
  instead of publishing a measurement over the arrivals that survived.
- **Row postconditions run the same dispatch a step's assertions do, and the
  transcript replay refuses what it cannot judge (#239).** The live driver's
  postcondition arm judged `field` and `version` and silently dropped
  `equivalent`, `returns`, `result_set`, `instance_of`, `xml_root` and
  `signature`, on the claim that those ride a flow read step; one catalogue
  case asserts `equivalent` as a postcondition over a flow with no read step,
  so that assertion passed without being evaluated. The seam now judges the
  row's postconditions through the one assertion dispatch the flow steps use,
  against the row's last completed step — its exchange, the binding whose
  `server_assigned` set an `equivalent` comparison excludes, and the signing
  posture of the instance that step ran on — so a family cannot be judged
  inside the flow and skipped after it. A row that completed no step records
  its postconditions inconclusive instead of passing them. The transcript
  player answered every postcondition with an empty list, which would have let
  a verification-pack entry claim a reproduced verdict over assertions nobody
  ran; it now refuses such an entry by name, since a transcript carries the
  flow's own exchanges and no versioned read, corpus reference or instance
  posture. Aggregate `unique` (law e) and the informative `message_exemplar`
  and `state` families are unaffected in both drivers.
- **The transcript replay evaluates a step's assertions, or refuses the entry
  (#255).** The player returned an empty failure list for every flow step, so a
  verification-pack entry reproduced its adjudicated verdict without evaluating
  a single `assert:` block — the pack's own passing case asserts
  `instance_of: EHR` on its step-3 read-back and that assertion was never run.
  A step's assertions are now judged from the recorded exchange through one
  shared dispatch the live driver runs too: `returns` (its Boolean wire-presence
  rule included), `xml_root`, `instance_of`, a `field` whose comparand carries
  no `${…}` reference, and a `result_set` with inline rows, a count or columns.
  The families whose ground a transcript never records — `equivalent` (the
  payload committed earlier in the row), `version` and `signature` (a versioned
  read and the addressed instance's signing posture), a referencing `field`
  comparand and `result_set rows.from` (the resolver and the committed-set
  uids) — refuse the entry by case, step and family, the way the postcondition
  seam already does. A recorded exchange that contradicts its own step
  assertion now fails its row instead of reproducing a pass.
- **`assert: version` judges the envelope it names (#225).** The driver's
  version arm evaluated to `Ok(())` unconditionally, so every one of the
  catalogue's authored version assertions reported a pass it never earned. The
  arm now reads the `ORIGINAL_VERSION` envelope the assertion names — the
  step's own body when that body already is the envelope, otherwise the
  family's `…/version/{version_uid}` read — and judges `uid_pattern` against
  `uid.value`, `change_type` against `commit_audit.change_type` mapped from the
  openEHR `audit change type` codes, and `lifecycle_state` against the served
  coded term. `count` is judged against the family's `REVISION_HISTORY`, which
  is the only released wire surface disclosing how many versions a container
  holds: `VERSIONED_OBJECT.version_count` is an RM function, and the released
  ITS-JSON `VERSIONED_OBJECT` schema closes the served object to
  `uid`/`owner_id`/`time_created`. A declared fact the ITS gives no read for —
  a `count` on a family with no revision history, an envelope member on a
  family with no version read, a version reference that resolves to nothing —
  is named loudly instead of passing silently, and is recorded inconclusive
  rather than charged to the server (#237, below).
- **An unjudgeable assertion is inconclusive, never a finding against the
  server (#237).** Every assertion failure became `failed`, which on a
  published record reads as a conformance finding the run proved. An assertion
  the run could not judge at all proves nothing about the server: the released
  ITS realizes no read for the fact, the case reaches no single versioned
  family to read, the authored `uid_pattern` carries a token outside its
  closed vocabulary, or a prerequisite the assertion reads was never bound
  because the row's earlier refusal was the expected outcome. Those now record
  `errored` (ISO/IEC 9646 *inconclusive*) beside a transport fault, with the
  same reason text, so triage attributes them to the runner or the catalogue
  where they belong. A served value that contradicts an assertion still fails
  the row, and a mismatch outranks an unjudgeable sibling assertion so a real
  finding is never hidden behind an inconclusive row. The channel is carried
  as a type through the step and postcondition seams, never inferred from the
  message text.
- **A failed run is never submittable (#197).** Submittability counted
  repetitions and baseline blocks and read no error count, so a run whose
  arrivals all failed still stamped `submittable: true` and was rankable on the
  public board. Every embedded pack now pins a failed-arrival ceiling of `0.01`,
  disclosed in the result as `pack.max_failed_share` and stated in the pack
  description, and `submittable_unmet` gains the `error_share` requirement: any
  repetition, phase and operation above the ceiling, on the target or on any
  baseline block, refuses the record for ranking. An operation that recorded no
  arrival at all counts as fully failed rather than dividing by zero. The
  rendered summary prints the failed-arrival share per phase for both sides and
  names every reading above the ceiling, and `bench-compare` prints the worst
  share beside its ceiling per column. The record stays valid for local
  diagnosis; it is only never rankable. `bench-packs` carries the ceiling per
  pack, so the published legend states it beside the seed. The submission gate
  keeps its own never-answered floor as defence in depth and now also asserts
  that a record claiming `submittable` agrees with the engine's arithmetic.

- **The smoke pack commits a composition its own template roots at (#208).**
  `bp_composition.json` and its invalid twin declared
  `openEHR-EHR-COMPOSITION.encounter.v1` in both `archetype_node_id` and
  `archetype_details.archetype_id`, while the template they name,
  `cnf.blood_pressure`, roots at `openEHR-EHR-COMPOSITION.minimal.v1`. Both
  fixtures now carry the template's own root, their sha256 pins move with the
  bytes, and the pack is `smoke@1.1.0`: a record produced at 1.1.0 is not
  comparable with a 1.0.0 one, because the bytes offered to the server changed.
  Loading any pack now also proves the coherence the fixtures lost — every
  composition fixture's two declared root ids are read back and compared
  against the root its named operational template defines, and a mismatch, an
  unseeded template id, or an unreadable fixture refuses the load with a typed
  error naming both ids.

- The committed party statements declare their AMB-220 branch, verified
  first-hand against each running SUT: FerroEHR refuses an RM-undefined
  containment pair as an invalid query, EHRbase executes it to an empty
  result. Without a declared branch the verdict pipeline's static review
  rightly refused every judgement over these statements, which surfaced the
  moment the console's export gates woke from their engine-pin drift skip.

- **The console image runs the engine it ships against (#172).** The console
  pinned `veredictum` at `0.1.0-alpha.6` after `0.1.0` published, so the image
  spawned the pre-release engine and passed it flags that version never
  carried, `--record-exchanges` among them. The pin now reads `0.1.0` on both
  halves of the fact, the manifest dependency and the `engine` string the shell
  footer displays. The results drawer reads the run transcript through the
  published `veredictum::transcript` types instead of a console-local copy of
  that document's shape (#129), so the console and the engine cannot disagree
  about what the wire record holds. The gates that drive a real engine —
  the two browser journeys, the console-versus-CLI document gate, the export
  and record gates — skipped themselves for the length of the drift window and
  execute again.

- **A step's `with:` value wins over a same-named capture everywhere, and both
  body paths are asserted on the wire (#231).** The driver answered the
  with-versus-capture question two opposite ways in one file: a header, query
  or URL slot took the step's own value, while a structured request body took
  the capture and left the authored value unused. The step's explicit input is
  now the most specific binding at every slot, so a case that passes a name
  inline sends that value in the body exactly as it already did in an
  `If-Match`. Under the old body resolution a step passing a name an earlier
  step had captured sent the earlier value with no diagnostic, which is how a
  negative case addressing an unknown identifier could be answered about the
  provisioned one and still pass. No case in the committed catalogue collides
  this way, so no verdict moves today. The named and structured body paths also
  gained the recorded-request-body assertions the patched path already had:
  what a case puts on the wire is now read back on all three.

### Security

- The container image build and the release pipeline now fetch their pinned
  tools with `curl --proto '=https' --tlsv1.2`, so a redirect cannot downgrade
  a supply-chain download to plain HTTP.


## [0.1.0] - 2026-08-28

The first stable release: the milestone closed at zero open issues, and
the pre-release alpha line ends here.

### Added

- **Scope builds the claim from CNF tiers (#100).** The run wizard's Scope
  screen gains a row of tier checkboxes — CORE, STANDARD, OPTIONS, SEC-BASIC —
  each carrying the capabilities the capability matrix puts in that tier and
  the number of catalogue cases those capabilities gate. Both counts come from
  the published library's own `tier_members` walk, the one every profile
  verdict is computed from, so the row cannot drift from the answer the
  judgement gives. Composing writes an ad-hoc statement into the same paste box
  the vendor path uses: the product identity from the Connect step, the checked
  tiers as claimed profiles, their required capabilities as the claimed
  capabilities, the schedule release read from the committed statements under
  the mounted party tree, and the spec-component versions derived from the
  catalogue's own `applies` floors. The operator reads the document before
  saving it, and it is then validated, stored and written into the run's output
  directory exactly like a pasted one, so a verdict stays a pure function of a
  statement. Option branches stay undeclared, because only the party running
  the server knows which branch it realizes.
- **The console exports a signed record, and anyone can check one (#68).** The
  verdicts screen gains one step that hands the finished run to the pinned
  instrument's own `verdicts --sign-key`: the rendered documents, a digest
  manifest over them, and a detached OpenPGP signature over that manifest. The
  console seals nothing itself. Beside the sealed set it renders three files a
  party publishes, each carrying the record digest prefix that ties it to the
  signed bytes — the brand's seal card with its three slots filled from the
  record, a compact badge SVG with copy-paste markdown and HTML snippets, and
  a self-contained HTML report of the results and verdicts surfaces whose
  footer carries the full digest, the signer fingerprint and the signing time.
  All three are pure functions of the record, so the same bundle reproduces
  the same bytes. The whole bundle downloads as one archive.
- **`/verify` is a public record check (#68).** No run, no server and no
  account: upload a bundle and the published library recomputes every digest
  its manifest names and checks the detached signature. A tampered file names
  itself. The upload is a plain HTML form posting to a server route, so it
  works with no JavaScript at all; uploaded bundles are transient and swept on
  a short timer. The honesty box renders on every outcome — a valid signature
  proves integrity and origin since signing, not the run's conditions, not the
  system under test's identity claims, not the catalogue's coverage — and the
  page prints the `veredictum verify-record` equivalent beside it, so nobody
  has to trust the console to check the console.
- **The export surfaces carry the openEHR trademark acknowledgment and the
  independence disclaimer visibly (#94).** The seal card renders it in its
  caption area and the report in its footer, because those are what a party
  publishes.
- **`run --record-exchanges` persists the wire (#96).** The flag writes
  `transcript.json` beside `results.json`: per case, in send order, the
  request line, request headers and request body, and the response status,
  headers and body. It is off by default, and persistence is a serialization
  of exchanges the driver already holds, so a recorded run sends nothing extra
  and reaches the same verdicts as an unrecorded one. The artifact records a
  SUT's response bodies verbatim, so it can carry real patient data: it is
  operator-controlled output, never a log, and the `authorization` request
  header's value is withheld. With `--sign-key` the sealed record manifest
  covers the transcript.
- **`run-transcript.schema.json` joins the published schema set.** The run
  transcript is its own artifact family, separate from the verification pack's
  replay `transcript.schema.json`, which requires an adjudicated verdict per
  entry and carries no request side at all.
- **The console records and reads the wire.** Scope gains a "Record the wire
  exchanges" checkbox, off by default with the clinical-data caution beside
  it, and the results drawer renders each recorded exchange as request and
  response panes. A run driven without the flag says so where the wire would
  be.

### Changed

- **A measured population now varies leaf by leaf, so earlier measured records
  are not comparable with later ones (#137).** The performance pack used to
  stamp each composition with only the event-context times and the composer
  name, so every composition of a population carried identical clinical values.
  A server storing that population can share structure, index entries and cache
  pages that a real population would never let it share, which flattered every
  number measured over it — records produced before this change are flattered
  in exactly that way, and a number from one of them must not be compared with
  a number produced after it. The pack now reads the leaf constraints the
  operational template itself declares and redraws every numeric leaf inside
  its own permitted range: `DV_QUANTITY` magnitudes against the
  `C_DV_QUANTITY` interval declared for the leaf's units, and `DV_COUNT`
  magnitudes against the `C_INTEGER` range declared for a `DV_COUNT`
  magnitude. A leaf whose permitted range the template does not declare, and
  every coded, textual and date-time leaf, keeps its committed value, so no
  arrival can send an instance the template refuses. The draw is seeded from
  the template key and the arrival index, so the same run reproduces the same
  population byte for byte.
- **`perf_run::pack::PackTemplate` carries a `constraints` field.** The library
  type gains the leaf ranges read out of its operational template, which is a
  breaking change for anything constructing the struct literally.
- **The console answers an unknown address with a real page (#84).** A path
  outside the route tree used to render the bare string "Page not found."
  with no chrome, no title and no way back. It now renders inside the
  console's own sidebar and toast chrome, sets its own title, names the path
  that missed, and offers the instrument and the catalogue as routes out. The
  HTTP status is unchanged at 404.
- **The console ships a full icon set (#84).** `favicon.ico`,
  `apple-touch-icon.png`, `icon-192.png`, `icon-512.png` and a web manifest,
  every one of them rendered from the two brand SVG masters by
  `scripts/render/brand-icons.sh` so the mark cannot fork. The image serves
  them from the same `public/` mount as the seal.
- **Every run-wizard mutation reports both outcomes as a notification.**
  Saving the scope, previewing the selection, starting a run and cancelling
  one each raise a toast on success and on failure, with the failure copy
  naming the object, the instrument's own diagnostic verbatim, and the next
  action. The detailed inline panes stay beside them, which is where a
  schema finding or a per-chapter breakdown is read line by line.
- **The console reads two new environment variables.** `VEREDICTUM_SIGN_KEY`
  names the armored OpenPGP secret key the export seals with, and
  `VEREDICTUM_VERIFY_KEY` names the public half. Both are optional and both
  unset is a first-class state the surfaces explain rather than an error. The
  export asks for the public key too because it verifies its own bundle before
  stating who signed it and when: it never prints a signing time it has not
  checked. A passphrase reaches the spawned instrument through
  `VEREDICTUM_SIGN_PASSPHRASE` and its child environment only — never a
  signal, a file, a command line, or a log line.

### Fixed

- **Issue numbers inherited from FerroEHR now say so (#123).** Three
  `ambiguity-register.schema.json` and `wire-surface.schema.json` descriptions
  carried bare `#2545`, `#2546` and `#271`, which resolve to unrelated issues
  on this repository's tracker; they read `FerroEHR#NNNN` now, as do the
  runner comments and the generated wire-surface coverage report that carried
  the same ids. Description text only: no schema keyword, type or constraint
  moved.
- **`validate --write-report` writes somewhere that exists (#91).** The
  coverage report's path was climbed out of the spec tree
  (`<specs>/../../conformance/coverage-report.md`), which matched the old
  mono-repo layout and, at this one, resolved to a repository-root
  `conformance/` directory that has never existed. The report is now derived
  from the artifact root it describes and lands at
  `<ROOT>/coverage-report.md`, so it follows the catalogue rather than
  wherever the specs happen to be mounted. `coverage_report_path` takes the
  artifact root and returns a plain `PathBuf`, since the derivation can no
  longer fail.

## [0.1.0-alpha.6] - 2026-08-27

### Fixed

- **The release pipeline locates the dependency SBOM instead of assuming its
  depth (#109 aftermath).** cargo-cyclonedx writes the instrument's SBOM
  beside its crate manifest, except when the workspace member's version
  matches a published crates.io version, when it writes to the workspace
  root — both observed first-hand. The binaries lane searched a fixed depth,
  found nothing at the alpha.5 cut, and both binary legs failed; it now
  searches both locations and excludes the console crate's SBOMs by path.

## [0.1.0-alpha.5] - 2026-08-27

This cut never published: the release pipeline's SBOM step failed on both
binary legs (the fix is 0.1.0-alpha.6's entry), the draft release was
withheld by its own asset gate, and no crate or binary shipped. The signed
tag stands; 0.1.0-alpha.6 ships the same tree plus the pipeline fix.

### Changed

- **The vendored oracle is pinned to released tags (#78).** RM, BASE, AM,
  TERM, LANG and QUERY now vendor at their released tags (RM 1.1.0,
  BASE 1.2.0, AM 2.3.0, TERM 3.0.0, LANG 1.0.0, QUERY 1.1.0, ITS-REST 1.1.0,
  adjudicated against specifications.openehr.org/releases) instead of master
  development snapshots; SM, CNF and ITS-JSON have never been released and
  keep frozen SHAs that say so, and ITS-XML's docs tree keeps its frozen SHA
  because the tags carry only XSDs, which are already tag-pinned in the
  second root. Every citation in the catalogue now resolves against released
  bytes: the UML class-doc references migrated to the released trees' short
  names, and the version-lifecycle cases re-grounded on released TERM 3.0.0
  (codes 800/801) with the unreleased transition table recorded on AMB-195
  as the anticipated resolution rather than cited as the oracle.

- **Scope takes the party's own claim (#101).** The statement select is
  gone: the operator pastes the vendor's statement.json into Scope (or loads
  a committed example into the same box), and the document is held to the
  PUBLISHED statement schema — refused with the validator's finding verbatim
  — before the typed parse ever runs. Saving answers with the claim overview
  (product, claimed tiers, capability count) beside the selection preview,
  so the screen says what will run before anything starts. The accepted
  bytes are written into the job's output directory beside the ixit, and the
  verdicts judge that file — the run's own claim, never the mutable draft.

- **The E2E harness grades real CDRs side by side (#99).** An opt-in mode
  (`UI_E2E_REAL_SUTS=1`) composes FerroEHR's published quickstart and
  EHRbase's official image pairing — both at their latest published images,
  deliberately unpinned, because the SUT is the thing being graded and not a
  supply-chain input — and a new journey drives the full wizard against each,
  photographing the two records for the book's side-by-side comparison. The
  default lane stays hermetic on the in-process fixture, and the harness now
  builds and hands the console the engine binary, so a driven run works
  outside CI too.
- **The console reads the record and speaks the verdict (#67).** Results:
  the finished run's outcomes red-rows-first with the engine's own tallies,
  and a URL-addressed detail joining each outcome to its catalogue case —
  the recorded reason verbatim, the excusing citation, the failing step and
  the per-row evidence, with the attribution law stated where a red row is
  read (the wire transcript itself is #96). Verdicts: the profile matrix and
  per-capability evidence with the not_evidenced/inconclusive coverage
  bounds first-class, computed by the published lib's own judgement — the same pure
  function the CLI runs, so the rendered report, statement and certificate
  shown (and the verdicts.json beside them) are the CLI's bodies by
  construction. A statement-less run answers honestly: no claim, no verdict.
- **The console runs the campaign live (#66).** Scope's save gains "Start
  the run": the console writes the job's ixit (env-var names only, pinned by
  a test and parsed back through the published lib's own reader), spawns the
  pinned engine with `--progress`, and supervises it server-side — one run
  at a time, in memory, the artifacts landing in the mounted output
  directory exactly as a terminal run leaves them. The live screen polls the
  job: a progress bar and counter fed by the engine's own stream (degrading
  to elapsed-only on a binary predating the flag — never a fabricated
  counter), a moving-median remainder always labelled an estimate, the
  current case, the output tail verbatim, cancel (which kills the
  subprocess and says so), and the finished tally with the results path.
  Refresh rejoins the running job, because the job is server state and the
  page only polls it.
- **The catalogue speaks the CNF profile language (#87).** Case rows carry
  their tier badges (CORE / STANDARD / OPTIONS / SEC-BASIC, the capability
  matrix's own vocabulary), a `?tier=` filter narrows any chapter to one
  tier, and the chapter listing groups its cases by the same two-level
  chapter → band taxonomy the published conformance visuals render — through
  the lib's own `band_of`, never a console-side re-taxonomy. The case card
  grows to the full core: verdict-bearing capabilities beside the informative
  exercises, the applies version windows, guards, formats, the register
  option tag, and the flow or decision-table size.
- **`run --progress` (#81).** One machine-parseable stdout line per processed
  case — `progress: 0/<n>` once the selection is final, then
  `progress: <k>/<n> <case-id>` — line-flushed so a driver reading through a
  pipe sees each case as it happens. Off by default: without the flag,
  existing output is byte-identical. The lib reports the same facts as typed
  `run::Progress` events through a callback on `execute_run`, a peer of the
  warning channel, and a unit test pins the line grammar.
- **The console's run wizard, first half (#65).** Connect: the CDR base URL,
  display name and version label, the authentication choice exactly as the
  ixit's vocabulary (none, basic, bearer), and a probe-before-continue whose
  answer — status line, elapsed, or the transport's own words — renders
  verbatim, with Continue gated on 2xx and a stated "continue anyway".
  Scope: the statement picker over the mounted party declarations, the
  case-id filter, and the honest preview: N cases in scope with the
  per-chapter breakdown, held by a test to what the engine then actually
  processes. Credential values live in the server-side draft and reach only
  the spawned run's environment — the client-safe view carries no secret by
  construction, tested. The probe is the one carved-out console request to a
  CDR (a diagnostic, never a judgement), recorded in the crate's mandates.
- **The console's browser journeys and the screenshot feedback loop (#69).**
  `scripts/ui-e2e.sh` builds the console, serves it over the repository's own
  catalogue and specification mounts, starts a digest-pinned headless
  Chromium, and runs the journeys in
  `app/veredictum-console/tests/it/e2e_console.rs` — the landing's four
  counts, the sidebar's walk to a case detail and its citations, dark mode
  surviving a reload, and the routed 404. Rust-native WebDriver throughout,
  never Playwright; every journey waits on explicit conditions and ends by
  reading the browser console, failing on any error entry, hydration failure
  or client panic. With `UI_E2E_DOCS_SHOTS=1` the same journeys photograph
  each surface in one 1440×900 viewport, light and dark, into
  `website/book/src/console/img/`, which the book's new console chapter
  embeds. Two CI jobs hold it together: `console journeys` runs the harness,
  and `ui-screenshot-guard` fails a pull request that changes the console's
  `src/` or `style/` without refreshing those captures or carrying the
  `no-ui-visual-change` label.
- **The console's read surfaces (#64).** The instrument landing shows the
  catalogue's own numbers — case cores, bindings, party statements, findings
  — read once at startup through the published lib, from the same
  expressions the validate summary prints (held by a test), with the mounts
  named and a full-screen explanation when no catalogue is mounted
  (`VEREDICTUM_ROOT` / `VEREDICTUM_SPECS`; the image defaults them to the
  documented `/work` mount). The catalogue explorer walks chapters → cases →
  one case in full: the test purpose, every spec citation verbatim, the
  realizing bindings with their realized/unrealized state, and the corpus
  references — filter, search and paging all in the URL.
- **The console's shell and design system (#63).** The sidebar chrome — the
  seal, one entry per surface, the engine pin and a dark-mode toggle in the
  footer — around every routed page, in the brand palette as semantic design
  tokens (warm paper surfaces, the teal action accent, the orange reserved
  for the running state and the seal, green and red only as verdict
  semantics). The shared kits every screen composes from: page header, stat
  card, surfaces, form controls, empty state, listing table with URL-state
  pagination, toast plus inline message bar, and the verbatim pane with a
  copy affordance. Surfaces still under construction render an honest
  placeholder naming their tracker issue.
- **The signed run record (#62).** `run` and `verdicts` take `--sign-key
  <FILE>`, an armored OpenPGP secret key, with its passphrase read from
  `VEREDICTUM_SIGN_PASSPHRASE`. The emitted documents are then sealed with
  `record-manifest.json`, a byte-deterministic SHA-256 digest manifest
  carrying the instrument's name and version, and `record-manifest.json.asc`,
  a detached RFC 9580 signature over it. The bundle is ordinary files, so
  `gpg --verify record-manifest.json.asc record-manifest.json` accepts it
  without this tool. A new `verify-record --record <DIR> --key <FILE>`
  recomputes every digest the manifest names and checks the signature against
  a supplied public key, printing the signer fingerprint, the signing time and
  one line per file; a mismatch, a missing file or a rejected signature exits
  `1` naming what failed. Every verification prints what the signature does
  not establish: it proves integrity and origin since signing, and says
  nothing about the conditions the run executed under.
- **The console's engine seam (#54).** `app/veredictum-console` consumes the
  instrument as `veredictum = "=0.1.0-alpha.4"` from crates.io — never a path
  dependency — and a console-started run spawns that same pinned binary as a
  subprocess (`engine::Engine`), located on `PATH` or through
  `VEREDICTUM_ENGINE` and refused unless `--version` reports the exact pin.
  Reads parse through the published lib's typed record. SUT credentials reach
  only the spawned run's environment, redacted from every rendering. The
  byte-identity gate runs in CI: the same fixture campaign driven through the
  seam and through the CLI must emit byte-identical `results.json` and
  `run-exceptions.json` — no tolerated delta, since the record carries no
  wall-clock stamp.


- **The repository is restructured: both products live under `app/`.** The
  instrument crate moved from the repository root to `app/veredictum` and the
  root manifest is a virtual workspace (#55). Every documented invocation is
  unchanged — the data trees stayed at the root and `cargo run -- <subcommand>`
  still means the instrument. The published crate carries the same files as
  before minus 26 stragglers the old repo-root package had been picking up
  from the vendored trees (their README and LICENSE files), which were never
  part of the crate's declared contents.
- `cargo publish` names its package in both publish lanes: the workspace
  carries the unpublishable console beside the crate, and the bare form
  refused on it — the v0.1.0-alpha.4 publish leg failed on exactly that.

## [0.1.0-alpha.4] - 2026-08-27

### Added

- A committed example results document, `examples/results.example.json`,
  generated by the crate's own machinery (`cargo run --example
  make_example_results`, deterministic): schema-valid, invariant-checked,
  with a real embedded HDR V2 histogram and a verdict computed against the
  catalogue's POC case. It doubles as reader documentation for the results
  schema and as a real seed for the `party_document` and `hdr_v2` fuzz
  targets, which previously started from mutations.
- A deliberate library API, `veredictum::pipeline`, so the engine is
  consumable by something other than the command line. It carries one seam per
  whole operation — `catalogue` validates an artifact tree, `conformance`
  drives it against a running system under test, `judgement` computes the
  verdicts and renders the submission set, `assets` renders the published
  visuals and the schema set, and `measured` runs the class window, the stress
  ladder and the AQL probe. Every seam returns typed values: a validation
  carries its findings and the tree it loaded, a run carries the results
  record and its outcome tally, a judgement carries the verdict report and its
  documents as named bodies, and the measured window reports its progress as
  typed events. Nothing returns console text, so a consumer renders its own
  views over the same facts the command line prints. The `veredictum` binary
  is now a clap front end over exactly those seams; its behaviour, its output
  and its exit codes are unchanged.
- A documentation website at <https://veredictum.eu>, built from `website/` and
  deployed to GitHub Pages by a new `Docs` workflow. The root serves a
  hand-written landing page in the project's own brand palette, and `/docs/`
  serves an mdBook with five chapters: an introduction, installation, running
  the instrument, a command reference covering every subcommand with its real
  flags, the conformance method (the attribution law, positive and negative
  testing, the ambiguity-register lifecycle), and catalogue authoring. The site
  loads nothing from an external host, renders in both light and dark, and takes
  its palette from the brand tokens. `scripts/site/build.sh` assembles the same
  tree locally that the workflow deploys, the `CNAME` for the custom domain
  included. A pull request touching the site builds, lints and link-checks it
  without deploying.
- The vendored CKM ADL 1.4 archetype pack is exercised in this repository, by
  `tests/it/corpus_packs.rs` on every `cargo nextest run`. All 944 ADL 1.4
  exports are decoded as UTF-8 and required to open with an `archetype (…)`
  header declaring `adl_version=1.4` and to declare the archetype id their file
  name carries; all 944 AM 1.4 XML twins are read to end of input and required
  to root at `archetype` in `http://schemas.openehr.org/v1` with that same
  identity; both counts are pinned against the pack's own inventory record. The
  pack had no exerciser here — its only one was an ADL-engine parse gate in the
  repository this instrument was split out of, and this repository ships no ADL
  parser. The pack stays as reserve material for wire batteries the catalogue
  has not authored yet, and the exercise is at the byte level, which is what the
  instrument can perform first-hand.
- The ADL 2 pair pack and the CKM Operational Template breadth pack gain the
  same byte-level exercisers, so every vendored corpus tree in this repository
  now has one. The pair pack's 654 files are all read and refused when empty,
  its 322 ADL 2 sources are checked for `adl_version=2.0.6` and its 330 ADL 1.4
  twins for `adl_version=1.4`, each against the archetype id written inside it,
  and the 321 archetypes upstream published in both dialects are proven to pair
  with a twin in the same directory. The files that do not pair are pinned as
  what they are: one ADL 2 template, which the archetypes-only 1.4 half has
  nothing to hold, and nine ADL 1.4 archetypes this snapshot never converted.
  The template pack's 305 exports are each parsed to end of input and checked
  to root at `template` in `http://schemas.openehr.org/v1` carrying a template
  id, and its file list is compared against the record's own vendored table
  rather than against its count alone.
- A fuzzing lane over the readers that parse text or bytes the instrument did
  not write, in its own nightly `fuzz/` workspace: six libFuzzer targets
  covering the `${…}` reference and identifier grammars, the decision-table
  literal grammar, the citation reader, a case core end to end through YAML and
  the published schema into the typed model, the IXIT, statement and results
  documents a party publishes, and the HDR histogram V2 decode path a measured
  verdict is re-derived from. Seeds come from the catalogue and the party
  declarations already committed here; recorded findings live in
  `fuzz/regressions/` and are re-checked by every run. The harnesses compile on
  the pull-request path as a gating CI job, and a weekly campaign fuzzes each
  target with its corpus kept between runs. `fuzz/README.md` carries the threat
  model and the commands, `.claude/rules/fuzzing.md` the discipline and the
  crash-to-regression-test procedure.
- `veredictum::load::yaml_str_to_value` parses artifact YAML from a string under
  the same budget and duplicate-key refusal the file reader uses, and
  `veredictum::validate` exposes `citation_clauses`, `expand_braces` and
  `section_candidates`, so a consumer can read a citation the way the validator
  does.
- A published VEX record under `security/vex/`, in OpenVEX format: the
  distroless base's adjudicated OpenSSL finding as a hand-authored statement
  beside its `.trivyignore.yaml` twin, and the Rust advisories `deny.toml`
  accepts as a GENERATED document whose id set cannot drift from the gate —
  `scripts/security/vex-generate.sh` refuses on any disagreement and the CI
  guard tier regenerates and diffs on every pull request. The scheduled
  published-image scan applies the documents, and
  `scripts/security/scan-images.sh` reruns that exact scan locally.

### Changed

- **The container image is the web console now.** `ghcr.io/rubentalstra/veredictum`
  ships the new `veredictum-console` Leptos server (`app/veredictum-console`,
  a second workspace package that never publishes to crates.io) instead of the
  CLI, per the ruling recorded in `docker/Dockerfile` when the image first
  shipped: the CLI payload was a placeholder, and its no-toolchain paths are
  `cargo install veredictum` and the attested release binaries. Start the
  console with `docker run --rm -p 127.0.0.1:3000:3000 -v "$PWD:/work"
  ghcr.io/rubentalstra/veredictum:<tag>`; it binds loopback through the
  publish flag because the console has no login. The server answers
  `/healthz`, the image bakes a `HEALTHCHECK` that probes it (the binary is
  its own probe, because distroless carries no curl), and the binary drains
  in-flight requests on SIGTERM, so `docker stop` ends it gracefully. The
  image build properties are unchanged: pushed by digest, smoke-driven and
  scanned before any tag applies, SLSA provenance and an SBOM attested on
  the digest, `:latest` moving only on a release tag.
- The CKM template breadth pack is re-vendored. CKM published new asset
  versions of `ips-problem-list` and `ips-allergies-and-intolerances` on
  2026-08-19, so those two exports carry different bytes. The library is still
  305 vendored templates beside the one private-incubator template that answers
  404 without an account.

### Fixed

- Three ways a document the instrument was JUDGING could stop the instrument,
  all found by the new fuzzing lane on its first local campaign. A
  decision-table cell nesting 4000 lists deep, or chaining 4000 ordinal tuples,
  ran `Literal::from_text` off the stack; a Rust stack overflow aborts rather
  than unwinding, so a validator run died instead of reporting a finding. And a
  113-byte citation carrying 22 `{a,b}` groups in one path hint asked citation
  resolution for four million strings, hanging the run: the 32-variant ceiling
  was applied across a clause's tokens but not within one. Literal nesting is
  now bounded at `literal::MAX_NESTING` and brace expansion at
  `validate::MAX_CITATION_VARIANTS`, both refusing with a typed finding. The
  grammars' own forms are unaffected — a literal reaches three levels and an
  authored shorthand names two or three sibling documents.
- The README quoted 1107 spec-cited cases, which was the file count under
  `artifacts/schedule/`. The instrument reports 1103, because the four
  `schedule/performance/` journey definitions load as measured-workload
  definitions and are not case cores. The page now carries the number
  `validate` prints and says where that number comes from.
- Re-running `scripts/vendor/ckm-archetypes.sh` would have regressed two facts
  in the pack's `PROVENANCE.md`: the corrected mixed-licence count, and now the
  exerciser. The script emits both, so the record survives a refresh.
- The SonarQube lane no longer runs on a Dependabot pull request. `SONAR_TOKEN`
  is an Actions secret and a Dependabot run reads a separate store, so every
  such run failed on the missing secret. The lane is advisory and gates no
  merge, so skipping it costs nothing.

## [0.1.0-alpha.3] - 2026-08-26

### Fixed

- The image vulnerability gate refused to tag the `0.1.0-alpha.2` image, so that
  release published its binaries and its crate but no pullable image tag. The
  finding was real: `libssl3t64` in the distroless base, CVE-2026-14456, HIGH,
  with a Debian fix the base image has not been rebuilt against — the current
  `:nonroot` digest still carries the vulnerable version, so a base bump does not
  resolve it and a distroless image has no package manager to upgrade it in a
  layer of our own.

  It is adjudicated as unreachable rather than suppressed, on the shipped
  binary's own ELF header: its dynamic dependencies are `libgcc_s`, `libm` and
  `libc` only. TLS is rustls and the JOSE signing is aws-lc-rs, so nothing this
  project builds links OpenSSL, and the image is distroless — no shell, no
  package manager, no second executable that could load the library. The entry
  lives in a new `.trivyignore.yaml`, scoped to that one package by PURL, with
  the evidence and a three-month expiry, so it has to be re-argued rather than
  quietly becoming permanent.

### Added

- `scripts/checks/image-labels.sh`, in the ungated guard tier. The base image
  digest is declared in three places — the runtime `FROM`, the Dockerfile's
  `base.digest` label, and the release pipeline's `labels:` input, which is the
  copy the published image actually carries because it overrides the Dockerfile's
  — and an automated base bump edits only the first. Without the guard, merging
  one publishes an image whose `base.digest` names a parent it was not built on.
  It also checks that every shared OCI key agrees between the two declaration
  sites, and refuses to pass vacuously if the publishing lane it expects is
  absent.
- **`ARCHITECTURE.md`** at the repository root: the instrument's design record,
  moved here from the FerroEHR mono-repo where it was written. It is the design
  authority for the machinery — the artifact set and the case-core field
  definitions, the operation bindings, the outcome taxonomy and the ambiguity
  register, the assertion vocabulary, verdict computation — and it carries the
  population-anchored performance-class model with its journey decomposition,
  plus the evidence base and the ISO/IEC 9646 and CASCO grounding the scheme is
  built in. Names and paths were adapted to this tree; the substance is
  unchanged.
- The GHCR image-pulls badge in the README, now that the package exists.
- A Dependabot `ignore` for `rand` major bumps. The dev-dependency exists to hand
  `pgp`'s signing call an RNG, and `pgp 0.20` is on `rand_core 0.6`, so a major
  bump does not compile. Patch and minor bumps within the pin are still proposed,
  and advisory-driven updates are unaffected.

### Changed

- The container image states in its own header that its current payload is a
  placeholder: it ships the CLI today and becomes the web UI's image when that
  lands (#6). The CLI's own distribution channels are `cargo install veredictum`
  and the prebuilt binaries on each release.
- The distroless base moves to the current `:nonroot` digest
  (`sha256:a77defd6…`). This is **not** a security fix — the new digest carries
  the same `libssl3t64` version, verified by scanning it — it is base currency,
  so the image is not built on a two-month-old parent.

## [0.1.0-alpha.2] - 2026-08-26

### Added

- **The release pipeline.** A `v*` tag now publishes a release: `release.yml`
  verifies every release fact against the tagged commit before anything is
  built, creates the GitHub release as a draft, builds per-architecture Linux
  binaries (x86_64 and aarch64) each with a checksum, a CycloneDX dependency
  SBOM and Sigstore provenance and SBOM bundles, attaches a repository-wide
  SPDX SBOM, and publishes the release only once every expected asset is
  attached. The binary and image builds each live in a reusable workflow, which
  is GitHub's documented construction for SLSA Build Level 3, so a consumer can
  pin the signer with `gh attestation verify --signer-workflow`.
- The multi-architecture container image on GHCR, pushed BY DIGEST, smoke-run and
  Trivy-scanned on both architectures before any tag names it, with provenance
  and an SPDX SBOM attested on the digest. `:latest` moves on a release tag and
  never on a pre-release.
- **`docker run` needs no Rust toolchain.** `docker/Dockerfile` builds a
  distroless image that runs as uid 65532 and carries nothing but the runner:
  mount the repository at `/work` and every subcommand works, because the
  entrypoint is the instrument itself.

  ```bash
  docker run --rm -v "$PWD:/work" ghcr.io/rubentalstra/veredictum:<tag> \
      validate --root /work/artifacts --specs /work/specs/openehr
  ```

  The catalogue and the vendored specification oracle are deliberately NOT baked
  in — 347 MB, read as run-time paths, and a party may want to point at their
  own — so the image stays 55 MB and the data comes from the mount.
- A `Dockerfile lint` job in CI, gated on a change to the image tier, running
  hadolint at its warning threshold against a configuration where any
  deliberately violated rule is named with its reason.
- **The changelog guard now requires an entry**, not just a valid file shape. A
  change touching a user-visible surface with no entry under the Unreleased
  heading fails, and the path set that decides "user-visible" is declared in
  `scripts/checks/changelog-entry.sh` beside the reason for each path rather
  than inferred from a pattern in a workflow. The `no-changelog` label waives it
  and says so in the run, so a waived guard is auditable afterwards.
- Two scheduled lanes, both of which report a finding by filing or updating one
  tracking issue and keep the run green — the run goes red only when the probe
  itself cannot answer, because a red scheduled run is invisible to anyone not
  watching the Actions tab:
  - `image-scan.yml`, Mondays, Trivy over the PUBLISHED image on both
    architectures, so a CVE disclosed after a release is still found. Before the
    first release it reports that nothing is published and exits green, so
    "nothing found" and "nothing looked at" are never the same line.
  - `latest-deps.yml`, Mondays, `cargo update` then `cargo check --all-targets`,
    the Cargo book's named mitigation for a committed lockfile: a breaking
    in-range upstream release is found on a schedule instead of during an
    unrelated pull request.
- Dependabot covers the `docker` ecosystem now that a Dockerfile exists, with a
  fourteen-day cooldown — the longest of the three, because a base-image bump
  changes the bytes every user of the published image runs.
- The crate publish joins the same tag, as the last leg of the pipeline and after
  the release is otherwise complete, so the `crates-io` environment's reviewer
  approval blocks nothing else. `publish-crates.yml` stays as the out-of-band
  dry-run and recovery lane, and both lanes call one implementation,
  `scripts/release/publish-crate.sh`.
- The release procedure is written into `CLAUDE.md` and driven by this file: a
  missing or empty section for the tagged version fails the pipeline's `plan` job
  before anything is published.
- The crates.io version, crate-downloads and docs.rs badges in the README.
- **Published on crates.io** as `veredictum`, both a binary and a library:
  `cargo install veredictum --version 0.1.0-alpha.2` puts the command on your
  `PATH`, and the library target lets an integrator consume the typed artifact
  model and the published JSON Schemas rather than reimplementing the format.
  The package carries the code and the legal set; the catalogue and the vendored
  specification oracle are 347 MB of data no registry accepts, and every root is
  a path passed at run time, so both come from the repository.
- `publish-crates.yml`: the release lane for the crate, authenticating through
  crates.io Trusted Publishing so no long-lived registry token exists in this
  repository. Manual dispatch, dry run by default, the upload built from the
  checkout with no cache restored, and the registry read back before the lane
  reports success.
- **The instrument itself builds and runs from this repository:** the runner,
  the catalogue with its 1107 case cores and 247 operation bindings, the
  corpora, the ambiguity register, the party declarations, and the vendored
  openEHR specification text that is its oracle.
- The command is `veredictum`. The package, the binary and the library carry the
  product's name, and so does the debug switch, now
  `VEREDICTUM_DEBUG_EXCHANGES`. Every subcommand keeps its name and its flags:
  `validate`, `run`, `verdicts`, `perf`, `stress`, `aql-probe`,
  `stress-compare`, `perf-assets`, `conformance-assets`, `emit-schemas`. Two
  paths move with the tree — an artifact root is now `artifacts` and a spec root
  is now `specs/openehr`.
- The standalone workspace: one package at the root, its own SemVer line from
  `0.1.0-alpha.1`, edition 2024, Apache-2.0, with the deny-tier lint tables,
  `rust-toolchain.toml`, `rustfmt.toml`, `clippy.toml`, `deny.toml` and
  `.config/nextest.toml` carried over and adapted to what this tree actually
  contains. `Cargo.lock` is committed, because this repository ships a binary.
- Three released machine-readable bundles beside the specification text, so a
  citation that can only resolve against a schema resolves here rather than
  nowhere: `specs/its-xml-schemas/` (the two XSD lineages),
  `specs/its-json-schemas/` (the ITS-JSON validation oracle) and
  `specs/rest-oas/` (the 21 released ITS-REST OpenAPI bundles).
- The corpus vendoring scripts, so every vendored tree can still be refreshed
  the only sanctioned way, by re-running its script:
  `scripts/vendor/ckm-templates.sh`, `scripts/vendor/ckm-archetypes.sh`,
  `scripts/vendor/adl2-archetypes.sh` and `scripts/generate-ckm-examples.sh`.
- The Rust CI tier, gated on whether a change touches anything it reads:
  `rustfmt`, `clippy --all-targets` at `-D warnings`, build plus `nextest` plus
  the instrument's own `validate` self-check, the rustdoc gate, the declared
  MSRV verified with `cargo hack check --rust-version`, `cargo deny check`, and
  `cargo machete` for dependencies nothing imports. All seven join the single
  required `conclusion` check.
- CodeQL analyzes `rust` beside `actions`, and the SonarQube scope covers `src/`
  and `tests/` with the vendored trees excluded.
- Continuous integration. `ci.yml` runs on every pull request, every push to
  `main` and every merge-queue entry: a guard tier (comment style, changelog
  structure, the no-attribution scan over the pushed commits, REUSE 3.3
  licensing), a workflow audit (zizmor for the security posture, actionlint with
  bundled shellcheck for correctness, and a check that every job actually gates
  the merge), and a single required `conclusion` check.
- `scorecard.yml`: the weekly OpenSSF Scorecard analysis, publishing its score
  to the OpenSSF API and its findings into code scanning.
- `sonar.yml` and `sonar-project.properties`: SonarQube Cloud analysis on every
  pull request and every push to `main`, advisory under
  `.claude/rules/ai-code-review.md`, with the New Code window anchored to the
  package version so "new code" means "since the last release".
- Test coverage, measured and published. The Sonar lane runs the suite under
  `cargo-llvm-cov` and imports the merged lcov; the denominator excludes the
  test tree, the CLI entry point and the two asset renderers, each with its
  reason recorded, because a coverage percentage is only useful if every file
  counted could in principle be covered by a test. The README carries the
  coverage and quality-gate badges beside the CI, CodeQL, reliability, security,
  maintainability and duplication readings.
- Dependabot covers the `cargo` ecosystem, with a seven-day cooldown against
  the actions entry's three: a crate compiles into the published binary, so a
  compromised release reaches every downstream run rather than one CI job.
- The OpenSSF Best Practices and OpenSSF Scorecard badges in the README, both
  reading live scores rather than asserting a posture.
- Two ported guard scripts: `scripts/checks/changelog-structure.sh` (Keep a
  Changelog structure) and `scripts/checks/ci-conclusion-complete.sh` (no CI job
  runs without gating the merge).
- The tracker machinery. `scripts/gh/rel.sh` is the one sanctioned write path
  for GitHub's four native issue edges — sub-issue, blocked-by and their
  inverses — resolving an issue number to the database id the write endpoints
  actually want and failing loud on a bad one, with
  `.claude/rules/issue-relationships.md` as its policy. The label taxonomy is
  complete against the scheme `CLAUDE.md` defines: `blocked-upstream`,
  `on-hold`, `no-changelog`, and the eight `spec:` component labels join the
  type and priority sets. Two milestones open the release spine, `v0.0.1` and
  `v0.1.0`. `/phase-status`, `/next-task` and `/phase-done` are ported and
  trimmed to the machinery that exists, each naming what it deliberately does
  not check.

### Removed

- The `accessibility` label. Nothing referenced it and it is not part of the
  taxonomy `CLAUDE.md` defines.

### Fixed

- The SonarQube Cloud lane, which had failed on every run since the code
  migration, the push to `main` included. `sonar.sources=.` and
  `sonar.tests=tests` overlapped, and the scanner refuses an overlap rather than
  picking a side, so one YAML fixture under `tests/fixtures/` ended the analysis
  at exit code 3 — leaving the quality gate and the coverage badge with no
  current reading at all.
- The changelog's own intro paragraph, which the v0.0.1-alpha.1 cut turned into
  a stray release heading by rewriting the first literal `## [Unreleased]` it
  found — which was in prose, not the heading. The v0.0.1-alpha.1 section now
  exists as a real section, and the paragraph names the heading instead of
  quoting it.
- `SUPPORT.md` said GitHub Discussions was not enabled. It is, with six
  categories.

## [0.0.1-alpha.1] - 2026-08-26

### Added

- The repository's working discipline: the root `CLAUDE.md`, the rule files
  under `.claude/rules/`, the guard hooks under `.claude/hooks/`, the agent
  definitions under `.claude/agents/`, the in-repo memory under
  `.claude/memory/`, and the comment-style guard under `scripts/checks/`.
- The product identity: the README, the origin of the name, and the pointer to
  the migration contract.
- The repository skeleton a public project is read by: `CONTRIBUTING.md`,
  `CODE_OF_CONDUCT.md`, `GOVERNANCE.md`, `MAINTAINERS.md`, `SECURITY.md`,
  `SUPPORT.md`, `AI_STATEMENT.md`, the `.github/` collaboration surface (issue
  forms, pull-request template, `CODEOWNERS`, `FUNDING.yml`, and a
  `dependabot.yml` covering the github-actions ecosystem), the `REUSE.toml`
  licensing declaration with `LICENSES/Apache-2.0.txt`, the
  attribution-stripping `commit-msg` hook with `scripts/install-hooks.sh`, and
  the Rust `.gitignore` set.

[unreleased]: https://github.com/rubentalstra/Veredictum/compare/v0.1.4...HEAD
[0.1.4]: https://github.com/rubentalstra/Veredictum/compare/v0.1.3...v0.1.4
[0.1.3]: https://github.com/rubentalstra/Veredictum/compare/v0.1.2...v0.1.3
[0.1.2]: https://github.com/rubentalstra/Veredictum/compare/v0.1.1...v0.1.2
[0.1.1]: https://github.com/rubentalstra/Veredictum/compare/v0.1.0...v0.1.1
[0.1.0]: https://github.com/rubentalstra/Veredictum/compare/v0.1.0-alpha.6...v0.1.0
[0.1.0-alpha.6]: https://github.com/rubentalstra/Veredictum/compare/v0.1.0-alpha.5...v0.1.0-alpha.6
[0.1.0-alpha.5]: https://github.com/rubentalstra/Veredictum/compare/v0.1.0-alpha.4...v0.1.0-alpha.5
[0.1.0-alpha.4]: https://github.com/rubentalstra/Veredictum/compare/v0.1.0-alpha.3...v0.1.0-alpha.4
[0.1.0-alpha.3]: https://github.com/rubentalstra/Veredictum/compare/v0.1.0-alpha.2...v0.1.0-alpha.3
[0.1.0-alpha.2]: https://github.com/rubentalstra/Veredictum/compare/v0.0.1-alpha.1...v0.1.0-alpha.2
[0.0.1-alpha.1]: https://github.com/rubentalstra/Veredictum/releases/tag/v0.0.1-alpha.1
