// SPDX-FileCopyrightText: Veredictum contributors
// SPDX-License-Identifier: Apache-2.0

//! S9 — the public record check (#68), over the engine's signed-record
//! machinery (#62).
//!
//! Anyone can verify a published bundle here: no run, no CDR, no login. The
//! verification itself is the published lib's `record::verify_bundle` — the
//! console recomputes nothing and decides nothing, it renders what the lib
//! reports. The page prints the command-line equivalent beside every outcome,
//! so nobody has to trust the console to check the console.
//!
//! An uploaded bundle is transient. It unpacks into a scratch directory under
//! the mounted output root, is verified, and is swept on a short TTL; the
//! console keeps no state of its own.

use serde::{Deserialize, Serialize};

/// The armored public key `/verify` checks against is operator configuration;
/// this is the copy the page shows when none is mounted.
pub const NO_KEY_HINT: &str =
    "Set VEREDICTUM_VERIFY_KEY to an armored OpenPGP public key file and restart the console.";

/// The server-owned route a bundle is posted to.
pub const UPLOAD_PATH: &str = "/verify/upload";

/// The command-line equivalent, printed on every outcome.
pub const CLI_EQUIVALENT: &str = "veredictum verify-record --record <dir> --key <public-key>";

/// What verification proves, and what it does not.
///
/// Rendered on EVERY outcome, clean or not: a signature that is overread is
/// worse than no signature. One fact with the engine's own
/// `veredictum::record::HONESTY_LINE`.
pub const HONESTY_LINE: &str = crate::export::HONESTY_LINE;

/// The three sentences the honesty box spells out beneath [`HONESTY_LINE`].
pub const HONESTY_BOUNDS: [&str; 3] = [
    "It does not prove the conditions the run executed under.",
    "It does not prove the system under test is what the record says it is.",
    "It does not prove the catalogue covered everything the specification defines.",
];

/// One file's row in a rendered verification.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FileRow {
    /// The file name the manifest carries.
    pub name: String,
    /// The digest the manifest names for it.
    pub digest: String,
    /// The lib's own outcome token (`matched` / `mismatched` / `missing` /
    /// `unreadable`).
    pub outcome: String,
    /// The lib's diagnostic for anything other than a match, verbatim.
    pub detail: Option<String>,
}

/// Everything one bundle verification established.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BundleView {
    /// Whether the detached signature verified against the mounted key.
    pub signature_accepted: bool,
    /// The signer's fingerprint, when the signature verified.
    pub fingerprint: Option<String>,
    /// When the signature was made, when it verified.
    pub signed_at: Option<String>,
    /// The instrument identity the manifest carries.
    pub instrument: String,
    /// One row per file the manifest names, in manifest order.
    pub files: Vec<FileRow>,
    /// Every problem the verification found, verbatim from the lib.
    pub findings: Vec<String>,
    /// Whether the bundle verified with zero findings.
    pub is_clean: bool,
}

/// What the verify page shows.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum VerifyScreen {
    /// No bundle has been uploaded in this visit.
    Idle,
    /// A bundle was checked.
    Checked(Box<BundleView>),
    /// The upload was refused; the field is the actionable reason.
    Refused {
        /// Why the bundle could not be checked.
        reason: String,
    },
    /// No public key is mounted, so nothing can be checked at all.
    NoKey,
}

#[cfg(feature = "ssr")]
pub mod unpack {
    //! Taking a zip from an anonymous stranger, safely.
    //!
    //! Every rule here exists because this input is untrusted: a size cap
    //! before anything is written, plain file names only (the same rule the
    //! lib's `UnsafeFileName` enforces on a manifest), a per-entry cap so a
    //! decompression bomb cannot fill the disk, and a scratch directory whose
    //! name the caller cannot choose.

    use std::path::{Path, PathBuf};

    use sha2::{Digest as _, Sha256};

    use crate::state::ConsoleState;

    /// The largest upload the page accepts, in mebibytes — the one number
    /// the cap is spelled from, so the page's copy and axum's body limit
    /// cannot drift apart.
    pub const MAX_UPLOAD_MIB: u64 = 16;

    /// The largest upload the page accepts, in bytes.
    pub const MAX_UPLOAD_BYTES: u64 = MAX_UPLOAD_MIB * 1024 * 1024;

    /// The largest single file the archive may expand to.
    pub const MAX_ENTRY_BYTES: u64 = MAX_UPLOAD_BYTES;

