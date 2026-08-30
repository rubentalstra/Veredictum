// SPDX-FileCopyrightText: Veredictum contributors
// SPDX-License-Identifier: Apache-2.0

//! The #68 acceptance gates: one prepared export over a real driven run.
//!
//! The bundle is sealed by the PINNED ENGINE with the committed test key,
//! then verified through the published lib — never through the console's own
//! arithmetic, which is the whole point of the engine boundary. The three
//! presentation files land beside the sealed set and stay outside the
//! manifest, the zip route serves the sealed bytes, and a tampered copy fails
//! naming the file that changed.
//!
//! `gpg --verify` compatibility is deliberately NOT asserted here: it is the
//! engine's own property (`veredictum::record`'s module documentation records
//! the RFC 9580 detached form), and the engine's suite shells out to `GnuPG`
//! for it. This gate checks the bundle through the lib.

#![allow(
    clippy::print_stderr,
    reason = "the skip-with-reason lines ARE this gate's report, the same shape run_live.rs uses"
)]

use std::path::{Path, PathBuf};

use veredictum_console::engine::{Engine, RunSpec, VerdictsSpec};
use veredictum_console::export::render::escape;
use veredictum_console::export_api::{ExportScreen, prepare, route};
use veredictum_console::run_job::{JobSlot, JobStatus};
use veredictum_console::state::ConsoleState;

use crate::engine_gate;

/// The committed test keypair, which carries no passphrase. The armored
/// certificate is self-describing: its packets carry the user id, the key
/// flags and the subkey binding signature.
fn key(name: &str) -> PathBuf {
    engine_gate::repo_root()
        .join("artifacts/corpus/keys")
        .join(name)
}

/// A console state over the repository's own mounts, with both keys.
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
        party: engine_gate::repo_root().join("party"),
        out: out.to_path_buf(),
        sign_key: Some(key("cnf-signing.sec.asc")),
        verify_key: Some(key("cnf-signing.pub.asc")),
        catalogue: std::sync::Arc::new(catalogue),
        draft: std::sync::Arc::new(std::sync::Mutex::new(
            veredictum_console::run_api::Drafts::new(),
        )),
        client_ip_header: None,
        jobs,
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

