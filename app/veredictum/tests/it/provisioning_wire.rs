// SPDX-FileCopyrightText: Veredictum contributors
// SPDX-License-Identifier: Apache-2.0

//! The precondition provisioning that speaks to the SUT, against the fake
//! SUT: templates, demographic parties, party relationships and a received
//! EHR-Extract.
//!
//! A precondition establishes the ground a case is ABOUT to exercise, so the
//! attribution law puts every failure here on the runner side: an
//! unestablished `requires` is a step-resolution failure that leaves the
//! behaviour under test undriven, never a conformance finding against the
//! server. What these tests pin is that each precondition drives the route
//! its own declaration names, mints the handle the flow addresses, and
//! reports a refusal as an errored row rather than driving a case over ground
//! that does not exist.

use assert_fs::prelude::{FileWriteStr as _, PathChild as _};
use serde_json::{Value, json};
use veredictum::exec::driver::HttpDriver;
use veredictum::exec::state::VarStore;
use veredictum::exec::{Provisioned, StepDriver};
use veredictum::ids::CaptureName;
use wiremock::matchers::{method, path};
use wiremock::{Mock, ResponseTemplate};

use crate::fake_sut::{FakeSut, artifact_set_over_corpus, case, ixit};

/// Anything a driver construction, a corpus write or a provisioning run can
/// fail with, so a test body propagates plumbing failures with `?`
/// (<https://doc.rust-lang.org/book/ch11-01-writing-tests.html>).
type Fallible = Result<(), Box<dyn std::error::Error>>;

/// The `VERSIONED_OBJECT` uid the SUT answers a party create with — the
/// identifier the SM admin operations take, read off the create's own `ETag`
/// through the binding's `root-uid` transform.
const PERSON_UID: &str = "11111111-1111-4111-8111-111111111111";
/// The container uid of the party at the relationship's target end.
const ORG_UID: &str = "22222222-2222-4222-8222-222222222222";

/// One manifest entry over a written fixture.
fn entry(source: &str, format: &str) -> Value {
    json!({
        "source": source,
        "format": format,
        "validity": { "verdict": "valid" },
        "provenance": "authored in-test: a provisioning precondition payload"
    })
}

/// Write the named fixtures into a fresh corpus directory. Each key's file is
/// `<key>.json`, whatever the declared format — the extension is never read.
fn corpus(fixtures: &[(&str, String)]) -> Result<assert_fs::TempDir, Box<dyn std::error::Error>> {
    let dir = assert_fs::TempDir::new()?;
    for (name, body) in fixtures {
        dir.child(format!("{name}.json")).write_str(body)?;
    }
    Ok(dir)
}

/// `<METHOD> <path>` per received request, in arrival order.
fn exchanges(sut: &FakeSut) -> Vec<String> {
    sut.requests()
        .iter()
        .map(|r| format!("{} {}", r.method, r.url.path()))
        .collect()
}

/// The body of the request at `index`, as JSON.
fn body_at(sut: &FakeSut, index: usize) -> Result<Value, Box<dyn std::error::Error>> {
    let received = sut.requests();
    let request = received
        .get(index)
        .ok_or("the SUT received no such request")?;
    Ok(serde_json::from_slice(&request.body)?)
}

/// Run the case's provisioning against the fake SUT over the written corpus.
fn provision(
    sut: &FakeSut,
    bindings: &[Value],
    manifest: Value,
    corpus_dir: &std::path::Path,
    case_document: Value,
    vars: &mut VarStore,
) -> Result<Provisioned, Box<dyn std::error::Error>> {
    let set = artifact_set_over_corpus(bindings, manifest, corpus_dir);
    let topology = ixit(&sut.base_url());
    let core = case(case_document);
    let mut driver = HttpDriver::new(&set, &topology, None)?;
    Ok(driver.provision(&core, 0, vars)?)
}

/// The two template-upload routes: the ADL 1.4 operational template and the
/// ADL2 artefact source, each its own SM operation.
fn upload_bindings() -> Vec<Value> {
    vec![
        json!({
            "sm_operation": "I_DEFINITION_ADL14.upload_opt",
            "its": "its-rest",
            "request": {
                "method": "POST", "path": "/definition/template/adl1.4", "body": "opt"
            },
            "outcomes": { "created": { "status": 201 } }
        }),
        json!({
            "sm_operation": "I_DEFINITION_ADL2.upload_artefact",
            "its": "its-rest",
            "request": {
                "method": "POST", "path": "/definition/template/adl2", "body": "artefact"
            },
            "outcomes": { "created": { "status": 201 } }
        }),
    ]
}

