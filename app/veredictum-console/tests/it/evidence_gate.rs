// SPDX-FileCopyrightText: Veredictum contributors
// SPDX-License-Identifier: Apache-2.0

//! The #463 acceptance gates: a red run hands over its own evidence.
//!
//! The carving is the PINNED ENGINE's `evidence --failing`, over a real run
//! driven against the gate's fixture SUT with a Basic credential. What the
//! console adds is the offer and the route, so what is asserted here is that
//! a red run offers the bundle, that the bundle carries exchanges with the
//! credential withheld, and that a run driven without recording says so
//! rather than handing over an empty document.
//!
//! No statement is involved anywhere in this file, deliberately: sealing a
//! record needs a claim, and reading the exchanges a run recorded does not.

#![allow(
    clippy::print_stderr,
    reason = "the skip-with-reason lines ARE this gate's report, the same shape export_gate.rs uses"
)]

use std::path::Path;

use veredictum_console::engine::{Credential, Engine, RunSpec, Secret};
use veredictum_console::evidence_api::{EvidenceOffer, prepare};
use veredictum_console::run_job::{JobSlot, JobStatus};
use veredictum_console::state::ConsoleState;

use crate::engine_gate;

/// The Basic password the gate's run authenticates with — the string the
/// bundle must never carry.
const PASSWORD: &str = "evidence-gate-password";

/// The case the gate drives: one small isolated case, so the run is seconds.
const GATE_FILTER: &str = "I_EHR_SERVICE.create_ehr-main";

/// A console state over the repository's own mounts, with no signing posture:
/// nothing here seals anything.
fn state_over(out: &Path, jobs: JobSlot) -> ConsoleState {
    let root = engine_gate::repo_root().join("artifacts");
    let catalogue = veredictum::pipeline::catalogue::validate_tree(
        &root,
        Some(&engine_gate::repo_root().join("specs/openehr")),
    )
    .map_err(|e| e.to_string());
    ConsoleState {
        root,
        specs: engine_gate::repo_root().join("specs/openehr"),
        out: out.to_path_buf(),
        sign_key: None,
        verify_key: None,
        catalogue: std::sync::Arc::new(catalogue),
        draft: std::sync::Arc::new(std::sync::Mutex::new(
            veredictum_console::run_api::Drafts::new(),
        )),
        client_ip_header: None,
        jobs,
        posture: veredictum_console::posture::Posture::Local,
        rates: veredictum_console::rate_limit::RateLimiter::default(),
        capture: false,
    }
}

/// Polls the map until the NAMED job leaves `Running`, bounded.
fn wait_terminal(
    slot: &JobSlot,
    id: veredictum_console::run_job::RunId,
) -> Option<veredictum_console::run_job::JobView> {
    for _ in 0..600 {
        let view = slot.view_of(id).ok()??;
        if view.status != JobStatus::Running {
            return Some(view);
        }
        std::thread::sleep(std::time::Duration::from_millis(100));
    }
    None
}

/// Drives one credentialed run into `out`, returning the state holding it.
///
/// The fixture SUT answers every request `500`, so every row goes red, which
/// is the run this whole seam exists for.
#[expect(
    clippy::panic_in_result_fn,
    reason = "the Book ch11 Result-returning shape: the run's terminal status is asserted, plumbing propagates with ?"
)]
fn driven(
    out: &Path,
    record_exchanges: bool,
) -> Result<Option<(ConsoleState, Engine)>, Box<dyn std::error::Error>> {
    let binary = engine_gate::gate_binary();
    if !binary.exists() {
        eprintln!(
            "SKIPPED(no engine binary at {}): build the workspace first",
            binary.display()
        );
        return Ok(None);
    }
    let engine = Engine::verified(&binary)?;

    let slot = JobSlot::default();
    let state = state_over(out, slot.clone());
    let id = slot.allocate_id();
    let job_dir = veredictum_console::run_job::job_dir(out, id);
    std::fs::create_dir_all(&job_dir)?;
    let port = engine_gate::fixture_sut()?;
    let ixit = engine_gate::write_ixit(&job_dir, port)?;

    slot.start(
        id,
        engine_gate::gate_submitter(),
        &engine,
        RunSpec {
            root: state.root.clone(),
            ixit,
            out_dir: job_dir,
            sut_name: String::from("evidence-gate"),
            sut_version: String::from("0.0.0-gate"),
            statement: None,
            filter: Some(String::from(GATE_FILTER)),
            credentials: vec![
                Credential {
                    name: String::from("GATE_SUT_USER"),
                    value: Secret::new(String::from("evidence-gate-user")),
                },
                Credential {
                    name: String::from("GATE_SUT_PASS"),
                    value: Secret::new(String::from(PASSWORD)),
                },
            ],
            progress: true,
            record_exchanges,
        },
        String::from("evidence-gate"),
    )?;
    let terminal = wait_terminal(&slot, id).ok_or("the job never left Running")?;
    assert_eq!(
        terminal.status,
        JobStatus::Finished,
        "tail: {:?}",
        terminal.tail
    );
    Ok(Some((state, engine)))
}