/// Drives one real run into `out`, returning the state whose slot holds it.
#[expect(
    clippy::panic_in_result_fn,
    reason = "the Book ch11 Result-returning shape: the run's terminal status is asserted, plumbing propagates with ?"
)]
fn driven(out: &Path) -> Result<Option<(ConsoleState, Engine)>, Box<dyn std::error::Error>> {
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
    // The run seam's ONE derivation of the path (#134), so the export finds
    // the statement and the results exactly where it looks for them.
    let job_dir = veredictum_console::run_job::job_dir(out, id);
    std::fs::create_dir_all(&job_dir)?;
    let port = engine_gate::fixture_sut()?;
    let ixit = engine_gate::write_ixit(&job_dir, port)?;
    let statement = job_dir.join("statement.json");
    std::fs::copy(
        engine_gate::repo_root().join("party/ehrbase/statement.json"),
        &statement,
    )?;

    slot.start(
        id,
        engine_gate::gate_submitter(),
        &engine,
        RunSpec {
            root: state.root.clone(),
            ixit,
            out_dir: job_dir,
            sut_name: String::from("export-gate"),
            sut_version: String::from("0.0.0-gate"),
            statement: Some(statement),
            filter: Some(String::from("I_EHR_SERVICE.create_ehr-main")),
            credentials: vec![],
            progress: true,
            record_exchanges: false,
        },
        String::from("export-gate"),
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

#[expect(
    clippy::panic_in_result_fn,
    reason = "the Book ch11 Result-returning test shape: assertions panic, plumbing propagates with ?"
)]
#[test]
fn an_export_seals_the_record_and_the_lib_verifies_it() -> Result<(), Box<dyn std::error::Error>> {
    let scratch = assert_fs::TempDir::new()?;
    let Some((state, engine)) = driven(scratch.path())? else {
        return Ok(());
    };

    // Before preparing, the section offers the step rather than a bundle.
    assert_eq!(
        prepare::screen(&state, engine_gate::gate_submitter())?,
        ExportScreen::Ready
    );

    let summary = prepare::run_with(&state, engine_gate::gate_submitter(), &engine)?;
    assert_eq!(summary.digest_prefix.len(), 12, "{}", summary.digest);
    assert!(
        summary.digest.starts_with(&summary.digest_prefix),
        "the prefix must be the digest's own head"
    );
    assert!(!summary.fingerprint.is_empty(), "the signer is named");
    assert!(
        !summary.signed_at.is_empty(),
        "the signing time is read back"
    );

    // The manifest covers the ENGINE's documents, and the console's three
    // presentation files stay deliberately outside it.
    let bundle = prepare::job_dir(&state, engine_gate::gate_submitter())?
        .ok_or("no job dir")?
        .join(prepare::EXPORT_DIR);
    let verification = veredictum::record::verify_bundle(&bundle, &key("cnf-signing.pub.asc"))?;
    assert!(
        verification.is_clean(),
        "findings: {:?}",
        verification.findings()
    );
    for name in ["seal-card.svg", "record-badge.svg", "record-report.html"] {
        assert!(bundle.join(name).is_file(), "{name} was not rendered");
        assert!(
            !verification.files.iter().any(|file| file.name == name),
            "{name} must stay OUTSIDE the manifest the engine signed"
        );
    }

    // Every rendered file names the record it belongs to.
    for name in ["seal-card.svg", "record-badge.svg"] {
        let body = std::fs::read_to_string(bundle.join(name))?;
        assert!(
            body.contains(&summary.digest_prefix),
            "{name} does not carry the digest prefix"
        );
    }
    let report = std::fs::read_to_string(bundle.join("record-report.html"))?;
    assert!(report.contains(&summary.digest), "the full digest");
    assert!(report.contains(&summary.fingerprint), "the fingerprint");
    assert!(
        report.contains(&escape(veredictum::record::HONESTY_LINE)),
        "the honesty line"
    );

    // The #94 ruling: what a party publishes says so on its face.
    let card = std::fs::read_to_string(bundle.join("seal-card.svg"))?;
    for line in veredictum_console::export::INDEPENDENCE_LINES {
        assert!(
            card.contains(&escape(line)),
            "the seal card must carry: {line}"
        );
    }
    assert!(
        report.contains(&escape(veredictum_console::export::INDEPENDENCE_LINE)),
        "the report footer must carry the independence line"
    );

    // A second read of the same bundle is the same answer.
    let ExportScreen::Prepared(again) = prepare::screen(&state, engine_gate::gate_submitter())?
    else {
        panic!("a sealed bundle must read back as prepared");
    };
    assert_eq!(again.digest, summary.digest);
    Ok(())
}

/// The response body, read whole — the archive is capped by what one sealed
/// bundle holds, and this gate refuses anything larger rather than streaming.
async fn body_of(response: axum::response::Response) -> Result<Vec<u8>, axum::Error> {
    axum::body::to_bytes(response.into_body(), MAX_ARCHIVE_BYTES)
        .await
        .map(|bytes| bytes.to_vec())
}

/// The read cap for the served archive: far above one bundle, far below a
/// test that hangs reading a body that never ends.
const MAX_ARCHIVE_BYTES: usize = 64 * 1024 * 1024;

