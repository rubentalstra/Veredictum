// SPDX-FileCopyrightText: Veredictum contributors
// SPDX-License-Identifier: Apache-2.0

//! The #166 acceptance gates over the benchmark surface.
//!
//! The console mirrors the bench-result document because its engine pin
//! predates the bench module, so the first gate here holds the fixtures to the
//! PUBLISHED schema (`schemas/bench-result.schema.json`) and the reader to the
//! fixtures. A mirror that drifted from the artifact family would otherwise
//! fail only against a record nobody in this repository has.
//!
//! The fixtures are raw JSON on purpose: the pinned engine cannot construct a
//! bench value at all, so raw bytes are the only authorable form, and they
//! hold the reader to the published document rather than to a value this crate
//! serialized itself.

use std::path::{Path, PathBuf};

use veredictum_console::bench_api::read::{compare, listing, screen};
use veredictum_console::bench_api::{BenchScreen, CompareScreen};

/// The repository root, two levels above this crate.
fn repo_root() -> &'static Path {
    Path::new(concat!(env!("CARGO_MANIFEST_DIR"), "/../.."))
}

/// The committed fixture directory.
fn fixtures() -> PathBuf {
    PathBuf::from(concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/bench"))
}

/// A console state whose output mount carries the two fixture records.
///
/// The catalogue is deliberately absent: the benchmark surface reads records
/// out of the output mount and nothing else, and a gate that needed the
/// catalogue would be testing the wrong seam.
fn state_over_fixtures(
    out: &Path,
) -> Result<veredictum_console::state::ConsoleState, std::io::Error> {
    for name in ["bench-result-alpha.json", "bench-result-beta.json"] {
        std::fs::copy(fixtures().join(name), out.join(name))?;
    }
    Ok(veredictum_console::state::ConsoleState {
        root: repo_root().join("artifacts"),
        specs: repo_root().join("specs/openehr"),
        party: repo_root().join("party"),
        out: out.to_path_buf(),
        catalogue: std::sync::Arc::new(Err(String::from("not read by this surface"))),
        draft: std::sync::Arc::new(std::sync::Mutex::new(
            veredictum_console::run_api::Drafts::new(),
        )),
        client_ip_header: None,
        sign_key: None,
        verify_key: None,
        jobs: veredictum_console::run_job::JobSlot::default(),
        posture: veredictum_console::posture::Posture::Local,
        rates: veredictum_console::rate_limit::RateLimiter::default(),
        capture: false,
    })
}

#[expect(
    clippy::panic_in_result_fn,
    reason = "the Book ch11 Result-returning test shape: assertions panic, plumbing propagates with ? (https://doc.rust-lang.org/book/ch11-01-writing-tests.html)"
)]
#[expect(
    clippy::disallowed_types,
    reason = "the emitted-schemas family: a JSON Schema and the document it judges have no typed model on either side of the validator"
)]
#[test]
fn the_fixtures_are_held_to_the_published_bench_result_schema()
-> Result<(), Box<dyn std::error::Error>> {
    let schema: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(
        repo_root().join("schemas/bench-result.schema.json"),
    )?)?;
    let validator = jsonschema::validator_for(&schema)?;
    for name in ["bench-result-alpha.json", "bench-result-beta.json"] {
        let document: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(fixtures().join(name))?)?;
        let findings: Vec<String> = validator
            .iter_errors(&document)
            .map(|error| format!("{}: {error}", error.instance_path()))
            .collect();
        assert!(
            findings.is_empty(),
            "{name} is not a published bench result:\n  {}",
            findings.join("\n  ")
        );
    }
    Ok(())
}