/// A case whose only precondition is the named template keys.
fn template_case(keys: &[&str]) -> Value {
    json!({
        "id": "WIRE-templates", "kind": "functional", "component": "DEFINITION_ADL14",
        "sm_operation": "I_DEFINITION_ADL14.upload_opt",
        "requires": { "templates": keys },
        "test_purpose": "t", "description": "d", "spec_refs": [],
        "flow": [{ "step": 1, "call": "upload_opt", "expect": "created" }]
    })
}

/// A template precondition is uploaded through the endpoint its OWN corpus
/// format names: the OPT 1.4 XML to the ADL 1.4 route, the ADL2 source to the
/// ADL2 route. One operation must never go on the wire two ways.
#[test]
#[expect(
    clippy::panic_in_result_fn,
    reason = "the Book's Result-test shape: assertions panic, plumbing propagates with `?`"
)]
fn a_template_precondition_uploads_through_the_route_its_corpus_format_names() -> Fallible {
    let dir = corpus(&[
        ("opt", "<template><uid/></template>".to_owned()),
        ("adl2", "template (adl_version=2.0.6)\n".to_owned()),
    ])?;
    let manifest = json!({
        "cnf.opt.minimal": entry("opt.json", "opt-xml"),
        "cnf.adl2.minimal": entry("adl2.json", "adl2-text")
    });
    let sut = FakeSut::start();
    for route in ["/definition/template/adl1.4", "/definition/template/adl2"] {
        sut.mount(
            Mock::given(method("POST"))
                .and(path(route))
                .respond_with(ResponseTemplate::new(201)),
        );
    }

    let mut vars = VarStore::default();
    let outcome = provision(
        &sut,
        &upload_bindings(),
        manifest,
        dir.path(),
        template_case(&["cnf.opt.minimal", "cnf.adl2.minimal"]),
        &mut vars,
    )?;
    assert_eq!(outcome, Provisioned::Ready);
    assert_eq!(
        exchanges(&sut),
        vec![
            "POST /definition/template/adl1.4".to_owned(),
            "POST /definition/template/adl2".to_owned(),
        ]
    );

    // The source text reaches the SUT unparsed: a text format is a carrier,
    // and re-encoding it would exercise the runner's serializer rather than
    // the server's reader.
    let received = sut.requests();
    let uploaded = received.first().ok_or("the SUT received no upload")?;
    assert_eq!(
        String::from_utf8(uploaded.body.clone())?,
        "<template><uid/></template>"
    );
    Ok(())
}

/// A refused template upload ERRORS the row: the ground the case needs was
/// never established, so the behaviour under test was not driven and the row
/// is inconclusive rather than a finding. A 409 is the one exception — it
/// says the ground already holds, which is what a re-run sees.
#[test]
#[expect(
    clippy::panic_in_result_fn,
    reason = "the Book's Result-test shape: assertions panic, plumbing propagates with `?`"
)]
fn a_refused_template_upload_errors_the_row_instead_of_driving_it() -> Fallible {
    let dir = corpus(&[("opt", "<template/>".to_owned())])?;
    let manifest = json!({ "cnf.opt.minimal": entry("opt.json", "opt-xml") });

    let refused = FakeSut::start();
    refused.mount(
        Mock::given(method("POST"))
            .and(path("/definition/template/adl1.4"))
            .respond_with(ResponseTemplate::new(406).set_body_json(json!({ "message": "no" }))),
    );
    let mut vars = VarStore::default();
    let outcome = provision(
        &refused,
        &upload_bindings(),
        manifest.clone(),
        dir.path(),
        template_case(&["cnf.opt.minimal"]),
        &mut vars,
    )?;
    match outcome {
        Provisioned::RowErrored { reason } => {
            assert!(reason.contains("upload_opt"), "{reason}");
            assert!(reason.contains("406"), "{reason}");
            assert!(reason.contains("was not driven"), "{reason}");
        }
        other => panic!("a refused upload must error the row, got {other:?}"),
    }

    // 409: the deterministic re-upload of a template that is already there.
    let existing = FakeSut::start();
    existing.mount(
        Mock::given(method("POST"))
            .and(path("/definition/template/adl1.4"))
            .respond_with(ResponseTemplate::new(409)),
    );
    let mut vars = VarStore::default();
    let outcome = provision(
        &existing,
        &upload_bindings(),
        manifest,
        dir.path(),
        template_case(&["cnf.opt.minimal"]),
        &mut vars,
    )?;
    assert_eq!(outcome, Provisioned::Ready);
    Ok(())
}

