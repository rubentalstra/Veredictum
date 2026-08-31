// SPDX-FileCopyrightText: Veredictum contributors
// SPDX-License-Identifier: Apache-2.0

//! Re-judging a recorded run from its own transcript, and refusing a record
//! the recorded exchanges do not support (#392).
//!
//! The property a published `console` entry rests on: its judgement is
//! arithmetic anybody can repeat. Here the same campaign is driven twice —
//! once against a fake SUT with the wire recorded, once with that recording
//! standing in for the server — and the second pass has to reach the first
//! pass's outcomes through the same composition, classification and assertion
//! evaluators.
//!
//! What that does NOT establish is the evidence itself: a transcript is what
//! the instrument says it sent and received. The tamper case below is about
//! the record, not the recording.

use serde_json::{Value, json};
use veredictum::party::{OutcomeRecord, OutcomeStatus, Results};
use veredictum::pipeline::replay::divergences;
use veredictum::run::{RunReport, execute, replay};
use veredictum::transcript::{Recording, RunTranscript};
use wiremock::matchers::{method, path};
use wiremock::{Mock, ResponseTemplate};

use crate::fake_sut::{FakeSut, artifact_set, case, ixit};

/// Anything a run or a fixture can fail with, so a test body propagates
/// plumbing failures with `?`
/// (<https://doc.rust-lang.org/book/ch11-01-writing-tests.html>).
type Fallible = Result<(), Box<dyn std::error::Error>>;

/// `POST /ehr` — the one driven wire in these campaigns.
fn create_ehr_binding() -> Value {
    json!({
        "sm_operation": "I_EHR_SERVICE.create_ehr",
        "its": "its-rest",
        "request": { "method": "POST", "path": "/ehr" },
        "outcomes": { "created": { "status": 201 } }
    })
}

fn driven_case() -> Value {
    json!({
        "id": "REDERIVE-create_ehr", "kind": "functional", "component": "EHR",
        "sm_operation": "I_EHR_SERVICE.create_ehr",
        "test_purpose": "t", "description": "d", "spec_refs": [],
        "flow": [{ "step": 1, "call": "create_ehr", "expect": "created" }]
    })
}

/// The results document a report stands for, as the record surfaces build it.
fn results_of(report: &RunReport) -> Results {
    Results {
        sut: veredictum::party::Sut {
            name: String::from("rederivation-gate"),
            version: String::from("0.0.0-gate"),
        },
        runner: veredictum::party::Runner {
            name: String::from("veredictum"),
            version: String::from("0"),
            verification_pack_status: veredictum::party::VerificationPackStatus::Passed,
        },
        schedule_release: String::from("cnf-2.0-w2"),
        tech_profile: veredictum::party::TechProfile {
            its: veredictum::vocab::ItsName::ItsRest,
            formats: veredictum::vocab::FormatName::ALL.to_vec(),
        },
        ixit_digest: String::from("0"),
        selection_basis: Some(veredictum::party::SelectionBasis::StatementBlind),
        restapi_specs_version: None,
        outcomes: report.records.iter().map(OutcomeRecord::from).collect(),
        measurements: Vec::new(),
        ambiguity_dispositions: Vec::new(),
    }
}

/// The transcript a recorded report stands for.
fn transcript_of(report: &RunReport) -> RunTranscript {
    RunTranscript {
        sut: veredictum::party::Sut {
            name: String::from("rederivation-gate"),
            version: String::from("0.0.0-gate"),
        },
        schedule_release: String::from("cnf-2.0-w2"),
        cases: report.transcripts.clone(),
    }
}

/// Drives the one case against a SUT answering `201`, with the wire recorded.
fn recorded_campaign(sut: &FakeSut) -> Result<RunReport, Box<dyn std::error::Error>> {
    let mut set = artifact_set(&[create_ehr_binding()]);
    set.cases
        .push((std::path::PathBuf::from("c.yaml"), case(driven_case())));
    let report = execute(
        &set,
        &ixit(&sut.base_url()),
        None,
        Recording::On,
        &mut |_| {},
    )?;
    Ok(report)
}

