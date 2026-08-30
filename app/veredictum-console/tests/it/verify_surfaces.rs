// SPDX-FileCopyrightText: Veredictum contributors
// SPDX-License-Identifier: Apache-2.0

//! S9's refusal surface, sealed without spawning the engine.
//!
//! The signing here is the published lib's own `record::seal` over the
//! committed test key, so these pins hold whether or not an engine binary was
//! built: what they exercise is the CONSOLE's upload, sweep, id and rendering
//! rules, which sit entirely on this side of the engine boundary. The sealed
//! end-to-end journey through the pinned CLI stays in `export_gate`.

use std::path::{Path, PathBuf};

use veredictum_console::state::ConsoleState;
use veredictum_console::verify_api::{VerifyScreen, read, unpack};

use crate::engine_gate;

/// The committed test keypair, whose armored certificate is self-describing.
fn key(name: &str) -> PathBuf {
    engine_gate::repo_root()
        .join("artifacts/corpus/keys")
        .join(name)
}

/// A console state over `out`, with or without the public half mounted.
///
/// The catalogue is deliberately absent: nothing S9 reads touches it, and a
/// verify page works with no catalogue at all by design.
fn state_over(out: &Path, verify_key: Option<PathBuf>) -> ConsoleState {
    ConsoleState {
        root: engine_gate::repo_root().join("artifacts"),
        specs: engine_gate::repo_root().join("specs/openehr"),
        party: engine_gate::repo_root().join("party"),
        out: out.to_path_buf(),
        catalogue: std::sync::Arc::new(Err(String::from("not loaded for this gate"))),
        draft: std::sync::Arc::new(std::sync::Mutex::new(
            veredictum_console::run_api::Drafts::new(),
        )),
        client_ip_header: None,
        sign_key: None,
        verify_key,
        jobs: veredictum_console::run_job::JobSlot::default(),
        capture: false,
    }
}

/// Seals `files` into `dir` through the lib: the manifest plus its detached
/// signature, exactly the two files the engine adds.
fn seal_into(dir: &Path, files: &[(&str, &[u8])]) -> Result<(), Box<dyn std::error::Error>> {
    std::fs::create_dir_all(dir)?;
    for (name, body) in files {
        std::fs::write(dir.join(name), body)?;
    }
    let recorded: Vec<veredictum::record::RecordedFile<'_>> = files
        .iter()
        .map(|(name, body)| veredictum::record::RecordedFile { name, body })
        .collect();
    let sealed = veredictum::record::seal(&recorded, &key("cnf-signing.sec.asc"), None)?;
    std::fs::write(dir.join(veredictum::record::MANIFEST_FILE), sealed.manifest)?;
    std::fs::write(
        dir.join(veredictum::record::SIGNATURE_FILE),
        sealed.signature,
    )?;
    Ok(())
}

/// A zip archive over `entries`, in memory.
fn zip_of(entries: &[(String, Vec<u8>)]) -> Result<Vec<u8>, Box<dyn std::error::Error>> {
    let mut buffer = std::io::Cursor::new(Vec::new());
    let mut writer = zip::ZipWriter::new(&mut buffer);
    let options = zip::write::SimpleFileOptions::default()
        .compression_method(zip::CompressionMethod::Deflated);
    for (name, body) in entries {
        writer.start_file(name.clone(), options)?;
        std::io::Write::write_all(&mut writer, body)?;
    }
    writer.finish()?;
    Ok(buffer.into_inner())
}

/// Unpacks a sealed directory into a scratch bundle and reads its screen.
fn checked(
    state: &ConsoleState,
    dir: &Path,
) -> Result<veredictum_console::verify_api::BundleView, Box<dyn std::error::Error>> {
    let mut entries: Vec<(String, Vec<u8>)> = Vec::new();
    for entry in std::fs::read_dir(dir)? {
        let entry = entry?;
        let name = entry.file_name().to_string_lossy().into_owned();
        entries.push((name, std::fs::read(entry.path())?));
    }
    entries.sort_by(|a, b| a.0.cmp(&b.0));
    let id = unpack::bundle(state, &zip_of(&entries)?)?;
    match read::screen(state, Some(&id)) {
        VerifyScreen::Checked(view) => Ok(*view),
        other => Err(format!("a sealed bundle must render as Checked: {other:?}").into()),
    }
}

