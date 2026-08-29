// SPDX-FileCopyrightText: Veredictum contributors
// SPDX-License-Identifier: Apache-2.0

//! Directory provisioning over a real corpus, against the fake SUT.
//!
//! A FOLDER tree that classifies committed compositions can only be posted
//! once those compositions exist, so its `OBJECT_REF` item ids are written as
//! `${committed_object_id_<k>}` references and rendered from the same run's
//! commit set. These tests pin the two halves that make that sound: the
//! ordering of the provisioning exchanges, and the index-addressed selection
//! specs the `result_set` comparator evaluates over the captured uid list.

use assert_fs::prelude::{FileWriteStr as _, PathChild as _};
use serde_json::{Value, json};
use veredictum::artifacts::ArtifactSet;
use veredictum::exec::driver::HttpDriver;
use veredictum::exec::outcome::Observation;
use veredictum::exec::state::{Captured, VarStore};
use veredictum::exec::{Provisioned, StepDriver, StepObservation};
use veredictum::ids::CaptureName;
use veredictum::vocab::OutcomeKind;
use wiremock::matchers::{body_string_contains, method, path};
use wiremock::{Mock, ResponseTemplate};

use crate::fake_sut::{FakeSut, artifact_set_over_corpus, case, ixit};

/// Anything a driver construction, a corpus write or a step can fail with, so
/// a test body propagates plumbing failures with `?`
/// (<https://doc.rust-lang.org/book/ch11-01-writing-tests.html>).
type Fallible = Result<(), Box<dyn std::error::Error>>;

/// The provisioned EHR the fake SUT hands back.
const EHR: &str = "EHR-folder-provisioning";

/// The version uid the first committed composition is answered with.
const UID_0: &str = "aaaaaaaa-1111-4111-8111-111111111111::sut.example::1";
/// The version uid the second committed composition is answered with.
const UID_1: &str = "bbbbbbbb-2222-4222-8222-222222222222::sut.example::1";

/// The three bindings a directory precondition drives: mint the EHR, commit
/// the set, then post the FOLDER tree. Each carries the `ETag` capture the
/// provisioning reads its uid out of (ITS-REST `Requests_and_responses.md`
/// §`ETag` and Last-Modified: the value carries a `W/` weakness indicator).
fn provisioning_bindings() -> Vec<Value> {
    vec![
        json!({
            "sm_operation": "I_EHR_SERVICE.create_ehr",
            "its": "its-rest",
            "request": { "method": "POST", "path": "/ehr" },
            "outcomes": { "created": { "status": 201 } },
            "captures": { "ehr_id": { "from": "header Location last-segment" } }
        }),
        json!({
            "sm_operation": "I_EHR_COMPOSITION.create_composition",
            "its": "its-rest",
            "request": { "method": "POST", "path": "/ehr/{ehr_id}/composition" },
            "outcomes": { "created": { "status": 201 } },
            "captures": { "version_uid": { "from": "header ETag", "strip": "weak-quotes" } }
        }),
        json!({
            "sm_operation": "I_EHR_DIRECTORY.create_directory",
            "its": "its-rest",
            "request": { "method": "POST", "path": "/ehr/{ehr_id}/directory" },
            "outcomes": { "created": { "status": 201 } },
            "captures": { "version_uid": { "from": "header ETag", "strip": "weak-quotes" } }
        }),
    ]
}

/// The two-composition commit set the directory tree classifies. The names
/// differ so the fake SUT can answer each post with its own version uid.
fn commit_set() -> Value {
    json!([
        {
            "_type": "COMPOSITION",
            "name": { "_type": "DV_TEXT", "value": "committed-one" },
            "archetype_node_id": "openEHR-EHR-COMPOSITION.encounter.v1"
        },
        {
            "_type": "COMPOSITION",
            "name": { "_type": "DV_TEXT", "value": "committed-two" },
            "archetype_node_id": "openEHR-EHR-COMPOSITION.encounter.v1"
        }
    ])
}

/// A root FOLDER whose two `items` `OBJECT_REF`s name committed objects by
/// index (RM common `folder.adoc` §Attributes: `items: List<OBJECT_REF>`).
fn folder_tree(first: &str, second: &str) -> Value {
    json!({
        "_type": "FOLDER",
        "name": { "_type": "DV_TEXT", "value": "root" },
        "archetype_node_id": "openEHR-EHR-FOLDER.generic.v1",
        "items": [
            {
                "_type": "OBJECT_REF",
                "namespace": "local",
                "type": "VERSIONED_COMPOSITION",
                "id": { "_type": "HIER_OBJECT_ID", "value": first }
            },
            {
                "_type": "OBJECT_REF",
                "namespace": "local",
                "type": "VERSIONED_COMPOSITION",
                "id": { "_type": "HIER_OBJECT_ID", "value": second }
            }
        ]
    })
}

