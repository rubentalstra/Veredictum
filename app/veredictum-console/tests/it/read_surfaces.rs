// SPDX-FileCopyrightText: Veredictum contributors
// SPDX-License-Identifier: Apache-2.0

//! The #64 acceptance gates: the landing's counts are the validate summary's
//! own numbers, checked rather than eyeballed, and the catalogue-missing
//! state names the mounts it looked at.
//!
//! The chapter, band and case-detail readers are pinned here too: they are
//! the whole of what S2 renders, they are component-free by construction, and
//! the committed catalogue is the only input they take.

use std::path::Path;

use veredictum_console::catalogue_api::{InstrumentView, read};
use veredictum_console::state::ConsoleState;

/// The repository root, two levels above this crate (#55).
fn repo_root() -> &'static Path {
    Path::new(concat!(env!("CARGO_MANIFEST_DIR"), "/../.."))
}

/// The startup state over the committed catalogue, exactly as `main` builds
/// it — the env override keeps the test independent of the process cwd.
fn committed_state() -> ConsoleState {
    let root = repo_root().join("artifacts");
    let specs = repo_root().join("specs/openehr");
    let catalogue = veredictum::pipeline::catalogue::validate_tree(&root, Some(&specs))
        .map_err(|e| e.to_string());
    ConsoleState {
        root,
        specs,
        out: repo_root().join("out"),
        catalogue: std::sync::Arc::new(catalogue),
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

#[expect(
    clippy::panic_in_result_fn,
    reason = "the Book ch11 Result-returning test shape: assertions panic, plumbing propagates with ? (https://doc.rust-lang.org/book/ch11-01-writing-tests.html)"
)]
#[test]
fn the_landing_counts_are_the_validate_summary_counts() -> Result<(), Box<dyn std::error::Error>> {
    let state = committed_state();
    let validation = state
        .catalogue
        .as_ref()
        .as_ref()
        .map_err(|e| format!("the committed catalogue must load: {e}"))?;

    // The same expressions the CLI's summary line prints
    // (app/veredictum/src/bin/veredictum.rs) — the mapping this test holds.
    let expected = (
        u64::try_from(validation.loaded.set.cases.len())?,
        u64::try_from(validation.loaded.set.bindings.len())?,
        u64::try_from(
            validation
                .loaded
                .set
                .matrix
                .as_ref()
                .map_or(0, |(_, matrix)| matrix.entries().len()),
        )?,
        u64::try_from(validation.findings.len())?,
    );

    match read::instrument_view(&state) {
        InstrumentView::Loaded(summary) => {
            assert_eq!(
                (
                    summary.cases,
                    summary.bindings,
                    summary.capabilities,
                    summary.findings
                ),
                expected,
                "the landing shows numbers the validate summary would not print"
            );
            // Catalogue cleanliness is gated by CI's validate job over the
            // WORKSPACE engine, not here: this crate validates through its
            // pinned PUBLISHED engine, so a catalogue legitimately using
            // machinery newer than the newest published crate reads findings
            // here mid-cycle (the release-cut window scripts/ui-e2e.sh also
            // models). The landing stays honest either way: it shows exactly
            // what the pinned engine computes, which the mapping assert pins.
        }
        InstrumentView::Missing(missing) => {
            panic!(
                "the committed catalogue read as missing: {}",
                missing.reason
            );
        }
    }
    Ok(())
}

