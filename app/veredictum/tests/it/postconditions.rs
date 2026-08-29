// SPDX-FileCopyrightText: Veredictum contributors
// SPDX-License-Identifier: Apache-2.0

//! The row postcondition seam against the fake SUT (#239).
//!
//! A postcondition is the step-assertion dispatch applied to the row's last
//! completed step, so the families that a flow step judges are judged after
//! the flow too. The tests here serve real answers on a real socket and check
//! that a postcondition contradicting one is a row FAILURE — a family that
//! returned no finding regardless of what came back would manufacture a
//! passing row out of an unevaluated expectation.

use serde_json::{Value, json};
use veredictum::exec::{RowOutcome, StepDriver as _};
use veredictum::model::assertion::{Assertion, PostconditionRole};
use wiremock::matchers::{method, path};
use wiremock::{Mock, ResponseTemplate};

use crate::fake_sut::{FakeSut, artifact_set, case, ixit};

/// Anything a driver construction or a row can fail with, so a test body
/// propagates plumbing failures with `?`
/// (<https://doc.rust-lang.org/book/ch11-01-writing-tests.html>).
type Fallible = Result<(), Box<dyn std::error::Error>>;

/// The committed payload every case below sends: an `EHR_STATUS` body on the
/// `POST /ehr` create (ITS-REST `operations/ehr_create.yaml` takes an optional
/// `EHR_STATUS` request body).
fn committed_status() -> Value {
    json!({
        "_type": "EHR_STATUS",
        "name": { "_type": "DV_TEXT", "value": "EHR Status" },
        "is_queryable": true,
        "is_modifiable": true
    })
}

/// A parameter-less create binding carrying a body, so a row needs no
/// provisioned ground: `run_case` establishes the empty `requires` block and
/// drives the single step straight away.
fn create_ehr_binding() -> Value {
    json!({
        "sm_operation": "I_EHR_SERVICE.create_ehr",
        "its": "its-rest",
        "request": {
            "method": "POST",
            "path": "/ehr",
            "body": "ehr_status"
        },
        "outcomes": { "created": { "status": 201 } },
        "server_assigned": ["uid"]
    })
}

/// A one-step case committing [`committed_status`] with the given
/// postconditions.
fn case_document(postconditions: &Value) -> Value {
    json!({
        "id": "WIRE-postconditions", "kind": "functional", "component": "EHR",
        "sm_operation": "I_EHR_SERVICE.create_ehr",
        "test_purpose": "t", "description": "d", "spec_refs": [],
        "flow": [{
            "step": 1, "call": "create_ehr", "expect": "created",
            "with": { "ehr_status": committed_status() }
        }],
        "postconditions": postconditions
    })
}

/// Drive the whole one-row case and return its row outcome.
fn row_of(served: Value, postconditions: &Value) -> Result<RowOutcome, Box<dyn std::error::Error>> {
    let sut = FakeSut::start();
    sut.mount(
        Mock::given(method("POST"))
            .and(path("/ehr"))
            .respond_with(ResponseTemplate::new(201).set_body_json(served)),
    );
    let set = artifact_set(&[create_ehr_binding()]);
    let topology = ixit(&sut.base_url());
    let core = case(case_document(postconditions));
    let mut driver = veredictum::exec::driver::HttpDriver::new(&set, &topology, None)?;
    let record = veredictum::exec::run_case(&core, None, &mut driver)?;
    record
        .rows
        .into_iter()
        .next()
        .ok_or_else(|| "the case drove no row".into())
}

