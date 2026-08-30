// SPDX-FileCopyrightText: Veredictum contributors
// SPDX-License-Identifier: Apache-2.0

//! The start-the-run seam: the console's most security-relevant server-side
//! write.
//!
//! `run_api::read::start_run` writes the ixit into the job's own directory,
//! invalidates any export sealed over that directory (#68), and MOVES the
//! drafted credentials into the spawned run's environment. The last one is the
//! property the whole credential posture rests on, so it is proved end to end:
//! the fixture SUT records the `Authorization` header the engine sent, and the
//! header must carry the values the draft held. Nothing else may: not the
//! ixit, not any file under the job directory, not the client-safe draft view.

#![allow(
    clippy::print_stderr,
    reason = "the skip-with-reason lines ARE this gate's report, the same shape run_live.rs uses"
)]

use std::io::{Read as _, Write as _};
use std::sync::{Arc, Mutex};

use veredictum_console::engine::{Credential, Engine, Secret};
use veredictum_console::run_api::{AuthChoice, RunDraft};
use veredictum_console::run_job::{JobSlot, JobStatus};
use veredictum_console::state::ConsoleState;

use crate::engine_gate;

/// The case the gate drives: one small isolated case, so the run is seconds.
const FILTER: &str = "I_EHR_SERVICE.create_ehr-main";

/// The drafted Basic credentials. Distinctive enough that a grep over the job
/// directory proves absence rather than coincidence.
const SUT_USER: &str = "console-operator";
const SUT_PASS: &str = "hunter2-never-on-disk";

/// The RFC 4648 alphabet (<https://www.rfc-editor.org/rfc/rfc4648#section-4>),
/// so the expected `Authorization` value is derived here rather than pasted as
/// an opaque literal nobody can check.
const ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

/// Encodes bytes as standard base64 with padding.
fn base64(bytes: &[u8]) -> String {
    let mut out = String::new();
    for chunk in bytes.chunks(3) {
        let (b0, b1, b2) = (
            u32::from(*chunk.first().unwrap_or(&0)),
            u32::from(*chunk.get(1).unwrap_or(&0)),
            u32::from(*chunk.get(2).unwrap_or(&0)),
        );
        let packed = (b0 << 16) | (b1 << 8) | b2;
        for (index, shift) in [18_u32, 12, 6, 0].into_iter().enumerate() {
            if index <= chunk.len() {
                let digit = usize::try_from((packed >> shift) & 0x3f).unwrap_or(0);
                out.push(char::from(*ALPHABET.get(digit).unwrap_or(&b'A')));
            } else {
                out.push('=');
            }
        }
    }
    out
}

/// The `Authorization` headers one fixture SUT was sent, shared with the
/// thread that serves it.
type SeenHeaders = Arc<Mutex<Vec<String>>>;

/// A fixture SUT that answers every request `500` deterministically and keeps
/// every `Authorization` header it was sent.
///
/// The recorded headers are what proves the credential reached the CHILD
/// process: the console never speaks to a CDR, so an `Authorization` value
/// arriving here can only have come from the spawned engine's environment.
fn recording_sut() -> Result<(u16, SeenHeaders), std::io::Error> {
    let listener = std::net::TcpListener::bind("127.0.0.1:0")?;
    let port = listener.local_addr()?.port();
    let seen: SeenHeaders = Arc::new(Mutex::new(Vec::new()));
    let recorder = Arc::clone(&seen);
    std::thread::spawn(move || {
        for stream in listener.incoming() {
            let Ok(mut stream) = stream else { continue };
            let mut scratch = [0_u8; 4096];
            let read = stream.read(&mut scratch).unwrap_or(0);
            let request = String::from_utf8_lossy(scratch.get(..read).unwrap_or(&[])).into_owned();
            for line in request.lines() {
                if let Some((name, value)) = line.split_once(':')
                    && name.eq_ignore_ascii_case("authorization")
                    && let Ok(mut guard) = recorder.lock()
                {
                    guard.push(value.trim().to_owned());
                }
            }
            let _write = stream.write_all(
                b"HTTP/1.1 500 Internal Server Error\r\ncontent-length: 2\r\nconnection: close\r\n\r\nno",
            );
        }
    });
    Ok((port, seen))
}

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