/// A red run offers the bundle, the engine carves it, and the credential the
/// run authenticated with is nowhere in it.
#[expect(
    clippy::panic_in_result_fn,
    reason = "the Book ch11 Result-returning test shape: assertions panic, plumbing propagates with ?"
)]
#[test]
fn a_red_run_hands_over_its_exchanges_with_the_credential_withheld()
-> Result<(), Box<dyn std::error::Error>> {
    let scratch = assert_fs::TempDir::new()?;
    let Some((state, engine)) = driven(scratch.path(), true)? else {
        return Ok(());
    };
    let who = engine_gate::gate_submitter();

    assert_eq!(
        prepare::offer(&state, who)?,
        EvidenceOffer::Available,
        "the fixture answers 500, so every row is red and the wire was recorded"
    );

    let bytes = prepare::bundle_with(&state, who, &engine)?;
    let body = String::from_utf8(bytes)?;
    assert!(
        !body.contains(PASSWORD),
        "the run's credential must never reach the bundle"
    );

    let bundle: veredictum::evidence::EvidenceBundle = serde_json::from_str(&body)?;
    assert_eq!(bundle.sut.name, "evidence-gate");
    assert!(
        bundle.exchange_count() > 0,
        "an empty bundle is exactly what must be unproducible"
    );
    let authorizations: Vec<&String> = bundle
        .cases
        .iter()
        .flat_map(|case| case.exchanges.iter())
        .filter_map(|exchange| exchange.request.headers.get("authorization"))
        .collect();
    assert!(
        !authorizations.is_empty(),
        "the authenticated run sent the header, so the bundle records the name"
    );
    for value in authorizations {
        assert_eq!(
            value,
            veredictum::transcript::REDACTED,
            "the header's value is withheld"
        );
    }
    for case in &bundle.cases {
        let status = case
            .outcome
            .as_ref()
            .map(|outcome| outcome.status)
            .ok_or("the console always supplies the results record")?;
        assert!(
            matches!(
                status,
                veredictum::party::OutcomeStatus::Failed
                    | veredictum::party::OutcomeStatus::Errored
            ),
            "only red rows are offered, and {} is {}",
            case.case,
            status.token()
        );
    }
    Ok(())
}

/// A run driven without recording says so, and refuses rather than hand over
/// a document with nothing in it.
#[expect(
    clippy::panic_in_result_fn,
    reason = "the Book ch11 Result-returning test shape: assertions panic, plumbing propagates with ?"
)]
#[test]
fn an_unrecorded_run_offers_nothing_and_refuses_the_export()
-> Result<(), Box<dyn std::error::Error>> {
    let scratch = assert_fs::TempDir::new()?;
    let Some((state, engine)) = driven(scratch.path(), false)? else {
        return Ok(());
    };
    let who = engine_gate::gate_submitter();

    assert_eq!(prepare::offer(&state, who)?, EvidenceOffer::NotRecorded);
    let refusal = prepare::bundle_with(&state, who, &engine)
        .expect_err("there is no transcript to carve anything out of");
    assert!(
        refusal.contains("record"),
        "the refusal must name what is missing: {refusal}"
    );
    Ok(())
}

/// Before any run the section offers nothing at all, rather than an error.
#[expect(
    clippy::panic_in_result_fn,
    reason = "the Book ch11 Result-returning test shape: assertions panic, plumbing propagates with ?"
)]
#[test]
fn no_run_offers_nothing() -> Result<(), Box<dyn std::error::Error>> {
    let scratch = assert_fs::TempDir::new()?;
    let state = state_over(scratch.path(), JobSlot::default());
    assert_eq!(
        prepare::offer(&state, engine_gate::gate_submitter())?,
        EvidenceOffer::NoRun
    );
    Ok(())
}
