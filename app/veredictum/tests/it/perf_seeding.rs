// SPDX-FileCopyrightText: Veredictum contributors
// SPDX-License-Identifier: Apache-2.0

//! Corpus provisioning against a fake SUT that answers wrongly.
//!
//! `perf_driver` proves the seeder works when the server behaves. This
//! module proves the other half: every wire answer the seeder does NOT
//! accept stops provisioning with a message naming the phase, so a measured
//! window never opens over a corpus that was only partly written. A seeder
//! that shrugged one of these off would publish a measurement of a
//! population nobody can reconstruct.

use veredictum::perf_run::client::{PerfClient, PerfPrincipals};
use veredictum::perf_run::corpus::{SeededCorpus, seed_scale_ladder, seed_ward, ward_size};
use veredictum::perf_run::jitter::LeafConstraints;
use veredictum::perf_run::pack::{AuxPayloads, FlatPayload, JourneyPack, PackTemplate, TddPayload};
use wiremock::matchers::{method, path};
use wiremock::{Mock, ResponseTemplate};

use crate::fake_sut::{FakeSut, closed_port_url, ixit};

/// Nothing here is measured, so the seeder's progress is discarded.
fn quiet(_message: String) {}

/// The primary client addressing a running fake SUT.
#[expect(
    clippy::expect_used,
    reason = "a test-support helper, outside the clippy.toml in-tests scoping: a broken harness must abort the test loudly, Book ch11"
)]
fn client(sut: &FakeSut) -> PerfClient {
    let topology = ixit(&sut.base_url());
    PerfPrincipals::from_ixit(&topology)
        .expect("the single-instance ixit resolves")
        .primary()
        .clone()
}

/// The template upload every seeding phase starts with, answered 201.
fn opt_upload_accepted(sut: &FakeSut) {
    sut.mount(
        Mock::given(method("POST"))
            .and(path("/definition/template/adl1.4"))
            .respond_with(ResponseTemplate::new(201)),
    );
}

/// A pack carrying the three keys the ward seeder resolves by name, plus
/// the two auxiliary payloads its preflight commits.
fn pack() -> JourneyPack {
    let skeleton = serde_json::json!({
        "_type": "COMPOSITION",
        "context": { "_type": "EVENT_CONTEXT",
                     "start_time": { "_type": "DV_DATE_TIME", "value": "2020-01-01T00:00:00Z" } },
        "composer": { "_type": "PARTY_IDENTIFIED", "name": "seed" }
    });
    let template = |key: &str, id: &str| PackTemplate {
        key: key.to_owned(),
        template_id: id.to_owned(),
        opt_xml: "<template/>".to_owned(),
        skeleton: skeleton.clone(),
        constraints: LeafConstraints::default(),
    };
    JourneyPack {
        templates: vec![
            template("cnf.ckm.gp_data_set", "GP data set"),
            template("cnf.ckm.lab_result", "Lab result"),
            template("cnf.ckm.medicines_list", "Medicines list item R1"),
        ],
        aux: AuxPayloads {
            flat: Some(FlatPayload {
                template_id: "minimal_action.en.v1".to_owned(),
                opt_xml: "<template/>".to_owned(),
                body: serde_json::json!({ "ctx/language": "en" }),
            }),
            person: None,
            person_amended: None,
            party_relationship: None,
            tdd: Some(TddPayload {
                opt_xml: "<template/>".to_owned(),
                document: "<Nested template_id=\"nested.en.v1\"/>".to_owned(),
            }),
        },
    }
}

/// A corpus index of `n` EHRs with one composition each, as the scale
/// phase would have left it.
fn seeded_index(n: usize) -> SeededCorpus {
    SeededCorpus {
        corpus: "cnf.scale.10k".to_owned(),
        ehr_ids: (0..n).map(|i| format!("ehr-{i}")).collect(),
        compositions: (0..n).map(|i| (i, format!("uid-{i}::stub::1"))).collect(),
        ward: Vec::new(),
    }
}