/// The download is a server-owned axum route, so the gate drives the HANDLER
/// axum invokes for [`veredictum_console::export_api::DOWNLOAD_PATH`] — its
/// status, its headers and its bytes — rather than the preparation helper
/// behind it.
#[expect(
    clippy::panic_in_result_fn,
    reason = "the Book ch11 Result-returning test shape: assertions panic, plumbing propagates with ?"
)]
#[tokio::test(flavor = "multi_thread")]
async fn the_download_route_serves_the_sealed_bytes() -> Result<(), Box<dyn std::error::Error>> {
    let scratch = assert_fs::TempDir::new()?;
    let Some((state, engine)) = driven(scratch.path())? else {
        return Ok(());
    };

    // Before anything is sealed the route answers 404 with the reason, so a
    // premature download never serves a stale or half-written archive.
    let empty = route::record_zip(
        axum::Extension(state.clone()),
        Some(axum::Extension(axum::extract::ConnectInfo(
            engine_gate::gate_peer(),
        ))),
        axum::http::HeaderMap::new(),
    )
    .await;
    assert_eq!(empty.status(), axum::http::StatusCode::NOT_FOUND);
    let reason = String::from_utf8(body_of(empty).await?)?;
    assert!(reason.contains("no prepared export"), "{reason}");

    prepare::run_with(&state, engine_gate::gate_submitter(), &engine)?;

    let response = route::record_zip(
        axum::Extension(state.clone()),
        Some(axum::Extension(axum::extract::ConnectInfo(
            engine_gate::gate_peer(),
        ))),
        axum::http::HeaderMap::new(),
    )
    .await;
    assert_eq!(response.status(), axum::http::StatusCode::OK);
    let headers = response.headers().clone();
    assert_eq!(
        headers
            .get(axum::http::header::CONTENT_TYPE)
            .map(axum::http::HeaderValue::to_str)
            .transpose()?,
        Some("application/zip"),
        "the route must answer as an archive"
    );
    let disposition = headers
        .get(axum::http::header::CONTENT_DISPOSITION)
        .ok_or("the route must offer the archive as an attachment")?
        .to_str()?
        .to_owned();
    assert!(
        disposition.contains("attachment") && disposition.contains("veredictum-record.zip"),
        "{disposition}"
    );

    let archive = body_of(response).await?;
    // What the route serves IS what the preparation helper packs; the route
    // adds a status and headers and nothing else.
    assert_eq!(
        archive,
        prepare::archive(&state, engine_gate::gate_submitter())?
    );
    let mut zip = zip::ZipArchive::new(std::io::Cursor::new(archive.as_slice()))?;
    let names: Vec<String> = (0..zip.len())
        .map(|index| Ok(zip.by_index(index)?.name().to_owned()))
        .collect::<Result<_, zip::result::ZipError>>()?;
    for expected in [
        veredictum::record::MANIFEST_FILE,
        veredictum::record::SIGNATURE_FILE,
        "seal-card.svg",
        "record-badge.svg",
        "record-report.html",
    ] {
        assert!(
            names.iter().any(|name| name == expected),
            "the archive omits {expected}: {names:?}"
        );
    }

    // Unpacking the served bytes reproduces a bundle the lib still accepts,
    // which is what a party's reader will actually do.
    let unpacked = scratch.path().join("roundtrip");
    std::fs::create_dir_all(&unpacked)?;
    for index in 0..zip.len() {
        let mut entry = zip.by_index(index)?;
        let name = entry.name().to_owned();
        let mut body = Vec::new();
        std::io::Read::read_to_end(&mut entry, &mut body)?;
        std::fs::write(unpacked.join(name), body)?;
    }
    let verification = veredictum::record::verify_bundle(&unpacked, &key("cnf-signing.pub.asc"))?;
    assert!(
        verification.is_clean(),
        "the downloaded archive must verify: {:?}",
        verification.findings()
    );
    Ok(())
}

#[expect(
    clippy::panic_in_result_fn,
    reason = "the Book ch11 Result-returning test shape: assertions panic, plumbing propagates with ?"
)]
#[test]
fn a_tampered_document_fails_naming_that_file() -> Result<(), Box<dyn std::error::Error>> {
    let scratch = assert_fs::TempDir::new()?;
    let Some((state, engine)) = driven(scratch.path())? else {
        return Ok(());
    };
    prepare::run_with(&state, engine_gate::gate_submitter(), &engine)?;
    let bundle = prepare::job_dir(&state, engine_gate::gate_submitter())?
        .ok_or("no job dir")?
        .join(prepare::EXPORT_DIR);

    // A COPY is tampered with: the sealed original stays intact, exactly as a
    // reader receiving a modified publication would experience it.
    let forged = scratch.path().join("forged");
    std::fs::create_dir_all(&forged)?;
    let mut tampered: Option<String> = None;
    for entry in std::fs::read_dir(&bundle)? {
        let entry = entry?;
        let name = entry.file_name().to_string_lossy().into_owned();
        let mut body = std::fs::read(entry.path())?;
        if Path::new(&name).extension() == Some(std::ffi::OsStr::new("md")) && tampered.is_none() {
            body.extend_from_slice(b"\nnot what was signed\n");
            tampered = Some(name.clone());
        }
        std::fs::write(forged.join(&name), body)?;
    }
    let tampered = tampered.ok_or("the judgement rendered no markdown document to tamper with")?;

    let verification = veredictum::record::verify_bundle(&forged, &key("cnf-signing.pub.asc"))?;
    assert!(!verification.is_clean(), "a tampered bundle must not pass");
    let findings = verification.findings();
    assert!(
        findings
            .iter()
            .any(|finding| finding.starts_with(&tampered)),
        "the finding must NAME the file that changed ({tampered}): {findings:?}"
    );
    Ok(())
}

