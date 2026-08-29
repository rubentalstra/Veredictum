// SPDX-FileCopyrightText: Veredictum contributors
// SPDX-License-Identifier: Apache-2.0

//! End-to-end exercise of the performance measurement machinery against a
//! local stub SUT: the open-loop driver seeds the scale corpus + the
//! standing ward through the API surface, drives the case's journey
//! workload, and produces a re-checkable measurement whose class verdict
//! re-derives from the embedded HDR histograms — including the
//! falsifiability direction (a faulting SUT can never earn the class).

#![expect(
    clippy::unwrap_used,
    reason = "test-support helpers (not `#[test]` fns, so the clippy.toml in-tests scoping does not reach them) are panic-idiomatic: a broken stub must abort the test loudly, Book ch11"
)]

use std::io::{BufRead, BufReader, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};

use veredictum::ixit::{Containers, Environment, Ixit};
use veredictum::perf::{ArrivalCurve, ClassVerdict, JourneyCatalogue, PerformanceCase};
use veredictum::perf_run::client::{PerfClient, PerfPrincipals};
use veredictum::perf_run::corpus::{SeededCorpus, seed_scale_ladder, seed_ward};
use veredictum::perf_run::jitter::LeafConstraints;
use veredictum::perf_run::pack::{AuxPayloads, FlatPayload, JourneyPack, PackTemplate, TddPayload};
use veredictum::perf_run::schedule::JourneyWorkload;
use veredictum::perf_run::window::{
    drive_case, measured_run_context, rederive_verdict, run_window,
};
use veredictum::probe::{ProbeOptions, run_probe};
use veredictum::stress::{StressOptions, run_stress};

/// The stub's live fault levers. They are flipped mid-life rather than
/// fixed at construction, because seeding and the measured window drive the
/// same server: a fault armed before the seed would fail provisioning and
/// the window would never open at all.
#[derive(Debug, Default)]
pub(crate) struct StubFaults {
    /// Every `n`-th composition read answers 500 (`0` disables it).
    fail_every_nth_read: AtomicU64,
    /// Every write answers 500, so every created/updated family arm takes
    /// its false branch and every versioned update re-resolves.
    fail_writes: AtomicBool,
    /// Every request answers 429, the one status that invalidates a whole
    /// measured window.
    rate_limit: AtomicBool,
}

impl StubFaults {
    /// Answer 500 to every `n`-th composition read from now on.
    fn fail_every_nth_read(&self, n: u64) {
        self.fail_every_nth_read.store(n, Ordering::Relaxed);
    }

    /// Answer 500 to every write from now on.
    fn fail_writes(&self) {
        self.fail_writes.store(true, Ordering::Relaxed);
    }

    /// Answer 429 to everything from now on.
    fn rate_limit(&self) {
        self.rate_limit.store(true, Ordering::Relaxed);
    }
}

/// A minimal keep-alive HTTP stub realizing the journey wire shapes: OPT
/// upload 201, EHR create 201+Location, EHR/status/directory/contribution
/// reads 200, composition commit/contribution 201+ETag/Location, versioned
/// update 200+ETag, delete 204, queries/templates/tags 200. The returned
/// [`StubFaults`] bends those answers once a test arms it.
fn spawn_stub() -> (String, std::thread::JoinHandle<()>, Arc<StubFaults>) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let addr = listener.local_addr().unwrap();
    let reads = Arc::new(AtomicU64::new(0));
    let ehr_counter = Arc::new(AtomicU64::new(0));
    let uid_counter = Arc::new(AtomicU64::new(0));
    let faults = Arc::new(StubFaults::default());
    let served = Arc::clone(&faults);
    let handle = std::thread::spawn(move || {
        for stream in listener.incoming() {
            let Ok(stream) = stream else { break };
            let reads = Arc::clone(&reads);
            let ehr_counter = Arc::clone(&ehr_counter);
            let uid_counter = Arc::clone(&uid_counter);
            let faults = Arc::clone(&served);
            std::thread::spawn(move || {
                serve(stream, &reads, &ehr_counter, &uid_counter, &faults);
            });
        }
    });
    (format!("http://{addr}/base/v1"), handle, faults)
}

fn serve(
    stream: TcpStream,
    reads: &AtomicU64,
    ehr_counter: &AtomicU64,
    uid_counter: &AtomicU64,
    faults: &StubFaults,
) {
    let mut reader = BufReader::new(stream.try_clone().unwrap());
    let mut stream = stream;
    loop {
        let mut request_line = String::new();
        if reader.read_line(&mut request_line).unwrap_or(0) == 0 {
            return; // connection closed
        }
        let mut content_length = 0usize;
        let mut principal = String::new();
        loop {
            let mut header = String::new();
            if reader.read_line(&mut header).unwrap_or(0) == 0 {
                return;
            }
            if header == "\r\n" {
                break;
            }
            let lower = header.to_ascii_lowercase();
            if let Some(v) = lower.strip_prefix("content-length:") {
                content_length = v.trim().parse().unwrap_or(0);
            }
            if let Some(v) = lower.strip_prefix("x-cnf-stub-principal:") {
                v.trim().clone_into(&mut principal);
            }
        }
        let mut body = vec![0u8; content_length];
        if content_length > 0 && reader.read_exact(&mut body).is_err() {
            return;
        }

        let mut parts = request_line.split_whitespace();
        let method = parts.next().unwrap_or("");
        let path = parts.next().unwrap_or("");
        let (status, extra, body) = route(
            method,
            path,
            &principal,
            reads,
            ehr_counter,
            uid_counter,
            faults,
        );
        let response = format!(
            "HTTP/1.1 {status}\r\n{extra}Content-Type: application/json\r\nContent-Length: {}\r\n\r\n{body}",
            body.len(),
        );
        if stream.write_all(response.as_bytes()).is_err() {
            return;
        }
    }
}

