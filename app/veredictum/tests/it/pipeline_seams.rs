// SPDX-FileCopyrightText: Veredictum contributors
// SPDX-License-Identifier: Apache-2.0

//! The consumable pipeline seams (`veredictum::pipeline`): loading and
//! validating a catalogue, judging a committed campaign, rendering the
//! published assets, and the typed failures each seam reports.
//!
//! Every input here is a file already committed to this repository — the
//! catalogue, the vendored spec tree, a party statement, the example results
//! document — so the seams are exercised the way a second consumer of the
//! library would drive them, with no system under test involved.

use std::path::{Path, PathBuf};

use veredictum::pipeline::{
    Error, assets, catalogue, judgement, measured, read_json, to_json_document,
};

/// Anything a seam or a fixture read can fail with, so a test body
/// propagates plumbing failures with `?` instead of unwrapping them
/// (<https://doc.rust-lang.org/book/ch11-01-writing-tests.html>).
type Fallible = Result<(), Box<dyn std::error::Error>>;

/// The repository root: the crate sits two levels under it.
fn repo_root() -> PathBuf {
    PathBuf::from(concat!(env!("CARGO_MANIFEST_DIR"), "/../.."))
}

fn artifacts() -> PathBuf {
    repo_root().join("artifacts")
}

fn specs() -> PathBuf {
    repo_root().join("specs/openehr")
}

/// The committed statement of the party whose declarations the catalogue's
/// claim-completeness gate already validates.
fn statement_path() -> PathBuf {
    repo_root().join("party/ehrbase/statement.json")
}

/// The committed example results document (`examples/results.example.json`).
fn results_path() -> PathBuf {
    PathBuf::from(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/examples/results.example.json"
    ))
}

/// Judges the example campaign against the committed catalogue.
fn judge_example() -> Result<judgement::Judgement, Error> {
    let statement = statement_path();
    let results = results_path();
    let root = artifacts();
    judgement::judge(&judgement::JudgementRequest {
        statement: &statement,
        results: &results,
        root: &root,
    })
}

// ── the catalogue seam ──────────────────────────────────────────────────────

/// The seam reports exactly what the `validate` gate reports over the same
/// root, so a consumer reading the seam sees the command line's own findings
/// — neither filtered nor reordered.
#[test]
#[expect(
    clippy::panic_in_result_fn,
    reason = "the Book's Result-test shape: assertions panic, plumbing propagates with `?`"
)]
fn the_validation_seam_reports_the_gate_findings_unchanged() -> Fallible {
    let root = artifacts();
    let specs = specs();
    let validation = catalogue::validate_tree(&root, Some(&specs))?;

    let loaded = veredictum::artifacts::load_root(&root)?;
    let direct = veredictum::validate::validate(&veredictum::validate::Context {
        set: &loaded.set,
        load_errors: &loaded.errors,
        spec_root: Some(&specs),
    });
    let rendered = |findings: &[veredictum::validate::Finding]| -> Vec<String> {
        findings.iter().map(ToString::to_string).collect()
    };
    assert_eq!(rendered(&validation.findings), rendered(&direct));
    assert_eq!(
        validation.is_clean(),
        validation.findings.is_empty(),
        "`is_clean` means no finding at all"
    );
    Ok(())
}

/// The oracle-dependent gates cannot run without the vendored spec tree, so
/// dropping it never turns a finding INTO cleanliness: the findings without
/// the oracle are a subset of the findings with it.
#[test]
#[expect(
    clippy::panic_in_result_fn,
    reason = "the Book's Result-test shape: assertions panic, plumbing propagates with `?`"
)]
fn dropping_the_oracle_only_removes_findings() -> Fallible {
    let root = artifacts();
    let specs = specs();
    let with_oracle = catalogue::validate_tree(&root, Some(&specs))?;
    let without = catalogue::validate_tree(&root, None)?;
    let with_rendered: Vec<String> = with_oracle
        .findings
        .iter()
        .map(ToString::to_string)
        .collect();
    for finding in &without.findings {
        assert!(
            with_rendered.contains(&finding.to_string()),
            "the oracle-less pass invented a finding: {finding}"
        );
    }
    assert_eq!(
        with_oracle.loaded.set.cases.len(),
        without.loaded.set.cases.len(),
        "the oracle changes the gates, never what loaded"
    );
    Ok(())
}

