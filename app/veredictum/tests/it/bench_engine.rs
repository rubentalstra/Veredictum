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
    BenchOp, BenchPack, BenchPhase, MeasurePhase, MixEntry, SeedPhase, aql_mix, community_vitals,
    smoke,
};
use veredictum::bench::posture::{
    Assurance, AuditSink, CompressionMode, MINIMAL, PostureItem, PostureProfile, SigningScheme,
    Tenancy, ValidationDepth,
};
use veredictum::bench::relative::{GapReason, RELATIVE_DERIVATION};
use veredictum::bench::result::{BaselineRecord, BenchResult, SubmissionRequirement};
use veredictum::pipeline::bench::{BenchRequest, compare_bench, run_bench};
use wiremock::matchers::{body_string_contains, method, path, path_regex};
use wiremock::{Mock, Request, Respond, ResponseTemplate};

use crate::fake_sut::FakeSut;

/// Answers a composition commit the way a template-validating server does:
/// the invalid twin (the pack's composition with the mandatory
/// `COMPOSITION.composer` gone) is refused `422`, everything else is created.
///
/// ITS-REST `specifications/responses/422.yaml` defines the refusal as the
/// case where the template "is not validating the supplied resource", and
/// `specifications/operations/composition_create.yaml` lists `422` on the
/// commit, so this is the spec-conformant answer rather than a convenience.
struct ValidatingCommit;

impl Respond for ValidatingCommit {
    fn respond(&self, request: &Request) -> ResponseTemplate {
        let body = String::from_utf8_lossy(&request.body);
        if body.contains("\"composer\"") {
            ResponseTemplate::new(201).insert_header("ETag", "\"c-1::sut::1\"")
        } else {
            ResponseTemplate::new(422)
        }
    }
}

/// A profile declaring no commit validation at all, so the same lenient
/// server that CONTRADICTS `minimal` CONFIRMS this one.
static UNVALIDATED: PostureProfile = PostureProfile {
    name: "test-unvalidated",
    summary: "A profile for this suite: nothing switched on, and commits unvalidated.",
    audit: AuditSink::Off,
    signing: SigningScheme::None,
    validation: ValidationDepth::None,
    compression: CompressionMode::Off,
    tenancy: Tenancy::Single,
};

/// A profile differing from `minimal` in the audit item alone, so a
/// comparison of the two has exactly one posture disagreement to state.
static AUDITED: PostureProfile = PostureProfile {
    name: "test-audited",
    summary: "A profile for this suite: the minimal surface with an audit trail written.",
    audit: AuditSink::Internal,
    signing: SigningScheme::None,
    validation: ValidationDepth::Template,
    compression: CompressionMode::Off,
    tenancy: Tenancy::Single,
};

/// A commit path that accepts anything, including the invalid twin, which is
/// what a server below the validation floor does.
struct AcceptingCommit;

impl Respond for AcceptingCommit {
    fn respond(&self, _request: &Request) -> ResponseTemplate {
        ResponseTemplate::new(201).insert_header("ETag", "\"c-1::sut::1\"")
    }
}

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
///
/// The rate is high enough that every operation in the vocabulary draws
/// several measured arrivals; the schedule is a pure function of the pack
/// seed, so what it covers here it covers on every run.
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
            rate_per_s: 120.0,
            warmup_s: 1,
            duration_s: 1,
            mix: BenchOp::ALL
                .iter()
                .map(|op| MixEntry::new(*op, 1, "the whole vocabulary at equal share"))
                .collect(),
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
            .respond_with(ValidatingCommit),
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
            profile: &MINIMAL,
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
    assert_eq!(
        result.pack.fixtures.len(),
        3,
        "the template, the composition and its pinned invalid twin"
    );
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

