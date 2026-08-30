// SPDX-FileCopyrightText: Veredictum contributors
// SPDX-License-Identifier: Apache-2.0

//! The #66 acceptance gates: the job supervises a real engine run to a
//! finished view, cancel kills the subprocess, and the generated ixit
//! carries env-var names only.
//!
//! Plus the #389 cap gates: two runs drive at once, a third queues with its
//! place, one address gets one run in flight, and a run past the wall clock
//! is ended by the console with its partial record discarded.

use veredictum_console::engine::{Credential, Engine, RunSpec, Secret};
use veredictum_console::run_api::{AuthChoice, RunDraft, RunScreen, StartOutcome, read};
use veredictum_console::run_job::{JobSlot, JobStatus, JobView, Latest, Limits, RunId};
use veredictum_console::submitter::{Submitter, of_request};

use crate::engine_gate;

/// Polls the map until the NAMED job leaves `Running`, bounded.
///
/// Addressed by id because several runs share the map (#389): "the job" is no
/// longer a thing this suite can ask about.
fn wait_terminal(slot: &JobSlot, id: RunId) -> Option<JobView> {
    for _ in 0..600 {
        let view = slot.view_of(id).ok()??;
        if view.status.is_terminal() {
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
    let engine = Engine::verified(&binary)?;

    let scratch = assert_fs::TempDir::new()?;
    let port = engine_gate::fixture_sut()?;
    let ixit = engine_gate::write_ixit(scratch.path(), port)?;
    let out = scratch.path().join("job-run");
    std::fs::create_dir_all(&out)?;

    let slot = JobSlot::default();
    let id = slot.allocate_id();
    slot.start(
        id,
        engine_gate::gate_submitter(),
        &engine,
        RunSpec {
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
            record_exchanges: false,
        },
        String::from("job-gate"),
    )?;

    let terminal = wait_terminal(&slot, id).ok_or("the job never left Running")?;
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
        record_exchanges: false,
    };
    let document = read::ixit_document(&draft);
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
    let engine = Engine::verified(&binary)?;

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
        draft: std::sync::Arc::new(std::sync::Mutex::new(engine_gate::drafts_of(RunDraft {
            base_url: String::from("http://unused"),
            sut_name: String::from("record-gate"),
            sut_version: String::from("0.0.0-gate"),
            auth: AuthChoice::None,
            credentials: vec![],
            probed_ok: true,
            statement_json: Some(std::fs::read_to_string(&statement)?),
            statement_product: Some(String::from("EHRbase 2.34.0")),
            filter: None,
            record_exchanges: false,
        }))),
        sign_key: None,
        verify_key: None,
        jobs: JobSlot::default(),
        client_ip_header: None,
        capture: false,
    };
    let id = state.jobs.allocate_id();
    state
        .jobs
        .start(
            id,
            engine_gate::gate_submitter(),
            &engine,
            RunSpec {
                root,
                ixit,
                out_dir: out,
                sut_name: String::from("record-gate"),
                sut_version: String::from("0.0.0-gate"),
                statement: Some(statement),
                filter: Some(String::from("I_EHR_SERVICE.create_ehr-main")),
                credentials: vec![],
                progress: true,
                record_exchanges: false,
            },
            String::from("record-gate"),
        )
        .map_err(|e| e.to_string())?;
    let terminal = wait_terminal(&state.jobs, id).ok_or("the job never left Running")?;
    assert_eq!(
        terminal.status,
        JobStatus::Finished,
        "tail: {:?}",
        terminal.tail
    );
    assert_eq!(terminal.id, id, "the view answers under the allocated id");

    // The run this process drove streams under its own address, and the bare
    // address resolves to the same run (#386).
    let who = engine_gate::gate_submitter();
    let RunScreen::Live(streamed) = read::run_screen(&state, who, Some(id))? else {
        panic!("the process holding the run streams it");
    };
    assert_eq!(streamed.id, id);
    assert_eq!(
        state
            .jobs
            .latest_of(who, Latest::Any)
            .map_err(|e| e.to_string())?,
        Some(id)
    );
    let RunScreen::Live(bare) = read::run_screen(&state, who, None)? else {
        panic!("a bare /run/live is this submitter's most recent run");
    };
    assert_eq!(bare.id, id);
    // #389: the bare address is per-submitter, and an ADDRESSED run stays
    // readable by anyone holding its id.
    let stranger = of_request(Some("203.0.113.5"), None);
    assert_eq!(
        read::run_screen(&state, stranger, None)?,
        RunScreen::NoRunNamed,
        "a bare /run/live never shows another visitor's run"
    );
    assert!(
        matches!(
            read::run_screen(&state, stranger, Some(id))?,
            RunScreen::Live(_)
        ),
        "a run's own address answers whoever holds it"
    );

    // S6: the results screen reads the record red-first.
    let results = veredictum_console::record_api::read::results_screen(&state, who)
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
        who,
        &first.case,
        first.format.as_deref(),
    )
    .map_err(|e| format!("detail: {e}"))?
    .ok_or("the first row must have a detail")?;
    assert!(detail.test_purpose.is_some(), "the catalogue join failed");
    assert!(!detail.spec_refs.is_empty());
    // This run asked for no transcript, so the drawer says so rather than
    // showing an empty wire section (#96).
    assert_eq!(
        detail.transcript,
        veredictum_console::record_api::TranscriptView::NotRecorded,
        "an unrecorded run carries no transcript file"
    );

    // S7: the judgement runs through the lib and carries the documents.
    match veredictum_console::record_api::read::verdicts_screen(&state, who)
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