    /// The largest total the archive may expand to.
    pub const MAX_TOTAL_BYTES: u64 = 4 * MAX_UPLOAD_BYTES;

    /// How many entries an accepted bundle may carry.
    pub const MAX_ENTRIES: usize = 256;

    /// How long an unpacked bundle survives before the next upload sweeps it.
    pub const TTL: std::time::Duration = std::time::Duration::from_mins(15);

    /// The prefix every scratch directory carries under the output root.
    pub const SCRATCH_PREFIX: &str = "console-verify-";

    /// How many digest bytes a bundle id is spelled from.
    const ID_BYTES: usize = 8;

    /// How many hex characters a bundle id carries.
    const ID_CHARS: usize = ID_BYTES * 2;

    /// The longest file name an uploaded entry may carry.
    const MAX_NAME_CHARS: usize = 128;

    /// Whether `name` is a plain bundle-relative file name.
    ///
    /// Mirrors `veredictum::record`'s own rule: an entry carrying a path
    /// separator or a parent hop would write outside the scratch directory,
    /// so it is refused rather than resolved.
    #[must_use]
    pub fn is_plain_file_name(name: &str) -> bool {
        !name.is_empty()
            && name != "."
            && name != ".."
            && !name.contains('/')
            && !name.contains('\\')
    }

    /// Rebuilds an uploaded entry's name from an allowlist, or refuses it.
    ///
    /// The returned string is CONSTRUCTED character by checked character —
    /// never the uploader's bytes reused — so nothing the uploader typed
    /// reaches a path join. The allowlist is what a record bundle's own
    /// files spell (letters, digits, dot, dash, underscore), a leading dot
    /// and over-long names refused with the reason.
    ///
    /// # Errors
    /// The actionable refusal naming the offending name.
    pub fn safe_entry_name(name: &str) -> Result<String, String> {
        if !is_plain_file_name(name) {
            return Err(format!(
                "the archive carries {name:?}, which is not a plain file name — a record bundle is one flat directory"
            ));
        }
        if name.len() > MAX_NAME_CHARS {
            return Err(format!(
                "the archive carries a {}-character file name; the page accepts at most {MAX_NAME_CHARS}",
                name.len()
            ));
        }
        if name.starts_with('.') {
            return Err(format!(
                "the archive carries the hidden file {name:?}; a record bundle carries none"
            ));
        }
        let mut rebuilt = String::with_capacity(name.len());
        for c in name.chars() {
            if c.is_ascii_alphanumeric() || matches!(c, '.' | '-' | '_') {
                rebuilt.push(c);
            } else {
                return Err(format!(
                    "the archive carries {name:?}, whose {c:?} is outside a record bundle's name alphabet (letters, digits, dot, dash, underscore)"
                ));
            }
        }
        Ok(rebuilt)
    }

    /// Whether `id` is one this console could have minted.
    ///
    /// The id arrives as a query parameter, which is user input: anything but
    /// lowercase hex of the right length is refused before it ever reaches a
    /// path join.
    #[must_use]
    pub fn is_bundle_id(id: &str) -> bool {
        id.len() == ID_CHARS
            && id
                .chars()
                .all(|c| c.is_ascii_hexdigit() && !c.is_uppercase())
    }

    /// Mints a scratch id that no two uploads share.
    ///
    /// A UNIQUENESS token, never a secret: the console has no login and binds
    /// loopback, so an unguessable id would protect nothing that the mount
    /// itself does not already expose. The process id plus a counter is
    /// enough to keep two concurrent uploads out of each other's directory,
    /// and [`bundle`] clears any directory a recycled id lands on.
    fn mint_id() -> String {
        use std::sync::atomic::{AtomicU64, Ordering};
        static COUNTER: AtomicU64 = AtomicU64::new(0);

        let seq = COUNTER.fetch_add(1, Ordering::Relaxed);
        let mut hasher = Sha256::new();
        hasher.update(std::process::id().to_le_bytes());
        hasher.update(seq.to_le_bytes());
        hasher.finalize().iter().take(ID_BYTES).fold(
            String::with_capacity(ID_CHARS),
            |mut out, byte| {
                use std::fmt::Write as _;
                let _ = write!(out, "{byte:02x}");
                out
            },
        )
    }