#[expect(
    clippy::panic_in_result_fn,
    reason = "the Book ch11 Result-returning test shape: assertions panic, plumbing propagates with ?"
)]
#[test]
fn no_key_mounted_is_an_honest_state_rather_than_a_failure()
-> Result<(), Box<dyn std::error::Error>> {
    let scratch = assert_fs::TempDir::new()?;
    let Some((mut state, engine)) = driven(scratch.path())? else {
        return Ok(());
    };
    state.sign_key = None;
    state.verify_key = None;

    let ExportScreen::NoKey { missing } = prepare::screen(&state, engine_gate::gate_submitter())?
    else {
        panic!("an unmounted key is a first-class state, not an error");
    };
    assert_eq!(
        missing,
        vec![
            String::from("VEREDICTUM_SIGN_KEY"),
            String::from("VEREDICTUM_VERIFY_KEY")
        ]
    );
    // And the mutation refuses with copy that names the variables to set.
    let refusal = prepare::run_with(&state, engine_gate::gate_submitter(), &engine)
        .expect_err("no key must refuse");
    assert!(refusal.contains("VEREDICTUM_SIGN_KEY"), "{refusal}");
    Ok(())
}

/// The console never puts the signing passphrase where anything can read it
/// back: not in the argument vector, not in a rendered file, not in the
/// summary that crosses the wire to the browser.
#[expect(
    clippy::panic_in_result_fn,
    reason = "the Book ch11 Result-returning test shape: assertions panic, plumbing propagates with ?"
)]
#[test]
fn the_signing_posture_never_reaches_a_file_or_the_wire() -> Result<(), Box<dyn std::error::Error>>
{
    let spec = VerdictsSpec {
        statement: PathBuf::from("s.json"),
        results: PathBuf::from("r.json"),
        root: PathBuf::from("artifacts"),
        out_dir: PathBuf::from("export"),
        sign_key: Some(key("cnf-signing.sec.asc")),
    };
    let rendered: Vec<String> = veredictum_console::engine::verdicts_args(&spec)
        .into_iter()
        .map(|arg| arg.to_string_lossy().into_owned())
        .collect();
    assert!(
        !rendered.iter().any(|arg| arg.contains("passphrase")),
        "{rendered:?}"
    );

    let scratch = assert_fs::TempDir::new()?;
    let Some((state, engine)) = driven(scratch.path())? else {
        return Ok(());
    };
    let summary = prepare::run_with(&state, engine_gate::gate_submitter(), &engine)?;
    // The SECRET KEY MATERIAL itself must never appear in anything published.
    let secret = std::fs::read_to_string(key("cnf-signing.sec.asc"))?;
    let marker = secret
        .lines()
        .nth(2)
        .ok_or("the armored secret key is shorter than expected")?
        .trim();
    assert!(marker.len() > 20, "a weak marker proves nothing: {marker}");

    let bundle = prepare::job_dir(&state, engine_gate::gate_submitter())?
        .ok_or("no job dir")?
        .join(prepare::EXPORT_DIR);
    for entry in std::fs::read_dir(&bundle)? {
        let entry = entry?;
        let body = std::fs::read_to_string(entry.path()).unwrap_or_default();
        assert!(
            !body.contains(marker),
            "{:?} carries secret key material",
            entry.file_name()
        );
    }
    let wire = serde_json::to_string(&summary)?;
    assert!(
        !wire.contains(marker),
        "the wire summary carries key material"
    );
    assert!(
        !wire.contains("PRIVATE KEY"),
        "the wire summary carries key material"
    );
    Ok(())
}

/// Repacks a directory into an archive the upload route would receive.
fn zip_dir(dir: &Path) -> Result<Vec<u8>, Box<dyn std::error::Error>> {
    let mut buffer = std::io::Cursor::new(Vec::new());
    let mut writer = zip::ZipWriter::new(&mut buffer);
    let options = zip::write::SimpleFileOptions::default();
    let mut names: Vec<String> = std::fs::read_dir(dir)?
        .filter_map(|entry| entry.ok()?.file_name().to_str().map(ToOwned::to_owned))
        .collect();
    names.sort();
    for name in names {
        writer.start_file(name.clone(), options)?;
        std::io::Write::write_all(&mut writer, &std::fs::read(dir.join(&name))?)?;
    }
    writer.finish()?;
    Ok(buffer.into_inner())
}

