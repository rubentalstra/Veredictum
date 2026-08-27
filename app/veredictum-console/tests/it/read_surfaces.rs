// SPDX-FileCopyrightText: Veredictum contributors
// SPDX-License-Identifier: Apache-2.0

//! The #64 acceptance gates: the landing's counts are the validate summary's
//! own numbers, checked rather than eyeballed, and the catalogue-missing
//! state names the mounts it looked at.

use std::path::Path;

/// The repository root, two levels above this crate (#55).
fn repo_root() -> &'static Path {
    Path::new(concat!(env!("CARGO_MANIFEST_DIR"), "/../.."))
}

/// The startup state over the committed catalogue, exactly as `main` builds
/// it — the env override keeps the test independent of the process cwd.
fn committed_state() -> veredictum_console::state::ConsoleState {
    let root = repo_root().join("artifacts");
    let specs = repo_root().join("specs/openehr");
    let catalogue = veredictum::pipeline::catalogue::validate_tree(&root, Some(&specs))
        .map_err(|e| e.to_string());
    veredictum_console::state::ConsoleState {
        root,
        specs,
        catalogue: std::sync::Arc::new(catalogue),
    }
}

#[expect(
    clippy::panic_in_result_fn,
    reason = "the Book ch11 Result-returning test shape: assertions panic, plumbing propagates with ? (.claude/rules/testing.md)"
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
        u64::try_from(validation.loaded.set.parties.len())?,
        u64::try_from(validation.findings.len())?,
    );

    match veredictum_console::catalogue_api::read::instrument_view(&state) {
        veredictum_console::catalogue_api::InstrumentView::Loaded(summary) => {
            assert_eq!(
                (
                    summary.cases,
                    summary.bindings,
                    summary.parties,
                    summary.findings
                ),
                expected,
                "the landing shows numbers the validate summary would not print"
            );
            assert_eq!(summary.findings, 0, "the committed catalogue is clean");
        }
        veredictum_console::catalogue_api::InstrumentView::Missing(missing) => {
            panic!(
                "the committed catalogue read as missing: {}",
                missing.reason
            );
        }
    }
    Ok(())
}

#[test]
fn a_missing_catalogue_names_the_mounts_it_looked_at() {
    let state = veredictum_console::state::ConsoleState {
        root: "/nonexistent/artifacts".into(),
        specs: "/nonexistent/specs".into(),
        catalogue: std::sync::Arc::new(Err(String::from("no such directory"))),
    };
    match veredictum_console::catalogue_api::read::instrument_view(&state) {
        veredictum_console::catalogue_api::InstrumentView::Missing(missing) => {
            assert_eq!(missing.root, "/nonexistent/artifacts");
            assert_eq!(missing.specs, "/nonexistent/specs");
            assert_eq!(missing.reason, "no such directory");
        }
        veredictum_console::catalogue_api::InstrumentView::Loaded(_) => {
            panic!("a failed load must render as the missing state")
        }
    }
}
