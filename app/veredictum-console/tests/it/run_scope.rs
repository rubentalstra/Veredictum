// SPDX-FileCopyrightText: Veredictum contributors
// SPDX-License-Identifier: Apache-2.0

//! The #65 acceptance gates: the scope preview equals what the engine then
//! actually processes, and the client-safe draft view carries no secret.

use veredictum_console::engine::{Credential, Engine, Secret};
use veredictum_console::run_api::{AuthChoice, RunDraft};

use crate::engine_gate;

/// The filter the equality gate drives: a handful of cases, so the run is
/// seconds.
const SCOPE_FILTER: &str = "I_EHR_SERVICE.create_ehr";

#[expect(
    clippy::panic_in_result_fn,
    reason = "the Book ch11 Result-returning test shape: assertions panic, plumbing propagates with ? (.claude/rules/testing.md)"
)]
#[test]
fn the_scope_preview_counts_what_the_engine_processes() -> Result<(), Box<dyn std::error::Error>> {
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

    let root = engine_gate::repo_root().join("artifacts");
    let specs = engine_gate::repo_root().join("specs/openehr");
    let state = veredictum_console::state::ConsoleState {
        root: root.clone(),
        specs,
        party: engine_gate::repo_root().join("party"),
        catalogue: std::sync::Arc::new(
            veredictum::pipeline::catalogue::validate_tree(&root, None).map_err(|e| e.to_string()),
        ),
        draft: std::sync::Arc::new(std::sync::Mutex::new(None)),
    };
    let preview = veredictum_console::run_api::read::scope_preview(&state, SCOPE_FILTER)
        .map_err(|e| format!("preview: {e}"))?;

    // The engine drives the same scope against the deterministic fixture SUT;
    // every case in scope lands as an outcome or a recorded exception.
    let scratch = assert_fs::TempDir::new()?;
    let port = engine_gate::fixture_sut()?;
    let ixit = engine_gate::write_ixit(scratch.path(), port)?;
    let out = scratch.path().join("scope-run");
    std::fs::create_dir_all(&out)?;
    let finished = engine.run(
        &veredictum_console::engine::RunSpec {
            root,
            ixit,
            out_dir: out.clone(),
            sut_name: String::from("scope-gate"),
            sut_version: String::from("0.0.0-gate"),
            statement: None,
            filter: Some(String::from(SCOPE_FILTER)),
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
        },
        |_line| {},
    )?;
    let exceptions: Vec<serde_json::Value> =
        serde_json::from_str(&std::fs::read_to_string(&finished.exceptions_path)?)?;
    // Outcome records are per case × FORMAT, so the case-level scope is the
    // distinct id set across outcomes and exceptions.
    let mut processed: std::collections::BTreeSet<String> = finished
        .results
        .outcomes
        .iter()
        .map(|outcome| outcome.case.to_string())
        .collect();
    for exception in &exceptions {
        if let Some(case) = exception.get("case").and_then(|c| c.as_str()) {
            processed.insert(case.to_owned());
        }
    }
    assert_eq!(
        u64::try_from(processed.len())?,
        preview.total,
        "the preview promised a scope the engine did not process"
    );
    assert!(preview.total > 0, "the gate filter selected nothing at all");
    Ok(())
}

#[expect(
    clippy::panic_in_result_fn,
    reason = "the Book ch11 Result-returning test shape: assertions panic, plumbing propagates with ? (.claude/rules/testing.md)"
)]
#[test]
fn the_draft_view_carries_no_secret() -> Result<(), Box<dyn std::error::Error>> {
    let state = veredictum_console::state::ConsoleState {
        root: "artifacts".into(),
        specs: "specs/openehr".into(),
        party: "party".into(),
        catalogue: std::sync::Arc::new(Err(String::from("unused"))),
        draft: std::sync::Arc::new(std::sync::Mutex::new(None)),
    };
    veredictum_console::run_api::read::save_connection(
        &state,
        RunDraft {
            base_url: String::from("http://cdr.example"),
            sut_name: String::from("x"),
            sut_version: String::from("y"),
            auth: AuthChoice::Basic,
            credentials: vec![Credential {
                name: String::from("CONSOLE_SUT_PASS"),
                value: Secret::new(String::from("hunter2-super-secret")),
            }],
            probed_ok: true,
            statement: None,
            filter: None,
        },
    )
    .map_err(|e| format!("save: {e}"))?;
    let view =
        veredictum_console::run_api::read::draft_view(&state).ok_or("the draft must read back")?;
    let serialized = serde_json::to_string(&view)?;
    assert!(
        !serialized.contains("hunter2"),
        "a secret value reached the client-safe view: {serialized}"
    );
    // The debug rendering of the whole draft redacts too.
    let debugged = format!("{:?}", state.draft.lock().map_err(|e| e.to_string())?);
    assert!(
        !debugged.contains("hunter2"),
        "a secret value reached a Debug rendering: {debugged}"
    );
    Ok(())
}
