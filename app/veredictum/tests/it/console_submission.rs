// SPDX-FileCopyrightText: Veredictum contributors
// SPDX-License-Identifier: Apache-2.0

//! The console lane end to end, minus GitHub (#392).
//!
//! A `console` submission arrives with no provenance block, because the
//! instrument that produced it is not allowed to state its own. What turns it
//! into a publishable entry is this repository: the judgement is recomputed
//! from the recorded exchanges, the record is sealed with the registry key,
//! and the provenance block is written from what the lane observed.
//!
//! Everything here is real except the two facts a workflow supplies (which
//! identity opened the submission, and on which branch) and the key, which is
//! the committed test keypair rather than the registry's own. The run is
//! driven against a hermetic in-process fixture that answers every request
//! `500`, so the outcomes are deterministic and no deployment is involved.

use std::io::{Read as _, Write as _};
use std::path::{Path, PathBuf};
use std::process::Command;

use veredictum::pipeline::conformance::{RunRequest, execute_run};
use veredictum::registry::{
    Provenance, REGISTRY_SCHEMA_VERSION, RULES_VERSION, RegistryEntry, Tier, entry_defects,
};
use veredictum::transcript::Recording;

/// Anything a seam, a fixture or a subprocess can fail with.
type Fallible = Result<(), Box<dyn std::error::Error>>;

/// The one small isolated case the submission is built over.
const FILTER: &str = "I_EHR_SERVICE.create_ehr-main";

/// The system and entry the fixture submission is filed under.
const SYSTEM: &str = "gate";
const ENTRY_ID: &str = "2026-01-02-gate-console";

fn repo_root() -> PathBuf {
    PathBuf::from(concat!(env!("CARGO_MANIFEST_DIR"), "/../.."))
}

/// A fixture SUT answering every request `500`, on an ephemeral loopback port.
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

/// An ixit whose every instance is the fixture SUT.
fn write_ixit(path: &Path, port: u16) -> Result<(), std::io::Error> {
    std::fs::write(
        path,
        format!(
            r#"{{
  "instances": {{
    "sut": {{ "base_url": "http://127.0.0.1:{port}", "auth": {{ "mode": "none" }} }},
    "admin": {{ "base_url": "http://127.0.0.1:{port}", "auth": {{ "mode": "none" }} }},
    "unauthenticated": {{ "base_url": "http://127.0.0.1:{port}", "auth": {{ "mode": "none" }} }}
  }}
}}
"#
        ),
    )
}

/// The SHA-256 of a committed file, as an entry pins it.
fn digest(path: &Path) -> Result<String, Box<dyn std::error::Error>> {
    use sha2::{Digest as _, Sha256};
    use std::fmt::Write as _;
    let bytes = std::fs::read(path)?;
    Ok(Sha256::digest(&bytes)
        .iter()
        .fold(String::new(), |mut out, byte| {
            let _written = write!(out, "{byte:02x}");
            out
        }))
}

/// One artifact reference, as the submission writes it.
fn artifact(
    role: &str,
    name: &str,
    tree: &Path,
) -> Result<serde_json::Value, Box<dyn std::error::Error>> {
    let path = format!("registry/records/{SYSTEM}/{ENTRY_ID}/{name}");
    Ok(serde_json::json!({
        "role": role,
        "path": path,
        "sha256": digest(&tree.join(&path))?
    }))
}

