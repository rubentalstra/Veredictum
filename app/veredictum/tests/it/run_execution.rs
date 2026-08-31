// SPDX-FileCopyrightText: Veredictum contributors
// SPDX-License-Identifier: Apache-2.0

//! The run orchestration (`veredictum::run::execute`) against the fake SUT:
//! which cases a campaign drives, which it records as registered exceptions,
//! and what the finished report carries.
//!
//! The selection law itself is adjudicated in the runner's own unit tests;
//! what needs a socket is the ORCHESTRATION around it — a campaign holding an
//! inactive case, an excused case, a content case and a driven one at once,
//! and the two facts the report only gains from a real exchange: the recorded
//! transcript and the System manifest's `restapi_specs_version`.

use serde_json::{Value, json};
use veredictum::exec::RowOutcome;
use veredictum::run::{Exception, Progress, UnestablishedFact, execute};
use veredictum::transcript::Recording;
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

/// `GET /` — the System OPTIONS manifest, whose `restapi_specs_version` the
/// report carries as an independent confirmation of the party's declaration.
fn system_options_binding() -> Value {
    json!({
        "sm_operation": "I_ITS_REST_SYSTEM.options",
        "its": "its-rest",
        "request": { "method": "GET", "path": "/" },
        "outcomes": { "ok": { "status": 200 } }
    })
}

/// A binding the release does not realize, so every case over it is excused
/// with the binding's own register citation.
fn unrealized_binding() -> Value {
    json!({
        "sm_operation": "I_PARTY_RELATIONSHIP.get_party_relationship",
        "its": "its-rest",
        "unrealized": {
            "reason": "ITS-REST 1.1.0 surfaces no PARTY_RELATIONSHIP resource",
            "source": "SM i_party_relationship.adoc vs ITS-REST demographic.openapi.yaml",
            "ambiguity": "AMB-32"
        },
        "request": { "method": "GET", "path": "/demographic/party_relationship/{versioned_object_uid}" },
        "outcomes": { "ok": { "status": 200 } }
    })
}

fn driven_case() -> Value {
    json!({
        "id": "RUN-create_ehr", "kind": "functional", "component": "EHR",
        "sm_operation": "I_EHR_SERVICE.create_ehr",
        "test_purpose": "t", "description": "d", "spec_refs": [],
        "flow": [{ "step": 1, "call": "create_ehr", "expect": "created" }]
    })
}

fn draft_case() -> Value {
    json!({
        "id": "RUN-create_ehr-draft", "kind": "functional", "component": "EHR",
        "status": "draft",
        "sm_operation": "I_EHR_SERVICE.create_ehr",
        "test_purpose": "t", "description": "d", "spec_refs": [],
        "flow": [{ "step": 1, "call": "create_ehr", "expect": "created" }]
    })
}

fn excused_case() -> Value {
    json!({
        "id": "RUN-party_relationship", "kind": "functional", "component": "DEMOGRAPHIC",
        "sm_operation": "I_PARTY_RELATIONSHIP.get_party_relationship",
        "test_purpose": "t", "description": "d", "spec_refs": [],
        "flow": [{ "step": 1, "call": "get_party_relationship", "expect": "ok" }]
    })
}

/// A case whose ground is a globally empty server: it sorts FIRST, because on
/// an exclusively-owned SUT that ground holds only before other cases
/// provision anything.
fn exclusive_case() -> Value {
    json!({
        "id": "RUN-create_ehr-exclusive", "kind": "functional", "component": "EHR",
        "sm_operation": "I_EHR_SERVICE.create_ehr",
        "test_purpose": "t", "description": "d", "spec_refs": [],
        "requires": { "server": "exclusive" },
        "flow": [{ "step": 1, "call": "create_ehr", "expect": "created" }]
    })
}

/// An ixit whose environment declares an exclusively-owned SUT, so the
/// exclusive-ground case is driven rather than excused.
fn exclusive_ixit(base_url: &str) -> Result<veredictum::ixit::Ixit, serde_json::Error> {
    serde_json::from_value(json!({
        "instances": { "sut": { "base_url": base_url, "auth": { "mode": "none" } } },
        "environment": {
            "exclusive_server": true, "hardware_class": "test", "cores": 1,
            "memory_gb": 1, "storage_class": "ram", "topology": "single node"
        }
    }))
}