/// The manifest over the two written fixtures, with the folder-containment
/// views declared on the tree (the driver answers them by name).
fn manifest() -> Value {
    json!({
        "cnf.test.set": {
            "source": "set.json",
            "format": "canonical-json",
            "validity": { "verdict": "valid" },
            "provenance": "authored in-test: the two-composition commit set"
        },
        "cnf.test.tree": {
            "source": "tree.json",
            "format": "canonical-json",
            "validity": { "verdict": "valid" },
            "provenance": "authored in-test: the FOLDER tree over the commit set",
            "views": {
                "f2_scoped_uids": { "select": "the committed uid folder f2 references" },
                "folder_composition_pairs": { "select": "every (folder, committed uid) pair" }
            }
        }
    })
}

/// Write the corpus the resolver reads: the commit set plus a tree naming the
/// two committed objects.
fn corpus(tree: &Value) -> Result<assert_fs::TempDir, Box<dyn std::error::Error>> {
    let dir = assert_fs::TempDir::new()?;
    dir.child("set.json").write_str(&commit_set().to_string())?;
    dir.child("tree.json").write_str(&tree.to_string())?;
    Ok(dir)
}

/// The case whose preconditions are the commit set and the directory tree.
fn directory_case() -> Value {
    json!({
        "id": "WIRE-directory_tree", "kind": "functional", "component": "EHR",
        "sm_operation": "I_EHR_DIRECTORY.create_directory",
        "requires": {
            "ehr": { "commits": "none" },
            "commit": ["cnf.test.set"],
            "directory": "cnf.test.tree"
        },
        "test_purpose": "t", "description": "d", "spec_refs": [],
        "flow": [{ "step": 1, "call": "create_directory", "expect": "created" }]
    })
}

/// Mount the provisioning answers: the EHR, one distinct version uid per
/// committed composition (matched on the composition's own name, so the
/// pairing never depends on mock ordering), and the directory.
fn mount_provisioning(sut: &FakeSut) {
    sut.mount(Mock::given(method("POST")).and(path("/ehr")).respond_with(
        ResponseTemplate::new(201).insert_header("Location", &format!("http://sut/v1/ehr/{EHR}")),
    ));
    for (marker, uid) in [("committed-one", UID_0), ("committed-two", UID_1)] {
        sut.mount(
            Mock::given(method("POST"))
                .and(path(format!("/ehr/{EHR}/composition")))
                .and(body_string_contains(marker))
                .respond_with(
                    ResponseTemplate::new(201).insert_header("ETag", &format!("W/\"{uid}\"")),
                ),
        );
    }
    sut.mount(
        Mock::given(method("POST"))
            .and(path(format!("/ehr/{EHR}/directory")))
            .respond_with(ResponseTemplate::new(201).insert_header(
                "ETag",
                "W/\"dddddddd-4444-4444-8444-444444444444::sut.example::1\"",
            )),
    );
}

/// `<METHOD> <path>` per received request, in arrival order.
fn exchanges(sut: &FakeSut) -> Vec<String> {
    sut.requests()
        .iter()
        .map(|r| format!("{} {}", r.method, r.url.path()))
        .collect()
}

