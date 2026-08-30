// SPDX-FileCopyrightText: Veredictum contributors
// SPDX-License-Identifier: Apache-2.0

//! The measured instruments against the fake SUT: the AQL probe's answer
//! classification, the stress ladder's step arithmetic, and the pipeline
//! preamble all three share.
//!
//! The probe and the ladder are exploration evidence, never a conformance
//! record, so what these tests pin is that each one reports honestly: a
//! failing request is a recorded finding rather than an instrument error, an
//! unavailable attribution channel says so in the artifact instead of
//! fabricating rows, and a step that leaves the envelope is named as
//! breached. The ladders here are deliberately tiny — one-second holds, two
//! rungs — because the arithmetic is what is under test, not throughput.
//!
//! The pipeline seams are driven to the point where the instrument would
//! contact the population it measures. Everything up to and including the
//! seeding's first wire call is drivable against a stub; the sustained window
//! past it is not, because the smallest rung of the scale ladder is ten
//! thousand seeded EHRs and a stub answering them would measure nothing.

#![expect(
    clippy::unwrap_used,
    reason = "test-support helpers (not `#[test]` fns, so the clippy.toml in-tests scoping does not reach them) are panic-idiomatic: a broken harness must abort the test loudly, Book ch11"
)]

use std::path::{Path, PathBuf};

use serde_json::json;
use veredictum::ixit::{Environment, Ixit};
use veredictum::perf::{ArrivalCurve, JourneyCatalogue, Percent};
use veredictum::perf_run::client::{PerfClient, PerfPrincipals};
use veredictum::perf_run::corpus::SeededCorpus;
use veredictum::perf_run::pack::{AuxPayloads, JourneyPack};
use veredictum::perf_run::schedule::JourneyWorkload;
use veredictum::pipeline::measured::{
    MeasuredRequest, ProbeRequest, StressRequest, SustainedWindow, run_aql_probe, run_measured,
    seed_corpus,
};
use veredictum::pipeline::{Error, measured};
use veredictum::probe::{ProbeOptions, run_probe};
use veredictum::stress::{StressOptions, run_stress};
use wiremock::matchers::{method, path};
use wiremock::{Mock, ResponseTemplate};

use crate::fake_sut::FakeSut;

/// Anything a construction or an instrument run can fail with, so a test
/// body propagates plumbing failures with `?`
/// (<https://doc.rust-lang.org/book/ch11-01-writing-tests.html>).
type Fallible = Result<(), Box<dyn std::error::Error>>;

fn topology(base_url: &str) -> Ixit {
    serde_json::from_value(json!({
        "instances": { "sut": { "base_url": base_url, "auth": { "mode": "none" } } },
        "environment": {
            "exclusive_server": true, "hardware_class": "test-stub",
            "cores": 1, "memory_gb": 1, "storage_class": "ram",
            "topology": "wiremock fake SUT"
        }
    }))
    .unwrap()
}

fn environment(ixit: &Ixit) -> Environment {
    ixit.environment.clone().unwrap()
}

/// A one-patient corpus index: enough for the probe's `ehr_id` parameter and
/// for one ward-addressed journey stage.
fn corpus() -> SeededCorpus {
    serde_json::from_value(json!({
        "corpus": "cnf.scale.10k",
        "ehr_ids": ["EHR-1"],
        "compositions": [[0, "c-1::sys::1"]],
        "ward": [{
            "ehr_index": 0,
            "gp_ovid": "c-1::sys::1",
            "medlist_ovid": "m-1::sys::1",
            "directory_ovid": "d-1::sys::1",
            "contribution_uid": "contrib-1"
        }]
    }))
    .unwrap()
}