/// With no public key mounted there is nothing to check against, and the page
/// says so instead of rendering a verification it could not make.
#[expect(
    clippy::panic_in_result_fn,
    reason = "the Book ch11 Result-returning test shape: assertions panic, plumbing propagates with ? (https://doc.rust-lang.org/book/ch11-01-writing-tests.html)"
)]
#[test]
fn no_public_key_mounted_is_the_no_key_screen() -> Result<(), Box<dyn std::error::Error>> {
    let scratch = assert_fs::TempDir::new()?;
    let state = state_over(scratch.path(), None);
    assert_eq!(read::screen(&state, None), VerifyScreen::NoKey);
    assert_eq!(
        read::screen(&state, Some("0123456789abcdef")),
        VerifyScreen::NoKey,
        "a mounted key is checked before the id is, because nothing can be verified without one"
    );
    Ok(())
}

/// An id this console could have minted, whose scratch directory the sweeper
/// already took, is an actionable "upload it again" rather than a failure.
#[expect(
    clippy::panic_in_result_fn,
    reason = "the Book ch11 Result-returning test shape: assertions panic, plumbing propagates with ? (https://doc.rust-lang.org/book/ch11-01-writing-tests.html)"
)]
#[test]
fn a_swept_bundle_asks_for_the_upload_again() -> Result<(), Box<dyn std::error::Error>> {
    let scratch = assert_fs::TempDir::new()?;
    let state = state_over(scratch.path(), Some(key("cnf-signing.pub.asc")));
    let VerifyScreen::Refused { reason } = read::screen(&state, Some("00112233445566ff")) else {
        panic!("a minted-shaped id with no directory is refused, not verified");
    };
    assert!(
        reason.contains("no longer here") && reason.contains("Upload it again"),
        "{reason}"
    );
    Ok(())
}

/// A bundle the lib sealed reads back clean: the signature is accepted, the
/// signer and the signing time are stated, and every file the manifest names
/// reproduced its digest.
#[expect(
    clippy::panic_in_result_fn,
    reason = "the Book ch11 Result-returning test shape: assertions panic, plumbing propagates with ? (https://doc.rust-lang.org/book/ch11-01-writing-tests.html)"
)]
#[test]
fn a_sealed_bundle_reads_clean_with_its_signer() -> Result<(), Box<dyn std::error::Error>> {
    let scratch = assert_fs::TempDir::new()?;
    let state = state_over(scratch.path(), Some(key("cnf-signing.pub.asc")));
    let bundle = scratch.path().join("sealed");
    seal_into(
        &bundle,
        &[
            ("CONFORMANCE_REPORT.md", b"# report\n"),
            ("CERTIFICATE.md", b"# certificate\n"),
        ],
    )?;

    let view = checked(&state, &bundle)?;
    assert!(view.signature_accepted, "{:?}", view.findings);
    assert!(view.is_clean, "{:?}", view.findings);
    assert!(view.findings.is_empty(), "{:?}", view.findings);
    assert!(
        view.fingerprint.is_some_and(|f| !f.is_empty()),
        "an accepted signature names its signer"
    );
    assert!(view.signed_at.is_some(), "an accepted signature is dated");
    assert!(
        view.instrument.contains("veredictum"),
        "the manifest's instrument identity is rendered: {}",
        view.instrument
    );
    let mut names: Vec<&str> = view.files.iter().map(|f| f.name.as_str()).collect();
    names.sort_unstable();
    assert_eq!(names, ["CERTIFICATE.md", "CONFORMANCE_REPORT.md"]);
    assert!(
        view.files.iter().all(|f| f.outcome == "matched"),
        "{:?}",
        view.files
    );
    assert!(view.files.iter().all(|f| f.detail.is_none()));
    Ok(())
}