/// The demographic create routes: the variant-less binding realizes PERSON,
/// its `agent` variant the AGENT subtype. Each maps the `versioned_object_uid`
/// the provisioning publishes.
fn party_bindings() -> Vec<Value> {
    let capture = json!({
        "version_uid": { "from": "header ETag", "strip": "weak-quotes" },
        "versioned_object_uid": { "from": "capture version_uid", "transform": "root-uid" }
    });
    vec![
        json!({
            "sm_operation": "I_DEMOGRAPHIC_SERVICE.create_party",
            "its": "its-rest",
            "request": { "method": "POST", "path": "/demographic/person", "body": "party" },
            "outcomes": { "created": { "status": 201 } },
            "captures": capture
        }),
        json!({
            "sm_operation": "I_DEMOGRAPHIC_SERVICE.create_party",
            "its": "its-rest",
            "variant": "agent",
            "request": { "method": "POST", "path": "/demographic/agent", "body": "party" },
            "outcomes": { "created": { "status": 201 } },
            "captures": capture
        }),
        json!({
            "sm_operation": "I_DEMOGRAPHIC_SERVICE.create_party_relationship",
            "its": "its-rest",
            "request": {
                "method": "POST",
                "path": "/demographic/party_relationship",
                "body": "party_relationship"
            },
            "outcomes": { "created": { "status": 201 } },
            "captures": capture
        }),
    ]
}

/// One demographic PARTY payload of the named concrete subtype.
fn party(rm_type: &str, name: &str) -> String {
    json!({
        "_type": rm_type,
        "name": { "_type": "DV_TEXT", "value": name },
        "archetype_node_id": "openEHR-DEMOGRAPHIC-PARTY.generic.v1"
    })
    .to_string()
}

/// Answer a create on `route` with a weak-quoted version uid over `container`.
fn mount_created(sut: &FakeSut, route: &str, container: &str) {
    sut.mount(
        Mock::given(method("POST")).and(path(route)).respond_with(
            ResponseTemplate::new(201)
                .insert_header("ETag", &format!("W/\"{container}::sut.example::1\"")),
        ),
    );
}

/// A `requires.party` precondition mints `${party_id}` as the party's
/// `VERSIONED_OBJECT` uid — the identifier the SM admin operations take, not
/// the version uid the create's `ETag` carries — and takes the route the
/// payload's own concrete RM type names.
#[test]
#[expect(
    clippy::panic_in_result_fn,
    reason = "the Book's Result-test shape: assertions panic, plumbing propagates with `?`"
)]
fn a_party_precondition_mints_the_container_uid_over_its_own_subtypes_route() -> Fallible {
    let dir = corpus(&[
        ("person", party("PERSON", "a person")),
        ("agent", party("AGENT", "an agent")),
        ("party", party("PARTY", "an abstract class")),
    ])?;
    let manifest = json!({
        "cnf.party.person": entry("person.json", "canonical-json"),
        "cnf.party.agent": entry("agent.json", "canonical-json"),
        "cnf.party.abstract": entry("party.json", "canonical-json")
    });
    let party_case = |key: &str| {
        json!({
            "id": "WIRE-party", "kind": "functional", "component": "DEMOGRAPHIC",
            "sm_operation": "I_DEMOGRAPHIC_SERVICE.create_party",
            "requires": { "party": key },
            "test_purpose": "t", "description": "d", "spec_refs": [],
            "flow": [{ "step": 1, "call": "create_party", "expect": "created" }]
        })
    };

    for (key, route) in [
        ("cnf.party.person", "/demographic/person"),
        ("cnf.party.agent", "/demographic/agent"),
    ] {
        let sut = FakeSut::start();
        mount_created(&sut, route, PERSON_UID);
        let mut vars = VarStore::default();
        let outcome = provision(
            &sut,
            &party_bindings(),
            manifest.clone(),
            dir.path(),
            party_case(key),
            &mut vars,
        )?;
        assert_eq!(outcome, Provisioned::Ready);
        assert_eq!(exchanges(&sut), vec![format!("POST {route}")]);
        assert_eq!(
            vars.scalar(&CaptureName::parse("party_id")?),
            Some(PERSON_UID),
            "{key} must mint the container uid, not the version uid"
        );
    }

    // A payload naming an ABSTRACT class names no create endpoint, and that
    // is a loud provisioning error rather than a guessed route.
    let sut = FakeSut::start();
    mount_created(&sut, "/demographic/person", PERSON_UID);
    let mut vars = VarStore::default();
    let error = provision(
        &sut,
        &party_bindings(),
        manifest,
        dir.path(),
        party_case("cnf.party.abstract"),
        &mut vars,
    )
    .expect_err("PARTY is not a concrete subtype");
    assert!(format!("{error}").contains("PARTY"), "{error}");
    assert!(
        exchanges(&sut).is_empty(),
        "nothing may reach the SUT before the route is known"
    );
    Ok(())
}