#[test]
fn an_unaccepted_template_upload_stops_the_scale_seed() {
    let sut = FakeSut::start();
    sut.mount(
        Mock::given(method("POST"))
            .and(path("/definition/template/adl1.4"))
            .respond_with(ResponseTemplate::new(500)),
    );
    let error = seed_scale_ladder(&client(&sut), "cnf.scale.10k", "<opt/>", 2, 1, 1, &quiet)
        .expect_err("a 500 on the OPT upload is not an accepted provisioning outcome");
    assert!(error.contains("OPT upload returned 500"), "{error}");
}

/// 409 is accepted: re-uploading the same operational template on a re-run
/// is the documented already-exists outcome, not a provisioning failure.
#[test]
fn an_already_present_template_does_not_stop_the_scale_seed() {
    let sut = FakeSut::start();
    sut.mount(
        Mock::given(method("POST"))
            .and(path("/definition/template/adl1.4"))
            .respond_with(ResponseTemplate::new(409)),
    );
    sut.mount(
        Mock::given(method("POST")).and(path("/ehr")).respond_with(
            ResponseTemplate::new(201).insert_header("Location", "http://sut/ehr/e1"),
        ),
    );
    sut.mount(
        Mock::given(method("POST"))
            .and(path("/ehr/e1/composition"))
            .respond_with(ResponseTemplate::new(201).insert_header("ETag", "W/\"c1::stub::1\"")),
    );
    let corpus = seed_scale_ladder(&client(&sut), "cnf.scale.10k", "<opt/>", 1, 1, 1, &quiet)
        .expect("409 on the template upload is the already-exists outcome");
    assert_eq!(corpus.ehr_ids, vec!["e1".to_owned()]);
    assert_eq!(corpus.compositions, vec![(0, "c1::stub::1".to_owned())]);
}

#[test]
fn an_ehr_create_outside_the_created_outcome_stops_the_scale_seed() {
    let sut = FakeSut::start();
    opt_upload_accepted(&sut);
    sut.mount(
        Mock::given(method("POST"))
            .and(path("/ehr"))
            .respond_with(ResponseTemplate::new(503)),
    );
    let error = seed_scale_ladder(&client(&sut), "cnf.scale.10k", "<opt/>", 2, 1, 1, &quiet)
        .expect_err("a 503 EHR create is not a seeded EHR");
    assert!(error.contains("create_ehr returned 503"), "{error}");
}

/// A 201 with no `Location` is the silent-corruption case: the seeder has
/// no id to address, so accepting it would leave a corpus whose EHRs cannot
/// be named.
#[test]
fn an_ehr_create_without_a_location_stops_the_scale_seed() {
    let sut = FakeSut::start();
    opt_upload_accepted(&sut);
    sut.mount(
        Mock::given(method("POST"))
            .and(path("/ehr"))
            .respond_with(ResponseTemplate::new(201)),
    );
    let error = seed_scale_ladder(&client(&sut), "cnf.scale.10k", "<opt/>", 2, 1, 1, &quiet)
        .expect_err("a 201 with no Location names no EHR");
    assert!(error.contains("no Location ehr_id"), "{error}");
}

#[test]
fn a_composition_commit_without_an_etag_stops_the_scale_seed() {
    let sut = FakeSut::start();
    opt_upload_accepted(&sut);
    sut.mount(
        Mock::given(method("POST")).and(path("/ehr")).respond_with(
            ResponseTemplate::new(201).insert_header("Location", "http://sut/ehr/e1"),
        ),
    );
    sut.mount(
        Mock::given(method("POST"))
            .and(path("/ehr/e1/composition"))
            .respond_with(ResponseTemplate::new(204)),
    );
    let error = seed_scale_ladder(&client(&sut), "cnf.scale.10k", "<opt/>", 1, 1, 1, &quiet)
        .expect_err("a minimal create with no ETag names no version");
    assert!(error.contains("no ETag"), "{error}");
}