/// Each way a covered file can fail its digest gets its own token and its own
/// diagnostic: changed, gone, and present but unreadable.
#[expect(
    clippy::panic_in_result_fn,
    reason = "the Book ch11 Result-returning test shape: assertions panic, plumbing propagates with ? (https://doc.rust-lang.org/book/ch11-01-writing-tests.html)"
)]
#[test]
fn each_file_failure_carries_its_own_token() -> Result<(), Box<dyn std::error::Error>> {
    let scratch = assert_fs::TempDir::new()?;
    let state = state_over(scratch.path(), Some(key("cnf-signing.pub.asc")));
    let bundle = scratch.path().join("sealed");
    seal_into(
        &bundle,
        &[
            ("changed.md", b"original\n"),
            ("gone.md", b"present at sealing\n"),
            ("kept.md", b"untouched\n"),
        ],
    )?;
    std::fs::write(bundle.join("changed.md"), b"tampered\n")?;
    std::fs::remove_file(bundle.join("gone.md"))?;

    let view = checked(&state, &bundle)?;
    assert!(view.signature_accepted, "the manifest itself is untouched");
    assert!(!view.is_clean);
    let outcome = |name: &str| {
        view.files
            .iter()
            .find(|f| f.name == name)
            .map(|f| (f.outcome.clone(), f.detail.clone()))
    };
    let (changed, changed_detail) = outcome("changed.md").ok_or("changed.md has a row")?;
    assert_eq!(changed, "mismatched");
    assert!(
        changed_detail.is_some_and(|d| d.starts_with("recomputed ")),
        "a mismatch names what was recomputed"
    );
    let (gone, gone_detail) = outcome("gone.md").ok_or("gone.md has a row")?;
    assert_eq!(gone, "missing");
    assert_eq!(
        gone_detail.as_deref(),
        Some("named by the manifest, absent")
    );
    assert_eq!(
        outcome("kept.md").map(|row| row.0).as_deref(),
        Some("matched")
    );
    assert!(
        view.findings.iter().any(|f| f.contains("changed.md"))
            && view.findings.iter().any(|f| f.contains("gone.md")),
        "{:?}",
        view.findings
    );
    Ok(())
}

/// A file the manifest covers that exists but cannot be read is UNREADABLE,
/// which is a distinct answer from absent: something is there and the check
/// could not be made.
#[expect(
    clippy::panic_in_result_fn,
    reason = "the Book ch11 Result-returning test shape: assertions panic, plumbing propagates with ? (https://doc.rust-lang.org/book/ch11-01-writing-tests.html)"
)]
#[test]
fn a_covered_file_that_will_not_read_is_unreadable() -> Result<(), Box<dyn std::error::Error>> {
    let scratch = assert_fs::TempDir::new()?;
    let state = state_over(scratch.path(), Some(key("cnf-signing.pub.asc")));
    // Sealed straight into a scratch directory this console could have
    // minted, because the shape under test cannot travel through a zip: an
    // archive carries no directory where a file is expected.
    let id = "00112233445566aa";
    let bundle = unpack::scratch_dir(&state, id)?;
    seal_into(&bundle, &[("blocked.md", b"sealed as a file\n")])?;
    // A directory standing where the manifest names a file: present, and no
    // digest can be taken of it.
    std::fs::remove_file(bundle.join("blocked.md"))?;
    std::fs::create_dir_all(bundle.join("blocked.md"))?;

    let VerifyScreen::Checked(view) = read::screen(&state, Some(id)) else {
        panic!("a bundle whose manifest and signature are intact is checked");
    };
    let row = view
        .files
        .iter()
        .find(|f| f.name == "blocked.md")
        .ok_or("blocked.md has a row")?;
    assert_eq!(row.outcome, "unreadable");
    assert!(
        row.detail.as_ref().is_some_and(|d| !d.is_empty()),
        "an unreadable file carries the filesystem's own diagnostic"
    );
    assert!(!view.is_clean);
    Ok(())
}

/// A manifest swapped after signing does not verify, and the page says the
/// signature was rejected rather than quietly rendering the file rows.
#[expect(
    clippy::panic_in_result_fn,
    reason = "the Book ch11 Result-returning test shape: assertions panic, plumbing propagates with ? (https://doc.rust-lang.org/book/ch11-01-writing-tests.html)"
)]
#[test]
fn a_manifest_swapped_after_signing_is_rejected() -> Result<(), Box<dyn std::error::Error>> {
    let scratch = assert_fs::TempDir::new()?;
    let state = state_over(scratch.path(), Some(key("cnf-signing.pub.asc")));
    let bundle = scratch.path().join("sealed");
    seal_into(&bundle, &[("report.md", b"the signed body\n")])?;

    // The signature stays; the manifest is replaced with one covering other
    // bytes, which is exactly the substitution a detached signature exists to
    // catch.
    let other = scratch.path().join("other");
    seal_into(&other, &[("report.md", b"a different body\n")])?;
    std::fs::copy(
        other.join(veredictum::record::MANIFEST_FILE),
        bundle.join(veredictum::record::MANIFEST_FILE),
    )?;

    let view = checked(&state, &bundle)?;
    assert!(!view.signature_accepted);
    assert_eq!(
        view.fingerprint, None,
        "a rejected signature names no signer"
    );
    assert_eq!(view.signed_at, None);
    assert!(!view.is_clean);
    assert!(
        view.findings
            .iter()
            .any(|f| f.contains("does not verify against the supplied public key")),
        "{:?}",
        view.findings
    );
    Ok(())
}