/// The #96 gate: the operator ticks "Record the wire exchanges" on Scope, the
/// draft carries it into the spawned run, and the results drawer reads the
/// exchanges the transcript beside the record holds.
#[expect(
    clippy::panic_in_result_fn,
    reason = "the Book ch11 Result-returning test shape: assertions panic, plumbing propagates with ?"
)]
#[expect(
    clippy::too_many_lines,
    reason = "one gate walks the whole chain — tick, drive, read the drawer — and splitting it would hide the chain it exists to assert"
)]
#[test]
fn a_recorded_run_fills_the_drawer_with_its_wire() -> Result<(), Box<dyn std::error::Error>> {
    let binary = engine_gate::gate_binary();
    if !binary.exists() {
        eprintln!(
            "SKIPPED(no engine binary at {}): build the workspace first",
            binary.display()
        );
        return Ok(());
    }
    let engine = Engine::verified(&binary)?;

    let scratch = assert_fs::TempDir::new()?;
    let port = engine_gate::fixture_sut()?;
    let ixit = engine_gate::write_ixit(scratch.path(), port)?;
    let out = scratch.path().join("wire-run");
    std::fs::create_dir_all(&out)?;

    let root = engine_gate::repo_root().join("artifacts");
    let state = veredictum_console::state::ConsoleState {
        root: root.clone(),
        specs: engine_gate::repo_root().join("specs/openehr"),
        party: engine_gate::repo_root().join("party"),
        out: scratch.path().to_path_buf(),
        catalogue: std::sync::Arc::new(
            veredictum::pipeline::catalogue::validate_tree(&root, None).map_err(|e| e.to_string()),
        ),
        draft: std::sync::Arc::new(std::sync::Mutex::new(engine_gate::drafts_of(RunDraft {
            base_url: String::from("http://unused"),
            sut_name: String::from("wire-gate"),
            sut_version: String::from("0.0.0-gate"),
            auth: AuthChoice::None,
            credentials: vec![],
            probed_ok: true,
            statement_json: None,
            statement_product: None,
            filter: None,
            record_exchanges: false,
        }))),
        sign_key: None,
        verify_key: None,
        jobs: JobSlot::default(),
        client_ip_header: None,
        capture: false,
    };

    // The Scope step with the box ticked: the save is what carries the choice
    // onto the draft that start_run then reads.
    let who = engine_gate::gate_submitter();
    let saved =
        read::save_scope(&state, who, None, None, true).map_err(|e| format!("scope: {e}"))?;
    assert_eq!(saved, None, "no claim was pasted, so there is no summary");
    let view = read::draft_view(&state, who).ok_or("the draft exists after the scope save")?;
    assert!(view.record_exchanges, "the ticked box reached the draft");

    let id = state.jobs.allocate_id();
    state
        .jobs
        .start(
            id,
            engine_gate::gate_submitter(),
            &engine,
            RunSpec {
                root,
                ixit,
                out_dir: out.clone(),
                sut_name: String::from("wire-gate"),
                sut_version: String::from("0.0.0-gate"),
                statement: None,
                filter: Some(String::from("I_EHR_SERVICE.create_ehr-main")),
                // The fixture ixit declares Basic auth: without the values
                // the driver refuses every step before it sends, and the run
                // would record no wire at all.
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
                record_exchanges: view.record_exchanges,
            },
            String::from("wire-gate"),
        )
        .map_err(|e| e.to_string())?;
    let terminal = wait_terminal(&state.jobs, id).ok_or("the job never left Running")?;
    assert_eq!(
        terminal.status,
        JobStatus::Finished,
        "tail: {:?}",
        terminal.tail
    );

    assert!(
        out.join("transcript.json").is_file(),
        "the recorded run wrote its transcript beside the record"
    );
    let results = veredictum_console::record_api::read::results_screen(&state, who)
        .map_err(|e| format!("results: {e}"))?
        .ok_or("a finished run must yield a results screen")?;
    let first = results.rows.first().ok_or("the run recorded rows")?;
    let detail = veredictum_console::record_api::read::result_detail(
        &state,
        who,
        &first.case,
        first.format.as_deref(),
    )
    .map_err(|e| format!("detail: {e}"))?
    .ok_or("the first row must have a detail")?;

    let veredictum_console::record_api::TranscriptView::Recorded(exchanges) = detail.transcript
    else {
        panic!("a recorded run fills the drawer");
    };
    let first_exchange = exchanges
        .first()
        .ok_or("the driven case sent at least one request")?;
    assert!(
        first_exchange.request_line.contains("http://127.0.0.1:"),
        "the request line names the fixture SUT: {}",
        first_exchange.request_line
    );
    assert_eq!(first_exchange.status_line, "HTTP 500");
    assert_eq!(first_exchange.response_body.as_deref(), Some("no"));
    Ok(())
}