/// The report path follows whichever spec tree was validated against: two
/// levels above the component directory, beside the conformance artifacts.
#[test]
fn the_coverage_report_path_follows_the_spec_tree() {
    assert_eq!(
        catalogue::coverage_report_path(Path::new("/tmp/docs/specs/openehr")),
        Some(PathBuf::from("/tmp/docs/conformance/coverage-report.md"))
    );
    assert_eq!(
        catalogue::coverage_report_path(Path::new("openehr")),
        None,
        "a path with no grandparent names no report location"
    );
}

/// The write is the render: the seam creates the directory it was pointed at
/// and writes exactly what the renderer produced.
#[test]
#[expect(
    clippy::panic_in_result_fn,
    reason = "the Book's Result-test shape: assertions panic, plumbing propagates with `?`"
)]
fn the_coverage_report_is_written_where_it_is_asked_for() -> Fallible {
    let root = artifacts();
    let specs = specs();
    let loaded = veredictum::artifacts::load_root(&root)?;
    let dir = assert_fs::TempDir::new()?;
    let path = dir.path().join("nested/conformance/coverage-report.md");

    catalogue::write_coverage_report(&loaded.set, &specs, &path)?;

    let written = std::fs::read_to_string(&path)?;
    assert_eq!(
        written,
        veredictum::validate::render_coverage_report(&loaded.set, Some(&specs)),
        "the written report is the rendered report, byte for byte"
    );
    assert!(!written.is_empty());
    Ok(())
}

// ── the judgement seam ─────────────────────────────────────────────────────

/// The submission set is the verdict record, the three documents and the
/// badge endpoints, in publication order — one seam call produces all of it
/// from three committed files.
#[test]
#[expect(
    clippy::panic_in_result_fn,
    reason = "the Book's Result-test shape: assertions panic, plumbing propagates with `?`"
)]
fn judging_a_committed_campaign_renders_its_whole_submission_set() -> Fallible {
    let judgement = judge_example()?;

    let names: Vec<&str> = judgement
        .documents
        .iter()
        .map(|d| d.name.as_str())
        .collect();
    assert_eq!(
        &names[..4],
        &[
            "verdicts.json",
            "CONFORMANCE_REPORT.md",
            "CONFORMANCE_STATEMENT.md",
            "CONFORMANCE_CERTIFICATE.md",
        ],
        "the verdict record leads the set, then the three documents"
    );
    assert!(
        names.len() > 4
            && names[4..]
                .iter()
                .all(|n| Path::new(n).extension().is_some_and(|ext| ext == "json")),
        "the badge endpoints follow the documents: {names:?}"
    );
    for document in &judgement.documents {
        assert!(!document.body.is_empty(), "{} is empty", document.name);
    }

    assert_eq!(judgement.results.sut.name, "example-cdr");
    assert_eq!(
        judgement.statement.product.name, "EHRbase",
        "the statement travels back with the judgement"
    );
    assert!(
        !judgement.report.capabilities.is_empty(),
        "every matrix capability carries evidence"
    );
    assert_eq!(
        judgement.is_clean(),
        judgement.report.review.is_empty(),
        "`is_clean` is the static review being empty"
    );
    Ok(())
}

/// The rendered `verdicts.json` is the report itself: a consumer parsing the
/// document sees the same per-capability evidence the seam computed.
#[test]
#[expect(
    clippy::panic_in_result_fn,
    reason = "the Book's Result-test shape: assertions panic, plumbing propagates with `?`"
)]
fn the_verdict_document_carries_the_computed_evidence() -> Fallible {
    let judgement = judge_example()?;
    let verdicts = judgement
        .documents
        .iter()
        .find(|d| d.name == "verdicts.json")
        .ok_or("the submission set carries a verdict record")?;
    assert!(
        verdicts.body.ends_with('\n'),
        "every emitted document ends with a newline"
    );
    let parsed: assets::VerdictEvidence = serde_json::from_str(&verdicts.body)?;
    assert_eq!(
        parsed.capabilities.len(),
        judgement.report.capabilities.len()
    );
    assert_eq!(
        parsed.capabilities.first().map(|(name, _)| name.clone()),
        judgement
            .report
            .capabilities
            .first()
            .map(|(name, _)| name.to_string()),
        "the record keeps the capability-matrix authored order"
    );
    Ok(())
}

/// A campaign the results record does not describe is refused rather than
/// judged: the statement path must name a statement.
#[test]
fn judging_refuses_a_document_that_is_not_a_statement() {
    let results = results_path();
    let root = artifacts();
    // The results document is a valid party artifact of the WRONG family, so
    // the schema stage is what rejects it.
    let error = judgement::judge(&judgement::JudgementRequest {
        statement: &results,
        results: &results,
        root: &root,
    })
    .expect_err("a results document is not a statement");
    let message = error.to_string();
    assert!(matches!(error, Error::Party(_)), "{message}");
    assert!(message.contains("schema"), "{message}");
}

