// SPDX-FileCopyrightText: Veredictum contributors
// SPDX-License-Identifier: Apache-2.0

//! The #66 acceptance gates: the job supervises a real engine run to a
//! finished view, cancel kills the subprocess, and the generated ixit
//! carries env-var names only.

use veredictum_console::engine::{Credential, Engine, RunSpec, Secret};
use veredictum_console::run_api::{AuthChoice, RunDraft};
use veredictum_console::run_job::{JobSlot, JobStatus};

use crate::engine_gate;

/// Polls the slot until the job leaves `Running`, bounded.
fn wait_terminal(slot: &JobSlot) -> Option<veredictum_console::run_job::JobView> {
    for _ in 0..600 {
        let view = slot.view().ok()??;
        if view.status != JobStatus::Running {
            return Some(view);
        }
        std::thread::sleep(std::time::Duration::from_millis(100));
    }
    None
}

#[expect(
    clippy::panic_in_result_fn,
    reason = "the Book ch11 Result-returning test shape: assertions panic, plumbing propagates with ?"
)]
#[test]
fn a_job_supervises_a_run_to_its_finished_view() -> Result<(), Box<dyn std::error::Error>> {
    let binary = engine_gate::gate_binary();
    if !binary.exists() {
        eprintln!(
            "SKIPPED(no engine binary at {}): build the workspace first",
            binary.display()
        );
        return Ok(());
    }
    let engine = match Engine::verified(&binary) {
        Ok(engine) => engine,
        Err(veredictum_console::engine::Error::VersionMismatch { reported }) => {
            eprintln!("SKIPPED(engine version drift): {reported}");
            return Ok(());
        }
        Err(other) => return Err(other.into()),
    };

    let scratch = assert_fs::TempDir::new()?;
    let port = engine_gate::fixture_sut()?;
    let ixit = engine_gate::write_ixit(scratch.path(), port)?;
    let out = scratch.path().join("job-run");
    std::fs::create_dir_all(&out)?;

    let slot = JobSlot::default();
    let id = slot.allocate_id()?;
    slot.start(
        id,
        &engine,
        &RunSpec {
            root: engine_gate::repo_root().join("artifacts"),
            ixit,
            out_dir: out.clone(),
            sut_name: String::from("job-gate"),
            sut_version: String::from("0.0.0-gate"),
            statement: None,
            filter: Some(String::from("I_EHR_SERVICE.create_ehr-main")),
            credentials: vec![
                Credential {
                    name: String::from("GATE_SUT_USER"),
                    value: Secret::new(String::from("gate-user")),
                },
                Credential {
                    name: String::from("GATE_SUT_PASS"),
                    value: Secret::new(String::from("gate-pass")),
                },
            ],
            progress: true,
        },
        String::from("job-gate"),
    )?;

    let terminal = wait_terminal(&slot).ok_or("the job never left Running")?;
    assert_eq!(
        terminal.status,
        JobStatus::Finished,
        "tail: {:?}",
        terminal.tail
    );
    let finished = terminal.finished.ok_or("finished without a summary")?;
    assert!(
        finished.failed + finished.errored > 0,
        "a 500-only SUT cannot pass"
    );
    assert!(std::path::Path::new(&finished.results_path).is_file());
    // The engine's own progress stream fed the counters (the workspace
    // binary carries --progress).
    assert!(
        terminal.total > 0,
        "the progress stream never announced a total"
    );
    assert_eq!(terminal.completed, terminal.total);
    Ok(())
}

#[test]
fn the_generated_ixit_carries_names_and_never_values() {
    let draft = RunDraft {
        base_url: String::from("http://cdr.example"),
        sut_name: String::from("x"),
        sut_version: String::from("y"),
        auth: AuthChoice::Basic,
        credentials: vec![
            Credential {
                name: String::from("CONSOLE_SUT_USER"),
                value: Secret::new(String::from("user-value-hunter2")),
            },
            Credential {
                name: String::from("CONSOLE_SUT_PASS"),
                value: Secret::new(String::from("pass-value-hunter2")),
            },
        ],
        probed_ok: true,
        statement_json: None,
        statement_product: None,
        filter: None,
    };
    let document = veredictum_console::run_api::read::ixit_document(&draft);
    assert!(document.contains("CONSOLE_SUT_USER"));
    assert!(document.contains("CONSOLE_SUT_PASS"));
    assert!(
        !document.contains("hunter2"),
        "a credential VALUE reached the ixit document: {document}"
    );
    // The document is a valid ixit by the published lib's own reader.
    let parsed: Result<veredictum::ixit::Ixit, _> = serde_json::from_str(&document);
    assert!(
        parsed.is_ok(),
        "the generated ixit does not parse: {parsed:?}"
    );
}