/// Every probe answers 200, so the report carries no failure and the
/// percentiles are ordered. The attribution field states the honest reason
/// the DB channel is absent rather than leaving the reader to guess.
#[test]
#[expect(
    clippy::panic_in_result_fn,
    reason = "the Book's Result-test shape: assertions panic, plumbing propagates with `?`"
)]
fn the_probe_reports_percentiles_and_declares_its_missing_attribution() -> Fallible {
    let sut = FakeSut::start();
    sut.mount(
        Mock::given(method("POST"))
            .and(path("/query/aql"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({ "rows": [] }))),
    );
    sut.mount(
        Mock::given(method("GET"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({ "rows": [] }))),
    );

    let ixit = topology(&sut.base_url());
    let client = PerfClient::from_instance(ixit.default_instance()?, &ixit)?;
    let report = run_probe(
        &client,
        &corpus(),
        &environment(&ixit),
        None,
        &ProbeOptions { requests: 3 },
        &|_message| {},
    )?;

    assert_eq!(report.requests_per_probe, 3);
    assert_eq!(
        report.probes.len(),
        3,
        "the closed probe set is ward, ad-hoc and stored"
    );
    let names: Vec<&str> = report.probes.iter().map(|p| p.name.as_str()).collect();
    assert_eq!(names, ["ward_worklist", "adhoc_trend", "stored_dashboard"]);
    for probe in &report.probes {
        assert_eq!(probe.failures, 0, "probe {} saw a failure", probe.name);
        assert!(
            probe.wire_ms.min_ms <= probe.wire_ms.p50_ms
                && probe.wire_ms.p50_ms <= probe.wire_ms.p95_ms
                && probe.wire_ms.p95_ms <= probe.wire_ms.max_ms,
            "probe {} percentiles are out of order: {:?}",
            probe.name,
            probe.wire_ms
        );
        assert!(
            probe.statements.is_empty(),
            "no attribution channel exists, so no statement may be reported"
        );
    }
    assert!(!report.maintenance_settled);
    assert!(
        report.attribution.starts_with("unavailable"),
        "attribution reads {:?}",
        report.attribution
    );
    assert!(report.remark.contains("never a conformance record"));

    // Three probes x three requests, all of them actually sent.
    assert_eq!(sut.requests().len(), 9);
    Ok(())
}

/// A probe the SUT refuses is a RECORDED FINDING, not an instrument error:
/// the run completes, the failure count names the bad probe, and the other
/// probes are unaffected.
#[test]
#[expect(
    clippy::panic_in_result_fn,
    reason = "the Book's Result-test shape: assertions panic, plumbing propagates with `?`"
)]
fn a_refused_probe_is_counted_not_raised() -> Fallible {
    let sut = FakeSut::start();
    sut.mount(
        Mock::given(method("POST"))
            .and(path("/query/aql"))
            .respond_with(ResponseTemplate::new(500)),
    );
    sut.mount(
        Mock::given(method("GET"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({ "rows": [] }))),
    );

    let ixit = topology(&sut.base_url());
    let client = PerfClient::from_instance(ixit.default_instance()?, &ixit)?;
    let report = run_probe(
        &client,
        &corpus(),
        &environment(&ixit),
        None,
        &ProbeOptions { requests: 2 },
        &|_message| {},
    )?;

    let failures: Vec<(&str, u32)> = report
        .probes
        .iter()
        .map(|p| (p.name.as_str(), p.failures))
        .collect();
    assert_eq!(
        failures,
        [
            ("ward_worklist", 2),
            ("adhoc_trend", 2),
            ("stored_dashboard", 0)
        ]
    );
    Ok(())
}

/// An empty corpus index has no EHR to parameterize the probes with, and
/// that is an instrument-construction error rather than a fabricated run.
#[test]
#[expect(
    clippy::panic_in_result_fn,
    reason = "the Book's Result-test shape: assertions panic, plumbing propagates with `?`"
)]
fn a_probe_over_an_empty_corpus_refuses_to_run() -> Fallible {
    let sut = FakeSut::start();
    let ixit = topology(&sut.base_url());
    let client = PerfClient::from_instance(ixit.default_instance()?, &ixit)?;
    let empty: SeededCorpus = serde_json::from_value(json!({
        "corpus": "cnf.scale.10k", "ehr_ids": [], "compositions": [], "ward": []
    }))?;
    let outcome = run_probe(
        &client,
        &empty,
        &environment(&ixit),
        None,
        &ProbeOptions::default(),
        &|_message| {},
    );
    assert_eq!(outcome.err(), Some("corpus index has no EHRs".to_owned()));
    assert_eq!(sut.requests().len(), 0);
    Ok(())
}