/// A console state over one scratch output tree, with no catalogue: the live
/// screen's resolver reads the slot and the output mount, and nothing else.
fn state_over(out: &std::path::Path) -> veredictum_console::state::ConsoleState {
    veredictum_console::state::ConsoleState {
        root: engine_gate::repo_root().join("artifacts"),
        specs: engine_gate::repo_root().join("specs/openehr"),
        party: engine_gate::repo_root().join("party"),
        out: out.to_path_buf(),
        catalogue: std::sync::Arc::new(Err(String::from("unused by the live screen"))),
        draft: std::sync::Arc::new(std::sync::Mutex::new(
            veredictum_console::run_api::Drafts::new(),
        )),
        client_ip_header: None,
        sign_key: None,
        verify_key: None,
        jobs: JobSlot::default(),
        capture: false,
    }
}

/// The #386 gate: the live screen has three honest answers about a run this
/// process is not driving, and "no run is in flight" is said only about a
/// request that named none.
#[expect(
    clippy::panic_in_result_fn,
    reason = "the Book ch11 Result-returning test shape: assertions panic, plumbing propagates with ?"
)]
#[test]
fn the_live_screen_answers_for_a_run_this_process_never_drove()
-> Result<(), Box<dyn std::error::Error>> {
    // Authored as bytes, the way the engine writes the document the console
    // reads back, so a codec change fails here.
    const RECORD: &str = r#"{
        "sut": { "name": "recorded-cdr", "version": "9.9" },
        "runner": {
            "name": "veredictum",
            "version": "0",
            "verification_pack_status": "passed"
        },
        "schedule_release": "0",
        "tech_profile": { "its": "its-rest", "formats": [] },
        "ixit_digest": "0",
        "outcomes": [
            { "case": "A-a", "status": "passed", "rows_driven": 1, "rows_total": 1 },
            { "case": "A-b", "status": "failed", "rows_driven": 1, "rows_total": 1 },
            { "case": "A-c", "status": "errored", "rows_driven": 1, "rows_total": 1 },
            {
                "case": "A-d",
                "status": "not_applicable",
                "rows_driven": 0,
                "rows_total": 1,
                "citation": "ITS-REST"
            }
        ]
    }"#;

    let scratch = assert_fs::TempDir::new()?;
    let state = state_over(scratch.path());
    let id = state.jobs.allocate_id();

    // Nothing named, nothing in flight: the one place that sentence is true.
    let who = engine_gate::gate_submitter();
    assert_eq!(read::run_screen(&state, who, None)?, RunScreen::NoRunNamed);

    // A named run with no directory: this instance knows nothing of it, and
    // says so about the run rather than about itself.
    assert_eq!(
        read::run_screen(&state, who, Some(id))?,
        RunScreen::Unknown(id)
    );

    // A directory with no results document is a recorded run that left none.
    let dir = veredictum_console::run_job::job_dir(scratch.path(), id);
    std::fs::create_dir_all(&dir)?;
    let RunScreen::Recorded(empty) = read::run_screen(&state, who, Some(id))? else {
        panic!("a job directory is a recorded run");
    };
    assert_eq!(empty.id, id);
    assert_eq!(empty.results, None, "no results document, no outcome");
    assert_eq!(empty.dir, dir.display().to_string());

    // With the record beside it, the tally is the engine's own, read through
    // the published lib.
    std::fs::write(dir.join("results.json"), RECORD)?;
    let RunScreen::Recorded(recorded) = read::run_screen(&state, who, Some(id))? else {
        panic!("a job directory is a recorded run");
    };
    let results = recorded.results.ok_or("the record parses into a tally")?;
    assert_eq!(results.sut_name, "recorded-cdr");
    assert_eq!(
        (
            results.passed,
            results.failed,
            results.errored,
            results.not_applicable
        ),
        (1, 1, 1, 1)
    );
    assert_eq!(
        results.results_path,
        dir.join("results.json").display().to_string()
    );
    Ok(())
}
/// One visitor, by the last octet of their address.
fn visitor(octet: u8) -> Submitter {
    of_request(
        None,
        Some(std::net::IpAddr::V4(std::net::Ipv4Addr::new(
            203, 0, 113, octet,
        ))),
    )
}

