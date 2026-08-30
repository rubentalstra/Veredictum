// SPDX-FileCopyrightText: Veredictum contributors
// SPDX-License-Identifier: Apache-2.0

//! The live HTTP driver against the fake SUT: the classification seams that
//! need a real answer on a real socket.
//!
//! Every test here drives [`veredictum::exec::driver::HttpDriver`] through
//! the [`veredictum::exec::StepDriver`] surface, so request construction,
//! transport, status classification, capture extraction and the outcome's
//! declared wire expectation all run exactly as they do against a deployed
//! CDR. The stub controls only what comes back.

use serde_json::{Value, json};
use veredictum::exec::driver::HttpDriver;
use veredictum::exec::outcome::{Observation, StepJudgement, judge};
use veredictum::exec::state::VarStore;
use veredictum::exec::{StepDriver, StepObservation};
use veredictum::transcript::Recording;
use veredictum::vocab::OutcomeKind;
use wiremock::matchers::{method, path};
use wiremock::{Mock, ResponseTemplate};

use crate::fake_sut::{FakeSut, artifact_set, case, closed_port_url, ixit};

/// Anything a driver construction or a step can fail with, so a test body
/// propagates plumbing failures with `?`
/// (<https://doc.rust-lang.org/book/ch11-01-writing-tests.html>).
type Fallible = Result<(), Box<dyn std::error::Error>>;

/// The EHR-creation binding: `POST /ehr`, two mapped outcomes, and the two
/// identifying headers a 201 carries — `ETag` weak-quoted
/// (ITS-REST `Requests_and_responses.md` §`ETag` and Last-Modified: the value
/// "should have a weakness indicator `W/` prefix") and `Location` naming the
/// created resource (§Location: "used by a server to indicate the URL of a
/// newly created resource").
fn create_ehr_binding() -> Value {
    json!({
        "sm_operation": "I_EHR_SERVICE.create_ehr",
        "its": "its-rest",
        "request": { "method": "POST", "path": "/ehr" },
        "outcomes": {
            "created": { "status": 201 },
            "already_exists": { "status": 409 }
        },
        "captures": {
            "version_uid": { "from": "header ETag", "strip": "weak-quotes" },
            "ehr_id": { "from": "header Location last-segment" }
        }
    })
}

fn create_ehr_case() -> Value {
    json!({
        "id": "WIRE-create_ehr", "kind": "functional", "component": "EHR",
        "sm_operation": "I_EHR_SERVICE.create_ehr",
        "test_purpose": "t", "description": "d", "spec_refs": [],
        "flow": [{
            "step": 1, "call": "create_ehr", "expect": "created",
            "capture": {
                "ehr_id": "created.ehr_id",
                "version_uid": "created.version_uid"
            }
        }]
    })
}

/// Drive step 1 of a one-step case against a running fake SUT.
fn drive_one(
    sut: &FakeSut,
    bindings: &[Value],
    case_document: Value,
    expected: OutcomeKind,
    vars: &mut VarStore,
) -> Result<StepObservation, Box<dyn std::error::Error>> {
    let set = artifact_set(bindings);
    let topology = ixit(&sut.base_url());
    let core = case(case_document);
    let step = core.flow.first().ok_or("the case declares no flow step")?;
    let mut driver = HttpDriver::new(&set, &topology, None)?;
    Ok(driver.perform(&core, step, expected, 0, vars)?)
}

/// Interpreter law (c), one branch per status the fake SUT can answer with:
/// the binding's own outcome map classifies first, the route-table-wide
/// vocabulary (`vocab/selectors.yaml`) covers 401, and a status neither maps
/// is UNMAPPED — inconclusive, never a conformance finding.
#[test]
#[expect(
    clippy::panic_in_result_fn,
    reason = "the Book's Result-test shape: assertions panic, plumbing propagates with `?`"
)]
fn each_status_classifies_against_the_binding_then_the_universal_vocabulary() -> Fallible {
    for (status, expected) in [
        (201_u16, Observation::Kind(OutcomeKind::Created)),
        (409, Observation::Kind(OutcomeKind::AlreadyExists)),
        (401, Observation::Kind(OutcomeKind::Unauthenticated)),
        (404, Observation::Unmapped { status: 404 }),
        (500, Observation::Unmapped { status: 500 }),
    ] {
        let sut = FakeSut::start();
        sut.mount(
            Mock::given(method("POST"))
                .and(path("/ehr"))
                .respond_with(ResponseTemplate::new(status)),
        );
        let mut vars = VarStore::default();
        let observed = drive_one(
            &sut,
            &[create_ehr_binding()],
            create_ehr_case(),
            OutcomeKind::Created,
            &mut vars,
        )?;
        assert_eq!(
            observed.observation, expected,
            "status {status} classified wrongly"
        );
    }
    Ok(())
}

/// The three judgements the classification feeds, over real answers: the
/// expected kind continues the row, a mapped-but-different kind FAILS it,
/// and an unmapped status ERRORS it (inconclusive).
#[test]
#[expect(
    clippy::panic_in_result_fn,
    reason = "the Book's Result-test shape: assertions panic, plumbing propagates with `?`"
)]
fn a_mapped_mismatch_fails_the_row_and_an_unmapped_status_errors_it() -> Fallible {
    let expectations = [
        (201_u16, StepJudgement::Continue),
        (
            409,
            StepJudgement::Failed {
                expected: OutcomeKind::Created,
                observed: OutcomeKind::AlreadyExists,
            },
        ),
    ];
    for (status, expected) in expectations {
        let sut = FakeSut::start();
        sut.mount(
            Mock::given(method("POST"))
                .and(path("/ehr"))
                .respond_with(ResponseTemplate::new(status)),
        );
        let mut vars = VarStore::default();
        let observed = drive_one(
            &sut,
            &[create_ehr_binding()],
            create_ehr_case(),
            OutcomeKind::Created,
            &mut vars,
        )?;
        assert_eq!(judge(OutcomeKind::Created, &observed.observation), expected);
    }

    let sut = FakeSut::start();
    sut.mount(
        Mock::given(method("POST"))
            .and(path("/ehr"))
            .respond_with(ResponseTemplate::new(418)),
    );
    let mut vars = VarStore::default();
    let observed = drive_one(
        &sut,
        &[create_ehr_binding()],
        create_ehr_case(),
        OutcomeKind::Created,
        &mut vars,
    )?;
    match judge(OutcomeKind::Created, &observed.observation) {
        StepJudgement::Errored(reason) => {
            assert!(reason.contains("inconclusive"), "{reason}");
        }
        other => panic!("an unmapped status must error the row, got {other:?}"),
    }
    Ok(())
}

/// A refused connection is a TRANSPORT fault: the row is inconclusive and
/// the server under test is never named. This is the attribution law's
/// falsifiability direction — an instrument that scored a broken socket as a
/// conformance failure would manufacture defects.
#[test]
#[expect(
    clippy::panic_in_result_fn,
    reason = "the Book's Result-test shape: assertions panic, plumbing propagates with `?`"
)]
fn a_refused_connection_is_inconclusive_never_a_sut_failure() -> Fallible {
    let set = artifact_set(&[create_ehr_binding()]);
    let topology = ixit(&closed_port_url());
    let core = case(create_ehr_case());
    let step = core.flow.first().ok_or("the case declares no flow step")?;
    let mut driver = HttpDriver::new(&set, &topology, None)?;
    let mut vars = VarStore::default();
    let observed = driver.perform(&core, step, OutcomeKind::Created, 0, &mut vars)?;

    match &observed.observation {
        Observation::Transport(fault) => assert!(fault.contains("transport"), "{fault}"),
        other => panic!("a refused connection must be a transport fault, got {other:?}"),
    }
    match judge(OutcomeKind::Created, &observed.observation) {
        StepJudgement::Errored(reason) => assert!(reason.contains("inconclusive"), "{reason}"),
        other => panic!("a transport fault must error the row, got {other:?}"),
    }
    Ok(())
}

/// A redirect that never terminates exhausts the client's redirect policy,
/// which is a transport fault and therefore inconclusive — the same
/// direction as a refused connection, and for the same reason.
#[test]
#[expect(
    clippy::panic_in_result_fn,
    reason = "the Book's Result-test shape: assertions panic, plumbing propagates with `?`"
)]
fn a_redirect_loop_is_inconclusive() -> Fallible {
    let sut = FakeSut::start();
    let self_reference = format!("{}/ehr", sut.base_url());
    sut.mount(
        Mock::given(method("POST"))
            .and(path("/ehr"))
            .respond_with(ResponseTemplate::new(307).insert_header("Location", &*self_reference)),
    );
    let mut vars = VarStore::default();
    let observed = drive_one(
        &sut,
        &[create_ehr_binding()],
        create_ehr_case(),
        OutcomeKind::Created,
        &mut vars,
    )?;
    match &observed.observation {
        Observation::Transport(fault) => assert!(fault.contains("transport"), "{fault}"),
        other => panic!("a redirect loop must be a transport fault, got {other:?}"),
    }
    Ok(())
}

