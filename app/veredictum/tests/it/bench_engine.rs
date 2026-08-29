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
use veredictum::bench::baselines::{DockerCli, ReferenceCdr, pinned_resources};
use veredictum::bench::client::AuthKind;
use veredictum::bench::pack::{
    BenchOp, BenchPack, BenchPhase, MeasurePhase, SeedPhase, community_vitals, smoke,
};
use veredictum::bench::relative::{GapReason, RELATIVE_DERIVATION};
use veredictum::bench::result::{BaselineRecord, BenchResult, SubmissionRequirement};
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

/// Mounts every composition read in the operation vocabulary, most specific
/// first, because wiremock matches in registration order.
fn mount_composition_reads(sut: &FakeSut) {
    for (pattern, body) in [
        (
            r"^/ehr/[^/]+/versioned_composition/[^/]+/version/[^/]+$",
            json!({ "_type": "ORIGINAL_VERSION" }),
        ),
        (
            r"^/ehr/[^/]+/versioned_composition/[^/]+/version$",
            json!({ "_type": "ORIGINAL_VERSION" }),
        ),
        (
            r"^/ehr/[^/]+/versioned_composition/[^/]+/revision_history$",
            json!({ "_type": "REVISION_HISTORY" }),
        ),
        (
            r"^/ehr/[^/]+/versioned_composition/[^/]+$",
            json!({ "_type": "VERSIONED_COMPOSITION" }),
        ),
        (
            r"^/ehr/[^/]+/composition/[^/]+$",
            json!({ "_type": "COMPOSITION" }),
        ),
    ] {
        sut.mount(
            Mock::given(method("GET"))
                .and(path_regex(pattern))
                .respond_with(ResponseTemplate::new(200).set_body_json(body)),
        );
    }
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
    mount_composition_reads(sut);
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

/// Runs `deck` against `sut` and returns the record with its two documents,
/// the result JSON first and the rendered Markdown summary second.
fn drive_pack(
    sut: &FakeSut,
    deck: &BenchPack,
    label: &str,
    repetitions: u32,
    scale: f64,
) -> Result<(BenchResult, String, String), Box<dyn std::error::Error>> {
    let outcome = run_bench(
        &BenchRequest {
            pack: deck,
            base_url: &sut.base_url(),
            auth: AuthKind::None,
            user: None,
            repetitions,
            label: Some(label),
            scale,
            seed_workers: None,
            with_baselines: false,
            docker: None,
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
    let summary = outcome
        .documents
        .iter()
        .find(|file| !is_json(&file.name))
        .ok_or("the run emitted no rendered summary")?
        .body
        .clone();
    Ok((outcome.result, document, summary))
}

/// Runs the tiny smoke-derived pack against `sut`.
fn drive(
    sut: &FakeSut,
    label: &str,
    repetitions: u32,
) -> Result<(BenchResult, String), Box<dyn std::error::Error>> {
    let (result, document, _summary) = drive_pack(sut, &tiny_pack(), label, repetitions, 1.0)?;
    Ok((result, document))
}

/// Validates one emitted document against the published bench-result schema.
fn schema_violations(document: &str) -> Result<Vec<String>, Box<dyn std::error::Error>> {
    let value: Value = serde_json::from_str(document)?;
    let schema = veredictum::schema::bench_result_schema();
    let validator = jsonschema::validator_for(&schema)?;
    Ok(validator
        .iter_errors(&value)
        .map(|error| format!("{}: {error}", error.instance_path()))
        .collect())
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
    let violations = schema_violations(&document)?;
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
            scale: 1.0,
            seed_workers: None,
            with_baselines: false,
            docker: None,
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
                scale: 1.0,
                seed_workers: None,
                with_baselines: false,
                docker: None,
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

/// The community pack at a unit-test population: two EHRs, three commits each,
/// the same seven-variant walk, and a one-second open-loop window.
fn tiny_community_pack() -> Result<BenchPack, Box<dyn std::error::Error>> {
    let deck = community_vitals();
    let fixtures = deck.fixtures();
    let walk = deck
        .sweep_phases()
        .first()
        .copied()
        .cloned()
        .ok_or("the community pack lost its walk")?;
    let open = deck
        .measure_phases()
        .first()
        .copied()
        .cloned()
        .ok_or("the community pack lost its open-loop phase")?;
    Ok(deck.with_phases(vec![
        BenchPhase::Seed(SeedPhase {
            name: "write".to_owned(),
            fixtures,
            ehrs: 2,
            compositions_per_ehr: 3,
            workers: 1,
        }),
        BenchPhase::Sweep(walk),
        BenchPhase::Measure(MeasurePhase {
            rate_per_s: 20.0,
            warmup_s: 1,
            duration_s: 1,
            ..open
        }),
    ]))
}

/// Mounts every route the community pack drives, most specific first, because
/// wiremock matches in registration order.
fn mount_community(sut: &FakeSut) {
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
    sut.mount(
        Mock::given(method("POST"))
            .and(path("/ehr"))
            .respond_with(ResponseTemplate::new(201).set_body_json(json!({ "uid": "EHR-1" }))),
    );
    sut.mount(
        Mock::given(method("POST"))
            .and(path_regex(r"^/ehr/[^/]+/composition$"))
            .respond_with(
                ResponseTemplate::new(201).set_body_json(json!({ "uid": "c-1::sut::1" })),
            ),
    );
    mount_composition_reads(sut);
}

/// The reproduction end to end: the record carries the closed-loop write
/// average, the closed-loop walk average over all seven variants, and the
/// open-loop percentiles, each labelled with the regime that produced it.
#[test]
#[expect(
    clippy::panic_in_result_fn,
    reason = "the Book's Result-test shape: assertions panic, plumbing propagates with `?`"
)]
fn the_community_pack_records_both_disciplines_with_their_labels() -> Fallible {
    let sut = FakeSut::start();
    mount_community(&sut);
    let (result, document, summary) =
        drive_pack(&sut, &tiny_community_pack()?, "community", 1, 1.0)?;

    assert_eq!(result.pack.id, "community-vitals");
    assert_eq!(result.pack.fixtures.len(), 2);
    assert_eq!(
        result
            .pack
            .fixtures
            .get("vital_signs_composition.json")
            .map(String::as_str),
        Some("468081c259c737d35d7f80403562b3f333e479d267286faf80fd7c087eaba947")
    );

    // The run matched the pack's pinned configuration, so it is offered as
    // comparable with the reference figures.
    assert!(result.scale.reference_configuration);
    assert!(result.version_at_time.is_some());

    let write = result.seed_phases.first().ok_or("no write phase")?;
    assert_eq!(write.regime.as_str(), "closed-loop");
    assert_eq!(write.ehrs, 2);
    assert_eq!(write.compositions_per_ehr, 3);
    assert_eq!(write.workers, 1);
    assert!(
        write.whole_loop_ms_per_composition > 0.0,
        "the write phase reported no whole-loop average"
    );

    let repetition = result.repetitions.first().ok_or("no repetition")?;
    let walk = repetition.sweeps.get("read_walk").ok_or("no sweep")?;
    assert_eq!(walk.regime.as_str(), "closed-loop");
    assert_eq!(walk.compositions, 6);
    assert_eq!(walk.requests_per_composition, 7);
    assert_eq!(walk.requests, 42);
    assert!(
        walk.whole_loop_us_per_request > 0.0,
        "the walk reported no whole-loop average"
    );
    assert_eq!(walk.operations.len(), 7, "{:?}", walk.operations.keys());
    for (operation, stats) in &walk.operations {
        assert_eq!(stats.errors, 0, "{operation} failed on a healthy system");
        assert_eq!(
            stats.count, 6,
            "{operation} did not visit every composition"
        );
    }

    let open = repetition
        .phases
        .get("read_open_loop")
        .ok_or("no open-loop phase")?;
    assert_eq!(open.regime.as_str(), "open-loop");
    assert!(open.dispatched_measured_arrivals > 0);
    for (operation, stats) in &open.operations {
        assert_eq!(stats.errors, 0, "{operation} failed on a healthy system");
    }

    // The cross summary keeps each phase's discipline beside its numbers, and
    // the rendered view prints both labels.
    assert_eq!(
        result
            .cross
            .get("read_walk")
            .map(|phase| phase.regime.as_str()),
        Some("closed-loop")
    );
    assert_eq!(
        result
            .cross
            .get("read_open_loop")
            .map(|phase| phase.regime.as_str()),
        Some("open-loop")
    );
    assert!(
        summary.contains("## Phase `read_walk` (closed-loop)"),
        "{summary}"
    );
    assert!(
        summary.contains("## Phase `read_open_loop` (open-loop)"),
        "{summary}"
    );
    assert!(summary.contains("us/request whole-loop"), "{summary}");
    assert!(summary.contains("ms/composition whole-loop"), "{summary}");

    let violations = schema_violations(&document)?;
    assert!(violations.is_empty(), "{}", violations.join("; "));
    Ok(())
}

/// A scaled run seeds a smaller population and says in the record that its
/// numbers are off the pack's reference configuration.
#[test]
#[expect(
    clippy::panic_in_result_fn,
    reason = "the Book's Result-test shape: assertions panic, plumbing propagates with `?`"
)]
fn a_scaled_community_run_is_marked_off_the_reference_configuration() -> Fallible {
    let sut = FakeSut::start();
    mount_community(&sut);
    let (result, document, summary) = drive_pack(&sut, &tiny_community_pack()?, "scaled", 1, 0.5)?;

    assert!((result.scale.factor - 0.5).abs() < f64::EPSILON);
    assert!(result.scale.declared_workers);
    assert!(!result.scale.reference_configuration);
    let write = result.seed_phases.first().ok_or("no write phase")?;
    assert_eq!(write.ehrs, 1, "the scale factor did not shrink the seed");
    let repetition = result.repetitions.first().ok_or("no repetition")?;
    let walk = repetition.sweeps.get("read_walk").ok_or("no sweep")?;
    assert_eq!(walk.compositions, 3);
    assert_eq!(walk.requests, 21);
    assert!(
        summary.contains("not comparable with the reference figures"),
        "{summary}"
    );

    let violations = schema_violations(&document)?;
    assert!(violations.is_empty(), "{}", violations.join("; "));
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
/// A baseline block whose measured half is the target's own record, so the
/// derivation is exercised without composing anything.
///
/// Faking the baseline half here is deliberate: the compose orchestration
/// needs a container runtime, and a suite that needs one is a suite that
/// stops running. The arguments the orchestration assembles are asserted in
/// the engine's own unit tests instead.
fn baseline_from(result: &BenchResult, cdr: ReferenceCdr, scale_medians_by: f64) -> BaselineRecord {
    let mut cross = result.cross.clone();
    for phase in cross.values_mut() {
        for operation in phase.operations.values_mut() {
            for stat in [
                &mut operation.p50_us,
                &mut operation.p75_us,
                &mut operation.p90_us,
                &mut operation.p99_us,
                &mut operation.p999_us,
                &mut operation.throughput_ops_s,
            ] {
                stat.median *= scale_medians_by;
            }
        }
    }
    let pin = cdr.pin();
    BaselineRecord {
        cdr: cdr.as_str().to_owned(),
        display_name: cdr.display_name().to_owned(),
        images: pin.images(),
        recipe: pin.recipe(),
        resources: pinned_resources(),
        base_url: pin.base_url(),
        sut_version: None,
        started_at: result.started_at.clone(),
        finished_at: result.finished_at.clone(),
        seed_phases: result.seed_phases.clone(),
        repetitions: result.repetitions.clone(),
        cross,
    }
}

/// The whole derivation and rendering path over injected baselines: the
/// record carries both blocks, the index is the quotient of the two medians,
/// the document still validates, and the summary names the machine.
#[test]
#[expect(
    clippy::panic_in_result_fn,
    reason = "the Book's Result-test shape: assertions panic, plumbing propagates with `?`"
)]
fn injected_baselines_derive_the_relative_index_and_render_it() -> Fallible {
    let sut = FakeSut::start();
    mount_healthy(&sut);
    let (mut result, _document, _summary) = drive_pack(&sut, &tiny_pack(), "target", 3, 1.0)?;

    assert!(!result.submittable, "a baseline-free record is submittable");
    assert_eq!(
        result.submittable_unmet,
        vec![SubmissionRequirement::Baseline]
    );

    result.attach_baselines(vec![
        baseline_from(&result, ReferenceCdr::EhrBase, 2.0),
        baseline_from(&result, ReferenceCdr::FerroEhr, 0.5),
    ]);

    assert!(result.submittable, "{:?}", result.submittable_unmet);
    assert!(result.submittable_unmet.is_empty());
    assert_eq!(result.baselines.len(), 2);
    assert_eq!(result.relative.len(), 2);

    let against_ehrbase = result
        .relative
        .iter()
        .find(|index| index.baseline == "ehrbase")
        .ok_or("no EHRbase index")?;
    assert!(
        against_ehrbase.gaps.is_empty(),
        "{:?}",
        against_ehrbase.gaps
    );
    let mut ratios = 0_usize;
    for phase in against_ehrbase.phases.values() {
        for operation in phase.operations.values() {
            for ratio in operation.metrics.values() {
                assert!((ratio.index - 0.5).abs() < 1e-9, "{ratio:?}");
                ratios = ratios.saturating_add(1);
            }
        }
    }
    assert!(ratios > 0, "the derivation produced no ratio at all");

    let against_ferroehr = result
        .relative
        .iter()
        .find(|index| index.baseline == "ferroehr")
        .ok_or("no FerroEHR index")?;
    let doubled = against_ferroehr
        .phases
        .values()
        .flat_map(|phase| phase.operations.values())
        .flat_map(|operation| operation.metrics.values())
        .all(|ratio| (ratio.index - 2.0).abs() < 1e-9);
    assert!(doubled, "{against_ferroehr:?}");

    let document = result.to_document()?;
    let violations = schema_violations(&document)?;
    assert!(violations.is_empty(), "{}", violations.join("; "));

    let summary = veredictum::bench::render::run_summary(&result);
    assert!(summary.contains("Machine: arch="), "{summary}");
    assert!(summary.contains("## Same-machine baselines"), "{summary}");
    assert!(summary.contains("## Relative index"), "{summary}");
    assert!(summary.contains("### vs EHRbase"), "{summary}");
    assert!(summary.contains("### vs FerroEHR"), "{summary}");
    assert!(summary.contains("@sha256:"), "{summary}");
    assert!(summary.contains(RELATIVE_DERIVATION), "{summary}");
    Ok(())
}