#[expect(
    clippy::panic_in_result_fn,
    reason = "the Book ch11 Result-returning test shape: assertions panic, plumbing propagates with ?"
)]
#[test]
fn the_listing_reads_every_mounted_record() -> Result<(), Box<dyn std::error::Error>> {
    let out = assert_fs::TempDir::new()?;
    let state = state_over_fixtures(out.path())?;
    let listed = listing(&state);
    assert!(listed.unreadable.is_empty(), "{:?}", listed.unreadable);
    assert_eq!(listed.records.len(), 2);
    // The boundary statement comes out of the records, verbatim and deduped:
    // both fixtures carry the same sentence, so the surface renders one.
    assert_eq!(listed.boundary_statements.len(), 1);
    assert!(
        listed
            .boundary_statements
            .first()
            .is_some_and(|line| line.contains("not a conformance record")),
        "{:?}",
        listed.boundary_statements
    );
    // Newest first, so the record an operator just wrote is the top row.
    let labels: Vec<&str> = listed
        .records
        .iter()
        .map(|record| record.label.as_str())
        .collect();
    assert_eq!(labels, vec!["Beta CDR 2.0", "Alpha CDR 3.1"]);
    let beta = listed.records.first().ok_or("the listing lost a record")?;
    assert!(!beta.submittable);
    assert_eq!(beta.unmet, vec!["repetitions", "baseline"]);
    Ok(())
}

#[expect(
    clippy::panic_in_result_fn,
    reason = "the Book ch11 Result-returning test shape: assertions panic, plumbing propagates with ?"
)]
#[test]
fn one_record_opens_with_every_figure_labelled() -> Result<(), Box<dyn std::error::Error>> {
    let out = assert_fs::TempDir::new()?;
    let state = state_over_fixtures(out.path())?;
    let listed = listing(&state);
    let alpha = listed
        .records
        .iter()
        .find(|record| record.label == "Alpha CDR 3.1")
        .ok_or("the alpha record must be listed")?;
    let BenchScreen::Record(detail) = screen(&state, Some(&alpha.key)) else {
        panic!("an address the listing carries must open");
    };

    // The boundary statement is furniture on every surface, verbatim.
    assert!(
        detail
            .boundary_statement
            .contains("never substitute for one"),
        "{}",
        detail.boundary_statement
    );
    assert_eq!(detail.pack, "console-fixture@1.0.0");
    assert!(detail.submittable, "{:?}", detail.unmet);
    assert!(detail.unmet.is_empty());

    // Every figure carries the discipline that produced it: the seed phase is
    // closed-loop, the measured phase is open-loop, and neither is inferred
    // from a name.
    assert_eq!(
        detail.seed_phases.first().map(|seed| seed.regime.as_str()),
        Some("closed-loop")
    );
    let phase = detail
        .phases
        .first()
        .ok_or("the record carries one phase")?;
    assert_eq!(phase.phase, "mixed");
    assert_eq!(phase.regime, "open-loop");
    assert_eq!(phase.rows.len(), 2);

    // The posture block labels each item verified or declared-only exactly as
    // the record wrote it — never a console recomputation.
    assert_eq!(detail.posture.len(), 7);
    let declared_only: Vec<&str> = detail
        .posture
        .iter()
        .filter(|line| !line.verified)
        .map(|line| line.item.as_str())
        .collect();
    assert_eq!(declared_only, vec!["audit", "tenancy"]);

    // The failed-arrival readings cover the target and the baseline, and none
    // of them breaches the pack's ceiling on a clean record.
    assert_eq!(
        detail.failed_shares.len(),
        6,
        "one reading per repetition and phase, target first then the baseline"
    );
    assert!(detail.failed_shares.iter().all(|row| !row.breaches));
    assert!(
        detail
            .failed_shares
            .iter()
            .any(|row| row.side == "the ehrbase baseline")
    );

    // The relative index is read out of the record as derived, with its own
    // derivation sentence attached.
    let relative = detail.relative.first().ok_or("one relative table")?;
    assert_eq!(relative.baseline, "ehrbase");
    assert!(relative.derivation.contains("dimensionless"));
    assert!(relative.gaps.is_empty());
    let p50 = relative
        .rows
        .iter()
        .find(|row| row.operation == "get_ehr" && row.metric == "p50_us")
        .ok_or("the index carries get_ehr at p50")?;
    assert!(
        (p50.index - (p50.target_median / p50.baseline_median)).abs() < 1e-12,
        "{p50:?}"
    );
    assert_eq!(detail.baselines.len(), 1);
    Ok(())
}