/// A redirect the client CAN follow classifies on the status the redirect
/// target answered with, and 307 preserves the method, so the created
/// resource is still a POST result. Implementation contract of the driver's
/// client policy; no openEHR spec governs redirect following here.
#[test]
#[expect(
    clippy::panic_in_result_fn,
    reason = "the Book's Result-test shape: assertions panic, plumbing propagates with `?`"
)]
fn a_followed_redirect_classifies_on_the_final_status() -> Fallible {
    let sut = FakeSut::start();
    let target = format!("{}/ehr-moved", sut.base_url());
    sut.mount(
        Mock::given(method("POST"))
            .and(path("/ehr"))
            .respond_with(ResponseTemplate::new(307).insert_header("Location", &*target)),
    );
    sut.mount(
        Mock::given(method("POST"))
            .and(path("/ehr-moved"))
            .respond_with(ResponseTemplate::new(201).insert_header("ETag", "W/\"moved::sys::1\"")),
    );
    let mut vars = VarStore::default();
    let observed = drive_one(
        &sut,
        &[create_ehr_binding()],
        create_ehr_case(),
        OutcomeKind::Created,
        &mut vars,
    )?;
    assert_eq!(
        observed.observation,
        Observation::Kind(OutcomeKind::Created)
    );
    let methods: Vec<String> = sut
        .requests()
        .iter()
        .map(|request| request.method.to_string())
        .collect();
    assert_eq!(methods, vec!["POST".to_owned(), "POST".to_owned()]);
    Ok(())
}

/// Header captures read the real response: the `ETag` weak-quote wrapper is
/// stripped (ITS-REST `Requests_and_responses.md` §`ETag` and Last-Modified —
/// the value "should have a weakness indicator `W/` prefix"), and the
/// `Location` capture takes the last path segment of the created resource's
/// URL (§Location).
#[test]
#[expect(
    clippy::panic_in_result_fn,
    reason = "the Book's Result-test shape: assertions panic, plumbing propagates with `?`"
)]
fn header_captures_strip_the_weak_indicator_and_take_the_created_id() -> Fallible {
    let sut = FakeSut::start();
    sut.mount(
        Mock::given(method("POST")).and(path("/ehr")).respond_with(
            ResponseTemplate::new(201)
                .insert_header("ETag", "W/\"7f1b::openEHRSys.example.com::1\"")
                .insert_header(
                    "Location",
                    "https://openEHRSys.example.com/v1/ehr/347a5490-55ee-4da9-b91a-9bba710f730e",
                ),
        ),
    );
    let mut vars = VarStore::default();
    let observed = drive_one(
        &sut,
        &[create_ehr_binding()],
        create_ehr_case(),
        OutcomeKind::Created,
        &mut vars,
    )?;
    assert_eq!(
        observed.observation,
        Observation::Kind(OutcomeKind::Created)
    );
    assert_eq!(
        vars.scalar(&veredictum::ids::CaptureName::parse("version_uid")?),
        Some("7f1b::openEHRSys.example.com::1")
    );
    assert_eq!(
        vars.scalar(&veredictum::ids::CaptureName::parse("ehr_id")?),
        Some("347a5490-55ee-4da9-b91a-9bba710f730e")
    );
    Ok(())
}

/// The `${…}` round trip where it meets the wire: a capture bound from step
/// 1's response renders into step 2's path template, so the second request
/// addresses the identifier the SUT actually minted. The assertion is the
/// path the fake SUT RECEIVED, not what the driver believed it sent.
#[test]
#[expect(
    clippy::panic_in_result_fn,
    reason = "the Book's Result-test shape: assertions panic, plumbing propagates with `?`"
)]
fn a_capture_from_the_first_answer_addresses_the_second_request() -> Fallible {
    let sut = FakeSut::start();
    sut.mount(Mock::given(method("POST")).and(path("/ehr")).respond_with(
        ResponseTemplate::new(201).insert_header("Location", "http://sut/v1/ehr/EHR-42"),
    ));
    sut.mount(
        Mock::given(method("GET"))
            .and(path("/ehr/EHR-42"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({ "_type": "EHR" }))),
    );

    let bindings = [
        create_ehr_binding(),
        json!({
            "sm_operation": "I_EHR_SERVICE.get_ehr",
            "its": "its-rest",
            "request": { "method": "GET", "path": "/ehr/{ehr_id}" },
            "outcomes": { "ok": { "status": 200 } }
        }),
    ];
    let set = artifact_set(&bindings);
    let topology = ixit(&sut.base_url());
    let core = case(json!({
        "id": "WIRE-capture_round_trip", "kind": "functional", "component": "EHR",
        "sm_operation": "I_EHR_SERVICE.create_ehr",
        "test_purpose": "t", "description": "d", "spec_refs": [],
        "flow": [
            {
                "step": 1, "call": "create_ehr", "expect": "created",
                "capture": { "ehr_id": "created.ehr_id" }
            },
            { "step": 2, "call": "get_ehr", "expect": "ok" }
        ]
    }));
    let mut driver = HttpDriver::new(&set, &topology, None)?;
    let mut vars = VarStore::default();
    let mut kinds = Vec::new();
    for (index, expected) in [OutcomeKind::Created, OutcomeKind::Ok]
        .into_iter()
        .enumerate()
    {
        let step = core.flow.get(index).ok_or("missing flow step")?;
        kinds.push(
            driver
                .perform(&core, step, expected, 0, &mut vars)?
                .observation,
        );
    }
    assert_eq!(
        kinds,
        vec![
            Observation::Kind(OutcomeKind::Created),
            Observation::Kind(OutcomeKind::Ok)
        ]
    );
    let paths: Vec<String> = sut
        .requests()
        .iter()
        .map(|request| request.url.path().to_owned())
        .collect();
    assert_eq!(paths, vec!["/ehr".to_owned(), "/ehr/EHR-42".to_owned()]);
    Ok(())
}

/// Content negotiation on a bodyless read: a case declaring the canonical
/// XML format sends `Accept: application/xml`, which is the header
/// ITS-REST `Resources.md` §XML Format names — "The client SHOULD use the
/// `Accept: application/xml` request header to specify the expected XML
/// response format." A canonical-JSON case sends the JSON media type on the
/// same operation.
#[test]
#[expect(
    clippy::panic_in_result_fn,
    reason = "the Book's Result-test shape: assertions panic, plumbing propagates with `?`"
)]
fn a_bodyless_read_accepts_the_representation_the_case_declares() -> Fallible {
    for (format, media) in [
        ("canonical-xml", "application/xml"),
        ("canonical-json", "application/json"),
    ] {
        let sut = FakeSut::start();
        sut.mount(
            Mock::given(method("GET"))
                .and(path("/ehr/EHR-1"))
                .respond_with(ResponseTemplate::new(200)),
        );
        let bindings = [json!({
            "sm_operation": "I_EHR_SERVICE.get_ehr",
            "its": "its-rest",
            "request": { "method": "GET", "path": "/ehr/{ehr_id}" },
            "outcomes": { "ok": { "status": 200 } }
        })];
        let mut vars = VarStore::default();
        vars.set(
            veredictum::ids::CaptureName::parse("ehr_id")?,
            veredictum::exec::state::Captured::Scalar("EHR-1".to_owned()),
        );
        let observed = drive_one(
            &sut,
            &bindings,
            json!({
                "id": "WIRE-negotiated_read", "kind": "functional", "component": "EHR",
                "sm_operation": "I_EHR_SERVICE.get_ehr",
                "formats": [format],
                "test_purpose": "t", "description": "d", "spec_refs": [],
                "flow": [{ "step": 1, "call": "get_ehr", "expect": "ok" }]
            }),
            OutcomeKind::Ok,
            &mut vars,
        )?;
        assert_eq!(observed.observation, Observation::Kind(OutcomeKind::Ok));

        let received = sut.requests();
        let request = received.first().ok_or("the SUT received no request")?;
        let accept = request
            .headers
            .get("accept")
            .ok_or("the request carried no Accept header")?;
        assert_eq!(accept.to_str()?, media, "format {format}");
    }
    Ok(())
}