/// A state whose catalogue refused to load, carrying `reason` verbatim.
fn refused_state(reason: &str) -> ConsoleState {
    ConsoleState {
        root: "/nonexistent/artifacts".into(),
        specs: "/nonexistent/specs".into(),
        out: "/nonexistent/out".into(),
        catalogue: std::sync::Arc::new(Err(reason.to_owned())),
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

#[test]
fn a_missing_catalogue_names_the_mounts_it_looked_at() {
    let state = refused_state("no such directory");
    match read::instrument_view(&state) {
        InstrumentView::Missing(missing) => {
            assert_eq!(missing.root, "/nonexistent/artifacts");
            assert_eq!(missing.specs, "/nonexistent/specs");
            assert_eq!(missing.reason, "no such directory");
        }
        InstrumentView::Loaded(_) => {
            panic!("a failed load must render as the missing state")
        }
    }
}

/// The chapter is the directory directly under `schedule/`, and a path that
/// does not run through `schedule/` names no chapter at all rather than
/// guessing one from the last component it saw.
#[test]
fn the_chapter_is_the_directory_under_schedule() {
    assert_eq!(
        read::chapter_of(Path::new("/a/b/artifacts/schedule/ehr/case.yaml")),
        "ehr"
    );
    assert_eq!(
        read::chapter_of(Path::new("artifacts/schedule/query/case.yaml")),
        "query"
    );
    assert_eq!(read::chapter_of(Path::new("artifacts/bindings/x.yaml")), "");
    assert_eq!(read::chapter_of(Path::new("schedule")), "");
}

/// Every loaded case lands in exactly one chapter row, so the chapter counts
/// add up to the landing's own case count and none is double-counted.
#[expect(
    clippy::panic_in_result_fn,
    reason = "the Book ch11 Result-returning test shape: assertions panic, plumbing propagates with ? (https://doc.rust-lang.org/book/ch11-01-writing-tests.html)"
)]
#[test]
fn the_chapter_counts_partition_the_catalogue() -> Result<(), Box<dyn std::error::Error>> {
    let state = committed_state();
    let rows = read::chapter_rows(&state)?;
    assert!(!rows.is_empty(), "the committed catalogue has chapters");

    let mut keys: Vec<&str> = rows.iter().map(|row| row.key.as_str()).collect();
    let sorted = {
        let mut copy = keys.clone();
        copy.sort_unstable();
        copy
    };
    assert_eq!(keys, sorted, "the chapter list is key-sorted");
    keys.dedup();
    assert_eq!(keys.len(), rows.len(), "a chapter appears once");

    let validation = state
        .catalogue
        .as_ref()
        .as_ref()
        .map_err(|e| format!("the committed catalogue must load: {e}"))?;
    let total: u64 = rows.iter().map(|row| row.cases).sum();
    assert_eq!(
        total,
        u64::try_from(validation.loaded.set.cases.len())?,
        "the chapter counts must partition the loaded case set"
    );
    assert!(
        rows.iter().any(|row| row.key == "ehr"),
        "the ehr chapter is committed: {keys:?}"
    );
    Ok(())
}

/// A catalogue that did not load is the SAME verbatim refusal on every
/// reader, never an empty listing that reads as "the catalogue is fine and
/// carries nothing".
#[test]
fn every_reader_reports_the_load_refusal_verbatim() {
    let state = refused_state("artifacts/schedule: no such directory");
    assert_eq!(
        read::chapter_rows(&state).err().as_deref(),
        Some("artifacts/schedule: no such directory")
    );
    assert_eq!(
        read::band_rows(&state, "ehr", "", "").err().as_deref(),
        Some("artifacts/schedule: no such directory")
    );
    assert_eq!(
        read::case_detail(&state, "any").err().as_deref(),
        Some("artifacts/schedule: no such directory")
    );
}

/// The band listing is the published SVG's own taxonomy, id-sorted within a
/// band, and every row carries the band it was filed under.
#[expect(
    clippy::panic_in_result_fn,
    reason = "the Book ch11 Result-returning test shape: assertions panic, plumbing propagates with ? (https://doc.rust-lang.org/book/ch11-01-writing-tests.html)"
)]
#[test]
fn a_chapter_listing_is_banded_and_id_sorted() -> Result<(), Box<dyn std::error::Error>> {
    let state = committed_state();
    let bands = read::band_rows(&state, "ehr", "", "")?;
    assert!(!bands.is_empty(), "the ehr chapter carries cases");
    for band in &bands {
        assert!(!band.cases.is_empty(), "a rendered band carries rows");
        let ids: Vec<&str> = band.cases.iter().map(|case| case.id.as_str()).collect();
        let sorted = {
            let mut copy = ids.clone();
            copy.sort_unstable();
            copy
        };
        assert_eq!(ids, sorted, "band {} is not id-sorted", band.band);
        for case in &band.cases {
            assert_eq!(
                case.band, band.band,
                "a row must carry the band it was filed under"
            );
            assert!(!case.purpose.is_empty(), "{} has no test purpose", case.id);
        }
    }
    // A chapter the schedule does not carry is an empty listing, not an error:
    // the screen renders "nothing here", and the catalogue is intact.
    assert!(read::band_rows(&state, "no-such-chapter", "", "")?.is_empty());
    Ok(())
}