/// A run whose query arrivals all failed is refused for submission by the
/// engine itself, with the error-share requirement named and the rendered
/// summary saying where the ceiling went.
///
/// This is the run the requirement exists for: percentiles over arrivals that
/// never answered describe the failure, and a record like this reaching a
/// public board would rank it.
#[test]
#[expect(
    clippy::panic_in_result_fn,
    reason = "the Book's Result-test shape: assertions panic, plumbing propagates with `?`"
)]
fn a_run_whose_arrivals_failed_is_never_submittable() -> Fallible {
    let sut = FakeSut::start();
    // Mounted FIRST so it wins over the healthy query stub below: wiremock
    // matches in registration order.
    sut.mount(
        Mock::given(method("POST"))
            .and(path("/query/aql"))
            .respond_with(ResponseTemplate::new(500)),
    );
    mount_healthy(&sut);
    let (result, _document, summary) = drive_pack(&sut, &tiny_pack(), "all-failed", 3, 1.0)?;

    assert!(
        result
            .submittable_unmet
            .contains(&SubmissionRequirement::ErrorShare),
        "{:?}",
        result.submittable_unmet
    );
    assert!(!result.submittable);
    let breaches = result.failed_share_breaches();
    assert!(!breaches.is_empty(), "no breach was recorded");
    assert!(
        breaches.iter().all(|breach| breach
            .worst_operation
            .as_deref()
            .is_some_and(|op| op.starts_with("adhoc_query"))),
        "a healthy operation was blamed: {breaches:?}"
    );
    assert!(
        (result.worst_failed_share() - 1.0).abs() < f64::EPSILON,
        "{}",
        result.worst_failed_share()
    );
    assert!(summary.contains("## Failed-arrival share"), "{summary}");
    assert!(summary.contains("adhoc_query"), "{summary}");
    assert!(
        summary.contains("above the pack ceiling of 0.01"),
        "{summary}"
    );
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
            profile: &MINIMAL,
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
                profile: &MINIMAL,
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
            .respond_with(CommunityCommit),
    );
    mount_composition_reads(sut);
}

/// The community stack's commit path: the identifier body the reproduction
/// reads, and the same `422` on the invalid twin a validating server answers.
struct CommunityCommit;

impl Respond for CommunityCommit {
    fn respond(&self, request: &Request) -> ResponseTemplate {
        let body = String::from_utf8_lossy(&request.body);
        if body.contains("\"composer\"") {
            ResponseTemplate::new(201).set_body_json(json!({ "uid": "c-1::sut::1" }))
        } else {
            ResponseTemplate::new(422)
        }
    }
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
    assert_eq!(
        result.pack.fixtures.len(),
        3,
        "the template, the composition and its pinned invalid twin"
    );
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

/// The AQL pack at a unit-test population: three EHRs, two commits each, and
/// a one-second measured window over the same six query classes the pack
/// pins, at the same equal share.
fn tiny_aql_pack() -> Result<BenchPack, Box<dyn std::error::Error>> {
    let deck = aql_mix();
    let fixtures = deck.fixtures();
    let queries = deck
        .measure_phases()
        .first()
        .copied()
        .cloned()
        .ok_or("the AQL pack lost its measured phase")?;
    Ok(deck.with_phases(vec![
        BenchPhase::Seed(SeedPhase {
            name: "seed".to_owned(),
            fixtures,
            ehrs: 3,
            compositions_per_ehr: 2,
            workers: 2,
        }),
        BenchPhase::Measure(MeasurePhase {
            rate_per_s: 120.0,
            warmup_s: 1,
            duration_s: 1,
            ..queries
        }),
    ]))
}

/// The six query classes the AQL pack measures, read from the pack itself so
/// this list cannot drift from the definition.
fn aql_classes() -> Vec<String> {
    aql_mix().probe_rationales().into_keys().collect()
}

/// The whole AQL pack end to end: every class lands with its own percentiles,
/// its own sample count and its own error tally.
#[test]
#[expect(
    clippy::panic_in_result_fn,
    reason = "the Book's Result-test shape: assertions panic, plumbing propagates with `?`"
)]
fn the_aql_pack_records_one_set_of_percentiles_per_class() -> Fallible {
    let sut = FakeSut::start();
    mount_healthy(&sut);
    let (result, document, _summary) = drive_pack(&sut, &tiny_aql_pack()?, "aql", 1, 1.0)?;

    assert_eq!(result.pack.id, "aql-mix");
    assert_eq!(result.pack.version, "1.1.0");
    assert_eq!(
        result
            .pack
            .fixtures
            .get("vital_signs_composition.json")
            .map(String::as_str),
        Some("468081c259c737d35d7f80403562b3f333e479d267286faf80fd7c087eaba947")
    );

    let repetition = result.repetitions.first().ok_or("no repetition")?;
    let phase = repetition.phases.get("queries").ok_or("no query phase")?;
    assert_eq!(phase.regime.as_str(), "open-loop");
    let classes = aql_classes();
    assert_eq!(classes.len(), 6);
    for class in &classes {
        let stats = phase
            .operations
            .get(class)
            .ok_or_else(|| format!("{class} recorded no arrival"))?;
        assert!(stats.count > 0, "{class} recorded no arrival");
        assert_eq!(stats.errors, 0, "{class} failed on a healthy system");
        assert!(
            !stats.hdr_v2_base64.is_empty(),
            "{class} carries no encoding"
        );
        let decoded = stats.decode_histogram()?;
        assert_eq!(decoded.value_at_quantile(0.50), stats.p50_us);
    }
    // Every measured operation is one of the six classes: the pack offers no
    // read or write beside them.
    for operation in phase.operations.keys() {
        assert!(classes.contains(operation), "{operation} is off the mix");
    }
    // The cross-repetition summary carries the same classes, which is what
    // the relative index and bench-compare then align per class.
    let cross = result.cross.get("queries").ok_or("no cross summary")?;
    for class in &classes {
        assert!(
            cross.operations.contains_key(class),
            "{class} is missing from the cross summary"
        );
    }

    let violations = schema_violations(&document)?;
    assert!(violations.is_empty(), "{}", violations.join("; "));
    Ok(())
}

