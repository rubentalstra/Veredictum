// SPDX-FileCopyrightText: Veredictum contributors
// SPDX-License-Identifier: Apache-2.0

//! The opt-in run wire transcript (#96): what `run --record-exchanges`
//! persists beside `results.json`, and what a run without the flag does not.
//!
//! Every run here drives a hermetic in-process fixture SUT that answers every
//! request `500` with a fixed body, so the exchanges are deterministic and no
//! real deployment is involved. The properties under test are the artifact's:
//! it validates against its published schema, it is ordered so a re-run emits
//! the same bytes, the sealed manifest covers it, and the credential the run
//! authenticated with never reaches the file.

use std::io::{Read as _, Write as _};
use std::path::{Path, PathBuf};

use veredictum::pipeline::conformance::{RunOutcome, RunRequest, execute_run};
use veredictum::transcript::{REDACTED, Recording, TRANSCRIPT_FILE};

/// Anything a seam or a fixture read can fail with, so a test body
/// propagates plumbing failures with `?`
/// (<https://doc.rust-lang.org/book/ch11-01-writing-tests.html>).
type Fallible = Result<(), Box<dyn std::error::Error>>;

/// The one small isolated case every run here drives, so a run is seconds.
const FILTER: &str = "I_EHR_SERVICE.create_ehr-main";

/// The basic credential the signed run authenticates with — the string the
/// transcript must never carry.
const PASSWORD: &str = "transcript-gate-password";

fn repo_root() -> PathBuf {
    PathBuf::from(concat!(env!("CARGO_MANIFEST_DIR"), "/../.."))
}

/// A fixture SUT answering every request `500` with a fixed body, on an
/// ephemeral loopback port. The thread ends with the test process.
fn fixture_sut() -> Result<u16, std::io::Error> {
    let listener = std::net::TcpListener::bind("127.0.0.1:0")?;
    let port = listener.local_addr()?.port();
    std::thread::spawn(move || {
        for stream in listener.incoming() {
            let Ok(mut stream) = stream else { continue };
            let mut scratch = [0_u8; 4096];
            let _bytes_read = stream.read(&mut scratch);
            let _write = stream.write_all(
                b"HTTP/1.1 500 Internal Server Error\r\ncontent-length: 2\r\nconnection: close\r\n\r\nno",
            );
        }
    });
    Ok(port)
}

/// Writes an ixit whose every instance is the fixture SUT.
///
/// Raw bytes on purpose: the lib's `Ixit` is deserialize-only, so authoring
/// one from a typed value is not possible.
fn write_ixit(dir: &Path, port: u16, authenticated: bool) -> Result<PathBuf, std::io::Error> {
    let auth = if authenticated {
        r#"{ "mode": "basic", "user_env": "TRANSCRIPT_GATE_USER", "password_env": "TRANSCRIPT_GATE_PASS" }"#
    } else {
        r#"{ "mode": "none" }"#
    };
    let document = format!(
        r#"{{
  "instances": {{
    "sut": {{ "base_url": "http://127.0.0.1:{port}", "auth": {auth} }},
    "admin": {{ "base_url": "http://127.0.0.1:{port}", "auth": {auth} }},
    "unauthenticated": {{ "base_url": "http://127.0.0.1:{port}", "auth": {{ "mode": "none" }} }}
  }}
}}
"#
    );
    let path = dir.join("ixit.json");
    std::fs::write(&path, document)?;
    Ok(path)
}

/// Drives the filtered case against the fixture SUT, in process.
fn drive(
    out_dir: &Path,
    ixit: &Path,
    recording: Recording,
) -> Result<RunOutcome, Box<dyn std::error::Error>> {
    let root = repo_root().join("artifacts");
    let request = RunRequest {
        root: &root,
        ixit,
        out_dir,
        sut_name: "transcript-gate",
        sut_version: "0.0.0-gate",
        filter: Some(FILTER),
        statement: None,
        recording,
    };
    let outcome = execute_run(&request, &|_| {}, &mut |_| {})?;
    Ok(outcome)
}

/// The committed published schema for the run transcript.
fn run_transcript_schema() -> Result<serde_json::Value, Box<dyn std::error::Error>> {
    let path = repo_root().join("schemas/run-transcript.schema.json");
    let body = std::fs::read_to_string(&path)?;
    Ok(serde_json::from_str(&body)?)
}