/// One `PARTY_RELATIONSHIP` payload whose ends declare the given `PARTY_REF`
/// types.
fn relationship(source_type: &str, target_type: &str, with_ids: bool) -> String {
    let end = |party_type: &str| {
        let mut reference = json!({
            "_type": "PARTY_REF", "namespace": "local", "type": party_type
        });
        if with_ids && let Some(map) = reference.as_object_mut() {
            map.insert(
                "id".to_owned(),
                json!({ "_type": "HIER_OBJECT_ID", "value": "__AUTO-GENERATED__" }),
            );
        }
        reference
    };
    json!({
        "_type": "PARTY_RELATIONSHIP",
        "name": { "_type": "DV_TEXT", "value": "employment" },
        "archetype_node_id": "openEHR-DEMOGRAPHIC-PARTY_RELATIONSHIP.generic.v1",
        "source": end(source_type),
        "target": end(target_type)
    })
    .to_string()
}

/// The case whose precondition is a relationship between the two named party
/// payloads.
fn relationship_case(relationship_key: &str) -> Value {
    json!({
        "id": "WIRE-party_relationship", "kind": "functional", "component": "DEMOGRAPHIC",
        "sm_operation": "I_DEMOGRAPHIC_SERVICE.create_party_relationship",
        "requires": {
            "party_relationship": {
                "source": "cnf.party.person",
                "target": "cnf.party.agent",
                "relationship": relationship_key
            }
        },
        "test_purpose": "t", "description": "d", "spec_refs": [],
        "flow": [{ "step": 1, "call": "create_party_relationship", "expect": "created" }]
    })
}