/// A directory fixture's committed-object references resolve against the same
/// run's commit set: the compositions are posted FIRST, their version uids are
/// bound per index, and the FOLDER tree reaches the SUT with every `${…}`
/// reference already rendered.
#[test]
#[expect(
    clippy::panic_in_result_fn,
    reason = "the Book's Result-test shape: assertions panic, plumbing propagates with `?`"
)]
fn a_directory_fixture_renders_its_references_from_the_same_runs_commit_set() -> Fallible {
    let dir = corpus(&folder_tree(
        "${committed_object_id_0}",
        "${committed_object_id_1}",
    ))?;
    let sut = FakeSut::start();
    mount_provisioning(&sut);

    let set = artifact_set_over_corpus(&provisioning_bindings(), manifest(), dir.path());
    let topology = ixit(&sut.base_url());
    let core = case(directory_case());
    let mut driver = HttpDriver::new(&set, &topology, None)?;
    let mut vars = VarStore::default();
    assert_eq!(driver.provision(&core, 0, &mut vars)?, Provisioned::Ready);

    assert_eq!(
        exchanges(&sut),
        vec![
            "POST /ehr".to_owned(),
            format!("POST /ehr/{EHR}/composition"),
            format!("POST /ehr/{EHR}/composition"),
            format!("POST /ehr/{EHR}/directory"),
        ],
        "the commit set must be posted before the tree that classifies it"
    );

    let received = sut.requests();
    let directory = received.last().ok_or("the SUT received no request")?;
    let body = String::from_utf8(directory.body.clone())?;
    assert!(
        !body.contains("${"),
        "an unrendered reference reached the SUT: {body}"
    );
    assert!(
        body.contains("aaaaaaaa-1111-4111-8111-111111111111"),
        "{body}"
    );
    assert!(
        body.contains("bbbbbbbb-2222-4222-8222-222222222222"),
        "{body}"
    );
    assert!(
        !body.contains("::sut.example::"),
        "an item reference names the OBJECT id, not the version uid: {body}"
    );

    assert_eq!(
        vars.scalar(&CaptureName::parse("committed_object_id_0")?),
        Some("aaaaaaaa-1111-4111-8111-111111111111")
    );
    assert_eq!(
        vars.scalar(&CaptureName::parse("committed_uid_1")?),
        Some(UID_1)
    );
    Ok(())
}

/// A directory fixture naming a capture the commit set never bound fails
/// provisioning loudly: a silently unrendered `${…}` would post a reference
/// that resolves to nothing and turn the whole case into a false red.
#[test]
#[expect(
    clippy::panic_in_result_fn,
    reason = "the Book's Result-test shape: assertions panic, plumbing propagates with `?`"
)]
fn a_directory_fixture_naming_an_unbound_capture_fails_provisioning() -> Fallible {
    let dir = corpus(&folder_tree(
        "${committed_object_id_0}",
        "${committed_object_id_9}",
    ))?;
    let sut = FakeSut::start();
    mount_provisioning(&sut);

    let set = artifact_set_over_corpus(&provisioning_bindings(), manifest(), dir.path());
    let topology = ixit(&sut.base_url());
    let core = case(directory_case());
    let mut driver = HttpDriver::new(&set, &topology, None)?;
    let mut vars = VarStore::default();
    let failure = driver
        .provision(&core, 0, &mut vars)
        .expect_err("an unbound reference in the directory payload must fail provisioning");
    assert_eq!(failure, "capture committed_object_id_9 is not bound");
    assert!(
        !exchanges(&sut).contains(&format!("POST /ehr/{EHR}/directory")),
        "an unrendered tree must never reach the SUT"
    );
    Ok(())
}

/// The query binding the selection-spec tests drive.
fn query_binding() -> Value {
    json!({
        "sm_operation": "I_QUERY_SERVICE.execute_ad_hoc_query",
        "its": "its-rest",
        "request": {
            "method": "POST",
            "path": "/query/aql",
            "body": { "q": "${q}" }
        },
        "outcomes": { "ok": { "status": 200 } }
    })
}

/// A one-step query case asserting the served rows against a named view.
fn query_case(view: &str, match_mode: &str, columns: &Value) -> Value {
    json!({
        "id": format!("WIRE-select_{view}"), "kind": "functional", "component": "QUERY",
        "sm_operation": "I_QUERY_SERVICE.execute_ad_hoc_query",
        "test_purpose": "t", "description": "d", "spec_refs": [],
        "flow": [{
            "step": 1, "call": "execute_ad_hoc_query", "expect": "ok",
            "with": { "q": "SELECT c/uid/value AS uid FROM EHR e CONTAINS FOLDER f" },
            "assert": [{
                "assert": "result_set",
                "match": match_mode,
                "rows": { "from": format!("${{ds:cnf.test.tree#{view}}}") },
                "columns": columns.clone()
            }]
        }]
    })
}

/// The uid list a `requires.commit` provisioning would have bound.
fn committed(vars: &mut VarStore, uids: &[&str]) -> Result<(), Box<dyn std::error::Error>> {
    vars.set(
        CaptureName::parse("committed_uids")?,
        Captured::List(uids.iter().map(|u| (*u).to_owned()).collect()),
    );
    Ok(())
}