/// A body-carrying commit labels its payload with the case's format and
/// still asks for the canonical JSON answer: ITS-REST `Resources.md`
/// §XML Format — "A client MAY use the header `Content-Type:
/// application/xml` in the requests to specify the XML payload format" —
/// while `ETag` and `Location` are representation-independent, so the
/// response negotiation is not the request payload's business.
#[test]
#[expect(
    clippy::panic_in_result_fn,
    reason = "the Book's Result-test shape: assertions panic, plumbing propagates with `?`"
)]
fn a_body_carrying_commit_labels_its_payload_format() -> Fallible {
    let sut = FakeSut::start();
    sut.mount(
        Mock::given(method("POST"))
            .and(path("/ehr/EHR-1/composition"))
            .respond_with(ResponseTemplate::new(201).insert_header("ETag", "W/\"c::sys::1\"")),
    );
    let bindings = [json!({
        "sm_operation": "I_EHR_COMPOSITION.commit_composition",
        "its": "its-rest",
        "request": {
            "method": "POST",
            "path": "/ehr/{ehr_id}/composition",
            "body": "composition"
        },
        "outcomes": { "created": { "status": 201 } }
    })];
    let mut vars = VarStore::default();
    vars.set(
        veredictum::ids::CaptureName::parse("ehr_id")?,
        veredictum::exec::state::Captured::Scalar("EHR-1".to_owned()),
    );
    let observed = drive_one(
        &sut,
        &bindings,
        json!({
            "id": "WIRE-labelled_commit", "kind": "functional", "component": "EHR_COMPOSITION",
            "sm_operation": "I_EHR_COMPOSITION.commit_composition",
            "formats": ["canonical-xml"],
            "test_purpose": "t", "description": "d", "spec_refs": [],
            "flow": [{
                "step": 1, "call": "commit_composition", "expect": "created",
                "with": { "ehr_id": "${ehr_id}", "composition": { "_type": "COMPOSITION" } }
            }]
        }),
        OutcomeKind::Created,
        &mut vars,
    )?;
    assert_eq!(
        observed.observation,
        Observation::Kind(OutcomeKind::Created)
    );

    let received = sut.requests();
    let request = received.first().ok_or("the SUT received no request")?;
    assert_eq!(
        request
            .headers
            .get("content-type")
            .ok_or("no Content-Type on a body-carrying request")?
            .to_str()?,
        "application/xml"
    );
    assert_eq!(
        request
            .headers
            .get("accept")
            .ok_or("no Accept on the request")?
            .to_str()?,
        "application/json"
    );
    Ok(())
}

/// The expected outcome's declared header matchers are EXECUTED against the
/// answer: a 201 that carries no `Location` fails the step even though its
/// status classified correctly (ITS-REST `Requests_and_responses.md`
/// §Location: the header "is used in `201 Created` responses when a new
/// resource is successfully created").
#[test]
#[expect(
    clippy::panic_in_result_fn,
    reason = "the Book's Result-test shape: assertions panic, plumbing propagates with `?`"
)]
fn a_declared_header_matcher_is_evaluated_against_the_answer() -> Fallible {
    let bindings = [json!({
        "sm_operation": "I_EHR_SERVICE.create_ehr",
        "its": "its-rest",
        "request": { "method": "POST", "path": "/ehr" },
        "outcomes": {
            "created": { "status": 201, "headers": { "ETag": "present", "Location": "present" } }
        }
    })];

    let complete = FakeSut::start();
    complete.mount(
        Mock::given(method("POST")).and(path("/ehr")).respond_with(
            ResponseTemplate::new(201)
                .insert_header("ETag", "W/\"e::sys::1\"")
                .insert_header("Location", "http://sut/v1/ehr/EHR-1"),
        ),
    );
    let mut vars = VarStore::default();
    let observed = drive_one(
        &complete,
        &bindings,
        create_ehr_case(),
        OutcomeKind::Created,
        &mut vars,
    )?;
    assert!(
        observed.assertion_failures.is_empty(),
        "a complete 201 must produce no header finding: {:?}",
        observed.assertion_failures
    );

    let partial = FakeSut::start();
    partial.mount(
        Mock::given(method("POST"))
            .and(path("/ehr"))
            .respond_with(ResponseTemplate::new(201).insert_header("ETag", "W/\"e::sys::1\"")),
    );
    let mut vars = VarStore::default();
    let observed = drive_one(
        &partial,
        &bindings,
        create_ehr_case(),
        OutcomeKind::Created,
        &mut vars,
    )?;
    assert_eq!(
        observed.observation,
        Observation::Kind(OutcomeKind::Created),
        "the status still classifies; the finding is the missing header"
    );
    assert!(
        observed
            .assertion_failures
            .iter()
            .any(|failure| failure.reason().contains("Location")),
        "findings {:?} name no missing Location",
        observed.assertion_failures
    );
    Ok(())
}

/// The recorded transcript keeps what actually crossed the wire, and the
/// credential the instance stamps is WITHHELD from the record: a published
/// run artifact must never carry a token.
#[test]
#[expect(
    clippy::panic_in_result_fn,
    reason = "the Book's Result-test shape: assertions panic, plumbing propagates with `?`"
)]
fn the_recorded_exchange_keeps_the_wire_and_withholds_the_credential() -> Fallible {
    let sut = FakeSut::start();
    sut.mount(
        Mock::given(method("POST"))
            .and(path("/ehr"))
            .respond_with(ResponseTemplate::new(201).insert_header("ETag", "W/\"r::sys::1\"")),
    );
    let set = artifact_set(&[create_ehr_binding()]);
    let topology: veredictum::ixit::Ixit = serde_json::from_value(json!({
        "instances": {
            "sut": {
                "base_url": sut.base_url(),
                "auth": { "mode": "none" },
                "headers": { "Authorization": "Bearer a-test-token" }
            }
        }
    }))?;
    let core = case(create_ehr_case());
    let step = core.flow.first().ok_or("the case declares no flow step")?;
    let mut driver = HttpDriver::new(&set, &topology, None)?.with_recording(Recording::On);
    let mut vars = VarStore::default();
    let observed = driver.perform(&core, step, OutcomeKind::Created, 0, &mut vars)?;
    assert_eq!(
        observed.observation,
        Observation::Kind(OutcomeKind::Created)
    );

    let exchanges = driver.take_exchanges();
    let exchange = exchanges.first().ok_or("recording produced no exchange")?;
    assert_eq!(exchange.seq, 1);
    assert_eq!(exchange.response.status, 201);
    let recorded_credential = exchange
        .request
        .headers
        .iter()
        .find(|(name, _)| name.eq_ignore_ascii_case("authorization"))
        .map(|(_, value)| value.clone());
    assert_ne!(
        recorded_credential.as_deref(),
        Some("Bearer a-test-token"),
        "the transcript recorded the credential verbatim"
    );
    assert!(
        driver.take_exchanges().is_empty(),
        "taking the exchanges must leave the driver empty"
    );
    Ok(())
}

/// An answer whose bytes are not JSON is preserved as TEXT rather than
/// silently discarded, so a wrong content type is still evidence in the
/// recorded exchange. Implementation contract of the driver's body reader.
#[test]
#[expect(
    clippy::panic_in_result_fn,
    reason = "the Book's Result-test shape: assertions panic, plumbing propagates with `?`"
)]
fn a_non_json_answer_is_recorded_as_text() -> Fallible {
    let sut = FakeSut::start();
    sut.mount(
        Mock::given(method("POST"))
            .and(path("/ehr"))
            .respond_with(ResponseTemplate::new(409).set_body_string("<error>duplicate</error>")),
    );
    let set = artifact_set(&[create_ehr_binding()]);
    let topology = ixit(&sut.base_url());
    let core = case(create_ehr_case());
    let step = core.flow.first().ok_or("the case declares no flow step")?;
    let mut driver = HttpDriver::new(&set, &topology, None)?.with_recording(Recording::On);
    let mut vars = VarStore::default();
    let observed = driver.perform(&core, step, OutcomeKind::Created, 0, &mut vars)?;
    assert_eq!(
        observed.observation,
        Observation::Kind(OutcomeKind::AlreadyExists)
    );

    let exchanges = driver.take_exchanges();
    let body = exchanges
        .first()
        .and_then(|exchange| exchange.response.body.clone())
        .ok_or("the recorded exchange carried no body")?;
    assert_eq!(body, Value::String("<error>duplicate</error>".to_owned()));
    Ok(())
}

/// An empty answer records NO body at all, which is a different fact from
/// an answer whose body is the JSON literal `null`.
#[test]
#[expect(
    clippy::panic_in_result_fn,
    reason = "the Book's Result-test shape: assertions panic, plumbing propagates with `?`"
)]
fn an_empty_answer_records_no_body() -> Fallible {
    let sut = FakeSut::start();
    sut.mount(
        Mock::given(method("POST"))
            .and(path("/ehr"))
            .respond_with(ResponseTemplate::new(201)),
    );
    let set = artifact_set(&[create_ehr_binding()]);
    let topology = ixit(&sut.base_url());
    let core = case(create_ehr_case());
    let step = core.flow.first().ok_or("the case declares no flow step")?;
    let mut driver = HttpDriver::new(&set, &topology, None)?.with_recording(Recording::On);
    let mut vars = VarStore::default();
    driver.perform(&core, step, OutcomeKind::Created, 0, &mut vars)?;
    let exchanges = driver.take_exchanges();
    assert_eq!(
        exchanges.first().and_then(|e| e.response.body.clone()),
        None
    );
    Ok(())
}