/// Bytes that are not an archive are refused by the unpacker, before any
/// scratch directory is made.
#[expect(
    clippy::panic_in_result_fn,
    reason = "the Book ch11 Result-returning test shape: assertions panic, plumbing propagates with ? (https://doc.rust-lang.org/book/ch11-01-writing-tests.html)"
)]
#[test]
fn bytes_that_are_not_an_archive_are_refused() -> Result<(), Box<dyn std::error::Error>> {
    let scratch = assert_fs::TempDir::new()?;
    let state = state_over(scratch.path(), Some(key("cnf-signing.pub.asc")));
    let refusal = unpack::bundle(&state, b"not a zip at all")
        .expect_err("an upload that is not an archive is refused");
    assert!(refusal.contains("not a readable zip archive"), "{refusal}");
    assert!(
        scratched(scratch.path())?.is_empty(),
        "a refused upload leaves no scratch directory"
    );
    Ok(())
}

/// The scratch directories currently under the output root.
fn scratched(out: &Path) -> Result<Vec<String>, std::io::Error> {
    let mut names = Vec::new();
    for entry in std::fs::read_dir(out)? {
        let name = entry?.file_name().to_string_lossy().into_owned();
        if name.starts_with(unpack::SCRATCH_PREFIX) {
            names.push(name);
        }
    }
    names.sort();
    Ok(names)
}

/// An archive carrying more entries than a record bundle ever does is refused
/// on the count, before a single entry is expanded.
#[expect(
    clippy::panic_in_result_fn,
    reason = "the Book ch11 Result-returning test shape: assertions panic, plumbing propagates with ? (https://doc.rust-lang.org/book/ch11-01-writing-tests.html)"
)]
#[test]
fn an_archive_with_too_many_entries_is_refused() -> Result<(), Box<dyn std::error::Error>> {
    let scratch = assert_fs::TempDir::new()?;
    let state = state_over(scratch.path(), Some(key("cnf-signing.pub.asc")));
    let entries: Vec<(String, Vec<u8>)> = (0..=unpack::MAX_ENTRIES)
        .map(|index| (format!("file-{index}.md"), b"x".to_vec()))
        .collect();
    let refusal = unpack::bundle(&state, &zip_of(&entries)?)
        .expect_err("an archive over the entry cap is refused");
    assert!(
        refusal.contains(&unpack::MAX_ENTRIES.to_string()),
        "the refusal names the cap: {refusal}"
    );
    assert!(scratched(scratch.path())?.is_empty());
    Ok(())
}

/// A single entry that declares more than the per-file cap is refused, and
/// the partial expansion goes with it.
#[expect(
    clippy::panic_in_result_fn,
    reason = "the Book ch11 Result-returning test shape: assertions panic, plumbing propagates with ? (https://doc.rust-lang.org/book/ch11-01-writing-tests.html)"
)]
#[test]
fn an_entry_over_the_per_file_cap_is_refused() -> Result<(), Box<dyn std::error::Error>> {
    let scratch = assert_fs::TempDir::new()?;
    let state = state_over(scratch.path(), Some(key("cnf-signing.pub.asc")));
    let oversized = usize::try_from(unpack::MAX_ENTRY_BYTES)
        .unwrap_or(usize::MAX)
        .saturating_add(1);
    let entries = vec![
        (
            String::from("small.md"),
            b"kept until the bad entry".to_vec(),
        ),
        (String::from("huge.md"), vec![0_u8; oversized]),
    ];
    let refusal = unpack::bundle(&state, &zip_of(&entries)?)
        .expect_err("an entry over the per-file cap is refused");
    assert!(
        refusal.contains("huge.md") && refusal.contains("declares"),
        "{refusal}"
    );
    assert!(
        scratched(scratch.path())?.is_empty(),
        "a refused archive leaves nothing behind, including what was written first"
    );
    Ok(())
}