// ── the asset seams ────────────────────────────────────────────────────────

/// The conformance visuals are a pure function of the two committed records,
/// and the suffix is what puts a comparison SUT's copies beside the primary
/// ones.
#[test]
#[expect(
    clippy::panic_in_result_fn,
    reason = "the Book's Result-test shape: assertions panic, plumbing propagates with `?`"
)]
fn the_conformance_visuals_render_from_committed_records() -> Fallible {
    let judgement = judge_example()?;
    let verdicts = judgement
        .documents
        .iter()
        .find(|d| d.name == "verdicts.json")
        .ok_or("the submission set carries a verdict record")?;
    let dir = assert_fs::TempDir::new()?;
    let verdicts_path = dir.path().join("verdicts.json");
    std::fs::write(&verdicts_path, &verdicts.body)?;

    let root = artifacts();
    let results = results_path();
    let files = assets::conformance_assets(&root, &results, &verdicts_path, "-compare")?;
    let names: Vec<&str> = files.iter().map(|f| f.name.as_str()).collect();
    assert_eq!(
        names,
        vec![
            "conformance-heat-grid-compare.svg",
            "conformance-chapter-bars-compare.svg"
        ]
    );
    for file in &files {
        assert!(file.body.starts_with("<svg"), "{} is not SVG", file.name);
        assert!(
            file.body.contains("example-cdr 0.0.0-example"),
            "{} does not label the system under test",
            file.name
        );
    }

    // Deterministic: the same inputs render the same bytes, which is what
    // makes regenerate-and-diff a build gate.
    let again = assets::conformance_assets(&root, &results, &verdicts_path, "-compare")?;
    assert_eq!(
        files.iter().map(|f| f.body.clone()).collect::<Vec<_>>(),
        again.iter().map(|f| f.body.clone()).collect::<Vec<_>>()
    );
    Ok(())
}

/// A verdict record the seam cannot read is a typed parse failure naming what
/// was being read, never a chart drawn from nothing.
#[test]
#[expect(
    clippy::panic_in_result_fn,
    reason = "the Book's Result-test shape: assertions panic, plumbing propagates with `?`"
)]
fn the_conformance_visuals_refuse_an_unreadable_verdict_record() -> Fallible {
    let dir = assert_fs::TempDir::new()?;
    let root = artifacts();
    let results = results_path();

    let missing = dir.path().join("absent.json");
    let error = assets::conformance_assets(&root, &results, &missing, "")
        .expect_err("an absent verdict record is not renderable");
    assert!(matches!(error, Error::Read { .. }), "{error}");

    let broken = dir.path().join("broken.json");
    std::fs::write(&broken, "{ not json")?;
    let error = assets::conformance_assets(&root, &results, &broken, "")
        .expect_err("a malformed verdict record is not renderable");
    assert!(matches!(error, Error::Parse { .. }), "{error}");
    assert!(error.to_string().starts_with("verdicts:"), "{error}");
    Ok(())
}

/// The performance visuals render the class ladder plus one latency chart per
/// measurement, and the resource and disk charts only from records that
/// actually carry those samples.
#[test]
#[expect(
    clippy::panic_in_result_fn,
    reason = "the Book's Result-test shape: assertions panic, plumbing propagates with `?`"
)]
fn the_performance_visuals_render_only_what_the_record_carries() -> Fallible {
    let root = artifacts();
    let results = results_path();
    let rendered = assets::performance_assets(&root, &results, None)?;
    let names: Vec<&str> = rendered.files.iter().map(|f| f.name.as_str()).collect();

    assert_eq!(names.first(), Some(&"perf-class-ladder.svg"));
    assert!(
        names.contains(&"perf-latency-class-POC.svg"),
        "the example's measured class gets its latency chart: {names:?}"
    );
    assert!(
        !names.iter().any(|n| n.starts_with("perf-resources-")),
        "the example samples no container resources, so no series is drawn: {names:?}"
    );
    for file in &rendered.files {
        assert!(file.body.starts_with("<svg"), "{} is not SVG", file.name);
    }

    let summary = rendered.summary_markdown()?;
    assert!(
        summary.contains("POC"),
        "the summary reports the measured class: {summary}"
    );
    Ok(())
}

