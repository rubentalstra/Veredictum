// SPDX-FileCopyrightText: Veredictum contributors
// SPDX-License-Identifier: Apache-2.0

//! The opt-in run wire transcript (#96) and the evidence bundle carved out of
//! it (#463): what `run --record-exchanges` persists beside `results.json`,
//! what a run without the flag does not, and what `veredictum evidence` will
//! and will not hand a triage.
//!
//! Every run here drives a hermetic in-process fixture SUT that answers every
//! request `500` with a fixed body, so the exchanges are deterministic and no
//! real deployment is involved. The properties under test are the artifacts':
//! they validate against their published schemas, the transcript is ordered so
//! a re-run emits the same bytes, the sealed manifest covers it, an export
//! that would carry nothing is refused, and the credential the run
//! authenticated with reaches neither file.

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
    drive_under(out_dir, ixit, recording, None)
}

/// The same run, selected under `statement` when one is supplied.
fn drive_under(
    out_dir: &Path,
    ixit: &Path,
    recording: Recording,
    statement: Option<&Path>,
) -> Result<RunOutcome, Box<dyn std::error::Error>> {
    let root = repo_root().join("artifacts");
    let request = RunRequest {
        root: &root,
        ixit,
        out_dir,
        sut_name: "transcript-gate",
        sut_version: "0.0.0-gate",
        filter: Some(FILTER),
        statement,
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

/// The run and the re-judgement of its own recording record the same
/// provenance (#461), because one constructor assembles both documents.
///
/// The two seams used to build `results.json` by hand, twice, and a member
/// added to one and missed in the other was invisible: `ambiguity_dispositions`
/// was a hardcoded empty list on both sides while the ICS answered fifteen
/// option families. Every provenance member is compared here, so adding a
/// member to one seam alone fails this test rather than shipping.
#[expect(
    clippy::panic_in_result_fn,
    reason = "the Book ch11 Result-returning test shape: assertions panic, plumbing propagates with ? (https://doc.rust-lang.org/book/ch11-01-writing-tests.html)"
)]
#[test]
fn a_run_and_a_replay_of_its_recording_record_the_same_provenance() -> Fallible {
    let scratch = assert_fs::TempDir::new()?;
    let port = fixture_sut()?;
    let ixit = write_ixit(scratch.path(), port, false)?;
    let statement = repo_root().join("fixtures/declaration/statement.json");
    let driven = drive_under(scratch.path(), &ixit, Recording::On, Some(&statement))?;
    let transcript = scratch.path().join(TRANSCRIPT_FILE);
    std::fs::write(
        &transcript,
        driven
            .transcript_document()?
            .ok_or("a recorded run against a live fixture produces a transcript")?,
    )?;

    let root = repo_root().join("artifacts");
    let rejudged = veredictum::pipeline::replay::replay_run(
        &veredictum::pipeline::replay::ReplayRequest {
            root: &root,
            ixit: &ixit,
            transcript: &transcript,
            statement: Some(&statement),
            filter: Some(FILTER),
            only: None,
        },
        &|_| {},
        &mut |_| {},
    )?;

    let run = &driven.results;
    let replay = &rejudged.results;
    assert_eq!(run.tech_profile.its, replay.tech_profile.its);
    assert_eq!(run.tech_profile.formats, replay.tech_profile.formats);
    assert_eq!(
        run.tech_profile.source,
        Some(veredictum::party::TechProfileSource::Declared),
        "the fixture declares its-rest formats, so the run read them"
    );
    assert_eq!(run.tech_profile.source, replay.tech_profile.source);
    assert_eq!(run.selection_basis, replay.selection_basis);
    assert_eq!(run.statement_digest, replay.statement_digest);
    assert!(
        !run.ambiguity_dispositions.is_empty(),
        "the fixture declaration answers option families, so the run applied dispositions"
    );
    assert_eq!(
        serde_json::to_value(&run.ambiguity_dispositions)?,
        serde_json::to_value(&replay.ambiguity_dispositions)?
    );
    assert_eq!(run.provenance_contradiction(), None);
    assert_eq!(replay.provenance_contradiction(), None);
    Ok(())
}