#[expect(clippy::too_many_lines, reason = "one arm per stubbed wire shape")]
fn route(
    method: &str,
    path: &str,
    principal: &str,
    reads: &AtomicU64,
    ehr_counter: &AtomicU64,
    uid_counter: &AtomicU64,
    faults: &StubFaults,
) -> (&'static str, String, String) {
    let fresh_uid = |counter: &AtomicU64| {
        let n = counter.fetch_add(1, Ordering::Relaxed);
        format!("uid-{n}::stub::1")
    };
    if faults.rate_limit.load(Ordering::Relaxed) {
        return ("429 Too Many Requests", String::new(), "{}".to_owned());
    }
    if faults.fail_writes.load(Ordering::Relaxed) && method != "GET" && method != "OPTIONS" {
        return ("500 Internal Server Error", String::new(), "{}".to_owned());
    }
    // The boundary principals: a credential-less caller is refused outright,
    // and a read-only caller is refused any write — the DENY branches the
    // access-control probe measures.
    match principal {
        "unauthenticated" => return ("401 Unauthorized", String::new(), "{}".to_owned()),
        "readonly" if method != "GET" => {
            return ("403 Forbidden", String::new(), "{}".to_owned());
        }
        _ => {}
    }
    match (method, path) {
        ("GET", p) if p.ends_with("/definition/archetype/adl2") => {
            ("200 OK", String::new(), "[]".to_owned())
        }
        ("GET", p) if p.contains("/admin/report/contribution/count") => {
            ("200 OK", String::new(), "{\"count\":0}".to_owned())
        }
        ("OPTIONS", _) => (
            "200 OK",
            "Allow: GET, OPTIONS\r\n".to_owned(),
            "{}".to_owned(),
        ),
        ("GET", p) if p.ends_with("/.well-known/smart-configuration") => {
            ("200 OK", String::new(), "{}".to_owned())
        }
        ("GET", p) if p.ends_with("/definition/template/adl2") => {
            ("200 OK", String::new(), "[]".to_owned())
        }
        ("POST", p) if p.ends_with("/demographic/person") => (
            "201 Created",
            format!("ETag: W/\"{}\"\r\n", fresh_uid(uid_counter)),
            String::new(),
        ),
        ("PUT", p) if p.contains("/demographic/person/") => (
            "200 OK",
            format!("ETag: W/\"{}\"\r\n", fresh_uid(uid_counter)),
            String::new(),
        ),
        ("GET", p) if p.contains("/demographic/person/") => {
            ("200 OK", String::new(), "{\"_type\":\"PERSON\"}".to_owned())
        }
        ("POST", p) if p.ends_with("/demographic/party_relationship") => (
            "201 Created",
            format!("ETag: W/\"{}\"\r\n", fresh_uid(uid_counter)),
            String::new(),
        ),
        ("GET", p) if p.contains("/demographic/party_relationship/") => (
            "200 OK",
            String::new(),
            "{\"_type\":\"PARTY_RELATIONSHIP\"}".to_owned(),
        ),
        ("POST", p) if p.ends_with("/definition/template/adl1.4") => {
            ("201 Created", String::new(), String::new())
        }
        ("GET", p) if p.ends_with("/definition/template/adl1.4") => {
            ("200 OK", String::new(), "[]".to_owned())
        }
        ("GET", p) if p.contains("/definition/template/adl1.4/") => {
            ("200 OK", String::new(), "{}".to_owned())
        }
        ("PUT", p) if p.contains("/definition/query/") => {
            ("200 OK", String::new(), "{}".to_owned())
        }
        ("POST", p) if p.ends_with("/ehr") => {
            let n = ehr_counter.fetch_add(1, Ordering::Relaxed);
            (
                "201 Created",
                format!("Location: http://stub/base/v1/ehr/ehr-{n}\r\n"),
                String::new(),
            )
        }
        ("GET", p) if p.contains("/ehr_status") => (
            "200 OK",
            format!("ETag: \"{}\"\r\n", fresh_uid(uid_counter)),
            "{\"_type\":\"EHR_STATUS\"}".to_owned(),
        ),
        ("PUT", p) if p.contains("/ehr_status") => (
            "200 OK",
            format!("ETag: \"{}\"\r\n", fresh_uid(uid_counter)),
            String::new(),
        ),
        ("POST", p) if p.ends_with("/directory") => (
            "201 Created",
            format!("ETag: \"{}\"\r\n", fresh_uid(uid_counter)),
            String::new(),
        ),
        ("GET", p) if p.ends_with("/directory") => (
            "200 OK",
            format!("ETag: \"{}\"\r\n", fresh_uid(uid_counter)),
            "{\"_type\":\"FOLDER\"}".to_owned(),
        ),
        ("PUT", p) if p.ends_with("/directory") => (
            "200 OK",
            format!("ETag: \"{}\"\r\n", fresh_uid(uid_counter)),
            String::new(),
        ),
        ("POST", p) if p.ends_with("/contribution") => {
            let n = uid_counter.fetch_add(1, Ordering::Relaxed);
            (
                "201 Created",
                format!("Location: http://stub/base/v1/contribution/contrib-{n}\r\n"),
                String::new(),
            )
        }
        ("GET", p) if p.contains("/contribution/") => (
            "200 OK",
            String::new(),
            "{\"_type\":\"CONTRIBUTION\"}".to_owned(),
        ),
        ("POST", p) if p.ends_with("/composition") => (
            "201 Created",
            format!("ETag: W/\"{}\"\r\n", fresh_uid(uid_counter)),
            String::new(),
        ),
        // The MESSAGE extension pair (register AMB-34): a TDD import commits
        // and names the created version; a whole-EHR export reads a list.
        ("POST", p) if p.contains("/message/tdd/") => (
            "201 Created",
            String::new(),
            format!("{{\"uid\":\"{}\"}}", fresh_uid(uid_counter)),
        ),
        ("GET", p) if p.contains("/message/export/") => (
            "200 OK",
            String::new(),
            "[{\"_type\":\"EXTRACT\"}]".to_owned(),
        ),
        ("PUT", p) if p.contains("/composition/") && p.ends_with("/tags") => {
            ("200 OK", String::new(), "[]".to_owned())
        }
        ("GET", p) if p.contains("/composition/") && p.ends_with("/tags") => {
            ("200 OK", String::new(), "[]".to_owned())
        }
        ("DELETE", p) if p.contains("/composition/") => {
            ("204 No Content", String::new(), String::new())
        }
        ("PUT", p) if p.contains("/composition/") => (
            "200 OK",
            format!("ETag: \"{}\"\r\n", fresh_uid(uid_counter)),
            String::new(),
        ),
        ("GET", p) if p.contains("/versioned_composition/") => {
            ("200 OK", String::new(), "{}".to_owned())
        }
        ("GET", p) if p.contains("/composition/") => {
            let n = reads.fetch_add(1, Ordering::Relaxed) + 1;
            let fail_every_nth_read = faults.fail_every_nth_read.load(Ordering::Relaxed);
            if fail_every_nth_read != 0 && n.is_multiple_of(fail_every_nth_read) {
                ("500 Internal Server Error", String::new(), "{}".to_owned())
            } else {
                (
                    "200 OK",
                    String::new(),
                    "{\"_type\":\"COMPOSITION\"}".to_owned(),
                )
            }
        }
        ("GET", p) if p.contains("/ehr/") => {
            ("200 OK", String::new(), "{\"_type\":\"EHR\"}".to_owned())
        }
        ("POST", p) if p.ends_with("/query/aql") => {
            ("200 OK", String::new(), "{\"rows\":[]}".to_owned())
        }
        ("GET", p) if p.contains("/query/") => {
            ("200 OK", String::new(), "{\"rows\":[]}".to_owned())
        }
        _ => ("404 Not Found", String::new(), String::new()),
    }
}