/// Every file under `dir`, recursively.
fn files_under(dir: &std::path::Path) -> Result<Vec<std::path::PathBuf>, std::io::Error> {
    let mut found = Vec::new();
    for entry in std::fs::read_dir(dir)? {
        let path = entry?.path();
        if path.is_dir() {
            found.extend(files_under(&path)?);
        } else {
            found.push(path);
        }
    }
    Ok(found)
}

#[expect(
    clippy::panic_in_result_fn,
    reason = "the Book ch11 Result-returning test shape: assertions panic, plumbing propagates with ? (https://doc.rust-lang.org/book/ch11-01-writing-tests.html)"
)]
#[expect(
    clippy::too_many_lines,
    reason = "one gate walks the whole write — ixit, invalidation, credential move, the driven exchange — and splitting it would hide the chain it exists to assert"
)]
#[test]
fn starting_a_run_writes_the_ixit_invalidates_the_export_and_moves_the_credentials()
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
    let out = scratch.path().join("out");
    std::fs::create_dir_all(&out)?;
    let (port, seen) = recording_sut()?;

    let claim =
        std::fs::read_to_string(engine_gate::repo_root().join("party/ehrbase/statement.json"))?;
    let state = ConsoleState {
        root: engine_gate::repo_root().join("artifacts"),
        specs: engine_gate::repo_root().join("specs/openehr"),
        party: engine_gate::repo_root().join("party"),
        out: out.clone(),
        sign_key: None,
        verify_key: None,
        // The start seam reads no catalogue: the engine reads the mounted root
        // itself, which is what the engine boundary exists to keep true.
        catalogue: Arc::new(Err(String::from("unused by the start seam"))),
        draft: Arc::new(Mutex::new(Some(RunDraft {
            base_url: format!("http://127.0.0.1:{port}"),
            sut_name: String::from("start-gate"),
            sut_version: String::from("0.0.0-gate"),
            auth: AuthChoice::Basic,
            credentials: vec![
                Credential {
                    name: String::from("CONSOLE_SUT_USER"),
                    value: Secret::new(String::from(SUT_USER)),
                },
                Credential {
                    name: String::from("CONSOLE_SUT_PASS"),
                    value: Secret::new(String::from(SUT_PASS)),
                },
            ],
            probed_ok: true,
            statement_json: Some(claim.clone()),
            statement_product: Some(String::from("EHRbase 2.34.0")),
            filter: Some(String::from(FILTER)),
            record_exchanges: false,
        }))),
        jobs: JobSlot::default(),
        capture: false,
    };

    let id = veredictum_console::run_api::read::start_run_with(&state, &engine)
        .map_err(|e| format!("start: {e}"))?;
    // The id the start answers with is the run's address: it reads back from
    // its own spelling, which is what `/run/live/{run_id}` relies on (#386).
    assert_eq!(
        id.to_string()
            .parse::<veredictum_console::run_job::RunId>()?,
        id
    );
    let job_dir = veredictum_console::run_job::job_dir(&out, id);
    assert!(
        job_dir.is_dir(),
        "the run's own directory carries the id: {}",
        job_dir.display()
    );

    // The seal lives inside the job directory, so the seam that creates it
    // guarantees no bundle is in it before the engine writes a single row.
    assert!(
        !job_dir.join("export").exists(),
        "a run started into a directory holding a sealed record"
    );

    // The ixit names the environment variables and carries no value.
    let ixit = std::fs::read_to_string(job_dir.join("ixit.json"))?;
    assert!(ixit.contains("CONSOLE_SUT_USER"), "{ixit}");
    assert!(ixit.contains("CONSOLE_SUT_PASS"), "{ixit}");
    assert!(
        !ixit.contains(SUT_PASS),
        "a credential VALUE reached the ixit"
    );
    let parsed: Result<veredictum::ixit::Ixit, _> = serde_json::from_str(&ixit);
    assert!(
        parsed.is_ok(),
        "the written ixit does not parse: {parsed:?}"
    );

    // The claim travels with the run, byte for byte.
    assert_eq!(
        std::fs::read_to_string(job_dir.join("statement.json"))?,
        claim
    );

    // The credentials MOVED: the draft that answered the browser a moment ago
    // no longer holds them, and everything else about it survives.
    {
        let guard = state.draft.lock().map_err(|e| e.to_string())?;
        let draft = guard.as_ref().ok_or("the draft survives the start")?;
        assert!(
            draft.credentials.is_empty(),
            "the credentials were copied instead of moved: {:?}",
            draft.credentials
        );
        assert_eq!(draft.sut_name, "start-gate");
    }
    let view = veredictum_console::run_api::read::draft_view(&state)
        .ok_or("the client-safe view still reads back")?;
    let wire = serde_json::to_string(&view)?;
    assert!(
        !wire.contains(SUT_PASS),
        "the wire view carries a secret: {wire}"
    );

    let terminal = wait_terminal(&state.jobs).ok_or("the job never left Running")?;
    assert_eq!(
        terminal.status,
        JobStatus::Finished,
        "tail: {:?}",
        terminal.tail
    );

    // The proof the value reached the CHILD: the fixture saw the drafted
    // credentials on the wire, and only the spawned engine could have sent
    // them.
    let expected = format!(
        "Basic {}",
        base64(format!("{SUT_USER}:{SUT_PASS}").as_bytes())
    );
    let headers = seen.lock().map_err(|e| e.to_string())?.clone();
    assert!(
        headers.contains(&expected),
        "the engine never sent the drafted credentials; saw {headers:?}"
    );

    // And nothing wrote them down: not the ixit, not the record, not the
    // exception document.
    for file in files_under(&job_dir)? {
        let body = std::fs::read(&file)?;
        assert!(
            !String::from_utf8_lossy(&body).contains(SUT_PASS),
            "{} carries the credential value",
            file.display()
        );
    }
    Ok(())
}