/// The two filters narrow rather than reshape: the id substring and the tier
/// token each keep a subset of the unfiltered listing, and a token no case
/// carries keeps nothing.
#[expect(
    clippy::panic_in_result_fn,
    reason = "the Book ch11 Result-returning test shape: assertions panic, plumbing propagates with ? (https://doc.rust-lang.org/book/ch11-01-writing-tests.html)"
)]
#[test]
fn the_listing_filters_narrow_the_same_rows() -> Result<(), Box<dyn std::error::Error>> {
    let state = committed_state();
    let all: Vec<String> = read::band_rows(&state, "ehr", "", "")?
        .into_iter()
        .flat_map(|band| band.cases)
        .map(|case| case.id)
        .collect();

    let by_query: Vec<String> = read::band_rows(&state, "ehr", "create_ehr", "")?
        .into_iter()
        .flat_map(|band| band.cases)
        .map(|case| case.id)
        .collect();
    assert!(!by_query.is_empty(), "create_ehr cases are committed");
    assert!(by_query.len() < all.len(), "the substring must narrow");
    assert!(
        by_query.iter().all(|id| id.contains("create_ehr")),
        "the substring filter kept a row that does not carry it"
    );

    let core: Vec<String> = read::band_rows(&state, "ehr", "", "CORE")?
        .into_iter()
        .flat_map(|band| band.cases)
        .map(|case| case.id)
        .collect();
    assert!(!core.is_empty(), "CORE cases are committed in ehr");
    assert!(
        core.iter().all(|id| all.contains(id)),
        "the tier filter must keep a subset of the unfiltered listing"
    );
    assert!(
        read::band_rows(&state, "ehr", "", "NO-SUCH-TIER")?
            .into_iter()
            .all(|band| band.cases.is_empty()),
        "a tier token no case carries keeps nothing"
    );
    Ok(())
}

/// The case detail is the case core's own fields, and every binding it names
/// realizes the case's SM operation.
#[expect(
    clippy::panic_in_result_fn,
    reason = "the Book ch11 Result-returning test shape: assertions panic, plumbing propagates with ? (https://doc.rust-lang.org/book/ch11-01-writing-tests.html)"
)]
#[test]
fn a_case_detail_is_the_case_core_and_its_bindings() -> Result<(), Box<dyn std::error::Error>> {
    let state = committed_state();
    let id = "I_EHR_SERVICE.create_ehr-clone_system_id";
    let detail =
        read::case_detail(&state, id)?.ok_or("the committed catalogue carries the case")?;

    assert_eq!(detail.id, id);
    assert_eq!(detail.chapter, "ehr");
    assert_eq!(detail.kind, "functional");
    assert_eq!(
        detail.sm_operation.as_deref(),
        Some("I_EHR_SERVICE.create_ehr")
    );
    assert!(!detail.test_purpose.is_empty());
    assert!(
        !detail.spec_refs.is_empty(),
        "every expectation cites the section it comes from"
    );
    assert!(
        detail.tiers.contains(&String::from("CORE")),
        "{:?}",
        detail.tiers
    );
    assert!(
        !detail.bindings.is_empty(),
        "the operation is realized by at least one binding"
    );
    assert!(
        detail.bindings.iter().any(|binding| binding.realized),
        "a realized binding exists for a driven operation"
    );
    assert!(
        detail.size.ends_with("flow step(s)"),
        "a functional case is sized in flow steps: {}",
        detail.size
    );

    let mut sorted = detail.corpus_keys.clone();
    sorted.sort();
    sorted.dedup();
    assert_eq!(
        detail.corpus_keys, sorted,
        "the corpus keys are sorted and deduplicated"
    );
    Ok(())
}

