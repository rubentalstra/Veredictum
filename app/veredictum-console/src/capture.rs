// SPDX-FileCopyrightText: Veredictum contributors
// SPDX-License-Identifier: Apache-2.0

//! Documentation capture mode: the stand-ins the surfaces show for the facts
//! one run stamps.
//!
//! The book's screenshots photograph a live run's clock, its record digest,
//! its signing time, the run's own identity, the address the fixture server
//! bound and the latency the connect probe measured. Every one of those
//! changes on every pass, so the `ui-screenshot-guard` job cannot tell a
//! re-run from a real visual change.
//!
//! In capture mode the server functions answer with the fixed stand-ins
//! below, so an unchanged console photographs identically. Nothing else
//! moves: the record, the manifest, the signature and the presentation files
//! carry real values, because the pinning happens where a value is SENT TO A
//! BROWSER and nowhere else.
//!
//! What a pass measures is pinned; what a pass RECORDED is not. The case
//! counter, the case now driving, the engine's own output and the finished
//! tally stay verbatim, because a screenshot that redacted them would
//! document a run nobody drove. What those carry of the run's identity — the
//! output directory every path names — is rewritten to the pinned id, which
//! is the one fact in them a re-run moves.
//!
//! A moving frame is the same problem as a moving fact: the served document
//! carries [`CAPTURE_CLASS`] on its root element in capture mode, and
//! `style/tailwind.css` switches every transition and animation off under it,
//! so a screenshot photographs a settled frame instead of whatever phase of
//! the progress bar's width transition it happened to catch.

use crate::export_api::{ExportScreen, ExportSummary};
use crate::run_api::{DraftView, ProbeAnswer, RecordedResults, RecordedRun, RunScreen};
use crate::run_job::{FinishedView, JobView, RunId};
use crate::verify_api::{BundleView, VerifyScreen};

/// The environment variable that turns capture mode on.
pub const CAPTURE_ENV: &str = "VEREDICTUM_CAPTURE_MODE";

/// The class the served document's root element carries in capture mode.
///
/// `style/tailwind.css` matches it, unlayered so it outranks every Tailwind
/// utility, and switches transitions and animations off beneath it.
pub const CAPTURE_CLASS: &str = "capture";

/// Whether capture mode is on for this process.
///
/// The one read of [`CAPTURE_ENV`]: the state's flag and the served
/// document's root class are the same fact, so they cannot disagree about
/// which mode a pass is photographing.
#[must_use]
pub fn enabled() -> bool {
    std::env::var(CAPTURE_ENV).is_ok_and(|value| !value.is_empty())
}

/// The served document's root class: [`CAPTURE_CLASS`] under capture mode,
/// empty otherwise.
///
/// Server-side only by construction — the shell renders the document around
/// the hydration root and is never re-rendered in the browser — so the class
/// changes no hydrated view.
#[must_use]
pub fn root_class(capture: bool) -> &'static str {
    if capture { CAPTURE_CLASS } else { "" }
}

/// The digest every captured surface shows in place of a real one.
///
/// All zeros: it has a digest's exact shape, so the layout is the layout a
/// reader will see, and it is unmistakably not the output of a hash.
pub const PINNED_DIGEST: &str = "0000000000000000000000000000000000000000000000000000000000000000";

/// The instant every captured surface shows in place of a real signing time:
/// the Unix epoch, which no run can have been signed at.
pub const PINNED_TIME: &str = "1970-01-01T00:00:00Z";

/// Every duration a captured surface measured, in milliseconds: the run
/// clock, the estimate beside it, and the connect probe's round trip.
///
/// Zero: no run and no request takes no time at all, so the reader can tell a
/// stand-in from a measurement.
pub const PINNED_ELAPSED_MS: u64 = 0;

/// The run id every captured live screen shows in place of a minted one.
///
/// A run id is a fresh UUID per run (#386), and the live screen prints it as
/// the run's own address, so without a stand-in every capture pass would
/// rewrite the screenshot with an id nobody changed.
pub const PINNED_RUN_ID: RunId = RunId::NIL;

/// The endpoint every captured surface shows in place of the address its run
/// drove: the submission's disclosure and the scope screen's connection pane.
///
/// The harness's fixture server binds an ephemeral port, so the real value
/// moves on every pass; port zero is a port no server ever answers on, which
/// makes the stand-in unmistakable.
pub const PINNED_ENDPOINT: &str = "http://127.0.0.1:0";