/// An operation the released ITS publishes no wire for is answered with a
/// transport-class observation, so the row is inconclusive and no request
/// is ever sent.
#[test]
#[expect(
    clippy::panic_in_result_fn,
    reason = "the Book's Result-test shape: assertions panic, plumbing propagates with `?`"
)]
fn an_unrealized_operation_sends_nothing_and_is_inconclusive() -> Fallible {
    let sut = FakeSut::start();
    let bindings = [json!({
        "sm_operation": "I_EHR_SERVICE.create_ehr",
        "its": "its-rest",
        "unrealized": {
            "reason": "the released ITS publishes no wire for this operation",
            "source": "ITS-REST overview, Resources",
            "ambiguity": "AMB-1"
        }
    })];
    let mut vars = VarStore::default();
    let observed = drive_one(
        &sut,
        &bindings,
        create_ehr_case(),
        OutcomeKind::Created,
        &mut vars,
    )?;
    match &observed.observation {
        Observation::Transport(fault) => assert!(fault.contains("unrealized"), "{fault}"),
        other => panic!("an unrealized operation must be inconclusive, got {other:?}"),
    }
    assert_eq!(sut.requests().len(), 0, "an unrealized step sent a request");
    Ok(())
}

/// A step addressing an instance the party's ixit does not declare is a
/// TOPOLOGY gap of that party: the row is inconclusive and the campaign
/// continues, rather than the whole run aborting.
#[test]
#[expect(
    clippy::panic_in_result_fn,
    reason = "the Book's Result-test shape: assertions panic, plumbing propagates with `?`"
)]
fn an_undeclared_instance_is_a_topology_gap_not_a_run_failure() -> Fallible {
    let sut = FakeSut::start();
    let mut vars = VarStore::default();
    let observed = drive_one(
        &sut,
        &[create_ehr_binding()],
        json!({
            "id": "WIRE-undeclared_instance", "kind": "functional", "component": "EHR",
            "sm_operation": "I_EHR_SERVICE.create_ehr",
            "test_purpose": "t", "description": "d", "spec_refs": [],
            "flow": [{ "step": 1, "call": "create_ehr", "on": "readonly", "expect": "created" }]
        }),
        OutcomeKind::Created,
        &mut vars,
    )?;
    match &observed.observation {
        Observation::Transport(fault) => {
            assert!(fault.contains("instance unavailable"), "{fault}");
        }
        other => panic!("an undeclared instance must be inconclusive, got {other:?}"),
    }
    assert_eq!(sut.requests().len(), 0);
    Ok(())
}

/// The `requires.ehr` precondition is established over the wire before the
/// flow runs: the driver drives the EHR-creation binding, mints `${ehr_id}`
/// from the answer, and reports the row ready.
#[test]
#[expect(
    clippy::panic_in_result_fn,
    reason = "the Book's Result-test shape: assertions panic, plumbing propagates with `?`"
)]
fn provisioning_mints_the_ehr_id_the_flow_addresses() -> Fallible {
    let sut = FakeSut::start();
    sut.mount(Mock::given(method("POST")).and(path("/ehr")).respond_with(
        ResponseTemplate::new(201).insert_header("Location", "http://sut/v1/ehr/EHR-provisioned"),
    ));
    let set = artifact_set(&[create_ehr_binding()]);
    let topology = ixit(&sut.base_url());
    let core = case(json!({
        "id": "WIRE-provisioned", "kind": "functional", "component": "EHR",
        "sm_operation": "I_EHR_SERVICE.create_ehr",
        "requires": { "ehr": { "commits": "none" } },
        "test_purpose": "t", "description": "d", "spec_refs": [],
        "flow": [{ "step": 1, "call": "create_ehr", "expect": "created" }]
    }));
    let mut driver = HttpDriver::new(&set, &topology, None)?;
    let mut vars = VarStore::default();
    let provisioned = driver.provision(&core, 0, &mut vars)?;
    assert_eq!(provisioned, veredictum::exec::Provisioned::Ready);
    assert_eq!(
        vars.scalar(&veredictum::ids::CaptureName::parse("ehr_id")?),
        Some("EHR-provisioned")
    );
    assert_eq!(sut.requests().len(), 1, "provisioning drove one exchange");
    Ok(())
}

/// A body capture reads a dotted path out of the answer the SUT actually
/// sent, and a `*.body` capture keeps the whole document for the row's
/// postconditions to read.
#[test]
#[expect(
    clippy::panic_in_result_fn,
    reason = "the Book's Result-test shape: assertions panic, plumbing propagates with `?`"
)]
fn a_body_capture_reads_a_dotted_path_out_of_the_answer() -> Fallible {
    let sut = FakeSut::start();
    sut.mount(
        Mock::given(method("GET"))
            .and(path("/ehr/EHR-1"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "_type": "EHR",
                "ehr_id": { "_type": "HIER_OBJECT_ID", "value": "EHR-1" },
                "time_created": { "value": "2024-01-01T00:00:00Z" }
            }))),
    );
    let bindings = [json!({
        "sm_operation": "I_EHR_SERVICE.get_ehr",
        "its": "its-rest",
        "request": { "method": "GET", "path": "/ehr/{ehr_id}" },
        "outcomes": { "ok": { "status": 200 } },
        "captures": { "ehr_uid": { "from": "body \"ehr_id.value\"" } }
    })];
    let mut vars = VarStore::default();
    vars.set(
        veredictum::ids::CaptureName::parse("ehr_id")?,
        veredictum::exec::state::Captured::Scalar("EHR-1".to_owned()),
    );
    let observed = drive_one(
        &sut,
        &bindings,
        json!({
            "id": "WIRE-body_capture", "kind": "functional", "component": "EHR",
            "sm_operation": "I_EHR_SERVICE.get_ehr",
            "test_purpose": "t", "description": "d", "spec_refs": [],
            "flow": [{
                "step": 1, "call": "get_ehr", "expect": "ok",
                "capture": { "ehr_uid": "ok.ehr_uid", "payload": "ok.body" },
                "assert": [
                    { "assert": "field", "path": "_type", "equals": "EHR" },
                    { "assert": "field", "path": "ehr_id/value", "exists": true },
                    { "assert": "field", "path": "system_id", "absent": true }
                ]
            }]
        }),
        OutcomeKind::Ok,
        &mut vars,
    )?;
    assert_eq!(observed.observation, Observation::Kind(OutcomeKind::Ok));
    assert!(
        observed.assertion_failures.is_empty(),
        "the assertions describe the answer the stub sent: {:?}",
        observed.assertion_failures
    );
    assert_eq!(
        vars.scalar(&veredictum::ids::CaptureName::parse("ehr_uid")?),
        Some("EHR-1")
    );
    assert!(
        vars.get(&veredictum::ids::CaptureName::parse("payload")?)
            .is_some(),
        "the whole answer body is kept for the row's postconditions"
    );
    Ok(())
}

/// A field assertion that does not hold names the path it read, and the
/// status still classifies: the two are separate findings.
#[test]
#[expect(
    clippy::panic_in_result_fn,
    reason = "the Book's Result-test shape: assertions panic, plumbing propagates with `?`"
)]
fn a_field_assertion_that_fails_names_its_path() -> Fallible {
    let sut = FakeSut::start();
    sut.mount(
        Mock::given(method("GET"))
            .and(path("/ehr/EHR-1"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({ "_type": "EHR" }))),
    );
    let bindings = [json!({
        "sm_operation": "I_EHR_SERVICE.get_ehr",
        "its": "its-rest",
        "request": { "method": "GET", "path": "/ehr/{ehr_id}" },
        "outcomes": { "ok": { "status": 200 } }
    })];
    let mut vars = VarStore::default();
    vars.set(
        veredictum::ids::CaptureName::parse("ehr_id")?,
        veredictum::exec::state::Captured::Scalar("EHR-1".to_owned()),
    );
    let observed = drive_one(
        &sut,
        &bindings,
        json!({
            "id": "WIRE-failing_field", "kind": "functional", "component": "EHR",
            "sm_operation": "I_EHR_SERVICE.get_ehr",
            "test_purpose": "t", "description": "d", "spec_refs": [],
            "flow": [{
                "step": 1, "call": "get_ehr", "expect": "ok",
                "assert": [{ "assert": "field", "path": "_type", "equals": "COMPOSITION" }]
            }]
        }),
        OutcomeKind::Ok,
        &mut vars,
    )?;
    assert_eq!(observed.observation, Observation::Kind(OutcomeKind::Ok));
    assert_eq!(observed.assertion_failures.len(), 1);
    assert!(
        observed
            .assertion_failures
            .first()
            .is_some_and(|failure| failure.reason().contains("_type")),
        "findings {:?} name no path",
        observed.assertion_failures
    );
    Ok(())
}