/// A recorded run re-judges to the outcomes it recorded: the replay reaches
/// the same status over the same rows, with no server involved at all.
#[test]
#[expect(
    clippy::panic_in_result_fn,
    reason = "the Book's Result-test shape: assertions panic, plumbing propagates with `?`"
)]
fn a_recorded_run_re_judges_to_the_outcomes_it_recorded() -> Fallible {
    let sut = FakeSut::start();
    sut.mount(
        Mock::given(method("POST"))
            .and(path("/ehr"))
            .respond_with(ResponseTemplate::new(201)),
    );
    let live = recorded_campaign(&sut)?;
    assert_eq!(live.records.len(), 1, "the campaign drove one case");
    let transcript = transcript_of(&live);
    assert_eq!(transcript.exchange_count(), 1, "one exchange was recorded");

    // The replay is built over the same catalogue and the same ixit, and the
    // fake SUT is dropped first: nothing here can reach a socket.
    let mut set = artifact_set(&[create_ehr_binding()]);
    set.cases
        .push((std::path::PathBuf::from("c.yaml"), case(driven_case())));
    let base_url = sut.base_url();
    drop(sut);
    let again = replay(&set, &ixit(&base_url), None, &transcript, &mut |_| {})?;

    let submitted = results_of(&live);
    let rederived = results_of(&again);
    assert_eq!(
        submitted.outcomes.first().map(|o| o.status),
        Some(OutcomeStatus::Passed),
        "the fixture answers the expected status, so the live row passes"
    );
    assert_eq!(
        divergences(&submitted, &rederived),
        Vec::new(),
        "a faithful recording re-judges to what it recorded"
    );
    Ok(())
}

/// A record altered after the run no longer follows from the recording, and
/// the comparison names the row. This is the gate a submission passes through
/// before anything is signed.
#[test]
#[expect(
    clippy::panic_in_result_fn,
    reason = "the Book's Result-test shape: assertions panic, plumbing propagates with `?`"
)]
fn a_record_altered_after_the_run_is_refused_by_the_re_judgement() -> Fallible {
    let sut = FakeSut::start();
    // The server refuses the operation, so the honest row FAILS.
    sut.mount(
        Mock::given(method("POST"))
            .and(path("/ehr"))
            .respond_with(ResponseTemplate::new(500)),
    );
    let live = recorded_campaign(&sut)?;
    let transcript = transcript_of(&live);

    let mut set = artifact_set(&[create_ehr_binding()]);
    set.cases
        .push((std::path::PathBuf::from("c.yaml"), case(driven_case())));
    let base_url = sut.base_url();
    drop(sut);
    let again = replay(&set, &ixit(&base_url), None, &transcript, &mut |_| {})?;
    let rederived = results_of(&again);

    let honest = results_of(&live);
    assert_ne!(
        honest.outcomes.first().map(|o| o.status),
        Some(OutcomeStatus::Passed),
        "a refused operation is not a pass"
    );
    assert_eq!(
        divergences(&honest, &rederived),
        Vec::new(),
        "the honest record follows from the recording"
    );

    // The tamper: the same transcript, and a record claiming the case passed.
    let mut altered = honest.clone();
    if let Some(outcome) = altered.outcomes.first_mut() {
        outcome.status = OutcomeStatus::Passed;
        outcome.reason = None;
        outcome.failed_rows.clear();
    }
    let found = divergences(&altered, &rederived);
    assert_eq!(found.len(), 1, "{found:?}");
    let named = found.first().ok_or("the divergence names its case")?;
    assert_eq!(named.case, "REDERIVE-create_ehr");
    assert!(named.submitted.starts_with("passed"), "{named}");
    assert!(!named.rederived.starts_with("passed"), "{named}");
    Ok(())
}

/// A recording that does not carry the exchanges a case needs cannot support
/// a pass, whatever the record says. The replay records a transport failure
/// rather than reproducing a verdict over evidence nobody has.
#[test]
#[expect(
    clippy::panic_in_result_fn,
    reason = "the Book's Result-test shape: assertions panic, plumbing propagates with `?`"
)]
fn a_record_whose_exchanges_were_removed_cannot_pass() -> Fallible {
    let sut = FakeSut::start();
    sut.mount(
        Mock::given(method("POST"))
            .and(path("/ehr"))
            .respond_with(ResponseTemplate::new(201)),
    );
    let live = recorded_campaign(&sut)?;
    let mut transcript = transcript_of(&live);
    for recorded in &mut transcript.cases {
        recorded.exchanges.clear();
    }

    let mut set = artifact_set(&[create_ehr_binding()]);
    set.cases
        .push((std::path::PathBuf::from("c.yaml"), case(driven_case())));
    let base_url = sut.base_url();
    drop(sut);
    let again = replay(&set, &ixit(&base_url), None, &transcript, &mut |_| {})?;

    let rederived = results_of(&again);
    assert_ne!(
        rederived.outcomes.first().map(|o| o.status),
        Some(OutcomeStatus::Passed),
        "an emptied recording supports no pass"
    );
    let found = divergences(&results_of(&live), &rederived);
    assert_eq!(found.len(), 1, "{found:?}");
    Ok(())
}