#[expect(
    clippy::panic_in_result_fn,
    reason = "the Book ch11 Result-returning test shape: assertions panic, plumbing propagates with ?"
)]
#[test]
fn an_uploaded_bundle_verifies_clean_and_a_tampered_one_names_the_file()
-> Result<(), Box<dyn std::error::Error>> {
    use veredictum_console::verify_api::{VerifyScreen, read, unpack};

    let scratch = assert_fs::TempDir::new()?;
    let Some((state, engine)) = driven(scratch.path())? else {
        return Ok(());
    };
    prepare::run_with(&state, engine_gate::gate_submitter(), &engine)?;
    let bundle = prepare::job_dir(&state, engine_gate::gate_submitter())?
        .ok_or("no job dir")?
        .join(prepare::EXPORT_DIR);

    // The clean journey: what a party publishes, checked by a stranger.
    let id = unpack::bundle(&state, &zip_dir(&bundle)?)?;
    let VerifyScreen::Checked(clean) = read::screen(&state, Some(&id)) else {
        panic!("a sealed bundle must verify");
    };
    assert!(clean.is_clean, "findings: {:?}", clean.findings);
    assert!(clean.signature_accepted, "the signature must be accepted");
    assert!(clean.fingerprint.is_some(), "the signer is named");
    assert!(clean.signed_at.is_some(), "the signing time is stated");
    assert!(
        clean.files.iter().all(|file| file.outcome == "matched"),
        "{:?}",
        clean.files
    );

    // The tampered journey: one edited document, and the check NAMES it.
    let forged_dir = scratch.path().join("forged-upload");
    std::fs::create_dir_all(&forged_dir)?;
    let mut tampered: Option<String> = None;
    for entry in std::fs::read_dir(&bundle)? {
        let entry = entry?;
        let name = entry.file_name().to_string_lossy().into_owned();
        let mut body = std::fs::read(entry.path())?;
        if Path::new(&name).extension() == Some(std::ffi::OsStr::new("md")) && tampered.is_none() {
            body.extend_from_slice(b"\nnot what was signed\n");
            tampered = Some(name.clone());
        }
        std::fs::write(forged_dir.join(&name), body)?;
    }
    let tampered = tampered.ok_or("no markdown document to tamper with")?;

    let forged_id = unpack::bundle(&state, &zip_dir(&forged_dir)?)?;
    let VerifyScreen::Checked(dirty) = read::screen(&state, Some(&forged_id)) else {
        panic!("a tampered bundle is still a checkable answer");
    };
    assert!(!dirty.is_clean, "a tampered bundle must not pass");
    assert!(
        dirty
            .files
            .iter()
            .any(|file| file.name == tampered && file.outcome == "mismatched"),
        "the row for {tampered} must name itself: {:?}",
        dirty.files
    );
    assert!(
        dirty.findings.iter().any(|f| f.starts_with(&tampered)),
        "{:?}",
        dirty.findings
    );
    Ok(())
}

/// The upload is anonymous input, so the archive's own entry names are the
/// attack surface: a `../` hop must be refused rather than resolved.
#[expect(
    clippy::panic_in_result_fn,
    reason = "the Book ch11 Result-returning test shape: assertions panic, plumbing propagates with ?"
)]
#[test]
fn a_zip_slip_entry_is_refused_and_writes_nothing() -> Result<(), Box<dyn std::error::Error>> {
    use veredictum_console::verify_api::unpack;

    let scratch = assert_fs::TempDir::new()?;
    let out = scratch.path().join("out");
    std::fs::create_dir_all(&out)?;
    let state = state_over(&out, JobSlot::default());

    let mut buffer = std::io::Cursor::new(Vec::new());
    let mut writer = zip::ZipWriter::new(&mut buffer);
    let options = zip::write::SimpleFileOptions::default();
    writer.start_file("../escaped.json", options)?;
    std::io::Write::write_all(&mut writer, b"{\"owned\":true}")?;
    writer.finish()?;
    let hostile = buffer.into_inner();

    let refusal = unpack::bundle(&state, &hostile).expect_err("a `..` entry must be refused");
    assert!(refusal.contains("plain file name"), "{refusal}");
    assert!(
        !scratch.path().join("escaped.json").exists(),
        "the entry escaped the scratch directory"
    );
    // The refused upload leaves no half-written scratch directory behind.
    let leftovers: Vec<_> = std::fs::read_dir(&out)?
        .filter_map(|entry| entry.ok()?.file_name().to_str().map(ToOwned::to_owned))
        .filter(|name| name.starts_with(unpack::SCRATCH_PREFIX))
        .collect();
    assert!(leftovers.is_empty(), "left behind: {leftovers:?}");
    Ok(())
}

