// SPDX-FileCopyrightText: Veredictum contributors
// SPDX-License-Identifier: Apache-2.0

//! Documentation capture mode: the stand-ins the surfaces show for the facts
//! one run stamps.
//!
//! Six of the book's screenshots photograph a live run's clock, its record
//! digest and its signing time, which change on every pass, so the
//! `ui-screenshot-guard` job cannot tell a re-run from a real visual change.
//!
//! In capture mode the server functions answer with the fixed stand-ins
//! below, so an unchanged console photographs identically. Nothing else
//! moves: the record, the manifest, the signature and the presentation files
//! carry real values, because the pinning happens where a value is SENT TO A
//! BROWSER and nowhere else.

use crate::export_api::{ExportScreen, ExportSummary};
use crate::run_job::JobView;
use crate::verify_api::{BundleView, VerifyScreen};

/// The environment variable that turns capture mode on.
pub const CAPTURE_ENV: &str = "VEREDICTUM_CAPTURE_MODE";

/// The digest every captured surface shows in place of a real one.
///
/// All zeros: it has a digest's exact shape, so the layout is the layout a
/// reader will see, and it is unmistakably not the output of a hash.
pub const PINNED_DIGEST: &str = "0000000000000000000000000000000000000000000000000000000000000000";

/// The instant every captured surface shows in place of a real signing time:
/// the Unix epoch, which no run can have been signed at.
pub const PINNED_TIME: &str = "1970-01-01T00:00:00Z";

/// The run clock every captured live screen shows, in milliseconds.
pub const PINNED_ELAPSED_MS: u64 = 0;

/// The export section as this console answers it: pinned under capture mode,
/// verbatim otherwise.
#[must_use]
pub fn export_screen(state: &crate::state::ConsoleState, screen: ExportScreen) -> ExportScreen {
    if state.capture {
        pin_export(screen)
    } else {
        screen
    }
}

/// One sealed record's summary as this console answers it.
#[must_use]
pub fn export_summary(state: &crate::state::ConsoleState, summary: ExportSummary) -> ExportSummary {
    if state.capture {
        pin_summary(summary)
    } else {
        summary
    }
}

/// The verification page as this console answers it.
#[must_use]
pub fn verification(state: &crate::state::ConsoleState, screen: VerifyScreen) -> VerifyScreen {
    if state.capture {
        pin_verification(screen)
    } else {
        screen
    }
}

/// The live run as this console answers it.
#[must_use]
pub fn job(state: &crate::state::ConsoleState, view: Option<JobView>) -> Option<JobView> {
    if state.capture {
        view.map(pin_job)
    } else {
        view
    }
}

/// The export section as a capture shows it.
#[must_use]
pub fn pin_export(screen: ExportScreen) -> ExportScreen {
    match screen {
        ExportScreen::Prepared(summary) => ExportScreen::Prepared(Box::new(pin_summary(*summary))),
        other => other,
    }
}

/// One sealed record's facts as a capture shows them.
#[must_use]
pub fn pin_summary(summary: ExportSummary) -> ExportSummary {
    ExportSummary {
        digest: String::from(PINNED_DIGEST),
        digest_prefix: crate::export::prefix_of(PINNED_DIGEST),
        signed_at: String::from(PINNED_TIME),
        ..summary
    }
}

/// The verification page as a capture shows it.
#[must_use]
pub fn pin_verification(screen: VerifyScreen) -> VerifyScreen {
    match screen {
        VerifyScreen::Checked(view) => VerifyScreen::Checked(Box::new(pin_bundle(*view))),
        other => other,
    }
}

/// One checked bundle's facts as a capture shows them.
///
/// The per-file digests are the manifest's own, so they move with every run
/// exactly as the record digest does.
#[must_use]
pub fn pin_bundle(view: BundleView) -> BundleView {
    BundleView {
        signed_at: view.signed_at.map(|_| String::from(PINNED_TIME)),
        files: view
            .files
            .into_iter()
            .map(|file| crate::verify_api::FileRow {
                digest: String::from(PINNED_DIGEST),
                ..file
            })
            .collect(),
        ..view
    }
}

/// The live run as a capture shows it.
#[must_use]
pub fn pin_job(view: JobView) -> JobView {
    JobView {
        elapsed_ms: PINNED_ELAPSED_MS,
        eta_ms: view.eta_ms.map(|_| PINNED_ELAPSED_MS),
        ..view
    }
}

#[cfg(test)]
mod tests {
    use super::{
        PINNED_DIGEST, PINNED_ELAPSED_MS, PINNED_TIME, pin_bundle, pin_export, pin_job,
        pin_summary, pin_verification,
    };
    use crate::export_api::{ExportScreen, ExportSummary};
    use crate::run_job::{JobStatus, JobView};
    use crate::verify_api::{BundleView, FileRow, VerifyScreen};