/// An operation the baseline never measured is recorded as a gap and named in
/// the rendered summary, so no reader mistakes silence for agreement.
#[test]
#[expect(
    clippy::panic_in_result_fn,
    reason = "the Book's Result-test shape: assertions panic, plumbing propagates with `?`"
)]
fn an_operation_missing_from_a_baseline_is_named_in_the_summary() -> Fallible {
    let sut = FakeSut::start();
    mount_healthy(&sut);
    let (mut result, _document, _summary) = drive_pack(&sut, &tiny_pack(), "target", 3, 1.0)?;

    let mut baseline = baseline_from(&result, ReferenceCdr::EhrBase, 1.0);
    let phase = baseline
        .cross
        .values_mut()
        .next()
        .ok_or("the target measured no phase")?;
    let dropped = phase
        .operations
        .keys()
        .next()
        .cloned()
        .ok_or("the target measured no operation")?;
    let _removed = phase.operations.remove(&dropped);
    result.attach_baselines(vec![baseline]);

    let index = result.relative.first().ok_or("no relative index")?;
    assert!(
        index
            .gaps
            .iter()
            .any(|gap| gap.operation == dropped
                && gap.reason == GapReason::OperationAbsentFromBaseline),
        "{:?}",
        index.gaps
    );
    let summary = veredictum::bench::render::run_summary(&result);
    assert!(summary.contains("No index exists for:"), "{summary}");
    assert!(
        summary.contains("operation-absent-from-baseline"),
        "{summary}"
    );

    let document = result.to_document()?;
    let violations = schema_violations(&document)?;
    assert!(violations.is_empty(), "{}", violations.join("; "));
    Ok(())
}