/// The schema seam serves exactly the published set: the same names and the
/// same bytes as the committed `schemas/` directory a party validates against.
#[test]
#[expect(
    clippy::panic_in_result_fn,
    reason = "the Book's Result-test shape: assertions panic, plumbing propagates with `?`"
)]
fn the_schema_seam_serves_the_committed_published_set() -> Fallible {
    let dir = repo_root().join("schemas");
    let files = assets::schema_files();
    assert!(!files.is_empty());
    for file in &files {
        let committed = std::fs::read_to_string(dir.join(&file.name))?;
        assert_eq!(
            committed, file.body,
            "{} differs from the committed schema",
            file.name
        );
    }
    Ok(())
}

// ── the shared readers ─────────────────────────────────────────────────────

/// A root that is not a catalogue at all is a runner defect, kept apart from
/// files that failed their own load stages.
#[test]
#[expect(
    clippy::panic_in_result_fn,
    reason = "the Book's Result-test shape: assertions panic, plumbing propagates with `?`"
)]
fn an_empty_root_loads_as_an_empty_catalogue() -> Fallible {
    let dir = assert_fs::TempDir::new()?;
    let loaded = veredictum::pipeline::load_clean_root(dir.path())?;
    assert!(
        loaded.set.cases.is_empty() && loaded.set.bindings.is_empty(),
        "nothing is discovered under an empty root"
    );
    Ok(())
}

#[test]
#[expect(
    clippy::panic_in_result_fn,
    reason = "the Book's Result-test shape: assertions panic, plumbing propagates with `?`"
)]
fn the_json_reader_names_what_it_was_reading() -> Fallible {
    let dir = assert_fs::TempDir::new()?;
    let absent = dir.path().join("absent.json");
    let error = read_json::<serde_json::Value>(&absent, "topology")
        .expect_err("an absent file cannot be read");
    match &error {
        Error::Read { path, .. } => assert_eq!(path, &absent),
        other => panic!("expected a read failure, got {other}"),
    }
    assert!(error.to_string().starts_with("cannot read "), "{error}");

    let broken = dir.path().join("broken.json");
    std::fs::write(&broken, "[")?;
    let error = read_json::<serde_json::Value>(&broken, "topology")
        .expect_err("a truncated document does not parse");
    assert!(matches!(error, Error::Parse { .. }), "{error}");
    assert!(error.to_string().starts_with("topology: "), "{error}");
    Ok(())
}

/// Every artifact this instrument writes is pretty JSON with a trailing
/// newline, and the parent directory is created before the write.
#[test]
#[expect(
    clippy::panic_in_result_fn,
    reason = "the Book's Result-test shape: assertions panic, plumbing propagates with `?`"
)]
fn documents_are_pretty_json_written_under_a_created_parent() -> Fallible {
    let dir = assert_fs::TempDir::new()?;
    let path = dir.path().join("deep/nested/out.json");
    let body = to_json_document(&serde_json::json!({ "a": [1, 2] }), "test")?;
    assert_eq!(body, "{\n  \"a\": [\n    1,\n    2\n  ]\n}\n");

    veredictum::pipeline::ensure_parent_dir(&path)?;
    veredictum::pipeline::write_file(&path, &body)?;
    assert_eq!(std::fs::read_to_string(&path)?, body);

    // A path with no parent to create is not a failure.
    veredictum::pipeline::ensure_parent_dir(Path::new("bare.json"))?;

    let error = veredictum::pipeline::write_file(&dir.path().join("no/such/dir.json"), &body)
        .expect_err("writing into an absent directory fails");
    assert!(matches!(error, Error::Write { .. }), "{error}");
    Ok(())
}