/// A campaign carrying an inactive case, an excused case and a driven one
/// reports each in its own channel: the draft and the unrealized case are
/// REGISTERED EXCEPTIONS with their reasons, the excused one also lands in
/// the record as a cited not-applicable row, and only the driven case counts
/// toward interpreter coverage.
#[test]
#[expect(
    clippy::panic_in_result_fn,
    reason = "the Book's Result-test shape: assertions panic, plumbing propagates with `?`"
)]
fn a_campaign_separates_the_driven_cases_from_its_registered_exceptions() -> Fallible {
    let sut = FakeSut::start();
    sut.mount(
        Mock::given(method("POST"))
            .and(path("/ehr"))
            .respond_with(ResponseTemplate::new(201)),
    );

    let mut set = artifact_set(&[create_ehr_binding(), unrealized_binding()]);
    for document in [driven_case(), draft_case(), excused_case()] {
        set.cases
            .push((std::path::PathBuf::from("c.yaml"), case(document)));
    }

    let mut seen: Vec<String> = Vec::new();
    let report = execute(
        &set,
        &ixit(&sut.base_url()),
        None,
        Recording::Off,
        &mut |progress| seen.push(progress.render_line()),
    )?;

    assert_eq!(report.considered, 3);
    assert_eq!(report.interpreter_run, 1, "only the driven case ran");

    let exceptions: Vec<(&str, &Exception)> = report
        .exceptions
        .iter()
        .map(|(id, exception)| (id.as_str(), exception))
        .collect();
    assert_eq!(exceptions.len(), 2, "{exceptions:?}");
    let draft = exceptions
        .iter()
        .find(|(id, _)| *id == "RUN-create_ehr-draft")
        .map(|(_, exception)| *exception)
        .ok_or("the draft case is a registered exception")?;
    assert!(
        matches!(draft, Exception::Status(reason) if reason.contains("Draft")),
        "{draft:?}"
    );
    let unrealized = exceptions
        .iter()
        .find(|(id, _)| *id == "RUN-party_relationship")
        .map(|(_, exception)| *exception)
        .ok_or("the unrealized case is a registered exception")?;
    assert!(
        matches!(unrealized, Exception::Unrealized(citation) if citation.contains("AMB-32")),
        "{unrealized:?}"
    );

    // The excused case is IN the record, as one cited not-applicable row; the
    // draft one is not verdict-bearing at all, so it is not recorded.
    let recorded: Vec<&str> = report.records.iter().map(|r| r.case.as_str()).collect();
    assert!(recorded.contains(&"RUN-party_relationship"), "{recorded:?}");
    assert!(!recorded.contains(&"RUN-create_ehr-draft"), "{recorded:?}");
    let excused = report
        .records
        .iter()
        .find(|r| r.case.as_str() == "RUN-party_relationship")
        .ok_or("the excused case is recorded")?;
    assert_eq!(excused.rows_driven, 0);
    assert!(matches!(
        excused.rows.as_slice(),
        [RowOutcome::NotApplicable { .. }]
    ));

    // Coverage counts the cases the interpreter drove against those it
    // considered, and the progress channel reports the same total.
    assert!(
        (report.interpreter_coverage() - 1.0 / 3.0).abs() < 1e-9,
        "{}",
        report.interpreter_coverage()
    );
    assert_eq!(seen.first().map(String::as_str), Some("progress: 0/3"));
    assert_eq!(
        seen.len(),
        4,
        "one selection line plus one per case: {seen:?}"
    );
    assert!(
        report.transcripts.is_empty(),
        "an unrecorded run keeps no exchanges"
    );
    Ok(())
}