    /// The scratch directory for one bundle id, under the mounted output root.
    ///
    /// # Errors
    /// The refusal when the id is not one this console mints.
    pub fn scratch_dir(state: &ConsoleState, id: &str) -> Result<PathBuf, String> {
        // NOTE: the explicit `..` refusal is the guard shape CodeQL's
        // rust/path-injection query models; is_bundle_id already implies it.
        if id.contains("..") || !is_bundle_id(id) {
            return Err(String::from("not a bundle this console unpacked"));
        }
        // The joined id is REBUILT from the checked characters, so no byte
        // of the query parameter itself reaches the filesystem path.
        let rebuilt: String = id.chars().filter(char::is_ascii_hexdigit).collect();
        Ok(state.out.join(format!("{SCRATCH_PREFIX}{rebuilt}")))
    }

    /// Removes every scratch directory older than [`TTL`].
    ///
    /// Best effort by design: a directory that cannot be read or removed is
    /// skipped rather than turned into an upload failure, because sweeping is
    /// housekeeping and never the caller's business.
    pub fn sweep(state: &ConsoleState) {
        let Ok(entries) = std::fs::read_dir(&state.out) else {
            return;
        };
        let now = jiff::Timestamp::now();
        for entry in entries.flatten() {
            let name = entry.file_name();
            let Some(name) = name.to_str() else { continue };
            if !name.starts_with(SCRATCH_PREFIX) || name.contains("..") {
                continue;
            }
            let expired = entry
                .metadata()
                .and_then(|meta| meta.modified())
                .ok()
                .and_then(|at| jiff::Timestamp::try_from(at).ok())
                .and_then(|at| now.since(at).ok())
                .and_then(|age| std::time::Duration::try_from(age).ok())
                .is_some_and(|age| age > TTL);
            if expired {
                drop(std::fs::remove_dir_all(entry.path()));
            }
        }
    }

    /// Unpacks one uploaded archive into a fresh scratch directory, returning
    /// its id.
    ///
    /// # Errors
    /// The actionable refusal: too large, not an archive, an entry that is
    /// not a plain file name, too many entries, or a filesystem failure.
    pub fn bundle(state: &ConsoleState, body: &[u8]) -> Result<String, String> {
        let size = u64::try_from(body.len()).unwrap_or(u64::MAX);
        if size > MAX_UPLOAD_BYTES {
            return Err(format!(
                "the upload is {size} bytes; the page accepts at most {MAX_UPLOAD_BYTES}"
            ));
        }
        sweep(state);

        let mut archive = zip::ZipArchive::new(std::io::Cursor::new(body))
            .map_err(|e| format!("not a readable zip archive: {e}"))?;
        if archive.len() > MAX_ENTRIES {
            return Err(format!(
                "the archive carries {} entries; a record bundle carries at most {MAX_ENTRIES}",
                archive.len()
            ));
        }

        let id = mint_id();
        let dir = scratch_dir(state, &id)?;
        // A recycled id (a restarted process on the same host) must never
        // inherit an older bundle's files, so the directory starts empty.
        drop(std::fs::remove_dir_all(&dir));
        std::fs::create_dir_all(&dir).map_err(|e| format!("{}: {e}", dir.display()))?;
        match fill(&mut archive, &dir) {
            Ok(()) => Ok(id),
            Err(reason) => {
                // A refused archive leaves nothing behind: whatever was
                // written before the bad entry goes with it.
                drop(std::fs::remove_dir_all(&dir));
                Err(reason)
            }
        }
    }

    /// Writes every entry of a validated archive into `dir`.
    fn fill(
        archive: &mut zip::ZipArchive<std::io::Cursor<&[u8]>>,
        dir: &Path,
    ) -> Result<(), String> {
        let mut total: u64 = 0;
        for index in 0..archive.len() {
            let mut entry = archive
                .by_index(index)
                .map_err(|e| format!("entry {index}: {e}"))?;
            if entry.is_dir() {
                continue;
            }
            // The RAW name, never a resolved one: resolving is exactly the
            // step that lets `../` escape. The on-disk name is then REBUILT
            // from the allowlist, so no uploader byte reaches the join.
            let name = safe_entry_name(entry.name())?;
            if name.contains("..") {
                return Err(format!(
                    "the archive carries {name:?}, which is not a plain file name — a record bundle is one flat directory"
                ));
            }
            let declared = entry.size();
            if declared > MAX_ENTRY_BYTES {
                return Err(format!(
                    "{name} declares {declared} bytes; the page accepts at most {MAX_ENTRY_BYTES} per file"
                ));
            }
            total = total.saturating_add(declared);
            if total > MAX_TOTAL_BYTES {
                return Err(format!(
                    "the archive expands to more than {MAX_TOTAL_BYTES} bytes"
                ));
            }
            let mut body = Vec::new();
            // The reader is capped independently of the declared size: a zip
            // header is the uploader's claim, not a measurement.
            std::io::Read::read_to_end(
                &mut std::io::Read::take(&mut entry, MAX_ENTRY_BYTES),
                &mut body,
            )
            .map_err(|e| format!("{name}: {e}"))?;
            std::fs::write(dir.join(&name), body).map_err(|e| format!("{name}: {e}"))?;
        }
        Ok(())
    }
}