/// A comparison carries the machine in every column header and the relative
/// index in its own section, which is what makes two columns from different
/// hosts readable at all.
#[test]
#[expect(
    clippy::panic_in_result_fn,
    reason = "the Book's Result-test shape: assertions panic, plumbing propagates with `?`"
)]
fn a_comparison_header_names_the_machine_and_the_relative_index() -> Fallible {
    let sut = FakeSut::start();
    mount_healthy(&sut);
    let directory = assert_fs::TempDir::new()?;
    let mut paths = Vec::new();
    for (label, factor) in [("left", 2.0), ("right", 0.5)] {
        let (mut result, _document, _summary) = drive_pack(&sut, &tiny_pack(), label, 3, 1.0)?;
        result.attach_baselines(vec![baseline_from(&result, ReferenceCdr::EhrBase, factor)]);
        let target = directory.join(result.file_name());
        std::fs::write(&target, result.to_document()?)?;
        paths.push(target);
    }
    let outcome = compare_bench(&paths)?;
    let rendered = &outcome.document.body;
    assert!(rendered.contains("| Column | Machine |"), "{rendered}");
    assert!(rendered.contains("arch="), "{rendered}");
    assert!(
        rendered.contains("## Relative index per column"),
        "{rendered}"
    );
    assert!(rendered.contains("| EHRbase |"), "{rendered}");
    for column in &outcome.comparison.columns {
        assert!(column.submittable, "{column:?}");
        assert_eq!(column.relative.len(), 1);
        assert!(!column.environment.is_empty(), "{column:?}");
    }
    Ok(())
}