/// Drive step 1 of a one-step case against the running fake SUT.
fn drive_query(
    base_url: &str,
    set: &ArtifactSet,
    case_document: Value,
    vars: &mut VarStore,
) -> Result<StepObservation, Box<dyn std::error::Error>> {
    let topology = ixit(base_url);
    let core = case(case_document);
    let step = core.flow.first().ok_or("the case declares no flow step")?;
    let mut driver = HttpDriver::new(set, &topology, None)?;
    Ok(driver.perform(&core, step, OutcomeKind::Ok, 0, vars)?)
}

/// A `select: uids` view names WHICH committed objects the answer must carry,
/// by index into the captured uid list: the row built from index 2 passes
/// against the uid actually committed there, and a server answering any other
/// uid fails the row with the mismatch named.
#[test]
#[expect(
    clippy::panic_in_result_fn,
    reason = "the Book's Result-test shape: assertions panic, plumbing propagates with `?`"
)]
fn a_uids_selection_spec_expects_the_committed_uid_at_each_index() -> Fallible {
    let uids = ["uid-zero", "uid-one", "uid-two", "uid-three"];
    let dir = corpus(&folder_tree("unused", "unused"))?;
    let set = artifact_set_over_corpus(&[query_binding()], manifest(), dir.path());

    let sut = FakeSut::start();
    sut.mount(
        Mock::given(method("POST"))
            .and(path("/query/aql"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "columns": [{ "name": "uid" }],
                "rows": [["uid-two"]]
            }))),
    );
    let mut vars = VarStore::default();
    committed(&mut vars, &uids)?;
    let observed = drive_query(
        &sut.base_url(),
        &set,
        query_case("f2_scoped_uids", "set", &json!([{ "name": "uid" }])),
        &mut vars,
    )?;
    assert_eq!(observed.observation, Observation::Kind(OutcomeKind::Ok));
    assert!(
        observed.assertion_failures.is_empty(),
        "the served row is the committed uid at index 2: {:?}",
        observed.assertion_failures
    );

    let wrong = FakeSut::start();
    wrong.mount(
        Mock::given(method("POST"))
            .and(path("/query/aql"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "columns": [{ "name": "uid" }],
                "rows": [["uid-three"]]
            }))),
    );
    let mut vars = VarStore::default();
    committed(&mut vars, &uids)?;
    let observed = drive_query(
        &wrong.base_url(),
        &set,
        query_case("f2_scoped_uids", "set", &json!([{ "name": "uid" }])),
        &mut vars,
    )?;
    let failure = observed
        .assertion_failures
        .first()
        .ok_or("a wrong uid must fail the result_set assertion")?;
    assert!(
        failure.reason().contains("uid-two"),
        "the failure must name the expected uid: {failure:?}"
    );
    Ok(())
}

/// A `select: pairs` view builds two-column rows of a literal beside the
/// committed uid at an index — the folder-containment shape, where the same
/// composition is expected once per folder that classifies it.
#[test]
#[expect(
    clippy::panic_in_result_fn,
    reason = "the Book's Result-test shape: assertions panic, plumbing propagates with `?`"
)]
fn a_pairs_selection_spec_pairs_a_literal_with_the_committed_uid() -> Fallible {
    let uids = ["uid-zero", "uid-one", "uid-two", "uid-three"];
    let dir = corpus(&folder_tree("unused", "unused"))?;
    let set = artifact_set_over_corpus(&[query_binding()], manifest(), dir.path());

    let sut = FakeSut::start();
    sut.mount(
        Mock::given(method("POST"))
            .and(path("/query/aql"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "rows": [
                    ["f11", "uid-three"],
                    ["f1", "uid-zero"], ["f1", "uid-one"],
                    ["f1", "uid-two"], ["f1", "uid-three"],
                    ["f2", "uid-two"],
                    ["root", "uid-zero"], ["root", "uid-one"],
                    ["root", "uid-two"], ["root", "uid-three"]
                ]
            }))),
    );
    let mut vars = VarStore::default();
    committed(&mut vars, &uids)?;
    let observed = drive_query(
        &sut.base_url(),
        &set,
        query_case("folder_composition_pairs", "set", &Value::Null),
        &mut vars,
    )?;
    assert!(
        observed.assertion_failures.is_empty(),
        "the ten authored pairs are the ten served rows: {:?}",
        observed.assertion_failures
    );
    Ok(())
}