/// A served `RESULT_SET` whose date/time cells are spelled with `+00:00`
/// against a case that spells them `Z`.
fn result_set_query(cells: Option<&str>) -> (Value, Value) {
    let binding = json!({
        "sm_operation": "I_QUERY_SERVICE.execute_ad_hoc_query",
        "its": "its-rest",
        "request": { "method": "GET", "path": "/query/aql" },
        "outcomes": { "ok": { "status": 200 } }
    });
    let mut assertion = json!({
        "assert": "result_set", "match": "ordered",
        "rows": [["2026-01-01T00:00:00Z"], ["2026-01-01T09:00:00Z"]]
    });
    if let (Some(cells), Some(object)) = (cells, assertion.as_object_mut()) {
        object.insert("cells".to_owned(), json!(cells));
    }
    let case_document = json!({
        "id": "WIRE-result_set_cells", "kind": "functional", "component": "QUERY",
        "sm_operation": "I_QUERY_SERVICE.execute_ad_hoc_query",
        "test_purpose": "t", "description": "d", "spec_refs": [],
        "flow": [{ "step": 1, "call": "execute_ad_hoc_query", "expect": "ok",
                   "assert": [assertion] }]
    });
    (binding, case_document)
}

/// `cells: instant` gates the row on the instant and RECORDS the respelling,
/// while the default exact comparison fails the same answer.
///
/// ITS-REST `docs/overview/Resources.md` §Datetime format assigns the query
/// path only a SHOULD ("Retrieval or querying those resources SHOULD return
/// date, datetime, or time values in the (original) format provided by
/// underlying backend engine"), and BASE `UML/classes/iso8601_timezone.adoc`
/// §Description makes `Z` "a literal meaning UTC …, i.e. timezone `+0000`".
#[test]
#[expect(
    clippy::panic_in_result_fn,
    reason = "the Book's Result-test shape: assertions panic, plumbing propagates with `?`"
)]
fn an_instant_cell_mode_passes_a_respelled_offset_and_records_it() -> Fallible {
    for (cells, failures, advisories) in [(None, 1_usize, 0_usize), (Some("instant"), 0, 2)] {
        let sut = FakeSut::start();
        sut.mount(
            Mock::given(method("GET"))
                .and(path("/query/aql"))
                .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                    "rows": [["2026-01-01T00:00:00+00:00"], ["2026-01-01T09:00:00+00:00"]]
                }))),
        );
        let (binding, case_document) = result_set_query(cells);
        let mut vars = VarStore::default();
        let observed = drive_one(&sut, &[binding], case_document, OutcomeKind::Ok, &mut vars)?;
        assert_eq!(observed.observation, Observation::Kind(OutcomeKind::Ok));
        assert_eq!(
            observed.assertion_failures.len(),
            failures,
            "cells {cells:?}: {:?}",
            observed.assertion_failures
        );
        assert_eq!(
            observed.advisories.len(),
            advisories,
            "cells {cells:?}: {:?}",
            observed.advisories
        );
        assert!(
            observed
                .advisories
                .iter()
                .all(|a| a.contains("+00:00") && a.contains("Datetime format")),
            "an observation names the served spelling and its spec sentence: {:?}",
            observed.advisories
        );
        assert!(
            observed
                .labelled_advisories(0, 1)
                .iter()
                .all(|a| a.starts_with("row 0 step 1: ")),
            "a recorded observation names the row and step it was made on"
        );
    }
    Ok(())
}

/// Row postconditions read the LAST answer of the row, so a field
/// postcondition is evaluated against the body the read step brought back.
#[test]
#[expect(
    clippy::panic_in_result_fn,
    reason = "the Book's Result-test shape: assertions panic, plumbing propagates with `?`"
)]
fn postconditions_are_evaluated_over_the_rows_last_answer() -> Fallible {
    let sut = FakeSut::start();
    sut.mount(
        Mock::given(method("GET"))
            .and(path("/ehr/EHR-1"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "_type": "EHR",
                "ehr_id": { "value": "EHR-1" }
            }))),
    );
    let bindings = [json!({
        "sm_operation": "I_EHR_SERVICE.get_ehr",
        "its": "its-rest",
        "request": { "method": "GET", "path": "/ehr/{ehr_id}" },
        "outcomes": { "ok": { "status": 200 } }
    })];
    let set = artifact_set(&bindings);
    let topology = ixit(&sut.base_url());
    let core = case(json!({
        "id": "WIRE-postconditions", "kind": "functional", "component": "EHR",
        "sm_operation": "I_EHR_SERVICE.get_ehr",
        "test_purpose": "t", "description": "d", "spec_refs": [],
        "flow": [{ "step": 1, "call": "get_ehr", "expect": "ok" }],
        "postconditions": [
            { "assert": "field", "path": "ehr_id/value", "equals": "EHR-1" },
            { "assert": "field", "path": "_type", "equals": "COMPOSITION" }
        ]
    }));
    let mut driver = HttpDriver::new(&set, &topology, None)?;
    let mut vars = VarStore::default();
    vars.set(
        veredictum::ids::CaptureName::parse("ehr_id")?,
        veredictum::exec::state::Captured::Scalar("EHR-1".to_owned()),
    );
    let step = core.flow.first().ok_or("the case declares no flow step")?;
    driver.perform(&core, step, OutcomeKind::Ok, 0, &mut vars)?;
    let failures = driver.postconditions(&core, 0, &mut vars)?.failures;
    assert_eq!(
        failures.len(),
        1,
        "one postcondition holds and one does not: {failures:?}"
    );
    assert!(
        failures
            .first()
            .is_some_and(|f| f.reason().contains("_type")),
        "{failures:?}"
    );
    Ok(())
}

/// A `commit_time` capture takes the live commit window and widens it by the
/// SUT's own `Date` header, so a clock skew between runner and server cannot
/// make a correctly stamped version look late.
#[test]
#[expect(
    clippy::panic_in_result_fn,
    reason = "the Book's Result-test shape: assertions panic, plumbing propagates with `?`"
)]
fn a_commit_time_capture_reads_the_servers_date_header() -> Fallible {
    let sut = FakeSut::start();
    sut.mount(Mock::given(method("POST")).and(path("/ehr")).respond_with(
        ResponseTemplate::new(201).insert_header("Date", "Wed, 22 Jul 2009 19:15:56 GMT"),
    ));
    let mut vars = VarStore::default();
    let observed = drive_one(
        &sut,
        &[create_ehr_binding()],
        json!({
            "id": "WIRE-commit_time", "kind": "functional", "component": "EHR",
            "sm_operation": "I_EHR_SERVICE.create_ehr",
            "test_purpose": "t", "description": "d", "spec_refs": [],
            "flow": [{
                "step": 1, "call": "create_ehr", "expect": "created",
                "capture": { "committed_at": "created.commit_time" }
            }]
        }),
        OutcomeKind::Created,
        &mut vars,
    )?;
    assert_eq!(
        observed.observation,
        Observation::Kind(OutcomeKind::Created)
    );
    assert!(
        vars.get(&veredictum::ids::CaptureName::parse("committed_at")?)
            .is_some(),
        "the commit window was never bound"
    );
    Ok(())
}

/// A declared query parameter is percent-encoded onto the URL the SUT
/// receives, and an optional reference nothing bound omits its parameter
/// instead of sending an empty one.
#[test]
#[expect(
    clippy::panic_in_result_fn,
    reason = "the Book's Result-test shape: assertions panic, plumbing propagates with `?`"
)]
fn query_parameters_are_encoded_and_an_unbound_optional_is_omitted() -> Fallible {
    let sut = FakeSut::start();
    sut.mount(
        Mock::given(method("GET"))
            .and(path("/query/aql"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({ "rows": [] }))),
    );
    let bindings = [json!({
        "sm_operation": "I_QUERY_SERVICE.execute_ad_hoc_query",
        "its": "its-rest",
        "request": {
            "method": "GET",
            "path": "/query/aql",
            "query": { "q": "${q}", "fetch": "${fetch?}" }
        },
        "outcomes": { "ok": { "status": 200 } }
    })];
    let mut vars = VarStore::default();
    let observed = drive_one(
        &sut,
        &bindings,
        json!({
            "id": "WIRE-query_params", "kind": "functional", "component": "QUERY",
            "sm_operation": "I_QUERY_SERVICE.execute_ad_hoc_query",
            "test_purpose": "t", "description": "d", "spec_refs": [],
            "flow": [{
                "step": 1, "call": "execute_ad_hoc_query", "expect": "ok",
                "with": { "q": "SELECT e/ehr_id/value FROM EHR e" }
            }]
        }),
        OutcomeKind::Ok,
        &mut vars,
    )?;
    assert_eq!(observed.observation, Observation::Kind(OutcomeKind::Ok));
    let received = sut.requests();
    let request = received.first().ok_or("the SUT received no request")?;
    let query = request.url.query().ok_or("no query string was sent")?;
    assert_eq!(
        query, "q=SELECT%20e%2Fehr_id%2Fvalue%20FROM%20EHR%20e",
        "the optional `fetch` must be omitted, not sent empty"
    );
    Ok(())
}