#[expect(
    clippy::panic_in_result_fn,
    reason = "the Book ch11 Result-returning test shape: assertions panic, plumbing propagates with ?"
)]
#[expect(
    clippy::too_many_lines,
    reason = "one gate walks the whole chain — run, red-first, join, judgement — and splitting it would hide the chain it exists to assert"
)]
#[test]
fn the_record_surfaces_read_a_finished_statement_run() -> Result<(), Box<dyn std::error::Error>> {
    let binary = engine_gate::gate_binary();
    if !binary.exists() {
        eprintln!(
            "SKIPPED(no engine binary at {}): build the workspace first",
            binary.display()
        );
        return Ok(());
    }
    let engine = match Engine::verified(&binary) {
        Ok(engine) => engine,
        Err(veredictum_console::engine::Error::VersionMismatch { reported }) => {
            eprintln!("SKIPPED(engine version drift): {reported}");
            return Ok(());
        }
        Err(other) => return Err(other.into()),
    };

    let scratch = assert_fs::TempDir::new()?;
    let port = engine_gate::fixture_sut()?;
    let ixit = engine_gate::write_ixit(scratch.path(), port)?;
    let out = scratch.path().join("record-run");
    std::fs::create_dir_all(&out)?;
    let statement = engine_gate::repo_root().join("party/ehrbase/statement.json");
    // What start_run does for a pasted claim: the accepted bytes travel with
    // the run, and the verdicts read them back from the job directory.
    std::fs::copy(&statement, out.join("statement.json"))?;

    let root = engine_gate::repo_root().join("artifacts");
    let state = veredictum_console::state::ConsoleState {
        root: root.clone(),
        specs: engine_gate::repo_root().join("specs/openehr"),
        party: engine_gate::repo_root().join("party"),
        out: scratch.path().to_path_buf(),
        catalogue: std::sync::Arc::new(
            veredictum::pipeline::catalogue::validate_tree(&root, None).map_err(|e| e.to_string()),
        ),
        draft: std::sync::Arc::new(std::sync::Mutex::new(Some(RunDraft {
            base_url: String::from("http://unused"),
            sut_name: String::from("record-gate"),
            sut_version: String::from("0.0.0-gate"),
            auth: AuthChoice::None,
            credentials: vec![],
            probed_ok: true,
            statement_json: Some(std::fs::read_to_string(&statement)?),
            statement_product: Some(String::from("EHRbase 2.34.0")),
            filter: None,
        }))),
        jobs: JobSlot::default(),
    };
    let id = state.jobs.allocate_id().map_err(|e| e.to_string())?;
    state
        .jobs
        .start(
            id,
            &engine,
            &RunSpec {
                root,
                ixit,
                out_dir: out,
                sut_name: String::from("record-gate"),
                sut_version: String::from("0.0.0-gate"),
                statement: Some(statement),
                filter: Some(String::from("I_EHR_SERVICE.create_ehr-main")),
                credentials: vec![],
                progress: true,
            },
            String::from("record-gate"),
        )
        .map_err(|e| e.to_string())?;
    let terminal = wait_terminal(&state.jobs).ok_or("the job never left Running")?;
    assert_eq!(
        terminal.status,
        JobStatus::Finished,
        "tail: {:?}",
        terminal.tail
    );

    // S6: the results screen reads the record red-first.
    let results = veredictum_console::record_api::read::results_screen(&state)
        .map_err(|e| format!("results: {e}"))?
        .ok_or("a finished run must yield a results screen")?;
    assert!(!results.rows.is_empty());
    let first = results.rows.first().ok_or("rows checked non-empty")?;
    assert!(
        matches!(first.status.as_str(), "failed" | "errored"),
        "red rows must sort first against a 500-only SUT: {first:?}"
    );

    // The drawer joins the catalogue.
    let detail = veredictum_console::record_api::read::result_detail(
        &state,
        &first.case,
        first.format.as_deref(),
    )
    .map_err(|e| format!("detail: {e}"))?
    .ok_or("the first row must have a detail")?;
    assert!(detail.test_purpose.is_some(), "the catalogue join failed");
    assert!(!detail.spec_refs.is_empty());

    // S7: the judgement runs through the lib and carries the documents.
    match veredictum_console::record_api::read::verdicts_screen(&state)
        .map_err(|e| format!("verdicts: {e}"))?
    {
        veredictum_console::record_api::VerdictsScreen::Judged {
            profiles,
            documents,
            ..
        } => {
            assert!(!profiles.is_empty(), "the matrix must carry the tiers");
            let names: Vec<&str> = documents.iter().map(|d| d.name.as_str()).collect();
            assert!(
                names.contains(&"CONFORMANCE_REPORT.md"),
                "documents: {names:?}"
            );
        }
        other => panic!("a statement run must judge: {other:?}"),
    }
    Ok(())
}
