// SPDX-FileCopyrightText: Veredictum contributors
// SPDX-License-Identifier: Apache-2.0

//! The three retrieval cases whose own binding negotiates a non-JSON form,
//! driven against the fake SUT over the COMMITTED corpus bytes.
//!
//! `I_DEFINITION_ADL14.get_opt` sends `Accept: application/xml` and its
//! fixtures are `opt-xml`; `I_DEFINITION_ADL2.get_artefact` sends
//! `Accept: text/plain` and its fixture is `adl2-text`. Both sides of each
//! `equivalent` assertion are therefore in one document form, and the register
//! pins what the comparison means: retrieval is VERBATIM, "the ADL 1.4 OPT as
//! the canonical XML document the client sent, the ADL2 artefact as the source
//! text it sent" (`artifacts/registers/ambiguities.yaml` AMB-111, disposition
//! `fixed_handling`).
//!
//! Every test here drives the REAL case core, the REAL binding and the REAL
//! fixture bytes: a served fixture judges EQUAL, a served mutation judges
//! UNEQUAL as a finding against the SUT, and a form the comparator cannot
//! judge stays in the inconclusive channel.

use std::path::PathBuf;

use veredictum::artifacts::{ArtifactSet, load_root};
use veredictum::exec::StepDriver;
use veredictum::exec::assertions::AssertionOutcome;
use veredictum::exec::driver::HttpDriver;
use veredictum::exec::state::{Captured, VarStore};
use veredictum::ids::{CaptureName, CaseId};
use veredictum::model::case::{CaseCore, FlowStep};
use veredictum::vocab::OutcomeKind;
use wiremock::matchers::{method, path};
use wiremock::{Mock, ResponseTemplate};

use crate::fake_sut::{FakeSut, ixit};

/// Anything a driver construction or a step can fail with, so a test body
/// propagates plumbing failures with `?`
/// (<https://doc.rust-lang.org/book/ch11-01-writing-tests.html>).
type Fallible = Result<(), Box<dyn std::error::Error>>;

const XML: &str = "application/xml";
const TEXT: &str = "text/plain";

/// The committed catalogue root.
fn artifacts_root() -> PathBuf {
    PathBuf::from(concat!(env!("CARGO_MANIFEST_DIR"), "/../../artifacts"))
}

/// The committed catalogue, loaded exactly as a run loads it.
fn catalogue() -> Result<ArtifactSet, Box<dyn std::error::Error>> {
    let loaded = load_root(&artifacts_root())?;
    if let Some(error) = loaded.errors.first() {
        return Err(Box::new(std::io::Error::other(error.to_string())));
    }
    Ok(loaded.set)
}

/// One committed case core, by id.
fn case_core(set: &ArtifactSet, id: &str) -> Result<CaseCore, Box<dyn std::error::Error>> {
    let wanted = CaseId::parse(id)?;
    set.cases
        .iter()
        .find(|(_, core)| core.id == wanted)
        .map(|(_, core)| core.clone())
        .ok_or_else(|| format!("the catalogue carries no case {id}").into())
}

/// One committed corpus fixture's bytes, read off disk the way the resolver
/// reads them.
fn fixture_bytes(source: &str) -> Result<String, Box<dyn std::error::Error>> {
    Ok(std::fs::read_to_string(artifacts_root().join(source))?)
}

/// The step a single-step retrieval case declares.
fn only_step(case: &CaseCore) -> Result<&FlowStep, Box<dyn std::error::Error>> {
    case.flow
        .first()
        .ok_or_else(|| "the case declares no flow step".into())
}

/// Answer one GET with the given body under the given media type, plus the
/// weak `ETag` the binding's header matcher requires.
fn mount_get(sut: &FakeSut, route: &str, body: &str, media_type: &str, etag: &str) {
    sut.mount(
        Mock::given(method("GET"))
            .and(path(route.to_owned()))
            .respond_with(
                ResponseTemplate::new(200)
                    .insert_header("ETag", format!("W/\"{etag}\"").as_str())
                    .set_body_raw(body.as_bytes().to_vec(), media_type),
            ),
    );
}