/// A server that fails one query shape and refuses another has each counted
/// in its own class, and the four healthy classes are not blamed for either.
#[test]
#[expect(
    clippy::panic_in_result_fn,
    reason = "the Book's Result-test shape: assertions panic, plumbing propagates with `?`"
)]
fn a_broken_query_class_never_blames_a_healthy_one() -> Fallible {
    let sut = FakeSut::start();
    // Mounted FIRST so they win over the healthy query stub: wiremock matches
    // in registration order, and the statement text is what separates one
    // class from another on a shared route.
    sut.mount(
        Mock::given(method("POST"))
            .and(path("/query/aql"))
            .and(body_string_contains("COUNT("))
            .respond_with(ResponseTemplate::new(500)),
    );
    sut.mount(
        Mock::given(method("POST"))
            .and(path("/query/aql"))
            .and(body_string_contains("ORDER BY"))
            .respond_with(ResponseTemplate::new(400)),
    );
    mount_healthy(&sut);
    let (result, document, _summary) = drive_pack(&sut, &tiny_aql_pack()?, "degraded", 1, 1.0)?;

    let repetition = result.repetitions.first().ok_or("no repetition")?;
    let phase = repetition.phases.get("queries").ok_or("no query phase")?;

    let aggregate = phase
        .operations
        .get("adhoc_query_aggregate")
        .ok_or("the aggregate class recorded no arrival")?;
    assert!(aggregate.errors > 0, "the 500s were not counted");
    assert_eq!(
        aggregate.errors_by_class.get("http_5xx").copied(),
        Some(aggregate.errors),
        "{:?}",
        aggregate.errors_by_class
    );

    let ordered = phase
        .operations
        .get("adhoc_query_ordered_page")
        .ok_or("the ordered-page class recorded no arrival")?;
    assert!(ordered.errors > 0, "the refusals were not counted");
    assert_eq!(
        ordered.errors_by_class.get("http_4xx").copied(),
        Some(ordered.errors),
        "{:?}",
        ordered.errors_by_class
    );

    for class in aql_classes() {
        if class == "adhoc_query_aggregate" || class == "adhoc_query_ordered_page" {
            continue;
        }
        let stats = phase
            .operations
            .get(&class)
            .ok_or_else(|| format!("{class} recorded no arrival"))?;
        assert_eq!(stats.errors, 0, "{class} was blamed for another class");
        assert!(stats.errors_by_class.is_empty(), "{class} carries a class");
    }

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
        posture: result.posture.clone(),
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
            profile: &MINIMAL,
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
            profile: &MINIMAL,
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

/// The declared posture reaches the record with every item labelled by what
/// the canaries could actually see: the five observable ones verified, and the
/// two nothing on the wire discloses honestly declared-only.
#[test]
#[expect(
    clippy::panic_in_result_fn,
    reason = "the Book's Result-test shape: assertions panic, plumbing propagates with `?`"
)]
fn the_posture_block_labels_every_item_verified_or_declared_only() -> Fallible {
    let sut = FakeSut::start();
    mount_healthy(&sut);
    let (result, document, summary) = drive_pack(&sut, &tiny_pack(), "posture", 1, 1.0)?;

    assert_eq!(result.posture.profile, "minimal");
    assert_eq!(result.posture.items.len(), PostureItem::ALL.len());
    assert_eq!(
        result.posture.declared(PostureItem::CommitValidation),
        Some("template")
    );
    assert_eq!(result.posture.declared(PostureItem::Authn), Some("none"));
    assert_eq!(result.posture.declared(PostureItem::Tls), Some("off"));

    // The five items with a black-box observable are verified against this
    // system; audit and tenancy have no read surface in released ITS-REST, so
    // the record says declared-only rather than claiming more than it saw.
    assert_eq!(
        result.posture.verified_items(),
        vec![
            PostureItem::VersionSigning,
            PostureItem::CommitValidation,
            PostureItem::Authn,
            PostureItem::Tls,
            PostureItem::Compression,
        ]
    );
    for item in [PostureItem::Audit, PostureItem::Tenancy] {
        let line = result
            .posture
            .items
            .iter()
            .find(|line| line.item == item)
            .ok_or("the posture block dropped an item")?;
        assert_eq!(line.assurance, Assurance::DeclaredOnly);
    }

    // Every item is bracketed: one reading before the measured window and one
    // after it.
    for line in &result.posture.items {
        assert_eq!(line.readings.len(), 2, "{} is not bracketed", line.item);
        assert_eq!(
            line.readings.first().map(|read| read.bracket.as_str()),
            Some("before")
        );
        assert_eq!(
            line.readings.get(1).map(|read| read.bracket.as_str()),
            Some("after")
        );
        assert!(
            !line
                .readings
                .iter()
                .any(|read| read.evidence.trim().is_empty())
        );
    }

    let violations = schema_violations(&document)?;
    assert!(violations.is_empty(), "{}", violations.join("; "));
    assert!(summary.contains("## Posture `minimal`"), "{summary}");
    assert!(summary.contains("declared-only"), "{summary}");
    assert!(
        summary.contains("| `commit_validation` | `template` | verified |"),
        "{summary}"
    );
    Ok(())
}