/// The smallest workload the stress ladder can climb: one read journey and
/// one write journey, mixed 95:5 so the expansion lands inside the
/// derivation band the schedule builder holds a workload to. The tests
/// measure the ladder's arithmetic, not a SUT.
fn read_write_catalogue() -> JourneyCatalogue {
    serde_saphyr::from_str(
        "chart_review:\n  description: d\n  derivation: g\n  stages:\n    - { op: composition_read, at: PT0S }\nadmission:\n  description: d\n  derivation: g\n  stages:\n    - { op: ehr_create, at: PT0S }\n",
    )
    .unwrap()
}

/// The 95:5 read/write mix the catalogue above is expanded with.
fn read_write_shares() -> [(String, Percent); 2] {
    [
        ("chart_review".to_owned(), Percent(95.0)),
        ("admission".to_owned(), Percent(5.0)),
    ]
}

fn empty_pack() -> JourneyPack {
    JourneyPack {
        templates: Vec::new(),
        aux: AuxPayloads::default(),
    }
}

/// The two answers the mix needs: a composition read and an EHR creation
/// whose `Location` names the new resource (ITS-REST
/// `Requests_and_responses.md` §Location).
fn mount_healthy(sut: &FakeSut) {
    sut.mount(
        Mock::given(method("GET")).respond_with(
            ResponseTemplate::new(200).set_body_json(json!({ "_type": "COMPOSITION" })),
        ),
    );
    sut.mount(Mock::given(method("POST")).and(path("/ehr")).respond_with(
        ResponseTemplate::new(201).insert_header("Location", "http://sut/v1/ehr/EHR-new"),
    ));
}

/// A healthy SUT holds the envelope, so the climb doubles the rate until it
/// passes the cap and the report says `ladder_capped`: the ladder found no
/// breach, which is a different fact from finding a knee.
#[test]
#[expect(
    clippy::panic_in_result_fn,
    reason = "the Book's Result-test shape: assertions panic, plumbing propagates with `?`"
)]
fn a_capped_climb_reports_the_last_stable_rate_and_no_knee() -> Fallible {
    let sut = FakeSut::start();
    mount_healthy(&sut);

    let ixit = topology(&sut.base_url());
    let principals = PerfPrincipals::from_ixit(&ixit)?;
    let catalogue = read_write_catalogue();
    let pack = empty_pack();
    let shares = read_write_shares();
    let workload = JourneyWorkload {
        catalogue: &catalogue,
        shares: &shares,
        pack: &pack,
        curve: ArrivalCurve::Uniform,
        principals: &principals,
    };
    let report = run_stress(
        &corpus(),
        &workload,
        &environment(&ixit),
        None,
        &StressOptions {
            start_rate: 4.0,
            max_rate: 4.0,
            step_warmup_s: 0,
            step_hold_s: 1,
            bisections: 0,
            ..StressOptions::default()
        },
        &|_message| {},
    )?;

    assert_eq!(report.steps.len(), 1, "one rung, then the cap");
    let step = report.steps.first().ok_or("the ladder ran no step")?;
    assert!(step.stable, "breaches: {:?}", step.breaches);
    assert!(step.resources.is_none(), "no containers block was declared");
    assert!(report.ladder_capped);
    assert!((report.max_sustainable_throughput_per_s - 4.0).abs() < f64::EPSILON);
    assert!(report.remark.contains("Exploration only"));
    Ok(())
}