/// The read-modify-write setter binding, with the caller's `set:` block: a PUT
/// whose body is the captured `status_body` with those fields overwritten.
fn patched_status_binding(set: &Value) -> Value {
    json!({
        "sm_operation": "I_EHR_STATUS.set_ehr_queryable",
        "its": "its-rest",
        "request": {
            "method": "PUT",
            "path": "/ehr/{ehr_id}/ehr_status",
            "body": { "from_capture": "status_body", "set": set }
        },
        "outcomes": { "ok": { "status": 200 } }
    })
}

/// The captured base resource the patch is applied to.
fn bound_status_body() -> Result<VarStore, Box<dyn std::error::Error>> {
    let mut vars = VarStore::default();
    vars.set(
        veredictum::ids::CaptureName::parse("status_body")?,
        veredictum::exec::state::Captured::Body(json!({
            "_type": "EHR_STATUS",
            "is_queryable": true
        })),
    );
    Ok(vars)
}

/// A one-step case driving the setter, with the given `with:` block.
fn patched_status_case(with: &Value) -> Value {
    json!({
        "id": "WIRE-patched_set", "kind": "functional", "component": "EHR",
        "sm_operation": "I_EHR_STATUS.set_ehr_queryable",
        "test_purpose": "t", "description": "d", "spec_refs": [],
        "flow": [{
            "step": 1, "call": "set_ehr_queryable", "expect": "ok", "with": with
        }]
    })
}

/// The body the SUT actually received, parsed as JSON.
fn received_body(sut: &FakeSut) -> Result<Value, Box<dyn std::error::Error>> {
    let received = sut.requests();
    let request = received.first().ok_or("the SUT received no request")?;
    Ok(serde_json::from_slice(&request.body)?)
}

/// A `${…}` in a patched body's `set:` value is RENDERED before the request is
/// sent: the wire carries the value the case supplied, never the literal
/// template text. The literal would make the exchange vacuous — the SUT would
/// store `${client_value}` and the row would still go green.
#[test]
#[expect(
    clippy::panic_in_result_fn,
    reason = "the Book's Result-test shape: assertions panic, plumbing propagates with `?`"
)]
fn a_patched_set_value_reaches_the_wire_rendered() -> Fallible {
    let sut = FakeSut::start();
    sut.mount(
        Mock::given(method("PUT"))
            .and(path("/ehr/EHR-1/ehr_status"))
            .respond_with(ResponseTemplate::new(200)),
    );
    let bindings = [patched_status_binding(&json!({
        "uid": { "_type": "OBJECT_VERSION_ID", "value": "${client_value}" }
    }))];
    let mut vars = bound_status_body()?;
    let observed = drive_one(
        &sut,
        &bindings,
        patched_status_case(&json!({
            "ehr_id": "EHR-1",
            "client_value": "cccccccc-2222-4222-8222-222222222222"
        })),
        OutcomeKind::Ok,
        &mut vars,
    )?;
    assert_eq!(observed.observation, Observation::Kind(OutcomeKind::Ok));

    let body = received_body(&sut)?;
    assert_eq!(
        body.get("uid"),
        Some(&json!({
            "_type": "OBJECT_VERSION_ID",
            "value": "cccccccc-2222-4222-8222-222222222222"
        })),
        "the wire body must carry the rendered value"
    );
    assert_eq!(
        body.get("is_queryable"),
        Some(&json!(true)),
        "the captured base resource must survive the patch"
    );
    Ok(())
}

/// The unbound twin: a `set:` value naming a reference nothing bound refuses
/// the step transport-class (inconclusive, runner-side) and sends NOTHING, so
/// no server is accused over a value the runner failed to build.
#[test]
#[expect(
    clippy::panic_in_result_fn,
    reason = "the Book's Result-test shape: assertions panic, plumbing propagates with `?`"
)]
fn an_unbound_reference_in_a_patched_set_value_refuses_the_step() -> Fallible {
    let sut = FakeSut::start();
    sut.mount(
        Mock::given(method("PUT"))
            .and(path("/ehr/EHR-1/ehr_status"))
            .respond_with(ResponseTemplate::new(200)),
    );
    let bindings = [patched_status_binding(&json!({
        "uid": { "_type": "OBJECT_VERSION_ID", "value": "${client_value}" }
    }))];
    let mut vars = bound_status_body()?;
    let observed = drive_one(
        &sut,
        &bindings,
        patched_status_case(&json!({ "ehr_id": "EHR-1" })),
        OutcomeKind::Ok,
        &mut vars,
    )?;
    match &observed.observation {
        Observation::Transport(reason) => {
            assert!(reason.contains("client_value"), "{reason}");
            assert!(reason.contains("uid"), "{reason}");
        }
        other => panic!("an unbound patched reference must refuse the step, got {other:?}"),
    }
    assert!(
        sut.requests().is_empty(),
        "nothing may reach the wire once the body failed to render"
    );
    Ok(())
}

/// A reference-free `set:` value is inserted verbatim: the rendering pass
/// rewrites nothing an ordinary binding authors.
#[test]
#[expect(
    clippy::panic_in_result_fn,
    reason = "the Book's Result-test shape: assertions panic, plumbing propagates with `?`"
)]
fn a_literal_patched_set_value_stays_byte_identical() -> Fallible {
    let sut = FakeSut::start();
    sut.mount(
        Mock::given(method("PUT"))
            .and(path("/ehr/EHR-1/ehr_status"))
            .respond_with(ResponseTemplate::new(200)),
    );
    let literal = json!({
        "is_queryable": false,
        "uid": { "_type": "OBJECT_VERSION_ID", "value": "fixed::sys::1" }
    });
    let bindings = [patched_status_binding(&literal)];
    let mut vars = bound_status_body()?;
    let observed = drive_one(
        &sut,
        &bindings,
        patched_status_case(&json!({ "ehr_id": "EHR-1" })),
        OutcomeKind::Ok,
        &mut vars,
    )?;
    assert_eq!(observed.observation, Observation::Kind(OutcomeKind::Ok));

    let body = received_body(&sut)?;
    assert_eq!(body.get("is_queryable"), literal.get("is_queryable"));
    assert_eq!(body.get("uid"), literal.get("uid"));
    Ok(())
}

/// A binding whose request body is a NAMED payload role: the case supplies the
/// resource under that name and the wire carries it.
fn named_body_binding() -> Value {
    json!({
        "sm_operation": "I_EHR_COMPOSITION.create_composition",
        "its": "its-rest",
        "request": {
            "method": "POST",
            "path": "/ehr/{ehr_id}/composition",
            "body": "composition"
        },
        "outcomes": { "created": { "status": 201 } }
    })
}

/// A one-step commit driving the named-body binding, with the given `with:`.
fn named_body_case(with: &Value) -> Value {
    json!({
        "id": "WIRE-named_body", "kind": "functional", "component": "EHR_COMPOSITION",
        "sm_operation": "I_EHR_COMPOSITION.create_composition",
        "test_purpose": "t", "description": "d", "spec_refs": [],
        "flow": [{
            "step": 1, "call": "create_composition", "expect": "created", "with": with
        }]
    })
}

/// The composition the case posts, and the only thing the wire may carry.
fn posted_composition() -> Value {
    json!({
        "_type": "COMPOSITION",
        "archetype_node_id": "openEHR-EHR-COMPOSITION.encounter.v1",
        "name": { "_type": "DV_TEXT", "value": "encounter" }
    })
}

/// The named-body path puts the case's payload on the wire UNCHANGED. Nothing
/// asserted the served request body on this path, which is how a body defect
/// reaches a SUT while the row still goes green: the status classifies, the
/// response assertions pass, and what was sent is never read back.
#[test]
#[expect(
    clippy::panic_in_result_fn,
    reason = "the Book's Result-test shape: assertions panic, plumbing propagates with `?`"
)]
fn a_named_body_reaches_the_wire_as_the_case_authored_it() -> Fallible {
    let sut = FakeSut::start();
    sut.mount(
        Mock::given(method("POST"))
            .and(path("/ehr/EHR-1/composition"))
            .respond_with(ResponseTemplate::new(201)),
    );
    let mut vars = VarStore::default();
    let observed = drive_one(
        &sut,
        &[named_body_binding()],
        named_body_case(&json!({ "ehr_id": "EHR-1", "composition": posted_composition() })),
        OutcomeKind::Created,
        &mut vars,
    )?;
    assert_eq!(
        observed.observation,
        Observation::Kind(OutcomeKind::Created)
    );
    assert_eq!(
        received_body(&sut)?,
        posted_composition(),
        "the wire body must be the payload the case named"
    );
    Ok(())
}