/// An `equivalent` postcondition is JUDGED against what the row's last step
/// served: the committed content modulo the binding's declared
/// server-assigned set holds, and a served body that differs anywhere else
/// fails the row. The catalogue's one `equivalent` postcondition sits on a
/// flow with no read step, so a seam that skipped the family reported a pass
/// nobody earned.
#[test]
#[expect(
    clippy::panic_in_result_fn,
    reason = "the Book's Result-test shape: assertions panic, plumbing propagates with `?`"
)]
fn an_equivalent_postcondition_judges_the_row_s_last_served_body() -> Fallible {
    let postconditions = json!([
        { "assert": "equivalent", "to": "committed", "ignoring": "server_assigned" }
    ]);

    let mut echoed = committed_status();
    echoed["uid"] = json!({ "_type": "HIER_OBJECT_ID", "value": "server-minted" });
    let outcome = row_of(echoed, &postconditions)?;
    assert_eq!(
        outcome,
        RowOutcome::Passed,
        "the served content differs only in the declared server-assigned member"
    );

    let mut diverged = committed_status();
    diverged["is_queryable"] = json!(false);
    match row_of(diverged, &postconditions)? {
        RowOutcome::Failed { reason, .. } => {
            assert!(reason.contains("equivalent"), "{reason}");
        }
        other => panic!("a divergent served body must fail the row, got {other:?}"),
    }
    Ok(())
}

/// The same for the four other families the seam used to drop: each judges
/// the row's last served body, and each contradicted expectation fails the
/// row instead of passing unevaluated.
#[test]
#[expect(
    clippy::panic_in_result_fn,
    reason = "the Book's Result-test shape: assertions panic, plumbing propagates with `?`"
)]
fn the_body_judging_families_all_bite_as_postconditions() -> Fallible {
    let contradicted = [
        json!([{ "assert": "instance_of", "rm_type": "COMPOSITION" }]),
        json!([{ "assert": "returns", "matches": "never-in-this-body" }]),
        json!([{ "assert": "xml_root", "name": "composition" }]),
        json!([{ "assert": "result_set", "match": "count", "count": 7 }]),
    ];
    for postconditions in contradicted {
        let outcome = row_of(committed_status(), &postconditions)?;
        assert!(
            matches!(outcome, RowOutcome::Failed { .. }),
            "{postconditions} passed unevaluated: {outcome:?}"
        );
    }
    Ok(())
}

/// The informative families stay informative after the flow: neither is a
/// pass/fail criterion, so neither can turn a green row red.
#[test]
#[expect(
    clippy::panic_in_result_fn,
    reason = "the Book's Result-test shape: assertions panic, plumbing propagates with `?`"
)]
fn the_informative_families_never_judge_a_row() -> Fallible {
    let outcome = row_of(
        committed_status(),
        &json!([
            { "assert": "state", "text": "an EHR exists" },
            { "assert": "message_exemplar", "text": "EHR with <ehr_id> does not exist" }
        ]),
    )?;
    assert_eq!(outcome, RowOutcome::Passed);
    Ok(())
}

/// Every authored family carries a role, and the eight verdict-bearing ones
/// are the eight the seam judges. A family that fell out of this
/// classification is exactly the silent pass #239 closed.
#[test]
fn every_assertion_family_carries_a_postcondition_role() {
    let families = [
        (
            json!({ "assert": "instance_of", "rm_type": "COMPOSITION" }),
            PostconditionRole::Judged,
        ),
        (
            json!({ "assert": "field", "path": "uid", "exists": true }),
            PostconditionRole::Judged,
        ),
        (
            json!({ "assert": "equivalent", "to": "committed" }),
            PostconditionRole::Judged,
        ),
        (
            json!({ "assert": "signature", "of": "${v}", "present": true }),
            PostconditionRole::Judged,
        ),
        (
            json!({ "assert": "version", "count": 2 }),
            PostconditionRole::Judged,
        ),
        (
            json!({ "assert": "result_set", "match": "count", "count": 1 }),
            PostconditionRole::Judged,
        ),
        (
            json!({ "assert": "returns", "matches": "x" }),
            PostconditionRole::Judged,
        ),
        (
            json!({ "assert": "xml_root", "name": "composition" }),
            PostconditionRole::Judged,
        ),
        (
            json!({ "assert": "unique", "over": "${x}", "aggregate": true }),
            PostconditionRole::Aggregate,
        ),
        (
            json!({ "assert": "message_exemplar", "text": "t" }),
            PostconditionRole::Informative,
        ),
        (
            json!({ "assert": "state", "text": "t" }),
            PostconditionRole::Informative,
        ),
    ];
    for (document, role) in families {
        let assertion: Assertion =
            serde_json::from_value(document.clone()).unwrap_or_else(|e| panic!("{document}: {e}"));
        assert_eq!(
            assertion.postcondition_role(),
            role,
            "{} is classified wrongly",
            assertion.family()
        );
    }
}

