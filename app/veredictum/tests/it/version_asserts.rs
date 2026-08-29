// SPDX-FileCopyrightText: Veredictum contributors
// SPDX-License-Identifier: Apache-2.0

//! The `version` assertion family against the fake SUT.
//!
//! The family's facts live on the `ORIGINAL_VERSION` envelope and the
//! `REVISION_HISTORY` (RM common `UML/classes/original_version.adoc`,
//! `audit_details.adoc`, `revision_history_item.adoc`), so each test here
//! serves those representations on a real socket and checks that the declared
//! member is judged against what came back — and that a fact the ITS gives no
//! read for is INCONCLUSIVE by name instead of passing silently or being
//! charged to the server (#237).

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
    assert!(failures.is_empty(), "{failures:?}");
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
        matches!(first, AssertionOutcome::Mismatch(_)),
        "a served value that contradicts the assertion is a finding against the SUT: {first:?}"
    );
    let reason = first.reason();
    assert!(
        reason.contains("523") && reason.contains("Create"),
        "the failure names neither side: {reason}"
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
    assert!(
        matches!(first, AssertionOutcome::Mismatch(_)),
        "a served uid that misses the pattern is a finding against the SUT: {first:?}"
    );
    assert!(first.reason().contains("uid_pattern"), "{first:?}");
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
    assert!(failures.is_empty(), "{failures:?}");
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
        // A served history that holds another number of items IS a finding
        // against the server: the value it served contradicts the assertion.
        assert!(
            failures
                .iter()
                .all(|f| matches!(f, AssertionOutcome::Mismatch(_))),
            "a served count difference is a conformance finding: {failures:?}"
        );
    }
    Ok(())
}

/// A version fact the released ITS-REST realizes no read for is LOUD and
/// INCONCLUSIVE, never a silent pass and never a finding against the server:
/// the directory family has neither a VERSION envelope read nor a revision
/// history, which is a gap of the ITS and the catalogue.
///
/// The channel was adjudicated in #237 — this test pinned `Failed` when the
/// family gap first became loud, and `Failed` reads as a conformance finding
/// the run never proved.
#[test]
#[expect(
    clippy::panic_in_result_fn,
    reason = "the Book's Result-test shape: assertions panic, plumbing propagates with `?`"
)]
fn an_unreadable_version_fact_is_inconclusive_never_a_sut_finding() -> Fallible {
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
        matches!(first, AssertionOutcome::Unjudgeable(_)),
        "an ITS realization gap is inconclusive, never a SUT finding: {first:?}"
    );
    let reason = first.reason();
    assert!(
        reason.contains("REVISION_HISTORY") && reason.contains("Directory"),
        "the failure names neither the read nor the family: {reason}"
    );
    Ok(())
}

/// The channel each assertion outcome routes to, pinned at the ROW level:
/// `run_case` is where a finding is charged to the SUT or recorded
/// inconclusive, so the routing is proven there rather than at the
/// assertion's own return type (#237).
///
/// The bindings below carry parameter-less paths so the rows need no
/// provisioned ground: `run_case` establishes an empty `requires` block and
/// drives the single step straight away.
fn row_of(
    sut: &FakeSut,
    bindings: &[Value],
    case_document: Value,
) -> Result<veredictum::exec::RowOutcome, Box<dyn std::error::Error>> {
    let set = artifact_set(bindings);
    let topology = ixit(&sut.base_url());
    let core = case(case_document);
    let mut driver = HttpDriver::new(&set, &topology, None)?;
    let record = veredictum::exec::run_case(&core, None, &mut driver)?;
    record
        .rows
        .into_iter()
        .next()
        .ok_or_else(|| "the case drove no row".into())
}

/// An assertion the released ITS gives no read for records the row
/// INCONCLUSIVE: the directory family has no `REVISION_HISTORY` read, which
/// is a gap of the ITS and of the catalogue, so a finding charged to the
/// server here would name a value the server was never asked for (#237).
#[test]
#[expect(
    clippy::panic_in_result_fn,
    reason = "the Book's Result-test shape: assertions panic, plumbing propagates with `?`"
)]
fn an_its_realization_gap_errors_the_row() -> Fallible {
    let sut = FakeSut::start();
    sut.mount(
        Mock::given(method("POST"))
            .and(path("/directory"))
            .respond_with(ResponseTemplate::new(201)),
    );
    let row = row_of(
        &sut,
        &[json!({
            "sm_operation": "I_EHR_DIRECTORY.create_directory",
            "its": "its-rest",
            "request": { "method": "POST", "path": "/directory" },
            "outcomes": { "created": { "status": 201 } }
        })],
        json!({
            "id": "WIRE-version_row_errored", "kind": "functional", "component": "EHR_DIRECTORY",
            "sm_operation": "I_EHR_DIRECTORY.create_directory",
            "test_purpose": "t", "description": "d", "spec_refs": [],
            "flow": [{
                "step": 1, "call": "create_directory", "expect": "created",
                "assert": [{ "assert": "version", "count": 1 }]
            }]
        }),
    )?;
    match row {
        veredictum::exec::RowOutcome::Errored { step, ref reason } => {
            assert_eq!(step, 1);
            assert!(
                reason.contains("REVISION_HISTORY") && reason.contains("Directory"),
                "the inconclusive row names neither the read nor the family: {reason}"
            );
        }
        other => panic!("an unjudgeable assertion must not be a SUT finding: {other:?}"),
    }
    Ok(())
}