/// The exclusive-ground case is driven FIRST whatever order the tree loaded
/// in, because on a freshly reset SUT its ground holds only before another
/// case provisions anything.
#[test]
#[expect(
    clippy::panic_in_result_fn,
    reason = "the Book's Result-test shape: assertions panic, plumbing propagates with `?`"
)]
fn an_exclusive_ground_case_is_driven_before_the_rest() -> Fallible {
    let sut = FakeSut::start();
    sut.mount(
        Mock::given(method("POST"))
            .and(path("/ehr"))
            .respond_with(ResponseTemplate::new(201)),
    );

    let mut set = artifact_set(&[create_ehr_binding()]);
    // Loaded in the opposite order to the one the run must use.
    for document in [driven_case(), exclusive_case()] {
        set.cases
            .push((std::path::PathBuf::from("c.yaml"), case(document)));
    }

    let report = execute(
        &set,
        &exclusive_ixit(&sut.base_url())?,
        None,
        Recording::Off,
        &mut |_| {},
    )?;
    let driven: Vec<&str> = report.records.iter().map(|r| r.case.as_str()).collect();
    assert_eq!(
        driven,
        vec!["RUN-create_ehr-exclusive", "RUN-create_ehr"],
        "the global-state ground runs before anything provisions"
    );
    assert_eq!(report.interpreter_run, 2);
    Ok(())
}

/// A recorded run keeps the exchanges it drove, and a campaign that drives the
/// System OPTIONS manifest carries the `restapi_specs_version` the server
/// served — an independent confirmation of the party's declared version,
/// gathered only because the exchange happened.
#[test]
#[expect(
    clippy::panic_in_result_fn,
    reason = "the Book's Result-test shape: assertions panic, plumbing propagates with `?`"
)]
fn a_recorded_campaign_carries_its_exchanges_and_the_served_manifest_version() -> Fallible {
    let sut = FakeSut::start();
    sut.mount(Mock::given(method("GET")).and(path("/")).respond_with(
        ResponseTemplate::new(200).set_body_json(json!({
            "solution": "fake-cdr",
            "restapi_specs_version": "1.1.0"
        })),
    ));

    let mut set = artifact_set(&[system_options_binding()]);
    set.cases.push((
        std::path::PathBuf::from("options.yaml"),
        case(json!({
            "id": "RUN-system_options", "kind": "functional", "component": "ADMIN",
            "sm_operation": "I_ITS_REST_SYSTEM.options",
            "test_purpose": "t", "description": "d", "spec_refs": [],
            "flow": [{ "step": 1, "call": "options", "expect": "ok" }]
        })),
    ));

    let report = execute(
        &set,
        &ixit(&sut.base_url()),
        None,
        Recording::On,
        &mut |_| {},
    )?;
    assert_eq!(report.restapi_specs_version.as_deref(), Some("1.1.0"));
    assert_eq!(report.transcripts.len(), 1, "the driven case is recorded");
    let transcript = report
        .transcripts
        .first()
        .ok_or("the recorded run carries one case transcript")?;
    assert_eq!(transcript.case.as_str(), "RUN-system_options");
    assert!(!transcript.exchanges.is_empty());
    Ok(())
}

/// A CONTENT case is driven through the synthesized generate→commit→expect
/// flow: one executor serves both kinds, so the decision table's rows arrive
/// as the run's row axis and the case counts as interpreter-run.
#[test]
#[expect(
    clippy::panic_in_result_fn,
    reason = "the Book's Result-test shape: assertions panic, plumbing propagates with `?`"
)]
fn a_content_case_is_driven_through_its_synthesized_flow() -> Fallible {
    let sut = FakeSut::start();
    sut.mount(
        Mock::given(method("POST"))
            .and(path("/ehr"))
            .respond_with(ResponseTemplate::new(201).insert_header("Location", "/ehr/e-1")),
    );
    sut.mount(
        Mock::given(method("POST"))
            .and(path("/ehr/e-1/composition"))
            .respond_with(ResponseTemplate::new(201)),
    );

    let mut set = artifact_set(&[
        create_ehr_binding(),
        json!({
            "sm_operation": "I_EHR_COMPOSITION.create_composition",
            "its": "its-rest",
            "request": {
                "method": "POST",
                "path": "/ehr/{ehr_id}/composition",
                "body": "composition"
            },
            "outcomes": { "created": { "status": 201 }, "validation_failed": { "status": 422 } }
        }),
    ]);
    set.cases.push((
        std::path::PathBuf::from("content.yaml"),
        case(json!({
            "id": "CONT-DV_TEXT-run", "kind": "content", "component": "CONTENT",
            "rm_class": "DV_TEXT",
            "test_purpose": "t", "description": "d", "spec_refs": [],
            "decision_table": {
                "columns": ["value", "expected", "violates"],
                "rows": [["hello", "accepted", []], ["world", "accepted", []]]
            }
        })),
    ));

    let report = execute(
        &set,
        &ixit(&sut.base_url()),
        None,
        Recording::Off,
        &mut |_| {},
    )?;
    assert_eq!(report.interpreter_run, 1, "the content case is driven");
    let record = report
        .records
        .first()
        .ok_or("the content case produced a record")?;
    assert_eq!(
        record.rows_total, 2,
        "the decision table's rows are the run's row axis"
    );
    Ok(())
}