#[expect(
    clippy::panic_in_result_fn,
    reason = "the Book ch11 Result-returning test shape: assertions panic, plumbing propagates with ?"
)]
#[test]
fn a_comparison_states_every_mismatch() -> Result<(), Box<dyn std::error::Error>> {
    let out = assert_fs::TempDir::new()?;
    let state = state_over_fixtures(out.path())?;
    let listed = listing(&state);
    let selection = listed
        .records
        .iter()
        .map(|record| record.key.clone())
        .collect::<Vec<_>>()
        .join(",");
    let CompareScreen::Aligned(aligned) = compare(&state, &selection) else {
        panic!("two addresses the listing carries must align");
    };
    assert_eq!(aligned.columns.len(), 2);
    // Every column names its own machine and its own posture, because an
    // absolute number without its machine is unreadable.
    assert!(
        aligned
            .columns
            .iter()
            .all(|column| column.machine.contains("arch=") && !column.posture_signature.is_empty())
    );
    let warnings = aligned.warnings.join("\n");
    for expected in [
        "DIFFERENT hosts",
        "DIFFERENT postures",
        "DIFFERENT scale factors",
        "not submittable",
        "off the pack's pinned configuration",
        "NO relative index",
    ] {
        assert!(
            warnings.contains(expected),
            "the comparison must warn about {expected:?}:\n{warnings}"
        );
    }
    // The two records ran the same pack, so that warning must NOT fire: a
    // warning list that always fires is a list nobody reads.
    assert!(!warnings.contains("DIFFERENT packs"), "{warnings}");

    // Every row carries its discipline, and one row exists per phase,
    // operation and metric.
    assert_eq!(aligned.rows.len(), 2 * 6, "two operations × six metrics");
    assert!(aligned.rows.iter().all(|row| row.regime == "open-loop"));
    assert!(
        aligned
            .rows
            .iter()
            .all(|row| row.cells.len() == aligned.columns.len())
    );
    assert_eq!(aligned.boundary_statements.len(), 1);
    Ok(())
}

#[expect(
    clippy::panic_in_result_fn,
    reason = "the Book ch11 Result-returning test shape: assertions panic, plumbing propagates with ?"
)]
#[test]
fn a_selection_of_one_is_not_a_comparison() -> Result<(), Box<dyn std::error::Error>> {
    let out = assert_fs::TempDir::new()?;
    let state = state_over_fixtures(out.path())?;
    let listed = listing(&state);
    let one = listed
        .records
        .first()
        .ok_or("the listing carries a record")?;
    assert_eq!(
        compare(&state, &one.key),
        CompareScreen::NeedsMore { selected: 1 }
    );
    assert_eq!(compare(&state, ""), CompareScreen::Idle);
    let CompareScreen::Unknown { reason } = compare(&state, "0123456789abcdef,fedcba9876543210")
    else {
        panic!("an address nothing carries must be an answer, never a silent empty table");
    };
    assert!(reason.contains("no longer here"), "{reason}");
    Ok(())
}

#[expect(
    clippy::panic_in_result_fn,
    reason = "the Book ch11 Result-returning test shape: assertions panic, plumbing propagates with ?"
)]
#[test]
fn an_unknown_vocabulary_token_is_refused_rather_than_defaulted()
-> Result<(), Box<dyn std::error::Error>> {
    let out = assert_fs::TempDir::new()?;
    let state = state_over_fixtures(out.path())?;
    // One token outside the closed posture vocabulary. A reader that silently
    // defaulted it would show a posture the run never had.
    let path = out.path().join("bench-result-typo.json");
    let body = std::fs::read_to_string(fixtures().join("bench-result-beta.json"))?
        .replace("\"declared-only\"", "\"declared-onlyy\"");
    std::fs::write(&path, body)?;
    let listed = listing(&state);
    assert_eq!(
        listed.records.len(),
        2,
        "the two well-formed records still list"
    );
    assert_eq!(listed.unreadable.len(), 1, "{:?}", listed.unreadable);
    assert!(
        listed
            .unreadable
            .first()
            .is_some_and(|line| line.contains("declared-onlyy")),
        "the refusal must name the token it refused: {:?}",
        listed.unreadable
    );
    Ok(())
}