/// One run against the fixture SUT, writing into its own directory.
fn spec_for(
    root: &std::path::Path,
    ixit: &std::path::Path,
    out_dir: std::path::PathBuf,
) -> RunSpec {
    RunSpec {
        root: root.to_path_buf(),
        ixit: ixit.to_path_buf(),
        out_dir,
        sut_name: String::from("cap-gate"),
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
        record_exchanges: false,
    }
}

/// Polls until the NAMED job leaves `Running`, within `budget`.
fn wait_terminal_for(slot: &JobSlot, id: RunId, budget: std::time::Duration) -> Option<JobView> {
    let deadline = std::time::Instant::now() + budget;
    while std::time::Instant::now() < deadline {
        let view = slot.view_of(id).ok()??;
        if view.status.is_terminal() {
            return Some(view);
        }
        std::thread::sleep(std::time::Duration::from_millis(100));
    }
    None
}

/// The #389 concurrency gate, driven at the caps the console ships: two
/// engine processes drive at once, each addressed by its own id and showing
/// only its own facts, and the start past the ceiling is QUEUED with its
/// place stated rather than refused or left spinning.
#[expect(
    clippy::panic_in_result_fn,
    reason = "the Book ch11 Result-returning test shape: assertions panic, plumbing propagates with ?"
)]
#[test]
fn two_runs_drive_at_once_and_the_third_is_queued() -> Result<(), Box<dyn std::error::Error>> {
    let binary = engine_gate::gate_binary();
    if !binary.exists() {
        eprintln!(
            "SKIPPED(no engine binary at {}): build the workspace first",
            binary.display()
        );
        return Ok(());
    }
    let engine = Engine::verified(&binary)?;
    let scratch = assert_fs::TempDir::new()?;
    // A slow SUT, so the runs are still in flight when the assertions read
    // them: a cap about concurrency cannot be observed on a run that ends
    // before the next line executes.
    let port = engine_gate::slow_fixture_sut(std::time::Duration::from_millis(400))?;
    let ixit = engine_gate::write_ixit(scratch.path(), port)?;
    let root = engine_gate::repo_root().join("artifacts");

    let slot = JobSlot::default();
    let limits = slot.limits();
    let mut started: Vec<RunId> = Vec::new();
    for seat in 0..=u8::try_from(limits.max_concurrent).unwrap_or(2) {
        let id = slot.allocate_id();
        let dir = veredictum_console::run_job::job_dir(scratch.path(), id);
        std::fs::create_dir_all(&dir)?;
        slot.start(
            id,
            visitor(seat),
            &engine,
            spec_for(&root, &ixit, dir),
            format!("cap-gate-{seat}"),
        )?;
        started.push(id);
    }

    assert_eq!(
        slot.running()?,
        limits.max_concurrent,
        "the ceiling is what drives at once"
    );
    let mut queued = 0_usize;
    for (seat, id) in started.iter().enumerate() {
        let view = slot
            .view_of(*id)?
            .ok_or("every started run is in the map")?;
        assert_eq!(view.id, *id, "a run's view is its own");
        assert_eq!(
            view.sut_name,
            format!("cap-gate-{seat}"),
            "one run's view never carries another's facts"
        );
        if let JobStatus::Queued { position } = view.status {
            queued += 1;
            assert_eq!(
                position,
                u32::try_from(queued).unwrap_or(0),
                "a queued run states its place"
            );
        }
    }
    assert_eq!(
        queued, 1,
        "the start past the ceiling is queued, not refused"
    );

    // Cancelling the queued run removes it without ever spawning a process,
    // and the runs that were driving are untouched.
    let queued_run = started
        .iter()
        .copied()
        .find(|id| {
            slot.view_of(*id)
                .ok()
                .flatten()
                .is_some_and(|view| matches!(view.status, JobStatus::Queued { .. }))
        })
        .ok_or("the queued run was located above")?;
    slot.cancel(queued_run)?;
    assert_eq!(
        slot.view_of(queued_run)?,
        None,
        "a cancelled queue entry leaves no run behind"
    );
    assert_eq!(slot.running()?, limits.max_concurrent);

    for id in started {
        if id != queued_run {
            drop(slot.cancel(id));
        }
    }
    Ok(())
}