/// A system that accepts the pinned invalid twin contradicts a `minimal`
/// declaration, and the run is REFUSED with the item named rather than
/// recorded with a footnote.
#[test]
fn a_lenient_server_contradicts_the_declared_validation_depth() {
    let sut = FakeSut::start();
    // Mounted FIRST so it wins over the validating stub below: wiremock
    // matches in registration order.
    sut.mount(
        Mock::given(method("POST"))
            .and(path_regex(r"^/ehr/[^/]+/composition$"))
            .respond_with(AcceptingCommit),
    );
    mount_healthy(&sut);
    let deck = tiny_pack();
    let error = run_bench(
        &BenchRequest {
            pack: &deck,
            profile: &MINIMAL,
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
    assert!(error.contains("posture canary contradicts"), "{error}");
    assert!(error.contains("commit_validation"), "{error}");
    assert!(error.contains("accepts-the-invalid-twin"), "{error}");
}

/// A profile that declares no validation at all is verified against that same
/// lenient server, which is what makes the label mean something: the canary
/// reports what it saw rather than what anyone hoped for.
#[test]
#[expect(
    clippy::panic_in_result_fn,
    reason = "the Book's Result-test shape: assertions panic, plumbing propagates with `?`"
)]
fn a_declared_absence_of_validation_is_verified_the_same_way() -> Fallible {
    let sut = FakeSut::start();
    sut.mount(
        Mock::given(method("POST"))
            .and(path_regex(r"^/ehr/[^/]+/composition$"))
            .respond_with(AcceptingCommit),
    );
    mount_healthy(&sut);
    let deck = tiny_pack();
    let outcome = run_bench(
        &BenchRequest {
            pack: &deck,
            profile: &UNVALIDATED,
            base_url: &sut.base_url(),
            auth: AuthKind::None,
            user: None,
            repetitions: 1,
            label: Some("unvalidated"),
            scale: 1.0,
            seed_workers: None,
            with_baselines: false,
            docker: None,
        },
        &|_message| {},
    )?;
    assert_eq!(outcome.result.posture.profile, "test-unvalidated");
    assert_eq!(
        outcome
            .result
            .posture
            .declared(PostureItem::CommitValidation),
        Some("none")
    );
    assert!(
        outcome
            .result
            .posture
            .verified_items()
            .contains(&PostureItem::CommitValidation)
    );
    Ok(())
}