/// The progress channel's rendering is the documented driver-facing grammar,
/// and it names each case as it is processed rather than only once it lands.
#[test]
fn the_progress_channel_names_each_case_as_it_is_processed() {
    let sut = FakeSut::start();
    sut.mount(
        Mock::given(method("POST"))
            .and(path("/ehr"))
            .respond_with(ResponseTemplate::new(201)),
    );
    let mut set = artifact_set(&[create_ehr_binding()]);
    set.cases
        .push((std::path::PathBuf::from("c.yaml"), case(driven_case())));

    let mut lines: Vec<String> = Vec::new();
    let report = execute(
        &set,
        &ixit(&sut.base_url()),
        None,
        Recording::Off,
        &mut |progress: Progress<'_>| lines.push(progress.render_line()),
    )
    .expect("the campaign is drivable");
    assert_eq!(report.interpreter_run, 1);
    assert_eq!(lines, vec!["progress: 0/1", "progress: 1/1 RUN-create_ehr"]);
}

/// A campaign driven WITH a party statement selects on the ICS: a case gating
/// only capabilities the statement does not claim is recorded as a GUARDED
/// exception carrying its citation, rather than driven into a red row against
/// a surface the party never offered to serve.
#[test]
#[expect(
    clippy::panic_in_result_fn,
    reason = "the Book's Result-test shape: assertions panic, plumbing propagates with `?`"
)]
fn a_statement_takes_the_cases_it_claims_nothing_for_out_of_the_campaign() -> Fallible {
    let sut = FakeSut::start();
    sut.mount(
        Mock::given(method("POST"))
            .and(path("/ehr"))
            .respond_with(ResponseTemplate::new(201)),
    );

    let mut set = artifact_set(&[create_ehr_binding()]);
    set.cases
        .push((std::path::PathBuf::from("c.yaml"), case(driven_case())));
    set.cases.push((
        std::path::PathBuf::from("gated.yaml"),
        case(json!({
            "id": "RUN-create_ehr-gated", "kind": "functional", "component": "EHR",
            "sm_operation": "I_EHR_SERVICE.create_ehr",
            "test_purpose": "t", "description": "d", "spec_refs": [],
            "capabilities": ["Signing"],
            "flow": [{ "step": 1, "call": "create_ehr", "expect": "created" }]
        })),
    ));

    let statement: veredictum::party::Statement = serde_json::from_value(json!({
        "product": { "name": "p", "version": "1", "vendor": "v", "identifier": "i" },
        "schedule_release": "CNF-2.0",
        "spec_versions": { "rm": "1.2.0", "its_rest": "1.1.0" },
        "claims": { "capabilities": ["EhrOperations"], "profiles": ["CORE"] },
        "tech_profiles": [{ "its": "its-rest", "formats": ["canonical-json"] }],
        "options": []
    }))?;

    let report = execute(
        &set,
        &ixit(&sut.base_url()),
        Some(&statement),
        Recording::Off,
        &mut |_| {},
    )?;
    assert_eq!(report.considered, 2);
    assert_eq!(report.interpreter_run, 1, "only the ungated case drove");

    let (excused, exception) = report
        .exceptions
        .first()
        .ok_or("the gated case is a registered exception")?;
    assert_eq!(excused.as_str(), "RUN-create_ehr-gated");
    let Exception::Guarded(citation) = exception else {
        panic!("expected a guarded exception, got {exception:?}");
    };
    assert!(citation.contains("Signing"), "{citation}");

    // The same citation is the excused case's single recorded row.
    let record = report
        .records
        .iter()
        .find(|r| r.case.as_str() == "RUN-create_ehr-gated")
        .ok_or("the excused case is recorded")?;
    assert_eq!(
        record.rows.as_slice(),
        [RowOutcome::NotApplicable {
            citation: citation.clone()
        }]
    );
    Ok(())
}
/// The register the option pair below belongs to: one `option_select` entry
/// enumerating both arms, which is how the catalogue records a behaviour the
/// release leaves to the service.
fn option_register() -> Result<veredictum::model::register::AmbiguityRegister, serde_json::Error> {
    serde_json::from_value(json!({
        "AMB-167": {
            "ambiguity": "the release declares application/xml on the EHR resource without defining its document root, so a service MAY offer XML or MAY refuse it under the XML Format 406 MUST",
            "source": "ITS-REST docs/overview/Requests_and_responses.md §XML Format + §Data representation",
            "handling": "sibling cases carry option tags; the ICS options declaration selects",
            "disposition": "option_select",
            "options": ["ehr-xml-supported", "ehr-xml-unsupported"]
        }
    }))
}