/// A compact journey catalogue exercising every dependency shape: standing
/// ward reads + versioned updates, a fresh-EHR admission chain, an
/// order→result pipeline with an in-window dependent stage, the governance
/// surface, the demographic chain, the Simplified-Format channel, the
/// definition/platform probes, and the two access-control DENY branches.
/// The deletion chain, the directory version chain and the extension routes
/// live in [`extension_catalogue`] instead, because this mix is what holds
/// the class case inside its write-share band.
fn catalogue() -> JourneyCatalogue {
    serde_saphyr::from_str(
        "chart_review:\n  description: d\n  derivation: g\n  stages:\n    - { op: composition_read, at: PT0S }\n    - { op: composition_revision_history, at: PT1S }\n    - { op: adhoc_query, at: PT2S }\n    - { op: directory_read, at: PT3S }\nadmission:\n  description: d\n  derivation: g\n  stages:\n    - { op: ehr_create, at: PT0S }\n    - { op: ehr_read, at: PT1S }\n    - { op: ehr_status_read, at: PT2S }\n    - { op: ehr_status_update, at: PT3S }\n    - { op: composition_commit, template: cnf.ckm.gp_data_set, at: PT4S }\n    - { op: directory_create, at: PT5S }\nlab_pipeline:\n  description: d\n  derivation: g\n  stages:\n    - { op: composition_commit, template: cnf.ckm.gp_data_set, at: PT0S }\n    - { op: contribution_commit, template: cnf.ckm.lab_result, at: { uniform: [PT2S, PT3S] } }\n    - { op: composition_read_current, at: PT4S }\n    - { op: contribution_read, at: PT5S }\ncorrection:\n  description: d\n  derivation: g\n  stages:\n    - { op: composition_read_current, at: PT0S }\n    - { op: composition_update, template: cnf.ckm.gp_data_set, at: PT1S }\nward_dashboard:\n  description: d\n  derivation: g\n  stages:\n    - { op: ward_query, at: PT0S }\n    - { op: stored_query_execute, at: PT1S }\n    - { op: template_list, at: PT2S }\n    - { op: template_get, at: PT3S }\n    - { op: tags_put, at: PT4S }\n    - { op: tags_read, at: PT5S }\ndemographic_admission:\n  description: d\n  derivation: g\n  stages:\n    - { op: party_create, at: PT0S }\n    - { op: party_read, at: PT1S }\n    - { op: party_relationship_create, at: PT2S }\n    - { op: party_relationship_read, at: PT3S }\n    - { op: party_update, at: PT4S }\nplatform_surface:\n  description: d\n  derivation: g\n  stages:\n    - { op: system_options, at: PT0S }\n    - { op: smart_configuration_read, at: PT1S }\n    - { op: template_example, at: PT2S }\n    - { op: template_adl2_list, at: PT3S }\n    - { op: analytics_query, at: PT4S }\n    - { op: terminology_query, at: PT5S }\nsimplified_formats_exchange:\n  description: d\n  derivation: g\n  stages:\n    - { op: composition_commit_flat, at: PT0S }\n    - { op: composition_read_flat, at: PT1S }\n    - { op: composition_version_read, at: PT2S }\naccess_control_probe:\n  description: d\n  derivation: g\n  stages:\n    - { op: unauthenticated_probe, at: PT0S }\n    - { op: readonly_write_denied, template: cnf.ckm.gp_data_set, at: PT1S }\n",
    )
    .unwrap()
}

