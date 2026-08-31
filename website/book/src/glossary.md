# Glossary

<!-- toc -->

The conformance vocabulary comes from two places: the ISO conformance-testing
tradition (ICS, IXIT, SDoC) and the openEHR specifications themselves (RM,
AQL, the component names). This page defines every term the instrument and
the console put in front of you.

## The parties and the claim

- **Party:** the vendor whose product is being graded and who makes the
  conformance claim. The two documents below are theirs to write and travel
  with the submission they describe; this repository commits neither.
- **ICS (Implementation Conformance Statement):** the party's claim document,
  `statement.json`. It declares which profile tiers and which capabilities
  the product claims. A verdict certifies this claim against the recorded
  evidence, so a run without a statement produces results but no verdict.
- **ICS proforma:** the blank form an ICS fills in,
  `artifacts/vocab/capability_matrix.yaml`. ISO/IEC 9646-7 assigns every cell
  of it to the test specification's author except the support and
  supported-values columns, which belong to the supplier of the
  implementation. So the proforma is published here, one row per capability
  with its specification citation, and the answers never are.
  The term comes from the ISO/IEC 9646 conformance-testing methodology.
- **IXIT (Implementation eXtra Information for Testing):** the deployment
  facts needed to drive the claim, `ixit.json`: the endpoint URLs, the
  authentication mode, the names of the credential environment variables.
  Same ISO/IEC 9646 origin. The console writes one for you from the Connect
  form.
- **SDoC (Supplier's Declaration of Conformity):** the self-declaration half
  of the statement, in the sense of ISO/IEC 17050: the supplier declares,
  the instrument checks, and nobody grades their own homework.
- **SUT (System Under Test):** the running server a campaign drives, here
  always a CDR reached over its own REST wire.
- **CDR (Clinical Data Repository):** a server that stores and serves
  openEHR clinical data, such as EHRbase or FerroEHR.

## The catalogue and the run

- **Catalogue:** the machine-readable test schedule under `artifacts/`:
  case cores, operation bindings, vocabulary, corpora, and the ambiguity
  register. `validate` holds it to zero findings before any server is
  composed.
- **Case:** one spec-cited behaviour, one file, one row in the record. One
  behaviour per case is a design rule: a red row then names exactly one
  defect.
- **Case id:** the stable identifier of a case, interface first, such as
  `I_EHR_SERVICE.create_ehr-main`.
- **Case-id filter:** a plain substring match over case ids, the engine's
  `--filter` flag. Every case whose id contains the typed text is in scope;
  an empty filter means the whole catalogue. `I_EHR_SERVICE.` selects the
  EHR-service family, `create_ehr-main` selects one case. The filter narrows
  the run, never the claim: a claimed capability whose cases were filtered
  out is reported `not_evidenced`, so a narrow run cannot pose as full
  coverage.
- **Corpus:** the committed wire payloads the cases post, valid and invalid.
  An invalid entry is deliberate: it pins a refusal the spec requires, so a
  lenient server fails it.
- **Excused:** a case the run did not hold against the server, with the
  citation that permits the excuse: an out-of-claim capability, or an instance
  the party does not declare. Excuses are printed, never silent.
- **Ambiguity register:** the record of behaviours where the released
  specifications are genuinely silent or contradictory, each with a typed
  disposition and, where one exists, its upstream report. Silence goes here;
  it is never resolved privately.

## The verdict

- **Verdict:** the computed answer to the claim, produced by a pure function
  over the statement, the record, the catalogue and the capability matrix.
  It is computed, never asserted, and re-running it over the same record
  produces the same bytes.
- **Profile tier:** one of the CNF profile levels a party can claim: **CORE**,
  **STANDARD**, **OPTIONS**, and the **SEC-BASIC** security family. A tier
  passes only when every capability it requires is `passed`.
- **Capability:** one named unit of claimable behaviour, such as
  `EhrOperations` or `QueryProvisioning`. The capability matrix maps each
  tier to the capabilities it requires, and each capability to the cases
  that evidence it.
- **Evidence tokens:** the per-capability answer in the matrix. `passed`
  (every selected gating case passed), `failed` (one failed),
  `inconclusive` (one errored and none failed; never counted against the
  server, but blocking green), `not_evidenced` (claimed, and no case
  produced a gating pass or fail), `not_claimed` (absent from the party's
  ICS).
- **Errored (inconclusive):** a row whose exchange could not be judged: a
  status mapping to no declared outcome, a transport fault. By the
  attribution law this is never a server failure; it is a defect in the
  runner or the catalogue until adjudicated.

## The authorities

- **Oracle:** the released openEHR specification text vendored under
  `specs/`. It is the only authority an expectation may cite, and it is
  never a suspect when a run goes red.
- **CNF:** openEHR's conformance specification component. Its Platform
  Conformance Test Schedule names which behaviours to cover; the released
  components say what the correct answer is.
- **Spec components:** **RM** (the Reference Model: the data structures),
  **AQL** (the Archetype Query Language), **AM** (the Archetype Model),
  **BASE** (foundation types and identifiers), **TERM** (terminology),
  **SM** (the Service Model: the abstract operations), **ITS** (the
  Implementation Technology Specifications for REST, JSON and XML: how the
  operations land on a wire).
- **Attribution law:** the discipline applied to every red row before
  anything changes. The failure belongs to exactly one of three suspects: the
  server, the runner, or the catalogue. Which one is decided by comparing
  spec-required against catalogue-expected against observed, with the cited
  text as the reference.