/// Drive the case's only step against the fake SUT and return its channelled
/// assertion outcomes.
fn drive(
    sut: &FakeSut,
    set: &ArtifactSet,
    case: &CaseCore,
    vars: &mut VarStore,
) -> Result<Vec<AssertionOutcome>, Box<dyn std::error::Error>> {
    let topology = ixit(&sut.base_url());
    let step = only_step(case)?;
    let mut driver = HttpDriver::new(set, &topology, None)?;
    let observed = driver.perform(case, step, OutcomeKind::Ok, 0, vars)?;
    Ok(observed.assertion_failures)
}

/// The `template_id` the `ETag` matcher reads back off the case variable of the
/// same name.
fn with_template_id(id: &str) -> Result<VarStore, Box<dyn std::error::Error>> {
    let mut vars = VarStore::default();
    vars.set(
        CaptureName::parse("template_id")?,
        Captured::Scalar(id.to_owned()),
    );
    Ok(vars)
}

/// The one failure a row carries, or an error naming the silence.
fn only_failure(
    failures: &[AssertionOutcome],
) -> Result<&AssertionOutcome, Box<dyn std::error::Error>> {
    failures
        .first()
        .ok_or_else(|| "the row carried no assertion failure at all".into())
}

/// `get_opt-retrieve_single` JUDGES the served OPT: the fixture served back
/// verbatim is equivalent, so the row passes on a server that answers the
/// negotiated `application/xml`.
#[test]
#[expect(
    clippy::panic_in_result_fn,
    reason = "the Book's Result-test shape: assertions panic, plumbing propagates with `?`"
)]
fn the_adl14_retrieval_judges_a_served_opt_equivalent() -> Fallible {
    let set = catalogue()?;
    let case = case_core(&set, "I_DEFINITION_ADL14.get_opt-retrieve_single")?;
    let opt = fixture_bytes("corpus/fixtures/opt/valid/minimal_all_entries.opt")?;
    let sut = FakeSut::start();
    mount_get(
        &sut,
        "/definition/template/adl1.4/obs_act.en.v1",
        &opt,
        XML,
        "obs_act.en.v1",
    );
    let mut vars = with_template_id("obs_act.en.v1")?;
    let failures = drive(&sut, &set, &case, &mut vars)?;
    assert!(
        failures.is_empty(),
        "the served OPT is the uploaded one, so nothing may fail: {failures:?}"
    );
    Ok(())
}

/// The same case GATES: an OPT served with one changed attribute value is a
/// finding against the SUT, because AMB-111 pins retrieval as verbatim.
#[test]
#[expect(
    clippy::panic_in_result_fn,
    reason = "the Book's Result-test shape: assertions panic, plumbing propagates with `?`"
)]
fn the_adl14_retrieval_gates_a_mutated_opt() -> Fallible {
    let set = catalogue()?;
    let case = case_core(&set, "I_DEFINITION_ADL14.get_opt-retrieve_single")?;
    let opt = fixture_bytes("corpus/fixtures/opt/valid/minimal_all_entries.opt")?;
    let mutated = opt.replacen("obs_act.en.v1", "obs_act.en.v9", 1);
    assert_ne!(mutated, opt, "the mutation changed nothing");
    let sut = FakeSut::start();
    mount_get(
        &sut,
        "/definition/template/adl1.4/obs_act.en.v1",
        &mutated,
        XML,
        "obs_act.en.v1",
    );
    let mut vars = with_template_id("obs_act.en.v1")?;
    let failures = drive(&sut, &set, &case, &mut vars)?;
    let first = only_failure(&failures)?;
    assert!(
        matches!(first, AssertionOutcome::Mismatch(_)),
        "a changed OPT is a fact about the SUT, not the instrument's limit: {first:?}"
    );
    assert!(
        first.reason().starts_with("equivalent:"),
        "the finding names another family: {}",
        first.reason()
    );
    Ok(())
}