/// A `requires.party_relationship` provisions both ends first and substitutes
/// each `PARTY_REF.id.value` with the CONTAINER uid that create minted — RM
/// demographic master02 §Party Relationships requires `HIER_OBJECT_ID`s
/// denoting the Version container, never `OBJECT_VERSION_ID`s.
#[test]
#[expect(
    clippy::panic_in_result_fn,
    reason = "the Book's Result-test shape: assertions panic, plumbing propagates with `?`"
)]
fn a_relationship_precondition_substitutes_the_container_uids_it_minted() -> Fallible {
    let dir = corpus(&[
        ("person", party("PERSON", "a person")),
        ("agent", party("AGENT", "an agent")),
        ("rel", relationship("PERSON", "AGENT", true)),
        ("mismatched", relationship("PERSON", "ORGANISATION", true)),
        ("idless", relationship("PERSON", "AGENT", false)),
    ])?;
    let manifest = json!({
        "cnf.party.person": entry("person.json", "canonical-json"),
        "cnf.party.agent": entry("agent.json", "canonical-json"),
        "cnf.rel.employment": entry("rel.json", "canonical-json"),
        "cnf.rel.mismatched": entry("mismatched.json", "canonical-json"),
        "cnf.rel.idless": entry("idless.json", "canonical-json")
    });

    let sut = FakeSut::start();
    mount_created(&sut, "/demographic/person", PERSON_UID);
    mount_created(&sut, "/demographic/agent", ORG_UID);
    mount_created(
        &sut,
        "/demographic/party_relationship",
        "33333333-3333-4333-8333-333333333333",
    );
    let mut vars = VarStore::default();
    let outcome = provision(
        &sut,
        &party_bindings(),
        manifest.clone(),
        dir.path(),
        relationship_case("cnf.rel.employment"),
        &mut vars,
    )?;
    assert_eq!(outcome, Provisioned::Ready);
    assert_eq!(
        exchanges(&sut),
        vec![
            "POST /demographic/person".to_owned(),
            "POST /demographic/agent".to_owned(),
            "POST /demographic/party_relationship".to_owned(),
        ],
        "both ends must exist before the relationship between them"
    );
    let posted = body_at(&sut, 2)?;
    assert_eq!(posted["source"]["id"]["value"], json!(PERSON_UID));
    assert_eq!(posted["target"]["id"]["value"], json!(ORG_UID));
    assert_eq!(
        vars.scalar(&CaptureName::parse("party_relationship_id")?),
        Some("33333333-3333-4333-8333-333333333333")
    );

    // A fixture whose declared `PARTY_REF.type` is not the type provisioned
    // for that end is a CATALOGUE defect, refused here rather than sent.
    let sut = FakeSut::start();
    mount_created(&sut, "/demographic/person", PERSON_UID);
    mount_created(&sut, "/demographic/agent", ORG_UID);
    let mut vars = VarStore::default();
    let error = provision(
        &sut,
        &party_bindings(),
        manifest.clone(),
        dir.path(),
        relationship_case("cnf.rel.mismatched"),
        &mut vars,
    )
    .expect_err("an ORGANISATION reference over a provisioned AGENT is a defect");
    let message = format!("{error}");
    assert!(
        message.contains("ORGANISATION") && message.contains("AGENT"),
        "{message}"
    );

    // A reference with no `id.value` slot has nowhere to put the minted uid.
    let sut = FakeSut::start();
    mount_created(&sut, "/demographic/person", PERSON_UID);
    mount_created(&sut, "/demographic/agent", ORG_UID);
    let mut vars = VarStore::default();
    let error = provision(
        &sut,
        &party_bindings(),
        manifest,
        dir.path(),
        relationship_case("cnf.rel.idless"),
        &mut vars,
    )
    .expect_err("a PARTY_REF with no id.value cannot carry the minted uid");
    assert!(format!("{error}").contains("source.id.value"), "{error}");
    Ok(())
}

/// The two receiving routes RM common master06 §Copying distinguishes: an
/// extract landing in an EHR that already exists, and a whole-EHR clone.
fn import_bindings() -> Vec<Value> {
    vec![
        json!({
            "sm_operation": "I_EHR_SERVICE.create_ehr",
            "its": "its-rest",
            "request": { "method": "POST", "path": "/ehr" },
            "outcomes": { "created": { "status": 201 } },
            "captures": { "ehr_id": { "from": "header Location last-segment" } }
        }),
        json!({
            "sm_operation": "I_EHR_EXTRACT_SERVICE.import_ehr_extract",
            "its": "its-rest",
            "request": {
                "method": "POST", "path": "/message/import/{an_ehr_id}", "body": "extract"
            },
            "outcomes": { "created": { "status": 201 } }
        }),
        json!({
            "sm_operation": "I_EHR_EXTRACT_SERVICE.import_ehr",
            "its": "its-rest",
            "request": { "method": "POST", "path": "/message/import", "body": "extract" },
            "outcomes": { "created": { "status": 201 } },
            "captures": { "ehr_id": { "from": "body \"uid.value\"" } }
        }),
    ]
}

/// An EHR-Extract carrying one versioned COMPOSITION, on a trunk and a
/// branch.
fn extract() -> String {
    let version = |uid: &str| json!({ "_type": "ORIGINAL_VERSION", "uid": { "value": uid } });
    json!({
        "_type": "EHR_EXTRACT",
        "chapters": [{ "items": [{ "item": {
            "_type": "X_VERSIONED_COMPOSITION",
            "uid": { "value": "comp-vo" },
            "versions": [
                version("comp-vo::src::1"),
                version("comp-vo::src::2"),
                version("comp-vo::other::1.1.1")
            ]
        } }] }]
    })
    .to_string()
}