fn journey_pack() -> JourneyPack {
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
            template(
                "cnf.ckm.lab_result",
                "Generic lab test result example simple",
            ),
            template("cnf.ckm.medicines_list", "Medicines list item R1"),
        ],
        aux: AuxPayloads {
            flat: Some(FlatPayload {
                template_id: "minimal_action.en.v1".to_owned(),
                opt_xml: "<template/>".to_owned(),
                body: serde_json::json!({ "ctx/language": "en" }),
            }),
            person: Some(serde_json::json!({
                "_type": "PERSON",
                "identities": [{ "_type": "PARTY_IDENTITY",
                    "details": { "_type": "ITEM_TREE", "items": [{ "_type": "ELEMENT",
                        "value": { "_type": "DV_TEXT", "value": "Person One" } }] } }]
            })),
            person_amended: Some(serde_json::json!({
                "_type": "PERSON",
                "identities": [{ "_type": "PARTY_IDENTITY",
                    "details": { "_type": "ITEM_TREE", "items": [{ "_type": "ELEMENT",
                        "value": { "_type": "DV_TEXT", "value": "Person Two" } }] } }]
            })),
            party_relationship: Some(serde_json::json!({
                "_type": "PARTY_RELATIONSHIP",
                "source": { "_type": "PARTY_REF",
                    "id": { "_type": "HIER_OBJECT_ID", "value": "placeholder" } }
            })),
            tdd: Some(TddPayload {
                opt_xml: "<template/>".to_owned(),
                document: "<Nested xmlns=\"http://schemas.oceanehr.com/templates\" \
                            template_id=\"nested.en.v1\"/>"
                    .to_owned(),
            }),
        },
    }
}

fn poc_case() -> PerformanceCase {
    serde_saphyr::from_str(
        "id: PERF-hospital_sim-class_POC\nkind: performance\ncomponent: PERFORMANCE\ndescription: d\ntest_purpose: t\nspec_refs: [\"CNF 2.0 performance schedule\"]\nclass: POC\ncorpus: cnf.scale.10k\nworkload:\n  arrival_rate: 60/s\n  warmup: PT5M\n  duration: PT1H\n  journeys: { chart_review: 78%, admission: 3%, lab_pipeline: 2%, correction: 2%, ward_dashboard: 5%, demographic_admission: 3%, platform_surface: 3%, simplified_formats_exchange: 2%, access_control_probe: 2% }\nthresholds:\n  - { metric: latency_p99, max: 1000 }\n  - { metric: error_rate, max: 0 }\n  - { metric: offered_load_sustained, min: 2 }\n",
    )
    .unwrap()
}

fn client_and_env(base_url: &str) -> (PerfPrincipals, Environment) {
    let ixit: Ixit = serde_json::from_value(serde_json::json!({
        "instances": {
            "sut": { "base_url": base_url, "auth": { "mode": "none" } },
            // The boundary principals the access-control probe addresses. The
            // stub answers 401/403 on their marker header, so the DENY
            // branches are measured exactly as a real deployment's are.
            "unauthenticated": { "base_url": base_url, "auth": { "mode": "none" },
                                  "headers": { "x-cnf-stub-principal": "unauthenticated" } },
            "readonly": { "base_url": base_url, "auth": { "mode": "none" },
                           "headers": { "x-cnf-stub-principal": "readonly" } },
            "smart_platform": { "base_url": base_url, "auth": { "mode": "none" } },
            // The admin-gated extension route's principal.
            "admin": { "base_url": base_url, "auth": { "mode": "none" },
                        "headers": { "x-cnf-stub-principal": "admin" } }
        },
        "smart": {
            "platform_instance": "smart_platform",
            "mint": { "issuer": "https://as.stub", "subject": "stub",
                       "key_file": "unused.pem", "kid": "stub", "ttl_seconds": 300 }
        },
        "environment": { "exclusive_server": true, "hardware_class": "test-stub",
                          "cores": 1, "memory_gb": 1, "storage_class": "ram",
                          "topology": "in-process stub" }
    }))
    .unwrap();
    let principals = PerfPrincipals::from_ixit(&ixit).unwrap();
    let environment = ixit.environment.clone().unwrap();
    (principals, environment)
}

fn seeded(client: &PerfClient, pack: &JourneyPack) -> SeededCorpus {
    let progress = |_message: String| {};
    let mut corpus =
        seed_scale_ladder(client, "cnf.scale.10k", "<opt/>", 6, 3, 4, &progress).unwrap();
    assert_eq!(
        corpus.ehr_ids.len(),
        6,
        "scale ladder seeds one EHR per requested slot"
    );
    assert_eq!(
        corpus.compositions.len(),
        18,
        "scale ladder seeds versions_per_ehr compositions per EHR"
    );
    seed_ward(client, &mut corpus, pack, 4, &progress).unwrap();
    assert_eq!(
        corpus.ward.len(),
        6,
        "the standing ward covers every seeded EHR"
    );
    corpus
}

#[test]
fn the_open_loop_journey_run_earns_the_class_on_a_healthy_sut() {
    let (base_url, _server, _faults) = spawn_stub();
    let (principals, environment) = client_and_env(&base_url);
    let progress = |_message: String| {};
    let pack = journey_pack();
    let catalogue = catalogue();
    let corpus = seeded(principals.primary(), &pack);

    let case = poc_case();
    let measurement = drive_case(
        &case,
        &principals,
        &corpus,
        &pack,
        &catalogue,
        &environment,
        1,
        8,
        &progress,
    )
    .unwrap();

    // The schedule dispatched the planned aggregate operation rate.
    assert!(
        measurement.offered_load_sustained >= 57.0,
        "offered load {} below the schedule rate",
        measurement.offered_load_sustained
    );
    // The full journey surface was measured: every catalogue operation
    // label appears, with zero errors, and every histogram re-checks.
    let labels: Vec<&str> = measurement
        .operations
        .iter()
        .map(|o| o.operation.as_str())
        .collect();
    for expected in [
        "ehr_create",
        "ehr_read",
        "ehr_status_read",
        "ehr_status_update",
        "composition_commit",
        "composition_read",
        "composition_read_current",
        "composition_revision_history",
        "composition_update",
        "directory_create",
        "directory_read",
        "contribution_commit",
        "contribution_read",
        "adhoc_query",
        "ward_query",
        "stored_query_execute",
        "template_list",
        "template_get",
        "tags_put",
        "tags_read",
        "party_create",
        "party_read",
        "party_update",
        "party_relationship_create",
        "party_relationship_read",
        "system_options",
        "smart_configuration_read",
        "template_example",
        "template_adl2_list",
        "analytics_query",
        "terminology_query",
        "composition_commit_flat",
        "composition_read_flat",
        "composition_version_read",
        "unauthenticated_probe",
        "readonly_write_denied",
    ] {
        assert!(labels.contains(&expected), "operation {expected} missing");
    }
    for op in &measurement.operations {
        assert_eq!(op.errors, 0, "{} saw errors", op.operation);
        let histogram = op.decode_histogram().unwrap();
        assert_eq!(histogram.len(), op.requests);
    }
    assert_eq!(measurement.verdict, ClassVerdict::Earned);
    assert!(measurement.violations.is_empty());
    assert_eq!(measurement.environment.hardware_class, "test-stub");

    // The verdict pipeline's re-derivation agrees with the stored verdict.
    let (rederived, violations) = rederive_verdict(&case, &measurement).unwrap();
    assert_eq!(rederived, ClassVerdict::Earned);
    assert!(violations.is_empty());
}