/// The submission screen as this console answers it: pinned under capture
/// mode, verbatim otherwise.
#[cfg(feature = "ssr")]
#[must_use]
pub fn submit_screen(
    state: &crate::state::ConsoleState,
    screen: crate::submit_api::SubmitScreen,
) -> crate::submit_api::SubmitScreen {
    if state.capture {
        pin_submission(screen)
    } else {
        screen
    }
}

/// The submission screen as a capture shows it.
///
/// Five facts move between runs: the run's own id, the entry id derived from
/// it and the run's start date, the branch that carries it, the disclosed
/// start, and the endpoint. The paths the screen lists carry the entry id, so
/// they are re-derived from the pinned one through the seam's own derivation.
#[must_use]
pub fn pin_submission(screen: crate::submit_api::SubmitScreen) -> crate::submit_api::SubmitScreen {
    let crate::submit_api::SubmitScreen::Ready(facts) = screen else {
        return screen;
    };
    let run_id = PINNED_RUN_ID.to_string();
    let entry_id = format!(
        "{}-{}",
        PINNED_TIME.get(..10).unwrap_or(PINNED_TIME),
        crate::submit_api::slug_of(&run_id)
    );
    crate::submit_api::SubmitScreen::Ready(Box::new(crate::submit_api::SubmissionFacts {
        files: crate::submit_api::submission_paths(&facts.system, &entry_id),
        branch: format!("console-run/{run_id}"),
        endpoint: String::from(PINNED_ENDPOINT),
        run_started_at: String::from(PINNED_TIME),
        run_id,
        entry_id,
        ..*facts
    }))
}

/// The export section as this console answers it: pinned under capture mode,
/// verbatim otherwise.
#[cfg(feature = "ssr")]
#[must_use]
pub fn export_screen(state: &crate::state::ConsoleState, screen: ExportScreen) -> ExportScreen {
    if state.capture {
        pin_export(screen)
    } else {
        screen
    }
}

/// One sealed record's summary as this console answers it.
#[cfg(feature = "ssr")]
#[must_use]
pub fn export_summary(state: &crate::state::ConsoleState, summary: ExportSummary) -> ExportSummary {
    if state.capture {
        pin_summary(summary)
    } else {
        summary
    }
}

/// The verification page as this console answers it.
#[cfg(feature = "ssr")]
#[must_use]
pub fn verification(state: &crate::state::ConsoleState, screen: VerifyScreen) -> VerifyScreen {
    if state.capture {
        pin_verification(screen)
    } else {
        screen
    }
}

/// The live screen as this console answers it.
#[cfg(feature = "ssr")]
#[must_use]
pub fn run_screen(state: &crate::state::ConsoleState, screen: RunScreen) -> RunScreen {
    if state.capture {
        pin_run_screen(screen)
    } else {
        screen
    }
}

/// The connection draft as this console answers it.
#[cfg(feature = "ssr")]
#[must_use]
pub fn draft(state: &crate::state::ConsoleState, view: Option<DraftView>) -> Option<DraftView> {
    if state.capture {
        view.map(pin_draft)
    } else {
        view
    }
}

/// The probe's answer as this console answers it.
#[cfg(feature = "ssr")]
#[must_use]
pub fn probe_answer(state: &crate::state::ConsoleState, answer: ProbeAnswer) -> ProbeAnswer {
    if state.capture {
        pin_probe(answer)
    } else {
        answer
    }
}

/// One connection draft as a capture shows it.
///
/// The harness's fixture SUT binds an ephemeral port, and the scope screen
/// prints the drafted address verbatim, so the one fact a re-run moves is the
/// address itself.
#[must_use]
pub fn pin_draft(view: DraftView) -> DraftView {
    DraftView {
        base_url: String::from(PINNED_ENDPOINT),
        ..view
    }
}