/// The import case, optionally provisioning an EHR first.
fn import_case(with_ehr: bool) -> Value {
    let mut requires = json!({
        "import": { "extract": "cnf.extract.v1", "container": "X_VERSIONED_COMPOSITION" }
    });
    if with_ehr && let Some(map) = requires.as_object_mut() {
        map.insert("ehr".to_owned(), json!({ "commits": "none" }));
    }
    json!({
        "id": "WIRE-import", "kind": "functional", "component": "EHR",
        "sm_operation": "I_EHR_EXTRACT_SERVICE.import_ehr_extract",
        "requires": requires,
        "test_purpose": "t", "description": "d", "spec_refs": [],
        "flow": [{ "step": 1, "call": "import_ehr_extract", "expect": "created" }]
    })
}

/// The manifest and corpus every import test reads.
fn import_corpus() -> Result<(assert_fs::TempDir, Value), Box<dyn std::error::Error>> {
    let dir = corpus(&[("extract", extract())])?;
    Ok((
        dir,
        json!({ "cnf.extract.v1": entry("extract.json", "canonical-json") }),
    ))
}

/// The receiving situation selects the route: a provisioned `${ehr_id}` takes
/// the path-addressed import, and no EHR at all takes the clone, whose answer
/// names the EHR it created. Either way the identities come from the EXTRACT
/// itself, because master06 §Copying keeps them — reading them back off the
/// SUT would make the server's own answer the reference.
#[test]
#[expect(
    clippy::panic_in_result_fn,
    reason = "the Book's Result-test shape: assertions panic, plumbing propagates with `?`"
)]
fn an_import_precondition_takes_the_route_its_receiving_situation_names() -> Fallible {
    let (dir, manifest) = import_corpus()?;

    // Case 2/3: the extract lands in the EHR the precondition just minted.
    let sut = FakeSut::start();
    sut.mount(Mock::given(method("POST")).and(path("/ehr")).respond_with(
        ResponseTemplate::new(201).insert_header("Location", "http://sut/v1/ehr/EHR-received"),
    ));
    sut.mount(
        Mock::given(method("POST"))
            .and(path("/message/import/EHR-received"))
            .respond_with(ResponseTemplate::new(201)),
    );
    let mut vars = VarStore::default();
    let outcome = provision(
        &sut,
        &import_bindings(),
        manifest.clone(),
        dir.path(),
        import_case(true),
        &mut vars,
    )?;
    assert_eq!(outcome, Provisioned::Ready);
    assert_eq!(
        exchanges(&sut),
        vec![
            "POST /ehr".to_owned(),
            "POST /message/import/EHR-received".to_owned(),
        ]
    );
    // "Latest" is by version_tree_id order, so the trunk handle is version 2
    // and the branch handle is the 1.1.1 position — never document order.
    assert_eq!(
        vars.scalar(&CaptureName::parse("imported_versioned_object_uid")?),
        Some("comp-vo")
    );
    assert_eq!(
        vars.scalar(&CaptureName::parse("imported_version_uid")?),
        Some("comp-vo::src::2")
    );
    assert_eq!(
        vars.scalar(&CaptureName::parse("imported_branch_version_uid")?),
        Some("comp-vo::other::1.1.1")
    );

    // Case 1: no EHR was provisioned, so the clone route runs and its answer
    // mints `${ehr_id}`.
    let sut = FakeSut::start();
    sut.mount(
        Mock::given(method("POST"))
            .and(path("/message/import"))
            .respond_with(
                ResponseTemplate::new(201)
                    .set_body_json(json!({ "uid": { "value": "EHR-clone" } })),
            ),
    );
    let mut vars = VarStore::default();
    let outcome = provision(
        &sut,
        &import_bindings(),
        manifest,
        dir.path(),
        import_case(false),
        &mut vars,
    )?;
    assert_eq!(outcome, Provisioned::Ready);
    assert_eq!(exchanges(&sut), vec!["POST /message/import".to_owned()]);
    assert_eq!(
        vars.scalar(&CaptureName::parse("ehr_id")?),
        Some("EHR-clone"),
        "the clone's answer names the EHR the case reads through"
    );
    Ok(())
}