/// A record that is not submittable says WHICH requirements it misses, in the
/// comparison header and in its warnings, rather than one bare `false`.
#[test]
#[expect(
    clippy::panic_in_result_fn,
    reason = "the Book's Result-test shape: assertions panic, plumbing propagates with `?`"
)]
fn a_non_submittable_column_names_its_unmet_requirements() -> Fallible {
    let sut = FakeSut::start();
    mount_healthy(&sut);
    let directory = assert_fs::TempDir::new()?;
    let mut paths = Vec::new();
    for (label, repetitions) in [("thin", 1_u32), ("thick", 3)] {
        let (result, document, _summary) = drive_pack(&sut, &tiny_pack(), label, repetitions, 1.0)?;
        let target = directory.join(result.file_name());
        std::fs::write(&target, &document)?;
        paths.push(target);
    }
    let outcome = compare_bench(&paths)?;
    let rendered = &outcome.document.body;
    assert!(
        rendered.contains("no (repetitions, baseline)"),
        "{rendered}"
    );
    assert!(rendered.contains("no (baseline)"), "{rendered}");
    assert!(
        outcome
            .comparison
            .warnings
            .iter()
            .any(|warning| warning.contains("unmet: repetitions, baseline")),
        "{:?}",
        outcome.comparison.warnings
    );
    Ok(())
}