#[test]
fn a_faulting_sut_cannot_earn_the_class() {
    let (base_url, _server, faults) = spawn_stub();
    faults.fail_every_nth_read(5); // every 5th composition read is a 500
    let (principals, environment) = client_and_env(&base_url);
    let progress = |_message: String| {};
    let pack = journey_pack();
    let catalogue = catalogue();
    let corpus = seeded(principals.primary(), &pack);

    let case = poc_case();
    let measurement = drive_case(
        &case,
        &principals,
        &corpus,
        &pack,
        &catalogue,
        &environment,
        0,
        8,
        &progress,
    )
    .unwrap();

    let errors: u64 = measurement.operations.iter().map(|o| o.errors).sum();
    assert!(errors > 0, "the faulting stub produced no errors");
    assert_eq!(measurement.verdict, ClassVerdict::NotEarned);
    assert!(
        measurement
            .violations
            .iter()
            .any(|v| v.contains("error_rate")),
        "violations {:?} name no error_rate breach",
        measurement.violations
    );
}

/// The journeys the class case does not carry: the deletion chain (which
/// deletes its OWN commit, so it resolves nothing from the standing ward),
/// the episode-directory version chain, and the four extension routes the
/// wire surface declares. They ride a dedicated short window rather than the
/// class case, because the class case's write-share band is what fixes its
/// journey mix.
fn extension_catalogue() -> JourneyCatalogue {
    serde_saphyr::from_str(
        "chart_review:\n  description: d\n  derivation: g\n  stages:\n    - { op: composition_read, at: PT0S }\n    - { op: composition_revision_history, at: PT1S }\n    - { op: adhoc_query, at: PT2S }\n    - { op: directory_read, at: PT3S }\n    - { op: ward_query, at: PT1S }\n    - { op: template_list, at: PT2S }\ndeletion:\n  description: d\n  derivation: g\n  stages:\n    - { op: composition_commit, template: cnf.ckm.gp_data_set, at: PT0S }\n    - { op: composition_delete, at: PT1S }\nepisode_management:\n  description: d\n  derivation: g\n  stages:\n    - { op: directory_read, at: PT0S }\n    - { op: directory_update, at: PT1S }\nextension_surface:\n  description: d\n  derivation: g\n  stages:\n    - { op: archetype_adl2_list, at: PT0S }\n    - { op: admin_contribution_report, at: PT1S }\n    - { op: ehr_extract_export, at: PT2S }\n    - { op: tdd_import, at: PT3S }\n",
    )
    .unwrap()
}

/// The share mix of [`extension_catalogue`], inside the reconciliation
/// band the expansion enforces.
fn extension_shares() -> Vec<(String, veredictum::perf::Percent)> {
    [
        ("chart_review", 70.0),
        ("deletion", 10.0),
        ("episode_management", 10.0),
        ("extension_surface", 10.0),
    ]
    .into_iter()
    .map(|(name, share)| (name.to_owned(), veredictum::perf::Percent(share)))
    .collect()
}

#[test]
fn the_deletion_directory_and_extension_arms_all_drive_their_wire() {
    let (base_url, _server, _faults) = spawn_stub();
    let (principals, _environment) = client_and_env(&base_url);
    let progress = |_message: String| {};
    let pack = journey_pack();
    let catalogue = extension_catalogue();
    let corpus = seeded(principals.primary(), &pack);

    let shares = extension_shares();
    let workload = JourneyWorkload {
        catalogue: &catalogue,
        shares: &shares,
        pack: &pack,
        curve: ArrivalCurve::Uniform,
        principals: &principals,
    };
    let window = run_window(&principals, &corpus, &workload, 100.0, 0, 4, &progress).unwrap();

    let labels: Vec<&str> = window
        .operations
        .iter()
        .map(|o| o.operation.as_str())
        .collect();
    for expected in [
        "composition_delete",
        "directory_update",
        "archetype_adl2_list",
        "admin_contribution_report",
        "ehr_extract_export",
        "tdd_import",
    ] {
        assert!(labels.contains(&expected), "operation {expected} missing");
    }
    for op in &window.operations {
        assert_eq!(
            op.errors, 0,
            "{} saw errors on a healthy stub",
            op.operation
        );
    }
}

/// A SUT that refuses every write: each created/updated acceptance family
/// takes its false branch, every versioned update re-resolves the current
/// version instead of advancing blindly, and the reads keep passing — so a
/// write outage is visible per operation rather than as one global number.
#[test]
fn a_write_refusing_sut_records_the_writes_as_errors_and_leaves_reads_clean() {
    let (base_url, _server, faults) = spawn_stub();
    let (principals, _environment) = client_and_env(&base_url);
    let progress = |_message: String| {};
    let pack = journey_pack();
    let catalogue = catalogue();
    // Seeding runs against the healthy stub; the outage starts with the
    // window, which is the only order that leaves a corpus to measure over.
    let corpus = seeded(principals.primary(), &pack);
    faults.fail_writes();

    let shares = poc_case().workload.journeys.clone();
    let workload = JourneyWorkload {
        catalogue: &catalogue,
        shares: &shares,
        pack: &pack,
        curve: ArrivalCurve::Uniform,
        principals: &principals,
    };
    let window = run_window(&principals, &corpus, &workload, 60.0, 0, 4, &progress).unwrap();

    let by_op = |name: &str| window.operations.iter().find(|o| o.operation == name);
    let commit = by_op("composition_commit").expect("the commit stages were scheduled");
    assert_eq!(
        commit.errors, commit.requests,
        "every refused commit is an error arrival"
    );
    let read = by_op("composition_read").expect("the read stages were scheduled");
    assert_eq!(read.errors, 0, "the reads were never refused");
}