/// The #389 per-submitter cap: one address gets one run in flight, and the
/// second start is refused NAMING the run they already have. Another visitor
/// is unaffected.
#[expect(
    clippy::panic_in_result_fn,
    reason = "the Book ch11 Result-returning test shape: assertions panic, plumbing propagates with ?"
)]
#[test]
fn a_second_start_from_one_address_names_the_run_it_already_has()
-> Result<(), Box<dyn std::error::Error>> {
    let binary = engine_gate::gate_binary();
    if !binary.exists() {
        eprintln!(
            "SKIPPED(no engine binary at {}): build the workspace first",
            binary.display()
        );
        return Ok(());
    }
    let engine = Engine::verified(&binary)?;
    let scratch = assert_fs::TempDir::new()?;
    let port = engine_gate::slow_fixture_sut(std::time::Duration::from_millis(400))?;
    let ixit = engine_gate::write_ixit(scratch.path(), port)?;
    let root = engine_gate::repo_root().join("artifacts");

    let slot = JobSlot::default();
    let mine = visitor(1);
    let first = slot.allocate_id();
    let dir = veredictum_console::run_job::job_dir(scratch.path(), first);
    std::fs::create_dir_all(&dir)?;
    slot.start(
        first,
        mine,
        &engine,
        spec_for(&root, &ixit, dir),
        String::from("cap-gate-first"),
    )?;

    let second = slot.allocate_id();
    let second_dir = veredictum_console::run_job::job_dir(scratch.path(), second);
    std::fs::create_dir_all(&second_dir)?;
    let refusal = slot
        .start(
            second,
            mine,
            &engine,
            spec_for(&root, &ixit, second_dir),
            String::from("cap-gate-second"),
        )
        .expect_err("one address gets one run in flight");
    assert!(
        matches!(refusal, veredictum_console::run_job::JobError::Busy(named) if named == first),
        "the refusal names the run they already have: {refusal:?}"
    );
    assert_eq!(
        slot.view_of(second)?,
        None,
        "the refused start left nothing in the map"
    );
    assert_eq!(
        slot.in_flight_of(mine)?,
        Some(first),
        "the pre-flight and the enforced check agree"
    );
    assert_eq!(
        slot.in_flight_of(visitor(2))?,
        None,
        "another visitor is unaffected"
    );

    drop(slot.cancel(first));
    Ok(())
}

