// SPDX-FileCopyrightText: Veredictum contributors
// SPDX-License-Identifier: Apache-2.0

//! A response body the driver cannot parse as JSON, against the fake SUT.
//!
//! Services MUST support at least one of the openEHR XML or JSON canonical
//! formats, and the two are separate bound document forms: canonical XML
//! conforms to the published XSDs, canonical JSON spells every RM attribute
//! as a lowercase `snake_case` member (ITS-REST
//! `specifications/docs/overview/Resources.md` §Data representation). This
//! runner parses the JSON binding only, so an XML-negotiated read reaches the
//! member-addressing assertion families as a body they cannot address. Every
//! test here pins the honest answer: the INCONCLUSIVE channel naming the
//! served media type, never a conformance finding against the server.

use serde_json::{Value, json};
use veredictum::exec::StepDriver;
use veredictum::exec::assertions::AssertionOutcome;
use veredictum::exec::driver::HttpDriver;
use veredictum::exec::state::{Captured, VarStore};
use veredictum::ids::CaptureName;
use veredictum::vocab::OutcomeKind;
use wiremock::matchers::{method, path, path_regex};
use wiremock::{Mock, ResponseTemplate};

use crate::fake_sut::{FakeSut, artifact_set, case, ixit};

/// Anything a driver construction or a step can fail with, so a test body
/// propagates plumbing failures with `?`
/// (<https://doc.rust-lang.org/book/ch11-01-writing-tests.html>).
type Fallible = Result<(), Box<dyn std::error::Error>>;

const EHR_ID: &str = "7d44b88c-4199-4bad-97dc-d78268e01398";
const OBJECT_ID: &str = "8849182c-82ad-4088-a07f-48ead4180515";
const SYSTEM_ID: &str = "cdr.example.org";
const XML: &str = "application/xml";

/// A canonical-XML COMPOSITION, as a service answering `Accept:
/// application/xml` serves it: well-formed, namespace-qualified, and
/// addressing none of the JSON member names the catalogue's field paths use.
const COMPOSITION_XML: &str = concat!(
    "<composition xmlns=\"http://schemas.openehr.org/v1\" ",
    "xmlns:xsi=\"http://www.w3.org/2001/XMLSchema-instance\" ",
    "xsi:type=\"COMPOSITION\">",
    "<context><setting><value>other care</value></setting></context>",
    "</composition>"
);

fn version_uid(tree: u32) -> String {
    format!("{OBJECT_ID}::{SYSTEM_ID}::{tree}")
}

fn create_composition_binding() -> Value {
    json!({
        "sm_operation": "I_EHR_COMPOSITION.create_composition",
        "its": "its-rest",
        "request": { "method": "POST", "path": "/ehr/{ehr_id}/composition" },
        "outcomes": { "created": { "status": 201 } },
        "captures": {
            "version_uid": { "from": "header ETag", "strip": "weak-quotes" }
        }
    })
}

fn version_read_binding() -> Value {
    json!({
        "sm_operation": "I_EHR_COMPOSITION.get_versioned_composition",
        "its": "its-rest",
        "variant": "version",
        "request": {
            "method": "GET",
            "path": "/ehr/{ehr_id}/versioned_composition/{versioned_object_uid}/version/{version_uid}"
        },
        "outcomes": { "ok": { "status": 200 } }
    })
}

/// A create-then-assert case whose step declares the given assertions.
fn create_case(step_assertions: &Value) -> Value {
    json!({
        "id": "WIRE-unparsed_body", "kind": "functional", "component": "EHR_COMPOSITION",
        "sm_operation": "I_EHR_COMPOSITION.create_composition",
        "test_purpose": "t", "description": "d", "spec_refs": [],
        "flow": [{
            "step": 1, "call": "create_composition", "expect": "created",
            "with": { "ehr_id": "${ehr_id}" },
            "capture": { "version_uid": "created.version_uid" },
            "assert": step_assertions.clone()
        }]
    })
}

/// The two identities the case's own provisioning would have bound.
fn provisioned_vars() -> Result<VarStore, Box<dyn std::error::Error>> {
    let mut vars = VarStore::default();
    vars.set(
        CaptureName::parse("ehr_id")?,
        Captured::Scalar(EHR_ID.to_owned()),
    );
    vars.set(
        CaptureName::parse("versioned_object_uid")?,
        Captured::Scalar(OBJECT_ID.to_owned()),
    );
    Ok(vars)
}

/// The create step, answering 201 with the identifying `ETag` and the given
/// body under the given media type.
fn mount_create(sut: &FakeSut, body: &str, media_type: &str) {
    sut.mount(
        Mock::given(method("POST"))
            .and(path(format!("/ehr/{EHR_ID}/composition")))
            .respond_with(
                ResponseTemplate::new(201)
                    .insert_header("ETag", format!("W/\"{}\"", version_uid(1)).as_str())
                    .set_body_raw(body.as_bytes().to_vec(), media_type),
            ),
    );
}

/// Drive the single step of a case and return its channelled assertion
/// outcomes.
fn drive(
    sut: &FakeSut,
    bindings: &[Value],
    case_document: Value,
    vars: &mut VarStore,
) -> Result<Vec<AssertionOutcome>, Box<dyn std::error::Error>> {
    let set = artifact_set(bindings);
    let topology = ixit(&sut.base_url());
    let core = case(case_document);
    let step = core.flow.first().ok_or("the case declares no flow step")?;
    let mut driver = HttpDriver::new(&set, &topology, None)?;
    let observed = driver.perform(&core, step, OutcomeKind::Created, 0, vars)?;
    Ok(observed.assertion_failures)
}