/// A server that signs the versions this run's OWN traffic committed
/// contradicts a declaration of `none`, so the sampling reads the measured
/// population rather than a probe an operator could special-case.
#[test]
fn a_signed_population_contradicts_a_declaration_of_no_signing() {
    let sut = FakeSut::start();
    sut.mount(
        Mock::given(method("GET"))
            .and(path_regex(
                r"^/ehr/[^/]+/versioned_composition/[^/]+/version/[^/]+$",
            ))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "_type": "ORIGINAL_VERSION",
                "signature": "sha256:6c1f2f8ac0d6dd1c8f0e2f0a0b1c2d3e4f5061728394a5b6c7d8e9f001122334"
            }))),
    );
    mount_healthy(&sut);
    let deck = tiny_pack();
    let error = run_bench(
        &BenchRequest {
            pack: &deck,
            profile: &MINIMAL,
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
    assert!(error.contains("posture canary contradicts"), "{error}");
    assert!(error.contains("version_signing"), "{error}");
    assert!(error.contains("digest"), "{error}");
}

/// Two columns taken under different profiles are two different sports, and
/// the comparison says so in the header rather than beside the numbers.
#[test]
#[expect(
    clippy::panic_in_result_fn,
    reason = "the Book's Result-test shape: assertions panic, plumbing propagates with `?`"
)]
fn a_posture_mismatch_is_stated_in_the_comparison_header() -> Fallible {
    let sut = FakeSut::start();
    mount_healthy(&sut);
    let directory = assert_fs::TempDir::new()?;
    let mut paths: Vec<PathBuf> = Vec::new();
    for (label, profile) in [("bare", &MINIMAL), ("audited", &AUDITED)] {
        let deck = tiny_pack();
        let outcome = run_bench(
            &BenchRequest {
                pack: &deck,
                profile,
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
    let outcome = compare_bench(&paths)?;
    assert!(
        outcome
            .comparison
            .warnings
            .iter()
            .any(|warning| warning.contains("DIFFERENT posture profiles")),
        "{:?}",
        outcome.comparison.warnings
    );
    assert!(
        outcome
            .comparison
            .warnings
            .iter()
            .any(|warning| warning.contains("DIFFERENT postures")),
        "{:?}",
        outcome.comparison.warnings
    );
    let header_at = outcome.document.body.find("DIFFERENT posture profiles");
    let table_at = outcome.document.body.find("## Aligned metrics");
    assert!(header_at < table_at, "{}", outcome.document.body);
    assert!(
        outcome.document.body.contains("| `test-audited` |"),
        "{}",
        outcome.document.body
    );
    Ok(())
}

/// Writes an executable stand-in for the docker CLI and answers `docker
/// version` with `version`.
///
/// `DockerCli` carries its binary path for exactly this
/// (`bench::baselines`'s own module documentation), so the orchestration
/// around a container runtime — the compose document that gets written, the
/// teardown that runs whether or not the measurement succeeded, the workspace
/// that gets cleaned up — is provable without composing a real stack, which
/// costs minutes and two multi-hundred-megabyte images.
///
/// The script logs every invocation, one argument line per call, so a test
/// asserts on what the engine ASKED the runtime to do rather than on a
/// side effect.
fn fake_docker(
    dir: &std::path::Path,
    version: &str,
    up_exit: i32,
) -> Result<(PathBuf, PathBuf), std::io::Error> {
    let binary = dir.join("docker");
    let log = dir.join("docker.log");
    let script = format!(
        r#"#!/bin/sh
printf '%s\n' "$*" >> '{log}'
case "$1" in
  version) printf '{version}\n' ;;
  compose)
    for arg in "$@"; do
      case "$arg" in
        up) printf 'compose up refused by the fake runtime\n' >&2 ; exit {up_exit} ;;
        down) exit 0 ;;
      esac
    done
    ;;
esac
exit 0
"#,
        log = log.display(),
    );
    std::fs::write(&binary, script)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt as _;
        std::fs::set_permissions(&binary, std::fs::Permissions::from_mode(0o700))?;
    }
    Ok((binary, log))
}

/// The lines a [`fake_docker`] run recorded, or an empty list when it was
/// never invoked at all.
fn docker_log(log: &std::path::Path) -> Vec<String> {
    std::fs::read_to_string(log)
        .unwrap_or_default()
        .lines()
        .map(str::to_owned)
        .collect()
}