#[test]
fn a_refused_composition_commit_stops_the_scale_seed() {
    let sut = FakeSut::start();
    opt_upload_accepted(&sut);
    sut.mount(
        Mock::given(method("POST")).and(path("/ehr")).respond_with(
            ResponseTemplate::new(201).insert_header("Location", "http://sut/ehr/e1"),
        ),
    );
    sut.mount(
        Mock::given(method("POST"))
            .and(path("/ehr/e1/composition"))
            .respond_with(ResponseTemplate::new(422)),
    );
    let error = seed_scale_ladder(&client(&sut), "cnf.scale.10k", "<opt/>", 1, 1, 1, &quiet)
        .expect_err("a 422 commit is not a seeded composition");
    assert!(error.contains("create_composition returned 422"), "{error}");
}

/// The ward seed is idempotent: a corpus whose ward already covers the
/// target size is left alone, and the SUT is never touched.
#[test]
fn an_already_seeded_ward_is_left_untouched() {
    let sut = FakeSut::start();
    let mut corpus = seeded_index(1);
    corpus.ward.push(veredictum::perf_run::corpus::WardPatient {
        ehr_index: 0,
        gp_ovid: "g::stub::1".to_owned(),
        medlist_ovid: "m::stub::1".to_owned(),
        directory_ovid: "d::stub::1".to_owned(),
        contribution_uid: "c1".to_owned(),
    });
    assert_eq!(ward_size(corpus.ehr_ids.len()), 1);

    let notes = std::sync::Mutex::new(Vec::new());
    seed_ward(&client(&sut), &mut corpus, &pack(), 1, &|message| {
        notes
            .lock()
            .expect("the progress lock is uncontended")
            .push(message);
    })
    .expect("a covered ward is already seeded");
    let seen = notes.lock().expect("the progress lock is uncontended");
    assert!(
        seen.iter().any(|m| m.contains("already seeded")),
        "{seen:?}"
    );
    assert!(
        sut.requests().is_empty(),
        "an idempotent ward seed still went on the wire"
    );
}

#[test]
fn a_pack_missing_a_ward_template_stops_the_ward_seed() {
    let sut = FakeSut::start();
    opt_upload_accepted(&sut);
    sut.mount(
        Mock::given(method("POST")).and(path("/ehr")).respond_with(
            ResponseTemplate::new(201).insert_header("Location", "http://sut/ehr/s1"),
        ),
    );
    sut.mount(
        Mock::given(method("POST"))
            .and(path("/ehr/s1/composition"))
            .respond_with(ResponseTemplate::new(201).insert_header("ETag", "W/\"x::stub::1\"")),
    );
    sut.mount(
        Mock::given(method("POST"))
            .and(path("/message/tdd/s1"))
            .respond_with(ResponseTemplate::new(201)),
    );
    sut.mount(
        Mock::given(method("PUT"))
            .and(path("/definition/query/org.openehr.cnf::ward_dashboard"))
            .respond_with(ResponseTemplate::new(200)),
    );
    let mut thin = pack();
    thin.templates.retain(|t| t.key != "cnf.ckm.medicines_list");
    let error = seed_ward(&client(&sut), &mut seeded_index(1), &thin, 1, &quiet)
        .expect_err("the ward addresses a medicines list the pack does not carry");
    assert!(error.contains("cnf.ckm.medicines_list"), "{error}");
}

#[test]
fn a_refused_stored_query_registration_stops_the_ward_seed() {
    let sut = FakeSut::start();
    opt_upload_accepted(&sut);
    sut.mount(
        Mock::given(method("POST")).and(path("/ehr")).respond_with(
            ResponseTemplate::new(201).insert_header("Location", "http://sut/ehr/s1"),
        ),
    );
    sut.mount(
        Mock::given(method("POST"))
            .and(path("/ehr/s1/composition"))
            .respond_with(ResponseTemplate::new(201).insert_header("ETag", "W/\"x::stub::1\"")),
    );
    sut.mount(
        Mock::given(method("POST"))
            .and(path("/message/tdd/s1"))
            .respond_with(ResponseTemplate::new(201)),
    );
    sut.mount(
        Mock::given(method("PUT"))
            .and(path("/definition/query/org.openehr.cnf::ward_dashboard"))
            .respond_with(ResponseTemplate::new(400)),
    );
    let error = seed_ward(&client(&sut), &mut seeded_index(1), &pack(), 1, &quiet)
        .expect_err("the dashboard query must register before the ward is usable");
    assert!(error.contains("store_query returned 400"), "{error}");
}