/// The one status that invalidates a whole measured window. It is observed
/// per arrival and latched process-wide, so both instruments can refuse to
/// publish a record that would describe a rate limiter's ceiling.
#[test]
fn a_rate_limited_arrival_latches_the_run_wide_refusal() {
    let (base_url, _server, faults) = spawn_stub();
    let (principals, _environment) = client_and_env(&base_url);
    let progress = |_message: String| {};
    let pack = journey_pack();
    let catalogue = catalogue();
    let corpus = seeded(principals.primary(), &pack);
    faults.rate_limit();

    let shares = poc_case().workload.journeys.clone();
    let workload = JourneyWorkload {
        catalogue: &catalogue,
        shares: &shares,
        pack: &pack,
        curve: ArrivalCurve::Uniform,
        principals: &principals,
    };
    let window = run_window(&principals, &corpus, &workload, 60.0, 0, 2, &progress).unwrap();

    assert!(
        window.operations.iter().all(|o| o.errors == o.requests),
        "a 429 is never a passing arrival"
    );
    assert!(
        veredictum::perf_run::rate_limited_observed(),
        "the 429 observation did not latch"
    );
    let refusal = veredictum::perf_run::rate_limited_refusal("perf");
    assert!(refusal.starts_with("perf: "), "{refusal}");
    assert!(refusal.contains("429"), "{refusal}");
}

/// Two window facts a uniform, fully declared run never shows: the diurnal
/// curve reports the planned busy hour rather than the mean rate, and a
/// journey whose principal the ixit leaves undeclared is dropped by name
/// with the remaining shares renormalized.
#[test]
fn the_diurnal_curve_runs_and_an_undeclared_principal_drops_its_journey() {
    let (base_url, _server, _faults) = spawn_stub();
    let (declared, _environment) = client_and_env(&base_url);
    let pack = journey_pack();
    let catalogue = catalogue();
    let corpus = seeded(declared.primary(), &pack);

    // Only the default `sut` instance: the access-control probe addresses
    // the boundary principals, so its journey cannot be scheduled.
    let principals = PerfPrincipals::single(declared.primary().clone());
    let notes = std::sync::Mutex::new(Vec::new());
    let progress = |message: String| {
        notes
            .lock()
            .expect("the progress lock is uncontended")
            .push(message);
    };
    let shares = poc_case().workload.journeys.clone();
    let workload = JourneyWorkload {
        catalogue: &catalogue,
        shares: &shares,
        pack: &pack,
        curve: ArrivalCurve::Diurnal,
        principals: &principals,
    };
    let window = run_window(&principals, &corpus, &workload, 60.0, 0, 3, &progress).unwrap();

    let seen = notes.lock().expect("the progress lock is uncontended");
    assert!(
        seen.iter()
            .any(|m| m.contains("journeys not scheduled") && m.contains("access_control_probe")),
        "the dropped journey was not named: {seen:?}"
    );
    assert!(
        window
            .operations
            .iter()
            .all(|o| o.operation != "unauthenticated_probe"),
        "a dropped journey still produced arrivals"
    );
    assert!(
        window.offered_load_sustained > 0.0,
        "the diurnal window reported no busy-hour load"
    );
}

/// An arrival the generator cannot fire is an INSTRUMENT fault, never a
/// wire observation about the server: the principal set driving the window
/// declares no instance for the access-control probe's boundary
/// principals, so those arrivals never reach the SUT at all. The window
/// refuses to publish a measurement and names how many arrivals it lost.
#[test]
fn arrivals_the_generator_cannot_fire_fail_the_window_with_their_count() {
    let (base_url, _server, _faults) = spawn_stub();
    let (declared, _environment) = client_and_env(&base_url);
    let notes = std::sync::Mutex::new(Vec::new());
    let progress = |message: String| {
        notes
            .lock()
            .expect("the progress lock is uncontended")
            .push(message);
    };
    let pack = journey_pack();
    let catalogue = catalogue();
    let corpus = seeded(declared.primary(), &pack);

    // The workload is PLANNED against the full declaration, so the probe
    // journey survives the schedulable filter; the window is DRIVEN by the
    // default instance alone, so the probe's arrivals have no client to
    // fire through.
    let driving = PerfPrincipals::single(declared.primary().clone());
    // The correction journey carries the one write, at 6.25% of the
    // expanded arrivals, which is inside the read:write derivation band the
    // expansion enforces.
    let shares = vec![
        ("chart_review".to_owned(), veredictum::perf::Percent(60.0)),
        ("correction".to_owned(), veredictum::perf::Percent(20.0)),
        (
            "access_control_probe".to_owned(),
            veredictum::perf::Percent(20.0),
        ),
    ];
    let workload = JourneyWorkload {
        catalogue: &catalogue,
        shares: &shares,
        pack: &pack,
        curve: ArrivalCurve::Uniform,
        principals: &declared,
    };
    let failure = run_window(&driving, &corpus, &workload, 100.0, 0, 3, &progress).unwrap_err();

    assert!(
        failure.contains("generator faults"),
        "the window failed for another reason: {failure}"
    );
    let counted: u64 = failure
        .split_whitespace()
        .next()
        .and_then(|n| n.parse().ok())
        .unwrap_or(0);
    assert!(counted > 0, "the failure names no fault count: {failure}");
    assert!(
        failure.contains("unauthenticated_probe"),
        "the failure names no faulting operation: {failure}"
    );
    let seen = notes.lock().expect("the progress lock is uncontended");
    assert!(
        seen.iter().any(|m| m.starts_with("arrival not fired")),
        "no unfired arrival was sampled to the progress channel: {seen:?}"
    );
}