/// A run nothing selected records the fallback profile, says it is the
/// fallback, and applies no disposition: the empty list is the honest record of
/// a campaign that answered no option family.
#[expect(
    clippy::panic_in_result_fn,
    reason = "the Book ch11 Result-returning test shape: assertions panic, plumbing propagates with ? (https://doc.rust-lang.org/book/ch11-01-writing-tests.html)"
)]
#[test]
fn a_blind_run_records_a_defaulted_profile_and_no_disposition() -> Fallible {
    let scratch = assert_fs::TempDir::new()?;
    let port = fixture_sut()?;
    let ixit = write_ixit(scratch.path(), port, false)?;
    let driven = drive(scratch.path(), &ixit, Recording::Off)?;

    let results = &driven.results;
    assert_eq!(
        results.selection_basis,
        Some(veredictum::party::SelectionBasis::StatementBlind)
    );
    assert_eq!(
        results.tech_profile.source,
        Some(veredictum::party::TechProfileSource::Defaulted),
        "no declaration named this ITS, and the record says so"
    );
    assert_eq!(
        results.tech_profile.formats,
        veredictum::vocab::FormatName::ALL.to_vec()
    );
    assert!(
        results.ambiguity_dispositions.is_empty(),
        "a campaign nothing selected declares no option arm"
    );
    assert_eq!(results.provenance_contradiction(), None);
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

/// The committed published schema for the evidence bundle.
fn evidence_bundle_schema() -> Result<serde_json::Value, Box<dyn std::error::Error>> {
    let path = repo_root().join("schemas/evidence-bundle.schema.json");
    let body = std::fs::read_to_string(&path)?;
    Ok(serde_json::from_str(&body)?)
}

/// Runs `veredictum evidence` with the given arguments, returning its output.
fn run_evidence(args: &[&str]) -> Result<std::process::Output, std::io::Error> {
    std::process::Command::new(env!("CARGO_BIN_EXE_veredictum"))
        .arg("evidence")
        .args(args)
        .output()
}

/// Drives one recorded, credentialed run and hands back its directory.
fn recorded_run() -> Result<assert_fs::TempDir, Box<dyn std::error::Error>> {
    let scratch = assert_fs::TempDir::new()?;
    let port = fixture_sut()?;
    let ixit = write_ixit(scratch.path(), port, true)?;
    // The fixture answers 500, so every row is red: a non-zero exit is the
    // campaign's own verdict and says nothing about the export under test.
    let _clean = run_binary(scratch.path(), &ixit, &["--record-exchanges"])?;
    Ok(scratch)
}

/// A finished run's red rows export as an evidence bundle with no statement
/// anywhere in the picture, and the credential the run authenticated with
/// reaches neither the bundle nor the console.
#[expect(
    clippy::panic_in_result_fn,
    reason = "the Book ch11 Result-returning test shape: assertions panic, plumbing propagates with ? (https://doc.rust-lang.org/book/ch11-01-writing-tests.html)"
)]
#[test]
fn the_red_rows_of_a_credentialed_run_export_without_a_statement() -> Fallible {
    let scratch = recorded_run()?;
    assert!(
        !scratch.path().join("statement.json").exists(),
        "the run was driven with no claim at all, which is the point"
    );

    let bundle_path = scratch.path().join(veredictum::evidence::EVIDENCE_FILE);
    let output = run_evidence(&[
        "--transcript",
        &scratch.path().join(TRANSCRIPT_FILE).display().to_string(),
        "--results",
        &scratch.path().join("results.json").display().to_string(),
        "--failing",
        "--out",
        &bundle_path.display().to_string(),
    ])?;
    assert!(
        output.status.success(),
        "the export was refused: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let body = std::fs::read_to_string(&bundle_path)?;
    assert!(body.ends_with('\n'), "documents end with a newline");
    assert!(
        !body.contains(PASSWORD),
        "the run's credential must never reach the bundle"
    );
    assert!(
        !String::from_utf8_lossy(&output.stdout).contains(PASSWORD)
            && !String::from_utf8_lossy(&output.stderr).contains(PASSWORD),
        "the run's credential must never reach the console either"
    );

    let value: serde_json::Value = serde_json::from_str(&body)?;
    let validator = jsonschema::validator_for(&evidence_bundle_schema()?)?;
    if let Some(finding) = validator.iter_errors(&value).next() {
        panic!(
            "the emitted bundle fails its published schema at {}: {finding}",
            finding.instance_path()
        );
    }

    let bundle: veredictum::evidence::EvidenceBundle = serde_json::from_str(&body)?;
    assert_eq!(bundle.sut.name, "transcript-gate");
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
        assert_eq!(value, REDACTED, "the header's value is withheld");
    }
    for case in &bundle.cases {
        let status = case
            .outcome
            .as_ref()
            .map(|outcome| outcome.status)
            .ok_or("--results was supplied, so every case carries its row")?;
        assert!(
            matches!(
                status,
                veredictum::party::OutcomeStatus::Failed
                    | veredictum::party::OutcomeStatus::Errored
            ),
            "--failing selects red rows only, and {} is {}",
            case.case,
            status.token()
        );
    }
    Ok(())
}

/// An export naming a case set the run never recorded is refused, names both
/// sides of the mismatch, and writes no file: a bundle of the right shape
/// with nothing in it must be unproducible.
#[expect(
    clippy::panic_in_result_fn,
    reason = "the Book ch11 Result-returning test shape: assertions panic, plumbing propagates with ? (https://doc.rust-lang.org/book/ch11-01-writing-tests.html)"
)]
#[test]
fn an_export_matching_nothing_is_refused_and_writes_no_file() -> Fallible {
    let scratch = recorded_run()?;
    let bundle_path = scratch.path().join("nothing.json");
    let output = run_evidence(&[
        "--transcript",
        &scratch.path().join(TRANSCRIPT_FILE).display().to_string(),
        "--only",
        "I_EHR_SERVICE.no_such_case-main",
        "--out",
        &bundle_path.display().to_string(),
    ])?;

    assert!(
        !output.status.success(),
        "an export that would carry nothing must fail loudly"
    );
    assert!(
        !bundle_path.exists(),
        "the refusal writes no document at all"
    );
    let diagnostic = String::from_utf8_lossy(&output.stderr);
    assert!(
        diagnostic.contains("no_such_case"),
        "the refusal names what was asked for: {diagnostic}"
    );
    assert!(
        diagnostic.contains(FILTER),
        "the refusal names what the transcript actually carries: {diagnostic}"
    );
    Ok(())
}