/// `--with-baselines` on a host with no container runtime refuses by name,
/// before the target is touched, so the flag never yields a half-anchored
/// record.
#[test]
fn a_missing_container_runtime_refuses_the_baseline_sweep() {
    let error = run_bench(
        &BenchRequest {
            pack: &smoke(),
            base_url: "http://127.0.0.1:1/openehr/v1",
            auth: AuthKind::None,
            user: None,
            repetitions: 3,
            label: None,
            scale: 1.0,
            seed_workers: None,
            with_baselines: true,
            docker: Some(DockerCli::at("/nonexistent/veredictum/docker")),
        },
        &|_message| {},
    )
    .unwrap_err()
    .to_string();
    assert!(
        error.contains("--with-baselines needs the docker CLI"),
        "{error}"
    );
    assert!(error.contains("/nonexistent/veredictum/docker"), "{error}");
}

/// A plain run needs no container runtime at all: the same nonexistent binary
/// is never consulted without the flag.
#[test]
#[expect(
    clippy::panic_in_result_fn,
    reason = "the Book's Result-test shape: assertions panic, plumbing propagates with `?`"
)]
fn a_run_without_the_flag_never_consults_the_container_runtime() -> Fallible {
    let sut = FakeSut::start();
    mount_healthy(&sut);
    let outcome = run_bench(
        &BenchRequest {
            pack: &tiny_pack(),
            base_url: &sut.base_url(),
            auth: AuthKind::None,
            user: None,
            repetitions: 1,
            label: Some("no-docker"),
            scale: 1.0,
            seed_workers: None,
            with_baselines: false,
            docker: Some(DockerCli::at("/nonexistent/veredictum/docker")),
        },
        &|_message| {},
    )?;
    assert!(outcome.result.baselines.is_empty());
    assert!(outcome.result.relative.is_empty());
    Ok(())
}