/// A served envelope whose `commit_audit.change_type` contradicts the
/// asserted class FAILS the row: the server supplied the value the finding is
/// about, which is exactly what a conformance finding needs.
#[test]
#[expect(
    clippy::panic_in_result_fn,
    reason = "the Book's Result-test shape: assertions panic, plumbing propagates with `?`"
)]
fn a_served_change_type_that_contradicts_the_assertion_fails_the_row() -> Fallible {
    let sut = FakeSut::start();
    sut.mount(
        Mock::given(method("POST"))
            .and(path("/composition"))
            .respond_with(
                ResponseTemplate::new(201)
                    .insert_header("ETag", format!("W/\"{}\"", version_uid(1)).as_str()),
            ),
    );
    sut.mount(
        Mock::given(method("GET"))
            .and(path_regex("^/versioned_composition/version/.+$"))
            // 523|deleted| where the case asserts CREATE.
            .respond_with(
                ResponseTemplate::new(200).set_body_json(envelope(1, "523", "523", "deleted")),
            ),
    );
    let row = row_of(
        &sut,
        &[
            json!({
                "sm_operation": "I_EHR_COMPOSITION.create_composition",
                "its": "its-rest",
                "request": { "method": "POST", "path": "/composition" },
                "outcomes": { "created": { "status": 201 } },
                "captures": {
                    "version_uid": { "from": "header ETag", "strip": "weak-quotes" }
                }
            }),
            json!({
                "sm_operation": "I_EHR_COMPOSITION.get_versioned_composition",
                "its": "its-rest",
                "variant": "version",
                "request": {
                    "method": "GET",
                    "path": "/versioned_composition/version/{version_uid}"
                },
                "outcomes": { "ok": { "status": 200 } }
            }),
        ],
        json!({
            "id": "WIRE-version_row_failed", "kind": "functional", "component": "EHR_COMPOSITION",
            "sm_operation": "I_EHR_COMPOSITION.create_composition",
            "test_purpose": "t", "description": "d", "spec_refs": [],
            "flow": [{
                "step": 1, "call": "create_composition", "expect": "created",
                "capture": { "version_uid": "created.version_uid" },
                "assert": [{
                    "assert": "version", "of": "${version_uid}", "change_type": "CREATE"
                }]
            }]
        }),
    )?;
    match row {
        veredictum::exec::RowOutcome::Failed { step, ref reason } => {
            assert_eq!(step, 1);
            assert!(
                reason.contains("523") && reason.contains("Create"),
                "the finding names neither side: {reason}"
            );
        }
        other => panic!("a served contradiction is a SUT finding: {other:?}"),
    }
    Ok(())
}

/// The control: a plain field mismatch is untouched by the channel split and
/// still FAILS the row — the inconclusive channel takes only assertions the
/// run could not judge, never a served value that simply differs.
#[test]
#[expect(
    clippy::panic_in_result_fn,
    reason = "the Book's Result-test shape: assertions panic, plumbing propagates with `?`"
)]
fn a_field_mismatch_still_fails_the_row() -> Fallible {
    let sut = FakeSut::start();
    sut.mount(
        Mock::given(method("GET"))
            .and(path("/composition"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({ "_type": "EHR" }))),
    );
    let row = row_of(
        &sut,
        &[json!({
            "sm_operation": "I_EHR_COMPOSITION.get_composition",
            "its": "its-rest",
            "request": { "method": "GET", "path": "/composition" },
            "outcomes": { "ok": { "status": 200 } }
        })],
        json!({
            "id": "WIRE-field_row_failed", "kind": "functional", "component": "EHR_COMPOSITION",
            "sm_operation": "I_EHR_COMPOSITION.get_composition",
            "test_purpose": "t", "description": "d", "spec_refs": [],
            "flow": [{
                "step": 1, "call": "get_composition", "expect": "ok",
                "assert": [{
                    "assert": "field", "path": "_type", "equals": "COMPOSITION"
                }]
            }]
        }),
    )?;
    match row {
        veredictum::exec::RowOutcome::Failed { step, ref reason } => {
            assert_eq!(step, 1);
            assert!(
                reason.contains("_type"),
                "the finding names no path: {reason}"
            );
        }
        other => panic!("a served field mismatch is a SUT finding: {other:?}"),
    }
    Ok(())
}