    /// One sealed record's summary as the export seam builds it.
    fn summary() -> ExportSummary {
        ExportSummary {
            digest: String::from(
                "9f2c1e5a7b3d0f4e6a8c2b1d3e5f70819a2b3c4d5e6f708192a3b4c5d6e7f809",
            ),
            digest_prefix: String::from("9f2c1e5a7b3d"),
            fingerprint: String::from("ABCD1234"),
            signed_at: String::from("2026-08-30T09:41:07Z"),
            sut: String::from("example-cdr 1.0"),
            profile_summary: String::from("CORE pass"),
            sealed_files: vec![String::from("record-manifest.json")],
            presentation_files: vec![String::from("seal-card.svg")],
            badge_markdown: String::from("[![badge](record-badge.svg)](/verify)"),
            badge_html: String::from("<a href=\"/verify\"></a>"),
        }
    }

    /// The volatile three are pinned and everything else survives untouched:
    /// a capture that redacted the verdicts would document a different
    /// console.
    #[test]
    fn a_pinned_summary_keeps_every_stable_fact() {
        let pinned = pin_summary(summary());
        assert_eq!(pinned.digest, PINNED_DIGEST);
        assert_eq!(pinned.digest_prefix, "000000000000");
        assert_eq!(pinned.signed_at, PINNED_TIME);
        assert_eq!(pinned.fingerprint, summary().fingerprint);
        assert_eq!(pinned.sut, summary().sut);
        assert_eq!(pinned.profile_summary, summary().profile_summary);
        assert_eq!(pinned.sealed_files, summary().sealed_files);
    }

    /// Pinning is idempotent, which is the whole property a capture pass
    /// needs: the second pass photographs what the first one did.
    #[test]
    fn pinning_twice_is_pinning_once() {
        let once = pin_summary(summary());
        assert_eq!(pin_summary(once.clone()), once);
    }

    /// A screen with nothing sealed carries nothing to pin.
    #[test]
    fn an_unprepared_export_is_left_alone() {
        assert_eq!(pin_export(ExportScreen::Ready), ExportScreen::Ready);
        assert_eq!(pin_export(ExportScreen::NoRun), ExportScreen::NoRun);
        let ExportScreen::Prepared(pinned) =
            pin_export(ExportScreen::Prepared(Box::new(summary())))
        else {
            panic!("a prepared export stays prepared");
        };
        assert_eq!(pinned.digest, PINNED_DIGEST);
    }

    /// The verification page pins the signing time and every file digest, and
    /// keeps the outcome tokens — which is what the page is actually about.
    #[test]
    fn a_pinned_bundle_keeps_its_outcomes() {
        let view = BundleView {
            signature_accepted: true,
            fingerprint: Some(String::from("ABCD1234")),
            signed_at: Some(String::from("2026-08-30T09:41:07Z")),
            instrument: String::from("veredictum 0.1.1"),
            files: vec![
                FileRow {
                    name: String::from("CONFORMANCE_REPORT.md"),
                    digest: String::from("aaaa"),
                    outcome: String::from("matched"),
                    detail: None,
                },
                FileRow {
                    name: String::from("verdicts.json"),
                    digest: String::from("bbbb"),
                    outcome: String::from("mismatched"),
                    detail: Some(String::from("the body does not reproduce its digest")),
                },
            ],
            findings: vec![String::from("verdicts.json: mismatched")],
            is_clean: false,
        };
        let pinned = pin_bundle(view);
        assert_eq!(pinned.signed_at.as_deref(), Some(PINNED_TIME));
        assert!(pinned.files.iter().all(|file| file.digest == PINNED_DIGEST));
        assert_eq!(
            pinned
                .files
                .iter()
                .map(|f| f.outcome.clone())
                .collect::<Vec<_>>(),
            vec![String::from("matched"), String::from("mismatched")]
        );
        assert_eq!(
            pinned.findings,
            vec![String::from("verdicts.json: mismatched")]
        );
        assert!(!pinned.is_clean);
        // An unsigned bundle states no time, and pinning must not invent one.
        let unsigned = pin_verification(VerifyScreen::Idle);
        assert_eq!(unsigned, VerifyScreen::Idle);
    }

    /// The live screen's clock is the run's, and only the clock is pinned.
    #[test]
    fn a_pinned_job_keeps_its_progress() {
        let view = JobView {
            id: 1,
            sut_name: String::from("example-cdr"),
            status: JobStatus::Running,
            completed: 7,
            total: 11,
            current_case: Some(String::from("I_EHR_SERVICE.create_ehr-main")),
            elapsed_ms: 91_337,
            eta_ms: Some(4_200),
            tail: vec![String::from("driving 11 cases")],
            finished: None,
        };
        let pinned = pin_job(view);
        assert_eq!(pinned.elapsed_ms, PINNED_ELAPSED_MS);
        assert_eq!(pinned.eta_ms, Some(PINNED_ELAPSED_MS));
        assert_eq!(pinned.completed, 7);
        assert_eq!(pinned.total, 11);
        assert_eq!(pinned.tail, vec![String::from("driving 11 cases")]);
    }
}