/// The entry as the instrument submits it: every disclosure the rules make
/// mandatory, the five artifacts a re-derivation reads, and no provenance
/// block at all — that one is this repository's to write.
fn write_submitted_entry(tree: &Path, port: u16) -> Result<PathBuf, Box<dyn std::error::Error>> {
    let entry_path = tree.join(format!(
        "registry/entries/conformance/{SYSTEM}/{ENTRY_ID}.json"
    ));
    let submitted = serde_json::json!({
        "registry_schema_version": REGISTRY_SCHEMA_VERSION,
        "entry_id": ENTRY_ID,
        "rules_version": RULES_VERSION,
        "submitter": {
            "name": "Gate Author",
            "contact": "https://example.invalid",
            "relationship": "independent"
        },
        "subject": {
            "system": SYSTEM,
            "display_name": "Gate CDR",
            "version": "0.0.0-gate",
            "deployment": {
                "kind": "hosted-endpoint",
                "endpoint": format!("http://127.0.0.1:{port}"),
                "reproduction_authorized": false
            }
        },
        "disclosure": {
            "instrument_version": env!("CARGO_PKG_VERSION"),
            "run_started_at": "2026-01-02T00:00:00Z",
            "environment": {"os": "linux", "arch": "x86_64", "host_class": "a fixture host"},
            "sut_configuration": "no authentication, a fixture that refuses everything",
            "conflict_of_interest": "the submitter maintains the instrument"
        },
        "result": {
            "kind": "conformance",
            "catalogue_revision": "gate",
            "statement": format!("registry/records/{SYSTEM}/{ENTRY_ID}/statement.json")
        },
        "artifacts": [
            artifact("results", "results.json", tree)?,
            artifact("verdicts", "verdicts.json", tree)?,
            artifact("transcript", "transcript.json", tree)?,
            artifact("ixit", "ixit.json", tree)?,
            artifact("statement", "statement.json", tree)?
        ]
    });
    std::fs::write(&entry_path, serde_json::to_string_pretty(&submitted)?)?;

    Ok(entry_path)
}

/// The tree a submission arrives in, and the run that produced it: the
/// catalogue is the repository's own, and everything the submission carries is
/// written into a scratch tree, so nothing here can touch the published
/// registry.
fn prepare_submission(
    root: &Path,
) -> Result<(assert_fs::TempDir, PathBuf, PathBuf, u16), Box<dyn std::error::Error>> {
    let tree = assert_fs::TempDir::new()?;
    let record = tree
        .path()
        .join(format!("registry/records/{SYSTEM}/{ENTRY_ID}"));
    std::fs::create_dir_all(&record)?;
    std::fs::create_dir_all(
        tree.path()
            .join(format!("registry/entries/conformance/{SYSTEM}")),
    )?;
    std::fs::create_dir_all(tree.path().join("registry/keys"))?;
    // The catalogue is the repository's own; the prepared tree only holds the
    // submission, so nothing here can touch the published registry.
    std::os::unix::fs::symlink(root.join("artifacts"), tree.path().join("artifacts"))?;
    let _copied = std::fs::copy(
        root.join("artifacts/corpus/keys/cnf-signing.pub.asc"),
        tree.path().join("registry/keys/registry-signing.pub.asc"),
    )?;
    let statement_source = root.join("fixtures/declaration/statement.json");
    let statement = record.join("statement.json");
    let _statement = std::fs::copy(&statement_source, &statement)?;

    let port = fixture_sut()?;
    let ixit = record.join("ixit.json");
    write_ixit(&ixit, port)?;

    let outcome = execute_run(
        &RunRequest {
            root: &root.join("artifacts"),
            ixit: &ixit,
            out_dir: &record,
            sut_name: "gate-cdr",
            sut_version: "0.0.0-gate",
            filter: Some(FILTER),
            statement: Some(&statement),
            recording: Recording::On,
        },
        &|_| {},
        &mut |_| {},
    )?;
    std::fs::write(record.join("results.json"), outcome.results_document()?)?;
    let transcript = outcome
        .transcript_document()?
        .ok_or("a recorded run against a live fixture produces a transcript")?;
    std::fs::write(record.join("transcript.json"), transcript)?;

    Ok((tree, record, statement, port))
}