/// A SUT that has stopped answering: every arrival's request is a transport
/// fault, so every stage propagates it as an error observation instead of a
/// run failure. That distinction is the attribution law on the measured
/// side — a transport fault is inconclusive, never a SUT verdict — so the
/// window must still close and still produce a record.
#[test]
fn a_sut_that_stops_answering_records_transport_faults_as_error_arrivals() {
    let (base_url, _server, _faults) = spawn_stub();
    let (live, _environment) = client_and_env(&base_url);
    let progress = |_message: String| {};
    let pack = journey_pack();
    let catalogue = catalogue();
    let corpus = seeded(live.primary(), &pack);

    // The corpus is real; the principal the window drives is not listening.
    let dead: Ixit = serde_json::from_value(serde_json::json!({
        "instances": { "sut": { "base_url": crate::fake_sut::closed_port_url(),
                                 "auth": { "mode": "none" } } }
    }))
    .unwrap();
    let dead = PerfPrincipals::from_ixit(&dead).unwrap();

    let shares = vec![
        ("chart_review".to_owned(), veredictum::perf::Percent(90.0)),
        ("correction".to_owned(), veredictum::perf::Percent(10.0)),
    ];
    let workload = JourneyWorkload {
        catalogue: &catalogue,
        shares: &shares,
        pack: &pack,
        curve: ArrivalCurve::Uniform,
        principals: &dead,
    };
    let window = run_window(&dead, &corpus, &workload, 40.0, 0, 2, &progress).unwrap();

    assert!(
        !window.operations.is_empty(),
        "an unreachable SUT still produces a measured record"
    );
    assert!(
        window.operations.iter().all(|o| o.errors == o.requests),
        "a transport fault is never a passing arrival"
    );
}

/// The measured-run precondition: every declared principal resolves, and the
/// environment block is mandatory, because a throughput number without the
/// deployment it was measured in describes nothing.
#[test]
fn the_measured_run_context_requires_a_declared_environment() {
    let (base_url, _server, _faults) = spawn_stub();
    let ixit: Ixit = serde_json::from_value(serde_json::json!({
        "instances": { "sut": { "base_url": base_url, "auth": { "mode": "none" } } },
        "environment": { "hardware_class": "test-stub", "cores": 1, "memory_gb": 1,
                          "storage_class": "ram", "topology": "in-process stub" }
    }))
    .unwrap();
    let (principals, environment) = measured_run_context(&ixit).unwrap();
    assert!(principals.declares(veredictum::perf::Principal::Primary));
    assert_eq!(environment.hardware_class, "test-stub");

    let bare: Ixit = serde_json::from_value(serde_json::json!({
        "instances": { "sut": { "base_url": "http://stub", "auth": { "mode": "none" } } }
    }))
    .unwrap();
    let error = measured_run_context(&bare).expect_err("a measured run needs its environment");
    assert!(error.contains("no environment block"), "{error}");
}

/// The step-load ladder against a healthy stub: it climbs geometrically,
/// reports the cap it hit, and says in its own remark that it earns
/// nothing. A stress report is exploration, so nothing here is a verdict.
#[test]
fn the_stress_ladder_climbs_to_its_cap_and_earns_nothing() {
    let (base_url, _server, _faults) = spawn_stub();
    let (principals, environment) = client_and_env(&base_url);
    let pack = journey_pack();
    let catalogue = catalogue();
    let corpus = seeded(principals.primary(), &pack);

    let notes = std::sync::Mutex::new(Vec::new());
    let progress = |message: String| {
        notes
            .lock()
            .expect("the progress lock is uncontended")
            .push(message);
    };
    let shares = poc_case().workload.journeys.clone();
    let workload = JourneyWorkload {
        catalogue: &catalogue,
        shares: &shares,
        pack: &pack,
        curve: ArrivalCurve::Uniform,
        principals: &principals,
    };
    let options = StressOptions {
        start_rate: 4.0,
        max_rate: 4.0,
        step_warmup_s: 1,
        step_hold_s: 2,
        bisections: 0,
        ..StressOptions::default()
    };
    let report = run_stress(
        &principals,
        &corpus,
        &workload,
        &environment,
        None,
        &options,
        &progress,
    )
    .unwrap();

    assert_eq!(report.steps.len(), 1, "one rung below the cap");
    assert!(report.ladder_capped, "the climb stopped at the cap");
    assert!(
        (report.max_sustainable_throughput_per_s - 4.0).abs() < f64::EPSILON,
        "throughput {}",
        report.max_sustainable_throughput_per_s
    );
    assert!(
        report.remark.contains("Exploration only"),
        "{}",
        report.remark
    );
    assert_eq!(report.corpus, corpus.corpus);
    let seen = notes.lock().expect("the progress lock is uncontended");
    assert!(
        seen.iter()
            .any(|m| m.contains("no ixit `containers` block")),
        "the absent telemetry capability was not reported: {seen:?}"
    );
    assert!(seen.iter().any(|m| m.starts_with("recap:")), "{seen:?}");
}