/// A versioned retrieval judges the version it addressed: `test_versioned.en.v1`
/// served as `v2` is unequal, which is the fidelity half of
/// `get_opt-retrieve_specific_version` (AMB-111).
#[test]
#[expect(
    clippy::panic_in_result_fn,
    reason = "the Book's Result-test shape: assertions panic, plumbing propagates with `?`"
)]
fn the_versioned_retrieval_judges_the_version_it_addressed() -> Fallible {
    let set = catalogue()?;
    let case = case_core(&set, "I_DEFINITION_ADL14.get_opt-retrieve_specific_version")?;
    let v1 = fixture_bytes("corpus/fixtures/opt/valid/versioned.v1.opt")?;
    let v2 = fixture_bytes("corpus/fixtures/opt/valid/versioned.v2.opt")?;
    let route = "/definition/template/adl1.4/test_versioned.en.v1";

    let equal = FakeSut::start();
    mount_get(&equal, route, &v1, XML, "test_versioned.en.v1");
    let mut vars = with_template_id("test_versioned.en.v1")?;
    let failures = drive(&equal, &set, &case, &mut vars)?;
    assert!(
        failures.is_empty(),
        "the addressed version served back verbatim must pass: {failures:?}"
    );

    let wrong = FakeSut::start();
    mount_get(&wrong, route, &v2, XML, "test_versioned.en.v1");
    let mut vars = with_template_id("test_versioned.en.v1")?;
    let failures = drive(&wrong, &set, &case, &mut vars)?;
    let first = only_failure(&failures)?;
    assert!(
        matches!(first, AssertionOutcome::Mismatch(_)),
        "serving the wrong version is a finding against the SUT: {first:?}"
    );
    Ok(())
}

/// `get_artefact-retrieve` JUDGES the served ADL2 source, and tolerates only
/// the line-break spelling HTTP grants (RFC 7231 Appendix A.2).
#[test]
#[expect(
    clippy::panic_in_result_fn,
    reason = "the Book's Result-test shape: assertions panic, plumbing propagates with `?`"
)]
fn the_adl2_retrieval_judges_served_source_text() -> Fallible {
    let set = catalogue()?;
    let case = case_core(&set, "I_DEFINITION_ADL2.get_artefact-retrieve")?;
    let source = fixture_bytes("corpus/fixtures/adl2/opt/minimal.adls")?;
    let hrid = "openEHR-EHR-COMPOSITION.cnf_minimal.v1.0.0";
    let route = format!("/definition/template/adl2/{hrid}");

    let served_with_crlf = source.replace('\n', "\r\n");
    let sut = FakeSut::start();
    mount_get(&sut, &route, &served_with_crlf, TEXT, hrid);
    let mut vars = with_template_id(hrid)?;
    let failures = drive(&sut, &set, &case, &mut vars)?;
    assert!(
        failures.is_empty(),
        "only the line-break spelling changed, which HTTP grants: {failures:?}"
    );

    let mutated = source.replacen("cnf_minimal", "cnf_other", 1);
    assert_ne!(mutated, source, "the mutation changed nothing");
    let wrong = FakeSut::start();
    mount_get(&wrong, &route, &mutated, TEXT, hrid);
    let mut vars = with_template_id(hrid)?;
    let failures = drive(&wrong, &set, &case, &mut vars)?;
    let first = only_failure(&failures)?;
    assert!(
        matches!(first, AssertionOutcome::Mismatch(_)),
        "a changed artefact is a fact about the SUT: {first:?}"
    );
    assert!(
        first.reason().contains("line"),
        "the finding names no line: {}",
        first.reason()
    );
    Ok(())
}

/// The FORM has to agree on both sides. An XML fixture against a `text/plain`
/// body is not a comparison anyone can make, so the row stays INCONCLUSIVE
/// and the refusal names both sides — never a pass, and never a finding
/// against the server.
#[test]
#[expect(
    clippy::panic_in_result_fn,
    reason = "the Book's Result-test shape: assertions panic, plumbing propagates with `?`"
)]
fn a_form_mismatch_stays_in_the_inconclusive_channel() -> Fallible {
    let set = catalogue()?;
    let case = case_core(&set, "I_DEFINITION_ADL14.get_opt-retrieve_single")?;
    let opt = fixture_bytes("corpus/fixtures/opt/valid/minimal_all_entries.opt")?;
    let sut = FakeSut::start();
    mount_get(
        &sut,
        "/definition/template/adl1.4/obs_act.en.v1",
        &opt,
        TEXT,
        "obs_act.en.v1",
    );
    let mut vars = with_template_id("obs_act.en.v1")?;
    let failures = drive(&sut, &set, &case, &mut vars)?;
    let first = only_failure(&failures)?;
    assert!(
        matches!(first, AssertionOutcome::Unjudgeable(_)),
        "a form disagreement is the instrument's limit, never a finding: {first:?}"
    );
    let reason = first.reason();
    assert!(reason.contains("opt-xml"), "no fixture format: {reason}");
    assert!(reason.contains("plain text"), "no served form: {reason}");
    Ok(())
}