/// A record altered after the run no longer follows from its own recording,
/// and the gate the lane runs is what says so.
///
/// The comparison itself is pinned by `rederivation.rs`; what this drives is
/// the gate a submission actually passes through, because a gate is proven by
/// refusing a bad submission rather than by staying quiet about a good one
/// (#408).
#[expect(
    clippy::panic_in_result_fn,
    reason = "the Book's Result-test shape: assertions panic, plumbing propagates with `?`"
)]
fn tampering_is_refused_by_the_gate(root: &Path, tree: &Path, record: &Path) -> Fallible {
    let results = record.join("results.json");
    let honest = std::fs::read_to_string(&results)?;
    let altered = honest.replace("\"errored\"", "\"passed\"");
    assert_ne!(altered, honest, "the fixture's run has a row to alter");
    std::fs::write(&results, &altered)?;

    let refused = Command::new("bash")
        .arg(root.join("scripts/checks/registry-rederive.sh"))
        .arg(format!(
            "registry/entries/conformance/{SYSTEM}/{ENTRY_ID}.json"
        ))
        .env("REGISTRY_TREE", tree)
        .env("VEREDICTUM_BIN", env!("CARGO_BIN_EXE_veredictum"))
        .env("VEREDICTUM_REQUIRE_REDERIVATION", "1")
        .output()?;
    let said = String::from_utf8_lossy(&refused.stderr).into_owned();
    assert!(
        !refused.status.success(),
        "a record claiming a pass its recording never supports must be refused:\n{}",
        String::from_utf8_lossy(&refused.stdout)
    );
    assert!(
        said.contains(FILTER),
        "the refusal names the row that does not follow:\n{said}"
    );

    std::fs::write(&results, &honest)?;
    Ok(())
}

/// The verdicts the instrument computes before it submits.
///
/// Exit 1 is an unclean JUDGEMENT — this fixture refuses every request, so the
/// run fails its cases — which is a result rather than a failure to compute.
/// Anything above that is the command refusing to run at all.
#[expect(
    clippy::panic_in_result_fn,
    reason = "the Book's Result-test shape: assertions panic, plumbing propagates with `?`"
)]
fn judge_as_the_console_does(root: &Path, record: &Path, statement: &Path) -> Fallible {
    let judged = Command::new(env!("CARGO_BIN_EXE_veredictum"))
        .args(["verdicts", "--statement"])
        .arg(statement)
        .arg("--results")
        .arg(record.join("results.json"))
        .arg("--root")
        .arg(root.join("artifacts"))
        .arg("--out")
        .arg(record)
        .output()?;
    // Exit 1 is an unclean JUDGEMENT (this fixture refuses every request, so
    // the run fails its cases), which is a result and not a failure to
    // compute. Anything above that is the command refusing to run.
    assert!(
        judged.status.code().is_some_and(|code| code <= 1),
        "the console computes its own verdicts:\n{}\n{}",
        String::from_utf8_lossy(&judged.stdout),
        String::from_utf8_lossy(&judged.stderr)
    );

    Ok(())
}