/// A runtime that answers `docker version` is probed once for the whole
/// sweep, and the version it disclosed reaches the progress stream.
#[test]
#[expect(
    clippy::panic_in_result_fn,
    reason = "the Book's Result-test shape: assertions panic, plumbing propagates with `?`"
)]
fn the_container_runtime_is_probed_once_and_its_version_reported() -> Fallible {
    let scratch = assert_fs::TempDir::new()?;
    let (binary, log) = fake_docker(scratch.path(), "27.5.1", 1)?;
    let docker = DockerCli::at(&binary);
    assert_eq!(docker.binary(), binary.as_path());
    assert_eq!(docker.probe()?, "27.5.1");

    let calls = docker_log(&log);
    assert_eq!(calls.len(), 1, "{calls:?}");
    assert!(
        calls
            .first()
            .is_some_and(|call| call.starts_with("version")),
        "{calls:?}"
    );
    Ok(())
}

/// A runtime that answers `docker version` with a failure is unavailable, and
/// the refusal carries the binary that was asked plus what it said.
#[test]
#[expect(
    clippy::panic_in_result_fn,
    reason = "the Book's Result-test shape: assertions panic, plumbing propagates with `?`"
)]
fn a_runtime_that_refuses_its_version_is_unavailable() -> Fallible {
    let scratch = assert_fs::TempDir::new()?;
    let binary = scratch.path().join("docker");
    std::fs::write(
        &binary,
        "#!/bin/sh\nprintf 'Cannot connect to the Docker daemon\\n' >&2\nexit 1\n",
    )?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt as _;
        std::fs::set_permissions(&binary, std::fs::Permissions::from_mode(0o700))?;
    }
    let error = DockerCli::at(&binary)
        .probe()
        .expect_err("a runtime that exits non-zero on `version` is not available")
        .to_string();
    assert!(error.contains(&binary.display().to_string()), "{error}");
    assert!(
        error.contains("Cannot connect to the Docker daemon"),
        "{error}"
    );
    Ok(())
}

/// The sweep writes the pinned compose document and its side files, asks the
/// runtime to bring the stack up, and — when that fails — tears the project
/// down anyway and removes the workspace it made.
///
/// The teardown-regardless property is the one a leaked container costs real
/// money for, and it is exactly the property a real-stack test cannot prove
/// cheaply: this drives it through the refusal arm, which reaches it in
/// milliseconds.
#[test]
#[expect(
    clippy::panic_in_result_fn,
    reason = "the Book's Result-test shape: assertions panic, plumbing propagates with `?`"
)]
fn a_refused_compose_still_tears_the_project_down() -> Fallible {
    use veredictum::bench::baselines::{BaselineRun, run_baselines};

    let scratch = assert_fs::TempDir::new()?;
    let (binary, log) = fake_docker(scratch.path(), "27.5.1", 3)?;
    let docker = DockerCli::at(&binary);
    let pack = smoke();

    let mut reported: Vec<String> = Vec::new();
    let progress = std::sync::Mutex::new(&mut reported);
    let error = run_baselines(
        &BaselineRun {
            pack: &pack,
            profile: &MINIMAL,
            repetitions: 1,
            scale: 1.0,
            seed_workers: None,
            docker: &docker,
        },
        &|message| {
            if let Ok(mut sink) = progress.lock() {
                sink.push(message);
            }
        },
    )
    .expect_err("a compose that exits non-zero refuses the baseline");

    // The refusal names the baseline and quotes the runtime's own diagnostic,
    // rather than a generic "the sweep failed".
    let text = error.to_string();
    let first = ReferenceCdr::ALL
        .first()
        .expect("the reference set is not empty");
    assert!(text.contains(first.as_str()), "{text}");
    assert!(
        text.contains("compose up refused by the fake runtime"),
        "{text}"
    );

    let calls = docker_log(&log);
    assert!(
        calls
            .first()
            .is_some_and(|call| call.starts_with("version")),
        "the runtime is proved before anything is composed: {calls:?}"
    );
    let up = calls
        .iter()
        .find(|call| call.contains(" up "))
        .expect("the sweep asked the runtime to bring the stack up");
    assert!(up.contains(&first.pin().project()), "{up}");
    assert!(up.contains("--wait"), "{up}");
    let down = calls
        .iter()
        .find(|call| call.contains(" down "))
        .expect("a refused compose is still torn down");
    assert!(down.contains(&first.pin().project()), "{down}");
    assert!(
        down.contains("--volumes"),
        "fresh volumes per baseline is the fairness rule: {down}"
    );
    assert!(
        calls.iter().all(|call| !call.contains(
            ReferenceCdr::ALL
                .get(1)
                .map_or("", |cdr| cdr.pin().recipe_repository)
        )),
        "the sweep stops at the first refusal rather than composing the next: {calls:?}"
    );

    // The workspace the sweep wrote is gone with it: a compose document left
    // in the temp directory would be inherited by the next run on this host.
    let workspace =
        std::env::temp_dir().join(format!("{}-{}", first.pin().project(), std::process::id()));
    assert!(!workspace.exists(), "{}", workspace.display());

    assert!(
        reported
            .iter()
            .any(|line| line.contains("container runtime answers, server version 27.5.1")),
        "{reported:?}"
    );
    assert!(
        reported.iter().any(|line| line.contains("tearing down")),
        "{reported:?}"
    );
    Ok(())
}