/// A content case is sized in decision-table rows, which is the other arm of
/// the size derivation.
#[expect(
    clippy::panic_in_result_fn,
    reason = "the Book ch11 Result-returning test shape: assertions panic, plumbing propagates with ? (https://doc.rust-lang.org/book/ch11-01-writing-tests.html)"
)]
#[test]
fn a_content_case_is_sized_in_decision_table_rows() -> Result<(), Box<dyn std::error::Error>> {
    let state = committed_state();
    let validation = state
        .catalogue
        .as_ref()
        .as_ref()
        .map_err(|e| format!("the committed catalogue must load: {e}"))?;
    let content = validation
        .loaded
        .set
        .cases
        .iter()
        .find(|(_, case)| case.decision_table.is_some())
        .map(|(_, case)| case.id.to_string())
        .ok_or("the committed catalogue carries a decision-table case")?;

    let detail = read::case_detail(&state, &content)?.ok_or("the case reads back by its own id")?;
    assert_eq!(detail.kind, "content");
    assert!(
        detail.size.ends_with("decision-table row(s)"),
        "a content case is sized in table rows: {}",
        detail.size
    );
    Ok(())
}

/// An id the catalogue does not carry is a legitimately absent page, never an
/// error the screen has to translate.
#[expect(
    clippy::panic_in_result_fn,
    reason = "the Book ch11 Result-returning test shape: assertions panic, plumbing propagates with ? (https://doc.rust-lang.org/book/ch11-01-writing-tests.html)"
)]
#[test]
fn an_unknown_case_id_is_absence_rather_than_failure() -> Result<(), Box<dyn std::error::Error>> {
    let state = committed_state();
    assert!(read::case_detail(&state, "I_NO_SERVICE.no_operation-none")?.is_none());
    assert!(read::case_detail(&state, "")?.is_none());
    Ok(())
}

/// The startup read takes every mount from the environment, and with none of
/// the variables set it falls back to the documented working-tree defaults —
/// never to an empty path, and never to a catalogue it silently invented.
///
/// A catalogue that is not there is a first-class state, so `load` still
/// answers: the landing renders the named-mount explanation rather than the
/// server refusing to start.
#[test]
fn the_startup_read_falls_back_to_the_documented_mounts() {
    use veredictum_console::posture::{POSTURE_ENV, Posture};
    use veredictum_console::state::{OUT_ENV, ROOT_ENV, SIGN_KEY_ENV, SPECS_ENV, VERIFY_KEY_ENV};

    // The variables are process-wide and `set_var` is unsafe, which this crate
    // forbids outright, so the unset case is read rather than arranged. An
    // ambient value is somebody's real console configuration: skip loudly
    // instead of asserting against it.
    let set: Vec<&str> = [
        ROOT_ENV,
        SPECS_ENV,
        OUT_ENV,
        SIGN_KEY_ENV,
        VERIFY_KEY_ENV,
        POSTURE_ENV,
    ]
    .into_iter()
    .filter(|name| std::env::var_os(name).is_some())
    .collect();
    if !set.is_empty() {
        eprintln!("SKIPPED({set:?} set in this environment): the defaults cannot be read");
        return;
    }

    let state = ConsoleState::load().expect("an unset posture is the local one, and it loads");
    assert_eq!(
        state.posture,
        Posture::Local,
        "an unset posture is the local one, which refuses no target"
    );
    assert_eq!(state.root, Path::new("artifacts"));
    assert_eq!(state.specs, Path::new("specs/openehr"));
    assert_eq!(state.out, Path::new("out"));
    assert_eq!(state.sign_key, None, "no signing posture without a mount");
    assert_eq!(state.verify_key, None);
    // A fresh console holds nothing for ANY submitter (#389): the job map and
    // the drafts map are both empty, and the question is asked per visitor
    // because that is the only shape either one has now.
    let who = veredictum_console::submitter::Submitter::Unknown;
    assert!(
        state
            .jobs
            .latest_of(who, veredictum_console::run_job::Latest::Any)
            .is_ok_and(|run| run.is_none()),
        "a fresh console has no job"
    );
    assert!(
        state
            .draft
            .lock()
            .is_ok_and(|drafts| drafts.get(who).is_none()),
        "a fresh console has no run draft"
    );
    assert_eq!(
        state.client_ip_header, None,
        "no forwarded header is trusted until the operator names one"
    );
    // The suite's own working directory carries no `artifacts` tree, so the
    // load refuses and the refusal is the state — not a panic, not a default.
    match read::instrument_view(&state) {
        InstrumentView::Missing(missing) => {
            assert_eq!(missing.root, "artifacts");
            assert!(!missing.reason.is_empty());
        }
        InstrumentView::Loaded(summary) => {
            assert_eq!(summary.root, "artifacts");
        }
    }
}
