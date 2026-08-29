// SPDX-FileCopyrightText: Veredictum contributors
// SPDX-License-Identifier: Apache-2.0

//! The `version` assertion family against the fake SUT.
//!
//! The family's facts live on the `ORIGINAL_VERSION` envelope and the
//! `REVISION_HISTORY` (RM common `UML/classes/original_version.adoc`,
//! `audit_details.adoc`, `revision_history_item.adoc`), so each test here
//! serves those representations on a real socket and checks that the declared
//! member is judged against what came back — and that a fact the ITS gives no
//! read for fails by name instead of passing.

use serde_json::{Value, json};
use veredictum::exec::StepDriver;
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

fn revision_history_binding() -> Value {
    json!({
        "sm_operation": "I_ITS_REST_REVISION_HISTORY.versioned_composition_revision_history",
        "its": "its-rest",
        "request": {
            "method": "GET",
            "path": "/ehr/{ehr_id}/versioned_composition/{versioned_object_uid}/revision_history"
        },
        "outcomes": { "ok": { "status": 200 } }
    })
}

/// One `ORIGINAL_VERSION` envelope as the canonical wire serves it.
fn envelope(tree: u32, change_code: &str, lifecycle_code: &str, lifecycle: &str) -> Value {
    json!({
        "_type": "ORIGINAL_VERSION",
        "uid": { "value": version_uid(tree) },
        "lifecycle_state": {
            "value": lifecycle,
            "defining_code": {
                "terminology_id": { "value": "openehr" },
                "code_string": lifecycle_code
            }
        },
        "commit_audit": {
            "change_type": {
                "value": "creation",
                "defining_code": {
                    "terminology_id": { "value": "openehr" },
                    "code_string": change_code
                }
            }
        }
    })
}

/// A create-then-assert case whose step declares the given version facts.
fn create_case(step_assertions: &Value) -> Value {
    json!({
        "id": "WIRE-version_assert", "kind": "functional", "component": "EHR_COMPOSITION",
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

fn mount_create(sut: &FakeSut) {
    sut.mount(
        Mock::given(method("POST"))
            .and(path(format!("/ehr/{EHR_ID}/composition")))
            .respond_with(
                ResponseTemplate::new(201)
                    .insert_header("ETag", format!("W/\"{}\"", version_uid(1)).as_str()),
            ),
    );
}

/// The driver percent-encodes every path parameter, so the `::` separators of
/// an `OBJECT_VERSION_ID` reach the socket escaped: the stub matches the
/// resource shape rather than a literal that would have to spell the escaping.
fn mount_envelope(sut: &FakeSut, served: Value) {
    sut.mount(
        Mock::given(method("GET"))
            .and(path_regex(format!(
                "^/ehr/{EHR_ID}/versioned_composition/{OBJECT_ID}/version/.+$"
            )))
            .respond_with(ResponseTemplate::new(200).set_body_json(served)),
    );
}

/// Drive the single step of a case and return its assertion failures.
fn drive(
    sut: &FakeSut,
    bindings: &[Value],
    case_document: Value,
    vars: &mut VarStore,
) -> Result<Vec<String>, Box<dyn std::error::Error>> {
    let set = artifact_set(bindings);
    let topology = ixit(&sut.base_url());
    let core = case(case_document);
    let step = core.flow.first().ok_or("the case declares no flow step")?;
    let mut driver = HttpDriver::new(&set, &topology, None)?;
    let observed = driver.perform(&core, step, OutcomeKind::Created, 0, vars)?;
    Ok(observed.assertion_failures)
}

/// Every declared envelope member is judged against the served
/// `ORIGINAL_VERSION`: `uid_pattern` against `uid.value`, `change_type`
/// against `commit_audit.change_type` (RM common `audit_details.adoc`), and
/// `lifecycle_state` against the served coded term
/// (`original_version.adoc` §Attributes).
#[test]
#[expect(
    clippy::panic_in_result_fn,
    reason = "the Book's Result-test shape: assertions panic, plumbing propagates with `?`"
)]
fn the_declared_members_are_judged_against_the_served_envelope() -> Fallible {
    let sut = FakeSut::start();
    mount_create(&sut);
    mount_envelope(&sut, envelope(1, "249", "532", "complete"));
    let mut vars = provisioned_vars()?;
    let failures = drive(
        &sut,
        &[
            create_composition_binding(),
            version_read_binding(),
            revision_history_binding(),
        ],
        create_case(&json!([{
            "assert": "version",
            "of": "${version_uid}",
            "change_type": "CREATE",
            "lifecycle_state": "openehr::532|complete|",
            "uid_pattern": "${versioned_object_uid}::<system>::1"
        }])),
        &mut vars,
    )?;
    assert_eq!(failures, Vec::<String>::new());
    Ok(())
}

/// A served envelope that carries another change type FAILS the row, and the
/// message names the served code — the whole point of the arm: the ninety
/// authored sites now earn their pass.
#[test]
#[expect(
    clippy::panic_in_result_fn,
    reason = "the Book's Result-test shape: assertions panic, plumbing propagates with `?`"
)]
fn a_served_envelope_that_contradicts_the_assertion_fails_the_row() -> Fallible {
    let sut = FakeSut::start();
    mount_create(&sut);
    // 523|deleted| where the case asserts CREATE.
    mount_envelope(&sut, envelope(1, "523", "523", "deleted"));
    let mut vars = provisioned_vars()?;
    let failures = drive(
        &sut,
        &[create_composition_binding(), version_read_binding()],
        create_case(&json!([{
            "assert": "version", "of": "${version_uid}", "change_type": "CREATE"
        }])),
        &mut vars,
    )?;
    let first = failures
        .first()
        .ok_or("the contradiction passed silently")?;
    assert!(
        first.contains("523") && first.contains("Create"),
        "the failure names neither side: {first}"
    );
    Ok(())
}