/// File references inside an ixit document resolve against the DOCUMENT, not
/// the working directory, and the raw text travels with it because the
/// campaign's digest is taken over exactly those bytes.
#[test]
#[expect(
    clippy::panic_in_result_fn,
    reason = "the Book's Result-test shape: assertions panic, plumbing propagates with `?`"
)]
fn the_ixit_reader_rebases_file_references_and_keeps_the_bytes() -> Fallible {
    let dir = assert_fs::TempDir::new()?;
    let path = dir.path().join("topology/ixit.json");
    std::fs::create_dir_all(path.parent().ok_or("the fixture path has a parent")?)?;
    let text = serde_json::to_string_pretty(&serde_json::json!({
        "instances": {
            "sut": { "base_url": "http://127.0.0.1:8080/openehr/v1", "auth": { "mode": "none" } },
            "smart_platform": { "base_url": "http://127.0.0.1:8080", "auth": { "mode": "none" } }
        },
        "smart": {
            "platform_instance": "smart_platform",
            "mint": {
                "issuer": "https://as.example.test",
                "subject": "cnf-smart-app",
                "key_file": "keys/test.key.pem",
                "kid": "k1",
                "ttl_seconds": 300
            }
        }
    }))?;
    std::fs::write(&path, &text)?;

    let (ixit, kept) = veredictum::pipeline::load_ixit(&path)?;
    assert_eq!(kept, text, "the digest is taken over the file's own bytes");
    let key = ixit
        .smart
        .as_ref()
        .ok_or("the fixture declares a SMART lane")?
        .mint
        .key_file
        .clone();
    assert_eq!(
        key,
        dir.path().join("topology/keys/test.key.pem"),
        "the key reference is rebased onto the document's directory, not the working directory"
    );

    // A document that is not a topology is a named parse failure.
    let broken = dir.path().join("not-ixit.json");
    std::fs::write(&broken, r#"{"instances": 7}"#)?;
    let error = veredictum::pipeline::load_ixit(&broken).expect_err("7 is not an instance map");
    assert!(matches!(error, Error::Parse { .. }), "{error}");
    assert!(error.to_string().starts_with("ixit: "), "{error}");
    Ok(())
}

// ── the measured seam's selectors ──────────────────────────────────────────

/// Only the ladder's own rungs are windows, and a window is a whole number of
/// hours in seconds.
#[test]
fn only_a_ladder_rung_is_a_sustained_window() {
    for hours in measured::SustainedWindow::LADDER {
        let window = measured::SustainedWindow::hours(*hours)
            .unwrap_or_else(|| panic!("{hours} is a rung of the ladder"));
        assert_eq!(window.seconds(), hours * 3600);
    }
    assert!(
        measured::SustainedWindow::hours(3).is_none(),
        "3 hours is not a rung, and nothing rounds it to one"
    );
    assert!(measured::SustainedWindow::hours(0).is_none());
    assert_eq!(
        measured::SustainedWindow::default().seconds(),
        3600,
        "the default window is the shortest rung"
    );
}

/// The class selector resolves against the catalogue's own performance cases,
/// and an unstocked class is a named miss rather than a silent empty run.
#[test]
#[expect(
    clippy::panic_in_result_fn,
    reason = "the Book's Result-test shape: assertions panic, plumbing propagates with `?`"
)]
fn a_class_selects_its_own_performance_case() -> Fallible {
    let loaded = veredictum::pipeline::load_clean_root(&artifacts())?;
    let class = veredictum::perf::PerfClass::parse("POC").map_err(Error::Selector)?;
    let (path, case) = measured::performance_case_of_class(&loaded, class, "POC")?;
    assert_eq!(case.class, class);
    assert!(
        path.ends_with("class_POC.yaml") || path.to_string_lossy().contains("POC"),
        "the case comes back with the artifact it was loaded from: {}",
        path.display()
    );

    let empty = veredictum::artifacts::Loaded::default();
    let error = measured::performance_case_of_class(&empty, class, "POC")
        .expect_err("an empty catalogue stocks no class");
    assert!(matches!(error, Error::Missing(_)), "{error}");
    assert!(error.to_string().contains("class POC"), "{error}");
    Ok(())
}

/// The journey context and the scale OPT come from the committed corpus, and
/// a tree without one says which piece is missing instead of measuring
/// nothing.
#[test]
#[expect(
    clippy::panic_in_result_fn,
    reason = "the Book's Result-test shape: assertions panic, plumbing propagates with `?`"
)]
fn the_measured_preamble_reads_its_inputs_from_the_committed_corpus() -> Fallible {
    let loaded = veredictum::pipeline::load_clean_root(&artifacts())?;
    let opt = measured::scale_opt_xml(&loaded)?;
    assert!(
        opt.contains("<template"),
        "the scale corpus commits against an OPT document"
    );
    let (journeys, pack) = measured::journey_context(&loaded)?;
    assert!(!journeys.0.is_empty(), "the catalogue names journeys");
    assert!(
        !pack.templates.is_empty(),
        "the pack carries a template per journey stage"
    );

    let empty = veredictum::artifacts::Loaded::default();
    let error = measured::scale_opt_xml(&empty).expect_err("no corpus, no OPT");
    assert!(matches!(error, Error::Missing(_)), "{error}");
    let error = measured::journey_context(&empty).expect_err("no catalogue, no journeys");
    assert!(
        error.to_string().contains("journey_catalogue.yaml"),
        "{error}"
    );
    Ok(())
}