/// A plain client over one fake SUT, with no authentication.
fn client_for(
    sut: &FakeSut,
) -> Result<veredictum::bench::client::BenchClient, Box<dyn std::error::Error>> {
    Ok(veredictum::bench::client::BenchClient::new(
        &sut.base_url(),
        AuthKind::None,
        None,
    )?)
}

/// Preflight stops at the FIRST exchange that does not answer, and the
/// refusal names that exchange rather than "the run failed".
///
/// Every arm is its own fake SUT: a refusal that fires only because an
/// earlier one already fired proves nothing about the arm it claims to pin.
#[test]
#[expect(
    clippy::panic_in_result_fn,
    reason = "the Book's Result-test shape: assertions panic, plumbing propagates with `?`"
)]
fn preflight_names_the_exchange_that_refused_it() -> Fallible {
    let pack = smoke();

    // 1. The template list itself is refused.
    let sut = FakeSut::start();
    sut.mount(
        Mock::given(method("GET"))
            .and(path("/definition/template/adl1.4"))
            .respond_with(ResponseTemplate::new(503)),
    );
    let error = veredictum::bench::run::preflight(&client_for(&sut)?, &pack)
        .expect_err("a server that will not list templates is not benchable")
        .to_string();
    assert!(error.contains("template list"), "{error}");
    assert!(error.contains("503"), "{error}");

    // 2. The template upload answers a status that is neither created nor
    //    already-there, which is the only other acceptable answer.
    let sut = FakeSut::start();
    sut.mount(
        Mock::given(method("GET"))
            .and(path("/definition/template/adl1.4"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!([]))),
    );
    sut.mount(
        Mock::given(method("POST"))
            .and(path("/definition/template/adl1.4"))
            .respond_with(ResponseTemplate::new(422)),
    );
    let error = veredictum::bench::run::preflight(&client_for(&sut)?, &pack)
        .expect_err("a template the server will not accept stops the run")
        .to_string();
    assert!(error.contains("template upload"), "{error}");
    assert!(
        error.contains("201, 204 or 409 expected"),
        "the refusal states what a conformant answer would have been: {error}"
    );

    // 3. The scratch EHR create answers something other than 201.
    let sut = FakeSut::start();
    mount_templates(&sut);
    sut.mount(
        Mock::given(method("POST"))
            .and(path("/ehr"))
            .respond_with(ResponseTemplate::new(200)),
    );
    let error = veredictum::bench::run::preflight(&client_for(&sut)?, &pack)
        .expect_err("a create that is not a 201 refuses the preflight")
        .to_string();
    assert!(error.contains("scratch ehr create"), "{error}");
    assert!(error.contains("201 expected"), "{error}");

    // 4. The create IS a 201, and discloses no identifier anywhere — no uid
    //    body, no ETag, no Location. The run cannot proceed, and says why.
    let sut = FakeSut::start();
    mount_templates(&sut);
    sut.mount(
        Mock::given(method("POST"))
            .and(path("/ehr"))
            .respond_with(ResponseTemplate::new(201)),
    );
    let error = veredictum::bench::run::preflight(&client_for(&sut)?, &pack)
        .expect_err("a create that discloses no ehr_id refuses the preflight")
        .to_string();
    assert!(error.contains("disclosed no ehr_id"), "{error}");

    // 5. The commit answers a status that is not a creation.
    let sut = FakeSut::start();
    mount_templates(&sut);
    mount_ehr_create(&sut);
    sut.mount(
        Mock::given(method("POST"))
            .and(path_regex(r"^/ehr/[^/]+/composition$"))
            .respond_with(ResponseTemplate::new(400)),
    );
    let error = veredictum::bench::run::preflight(&client_for(&sut)?, &pack)
        .expect_err("a refused commit refuses the preflight")
        .to_string();
    assert!(error.contains("scratch composition commit"), "{error}");
    assert!(error.contains("201 or 204 expected"), "{error}");

    // 6. The commit is created and discloses no version uid.
    let sut = FakeSut::start();
    mount_templates(&sut);
    mount_ehr_create(&sut);
    sut.mount(
        Mock::given(method("POST"))
            .and(path_regex(r"^/ehr/[^/]+/composition$"))
            .respond_with(ResponseTemplate::new(201)),
    );
    let error = veredictum::bench::run::preflight(&client_for(&sut)?, &pack)
        .expect_err("a commit that discloses no version uid refuses the preflight")
        .to_string();
    assert!(error.contains("disclosed no version uid"), "{error}");

    // 7. Everything is created and the read back does not answer 200, which
    //    is the write-then-READ half the preflight exists to prove.
    let sut = FakeSut::start();
    mount_templates(&sut);
    mount_ehr_create(&sut);
    sut.mount(
        Mock::given(method("POST"))
            .and(path_regex(r"^/ehr/[^/]+/composition$"))
            .respond_with(ResponseTemplate::new(201).insert_header("ETag", "\"c-1::sut::1\"")),
    );
    sut.mount(
        Mock::given(method("GET"))
            .and(path_regex(r"^/ehr/[^/]+/composition/[^/]+$"))
            .respond_with(ResponseTemplate::new(404)),
    );
    let error = veredictum::bench::run::preflight(&client_for(&sut)?, &pack)
        .expect_err("a commit that cannot be read back refuses the preflight")
        .to_string();
    assert!(error.contains("scratch composition read"), "{error}");
    assert!(error.contains("200 expected"), "{error}");
    Ok(())
}