/// `POST /ehr` with both arms of the negotiation branch declared, so the
/// offering arm and the refusal arm are each drivable.
fn create_ehr_binding_with_refusal() -> Value {
    json!({
        "sm_operation": "I_EHR_SERVICE.create_ehr",
        "its": "its-rest",
        "request": { "method": "POST", "path": "/ehr" },
        "outcomes": { "created": { "status": 201 }, "not_acceptable": { "status": 406 } }
    })
}

/// The offering arm: the service serves the XML representation.
fn xml_supported_case() -> Value {
    json!({
        "id": "RUN-create_ehr-xml_supported", "kind": "functional", "component": "EHR",
        "sm_operation": "I_EHR_SERVICE.create_ehr",
        "test_purpose": "t", "description": "d", "spec_refs": [],
        "option": "ehr-xml-supported",
        "ambiguities": ["AMB-167"],
        "flow": [{ "step": 1, "call": "create_ehr", "expect": "created" }]
    })
}

/// The refusal arm: the service refuses the representation it never defined.
fn xml_unsupported_case() -> Value {
    json!({
        "id": "RUN-create_ehr-xml_unsupported", "kind": "functional", "component": "EHR",
        "sm_operation": "I_EHR_SERVICE.create_ehr",
        "test_purpose": "t", "description": "d", "spec_refs": [],
        "option": "ehr-xml-unsupported",
        "ambiguities": ["AMB-167"],
        "flow": [{ "step": 1, "call": "create_ehr", "expect": "not_acceptable" }]
    })
}

/// The catalogue plus the register, carrying both arms of the one branch.
fn option_pair_world() -> Result<veredictum::artifacts::ArtifactSet, serde_json::Error> {
    let mut set = artifact_set(&[create_ehr_binding_with_refusal()]);
    set.register = Some((
        std::path::PathBuf::from("registers/ambiguities.yaml"),
        option_register()?,
    ));
    for document in [xml_supported_case(), xml_unsupported_case()] {
        set.cases
            .push((std::path::PathBuf::from("c.yaml"), case(document)));
    }
    Ok(set)
}