/// The ad-hoc query binding: a STRUCTURED request body whose slots resolve
/// against the step's scope, one mandatory (`${q}`) and one optional
/// (`${fetch?}`).
fn structured_body_binding() -> Value {
    json!({
        "sm_operation": "I_QUERY_SERVICE.execute_ad_hoc_query",
        "its": "its-rest",
        "request": {
            "method": "POST",
            "path": "/query/aql",
            "body": { "q": "${q}", "fetch": "${fetch?}" }
        },
        "outcomes": { "ok": { "status": 200 } }
    })
}

/// A one-step query driving the structured-body binding, with the given `with:`.
fn structured_body_case(with: &Value) -> Value {
    json!({
        "id": "WIRE-structured_body", "kind": "functional", "component": "QUERY",
        "sm_operation": "I_QUERY_SERVICE.execute_ad_hoc_query",
        "test_purpose": "t", "description": "d", "spec_refs": [],
        "flow": [{
            "step": 1, "call": "execute_ad_hoc_query", "expect": "ok", "with": with
        }]
    })
}

/// Drive the structured-body query with the given `with:` over the given var
/// store, and return what the step observed beside the body the SUT received.
fn structured_body_on_the_wire(
    sut: &FakeSut,
    with: &Value,
    vars: &mut VarStore,
) -> Result<(StepObservation, Value), Box<dyn std::error::Error>> {
    sut.mount(
        Mock::given(method("POST"))
            .and(path("/query/aql"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({ "rows": [] }))),
    );
    let observed = drive_one(
        sut,
        &[structured_body_binding()],
        structured_body_case(with),
        OutcomeKind::Ok,
        vars,
    )?;
    let body = received_body(sut)?;
    Ok((observed, body))
}

/// The structured-body path renders the step's scope onto the wire: the
/// case's own value fills its slot, a NUMBER keeps its JSON type rather than
/// becoming quoted text, and an unbound optional slot is omitted instead of
/// being sent as a literal `${fetch?}` or a null.
#[test]
#[expect(
    clippy::panic_in_result_fn,
    reason = "the Book's Result-test shape: assertions panic, plumbing propagates with `?`"
)]
fn a_structured_body_reaches_the_wire_rendered() -> Fallible {
    let sut = FakeSut::start();
    let mut vars = VarStore::default();
    let (observed, body) = structured_body_on_the_wire(
        &sut,
        &json!({ "q": "SELECT e FROM EHR e", "fetch": 5 }),
        &mut vars,
    )?;
    assert_eq!(observed.observation, Observation::Kind(OutcomeKind::Ok));
    assert_eq!(
        body.get("q"),
        Some(&json!("SELECT e FROM EHR e")),
        "the mandatory body slot must carry the case's own value"
    );
    assert_eq!(
        body.get("fetch"),
        Some(&json!(5)),
        "a numeric body slot must keep the type the case authored"
    );

    let sut = FakeSut::start();
    let mut vars = VarStore::default();
    let (_, body) =
        structured_body_on_the_wire(&sut, &json!({ "q": "SELECT e FROM EHR e" }), &mut vars)?;
    assert_eq!(
        body.get("fetch"),
        None,
        "an unbound optional body slot is omitted, never sent unrendered"
    );
    Ok(())
}

/// The with-versus-capture priority at the BODY seam, direction one: a
/// structured slot the step does not name in its `with:` renders from the
/// capture, so an earlier step's answer still addresses this request.
#[test]
#[expect(
    clippy::panic_in_result_fn,
    reason = "the Book's Result-test shape: assertions panic, plumbing propagates with `?`"
)]
fn a_structured_body_slot_renders_the_capture_the_step_did_not_supply() -> Fallible {
    let sut = FakeSut::start();
    let mut vars = VarStore::default();
    vars.set(
        veredictum::ids::CaptureName::parse("q")?,
        veredictum::exec::state::Captured::Scalar("SELECT c FROM COMPOSITION c".to_owned()),
    );
    let (observed, body) = structured_body_on_the_wire(&sut, &json!({}), &mut vars)?;
    assert_eq!(observed.observation, Observation::Kind(OutcomeKind::Ok));
    assert_eq!(
        body.get("q"),
        Some(&json!("SELECT c FROM COMPOSITION c")),
        "the capture must fill a body slot the step left unnamed"
    );
    Ok(())
}

/// Direction two, the shadowing pin: the step's own `with:` value wins over a
/// same-named capture in the structured body, exactly as it does in a header
/// or a URL slot. Letting the capture win puts a STALE value on the wire and
/// makes the case's authored input dead text with no diagnostic.
#[test]
#[expect(
    clippy::panic_in_result_fn,
    reason = "the Book's Result-test shape: assertions panic, plumbing propagates with `?`"
)]
fn a_structured_body_slot_takes_the_steps_with_value_over_the_capture() -> Fallible {
    let sut = FakeSut::start();
    let mut vars = VarStore::default();
    vars.set(
        veredictum::ids::CaptureName::parse("q")?,
        veredictum::exec::state::Captured::Scalar("SELECT c FROM COMPOSITION c".to_owned()),
    );
    let (observed, body) =
        structured_body_on_the_wire(&sut, &json!({ "q": "SELECT e FROM EHR e" }), &mut vars)?;
    assert_eq!(observed.observation, Observation::Kind(OutcomeKind::Ok));
    assert_eq!(
        body.get("q"),
        Some(&json!("SELECT e FROM EHR e")),
        "the step's explicit with: value must reach the wire"
    );
    Ok(())
}

/// The setter binding again, this time with a templated `If-Match`: the header
/// seam of the same with-versus-capture priority.
fn if_match_status_binding() -> Value {
    json!({
        "sm_operation": "I_EHR_STATUS.set_ehr_queryable",
        "its": "its-rest",
        "request": {
            "method": "PUT",
            "path": "/ehr/{ehr_id}/ehr_status",
            "body": { "is_queryable": false },
            "headers": { "If-Match": "\"${preceding_version_uid}\"" }
        },
        "outcomes": { "ok": { "status": 200 } }
    })
}

/// Drive the If-Match setter and return what the step observed beside the
/// entity tag the SUT received.
fn if_match_on_the_wire(
    sut: &FakeSut,
    with: &Value,
    vars: &mut VarStore,
) -> Result<(StepObservation, String), Box<dyn std::error::Error>> {
    sut.mount(
        Mock::given(method("PUT"))
            .and(path("/ehr/EHR-1/ehr_status"))
            .respond_with(ResponseTemplate::new(200)),
    );
    let observed = drive_one(
        sut,
        &[if_match_status_binding()],
        patched_status_case(with),
        OutcomeKind::Ok,
        vars,
    )?;
    let received = sut.requests();
    let request = received.first().ok_or("the SUT received no request")?;
    let sent = request
        .headers
        .get("if-match")
        .ok_or("the request carried no If-Match")?;
    Ok((observed, sent.to_str()?.to_owned()))
}

/// A var store holding the version identifier an earlier step captured.
fn bound_preceding_version_uid() -> Result<VarStore, Box<dyn std::error::Error>> {
    let mut vars = VarStore::default();
    vars.set(
        veredictum::ids::CaptureName::parse("preceding_version_uid")?,
        veredictum::exec::state::Captured::Scalar("vo::sut::1".to_owned()),
    );
    Ok(vars)
}

/// The header seam, direction one: a header slot the step does not name in its
/// `with:` renders from the capture the earlier step bound.
#[test]
#[expect(
    clippy::panic_in_result_fn,
    reason = "the Book's Result-test shape: assertions panic, plumbing propagates with `?`"
)]
fn a_header_slot_renders_the_capture_the_step_did_not_supply() -> Fallible {
    let sut = FakeSut::start();
    let mut vars = bound_preceding_version_uid()?;
    let (observed, sent) = if_match_on_the_wire(&sut, &json!({ "ehr_id": "EHR-1" }), &mut vars)?;
    assert_eq!(observed.observation, Observation::Kind(OutcomeKind::Ok));
    assert_eq!(
        sent, "\"vo::sut::1\"",
        "the capture must fill a header slot the step left unnamed"
    );
    Ok(())
}