/// The template list and upload a healthy server answers.
fn mount_templates(sut: &FakeSut) {
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
}

/// A scratch EHR create that discloses its identifier through `Location`.
fn mount_ehr_create(sut: &FakeSut) {
    sut.mount(Mock::given(method("POST")).and(path("/ehr")).respond_with(
        ResponseTemplate::new(201).insert_header("Location", "http://sut/ehr/EHR-1"),
    ));
}

/// A server that discloses a version has it recorded; one that discloses
/// none is legitimately silent, and silence is `None` rather than a run
/// failure or an invented string.
///
/// No openEHR specification defines a version-disclosure endpoint, so every
/// arm here is our own best-effort probe over shapes deployments serve.
#[test]
#[expect(
    clippy::panic_in_result_fn,
    reason = "the Book's Result-test shape: assertions panic, plumbing propagates with `?`"
)]
fn the_version_probe_reads_what_a_server_discloses_and_nothing_more() -> Fallible {
    use veredictum::bench::run::probe_sut_version;

    // The first shape: `system/info` carrying `solution_version`.
    let sut = FakeSut::start();
    sut.mount(
        Mock::given(method("GET"))
            .and(path("/system/info"))
            .respond_with(
                ResponseTemplate::new(200).set_body_json(json!({ "solution_version": "2.19.0" })),
            ),
    );
    assert_eq!(
        probe_sut_version(&client_for(&sut)?).as_deref(),
        Some("2.19.0")
    );

    // The second shape: the base path carrying a nested `info.version`, read
    // only because the first shape answered nothing usable.
    let sut = FakeSut::start();
    sut.mount(
        Mock::given(method("GET"))
            .and(path("/system/info"))
            .respond_with(ResponseTemplate::new(404)),
    );
    sut.mount(Mock::given(method("GET")).and(path("/")).respond_with(
        ResponseTemplate::new(200).set_body_json(json!({
            "info": { "version": "1.4.2" }
        })),
    ));
    assert_eq!(
        probe_sut_version(&client_for(&sut)?).as_deref(),
        Some("1.4.2")
    );

    // A body that is not JSON at all, and a JSON body carrying none of the
    // three pointers: both are silence, never a guess.
    let sut = FakeSut::start();
    sut.mount(
        Mock::given(method("GET"))
            .and(path("/system/info"))
            .respond_with(ResponseTemplate::new(200).set_body_string("not json")),
    );
    sut.mount(
        Mock::given(method("GET"))
            .and(path("/"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({ "name": "a cdr" }))),
    );
    assert_eq!(probe_sut_version(&client_for(&sut)?), None);

    // A server that refuses both probes discloses nothing, and the probe is
    // still not an error.
    let sut = FakeSut::start();
    sut.mount(Mock::given(method("GET")).respond_with(ResponseTemplate::new(500)));
    assert_eq!(probe_sut_version(&client_for(&sut)?), None);
    Ok(())
}