/// Entries that are each inside the per-file cap but together expand past the
/// total are refused on the total, which is the decompression-bomb rule.
#[expect(
    clippy::panic_in_result_fn,
    reason = "the Book ch11 Result-returning test shape: assertions panic, plumbing propagates with ? (https://doc.rust-lang.org/book/ch11-01-writing-tests.html)"
)]
#[test]
fn an_archive_expanding_past_the_total_cap_is_refused() -> Result<(), Box<dyn std::error::Error>> {
    let scratch = assert_fs::TempDir::new()?;
    let state = state_over(scratch.path(), Some(key("cnf-signing.pub.asc")));
    let each = usize::try_from(unpack::MAX_ENTRY_BYTES).unwrap_or(usize::MAX);
    let entries: Vec<(String, Vec<u8>)> = (0..5)
        .map(|index| (format!("part-{index}.bin"), vec![0_u8; each]))
        .collect();
    let refusal = unpack::bundle(&state, &zip_of(&entries)?)
        .expect_err("an archive expanding past the total cap is refused");
    assert!(
        refusal.contains(&unpack::MAX_TOTAL_BYTES.to_string()),
        "the refusal names the total cap: {refusal}"
    );
    assert!(scratched(scratch.path())?.is_empty());
    Ok(())
}

/// The sweeper takes expired scratch directories and leaves everything else,
/// including a fresh bundle and anything the output root holds that is not a
/// scratch directory at all.
#[expect(
    clippy::panic_in_result_fn,
    reason = "the Book ch11 Result-returning test shape: assertions panic, plumbing propagates with ? (https://doc.rust-lang.org/book/ch11-01-writing-tests.html)"
)]
#[test]
fn the_sweeper_takes_only_expired_scratch_directories() -> Result<(), Box<dyn std::error::Error>> {
    let scratch = assert_fs::TempDir::new()?;
    let state = state_over(scratch.path(), Some(key("cnf-signing.pub.asc")));

    let expired = scratch
        .path()
        .join(format!("{}00000000000000ff", unpack::SCRATCH_PREFIX));
    std::fs::create_dir_all(&expired)?;
    std::fs::write(expired.join("record-manifest.json"), b"{}")?;
    // Wall-clock time comes from jiff, which the sweeper itself reads.
    let long_ago = std::time::SystemTime::from(jiff::Timestamp::now().checked_sub(
        jiff::SignedDuration::try_from(unpack::TTL.saturating_mul(2))?,
    )?);
    std::fs::File::open(&expired)?.set_modified(long_ago)?;

    let kept = scratch.path().join("console-job-1");
    std::fs::create_dir_all(&kept)?;
    let fresh = unpack::bundle(
        &state,
        &zip_of(&[(String::from("report.md"), b"fresh".to_vec())])?,
    )?;

    unpack::sweep(&state);
    assert!(
        !expired.is_dir(),
        "a scratch directory older than the TTL is swept"
    );
    assert!(
        kept.is_dir(),
        "the sweeper only ever touches its own prefix"
    );
    assert!(
        unpack::scratch_dir(&state, &fresh)?.is_dir(),
        "a bundle uploaded moments ago survives the sweep"
    );
    Ok(())
}