/// The bundle id arrives in a query string, so it is user input: it must
/// never reach a path join unless this console minted it.
#[expect(
    clippy::panic_in_result_fn,
    reason = "the Book ch11 Result-returning test shape: assertions panic, plumbing propagates with ?"
)]
#[test]
fn an_unminted_bundle_id_never_reaches_the_filesystem() -> Result<(), Box<dyn std::error::Error>> {
    use veredictum_console::verify_api::{VerifyScreen, read};

    let scratch = assert_fs::TempDir::new()?;
    let out = scratch.path().join("out");
    std::fs::create_dir_all(&out)?;
    let state = state_over(&out, JobSlot::default());

    for hostile in [
        "../../../etc/pas",
        "..%2F..%2Fetc%2Fpa",
        "ZZZZZZZZZZZZZZZZ",
        "",
    ] {
        let VerifyScreen::Refused { reason } = read::screen(&state, Some(hostile)) else {
            panic!("{hostile:?} must be refused, not resolved");
        };
        assert!(reason.contains("not a bundle"), "{hostile:?}: {reason}");
    }
    // No bundle at all is the page's resting state, never a refusal.
    assert_eq!(read::screen(&state, None), VerifyScreen::Idle);
    Ok(())
}

/// An oversized upload is refused by size before a byte is written.
#[expect(
    clippy::panic_in_result_fn,
    reason = "the Book ch11 Result-returning test shape: assertions panic, plumbing propagates with ?"
)]
#[test]
fn an_oversized_upload_is_refused_before_anything_is_written()
-> Result<(), Box<dyn std::error::Error>> {
    use veredictum_console::verify_api::unpack;

    let scratch = assert_fs::TempDir::new()?;
    let out = scratch.path().join("out");
    std::fs::create_dir_all(&out)?;
    let state = state_over(&out, JobSlot::default());

    let over = usize::try_from(unpack::MAX_UPLOAD_BYTES).unwrap_or(usize::MAX);
    let refusal = unpack::bundle(&state, &vec![0_u8; over.saturating_add(1)])
        .expect_err("an oversized upload must be refused");
    assert!(refusal.contains("at most"), "{refusal}");
    let leftovers = std::fs::read_dir(&out)?.count();
    assert_eq!(leftovers, 0, "an oversized upload wrote something");
    Ok(())
}

/// The stale-bundle trap, found live by the browser journeys: the job counter
/// restarts with the console process while the output mount persists, so a
/// fresh run CAN land on an older run's directory. A bundle names no run, so
/// one left behind would be presented as the new run's record — a signature
/// certifying documents nobody graded in this campaign. Starting a run into a
/// directory therefore invalidates any export of it.
#[expect(
    clippy::panic_in_result_fn,
    reason = "the Book ch11 Result-returning test shape: assertions panic, plumbing propagates with ?"
)]
#[test]
fn a_run_into_a_directory_invalidates_an_older_export_of_it()
-> Result<(), Box<dyn std::error::Error>> {
    let scratch = assert_fs::TempDir::new()?;
    let Some((state, engine)) = driven(scratch.path())? else {
        return Ok(());
    };
    let summary = prepare::run_with(&state, engine_gate::gate_submitter(), &engine)?;
    let job = prepare::job_dir(&state, engine_gate::gate_submitter())?.ok_or("no job dir")?;
    let bundle = job.join(prepare::EXPORT_DIR);
    assert!(bundle.join(veredictum::record::MANIFEST_FILE).is_file());
    // The surface reports it, which is exactly what must NOT survive a
    // re-run into the same directory.
    let ExportScreen::Prepared(before) = prepare::screen(&state, engine_gate::gate_submitter())?
    else {
        panic!("the sealed bundle must read back as prepared");
    };
    assert_eq!(before.digest, summary.digest);

    prepare::invalidate(&job)?;

    assert!(!bundle.exists(), "the stale bundle survived the new run");
    assert_eq!(
        prepare::screen(&state, engine_gate::gate_submitter())?,
        ExportScreen::Ready,
        "a run whose export was invalidated must offer the step again, never another run's record"
    );
    // Idempotent: a directory that never had one is not an error.
    prepare::invalidate(&job)?;
    Ok(())
}