/// A `field` assertion over an XML-negotiated body takes the INCONCLUSIVE
/// channel and names the served media type. Reporting the RM attribute
/// absent would attribute the runner's own JSON-only parse to the server,
/// which is the misattribution the three-way law exists to prevent.
#[test]
#[expect(
    clippy::panic_in_result_fn,
    reason = "the Book's Result-test shape: assertions panic, plumbing propagates with `?`"
)]
fn a_field_assertion_over_an_xml_body_is_inconclusive_by_name() -> Fallible {
    let sut = FakeSut::start();
    mount_create(&sut, COMPOSITION_XML, XML);
    let mut vars = provisioned_vars()?;
    let failures = drive(
        &sut,
        &[create_composition_binding()],
        create_case(&json!([{
            "assert": "field", "path": "context/setting/value", "exists": true
        }])),
        &mut vars,
    )?;
    let first = failures.first().ok_or("the XML body passed silently")?;
    assert!(
        matches!(first, AssertionOutcome::Unjudgeable(_)),
        "an unparsed body is the instrument's limit, never a finding against the SUT: {first:?}"
    );
    let reason = first.reason();
    assert!(
        reason.contains(XML),
        "the refusal names no media type: {reason}"
    );
    assert!(
        !reason.contains("carries no"),
        "the silent absent-member wording survived: {reason}"
    );
    Ok(())
}

/// The `version` family resolves its own `ORIGINAL_VERSION` envelope, so an
/// XML-negotiated case refuses at that READ: the envelope comes back in the
/// other document form and `change_type` is unjudgeable, not contradicted.
#[test]
#[expect(
    clippy::panic_in_result_fn,
    reason = "the Book's Result-test shape: assertions panic, plumbing propagates with `?`"
)]
fn a_version_assertion_refuses_an_xml_envelope_read() -> Fallible {
    let sut = FakeSut::start();
    mount_create(&sut, COMPOSITION_XML, XML);
    sut.mount(
        Mock::given(method("GET"))
            .and(path_regex(format!(
                "^/ehr/{EHR_ID}/versioned_composition/{OBJECT_ID}/version/.+$"
            )))
            .respond_with(
                ResponseTemplate::new(200).set_body_raw(COMPOSITION_XML.as_bytes().to_vec(), XML),
            ),
    );
    let mut vars = provisioned_vars()?;
    let failures = drive(
        &sut,
        &[create_composition_binding(), version_read_binding()],
        create_case(&json!([{
            "assert": "version", "of": "${version_uid}", "change_type": "CREATE"
        }])),
        &mut vars,
    )?;
    let first = failures.first().ok_or("the XML envelope passed silently")?;
    assert!(
        matches!(first, AssertionOutcome::Unjudgeable(_)),
        "an unreadable envelope is inconclusive, never a finding against the SUT: {first:?}"
    );
    let reason = first.reason();
    assert!(
        reason.contains(XML),
        "the refusal names no media type: {reason}"
    );
    Ok(())
}

/// The identity half of the same assertion still GATES: `uid_pattern` is
/// judged off the resolved `OBJECT_VERSION_ID`, which the `ETag` carried, so
/// an XML body costs no coverage where no member is addressed.
#[test]
#[expect(
    clippy::panic_in_result_fn,
    reason = "the Book's Result-test shape: assertions panic, plumbing propagates with `?`"
)]
fn a_uid_pattern_still_gates_on_an_xml_body() -> Fallible {
    let sut = FakeSut::start();
    mount_create(&sut, COMPOSITION_XML, XML);
    let mut vars = provisioned_vars()?;
    let failures = drive(
        &sut,
        &[create_composition_binding()],
        create_case(&json!([{
            "assert": "version", "of": "${version_uid}",
            "uid_pattern": "${versioned_object_uid}::<system>::2"
        }])),
        &mut vars,
    )?;
    let first = failures.first().ok_or("a wrong version ordinal passed")?;
    assert!(
        matches!(first, AssertionOutcome::Mismatch(_)),
        "the served uid missed the pattern, which is a finding against the SUT: {first:?}"
    );
    assert!(first.reason().contains("uid_pattern"), "{first:?}");
    Ok(())
}

/// A JSON body is unaffected: the same field assertion is judged against the
/// served document, and a contradiction is a MISMATCH — the refusal above
/// narrows nothing on the JSON path.
#[test]
#[expect(
    clippy::panic_in_result_fn,
    reason = "the Book's Result-test shape: assertions panic, plumbing propagates with `?`"
)]
fn a_json_body_is_still_judged_member_by_member() -> Fallible {
    let sut = FakeSut::start();
    mount_create(
        &sut,
        &json!({ "_type": "COMPOSITION" }).to_string(),
        "application/json",
    );
    let mut vars = provisioned_vars()?;
    let failures = drive(
        &sut,
        &[create_composition_binding()],
        create_case(&json!([{
            "assert": "field", "path": "context/setting/value", "exists": true
        }])),
        &mut vars,
    )?;
    let first = failures
        .first()
        .ok_or("the missing member passed silently")?;
    assert!(
        matches!(first, AssertionOutcome::Mismatch(_)),
        "a member missing from a JSON body is a fact about the SUT: {first:?}"
    );
    Ok(())
}