/// The upload route answers with the page either way: a redirect to the
/// unpacked bundle, or a redirect carrying the refusal reason in the query.
#[expect(
    clippy::panic_in_result_fn,
    reason = "the Book ch11 Result-returning test shape: assertions panic, plumbing propagates with ? (https://doc.rust-lang.org/book/ch11-01-writing-tests.html)"
)]
#[tokio::test]
async fn the_upload_route_redirects_to_the_page_either_way()
-> Result<(), Box<dyn std::error::Error>> {
    use axum::extract::FromRequest as _;

    let scratch = assert_fs::TempDir::new()?;
    let state = state_over(scratch.path(), Some(key("cnf-signing.pub.asc")));

    let post = async |state: &ConsoleState,
                      field: &str,
                      body: Vec<u8>|
           -> Result<axum::response::Response, Box<dyn std::error::Error>> {
        let boundary = "veredictumgate";
        let mut wire = Vec::new();
        wire.extend_from_slice(
            format!(
                "--{boundary}\r\nContent-Disposition: form-data; name=\"{field}\"; filename=\"record.zip\"\r\nContent-Type: application/zip\r\n\r\n"
            )
            .as_bytes(),
        );
        wire.extend_from_slice(&body);
        wire.extend_from_slice(format!("\r\n--{boundary}--\r\n").as_bytes());
        let request = axum::http::Request::builder()
            .method(axum::http::Method::POST)
            .uri(veredictum_console::verify_api::UPLOAD_PATH)
            .header(
                axum::http::header::CONTENT_TYPE,
                format!("multipart/form-data; boundary={boundary}"),
            )
            .body(axum::body::Body::from(wire))?;
        let form = axum::extract::Multipart::from_request(request, &()).await?;
        Ok(
            veredictum_console::verify_api::route::upload(axum::Extension(state.clone()), form)
                .await,
        )
    };

    let location = |response: &axum::response::Response| -> String {
        response
            .headers()
            .get(axum::http::header::LOCATION)
            .and_then(|value| value.to_str().ok())
            .unwrap_or_default()
            .to_owned()
    };

    let accepted = post(
        &state,
        "bundle",
        zip_of(&[(String::from("report.md"), b"a body".to_vec())])?,
    )
    .await?;
    assert_eq!(accepted.status(), axum::http::StatusCode::SEE_OTHER);
    let target = location(&accepted);
    assert!(target.starts_with("/verify?bundle="), "{target}");

    let refused = post(&state, "bundle", b"not a zip".to_vec()).await?;
    assert_eq!(refused.status(), axum::http::StatusCode::SEE_OTHER);
    let refusal = location(&refused);
    assert!(
        refusal.starts_with("/verify?refused=") && refusal.contains("zip"),
        "{refusal}"
    );

    // A form whose file field is named something else carried no bundle at
    // all, which is the "no file was chosen" answer rather than a parse error.
    let unnamed = post(&state, "other", Vec::new()).await?;
    assert!(
        location(&unnamed).contains("no%20file%20was%20chosen"),
        "{}",
        location(&unnamed)
    );
    Ok(())
}

/// Capture mode pins what a photograph of this page would otherwise change:
/// the signing time and every file digest. The outcomes, the findings and the
/// signer stay exactly as verification found them, so a capture still
/// documents the console's real answer.
#[expect(
    clippy::panic_in_result_fn,
    reason = "the Book ch11 Result-returning test shape: assertions panic, plumbing propagates with ? (https://doc.rust-lang.org/book/ch11-01-writing-tests.html)"
)]
#[test]
fn capture_mode_pins_the_signing_time_and_the_digests() -> Result<(), Box<dyn std::error::Error>> {
    use veredictum_console::capture::{self, PINNED_DIGEST, PINNED_TIME};

    let scratch = assert_fs::TempDir::new()?;
    let mut state = state_over(scratch.path(), Some(key("cnf-signing.pub.asc")));
    let sealed = scratch.path().join("sealed");
    seal_into(&sealed, &[("CONFORMANCE_REPORT.md", b"# a report\n")])?;
    let live = checked(&state, &sealed)?;
    assert_ne!(live.signed_at.as_deref(), Some(PINNED_TIME));
    assert!(live.files.iter().all(|file| file.digest != PINNED_DIGEST));

    // Off, the page answers with what verification actually found.
    let VerifyScreen::Checked(off) =
        capture::verification(&state, VerifyScreen::Checked(Box::new(live.clone())))
    else {
        panic!("a checked bundle stays checked");
    };
    assert_eq!(*off, live);

    state.capture = true;
    let VerifyScreen::Checked(on) =
        capture::verification(&state, VerifyScreen::Checked(Box::new(live.clone())))
    else {
        panic!("a checked bundle stays checked");
    };
    assert_eq!(on.signed_at.as_deref(), Some(PINNED_TIME));
    assert!(on.files.iter().all(|file| file.digest == PINNED_DIGEST));
    assert_eq!(on.is_clean, live.is_clean);
    assert_eq!(on.fingerprint, live.fingerprint);
    assert_eq!(
        on.files.iter().map(|f| f.name.clone()).collect::<Vec<_>>(),
        live.files
            .iter()
            .map(|f| f.name.clone())
            .collect::<Vec<_>>()
    );
    Ok(())
}
