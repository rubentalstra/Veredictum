// SPDX-FileCopyrightText: Veredictum contributors
// SPDX-License-Identifier: Apache-2.0

//! The published interop claim, checked against `GnuPG` itself (#76).
//!
//! The README, the assurance case, the book and the console all tell a party
//! that `gpg --verify record-manifest.json.asc record-manifest.json` accepts a
//! sealed bundle with no Veredictum binary present. That claim is about
//! another program's acceptance of our bytes, so nothing inside this crate can
//! establish it: only `GnuPG` can. The gate therefore shells out.
//!
//! It SKIPS with a printed reason when `gpg` is not on `PATH`, so a
//! contributor without `GnuPG` is not blocked, and the CI job asserts `GnuPG` is
//! installed before the suite runs, so a skip can never be the outcome that
//! gates a merge.

use std::path::{Path, PathBuf};
use std::process::Command;

use veredictum::record::{RecordedFile, seal};

/// The committed test keypair, which carries no passphrase. The armored
/// certificate is self-describing: its packets carry the user id, the key
/// flags and the subkey binding signature.
fn key_dir() -> PathBuf {
    Path::new(concat!(env!("CARGO_MANIFEST_DIR"), "/../.."))
        .join("artifacts")
        .join("corpus")
        .join("keys")
}

/// Whether `GnuPG` is callable, with its version line as the evidence.
fn gpg_version() -> Option<String> {
    let out = Command::new("gpg").arg("--version").output().ok()?;
    if !out.status.success() {
        return None;
    }
    String::from_utf8(out.stdout)
        .ok()?
        .lines()
        .next()
        .map(str::to_owned)
}

#[test]
#[expect(
    clippy::panic_in_result_fn,
    reason = "the Book's Result-test shape: assertions panic, plumbing propagates with `?`"
)]
fn gpg_verify_accepts_a_sealed_bundle() -> Result<(), Box<dyn std::error::Error>> {
    let Some(version) = gpg_version() else {
        eprintln!("SKIP gpg_verify_accepts_a_sealed_bundle: no `gpg` on PATH");
        return Ok(());
    };

    let bundle = assert_fs::TempDir::new()?;
    let home = assert_fs::TempDir::new()?;
    let files = [
        RecordedFile {
            name: "verdicts.json",
            body: b"{\"verdicts\":[]}\n",
        },
        RecordedFile {
            name: "report.md",
            body: b"# report\n",
        },
    ];
    for file in &files {
        std::fs::write(bundle.path().join(file.name), file.body)?;
    }
    let sealed = seal(&files, &key_dir().join("cnf-signing.sec.asc"), None)?;
    std::fs::write(bundle.path().join("record-manifest.json"), &sealed.manifest)?;
    std::fs::write(
        bundle.path().join("record-manifest.json.asc"),
        &sealed.signature,
    )?;

    // A throwaway GNUPGHOME: the gate must never read or write the developer's
    // own keyring, and importing the public half is what a party does.
    let import = Command::new("gpg")
        .arg("--homedir")
        .arg(home.path())
        .arg("--batch")
        .arg("--import")
        .arg(key_dir().join("cnf-signing.pub.asc"))
        .output()?;
    assert!(
        import.status.success(),
        "{version}: importing the committed public key failed: {}",
        String::from_utf8_lossy(&import.stderr)
    );

    let verify = Command::new("gpg")
        .arg("--homedir")
        .arg(home.path())
        .arg("--batch")
        .arg("--verify")
        .arg(bundle.path().join("record-manifest.json.asc"))
        .arg(bundle.path().join("record-manifest.json"))
        .output()?;
    assert!(
        verify.status.success(),
        "{version}: gpg refused a bundle this instrument sealed: {}",
        String::from_utf8_lossy(&verify.stderr)
    );

    // The same GnuPG must refuse a tampered manifest, or the acceptance above
    // proves nothing about tamper-evidence.
    std::fs::write(
        bundle.path().join("record-manifest.json"),
        sealed.manifest.replace("report.md", "report.txt"),
    )?;
    let tampered = Command::new("gpg")
        .arg("--homedir")
        .arg(home.path())
        .arg("--batch")
        .arg("--verify")
        .arg(bundle.path().join("record-manifest.json.asc"))
        .arg(bundle.path().join("record-manifest.json"))
        .output()?;
    assert!(
        !tampered.status.success(),
        "{version}: gpg accepted a tampered manifest"
    );
    Ok(())
}