/// A refused import errors the row, and a clone the SUT accepted without
/// naming the EHR it created leaves the case with no `${ehr_id}` to read
/// through — a loud provisioning failure rather than a row driven against
/// nothing. On this route a 409 can never mean "the ground already holds":
/// master06 §Copying gives one received `object_id` one local container, so
/// a conflict says the container exists in ANOTHER EHR.
#[test]
#[expect(
    clippy::panic_in_result_fn,
    reason = "the Book's Result-test shape: assertions panic, plumbing propagates with `?`"
)]
fn a_refused_or_anonymous_import_never_drives_the_case() -> Fallible {
    let (dir, manifest) = import_corpus()?;

    for status in [409_u16, 422] {
        let sut = FakeSut::start();
        sut.mount(
            Mock::given(method("POST"))
                .and(path("/message/import"))
                .respond_with(ResponseTemplate::new(status)),
        );
        let mut vars = VarStore::default();
        let outcome = provision(
            &sut,
            &import_bindings(),
            manifest.clone(),
            dir.path(),
            import_case(false),
            &mut vars,
        )?;
        match outcome {
            Provisioned::RowErrored { reason } => {
                assert!(reason.contains("import_ehr"), "{reason}");
                assert!(reason.contains(&status.to_string()), "{reason}");
            }
            other => panic!("status {status} must error the row, got {other:?}"),
        }
    }

    // Accepted, but the answer names no EHR: the case has nothing to address.
    let sut = FakeSut::start();
    sut.mount(
        Mock::given(method("POST"))
            .and(path("/message/import"))
            .respond_with(ResponseTemplate::new(201)),
    );
    let mut vars = VarStore::default();
    let error = provision(
        &sut,
        &import_bindings(),
        manifest,
        dir.path(),
        import_case(false),
        &mut vars,
    )
    .expect_err("a clone that names no EHR leaves the case unaddressable");
    assert!(
        format!("{error}").contains("without naming the EHR it cloned"),
        "{error}"
    );
    Ok(())
}

/// A `requires.commit` set is a set ARRAY or a single composition object, and
/// anything else is a catalogue defect refused outright. Skipping it silently
/// would drive a case whose stated precondition — "the EHR holds commits" —
/// does not hold.
#[test]
#[expect(
    clippy::panic_in_result_fn,
    reason = "the Book's Result-test shape: assertions panic, plumbing propagates with `?`"
)]
fn a_commit_set_that_is_neither_a_set_nor_a_composition_is_refused() -> Fallible {
    let dir = corpus(&[(
        "aql",
        "SELECT c FROM EHR e CONTAINS COMPOSITION c".to_owned(),
    )])?;
    let manifest = json!({ "cnf.query.trend": entry("aql.json", "aql-text") });
    let bindings = vec![
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
            "request": {
                "method": "POST", "path": "/ehr/{ehr_id}/composition", "body": "composition"
            },
            "outcomes": { "created": { "status": 201 } },
            "captures": { "version_uid": { "from": "header ETag", "strip": "weak-quotes" } }
        }),
    ];
    let sut = FakeSut::start();
    sut.mount(Mock::given(method("POST")).and(path("/ehr")).respond_with(
        ResponseTemplate::new(201).insert_header("Location", "http://sut/v1/ehr/EHR-1"),
    ));

    let mut vars = VarStore::default();
    let error = provision(
        &sut,
        &bindings,
        manifest,
        dir.path(),
        json!({
            "id": "WIRE-commit_shape", "kind": "functional", "component": "EHR_COMPOSITION",
            "sm_operation": "I_EHR_COMPOSITION.create_composition",
            "requires": { "ehr": { "commits": "none" }, "commit": ["cnf.query.trend"] },
            "test_purpose": "t", "description": "d", "spec_refs": [],
            "flow": [{ "step": 1, "call": "create_composition", "expect": "created" }]
        }),
        &mut vars,
    )
    .expect_err("AQL text is neither a set array nor a composition object");
    let message = format!("{error}");
    assert!(message.contains("cnf.query.trend"), "{message}");
    assert!(
        message.contains("expected a set array or a composition object"),
        "{message}"
    );
    assert_eq!(
        exchanges(&sut),
        vec!["POST /ehr".to_owned()],
        "nothing is committed once the set's shape is refused"
    );
    Ok(())
}