/// A `uid_pattern` whose `<n>` segment differs from the served `uid` fails:
/// the pattern's structural tokens carry the released `OBJECT_VERSION_ID`
/// grammar (BASE `base_types` master05 §Syntaxes), never a wildcard.
#[test]
#[expect(
    clippy::panic_in_result_fn,
    reason = "the Book's Result-test shape: assertions panic, plumbing propagates with `?`"
)]
fn a_uid_pattern_is_a_grammar_not_a_wildcard() -> Fallible {
    let sut = FakeSut::start();
    mount_create(&sut);
    mount_envelope(&sut, envelope(1, "249", "532", "complete"));
    let mut vars = provisioned_vars()?;
    let failures = drive(
        &sut,
        &[create_composition_binding(), version_read_binding()],
        create_case(&json!([{
            "assert": "version", "of": "${version_uid}",
            "uid_pattern": "${versioned_object_uid}::<system>::2"
        }])),
        &mut vars,
    )?;
    let first = failures.first().ok_or("a wrong version ordinal passed")?;
    assert!(first.contains("uid_pattern"), "{first}");
    Ok(())
}

/// A step whose own body IS the envelope is judged directly: the assertion
/// costs no second exchange, which is the shape the signature family already
/// drives (a version-envelope read step).
#[test]
#[expect(
    clippy::panic_in_result_fn,
    reason = "the Book's Result-test shape: assertions panic, plumbing propagates with `?`"
)]
fn an_envelope_already_in_hand_is_judged_without_a_second_read() -> Fallible {
    let sut = FakeSut::start();
    sut.mount(
        Mock::given(method("POST"))
            .and(path(format!("/ehr/{EHR_ID}/composition")))
            .respond_with(
                ResponseTemplate::new(201)
                    .insert_header("ETag", format!("W/\"{}\"", version_uid(1)).as_str())
                    .set_body_json(envelope(1, "249", "532", "complete")),
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
    assert_eq!(failures, Vec::<String>::new());
    assert_eq!(
        sut.requests().len(),
        1,
        "an envelope already in hand must not be re-read"
    );
    Ok(())
}

/// `count` is judged against the family's `REVISION_HISTORY`, one
/// `REVISION_HISTORY_ITEM` per version (RM common
/// `revision_history_item.adoc` §Description).
#[test]
#[expect(
    clippy::panic_in_result_fn,
    reason = "the Book's Result-test shape: assertions panic, plumbing propagates with `?`"
)]
fn the_version_count_comes_from_the_revision_history() -> Fallible {
    for (served_items, asserted, expect_failure) in [(2_usize, 2_u64, false), (2, 3, true)] {
        let sut = FakeSut::start();
        mount_create(&sut);
        let items: Vec<Value> = (1..=served_items)
            .map(|i| json!({ "version_id": { "value": format!("{OBJECT_ID}::{SYSTEM_ID}::{i}") } }))
            .collect();
        sut.mount(
            Mock::given(method("GET"))
                .and(path(format!(
                    "/ehr/{EHR_ID}/versioned_composition/{OBJECT_ID}/revision_history"
                )))
                .respond_with(ResponseTemplate::new(200).set_body_json(json!({ "items": items }))),
        );
        let mut vars = provisioned_vars()?;
        let failures = drive(
            &sut,
            &[create_composition_binding(), revision_history_binding()],
            create_case(&json!([{ "assert": "version", "count": asserted }])),
            &mut vars,
        )?;
        assert_eq!(
            !failures.is_empty(),
            expect_failure,
            "count {asserted} over {served_items} served items judged wrongly: {failures:?}"
        );
    }
    Ok(())
}

/// A version fact the released ITS-REST realizes no read for is a LOUD row
/// failure naming the family, never a silent pass: the directory family has
/// neither a VERSION envelope read nor a revision history.
#[test]
#[expect(
    clippy::panic_in_result_fn,
    reason = "the Book's Result-test shape: assertions panic, plumbing propagates with `?`"
)]
fn an_unreadable_version_fact_fails_loudly() -> Fallible {
    let sut = FakeSut::start();
    sut.mount(
        Mock::given(method("POST"))
            .and(path(format!("/ehr/{EHR_ID}/directory")))
            .respond_with(ResponseTemplate::new(201)),
    );
    let bindings = [json!({
        "sm_operation": "I_EHR_DIRECTORY.create_directory",
        "its": "its-rest",
        "request": { "method": "POST", "path": "/ehr/{ehr_id}/directory" },
        "outcomes": { "created": { "status": 201 } }
    })];
    let document = json!({
        "id": "WIRE-version_unreadable", "kind": "functional", "component": "EHR_DIRECTORY",
        "sm_operation": "I_EHR_DIRECTORY.create_directory",
        "test_purpose": "t", "description": "d", "spec_refs": [],
        "flow": [{
            "step": 1, "call": "create_directory", "expect": "created",
            "with": { "ehr_id": "${ehr_id}" },
            "assert": [{ "assert": "version", "count": 1 }]
        }]
    });
    let mut vars = provisioned_vars()?;
    let failures = drive(&sut, &bindings, document, &mut vars)?;
    let first = failures
        .first()
        .ok_or("an unreadable version fact passed silently")?;
    assert!(
        first.contains("REVISION_HISTORY") && first.contains("Directory"),
        "the failure names neither the read nor the family: {first}"
    );
    Ok(())
}