#[cfg(feature = "ssr")]
pub mod read {
    //! The ssr reader: the lib's verification, rendered.

    use super::{BundleView, FileRow, VerifyScreen};
    use crate::state::ConsoleState;

    /// Verifies one previously unpacked bundle, or explains why it cannot.
    ///
    /// Infallible on purpose: an unverifiable bundle is an ANSWER, so every
    /// refusal renders as [`VerifyScreen::Refused`] rather than as a failed
    /// read the surface would have to translate.
    #[must_use]
    pub fn screen(state: &ConsoleState, bundle: Option<&str>) -> VerifyScreen {
        let Some(key) = state.verify_key.as_ref() else {
            return VerifyScreen::NoKey;
        };
        let Some(id) = bundle else {
            return VerifyScreen::Idle;
        };
        let dir = match super::unpack::scratch_dir(state, id) {
            Ok(dir) => dir,
            Err(reason) => return VerifyScreen::Refused { reason },
        };
        if !dir.is_dir() {
            return VerifyScreen::Refused {
                reason: String::from(
                    "that bundle is no longer here: uploads are transient and are swept on a short timer. Upload it again.",
                ),
            };
        }
        match veredictum::record::verify_bundle(&dir, key) {
            Ok(verification) => VerifyScreen::Checked(Box::new(view_of(&verification))),
            Err(e) => VerifyScreen::Refused {
                reason: e.to_string(),
            },
        }
    }

    /// Maps the lib's verification onto the wire type, token for token.
    fn view_of(verification: &veredictum::record::BundleVerification) -> BundleView {
        let (accepted, fingerprint, signed_at) = match &verification.signature {
            veredictum::record::SignatureOutcome::Accepted(record) => (
                true,
                Some(record.signer_fingerprint.clone()),
                Some(record.signed_at.to_string()),
            ),
            veredictum::record::SignatureOutcome::Rejected => (false, None, None),
        };
        BundleView {
            signature_accepted: accepted,
            fingerprint,
            signed_at,
            instrument: format!(
                "{} {}",
                verification.instrument.name, verification.instrument.version
            ),
            files: verification.files.iter().map(row_of).collect(),
            findings: verification.findings(),
            is_clean: verification.is_clean(),
        }
    }

    /// One file verdict as a row; the token vocabulary is the lib's own.
    fn row_of(file: &veredictum::record::FileVerdict) -> FileRow {
        let (outcome, detail) = match &file.outcome {
            veredictum::record::DigestOutcome::Matches => ("matched", None),
            veredictum::record::DigestOutcome::Mismatch { recomputed } => {
                ("mismatched", Some(format!("recomputed {recomputed}")))
            }
            veredictum::record::DigestOutcome::Missing => (
                "missing",
                Some(String::from("named by the manifest, absent")),
            ),
            veredictum::record::DigestOutcome::Unreadable { message } => {
                ("unreadable", Some(message.clone()))
            }
        };
        FileRow {
            name: file.name.clone(),
            digest: file.digest.clone(),
            outcome: outcome.to_owned(),
            detail,
        }
    }
}

#[cfg(feature = "ssr")]
pub mod route {
    //! The server-owned upload route.
    //!
    //! A plain `<form method="post" enctype="multipart/form-data">` posts
    //! here and is answered with a redirect back to the page. That is the
    //! whole mechanism: a file upload with zero JavaScript, working before
    //! the WASM bundle has loaded and working with it disabled entirely.

    use crate::redirect::{percent_encode, see_other};

