// SPDX-FileCopyrightText: Veredictum contributors
// SPDX-License-Identifier: Apache-2.0

//! The universal-benchmark engine against the fake SUT: preflight, seed, one
//! short measured phase, and the comparison over two emitted results.
//!
//! Wall-clock cost is kept to a few seconds on purpose. The packs here run a
//! handful of EHRs and a one-second measured span, because what is under test
//! is the shape of the emitted record — the boundary statement, the pins, the
//! warmup split, the error classing, the cross-repetition summary — rather
//! than any throughput number.

use std::path::PathBuf;

use serde_json::{Value, json};
use veredictum::bench::BOUNDARY_STATEMENT;
use veredictum::bench::client::AuthKind;
use veredictum::bench::pack::{BenchOp, BenchPack, BenchPhase, MeasurePhase, SeedPhase, smoke};
use veredictum::bench::result::BenchResult;
use veredictum::pipeline::bench::{BenchRequest, compare_bench, run_bench};
use wiremock::matchers::{method, path, path_regex};
use wiremock::{Mock, ResponseTemplate};

use crate::fake_sut::FakeSut;

/// Anything a construction or an engine run can fail with, so a test body
/// propagates plumbing failures with `?`
/// (<https://doc.rust-lang.org/book/ch11-01-writing-tests.html>).
type Fallible = Result<(), Box<dyn std::error::Error>>;

/// Whether an emitted file is the result document rather than its summary.
fn is_json(name: &str) -> bool {
    std::path::Path::new(name)
        .extension()
        .is_some_and(|extension| extension.eq_ignore_ascii_case("json"))
}

/// A pack the size of a unit test: four EHRs, one composition each, one
/// second of measured arrivals over the whole operation vocabulary.
fn tiny_pack() -> BenchPack {
    let deck = smoke();
    let fixtures = deck.fixtures();
    deck.with_phases(vec![
        BenchPhase::Seed(SeedPhase {
            name: "seed".to_owned(),
            fixtures,
            ehrs: 4,
            compositions_per_ehr: 1,
            workers: 2,
        }),
        BenchPhase::Measure(MeasurePhase {
            name: "mixed".to_owned(),
            rate_per_s: 20.0,
            warmup_s: 1,
            duration_s: 1,
            mix: BenchOp::ALL.iter().map(|op| (*op, 1)).collect(),
        }),
    ])
}

/// Mounts a system that answers every exchange the engine drives.
fn mount_healthy(sut: &FakeSut) {
    sut.mount(
        Mock::given(method("GET"))
            .and(path("/definition/template/adl1.4"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!([]))),
    );
    sut.mount(
        Mock::given(method("POST"))
            .and(path("/definition/template/adl1.4"))
            .respond_with(ResponseTemplate::new(409)),
    );
    sut.mount(Mock::given(method("POST")).and(path("/ehr")).respond_with(
        ResponseTemplate::new(201).insert_header("Location", "http://sut/ehr/EHR-1"),
    ));
    sut.mount(
        Mock::given(method("POST"))
            .and(path_regex(r"^/ehr/[^/]+/composition$"))
            .respond_with(ResponseTemplate::new(201).insert_header("ETag", "\"c-1::sut::1\"")),
    );
    sut.mount(
        Mock::given(method("GET"))
            .and(path_regex(r"^/ehr/[^/]+/composition/[^/]+$"))
            .respond_with(
                ResponseTemplate::new(200).set_body_json(json!({ "_type": "COMPOSITION" })),
            ),
    );
    sut.mount(
        Mock::given(method("GET"))
            .and(path_regex(r"^/ehr/[^/]+/ehr_status$"))
            .respond_with(
                ResponseTemplate::new(200).set_body_json(json!({ "_type": "EHR_STATUS" })),
            ),
    );
    sut.mount(
        Mock::given(method("GET"))
            .and(path_regex(r"^/ehr/[^/]+$"))
            .respond_with(
                ResponseTemplate::new(200).set_body_json(json!({ "ehr_id": { "value": "EHR-1" } })),
            ),
    );
    sut.mount(
        Mock::given(method("POST"))
            .and(path("/query/aql"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({ "rows": [] }))),
    );
}