/// The card's three slots are single lines the master draws a rule under, so
/// the profile slot states the verdicts a party CLAIMED and stops. An
/// unclaimed tier has no verdict to state, and listing it runs the line past
/// its rule on the artifact parties publish.
#[expect(
    clippy::panic_in_result_fn,
    reason = "the Book ch11 Result-returning test shape: assertions panic, plumbing propagates with ?"
)]
#[test]
fn the_card_states_claimed_verdicts_and_not_unclaimed_tiers()
-> Result<(), Box<dyn std::error::Error>> {
    let scratch = assert_fs::TempDir::new()?;
    let Some((state, engine)) = driven(scratch.path())? else {
        return Ok(());
    };
    let summary = prepare::run_with(&state, engine_gate::gate_submitter(), &engine)?;
    assert!(
        !summary.profile_summary.contains("not_claimed"),
        "an unclaimed tier reached the card: {}",
        summary.profile_summary
    );
    // The EHRbase claim this gate drives carries CORE, so the slot is never
    // empty — a card that stated nothing would certify nothing.
    assert!(
        summary.profile_summary.contains("CORE"),
        "the claimed tier must be stated: {}",
        summary.profile_summary
    );
    let card = std::fs::read_to_string(
        prepare::job_dir(&state, engine_gate::gate_submitter())?
            .ok_or("no job dir")?
            .join(prepare::EXPORT_DIR)
            .join("seal-card.svg"),
    )?;
    assert!(card.contains(&summary.profile_summary));
    assert!(!card.contains("not_claimed"));
    Ok(())
}

/// Capture mode changes what the browser is TOLD and nothing that is sealed.
///
/// The book's screenshots are refreshed by driving this console, and a real
/// digest and signing time make every capture pass rewrite committed images.
/// The stand-ins therefore live at the answering seam: the manifest, the
/// signature and the three rendered files keep the record's own facts, which
/// is what a party publishes and a stranger re-checks.
#[expect(
    clippy::panic_in_result_fn,
    reason = "the Book ch11 Result-returning test shape: assertions panic, plumbing propagates with ?"
)]
#[test]
fn capture_mode_pins_the_answer_and_never_the_seal() -> Result<(), Box<dyn std::error::Error>> {
    use veredictum_console::capture::{self, PINNED_DIGEST, PINNED_TIME};

    let scratch = assert_fs::TempDir::new()?;
    let Some((mut state, engine)) = driven(scratch.path())? else {
        return Ok(());
    };
    let sealed = prepare::run_with(&state, engine_gate::gate_submitter(), &engine)?;
    assert_ne!(sealed.digest, PINNED_DIGEST);
    assert_ne!(sealed.signed_at, PINNED_TIME);

    state.capture = true;
    let shown = capture::export_summary(&state, sealed.clone());
    assert_eq!(shown.digest, PINNED_DIGEST);
    assert_eq!(shown.signed_at, PINNED_TIME);
    // What the surface is FOR survives: the verdicts, the SUT, the file list.
    assert_eq!(shown.profile_summary, sealed.profile_summary);
    assert_eq!(shown.sut, sealed.sut);
    assert_eq!(shown.sealed_files, sealed.sealed_files);

    // The bundle on disk is the run's own record, unpinned and still valid.
    let bundle = prepare::job_dir(&state, engine_gate::gate_submitter())?
        .ok_or("no job dir")?
        .join(prepare::EXPORT_DIR);
    let card = std::fs::read_to_string(bundle.join("seal-card.svg"))?;
    assert!(
        card.contains(&sealed.digest_prefix),
        "the sealed card must name the real record"
    );
    assert!(
        !card.contains(&shown.digest_prefix),
        "a capture stand-in reached a published artifact"
    );
    let report = std::fs::read_to_string(bundle.join("record-report.html"))?;
    assert!(report.contains(&sealed.digest), "the real full digest");
    assert!(!report.contains(PINNED_DIGEST));
    let verification = veredictum::record::verify_bundle(&bundle, &key("cnf-signing.pub.asc"))?;
    assert!(
        verification.is_clean(),
        "findings: {:?}",
        verification.findings()
    );
    Ok(())
}