/// A budget the SUT cannot hold breaches the first rung, so the ladder stops
/// climbing, bisects between the last stable rate (zero) and the breached
/// one, and reports a maximum sustainable throughput of zero rather than the
/// rate it tried.
#[test]
#[expect(
    clippy::panic_in_result_fn,
    reason = "the Book's Result-test shape: assertions panic, plumbing propagates with `?`"
)]
fn a_breached_envelope_stops_the_climb_and_bisects() -> Fallible {
    let sut = FakeSut::start();
    mount_healthy(&sut);

    let ixit = topology(&sut.base_url());
    let principals = PerfPrincipals::from_ixit(&ixit)?;
    let catalogue = read_write_catalogue();
    let pack = empty_pack();
    let shares = read_write_shares();
    let workload = JourneyWorkload {
        catalogue: &catalogue,
        shares: &shares,
        pack: &pack,
        curve: ArrivalCurve::Uniform,
        principals: &principals,
    };
    let report = run_stress(
        &corpus(),
        &workload,
        &environment(&ixit),
        None,
        &StressOptions {
            start_rate: 4.0,
            max_rate: 64.0,
            step_warmup_s: 0,
            step_hold_s: 1,
            bisections: 1,
            // Zero budget: every rung breaches on p99, whatever the SUT does.
            p99_budget_ms: 0.0,
            ..StressOptions::default()
        },
        &|_message| {},
    )?;

    assert_eq!(
        report.steps.len(),
        2,
        "the breached rung, then one bisection"
    );
    for step in &report.steps {
        assert!(!step.stable, "rung {} held a zero budget", step.rate);
        assert!(
            step.breaches.iter().any(|b| b.contains("p99")),
            "rung {} breached for another reason: {:?}",
            step.rate,
            step.breaches
        );
    }
    let rates: Vec<f64> = report.steps.iter().map(|s| s.rate).collect();
    assert_eq!(rates, vec![4.0, 2.0], "the bisection halves toward zero");
    assert!(!report.ladder_capped);
    assert!(report.max_sustainable_throughput_per_s.abs() < f64::EPSILON);
    Ok(())
}

/// A refused SUT is an ERROR ARRIVAL, not an instrument failure: the rung
/// still completes, the error rate breaches the tolerance, and the report
/// names it — the exploration equivalent of the attribution law's
/// inconclusive row.
#[test]
#[expect(
    clippy::panic_in_result_fn,
    reason = "the Book's Result-test shape: assertions panic, plumbing propagates with `?`"
)]
fn a_refusing_sut_breaches_the_error_tolerance() -> Fallible {
    let sut = FakeSut::start();
    sut.mount(Mock::given(method("GET")).respond_with(ResponseTemplate::new(500)));
    sut.mount(Mock::given(method("POST")).respond_with(ResponseTemplate::new(500)));

    let ixit = topology(&sut.base_url());
    let principals = PerfPrincipals::from_ixit(&ixit)?;
    let catalogue = read_write_catalogue();
    let pack = empty_pack();
    let shares = read_write_shares();
    let workload = JourneyWorkload {
        catalogue: &catalogue,
        shares: &shares,
        pack: &pack,
        curve: ArrivalCurve::Uniform,
        principals: &principals,
    };
    let report = run_stress(
        &corpus(),
        &workload,
        &environment(&ixit),
        None,
        &StressOptions {
            start_rate: 4.0,
            max_rate: 4.0,
            step_warmup_s: 0,
            step_hold_s: 1,
            bisections: 0,
            ..StressOptions::default()
        },
        &|_message| {},
    )?;

    let step = report.steps.first().ok_or("the ladder ran no step")?;
    assert!(!step.stable);
    assert!(
        step.breaches.iter().any(|b| b.contains("error rate")),
        "breaches {:?} name no error-rate finding",
        step.breaches
    );
    let errors: u64 = step.operations.iter().map(|op| op.errors).sum();
    assert!(errors > 0, "a refusing SUT produced no error arrival");
    Ok(())
}

/// The committed artifact root, addressed from this package rather than from
/// whatever directory the runner was started in.
fn artifact_root() -> &'static Path {
    Path::new(concat!(env!("CARGO_MANIFEST_DIR"), "/../../artifacts"))
}

/// Writes one ixit document into `dir` and returns its path.
fn staged_ixit(dir: &assert_fs::TempDir, document: &serde_json::Value) -> PathBuf {
    let path = dir.path().join("ixit.json");
    std::fs::write(&path, document.to_string()).unwrap();
    path
}

/// The topology every pipeline instrument needs: one instance addressing the
/// fake SUT, plus the environment block a measured run makes mandatory
/// (a throughput number with no deployment described is unreadable).
fn staged_topology(dir: &assert_fs::TempDir, base_url: &str) -> PathBuf {
    staged_ixit(
        dir,
        &json!({
            "instances": { "sut": { "base_url": base_url, "auth": { "mode": "none" } } },
            "environment": {
                "exclusive_server": true, "hardware_class": "test-stub",
                "cores": 1, "memory_gb": 1, "storage_class": "ram",
                "topology": "wiremock fake SUT"
            }
        }),
    )
}

