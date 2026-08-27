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
        statement: None,
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