/// An unconnected wizard is refused for what it is, before the engine is even
/// looked for: a host with no engine mounted must not turn "you have not
/// connected yet" into "no engine binary".
#[expect(
    clippy::panic_in_result_fn,
    reason = "the Book ch11 Result-returning test shape: assertions panic, plumbing propagates with ? (https://doc.rust-lang.org/book/ch11-01-writing-tests.html)"
)]
#[test]
fn starting_without_a_draft_is_refused_by_name() -> Result<(), Box<dyn std::error::Error>> {
    let scratch = assert_fs::TempDir::new()?;
    let state = ConsoleState {
        root: engine_gate::repo_root().join("artifacts"),
        specs: engine_gate::repo_root().join("specs/openehr"),
        party: engine_gate::repo_root().join("party"),
        out: scratch.path().to_path_buf(),
        sign_key: None,
        verify_key: None,
        catalogue: Arc::new(Err(String::from("unused by the start seam"))),
        draft: Arc::new(Mutex::new(None)),
        jobs: JobSlot::default(),
        capture: false,
    };
    let refusal =
        veredictum_console::run_api::read::start_run(&state).expect_err("no draft must refuse");
    assert!(refusal.contains("no connection draft"), "{refusal}");
    // And nothing was written: no job directory, no ixit.
    assert_eq!(
        std::fs::read_dir(scratch.path())?.count(),
        0,
        "the refusal wrote something"
    );
    Ok(())
}

/// The RFC 4648 §10 test vectors
/// (<https://www.rfc-editor.org/rfc/rfc4648#section-10>), so the expected
/// `Authorization` value above is derived by an encoder this suite checked.
#[test]
fn the_encoder_matches_the_rfc_vectors() {
    assert_eq!(base64(b""), "");
    assert_eq!(base64(b"f"), "Zg==");
    assert_eq!(base64(b"fo"), "Zm8=");
    assert_eq!(base64(b"foo"), "Zm9v");
    assert_eq!(base64(b"foob"), "Zm9vYg==");
    assert_eq!(base64(b"fooba"), "Zm9vYmE=");
    assert_eq!(base64(b"foobar"), "Zm9vYmFy");
}