/// The preflight is what keeps an invalid committed payload from surfacing
/// as silent error arrivals inside a measured window: a refused example
/// stops seeding and names the payload family.
#[test]
fn a_refused_flat_preflight_payload_stops_the_ward_seed() {
    let sut = FakeSut::start();
    opt_upload_accepted(&sut);
    sut.mount(
        Mock::given(method("POST")).and(path("/ehr")).respond_with(
            ResponseTemplate::new(201).insert_header("Location", "http://sut/ehr/s1"),
        ),
    );
    // The typed commits pass; only the Simplified-FLAT channel is refused.
    sut.mount(
        Mock::given(method("POST"))
            .and(path("/ehr/s1/composition"))
            .and(wiremock::matchers::header(
                "Content-Type",
                "application/openehr.wt.flat+json",
            ))
            .respond_with(ResponseTemplate::new(422))
            // A lower number is a higher priority, so the FLAT-specific
            // stub wins over the catch-all commit below it.
            .with_priority(1),
    );
    sut.mount(
        Mock::given(method("POST"))
            .and(path("/ehr/s1/composition"))
            .respond_with(ResponseTemplate::new(201).insert_header("ETag", "W/\"x::stub::1\"")),
    );
    let error = seed_ward(&client(&sut), &mut seeded_index(1), &pack(), 1, &quiet)
        .expect_err("a refused FLAT example is a payload-ground defect");
    assert!(
        error.contains("Simplified-FLAT payload returned 422"),
        "{error}"
    );
}

#[test]
fn a_refused_tdd_preflight_payload_stops_the_ward_seed() {
    let sut = FakeSut::start();
    opt_upload_accepted(&sut);
    sut.mount(
        Mock::given(method("POST")).and(path("/ehr")).respond_with(
            ResponseTemplate::new(201).insert_header("Location", "http://sut/ehr/s1"),
        ),
    );
    sut.mount(
        Mock::given(method("POST"))
            .and(path("/ehr/s1/composition"))
            .respond_with(ResponseTemplate::new(201).insert_header("ETag", "W/\"x::stub::1\"")),
    );
    sut.mount(
        Mock::given(method("POST"))
            .and(path("/message/tdd/s1"))
            .respond_with(ResponseTemplate::new(400)),
    );
    let error = seed_ward(&client(&sut), &mut seeded_index(1), &pack(), 1, &quiet)
        .expect_err("a refused TDD example is a payload-ground defect");
    assert!(error.contains("TDD payload returned 400"), "{error}");
}

/// The preflight's own scratch EHR: a SUT that will not create one has
/// nothing the pack examples can be committed into.
#[test]
fn a_refused_preflight_scratch_ehr_stops_the_ward_seed() {
    let sut = FakeSut::start();
    opt_upload_accepted(&sut);
    sut.mount(
        Mock::given(method("POST"))
            .and(path("/ehr"))
            .respond_with(ResponseTemplate::new(500)),
    );
    let error = seed_ward(&client(&sut), &mut seeded_index(1), &pack(), 1, &quiet)
        .expect_err("the preflight needs a scratch EHR");
    assert!(
        error.contains("preflight EHR create returned 500"),
        "{error}"
    );
}