/// A SUT that refuses the OPT upload, which is the FIRST wire call every
/// seeding makes: the scale ladder's constraint carrier goes up before a
/// single EHR is created, so a refusal there stops the instrument before it
/// allocates the population.
fn mount_refusing_seed(sut: &FakeSut) {
    sut.mount(
        Mock::given(method("POST"))
            .and(path("/definition/template/adl1.4"))
            .respond_with(ResponseTemplate::new(500)),
    );
}

/// Seeding is judged by what the SUT answered, and a refused OPT upload is
/// named as the stage that failed rather than swallowed into a population
/// nothing carries.
#[test]
#[expect(
    clippy::panic_in_result_fn,
    reason = "the Book's Result-test shape: assertions panic, plumbing propagates with `?`"
)]
fn a_refused_opt_upload_fails_the_seeding_by_name() -> Fallible {
    let sut = FakeSut::start();
    mount_refusing_seed(&sut);
    let ixit: Ixit = serde_json::from_value(json!({
        "instances": { "sut": { "base_url": sut.base_url(), "auth": { "mode": "none" } } }
    }))?;
    let client = PerfClient::from_instance(ixit.default_instance()?, &ixit)?;
    let stages = std::sync::Mutex::new(Vec::new());
    let error = seed_corpus(
        &client,
        "cnf.scale.10k",
        "<template/>",
        &empty_pack(),
        1,
        &|_| {},
        &mut |stage| stages.lock().unwrap().push(format!("{stage:?}")),
    )
    .expect_err("a refused OPT upload establishes no corpus");
    assert!(
        matches!(&error, Error::Instrument(m) if m.contains("seeding failed")),
        "{error}"
    );
    assert_eq!(
        *stages.lock().unwrap(),
        vec!["BeforeScale".to_owned()],
        "the empty-baseline milestone is observed before the first write, and no later one is"
    );
    Ok(())
}

/// Every instrument reads its catalogue, its topology and its class case
/// BEFORE it contacts anything, and each preamble failure names the artifact
/// it wanted — a half-built root must never look like an empty catalogue.
#[test]
#[expect(
    clippy::panic_in_result_fn,
    reason = "the Book's Result-test shape: assertions panic, plumbing propagates with `?`"
)]
fn each_instrument_names_the_artifact_its_preamble_could_not_read() -> Fallible {
    let dir = assert_fs::TempDir::new()?;
    let good_ixit = staged_topology(&dir, "http://127.0.0.1:1");
    let empty_root = dir.path().join("no-such-root");
    let missing_ixit = dir.path().join("no-such-ixit.json");
    // A root whose performance case does not load: reported one diagnostic
    // per file, never as a tree that simply carries no case.
    let defective_root = dir.path().join("defective");
    std::fs::create_dir_all(defective_root.join("schedule/performance"))?;
    std::fs::write(
        defective_root.join("schedule/performance/PERF-broken.yaml"),
        "id: [not a case id\n",
    )?;
    // A topology with no `environment` block: mandatory for every measured
    // instrument, because a number without the deployment described is not a
    // measurement of anything.
    let envless = dir.path().join("envless.json");
    std::fs::write(
        &envless,
        json!({ "instances": { "sut": { "base_url": "http://127.0.0.1:1", "auth": { "mode": "none" } } } })
            .to_string(),
    )?;

    for (root, ixit, expected) in [
        (empty_root.as_path(), good_ixit.as_path(), "class"),
        (defective_root.as_path(), good_ixit.as_path(), "artifacts"),
        (empty_root.as_path(), missing_ixit.as_path(), "read"),
        (empty_root.as_path(), envless.as_path(), "environment"),
    ] {
        let measured = run_measured(
            &MeasuredRequest {
                root,
                ixit,
                results: &dir.path().join("results.json"),
                class: "POC",
                seed_workers: 1,
                window: SustainedWindow::default(),
            },
            &|_| {},
        )
        .expect_err("the preamble must refuse before any wire call");
        let stress = measured::run_stress(
            &StressRequest {
                root,
                ixit,
                corpus_class: "POC",
                seed_workers: 1,
                step_secs: 10,
                bisections: 0,
                max_rate: 8.0,
            },
            &|_| {},
        )
        .expect_err("the preamble must refuse before any wire call");
        let probe = run_aql_probe(
            &ProbeRequest {
                root,
                ixit,
                corpus_class: "POC",
                seed_workers: 1,
                requests: 1,
            },
            &|_| {},
        )
        .expect_err("the preamble must refuse before any wire call");
        for error in [measured, stress, probe] {
            let matched = match expected {
                "class" => matches!(&error, Error::Missing(m) if m.contains("class POC")),
                "artifacts" => matches!(error, Error::Artifacts(_)),
                "read" => matches!(error, Error::Read { .. }),
                _ => matches!(&error, Error::Instrument(m) if m.contains("environment")),
            };
            assert!(matched, "{expected}: {error}");
        }
    }
    Ok(())
}