/// A budget nothing can hold: the first rung breaches, the ladder bisects
/// between zero and it, and the report names no sustainable throughput. The
/// ixit here DOES declare containers, so the resource sampler and the
/// maintenance settling both run, and both degrade to a named note rather
/// than failing the rung when the runtime cannot answer for those names.
#[test]
fn an_unholdable_budget_breaches_the_first_rung_and_bisects() {
    let (base_url, _server, _faults) = spawn_stub();
    let (principals, environment) = client_and_env(&base_url);
    let pack = journey_pack();
    let catalogue = catalogue();
    let corpus = seeded(principals.primary(), &pack);

    let notes = std::sync::Mutex::new(Vec::new());
    let progress = |message: String| {
        notes
            .lock()
            .expect("the progress lock is uncontended")
            .push(message);
    };
    let shares = poc_case().workload.journeys.clone();
    let workload = JourneyWorkload {
        catalogue: &catalogue,
        shares: &shares,
        pack: &pack,
        curve: ArrivalCurve::Uniform,
        principals: &principals,
    };
    let containers = Containers {
        sut: "veredictum-no-such-sut-container".to_owned(),
        db: "veredictum-no-such-db-container".to_owned(),
    };
    let options = StressOptions {
        start_rate: 4.0,
        max_rate: 64.0,
        step_warmup_s: 1,
        step_hold_s: 2,
        bisections: 1,
        p99_budget_ms: 0.0,
        ..StressOptions::default()
    };
    let report = run_stress(
        &principals,
        &corpus,
        &workload,
        &environment,
        Some(&containers),
        &options,
        &progress,
    )
    .unwrap();

    assert_eq!(
        report.steps.len(),
        2,
        "the breached rung plus one bisection"
    );
    assert!(!report.ladder_capped, "the ladder breached before its cap");
    assert!(
        report.max_sustainable_throughput_per_s.abs() < f64::EPSILON,
        "nothing held the envelope, so nothing is sustainable"
    );
    assert!(
        report
            .steps
            .iter()
            .all(|s| !s.stable && !s.breaches.is_empty()),
        "a breached rung names its breach"
    );
    assert!(
        report.steps.iter().all(|s| s.resources.is_none()),
        "an unreachable container runtime fabricates no telemetry"
    );
    let seen = notes.lock().expect("the progress lock is uncontended");
    assert!(
        seen.iter().any(|m| m.contains("maintenance not settled")),
        "the unreachable DB container was not reported: {seen:?}"
    );
    assert!(
        seen.iter().any(|m| m.contains("bisecting between")),
        "{seen:?}"
    );
}

/// The AQL probe over a seeded corpus with no container capability: wire
/// percentiles for each of the three probes, and the attribution field
/// saying honestly that DB-side cost could not be attributed.
#[test]
fn the_aql_probe_reports_wire_percentiles_and_names_its_missing_attribution() {
    let (base_url, _server, _faults) = spawn_stub();
    let (principals, environment) = client_and_env(&base_url);
    let pack = journey_pack();
    let corpus = seeded(principals.primary(), &pack);

    let progress = |_message: String| {};
    let report = run_probe(
        principals.primary(),
        &corpus,
        &environment,
        None,
        &ProbeOptions { requests: 4 },
        &progress,
    )
    .unwrap();

    assert_eq!(report.requests_per_probe, 4);
    assert!(!report.maintenance_settled);
    assert!(
        report.attribution.starts_with("unavailable"),
        "{}",
        report.attribution
    );
    let names: Vec<&str> = report.probes.iter().map(|p| p.name.as_str()).collect();
    assert_eq!(
        names,
        vec!["ward_worklist", "adhoc_trend", "stored_dashboard"]
    );
    for probe in &report.probes {
        assert_eq!(probe.failures, 0, "{} failed on a healthy stub", probe.name);
        assert!(probe.statements.is_empty(), "no attribution, no statements");
        assert!(probe.wire_ms.max_ms >= probe.wire_ms.p50_ms);
    }
    assert!(
        report.remark.contains("never a conformance record"),
        "{}",
        report.remark
    );
}

/// A DECLARED container capability the runtime cannot answer for: the probe
/// still runs, and both the settling and the attribution degrade to honest
/// report fields naming why. Fabricating statement costs from an
/// unreachable database is the one thing an attribution channel must never
/// do.
#[test]
fn an_unreachable_container_leaves_the_probes_attribution_honestly_absent() {
    let (base_url, _server, _faults) = spawn_stub();
    let (principals, environment) = client_and_env(&base_url);
    let pack = journey_pack();
    let corpus = seeded(principals.primary(), &pack);

    let containers = Containers {
        sut: "veredictum-no-such-sut-container".to_owned(),
        db: "veredictum-no-such-db-container".to_owned(),
    };
    let notes = std::sync::Mutex::new(Vec::new());
    let report = run_probe(
        principals.primary(),
        &corpus,
        &environment,
        Some(&containers),
        &ProbeOptions { requests: 1 },
        &|message| {
            notes
                .lock()
                .expect("the progress lock is uncontended")
                .push(message);
        },
    )
    .unwrap();

    assert!(!report.maintenance_settled);
    assert!(
        report.attribution.starts_with("unavailable"),
        "{}",
        report.attribution
    );
    assert!(
        report.probes.iter().all(|p| p.statements.is_empty()),
        "an unreachable database produced attributed statements"
    );
    let seen = notes.lock().expect("the progress lock is uncontended");
    assert!(
        seen.iter()
            .any(|m| m.contains("statement attribution unavailable")),
        "{seen:?}"
    );
    assert!(
        seen.iter().any(|m| m.contains("maintenance not settled")),
        "{seen:?}"
    );
}

/// A probe whose SUT is not listening: every request is a recorded finding,
/// never an instrument error. The probe still returns a report, because a
/// failing probe IS the evidence the optimization loop wants.
#[test]
fn a_probe_against_an_unreachable_sut_records_failures_rather_than_erroring() {
    let (base_url, _server, _faults) = spawn_stub();
    let (principals, environment) = client_and_env(&base_url);
    let pack = journey_pack();
    let corpus = seeded(principals.primary(), &pack);

    let dead: Ixit = serde_json::from_value(serde_json::json!({
        "instances": { "sut": { "base_url": crate::fake_sut::closed_port_url(),
                                 "auth": { "mode": "none" } } }
    }))
    .unwrap();
    let dead = PerfPrincipals::from_ixit(&dead).unwrap();

    let progress = |_message: String| {};
    let report = run_probe(
        dead.primary(),
        &corpus,
        &environment,
        None,
        &ProbeOptions { requests: 2 },
        &progress,
    )
    .unwrap();
    for probe in &report.probes {
        assert_eq!(probe.failures, 2, "{} recorded no failures", probe.name);
    }
}