/// The header seam, direction two — the run-2 triage regression on the wire:
/// the step's own `with:` value wins, so the newer identifier the case passes
/// inline is what `If-Match` carries. Rendering the step-1 capture instead made
/// the SUT's correct 412 read as a red row.
#[test]
#[expect(
    clippy::panic_in_result_fn,
    reason = "the Book's Result-test shape: assertions panic, plumbing propagates with `?`"
)]
fn a_header_slot_takes_the_steps_with_value_over_the_capture() -> Fallible {
    let sut = FakeSut::start();
    let mut vars = bound_preceding_version_uid()?;
    let (observed, sent) = if_match_on_the_wire(
        &sut,
        &json!({ "ehr_id": "EHR-1", "preceding_version_uid": "vo::sut::2" }),
        &mut vars,
    )?;
    assert_eq!(observed.observation, Observation::Kind(OutcomeKind::Ok));
    assert_eq!(
        sent, "\"vo::sut::2\"",
        "the step's explicit with: value must reach the wire"
    );
    Ok(())
}

/// A negative against a non-existent resource has no captured base body, so
/// the wire gets a minimal RM-VALID canonical `EHR_STATUS`: the SUT must
/// refuse on the unknown id, not on the body. RM-validity is load-bearing —
/// `EHR_STATUS` is an unconditional archetype root (RM ehr `ehr_status.adoc`
/// `Is_archetype_root`) and a root without `ARCHETYPED` violates
/// `Archetyped_valid` (RM common `locatable.adoc`).
#[test]
#[expect(
    clippy::panic_in_result_fn,
    reason = "the Book's Result-test shape: assertions panic, plumbing propagates with `?`"
)]
fn an_unbound_status_capture_sends_the_minimal_valid_status_the_negative_needs() -> Fallible {
    let sut = FakeSut::start();
    sut.mount(
        Mock::given(method("PUT"))
            .and(path("/ehr/EHR-absent/ehr_status"))
            .respond_with(ResponseTemplate::new(404)),
    );
    let bindings = [patched_status_binding(&json!({ "is_queryable": false }))];
    // No `status_body` capture: the case never read one back, because the
    // resource it addresses does not exist.
    let mut vars = VarStore::default();
    let observed = drive_one(
        &sut,
        &bindings,
        patched_status_case(&json!({ "ehr_id": "EHR-absent" })),
        OutcomeKind::Ok,
        &mut vars,
    )?;
    assert_eq!(observed.observation, Observation::Unmapped { status: 404 });

    let body = received_body(&sut)?;
    assert_eq!(body["_type"], json!("EHR_STATUS"));
    assert_eq!(body["subject"]["_type"], json!("PARTY_SELF"));
    assert_eq!(
        body["archetype_details"]["_type"],
        json!("ARCHETYPED"),
        "an archetype root without ARCHETYPED violates Archetyped_valid: {body}"
    );
    assert_eq!(
        body["is_queryable"],
        json!(false),
        "the declared set: is applied to the substituted base too"
    );
    Ok(())
}

/// The commit binding whose simplified format carries its template identity
/// in a request header (ITS-REST `operations/composition_create.yaml`).
fn flat_commit_binding() -> Value {
    json!({
        "sm_operation": "I_EHR_COMPOSITION.create_composition",
        "its": "its-rest",
        "request": {
            "method": "POST", "path": "/ehr/{ehr_id}/composition", "body": "composition"
        },
        "formats": ["canonical-json", "wt-flat"],
        "format_headers": {
            "wt-flat": {
                "Content-Type": "application/openehr.wt.flat+json",
                "openehr-template-id": "required"
            }
        },
        "outcomes": { "created": { "status": 201 } }
    })
}

/// A one-step commit in the given format.
fn flat_commit_case(format: Option<&str>) -> Value {
    let mut step = json!({
        "step": 1, "call": "create_composition", "expect": "created",
        "with": { "ehr_id": "EHR-1", "composition": { "ctx/language": "en" } }
    });
    if let Some(format) = format
        && let Some(map) = step.as_object_mut()
    {
        map.insert("format".to_owned(), Value::String(format.to_owned()));
    }
    json!({
        "id": "WIRE-flat_commit", "kind": "functional", "component": "SIMPLIFIED_FORMATS",
        "sm_operation": "I_EHR_COMPOSITION.create_composition",
        "requires": { "templates": ["cnf.opt.minimal"] },
        "test_purpose": "t", "description": "d", "spec_refs": [],
        "flow": [step]
    })
}

/// The request headers the SUT received on its last request, lower-cased.
fn sent_headers(sut: &FakeSut) -> Result<Vec<(String, String)>, Box<dyn std::error::Error>> {
    let received = sut.requests();
    let request = received.last().ok_or("the SUT received no request")?;
    Ok(request
        .headers
        .iter()
        .map(|(name, value)| {
            (
                name.as_str().to_ascii_lowercase(),
                value.to_str().unwrap_or_default().to_owned(),
            )
        })
        .collect())
}

/// A step's declared FORMAT selects the format headers its binding attaches:
/// the simplified representation labels the payload and carries the template
/// identity the receiving server needs to expand it, and a step in the
/// canonical representation carries neither. The identity comes from the
/// case's own provisioned template, so a case never has to restate it.
#[test]
#[expect(
    clippy::panic_in_result_fn,
    reason = "the Book's Result-test shape: assertions panic, plumbing propagates with `?`"
)]
fn a_simplified_format_carries_the_template_identity_its_binding_requires() -> Fallible {
    let dir = assert_fs::TempDir::new()?;
    let manifest = json!({
        "cnf.opt.minimal": {
            "source": "opt.json",
            "format": "opt-xml",
            "template_id": "minimal.event.v1",
            "validity": { "verdict": "valid" },
            "provenance": "authored in-test: the template the flat payload names"
        }
    });
    std::fs::write(dir.path().join("opt.json"), "<template/>")?;

    let drive =
        |format: Option<&str>| -> Result<Vec<(String, String)>, Box<dyn std::error::Error>> {
            let sut = FakeSut::start();
            sut.mount(
                Mock::given(method("POST"))
                    .and(path("/ehr/EHR-1/composition"))
                    .respond_with(ResponseTemplate::new(201)),
            );
            let set = crate::fake_sut::artifact_set_over_corpus(
                &[flat_commit_binding()],
                manifest.clone(),
                dir.path(),
            );
            let topology = ixit(&sut.base_url());
            let core = case(flat_commit_case(format));
            let step = core.flow.first().ok_or("the case declares no flow step")?;
            let mut driver = HttpDriver::new(&set, &topology, None)?;
            let mut vars = VarStore::default();
            driver.perform(&core, step, OutcomeKind::Created, 0, &mut vars)?;
            sent_headers(&sut)
        };

    let flat = drive(Some("wt-flat"))?;
    assert!(
        flat.contains(&(
            "content-type".to_owned(),
            "application/openehr.wt.flat+json".to_owned()
        )),
        "the flat commit does not label its payload: {flat:?}"
    );
    assert!(
        flat.contains(&(
            "openehr-template-id".to_owned(),
            "minimal.event.v1".to_owned()
        )),
        "the template identity is the corpus entry's own, not the corpus key: {flat:?}"
    );

    let canonical = drive(None)?;
    assert!(
        !canonical
            .iter()
            .any(|(name, _)| name == "openehr-template-id"),
        "the canonical representation needs no template header: {canonical:?}"
    );
    assert!(
        canonical.contains(&("content-type".to_owned(), "application/json".to_owned())),
        "a body-carrying request must label its payload: {canonical:?}"
    );
    Ok(())
}

/// A step's `scopes:` resolves on the same footing as its `with:` values, and
/// an entry that is not a scope STRING is a step-resolution failure — an
/// inconclusive row, never a request carrying a scope claim nobody wrote.
#[test]
#[expect(
    clippy::panic_in_result_fn,
    reason = "the Book's Result-test shape: assertions propagate with `?`"
)]
fn a_scope_entry_that_is_not_a_string_is_a_step_resolution_failure() -> Fallible {
    let sut = FakeSut::start();
    sut.mount(
        Mock::given(method("POST"))
            .and(path("/ehr"))
            .respond_with(ResponseTemplate::new(201)),
    );
    let mut document = create_ehr_case();
    let step = document
        .get_mut("flow")
        .and_then(|flow| flow.get_mut(0))
        .and_then(Value::as_object_mut)
        .ok_or("the case declares no flow step")?;
    step.insert("scopes".to_owned(), json!(["${captured_object}"]));

    let mut vars = VarStore::default();
    vars.set(
        veredictum::ids::CaptureName::parse("captured_object")?,
        veredictum::exec::state::Captured::Body(json!({ "scope": "user/*.r" })),
    );
    let observed = drive_one(
        &sut,
        &[create_ehr_binding()],
        document,
        OutcomeKind::Created,
        &mut vars,
    )?;
    match &observed.observation {
        Observation::Transport(reason) => {
            assert!(reason.contains("expected a scope string"), "{reason}");
        }
        other => panic!("a non-string scope must be inconclusive, got {other:?}"),
    }
    assert!(
        sut.requests().is_empty(),
        "nothing may reach the SUT once the step failed to resolve"
    );
    Ok(())
}