#[expect(
    clippy::panic_in_result_fn,
    reason = "the Book ch11 Result-returning test shape: assertions panic, plumbing propagates with ?"
)]
#[test]
fn an_uploaded_batch_lands_in_the_listing_and_is_marked_transient()
-> Result<(), Box<dyn std::error::Error>> {
    let out = assert_fs::TempDir::new()?;
    // A bare output mount: what the upload adds is the whole assertion.
    let state = veredictum_console::state::ConsoleState {
        root: repo_root().join("artifacts"),
        specs: repo_root().join("specs/openehr"),
        party: repo_root().join("party"),
        out: out.path().to_path_buf(),
        catalogue: std::sync::Arc::new(Err(String::from("not read by this surface"))),
        draft: std::sync::Arc::new(std::sync::Mutex::new(
            veredictum_console::run_api::Drafts::new(),
        )),
        client_ip_header: None,
        sign_key: None,
        verify_key: None,
        jobs: veredictum_console::run_job::JobSlot::default(),
        posture: veredictum_console::posture::Posture::Local,
        rates: veredictum_console::rate_limit::RateLimiter::default(),
        capture: false,
    };
    assert!(listing(&state).records.is_empty());

    let alpha = std::fs::read(fixtures().join("bench-result-alpha.json"))?;
    let written = veredictum_console::bench_api::upload::batch(
        &state,
        &[(String::from("../../escape.json"), alpha)],
    )?;
    assert_eq!(written, 1);
    let listed = listing(&state);
    assert_eq!(listed.records.len(), 1);
    let record = listed.records.first().ok_or("the uploaded record lists")?;
    assert_eq!(
        record.source,
        veredictum_console::bench_api::BenchSource::Uploaded,
        "an uploaded record is labelled transient, never as a mounted one"
    );
    // The name was REBUILT, so the traversal never reached the filesystem.
    assert!(!record.file.contains(".."), "{}", record.file);
    assert!(record.file.starts_with("bench-result-"), "{}", record.file);
    assert_eq!(record.label, "Alpha CDR 3.1");

    // A record with no file is a refusal, never an empty batch.
    assert!(veredictum_console::bench_api::upload::batch(&state, &[]).is_err());
    Ok(())
}

/// A bare output mount, with no catalogue: nothing the bench surfaces read
/// touches one.
fn bare_state(out: &Path) -> veredictum_console::state::ConsoleState {
    veredictum_console::state::ConsoleState {
        root: repo_root().join("artifacts"),
        specs: repo_root().join("specs/openehr"),
        party: repo_root().join("party"),
        out: out.to_path_buf(),
        catalogue: std::sync::Arc::new(Err(String::from("not read by this surface"))),
        draft: std::sync::Arc::new(std::sync::Mutex::new(
            veredictum_console::run_api::Drafts::new(),
        )),
        client_ip_header: None,
        sign_key: None,
        verify_key: None,
        jobs: veredictum_console::run_job::JobSlot::default(),
        posture: veredictum_console::posture::Posture::Local,
        rates: veredictum_console::rate_limit::RateLimiter::default(),
        capture: false,
    }
}