/// The whole lane over a prepared tree: a run, its record, the entry the
/// instrument submits, and the completion this repository performs.
#[test]
#[expect(
    clippy::panic_in_result_fn,
    reason = "the Book's Result-test shape: assertions panic, plumbing propagates with `?`"
)]
fn a_console_submission_is_completed_and_sealed_by_the_lane() -> Fallible {
    if Command::new("jq").arg("--version").output().is_err() {
        eprintln!("SKIP a_console_submission_is_completed_and_sealed_by_the_lane: no `jq` on PATH");
        return Ok(());
    }
    let root = repo_root();
    let (tree, record, statement, port) = prepare_submission(&root)?;
    judge_as_the_console_does(&root, &record, &statement)?;
    let entry_path = write_submitted_entry(tree.path(), port)?;

    // The gate: the outcomes follow from the recording, and the verdicts from
    // the outcomes.
    let rederived = Command::new("bash")
        .arg(root.join("scripts/checks/registry-rederive.sh"))
        .arg(format!(
            "registry/entries/conformance/{SYSTEM}/{ENTRY_ID}.json"
        ))
        .env("REGISTRY_TREE", tree.path())
        .env("VEREDICTUM_BIN", env!("CARGO_BIN_EXE_veredictum"))
        // The lane sets this, so the test drives the lane's own configuration:
        // a run that recomputed nothing fails instead of passing quietly.
        .env("VEREDICTUM_REQUIRE_REDERIVATION", "1")
        .output()?;
    let said = String::from_utf8_lossy(&rederived.stdout).into_owned();
    assert!(
        rederived.status.success(),
        "an honest submission re-derives to what it submitted:\n{said}\n{}",
        String::from_utf8_lossy(&rederived.stderr)
    );
    // A gate is proven by having RUN. Asserting only that it did not complain
    // passed over a gate that skipped every submission for a whole release
    // (#408), because a skip exits zero exactly as a clean re-derivation does.
    assert!(
        said.contains("re-derived 1 of 1 entry"),
        "the gate must re-derive the submission rather than skip it:\n{said}"
    );

    tampering_is_refused_by_the_gate(&root, tree.path(), &record)?;

    // The completion: seal the record, write the provenance the lane observed.
    let completed = Command::new("bash")
        .arg(root.join("scripts/registry/complete-console-entry.sh"))
        .arg(format!(
            "registry/entries/conformance/{SYSTEM}/{ENTRY_ID}.json"
        ))
        .env("REGISTRY_TREE", tree.path())
        .env("VEREDICTUM_BIN", env!("CARGO_BIN_EXE_veredictum"))
        .env("CONSOLE_ORIGIN", "https://console.veredictum.eu")
        .env("CONSOLE_RUN_ID", "018f3b1e-6f0a-7c21-9a3d-6c2f5d4b8e77")
        .env(
            "SIGN_WORKFLOW_REF",
            "rubentalstra/Veredictum/.github/workflows/registry-console.yml@refs/heads/main",
        )
        .env("SIGN_RUN_ID", "42")
        .env("SIGN_RUN_ATTEMPT", "1")
        .env(
            "REGISTRY_SIGN_KEY",
            root.join("artifacts/corpus/keys/cnf-signing.sec.asc"),
        )
        .env(
            "REGISTRY_PUBLIC_KEY",
            "registry/keys/registry-signing.pub.asc",
        )
        .output()?;
    assert!(
        completed.status.success(),
        "the lane completes an honest submission:\n{}\n{}",
        String::from_utf8_lossy(&completed.stdout),
        String::from_utf8_lossy(&completed.stderr)
    );

    // The completed entry is publishable by the rules it declares, and its
    // provenance says what the lane established rather than what the
    // instrument claimed.
    let entry: RegistryEntry = serde_json::from_str(&std::fs::read_to_string(&entry_path)?)?;
    assert_eq!(entry.tier(), Tier::Console);
    match &entry.provenance {
        Provenance::Console {
            instrument_origin,
            console_run_id,
            identity,
            ..
        } => {
            assert_eq!(instrument_origin, "https://console.veredictum.eu");
            assert_eq!(console_run_id, "018f3b1e-6f0a-7c21-9a3d-6c2f5d4b8e77");
            assert!(!identity.is_empty(), "the signer is named by fingerprint");
        }
        other => panic!("the lane writes a console provenance block, not {other:?}"),
    }
    assert_eq!(entry_defects(&entry), Vec::new());

    // And the published record verifies offline, against the public half a
    // reader has.
    let verified = Command::new(env!("CARGO_BIN_EXE_veredictum"))
        .args(["verify-record", "--record"])
        .arg(record)
        .arg("--key")
        .arg(tree.path().join("registry/keys/registry-signing.pub.asc"))
        .output()?;
    assert!(
        verified.status.success(),
        "verify-record accepts the sealed record offline:\n{}",
        String::from_utf8_lossy(&verified.stderr)
    );
    Ok(())
}