    /// Accepts one uploaded bundle and redirects to its verification.
    ///
    /// Every refusal is a redirect too, carrying its reason in the query, so
    /// the answer is always the page rather than a bare error body.
    pub async fn upload(
        axum::Extension(state): axum::Extension<crate::state::ConsoleState>,
        mut form: axum::extract::Multipart,
    ) -> axum::response::Response {
        let mut body: Option<Vec<u8>> = None;
        loop {
            match form.next_field().await {
                Ok(Some(field)) => {
                    if field.name() != Some("bundle") {
                        continue;
                    }
                    match field.bytes().await {
                        Ok(bytes) => body = Some(bytes.to_vec()),
                        Err(e) => return refused(&format!("the upload did not arrive whole: {e}")),
                    }
                }
                Ok(None) => break,
                Err(e) => return refused(&format!("the upload could not be read: {e}")),
            }
        }
        let Some(body) = body.filter(|b| !b.is_empty()) else {
            return refused("no file was chosen");
        };
        match crate::verify_api::unpack::bundle(&state, &body) {
            Ok(id) => see_other(&format!("{}?bundle={id}", crate::export_api::VERIFY_PATH)),
            Err(reason) => refused(&reason),
        }
    }

    /// Redirects back to the page carrying a refusal reason.
    fn refused(reason: &str) -> axum::response::Response {
        see_other(&format!(
            "{}?refused={}",
            crate::export_api::VERIFY_PATH,
            percent_encode(reason)
        ))
    }
}

pub mod fns {
    //! The `#[server]` endpoints, one module for one inner suppression.
    //!
    //! The same adjudication as `catalogue_api::fns`: macro-expanded
    //! `unused_async` and `missing_docs`, module-scoped, signed off in the
    //! pull request.
    #![allow(
        clippy::unused_async,
        missing_docs,
        reason = "fires only in some #[server] expansions; see catalogue_api::fns"
    )]

    use leptos::prelude::{ServerFnError, server};

    use super::VerifyScreen;

    /// Verifies one previously uploaded bundle.
    ///
    /// The id is user input and is treated as such: only an id this console
    /// could have minted resolves to a path at all.
    ///
    /// # Errors
    /// Never on its own account — the signature is a server fn's, and every
    /// refusal travels as a [`VerifyScreen`] variant.
    #[server]
    pub async fn fetch_verification(bundle: Option<String>) -> Result<VerifyScreen, ServerFnError> {
        let state: crate::state::ConsoleState = leptos::prelude::expect_context();
        Ok(super::read::screen(&state, bundle.as_deref()))
    }
}

#[cfg(all(test, feature = "ssr"))]
mod tests {
    use super::unpack::{is_bundle_id, is_plain_file_name};

    /// The rule that keeps an uploaded archive inside its scratch directory.
    #[test]
    fn only_plain_entry_names_are_accepted() {
        assert!(is_plain_file_name("record-manifest.json"));
        assert!(is_plain_file_name("CONFORMANCE_REPORT.md"));
        assert!(!is_plain_file_name(""));
        assert_eq!(
            super::unpack::safe_entry_name("record-manifest.json.asc").as_deref(),
            Ok("record-manifest.json.asc")
        );
        assert!(super::unpack::safe_entry_name(".hidden").is_err());
        assert!(super::unpack::safe_entry_name("na me.json").is_err());
        assert!(super::unpack::safe_entry_name("nam\u{202e}e.json").is_err());
        assert!(super::unpack::safe_entry_name(&"n".repeat(200)).is_err());
        assert!(!is_plain_file_name("."));
        assert!(!is_plain_file_name(".."));
        assert!(!is_plain_file_name("../outside.json"));
        assert!(!is_plain_file_name("nested/report.md"));
        assert!(!is_plain_file_name("nested\\report.md"));
        assert!(!is_plain_file_name("/etc/passwd"));
    }

    /// The bundle id arrives in a query string, so it is user input.
    #[test]
    fn only_a_minted_bundle_id_resolves() {
        assert!(is_bundle_id("0123456789abcdef"));
        assert!(!is_bundle_id("0123456789ABCDEF"), "uppercase is not minted");
        assert!(!is_bundle_id("0123456789abcde"), "too short");
        assert!(!is_bundle_id("0123456789abcdef0"), "too long");
        assert!(
            !is_bundle_id("../../../etc/pas"),
            "a traversal of the right length"
        );
        assert!(!is_bundle_id(""));
    }

    /// The honesty box says the same thing the engine says.
    #[test]
    fn the_honesty_line_matches_the_engines() {
        assert_eq!(super::HONESTY_LINE, veredictum::record::HONESTY_LINE);
    }
}