/// A recorded run writes a transcript that satisfies its published schema,
/// carries the wire on both sides, and orders itself so a re-run of the same
/// campaign emits the same bytes.
#[expect(
    clippy::panic_in_result_fn,
    reason = "the Book ch11 Result-returning test shape: assertions panic, plumbing propagates with ? (https://doc.rust-lang.org/book/ch11-01-writing-tests.html)"
)]
#[test]
fn a_recorded_run_emits_a_schema_valid_ordered_transcript() -> Fallible {
    let scratch = assert_fs::TempDir::new()?;
    let port = fixture_sut()?;
    let ixit = write_ixit(scratch.path(), port, false)?;
    let outcome = drive(scratch.path(), &ixit, Recording::On)?;

    let document = outcome
        .transcript_document()?
        .ok_or("a recorded run against a live fixture must produce a transcript")?;
    assert!(document.ends_with('\n'), "documents end with a newline");
    assert_eq!(
        outcome.transcript_path,
        Some(scratch.path().join(TRANSCRIPT_FILE)),
        "the transcript belongs beside the results record"
    );

    let value: serde_json::Value = serde_json::from_str(&document)?;
    let validator = jsonschema::validator_for(&run_transcript_schema()?)?;
    if let Some(finding) = validator.iter_errors(&value).next() {
        panic!(
            "the emitted transcript fails its published schema at {}: {finding}",
            finding.instance_path()
        );
    }

    let transcript: veredictum::transcript::RunTranscript = serde_json::from_str(&document)?;
    assert_eq!(transcript.sut.name, "transcript-gate");
    assert!(
        transcript.exchange_count() > 0,
        "the run drove exchanges, so the transcript carries them"
    );
    let ids: Vec<&str> = transcript
        .cases
        .iter()
        .map(|case| case.case.as_str())
        .collect();
    let mut sorted = ids.clone();
    sorted.sort_unstable();
    assert_eq!(ids, sorted, "cases are ordered by id");
    for case in &transcript.cases {
        let seqs: Vec<u32> = case.exchanges.iter().map(|exchange| exchange.seq).collect();
        let expected: Vec<u32> = (1..=u32::try_from(case.exchanges.len())?).collect();
        assert_eq!(
            seqs, expected,
            "{} renumbers from 1 in send order",
            case.case
        );
        for exchange in &case.exchanges {
            assert_eq!(exchange.response.status, 500, "the fixture answers 500");
            assert!(
                !exchange.request.url.is_empty() && !exchange.request.method.is_empty(),
                "the request line is recorded"
            );
        }
    }

    // Rendering the same outcome twice is byte-identical: the ordering is a
    // property of the document, not of the moment it was written.
    assert_eq!(outcome.transcript_document()?, Some(document));
    Ok(())
}

/// Without the flag there is no transcript at all: no document, no path, and
/// nothing for a caller to write.
#[expect(
    clippy::panic_in_result_fn,
    reason = "the Book ch11 Result-returning test shape: assertions panic, plumbing propagates with ? (https://doc.rust-lang.org/book/ch11-01-writing-tests.html)"
)]
#[test]
fn an_unrecorded_run_produces_no_transcript() -> Fallible {
    let scratch = assert_fs::TempDir::new()?;
    let port = fixture_sut()?;
    let ixit = write_ixit(scratch.path(), port, false)?;
    let outcome = drive(scratch.path(), &ixit, Recording::Off)?;

    assert!(
        outcome.report.transcripts.is_empty(),
        "recording is off, so the driver kept nothing"
    );
    assert_eq!(outcome.transcript_document()?, None);
    assert_eq!(outcome.transcript_path, None);
    assert!(
        !outcome.report.records.is_empty(),
        "the run still drove the case and recorded its outcome"
    );
    Ok(())
}