/// Runs the tiny pack against `sut` and returns the emitted document text.
fn drive(
    sut: &FakeSut,
    label: &str,
    repetitions: u32,
) -> Result<(BenchResult, String), Box<dyn std::error::Error>> {
    let deck = tiny_pack();
    let outcome = run_bench(
        &BenchRequest {
            pack: &deck,
            base_url: &sut.base_url(),
            auth: AuthKind::None,
            user: None,
            repetitions,
            label: Some(label),
        },
        &|_message| {},
    )?;
    let document = outcome
        .documents
        .iter()
        .find(|file| is_json(&file.name))
        .ok_or("the run emitted no result document")?
        .body
        .clone();
    Ok((outcome.result, document))
}

/// The whole path end to end: preflight, seed, one measured repetition, and
/// an emitted record that carries the boundary statement, the fixture pins
/// and a warmup split.
#[test]
#[expect(
    clippy::panic_in_result_fn,
    reason = "the Book's Result-test shape: assertions panic, plumbing propagates with `?`"
)]
fn a_bench_run_emits_a_bounded_record() -> Fallible {
    let sut = FakeSut::start();
    mount_healthy(&sut);
    let (result, document) = drive(&sut, "healthy", 1)?;

    assert_eq!(result.boundary_statement, BOUNDARY_STATEMENT);
    assert!(!result.submittable, "one repetition is not submittable");
    assert_eq!(result.repetitions.len(), 1);
    assert_eq!(result.pack.fixtures.len(), 2);
    for digest in result.pack.fixtures.values() {
        assert_eq!(digest.len(), 64, "a pin is not a sha256: {digest}");
    }
    assert_eq!(result.seed_phases.len(), 1);
    let seed = result.seed_phases.first().ok_or("no seed phase")?;
    assert_eq!(seed.ehrs, 4);
    assert_eq!(seed.regime.as_str(), "closed-loop");
    assert!(seed.bulk_load_writes_per_s > 0.0);

    let repetition = result.repetitions.first().ok_or("no repetition")?;
    let phase = repetition.phases.get("mixed").ok_or("no measured phase")?;
    assert_eq!(phase.regime.as_str(), "open-loop");
    assert!(
        phase.warmup_arrivals > 0,
        "the warmup span recorded nothing"
    );
    assert!(phase.dispatched_measured_arrivals > 0);
    assert!(!phase.operations.is_empty());
    for stats in phase.operations.values() {
        assert_eq!(stats.errors, 0, "a healthy system produced an error");
        assert!(!stats.hdr_v2_base64.is_empty());
        assert!(stats.p99_us >= stats.p50_us);
        let decoded = stats.decode_histogram()?;
        assert_eq!(decoded.value_at_quantile(0.50), stats.p50_us);
    }

    // The emitted document is what a reader gets, so it validates against the
    // published schema rather than only against the in-memory model.
    let value: Value = serde_json::from_str(&document)?;
    let schema = veredictum::schema::bench_result_schema();
    let validator = jsonschema::validator_for(&schema)?;
    let violations: Vec<String> = validator
        .iter_errors(&value)
        .map(|error| format!("{}: {error}", error.instance_path()))
        .collect();
    assert!(violations.is_empty(), "{}", violations.join("; "));
    Ok(())
}