/// The `equivalent` family, re-derived over a recorded XML document (#469).
///
/// #468 gave the live driver a whole-document comparator, so a served OPT is
/// judged against its corpus fixture. The transcript records that document
/// verbatim and the fixture comes from the catalogue the replay is given, so
/// the re-derivation reaches the same status — and the gate says so over a
/// real recording rather than by reasoning about the classification.
#[test]
#[expect(
    clippy::panic_in_result_fn,
    reason = "the Book's Result-test shape: assertions panic, plumbing propagates with `?`"
)]
fn an_equivalent_case_re_judges_to_the_status_the_live_run_reached() -> Fallible {
    let corpus = assert_fs::TempDir::new()?;
    std::fs::write(corpus.path().join("opt.xml"), OPT_XML)?;
    let sut = FakeSut::start();
    sut.mount(
        Mock::given(method("GET"))
            .and(path("/definition/template/adl1.4/gate.en.v1"))
            .respond_with(ResponseTemplate::new(200).set_body_raw(OPT_XML, "application/xml")),
    );
    let live = execute(
        &equivalent_set(corpus.path()),
        &ixit(&sut.base_url()),
        None,
        Recording::On,
        &mut |_| {},
    )?;
    let transcript = transcript_of(&live);
    assert_eq!(transcript.exchange_count(), 1, "one exchange was recorded");

    let base_url = sut.base_url();
    drop(sut);
    let again = replay(
        &equivalent_set(corpus.path()),
        &ixit(&base_url),
        None,
        &transcript,
        &mut |_| {},
    )?;
    let submitted = results_of(&live);
    let rederived = results_of(&again);
    assert_eq!(
        submitted.outcomes.first().map(|o| o.status),
        Some(OutcomeStatus::Passed),
        "the SUT serves the fixture document back, so the live row passes: {:?}",
        live.records
    );
    assert_eq!(
        divergences(&submitted, &rederived),
        Vec::new(),
        "an `equivalent` row must re-derive to the status the live run reached"
    );
    Ok(())
}

/// The document both sides of the `equivalent` comparison are in.
const OPT_XML: &str = concat!(
    "<?xml version=\"1.0\" encoding=\"utf-8\"?>\n",
    "<template xmlns=\"http://schemas.openehr.org/v1\">",
    "<template_id><value>gate.en.v1</value></template_id>",
    "</template>\n"
);

/// The catalogue the `equivalent` re-derivation is driven over: one binding,
/// one corpus fixture on disk, one case comparing the served document.
fn equivalent_set(corpus_dir: &std::path::Path) -> veredictum::artifacts::ArtifactSet {
    let mut set = crate::fake_sut::artifact_set_over_corpus(
        &[json!({
            "sm_operation": "I_DEFINITION_ADL14.get_opt",
            "its": "its-rest",
            "request": {
                "method": "GET",
                "path": "/definition/template/adl1.4/{template_id}",
                "headers": { "Accept": "application/xml" }
            },
            "outcomes": { "ok": { "status": 200 } }
        })],
        json!({
            "gate.opt": {
                "source": "opt.xml",
                "format": "opt-xml",
                "rm_versions": [">=1.0.2"],
                "validity": { "verdict": "valid" },
                "template_id": "gate.en.v1",
                "provenance": "authored for the re-derivation gate (#469)"
            }
        }),
        corpus_dir,
    );
    set.cases.push((
        std::path::PathBuf::from("equivalent.yaml"),
        case(json!({
            "id": "REDERIVE-get_opt", "kind": "functional", "component": "DEFINITION_ADL14",
            "sm_operation": "I_DEFINITION_ADL14.get_opt",
            "test_purpose": "t", "description": "d", "spec_refs": [],
            "data_sets": ["gate.opt"],
            "flow": [{
                "step": 1, "call": "get_opt",
                "with": { "template_id": "gate.en.v1" },
                "expect": "ok",
                "assert": [{ "assert": "equivalent", "to": "${ds:gate.opt}" }]
            }]
        })),
    ));
    set
}