/// Every cap the upload enforces is enforced BEFORE anything is written, and
/// a refused batch leaves the output mount exactly as it found it.
///
/// The endpoint takes a document from an anonymous stranger — the console has
/// no login by design — so each refusal is a boundary rather than a nicety.
#[expect(
    clippy::panic_in_result_fn,
    reason = "the Book ch11 Result-returning test shape: assertions panic, plumbing propagates with ?"
)]
#[test]
fn a_refused_batch_leaves_the_output_mount_untouched() -> Result<(), Box<dyn std::error::Error>> {
    use veredictum_console::bench_api::upload::{
        MAX_BATCH_BYTES, MAX_RECORD_BYTES, MAX_RECORDS, batch,
    };

    let out = assert_fs::TempDir::new()?;
    let state = bare_state(out.path());
    let alpha = std::fs::read(fixtures().join("bench-result-alpha.json"))?;

    let scratched = |root: &Path| -> Result<usize, std::io::Error> {
        let mut count = 0;
        for entry in std::fs::read_dir(root)? {
            if entry?
                .file_name()
                .to_string_lossy()
                .starts_with(veredictum_console::bench_api::scan::SCRATCH_PREFIX)
            {
                count += 1;
            }
        }
        Ok(count)
    };

    // More records than one batch accepts: refused on the count.
    let many: Vec<(String, Vec<u8>)> = (0..=MAX_RECORDS)
        .map(|index| (format!("r-{index}.json"), alpha.clone()))
        .collect();
    let refusal = batch(&state, &many).expect_err("a batch over the record cap is refused");
    assert!(refusal.contains(&MAX_RECORDS.to_string()), "{refusal}");

    // One record over the per-record cap: refused naming that record.
    let oversized = usize::try_from(MAX_RECORD_BYTES)
        .unwrap_or(usize::MAX)
        .saturating_add(1);
    let refusal = batch(
        &state,
        &[
            (String::from("small.json"), alpha.clone()),
            (String::from("huge.json"), vec![b'0'; oversized]),
        ],
    )
    .expect_err("a record over the per-record cap is refused");
    assert!(refusal.contains("huge.json"), "{refusal}");
    assert_eq!(
        scratched(out.path())?,
        0,
        "a refused batch takes back what it had already written"
    );

    // Records each inside the per-record cap that together exceed the batch
    // cap: refused on the total, which is the bomb rule.
    let each = usize::try_from(MAX_RECORD_BYTES).unwrap_or(usize::MAX);
    let heavy: Vec<(String, Vec<u8>)> = (0..MAX_RECORDS)
        .map(|index| (format!("part-{index}.json"), vec![b'0'; each]))
        .collect();
    let refusal = batch(&state, &heavy).expect_err("a batch over the total cap is refused");
    assert!(refusal.contains(&MAX_BATCH_BYTES.to_string()), "{refusal}");
    assert_eq!(scratched(out.path())?, 0);
    assert!(
        listing(&state).records.is_empty(),
        "no refused record ever reached the listing"
    );
    Ok(())
}

/// The comparison answers every selection shape without reading a record it
/// cannot resolve: one address needs a second, too many overflow the columns,
/// and an address naming nothing says the record went away.
#[expect(
    clippy::panic_in_result_fn,
    reason = "the Book ch11 Result-returning test shape: assertions panic, plumbing propagates with ?"
)]
#[test]
fn the_comparison_states_why_a_selection_compares_nothing() -> Result<(), Box<dyn std::error::Error>>
{
    let out = assert_fs::TempDir::new()?;
    let state = state_over_fixtures(out.path())?;

    assert_eq!(compare(&state, ""), CompareScreen::Idle);
    assert_eq!(
        compare(&state, " , , "),
        CompareScreen::Idle,
        "a selection of nothing but separators selects nothing"
    );

    let listed = listing(&state);
    let first = listed
        .records
        .first()
        .ok_or("the fixture mount lists records")?;
    assert_eq!(
        compare(&state, &first.key),
        CompareScreen::NeedsMore { selected: 1 },
        "one record compares with nothing"
    );

    let overflowing = (0..12)
        .map(|_| first.key.clone())
        .collect::<Vec<_>>()
        .join(",");
    let CompareScreen::Unknown { reason } = compare(&state, &overflowing) else {
        panic!("a selection past the column bound is refused with its reason");
    };
    assert!(reason.contains("records are selected"), "{reason}");

    let CompareScreen::Unknown { reason } =
        compare(&state, &format!("{},no-such-record", first.key))
    else {
        panic!("an address naming nothing is refused with its reason");
    };
    assert!(reason.contains("no longer here"), "{reason}");
    Ok(())
}