/// A statement-blind campaign reports NEITHER arm of a mutually exclusive
/// option pair as a failure.
///
/// The arms are the halves of one `option_select` register branch, so a server
/// serves exactly one of them: driving both guarantees a red row no conformant
/// server could avoid. With no ICS to select the arm from, each case is
/// recorded not-applicable with the citation naming the fact that was missing:
/// never passed, because nothing was driven, and never failed, because nothing
/// the server did is at issue.
#[test]
#[expect(
    clippy::panic_in_result_fn,
    reason = "the Book's Result-test shape: assertions panic, plumbing propagates with `?`"
)]
fn a_statement_blind_run_reports_no_arm_of_a_mutually_exclusive_option_pair_as_a_failure()
-> Fallible {
    let sut = FakeSut::start();
    sut.mount(
        Mock::given(method("POST"))
            .and(path("/ehr"))
            .respond_with(ResponseTemplate::new(201)),
    );

    let report = execute(
        &option_pair_world()?,
        &ixit(&sut.base_url()),
        None,
        Recording::Off,
        &mut |_| {},
    )?;

    // The criterion the live run violated: no row of the pair is a failure.
    let failures: Vec<(&str, &RowOutcome)> = report
        .records
        .iter()
        .flat_map(|record| {
            record
                .rows
                .iter()
                .map(move |row| (record.case.as_str(), row))
        })
        .filter(|(_, row)| matches!(row, RowOutcome::Failed { .. }))
        .collect();
    assert!(
        failures.is_empty(),
        "a statement-blind campaign published an option arm as a failure: {failures:?}"
    );

    assert_eq!(report.considered, 2);
    assert_eq!(report.interpreter_run, 0, "neither arm is driven blind");
    for record in &report.records {
        let rolled = veredictum::party::OutcomeRecord::from(record);
        assert_eq!(
            rolled.status,
            veredictum::party::OutcomeStatus::NotApplicable,
            "{}: {rolled:?}",
            record.case
        );
        assert_eq!(record.rows_driven, 0, "{}", record.case);
        let [RowOutcome::NotApplicable { citation }] = record.rows.as_slice() else {
            panic!(
                "{}: expected one not-applicable row, got {:?}",
                record.case, record.rows
            );
        };
        assert!(citation.contains("AMB-167"), "{citation}");
        assert!(
            citation.contains("no party statement was supplied"),
            "{citation}"
        );
    }
    let recorded: Vec<&str> = report.records.iter().map(|r| r.case.as_str()).collect();
    assert_eq!(
        recorded,
        vec![
            "RUN-create_ehr-xml_supported",
            "RUN-create_ehr-xml_unsupported"
        ],
        "both arms are recorded, both as selection records"
    );
    assert!(
        sut.requests().is_empty(),
        "an unselectable arm is never driven at the server"
    );

    // The run's own account of what it could not select, and how many cases
    // that touched — reported once at run level, not per row.
    assert_eq!(
        report.unestablished.get(&UnestablishedFact::OptionBranch),
        Some(&2)
    );
    Ok(())
}

/// The same pair driven WITH an ICS declaring one arm: that arm drives against
/// the server and the other is excused. The blind run's silence is therefore
/// the absent ICS, never an undrivable pair.
#[test]
#[expect(
    clippy::panic_in_result_fn,
    reason = "the Book's Result-test shape: assertions panic, plumbing propagates with `?`"
)]
fn an_ics_declaring_one_arm_drives_exactly_that_arm() -> Fallible {
    let sut = FakeSut::start();
    sut.mount(
        Mock::given(method("POST"))
            .and(path("/ehr"))
            .respond_with(ResponseTemplate::new(201)),
    );

    let statement: veredictum::party::Statement = serde_json::from_value(json!({
        "product": { "name": "p", "version": "1", "vendor": "v", "identifier": "i" },
        "schedule_release": "CNF-2.0",
        "spec_versions": { "rm": "1.2.0", "its_rest": "1.1.0" },
        "claims": { "capabilities": ["EhrOperations"], "profiles": ["CORE"] },
        "tech_profiles": [{ "its": "its-rest", "formats": ["canonical-json"] }],
        "options": ["ehr-xml-supported"]
    }))?;

    let report = execute(
        &option_pair_world()?,
        &ixit(&sut.base_url()),
        Some(&statement),
        Recording::Off,
        &mut |_| {},
    )?;

    assert_eq!(report.interpreter_run, 1, "the declared arm drives");
    assert!(
        report.unestablished.is_empty(),
        "an ICS establishes every selection fact: {:?}",
        report.unestablished
    );
    let driven = report
        .records
        .iter()
        .find(|r| r.case.as_str() == "RUN-create_ehr-xml_supported")
        .ok_or("the declared arm is recorded")?;
    assert_eq!(driven.rows.as_slice(), [RowOutcome::Passed]);
    let excused = report
        .records
        .iter()
        .find(|r| r.case.as_str() == "RUN-create_ehr-xml_unsupported")
        .ok_or("the undeclared arm is recorded")?;
    let [RowOutcome::NotApplicable { citation }] = excused.rows.as_slice() else {
        panic!("expected one not-applicable row, got {:?}", excused.rows);
    };
    assert!(citation.contains("statement.options"), "{citation}");
    Ok(())
}