/// Runs the instrument binary, returning its exit status.
fn run_binary(out_dir: &Path, ixit: &Path, extra: &[&str]) -> Result<bool, std::io::Error> {
    let status = std::process::Command::new(env!("CARGO_BIN_EXE_veredictum"))
        .args([
            "run",
            "--root",
            &repo_root().join("artifacts").display().to_string(),
            "--ixit",
            &ixit.display().to_string(),
            "--out",
            &out_dir.display().to_string(),
            "--sut-name",
            "transcript-gate",
            "--sut-version",
            "0.0.0-gate",
            "--filter",
            FILTER,
        ])
        .args(extra)
        .env("TRANSCRIPT_GATE_USER", "transcript-gate-user")
        .env("TRANSCRIPT_GATE_PASS", PASSWORD)
        .status()?;
    Ok(status.success())
}

/// The signed bundle covers the transcript, and the credential the run
/// authenticated with is nowhere in it.
#[expect(
    clippy::panic_in_result_fn,
    reason = "the Book ch11 Result-returning test shape: assertions panic, plumbing propagates with ? (https://doc.rust-lang.org/book/ch11-01-writing-tests.html)"
)]
#[test]
fn a_sealed_recorded_run_covers_the_transcript_and_withholds_the_credential() -> Fallible {
    let scratch = assert_fs::TempDir::new()?;
    let port = fixture_sut()?;
    let ixit = write_ixit(scratch.path(), port, true)?;
    let key = repo_root().join("artifacts/corpus/keys/cnf-signing.sec.asc");
    // The fixture answers 500, so every row fails: a non-zero exit is the
    // campaign's own verdict and says nothing about the emission under test.
    let _clean = run_binary(
        scratch.path(),
        &ixit,
        &[
            "--record-exchanges",
            "--sign-key",
            &key.display().to_string(),
        ],
    )?;

    let transcript_path = scratch.path().join(TRANSCRIPT_FILE);
    let body = std::fs::read_to_string(&transcript_path)?;
    assert!(
        !body.contains(PASSWORD),
        "the run's credential must never reach the artifact"
    );
    let transcript: veredictum::transcript::RunTranscript = serde_json::from_str(&body)?;
    let authorizations: Vec<&String> = transcript
        .cases
        .iter()
        .flat_map(|case| case.exchanges.iter())
        .filter_map(|exchange| exchange.request.headers.get("authorization"))
        .collect();
    assert!(
        !authorizations.is_empty(),
        "the authenticated run sent the header, so the transcript records the name"
    );
    for value in authorizations {
        assert_eq!(value, REDACTED, "the header's value is withheld");
    }

    let public = repo_root().join("artifacts/corpus/keys/cnf-signing.pub.asc");
    let verification = veredictum::record::verify_bundle(scratch.path(), &public)?;
    assert!(
        verification.is_clean(),
        "the sealed bundle must verify: {:?}",
        verification.findings()
    );
    assert!(
        verification
            .files
            .iter()
            .any(|file| file.name == TRANSCRIPT_FILE),
        "the manifest names the transcript: {:?}",
        verification
            .files
            .iter()
            .map(|file| file.name.clone())
            .collect::<Vec<_>>()
    );
    Ok(())
}

/// A run without the flag writes no transcript file, and its manifest names
/// none — the refusal path, checked at the surface an operator sees.
#[expect(
    clippy::panic_in_result_fn,
    reason = "the Book ch11 Result-returning test shape: assertions panic, plumbing propagates with ? (https://doc.rust-lang.org/book/ch11-01-writing-tests.html)"
)]
#[test]
fn an_unrecorded_sealed_run_names_no_transcript() -> Fallible {
    let scratch = assert_fs::TempDir::new()?;
    let port = fixture_sut()?;
    let ixit = write_ixit(scratch.path(), port, true)?;
    let key = repo_root().join("artifacts/corpus/keys/cnf-signing.sec.asc");
    let _clean = run_binary(
        scratch.path(),
        &ixit,
        &["--sign-key", &key.display().to_string()],
    )?;

    assert!(
        !scratch.path().join(TRANSCRIPT_FILE).exists(),
        "no flag, no transcript file"
    );
    let public = repo_root().join("artifacts/corpus/keys/cnf-signing.pub.asc");
    let verification = veredictum::record::verify_bundle(scratch.path(), &public)?;
    assert!(
        verification.is_clean(),
        "the sealed bundle must verify: {:?}",
        verification.findings()
    );
    assert!(
        !verification
            .files
            .iter()
            .any(|file| file.name == TRANSCRIPT_FILE),
        "an unrecorded run's manifest names no transcript"
    );
    Ok(())
}