/// A SUT that stops answering: the very first provisioning call is a
/// transport fault, and seeding propagates it rather than treating an
/// unreachable server as an empty corpus.
#[test]
fn a_transport_fault_stops_the_scale_seed() {
    let ixit = ixit(&closed_port_url());
    let dead = PerfPrincipals::from_ixit(&ixit)
        .expect("the single-instance ixit resolves")
        .primary()
        .clone();
    let error = seed_scale_ladder(&dead, "cnf.scale.10k", "<opt/>", 1, 1, 1, &quiet)
        .expect_err("nothing is listening");
    assert!(error.contains("transport"), "{error}");
}

/// The first EHR create runs serially so a SUT's lazy per-principal
/// bookkeeping settles once; the rest fan out. A worker that fails inside
/// that fan-out stops the whole seed with the first reason, so a corpus with
/// a hole never reaches a window.
#[test]
fn a_worker_failure_in_the_ehr_fan_out_stops_the_scale_seed() {
    let sut = FakeSut::start();
    opt_upload_accepted(&sut);
    // The serial first create lands; every later one is refused.
    sut.mount(
        Mock::given(method("POST"))
            .and(path("/ehr"))
            .respond_with(ResponseTemplate::new(201).insert_header("Location", "http://sut/ehr/e1"))
            .up_to_n_times(1)
            .with_priority(1),
    );
    sut.mount(
        Mock::given(method("POST"))
            .and(path("/ehr"))
            .respond_with(ResponseTemplate::new(500)),
    );
    let error = seed_scale_ladder(&client(&sut), "cnf.scale.10k", "<opt/>", 4, 1, 2, &quiet)
        .expect_err("a refused create leaves a hole in the corpus");
    assert!(error.contains("create_ehr returned 500"), "{error}");
}

/// The per-patient ward state: each of the four documents the journey
/// stages address must land, and each names itself when it does not. A ward
/// missing one document would surface as error arrivals the SUT never
/// caused.
#[test]
fn each_refused_ward_document_stops_the_ward_seed_by_name() {
    for (target, status, expected) in [
        ("/ehr/ehr-0/composition", 500, "ward commit"),
        (
            "/ehr/ehr-0/directory",
            500,
            "ward directory create returned 500",
        ),
        (
            "/ehr/ehr-0/contribution",
            500,
            "ward contribution returned 500",
        ),
    ] {
        let sut = FakeSut::start();
        opt_upload_accepted(&sut);
        sut.mount(Mock::given(method("POST")).and(path("/ehr")).respond_with(
            ResponseTemplate::new(201).insert_header("Location", "http://sut/ehr/s1"),
        ));
        sut.mount(
            Mock::given(method("POST"))
                .and(path("/message/tdd/s1"))
                .respond_with(ResponseTemplate::new(201)),
        );
        sut.mount(
            Mock::given(method("PUT"))
                .and(path("/definition/query/org.openehr.cnf::ward_dashboard"))
                .respond_with(ResponseTemplate::new(200)),
        );
        // The refused document, ahead of the accepting catch-alls.
        sut.mount(
            Mock::given(method("POST"))
                .and(path(target))
                .respond_with(ResponseTemplate::new(status))
                .with_priority(1),
        );
        for accepted in ["/ehr/s1/composition", "/ehr/ehr-0/composition"] {
            sut.mount(
                Mock::given(method("POST"))
                    .and(path(accepted))
                    .respond_with(
                        ResponseTemplate::new(201).insert_header("ETag", "W/\"x::stub::1\""),
                    ),
            );
        }
        sut.mount(
            Mock::given(method("POST"))
                .and(path("/ehr/ehr-0/directory"))
                .respond_with(ResponseTemplate::new(201).insert_header("ETag", "W/\"d::stub::1\"")),
        );
        sut.mount(
            Mock::given(method("POST"))
                .and(path("/ehr/ehr-0/contribution"))
                .respond_with(
                    ResponseTemplate::new(201).insert_header("Location", "http://sut/contrib/c1"),
                ),
        );

        let error = seed_ward(&client(&sut), &mut seeded_index(1), &pack(), 1, &quiet)
            .expect_err("a refused ward document is not a seeded ward");
        assert!(error.contains(expected), "{error}");
    }
}