/// A media type this runner reads as neither form keeps the standing refusal,
/// with the sentence the member-addressing families share.
#[test]
#[expect(
    clippy::panic_in_result_fn,
    reason = "the Book's Result-test shape: assertions panic, plumbing propagates with `?`"
)]
fn an_unreadable_media_type_keeps_the_standing_refusal() -> Fallible {
    let set = catalogue()?;
    let case = case_core(&set, "I_DEFINITION_ADL14.get_opt-retrieve_single")?;
    let sut = FakeSut::start();
    mount_get(
        &sut,
        "/definition/template/adl1.4/obs_act.en.v1",
        "\u{fffd}\u{1}not a document",
        "application/octet-stream",
        "obs_act.en.v1",
    );
    let mut vars = with_template_id("obs_act.en.v1")?;
    let failures = drive(&sut, &set, &case, &mut vars)?;
    let first = only_failure(&failures)?;
    assert!(
        matches!(first, AssertionOutcome::Unjudgeable(_)),
        "an unreadable form is the instrument's limit: {first:?}"
    );
    assert!(
        first.reason().contains("application/octet-stream"),
        "the refusal names no media type: {}",
        first.reason()
    );
    Ok(())
}

/// A served document that is not well-formed XML is a finding against the
/// SUT, and an ILL-FORMED FIXTURE is not: the two sides of the comparison
/// keep separate channels.
#[test]
#[expect(
    clippy::panic_in_result_fn,
    reason = "the Book's Result-test shape: assertions panic, plumbing propagates with `?`"
)]
fn an_ill_formed_served_document_is_a_finding() -> Fallible {
    let set = catalogue()?;
    let case = case_core(&set, "I_DEFINITION_ADL14.get_opt-retrieve_single")?;
    let opt = fixture_bytes("corpus/fixtures/opt/valid/minimal_all_entries.opt")?;
    let truncated: String = opt.chars().take(200).collect();
    let sut = FakeSut::start();
    mount_get(
        &sut,
        "/definition/template/adl1.4/obs_act.en.v1",
        &truncated,
        XML,
        "obs_act.en.v1",
    );
    let mut vars = with_template_id("obs_act.en.v1")?;
    let failures = drive(&sut, &set, &case, &mut vars)?;
    let first = only_failure(&failures)?;
    assert!(
        matches!(first, AssertionOutcome::Mismatch(_)),
        "a truncated served document is a fact about the SUT: {first:?}"
    );
    assert!(
        first.reason().contains("served body"),
        "the finding does not name the served side: {}",
        first.reason()
    );
    Ok(())
}

/// Every corpus entry the three cases name is still declared in the form this
/// comparison rests on. A manifest edit that flipped one to a JSON format
/// would silently put these rows back in the inconclusive channel.
#[test]
fn the_three_cases_still_rest_on_non_json_fixtures() {
    let set = catalogue().expect("the committed catalogue loads");
    let expected: &[(&str, &str)] = &[
        ("cnf.opt.minimal_all_entries", "opt-xml"),
        ("cnf.opt.versioned.v1", "opt-xml"),
        ("cnf.adl2.opt.minimal", "adl2-text"),
    ];
    let (_, manifest) = set.corpus.as_ref().expect("the corpus manifest loads");
    for (key, format) in expected {
        let parsed = veredictum::ids::CorpusKey::parse(key).expect("a committed corpus key");
        let entry = manifest
            .get(&parsed)
            .unwrap_or_else(|| panic!("the manifest declares no entry {key}"));
        assert_eq!(entry.format.token(), *format, "{key} changed form");
    }
}