/// One probe answer as a capture shows it.
///
/// The connect screen prints the measured round trip verbatim, so a stopwatch
/// is in the picture until it is pinned. An unreachable server carries the
/// transport's own words and no measurement.
#[must_use]
pub fn pin_probe(answer: ProbeAnswer) -> ProbeAnswer {
    match answer {
        ProbeAnswer::Answered { status, ok, .. } => ProbeAnswer::Answered {
            status,
            elapsed_ms: PINNED_ELAPSED_MS,
            ok,
        },
        other @ ProbeAnswer::Unreachable { .. } => other,
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

/// The live screen as a capture shows it.
///
/// A streamed run and a run read back from its own directory both carry the
/// run's identity; the other two states name nothing a re-run moves.
#[must_use]
pub fn pin_run_screen(screen: RunScreen) -> RunScreen {
    match screen {
        RunScreen::Live(view) => RunScreen::Live(Box::new(pin_job(*view))),
        RunScreen::Recorded(run) => RunScreen::Recorded(Box::new(pin_recorded(*run))),
        other => other,
    }
}

/// The live run as a capture shows it.
///
/// The clock, the estimate and the run's own address are stand-ins. The
/// counter, the case now driving, the engine's output and the finished tally
/// are the run's own, with the minted id every path in them names rewritten
/// to the pinned one: the output directory carries that id, so the tail and
/// the results path move on every pass while nothing else in them does.
#[must_use]
pub fn pin_job(view: JobView) -> JobView {
    let minted = view.id.to_string();
    JobView {
        id: PINNED_RUN_ID,
        elapsed_ms: PINNED_ELAPSED_MS,
        eta_ms: view.eta_ms.map(|_| PINNED_ELAPSED_MS),
        tail: view
            .tail
            .iter()
            .map(|line| pin_run_paths(line, &minted))
            .collect(),
        finished: view.finished.map(|summary| FinishedView {
            results_path: pin_run_paths(&summary.results_path, &minted),
            ..summary
        }),
        ..view
    }
}

/// One recorded run's facts as a capture shows them.
///
/// Its directory and the document inside it are named by the run's own id, so
/// both are re-derived from the pinned one; the tally the record carries is
/// the record's own.
#[must_use]
pub fn pin_recorded(run: RecordedRun) -> RecordedRun {
    let minted = run.id.to_string();
    RecordedRun {
        id: PINNED_RUN_ID,
        dir: pin_run_paths(&run.dir, &minted),
        results: run.results.map(|results| RecordedResults {
            results_path: pin_run_paths(&results.results_path, &minted),
            ..results
        }),
    }
}

/// One line with the run's minted id rewritten to the pinned one.
#[must_use]
fn pin_run_paths(line: &str, minted: &str) -> String {
    line.replace(minted, &PINNED_RUN_ID.to_string())
}

#[cfg(test)]
mod tests {
    use super::{
        PINNED_DIGEST, PINNED_ELAPSED_MS, PINNED_RUN_ID, PINNED_TIME, pin_bundle, pin_export,
        pin_job, pin_run_screen, pin_summary, pin_verification,
    };
    use crate::export_api::{ExportScreen, ExportSummary};
    use crate::run_api::{DraftView, ProbeAnswer, RecordedResults, RecordedRun, RunScreen};
    use crate::run_job::{FinishedView, JobStatus, JobView, RunId};
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

    /// The stylesheet's still-frame rule keys on this exact class, and an
    /// ordinary serve carries none of it: a console that photographed with
    /// transitions off would document a different product.
    #[test]
    fn only_a_capture_serve_stills_the_frame() {
        assert_eq!(super::root_class(true), super::CAPTURE_CLASS);
        assert_eq!(super::root_class(false), "");
    }

    /// The minted id every fixture run carries in the harness.
    const MINTED: &str = "3f2504e0-4f89-41d3-9a0c-0305e82c3301";

    /// One driving job as the slot answers it.
    fn driving() -> JobView {
        JobView {
            id: RunId::NIL,
            sut_name: String::from("example-cdr"),
            status: JobStatus::Running,
            completed: 7,
            total: 11,
            current_case: Some(String::from("I_EHR_SERVICE.create_ehr-main")),
            elapsed_ms: 91_337,
            eta_ms: Some(4_200),
            tail: vec![String::from("driving 11 cases")],
            finished: None,
        }
    }

    /// The live screen's clock, its estimate and the run's own address are
    /// pinned; the engine's own progress facts are not, because a screenshot
    /// that redacted them would document a run nobody drove.
    #[expect(
        clippy::panic_in_result_fn,
        reason = "the Book ch11 Result-returning test shape: assertions panic, plumbing propagates with ?"
    )]
    #[test]
    fn a_pinned_job_keeps_the_engine_s_own_progress() -> Result<(), crate::run_job::RunIdError> {
        let minted: RunId = MINTED.parse()?;
        let pinned = pin_job(JobView {
            id: minted,
            ..driving()
        });
        assert_eq!(pinned.elapsed_ms, PINNED_ELAPSED_MS);
        assert_eq!(pinned.eta_ms, Some(PINNED_ELAPSED_MS));
        assert_eq!(pinned.id, PINNED_RUN_ID, "the address is a fresh UUID");
        assert_eq!(pinned.completed, 7);
        assert_eq!(pinned.total, 11);
        assert_eq!(
            pinned.current_case.as_deref(),
            Some("I_EHR_SERVICE.create_ehr-main")
        );
        Ok(())
    }

    /// The run's own directory is named by the id a run mints, so every path
    /// the live screen prints — the engine's output tail and the finished
    /// summary's results document — is re-derived from the pinned id, and the
    /// engine's own words around it survive.
    #[expect(
        clippy::panic_in_result_fn,
        reason = "the Book ch11 Result-returning test shape: assertions panic, plumbing propagates with ?"
    )]
    #[test]
    fn a_pinned_job_names_no_minted_directory() -> Result<(), crate::run_job::RunIdError> {
        let minted: RunId = MINTED.parse()?;
        let pinned = pin_job(JobView {
            id: minted,
            tail: vec![
                String::from("1 case-records: 0 passed / 0 failed / 1 errored / 0 n-a"),
                format!("wrote target/out/console-job-{MINTED}/transcript.json"),
            ],
            finished: Some(FinishedView {
                passed: 0,
                failed: 0,
                errored: 1,
                not_applicable: 0,
                results_path: format!("target/out/console-job-{MINTED}/results.json"),
            }),
            ..driving()
        });
        assert_eq!(
            pinned.tail,
            vec![
                String::from("1 case-records: 0 passed / 0 failed / 1 errored / 0 n-a"),
                format!("wrote target/out/console-job-{PINNED_RUN_ID}/transcript.json"),
            ]
        );
        let Some(summary) = pinned.finished else {
            panic!("a finished run keeps its summary");
        };
        assert_eq!(
            summary.results_path,
            format!("target/out/console-job-{PINNED_RUN_ID}/results.json")
        );
        assert_eq!(summary.errored, 1, "the record's own tally is the record's");
        Ok(())
    }

    /// A run read back from its own directory carries the same identity in
    /// the same two places.
    #[expect(
        clippy::panic_in_result_fn,
        reason = "the Book ch11 Result-returning test shape: assertions panic, plumbing propagates with ?"
    )]
    #[test]
    fn a_pinned_recorded_run_names_no_minted_directory() -> Result<(), crate::run_job::RunIdError> {
        let minted: RunId = MINTED.parse()?;
        let RunScreen::Recorded(pinned) =
            pin_run_screen(RunScreen::Recorded(Box::new(RecordedRun {
                id: minted,
                dir: format!("target/out/console-job-{MINTED}"),
                results: Some(RecordedResults {
                    sut_name: String::from("my-cdr"),
                    passed: 0,
                    failed: 0,
                    errored: 1,
                    not_applicable: 0,
                    results_path: format!("target/out/console-job-{MINTED}/results.json"),
                }),
            })))
        else {
            panic!("a recorded run stays recorded");
        };
        assert_eq!(pinned.id, PINNED_RUN_ID);
        assert_eq!(
            pinned.dir,
            format!("target/out/console-job-{PINNED_RUN_ID}")
        );
        let Some(results) = pinned.results else {
            panic!("a recorded run keeps its results");
        };
        assert_eq!(
            results.results_path,
            format!("target/out/console-job-{PINNED_RUN_ID}/results.json")
        );
        assert_eq!(results.sut_name, "my-cdr");
        Ok(())
    }

    /// The scope screen prints the drafted address, and the harness's fixture
    /// SUT binds an ephemeral port, so the address is a stand-in and every
    /// selection fact the screen is about survives.
    #[test]
    fn a_pinned_draft_keeps_every_selection_fact() {
        let pinned = super::pin_draft(DraftView {
            base_url: String::from("http://127.0.0.1:54270"),
            sut_name: String::from("my-cdr"),
            sut_version: String::from("unknown"),
            auth: String::from("none"),
            probed_ok: false,
            statement: Some(String::from("my-cdr unknown")),
            filter: Some(String::from("I_EHR_SERVICE.create_ehr-main")),
            record_exchanges: true,
            postures: vec![String::from("system id: cdr.example.org")],
        });
        assert_eq!(pinned.base_url, super::PINNED_ENDPOINT);
        assert_eq!(pinned.sut_name, "my-cdr");
        assert_eq!(pinned.statement.as_deref(), Some("my-cdr unknown"));
        assert_eq!(
            pinned.filter.as_deref(),
            Some("I_EHR_SERVICE.create_ehr-main")
        );
        assert!(pinned.record_exchanges);
        assert!(!pinned.probed_ok);
        assert_eq!(
            pinned.postures,
            vec![String::from("system id: cdr.example.org")]
        );
    }

    /// The connect screen prints the probe's measured round trip, so the
    /// measurement is a stand-in and the server's own answer is not.
    #[test]
    fn a_pinned_probe_keeps_the_server_s_own_answer() {
        let pinned = super::pin_probe(ProbeAnswer::Answered {
            status: String::from("HTTP 500 Internal Server Error"),
            elapsed_ms: 37,
            ok: false,
        });
        assert_eq!(
            pinned,
            ProbeAnswer::Answered {
                status: String::from("HTTP 500 Internal Server Error"),
                elapsed_ms: PINNED_ELAPSED_MS,
                ok: false,
            }
        );
        // Nothing answered, so there is no measurement to pin.
        let unreachable = ProbeAnswer::Unreachable {
            error: String::from("connection refused"),
        };
        assert_eq!(super::pin_probe(unreachable.clone()), unreachable);
    }

    /// A live screen with no streamed run carries nothing a re-run moves.
    #[test]
    fn only_a_streamed_run_is_pinned() {
        assert_eq!(pin_run_screen(RunScreen::NoRunNamed), RunScreen::NoRunNamed);
        assert_eq!(
            pin_run_screen(RunScreen::Unknown(RunId::NIL)),
            RunScreen::Unknown(RunId::NIL)
        );
        let RunScreen::Live(pinned) = pin_run_screen(RunScreen::Live(Box::new(driving()))) else {
            panic!("a streamed run stays streamed");
        };
        assert_eq!(pinned.elapsed_ms, PINNED_ELAPSED_MS);
    }

    /// One ready submission as the seam builds it.
    fn ready() -> crate::submit_api::SubmitScreen {
        crate::submit_api::SubmitScreen::Ready(Box::new(crate::submit_api::SubmissionFacts {
            run_id: String::from("3f2504e0-4f89-41d3-9a0c-0305e82c3301"),
            entry_id: String::from("2026-08-31-console-3f2504e04f89"),
            branch: String::from("console-run/3f2504e0-4f89-41d3-9a0c-0305e82c3301"),
            repo: String::from("rubentalstra/Veredictum"),
            display_name: String::from("my-cdr"),
            version: String::from("unknown"),
            system: String::from("my-cdr"),
            endpoint: String::from("http://127.0.0.1:54321"),
            instrument_version: String::from(crate::ENGINE_PIN),
            run_started_at: String::from("2026-08-31T09:41:07Z"),
            catalogue_revision: String::from("cnf-2.0-w2"),
            files: crate::submit_api::submission_paths("my-cdr", "2026-08-31-console-3f2504e04f89"),
        }))
    }

    /// The five facts a re-run moves are pinned, the paths follow the pinned
    /// entry id, and the disclosure the screen is about survives untouched.
    #[test]
    fn a_pinned_submission_keeps_what_a_re_run_does_not_move() {
        let crate::submit_api::SubmitScreen::Ready(pinned) = super::pin_submission(ready()) else {
            panic!("a ready submission stays ready");
        };
        assert_eq!(pinned.run_id, PINNED_RUN_ID.to_string());
        assert_eq!(pinned.run_started_at, PINNED_TIME);
        assert_eq!(pinned.endpoint, super::PINNED_ENDPOINT);
        assert_eq!(pinned.entry_id, "1970-01-01-console-000000000000");
        assert_eq!(pinned.branch, format!("console-run/{PINNED_RUN_ID}"));
        assert!(
            pinned
                .files
                .iter()
                .all(|path| path.contains(&pinned.entry_id)),
            "{:?}",
            pinned.files
        );
        assert_eq!(pinned.repo, "rubentalstra/Veredictum");
        assert_eq!(pinned.catalogue_revision, "cnf-2.0-w2");
        assert_eq!(pinned.system, "my-cdr");
    }

    /// Pinning is idempotent, and a screen with no submission to make carries
    /// nothing a re-run moves.
    #[test]
    fn pinning_a_submission_twice_is_pinning_it_once() {
        let once = super::pin_submission(ready());
        assert_eq!(super::pin_submission(once.clone()), once);
        assert_eq!(
            super::pin_submission(crate::submit_api::SubmitScreen::NoRun),
            crate::submit_api::SubmitScreen::NoRun
        );
    }
}