/// The three instruments reach the SAME seeding over the committed
/// catalogue's own POC case, and a SUT that refuses it stops each of them
/// with the seeding named: no report, no probe record, and no measurement
/// merged into the results document.
#[test]
#[expect(
    clippy::panic_in_result_fn,
    reason = "the Book's Result-test shape: assertions panic, plumbing propagates with `?`"
)]
fn a_refused_seeding_stops_every_instrument_before_it_records_anything() -> Fallible {
    let sut = FakeSut::start();
    mount_refusing_seed(&sut);
    let dir = assert_fs::TempDir::new()?;
    let ixit = staged_topology(&dir, &sut.base_url());
    let results = dir.path().join("results.json");

    let measured = run_measured(
        &MeasuredRequest {
            root: artifact_root(),
            ixit: &ixit,
            results: &results,
            class: "POC",
            seed_workers: 1,
            window: SustainedWindow::default(),
        },
        &|_| {},
    )
    .expect_err("a refused seeding is no measured window");
    assert!(
        matches!(&measured, Error::Instrument(m) if m.contains("seeding failed")),
        "{measured}"
    );
    assert!(
        !results.exists(),
        "a refused seeding must merge nothing into the results record"
    );

    let stress = measured::run_stress(
        &StressRequest {
            root: artifact_root(),
            ixit: &ixit,
            corpus_class: "POC",
            seed_workers: 1,
            step_secs: 10,
            bisections: 0,
            max_rate: 8.0,
        },
        &|_| {},
    )
    .expect_err("a refused seeding is no stress ladder");
    assert!(
        matches!(&stress, Error::Instrument(m) if m.contains("seeding failed")),
        "{stress}"
    );

    let probe = run_aql_probe(
        &ProbeRequest {
            root: artifact_root(),
            ixit: &ixit,
            corpus_class: "POC",
            seed_workers: 1,
            requests: 1,
        },
        &|_| {},
    )
    .expect_err("a refused seeding is no probe");
    assert!(
        matches!(&probe, Error::Instrument(m) if m.contains("seeding failed")),
        "{probe}"
    );
    Ok(())
}

/// A measured run whose ixit declares no `containers` block SAYS so and
/// carries on: resource sampling is optional by capability, so its absence is
/// a stated boundary of the record rather than a failed run.
#[test]
#[expect(
    clippy::panic_in_result_fn,
    reason = "the Book's Result-test shape: assertions panic, plumbing propagates with `?`"
)]
fn a_measured_run_states_that_it_sampled_no_resources() -> Fallible {
    let sut = FakeSut::start();
    mount_refusing_seed(&sut);
    let dir = assert_fs::TempDir::new()?;
    let ixit = staged_topology(&dir, &sut.base_url());

    let seen = std::sync::Mutex::new(Vec::new());
    let error = run_measured(
        &MeasuredRequest {
            root: artifact_root(),
            ixit: &ixit,
            results: &dir.path().join("results.json"),
            class: "POC",
            seed_workers: 1,
            window: SustainedWindow::default(),
        },
        &|event| seen.lock().unwrap().push(format!("{event:?}")),
    )
    .expect_err("a refused seeding is no measured window");
    assert!(matches!(&error, Error::Instrument(_)), "{error}");

    let observed = seen.lock().unwrap();
    assert!(
        observed
            .iter()
            .any(|line| line.contains("resources: not sampled")
                && line.contains("`containers` block")),
        "the run never stated its unsampled resources: {observed:?}"
    );
    assert!(
        observed.iter().any(|line| line.starts_with("CaseStarted")),
        "the selected case was never announced: {observed:?}"
    );
    Ok(())
}