/// A row that completed NO flow step served nothing to judge a postcondition
/// against, and that is INCONCLUSIVE — a family reporting no finding over an
/// exchange that never happened would manufacture a passing row out of an
/// unevaluated expectation.
#[test]
#[expect(
    clippy::panic_in_result_fn,
    reason = "the Book's Result-test shape: assertions panic, plumbing propagates with `?`"
)]
fn a_row_that_completed_no_step_leaves_its_postconditions_unjudgeable() -> Fallible {
    let judged = json!([
        { "assert": "field", "path": "uid/value", "exists": true },
        { "assert": "instance_of", "rm_type": "EHR" }
    ]);
    let sut = FakeSut::start();
    let set = artifact_set(&[create_ehr_binding()]);
    let topology = ixit(&sut.base_url());
    let core = case(case_document(&judged));
    let mut driver = veredictum::exec::driver::HttpDriver::new(&set, &topology, None)?;
    let mut vars = veredictum::exec::state::VarStore::default();
    let outcomes = driver.postconditions(&core, 0, &mut vars)?;
    assert_eq!(
        outcomes.failures.len(),
        2,
        "one unjudgeable outcome per judged family: {:?}",
        outcomes.failures
    );
    for failure in &outcomes.failures {
        assert!(
            failure.reason().contains("completed no flow step"),
            "{failure:?}"
        );
    }
    assert!(outcomes.advisories.is_empty());

    // A case with no JUDGED postcondition asks nothing of the last step, so
    // the seam reports nothing rather than an absence.
    let informative = json!([{ "assert": "message_exemplar", "text": "t" }]);
    let core = case(case_document(&informative));
    let outcomes = driver.postconditions(&core, 0, &mut vars)?;
    assert!(outcomes.failures.is_empty(), "{:?}", outcomes.failures);
    Ok(())
}

/// `unique` is AGGREGATE: it is judged across the whole parameter matrix
/// rather than inside one row, so two rows binding the same value fail while
/// distinct bindings pass. A per-row evaluation could never see the
/// duplication at all.
#[test]
#[expect(
    clippy::panic_in_result_fn,
    reason = "the Book's Result-test shape: assertions panic, plumbing propagates with `?`"
)]
fn the_unique_aggregate_is_judged_across_the_rows_not_inside_one() -> Fallible {
    let sut = FakeSut::start();
    let set = artifact_set(&[create_ehr_binding()]);
    let topology = ixit(&sut.base_url());
    let core = case(case_document(&json!([
        { "assert": "unique", "over": "${minted}", "aggregate": true }
    ])));
    let mut driver = veredictum::exec::driver::HttpDriver::new(&set, &topology, None)?;

    let row =
        |value: &str| -> Result<veredictum::exec::state::VarStore, Box<dyn std::error::Error>> {
            let mut vars = veredictum::exec::state::VarStore::default();
            vars.set(
                veredictum::ids::CaptureName::parse("minted")?,
                veredictum::exec::state::Captured::Scalar(value.to_owned()),
            );
            Ok(vars)
        };
    let distinct = [row("uid-1")?, row("uid-2")?];
    assert!(
        driver.aggregates(&core, &distinct)?.is_empty(),
        "two distinct bindings satisfy uniqueness"
    );

    let repeated = [row("uid-1")?, row("uid-1")?];
    let failures = driver.aggregates(&core, &repeated)?;
    let first = failures
        .first()
        .ok_or("a repeated binding must fail the aggregate")?;
    assert!(first.contains("uid-1"), "{first}");
    Ok(())
}