/// A system that answers 500 on the query route has those arrivals counted in
/// their own error class rather than quietly dropped.
#[test]
#[expect(
    clippy::panic_in_result_fn,
    reason = "the Book's Result-test shape: assertions panic, plumbing propagates with `?`"
)]
fn a_failing_operation_is_counted_by_class() -> Fallible {
    let sut = FakeSut::start();
    // Mounted FIRST so it wins over the healthy query stub below: wiremock
    // matches in registration order.
    sut.mount(
        Mock::given(method("POST"))
            .and(path("/query/aql"))
            .respond_with(ResponseTemplate::new(500)),
    );
    mount_healthy(&sut);
    let (result, _document) = drive(&sut, "broken-query", 1)?;

    let repetition = result.repetitions.first().ok_or("no repetition")?;
    let phase = repetition.phases.get("mixed").ok_or("no measured phase")?;
    let query = phase
        .operations
        .get("adhoc_query_uid")
        .ok_or("the query operation was not recorded")?;
    assert!(query.errors > 0, "the 500s were not counted");
    assert_eq!(
        query.errors_by_class.get("http_5xx").copied(),
        Some(query.errors),
        "{:?}",
        query.errors_by_class
    );
    let reads = phase
        .operations
        .get("get_ehr")
        .ok_or("the EHR read was not recorded")?;
    assert_eq!(reads.errors, 0, "a healthy operation was blamed");
    Ok(())
}

/// A preflight that cannot read the template list refuses the run, and no
/// measured document exists.
#[test]
fn a_failed_preflight_refuses_the_run() {
    let sut = FakeSut::start();
    sut.mount(
        Mock::given(method("GET"))
            .and(path("/definition/template/adl1.4"))
            .respond_with(ResponseTemplate::new(401)),
    );
    let deck = tiny_pack();
    let error = run_bench(
        &BenchRequest {
            pack: &deck,
            base_url: &sut.base_url(),
            auth: AuthKind::None,
            user: None,
            repetitions: 1,
            label: None,
        },
        &|_message| {},
    )
    .unwrap_err()
    .to_string();
    assert!(error.contains("preflight refused the run"), "{error}");
    assert!(error.contains("template list"), "{error}");
}

/// Two emitted results align into one comparison, and a differing pack
/// version is stated in the header before any number.
#[test]
#[expect(
    clippy::panic_in_result_fn,
    reason = "the Book's Result-test shape: assertions panic, plumbing propagates with `?`"
)]
fn two_results_compare_with_their_mismatches_named() -> Fallible {
    let sut = FakeSut::start();
    mount_healthy(&sut);
    let directory = assert_fs::TempDir::new()?;
    let mut paths: Vec<PathBuf> = Vec::new();
    for (label, version) in [("left", "1.0.0"), ("right", "2.0.0")] {
        let mut deck = tiny_pack();
        deck.version = version.to_owned();
        let outcome = run_bench(
            &BenchRequest {
                pack: &deck,
                base_url: &sut.base_url(),
                auth: AuthKind::None,
                user: None,
                repetitions: 1,
                label: Some(label),
            },
            &|_message| {},
        )?;
        for file in &outcome.documents {
            let target = directory.join(&file.name);
            std::fs::write(&target, &file.body)?;
            if is_json(&file.name) {
                paths.push(target);
            }
        }
    }
    assert_eq!(paths.len(), 2);

    let comparison = compare_bench(&paths)?;
    assert_eq!(comparison.comparison.columns.len(), 2);
    assert!(!comparison.comparison.rows.is_empty());
    assert!(
        comparison
            .comparison
            .warnings
            .iter()
            .any(|warning| warning.contains("DIFFERENT packs")),
        "{:?}",
        comparison.comparison.warnings
    );
    assert!(
        comparison
            .comparison
            .warnings
            .iter()
            .any(|warning| warning.contains("not submittable")),
        "{:?}",
        comparison.comparison.warnings
    );
    let body = &comparison.document.body;
    assert!(body.contains(BOUNDARY_STATEMENT), "{body}");
    let warning_at = body.find("DIFFERENT packs");
    let table_at = body.find("## Aligned metrics");
    assert!(warning_at < table_at, "{body}");
    Ok(())
}

/// One result file is refused: a comparison needs a second column.
#[test]
fn a_single_result_is_not_a_comparison() {
    let error = compare_bench(&[PathBuf::from("only.json")])
        .unwrap_err()
        .to_string();
    assert!(error.contains("at least two result files"), "{error}");
}