/// The #389 wall-clock cap: a run that outlives its budget is ended by the
/// console, says so rather than blaming the operator, and its partial record
/// is discarded.
///
/// Driven at an injected two seconds. Thirty minutes is the value the server
/// ships and no test may take that long, so what is proven here is the
/// mechanism that number feeds.
#[expect(
    clippy::panic_in_result_fn,
    reason = "the Book ch11 Result-returning test shape: assertions panic, plumbing propagates with ?"
)]
#[test]
fn the_wall_clock_cap_ends_a_run_and_discards_its_record() -> Result<(), Box<dyn std::error::Error>>
{
    let binary = engine_gate::gate_binary();
    if !binary.exists() {
        eprintln!(
            "SKIPPED(no engine binary at {}): build the workspace first",
            binary.display()
        );
        return Ok(());
    }
    let engine = Engine::verified(&binary)?;
    let scratch = assert_fs::TempDir::new()?;
    let port = engine_gate::slow_fixture_sut(std::time::Duration::from_secs(3))?;
    let ixit = engine_gate::write_ixit(scratch.path(), port)?;
    let root = engine_gate::repo_root().join("artifacts");

    let slot = JobSlot::with_limits(Limits {
        wall_clock: std::time::Duration::from_secs(2),
        watchdog_tick: std::time::Duration::from_millis(200),
        ..Limits::default()
    });
    let id = slot.allocate_id();
    let dir = veredictum_console::run_job::job_dir(scratch.path(), id);
    std::fs::create_dir_all(&dir)?;
    slot.start(
        id,
        visitor(1),
        &engine,
        spec_for(&root, &ixit, dir.clone()),
        String::from("cap-gate-slow"),
    )?;

    let terminal = wait_terminal_for(&slot, id, std::time::Duration::from_mins(2))
        .ok_or("the capped run never stopped")?;
    assert_eq!(
        terminal.status,
        JobStatus::Expired,
        "the cap ended it, so the screen must not read `cancelled`: {:?}",
        terminal.tail
    );
    assert!(
        terminal.finished.is_none(),
        "a capped run states no outcome"
    );
    // The partial record is discarded: what the run wrote graded nothing.
    for _ in 0..50 {
        if !dir.exists() {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(100));
    }
    assert!(
        !dir.exists(),
        "the capped run's directory survived: {}",
        dir.display()
    );
    Ok(())
}

/// The wizard's own start seam refuses a second run from one address with an
/// ANSWER naming that run, which is what lets the screen link to it.
#[expect(
    clippy::panic_in_result_fn,
    reason = "the Book ch11 Result-returning test shape: assertions panic, plumbing propagates with ?"
)]
#[test]
fn the_start_seam_answers_with_the_run_already_in_flight() -> Result<(), Box<dyn std::error::Error>>
{
    let binary = engine_gate::gate_binary();
    if !binary.exists() {
        eprintln!(
            "SKIPPED(no engine binary at {}): build the workspace first",
            binary.display()
        );
        return Ok(());
    }
    let engine = Engine::verified(&binary)?;
    let scratch = assert_fs::TempDir::new()?;
    let port = engine_gate::slow_fixture_sut(std::time::Duration::from_millis(400))?;

    let root = engine_gate::repo_root().join("artifacts");
    let state = veredictum_console::state::ConsoleState {
        root,
        specs: engine_gate::repo_root().join("specs/openehr"),
        party: engine_gate::repo_root().join("party"),
        out: scratch.path().to_path_buf(),
        catalogue: std::sync::Arc::new(Err(String::from("unused by the start seam"))),
        draft: std::sync::Arc::new(std::sync::Mutex::new(engine_gate::drafts_of(RunDraft {
            base_url: format!("http://127.0.0.1:{port}"),
            sut_name: String::from("busy-gate"),
            sut_version: String::from("0.0.0-gate"),
            auth: AuthChoice::None,
            credentials: vec![],
            probed_ok: true,
            statement_json: None,
            statement_product: None,
            filter: Some(String::from("I_EHR_SERVICE.create_ehr-main")),
            record_exchanges: false,
        }))),
        sign_key: None,
        verify_key: None,
        jobs: JobSlot::default(),
        client_ip_header: None,
        capture: false,
    };

    let who = engine_gate::gate_submitter();
    let StartOutcome::Accepted(first) =
        read::start_run_with(&state, who, &engine).map_err(|e| format!("start: {e}"))?
    else {
        panic!("the first start from an idle address is accepted");
    };
    let again = read::start_run_with(&state, who, &engine).map_err(|e| format!("start: {e}"))?;
    assert_eq!(
        again,
        StartOutcome::AlreadyInFlight(first),
        "the second start names the run they already have"
    );

    drop(state.jobs.cancel(first));
    Ok(())
}
