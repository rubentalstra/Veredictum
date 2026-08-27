// SPDX-FileCopyrightText: Veredictum contributors
// SPDX-License-Identifier: Apache-2.0

//! The signed run record: a digest manifest over the documents a run or a
//! judgement emitted, a detached OpenPGP signature over that manifest, and the
//! verification that recomputes both.
//!
//! No openEHR spec governs this — our own design. The bundle is ordinary
//! files: the manifest is JSON, the signature is an armored RFC 9580 detached
//! signature over the manifest's exact bytes, so
//! `gpg --verify record-manifest.json.asc record-manifest.json` accepts a
//! bundle without this tool ever running. The signing machinery is the same
//! `rpgp` path [`crate::exec::signature`] already uses to verify a SUT's
//! version signatures, pointed at the instrument's own output.
//!
//! The manifest is byte-deterministic. File names live in a [`BTreeMap`], so
//! they render sorted; JSON key order is fixed by the field order of
//! [`RecordManifest`]. Re-rendering the same documents therefore reproduces
//! the same bytes, which is what makes one signature re-checkable against a
//! regenerated bundle.
//!
//! A valid signature proves integrity and origin since signing. It says
//! nothing about the conditions the run executed under, which is why
//! [`HONESTY_LINE`] ships in the verification output.

use std::collections::BTreeMap;
use std::fmt::Write as _;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use sha2::{Digest as _, Sha256};
use thiserror::Error;

/// The manifest file name a sealed bundle carries.
pub const MANIFEST_FILE: &str = "record-manifest.json";

/// The detached-signature file name, beside the manifest it covers.
pub const SIGNATURE_FILE: &str = "record-manifest.json.asc";

/// What a verified signature does and does not establish, printed with every
/// verification so the claim is never overread.
pub const HONESTY_LINE: &str =
    "A valid signature proves integrity and origin since signing — not the run's conditions.";

/// The instrument identity stamped into every manifest.
const INSTRUMENT_NAME: &str = "veredictum";

/// A failure of the signed-record machinery.
///
/// Every variant here means the check could not be RUN: unreadable input, a
/// malformed key, a manifest that does not parse. A signature that verifies
/// against nothing, and a digest that does not match, are findings rather than
/// errors, and travel as [`SignatureOutcome::Rejected`] and
/// [`DigestOutcome`].
#[derive(Debug, Error)]
pub enum RecordError {
    /// Two documents claimed the same name, so one digest would silently
    /// replace the other.
    #[error("record manifest: duplicate file name {name:?}")]
    DuplicateFile {
        /// The name that appeared twice.
        name: String,
    },
    /// A manifest entry is not a plain file name, so recomputing its digest
    /// would read outside the bundle directory.
    #[error("record manifest: {name:?} is not a plain file name (no path separators, no `..`)")]
    UnsafeFileName {
        /// The rejected entry.
        name: String,
    },
    /// The manifest could not be serialized.
    #[error("record manifest: cannot serialize: {source}")]
    Serialize {
        /// The serializer's own diagnostic.
        #[source]
        source: serde_json::Error,
    },
    /// The manifest document did not parse as a manifest.
    #[error("record manifest: cannot parse {path}: {message}")]
    ManifestParse {
        /// The document that did not parse.
        path: PathBuf,
        /// The parser's own diagnostic.
        message: String,
    },
    /// A file could not be read.
    #[error("cannot read {path}: {source}")]
    Read {
        /// The file that could not be read.
        path: PathBuf,
        /// The underlying filesystem error.
        #[source]
        source: std::io::Error,
    },
    /// The armored secret key did not parse, or signing with it failed.
    #[error("signing key: {message}")]
    SigningKey {
        /// The diagnostic, which never carries key material.
        message: String,
    },
    /// The armored public key did not parse.
    #[error("public key: {message}")]
    PublicKey {
        /// The parser's own diagnostic.
        message: String,
    },
    /// The armored detached signature did not parse, or could not be
    /// re-armored after signing.
    #[error("detached signature: {message}")]
    Signature {
        /// The diagnostic.
        message: String,
    },
    /// The detached signature carries no creation time, so it cannot say when
    /// it was made.
    ///
    /// The signature creation time is a hashed subpacket every conforming
    /// signature carries
    /// (<https://www.rfc-editor.org/rfc/rfc9580.html#name-signature-creation-time>).
    #[error("detached signature: no creation time subpacket")]
    NoSigningTime,
    /// The creation time the signature carries is outside the range a
    /// timestamp can represent.
    #[error("detached signature: creation time is not representable: {message}")]
    SigningTime {
        /// The time library's own diagnostic.
        message: String,
    },
}

/// The digest algorithm a manifest's file digests are taken with.
///
/// A closed vocabulary: an unknown token in a committed manifest is a parse
/// failure, never a silent fallback that would compare digests under the
/// wrong algorithm.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum DigestAlgorithm {
    /// SHA-256, lowercase hex.
    Sha256,
}

/// The instrument that produced a record.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Instrument {
    /// The instrument's package name.
    pub name: String,
    /// The instrument's package version.
    pub version: String,
}

/// The digest manifest over one bundle's emitted documents.
///
/// Field order here IS the JSON key order, and [`Self::files`] renders sorted
/// by name, so the rendered document is byte-deterministic.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RecordManifest {
    /// The instrument that produced the documents.
    pub instrument: Instrument,
    /// The algorithm every digest below was taken with.
    pub digest_algorithm: DigestAlgorithm,
    /// Each document's bundle-relative name mapped to its digest.
    pub files: BTreeMap<String, String>,
}

/// One document a record covers.
#[derive(Debug, Clone, Copy)]
pub struct RecordedFile<'a> {
    /// The file name, relative to the bundle directory.
    pub name: &'a str,
    /// The complete file body, exactly as written.
    pub body: &'a [u8],
}

/// The two files sealing a bundle adds to it.
#[derive(Debug, Clone)]
pub struct SealedRecord {
    /// The manifest document, to be written as [`MANIFEST_FILE`].
    pub manifest: String,
    /// The armored detached signature, to be written as [`SIGNATURE_FILE`].
    pub signature: String,
}

/// What a detached signature established about a manifest.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VerifiedRecord {
    /// The fingerprint of the key component that verified the signature,
    /// lowercase hex.
    pub signer_fingerprint: String,
    /// When the signature was made, from its creation-time subpacket.
    pub signed_at: jiff::Timestamp,
}

/// Whether the supplied public key verified the detached signature.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SignatureOutcome {
    /// A key component of the supplied certificate verified the signature.
    Accepted(VerifiedRecord),
    /// No key component of the supplied certificate verified the signature,
    /// so the manifest bytes and the key do not belong together.
    Rejected,
}

/// What recomputing one file's digest found.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DigestOutcome {
    /// The recomputed digest equals the one the manifest names.
    Matches,
    /// The file is present and its digest differs from the manifest's.
    Mismatch {
        /// The digest the bundle's bytes actually produce.
        recomputed: String,
    },
    /// The manifest names a file the bundle does not carry.
    Missing,
    /// The file is named and present but could not be read.
    Unreadable {
        /// The filesystem's own diagnostic.
        message: String,
    },
}

/// One file's row in a bundle verification.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileVerdict {
    /// The file name, relative to the bundle directory.
    pub name: String,
    /// The digest the manifest names for it.
    pub digest: String,
    /// What recomputing its digest found.
    pub outcome: DigestOutcome,
}

/// Everything one bundle verification established.
///
/// Zero findings is the only passing result: [`Self::findings`] is empty only
/// when the signature was accepted and every file the manifest names
/// reproduced its digest.
#[derive(Debug, Clone)]
pub struct BundleVerification {
    /// Whether the detached signature verified, and what it says if it did.
    pub signature: SignatureOutcome,
    /// The instrument identity the manifest carries.
    pub instrument: Instrument,
    /// One row per file the manifest names, in sorted manifest order.
    pub files: Vec<FileVerdict>,
}

impl BundleVerification {
    /// Every problem the verification found, one diagnostic per line.
    #[must_use]
    pub fn findings(&self) -> Vec<String> {
        let mut findings = Vec::new();
        if self.signature == SignatureOutcome::Rejected {
            findings.push(format!(
                "{MANIFEST_FILE}: the detached signature does not verify against the supplied public key"
            ));
        }
        for file in &self.files {
            match &file.outcome {
                DigestOutcome::Matches => {}
                DigestOutcome::Mismatch { recomputed } => findings.push(format!(
                    "{}: digest mismatch (manifest {}, recomputed {recomputed})",
                    file.name, file.digest
                )),
                DigestOutcome::Missing => {
                    findings.push(format!("{}: named by the manifest, absent", file.name));
                }
                DigestOutcome::Unreadable { message } => {
                    findings.push(format!("{}: unreadable: {message}", file.name));
                }
            }
        }
        findings
    }

    /// Whether the bundle verified with zero findings.
    #[must_use]
    pub fn is_clean(&self) -> bool {
        self.findings().is_empty()
    }
}

/// Renders the byte-deterministic digest manifest over `files`.
///
/// # Errors
/// [`RecordError::DuplicateFile`] when two documents share a name,
/// [`RecordError::UnsafeFileName`] when one is not a plain file name, and
/// [`RecordError::Serialize`] when the manifest cannot be serialized.
pub fn manifest(files: &[RecordedFile<'_>]) -> Result<String, RecordError> {
    let mut digests: BTreeMap<String, String> = BTreeMap::new();
    for file in files {
        if !is_plain_file_name(file.name) {
            return Err(RecordError::UnsafeFileName {
                name: file.name.to_owned(),
            });
        }
        if digests
            .insert(file.name.to_owned(), hex(&Sha256::digest(file.body)))
            .is_some()
        {
            return Err(RecordError::DuplicateFile {
                name: file.name.to_owned(),
            });
        }
    }
    render(&RecordManifest {
        instrument: Instrument {
            name: INSTRUMENT_NAME.to_owned(),
            version: env!("CARGO_PKG_VERSION").to_owned(),
        },
        digest_algorithm: DigestAlgorithm::Sha256,
        files: digests,
    })
}

/// Signs `manifest_bytes` with an armored secret key, returning the armored
/// detached signature.
///
/// The signing component is a signing-flagged subkey where the certificate
/// carries one, and the primary key otherwise, which is the same key-usage
/// order [`crate::exec::signature`] accepts on the verifying side (RFC 9580
/// key flag 0x02 marks a component as usable to sign data).
///
/// # Errors
/// [`RecordError::SigningKey`] when the key does not parse or the passphrase
/// does not unlock it, and [`RecordError::Signature`] when the signature
/// cannot be armored.
pub fn sign(
    manifest_bytes: &[u8],
    secret_key_armored: &str,
    passphrase: Option<&str>,
) -> Result<String, RecordError> {
    use pgp::composed::{ArmorOptions, Deserializable as _, DetachedSignature, SignedSecretKey};
    use pgp::crypto::hash::HashAlgorithm;
    use pgp::types::Password;
    use rand::rngs::OsRng;

    let (secret, _) =
        SignedSecretKey::from_string(secret_key_armored).map_err(|e| RecordError::SigningKey {
            message: e.to_string(),
        })?;
    let password = passphrase.map_or_else(Password::empty, Password::from);
    let signing_subkey = secret
        .secret_subkeys
        .iter()
        .find(|subkey| subkey.signatures.iter().any(|sig| sig.key_flags().sign()));
    let signature = match signing_subkey {
        Some(subkey) => DetachedSignature::sign_binary_data(
            OsRng,
            &subkey.key,
            &password,
            HashAlgorithm::Sha256,
            manifest_bytes,
        ),
        None => DetachedSignature::sign_binary_data(
            OsRng,
            &secret.primary_key,
            &password,
            HashAlgorithm::Sha256,
            manifest_bytes,
        ),
    }
    .map_err(|e| RecordError::SigningKey {
        message: e.to_string(),
    })?;
    signature
        .to_armored_string(ArmorOptions::default())
        .map_err(|e| RecordError::Signature {
            message: e.to_string(),
        })
}

/// Verifies an armored detached signature over `manifest_bytes` against an
/// armored public key.
///
/// A signature that verifies against no component of the certificate is
/// [`SignatureOutcome::Rejected`], which is a finding rather than an error:
/// the check ran and it failed.
///
/// # Errors
/// [`RecordError::PublicKey`] or [`RecordError::Signature`] when either input
/// does not parse, [`RecordError::NoSigningTime`] when the signature carries
/// no creation time, and [`RecordError::SigningTime`] when that time is not
/// representable.
pub fn verify(
    manifest_bytes: &[u8],
    signature_armored: &str,
    public_key_armored: &str,
) -> Result<SignatureOutcome, RecordError> {
    use pgp::composed::{Deserializable as _, DetachedSignature, SignedPublicKey};
    use pgp::types::KeyDetails as _;

    let (key, _) =
        SignedPublicKey::from_string(public_key_armored).map_err(|e| RecordError::PublicKey {
            message: e.to_string(),
        })?;
    let (signature, _) =
        DetachedSignature::from_string(signature_armored).map_err(|e| RecordError::Signature {
            message: e.to_string(),
        })?;
    let created = signature
        .signature
        .created()
        .ok_or(RecordError::NoSigningTime)?;
    let signed_at = jiff::Timestamp::from_second(i64::from(created.as_secs())).map_err(|e| {
        RecordError::SigningTime {
            message: e.to_string(),
        }
    })?;

    if signature.verify(&key, manifest_bytes).is_ok() {
        return Ok(SignatureOutcome::Accepted(VerifiedRecord {
            signer_fingerprint: key.fingerprint().to_string(),
            signed_at,
        }));
    }
    for subkey in &key.public_subkeys {
        if signature.verify(subkey, manifest_bytes).is_ok() {
            return Ok(SignatureOutcome::Accepted(VerifiedRecord {
                signer_fingerprint: subkey.fingerprint().to_string(),
                signed_at,
            }));
        }
    }
    Ok(SignatureOutcome::Rejected)
}

/// Seals a set of finished documents: the digest manifest plus its detached
/// signature, ready to be written beside them.
///
/// # Errors
/// [`RecordError::Read`] when the secret key file cannot be read, plus
/// whatever [`manifest`] and [`sign`] report.
pub fn seal(
    files: &[RecordedFile<'_>],
    secret_key_path: &Path,
    passphrase: Option<&str>,
) -> Result<SealedRecord, RecordError> {
    let secret_key_armored = read_text(secret_key_path)?;
    let manifest = manifest(files)?;
    let signature = sign(manifest.as_bytes(), &secret_key_armored, passphrase)?;
    Ok(SealedRecord {
        manifest,
        signature,
    })
}

/// Verifies one sealed bundle directory against an armored public key file.
///
/// The signature is checked over the manifest's exact bytes as read, then
/// every file the manifest names has its digest recomputed from the bundle.
///
/// # Errors
/// [`RecordError::Read`] when the manifest, the signature or the key cannot
/// be read, [`RecordError::ManifestParse`] when the manifest does not parse,
/// [`RecordError::UnsafeFileName`] when it names something outside the
/// bundle, and whatever [`verify`] reports.
pub fn verify_bundle(
    bundle_dir: &Path,
    public_key_path: &Path,
) -> Result<BundleVerification, RecordError> {
    let manifest_path = bundle_dir.join(MANIFEST_FILE);
    let manifest_bytes = std::fs::read(&manifest_path).map_err(|source| RecordError::Read {
        path: manifest_path.clone(),
        source,
    })?;
    let signature_armored = read_text(&bundle_dir.join(SIGNATURE_FILE))?;
    let public_key_armored = read_text(public_key_path)?;

    let signature = verify(&manifest_bytes, &signature_armored, &public_key_armored)?;
    let parsed: RecordManifest =
        serde_json::from_slice(&manifest_bytes).map_err(|e| RecordError::ManifestParse {
            path: manifest_path,
            message: e.to_string(),
        })?;

    let mut files = Vec::with_capacity(parsed.files.len());
    for (name, expected) in &parsed.files {
        if !is_plain_file_name(name) {
            return Err(RecordError::UnsafeFileName { name: name.clone() });
        }
        files.push(FileVerdict {
            name: name.clone(),
            digest: expected.clone(),
            outcome: digest_outcome(&bundle_dir.join(name), expected, parsed.digest_algorithm),
        });
    }
    Ok(BundleVerification {
        signature,
        instrument: parsed.instrument,
        files,
    })
}

/// Recomputes one file's digest and compares it against the manifest's.
fn digest_outcome(path: &Path, expected: &str, algorithm: DigestAlgorithm) -> DigestOutcome {
    let bytes = match std::fs::read(path) {
        Ok(bytes) => bytes,
        Err(source) if source.kind() == std::io::ErrorKind::NotFound => {
            return DigestOutcome::Missing;
        }
        Err(source) => {
            return DigestOutcome::Unreadable {
                message: source.to_string(),
            };
        }
    };
    let recomputed = match algorithm {
        DigestAlgorithm::Sha256 => hex(&Sha256::digest(&bytes)),
    };
    if recomputed == expected {
        DigestOutcome::Matches
    } else {
        DigestOutcome::Mismatch { recomputed }
    }
}

/// Renders a manifest as the pretty-JSON-with-trailing-newline form every
/// artifact this instrument writes uses.
fn render(value: &RecordManifest) -> Result<String, RecordError> {
    let mut text =
        serde_json::to_string_pretty(value).map_err(|source| RecordError::Serialize { source })?;
    text.push('\n');
    Ok(text)
}

/// Reads one text file, naming it in the diagnostic.
fn read_text(path: &Path) -> Result<String, RecordError> {
    std::fs::read_to_string(path).map_err(|source| RecordError::Read {
        path: path.to_owned(),
        source,
    })
}

/// Whether `name` is a plain bundle-relative file name.
///
/// A manifest entry carrying a path separator or a parent-directory hop would
/// send digest recomputation outside the bundle, so it is refused rather than
/// resolved.
fn is_plain_file_name(name: &str) -> bool {
    !name.is_empty() && name != "." && name != ".." && !name.contains('/') && !name.contains('\\')
}

/// Lowercase hex, the encoding every digest in a manifest carries.
fn hex(bytes: &[u8]) -> String {
    bytes.iter().fold(
        String::with_capacity(bytes.len().saturating_mul(2)),
        |mut out, byte| {
            let _ = write!(out, "{byte:02x}");
            out
        },
    )
}

#[cfg(test)]
#[expect(
    clippy::panic_in_result_fn,
    reason = "nine Result-returning tests in the Book ch11 shape, each asserting; \
              clippy offers no allow-in-tests knob for this lint"
)]
mod tests {
    use super::*;

    /// The committed test keypair: a primary key certifying a signing subkey,
    /// with no passphrase. The armored certificate is self-describing — its
    /// packets carry the user id, the key flags and the subkey binding
    /// signature, so nothing outside the key material states its identity.
    fn key_dir() -> PathBuf {
        Path::new(concat!(env!("CARGO_MANIFEST_DIR"), "/../..")).join("artifacts/corpus/keys")
    }

    fn secret_key() -> Result<String, RecordError> {
        read_text(&key_dir().join("cnf-signing.sec.asc"))
    }

    fn public_key() -> Result<String, RecordError> {
        read_text(&key_dir().join("cnf-signing.pub.asc"))
    }

    /// A second certificate, so "wrong key" is a real key rather than
    /// malformed bytes. Ed25519 keygen is instant, unlike the RSA fixture.
    fn other_public_key() -> Result<String, Box<dyn std::error::Error>> {
        use pgp::composed::{KeyType, SecretKeyParamsBuilder};

        let params = SecretKeyParamsBuilder::default()
            .key_type(KeyType::Ed25519)
            .can_sign(true)
            .can_certify(true)
            .primary_user_id("Veredictum Record Test <record@test.invalid>".to_owned())
            .build()?;
        let generated = params.generate(rand::rngs::OsRng)?;
        Ok(generated.to_public_key().to_armored_string(None.into())?)
    }

    fn documents<'a>() -> [RecordedFile<'a>; 3] {
        [
            RecordedFile {
                name: "verdicts.json",
                body: b"{\"verdicts\":[]}\n",
            },
            RecordedFile {
                name: "report.md",
                body: b"# report\n",
            },
            RecordedFile {
                name: "certificate.md",
                body: b"# certificate\n",
            },
        ]
    }

    /// Writes the three documents plus a sealed manifest into `dir`.
    fn seal_into(dir: &Path) -> Result<(), Box<dyn std::error::Error>> {
        let files = documents();
        for file in &files {
            std::fs::write(dir.join(file.name), file.body)?;
        }
        let sealed = seal(&files, &key_dir().join("cnf-signing.sec.asc"), None)?;
        std::fs::write(dir.join(MANIFEST_FILE), &sealed.manifest)?;
        std::fs::write(dir.join(SIGNATURE_FILE), &sealed.signature)?;
        Ok(())
    }

    #[test]
    fn manifest_is_byte_deterministic_across_renders() -> Result<(), RecordError> {
        let first = manifest(&documents())?;
        let second = manifest(&documents())?;
        assert_eq!(first, second);
        // Input order must not move a byte: the names render sorted.
        let mut reordered = documents();
        reordered.reverse();
        assert_eq!(first, manifest(&reordered)?);
        Ok(())
    }

    #[test]
    fn manifest_names_every_document_with_its_digest() -> Result<(), Box<dyn std::error::Error>> {
        let rendered = manifest(&documents())?;
        let parsed: RecordManifest = serde_json::from_str(&rendered)?;
        assert_eq!(parsed.instrument.name, INSTRUMENT_NAME);
        assert_eq!(parsed.instrument.version, env!("CARGO_PKG_VERSION"));
        assert_eq!(parsed.digest_algorithm, DigestAlgorithm::Sha256);
        assert_eq!(parsed.files.len(), 3);
        assert_eq!(
            parsed.files.get("report.md"),
            Some(&hex(&Sha256::digest(b"# report\n")))
        );
        // Sorted key order, so the rendered bytes are reproducible.
        assert_eq!(
            parsed.files.keys().cloned().collect::<Vec<_>>(),
            vec![
                "certificate.md".to_owned(),
                "report.md".to_owned(),
                "verdicts.json".to_owned()
            ]
        );
        Ok(())
    }

    #[test]
    fn a_manifest_entry_that_escapes_the_bundle_is_refused() {
        let escaping = [RecordedFile {
            name: "../outside.json",
            body: b"{}",
        }];
        assert!(matches!(
            manifest(&escaping),
            Err(RecordError::UnsafeFileName { .. })
        ));
    }

    #[test]
    fn a_duplicate_document_name_is_refused() {
        let duplicated = [
            RecordedFile {
                name: "results.json",
                body: b"{}",
            },
            RecordedFile {
                name: "results.json",
                body: b"{\"other\":true}",
            },
        ];
        assert!(matches!(
            manifest(&duplicated),
            Err(RecordError::DuplicateFile { .. })
        ));
    }

    #[test]
    fn a_good_bundle_verifies_clean() -> Result<(), Box<dyn std::error::Error>> {
        let dir = assert_fs::TempDir::new()?;
        seal_into(dir.path())?;
        let verification = verify_bundle(dir.path(), &key_dir().join("cnf-signing.pub.asc"))?;
        assert_eq!(verification.findings(), Vec::<String>::new());
        assert!(verification.is_clean());
        let SignatureOutcome::Accepted(record) = &verification.signature else {
            panic!("the committed keypair must verify its own signature");
        };
        assert!(!record.signer_fingerprint.is_empty());
        assert_eq!(verification.files.len(), 3);
        Ok(())
    }

    #[test]
    fn a_tampered_file_fails_naming_that_file() -> Result<(), Box<dyn std::error::Error>> {
        let dir = assert_fs::TempDir::new()?;
        seal_into(dir.path())?;
        std::fs::write(dir.path().join("report.md"), b"# report (edited)\n")?;
        let verification = verify_bundle(dir.path(), &key_dir().join("cnf-signing.pub.asc"))?;
        assert!(!verification.is_clean());
        let findings = verification.findings();
        assert_eq!(findings.len(), 1, "{findings:?}");
        assert!(
            findings
                .first()
                .is_some_and(|f| f.starts_with("report.md: digest mismatch")),
            "{findings:?}"
        );
        Ok(())
    }

    #[test]
    fn a_missing_file_fails_naming_that_file() -> Result<(), Box<dyn std::error::Error>> {
        let dir = assert_fs::TempDir::new()?;
        seal_into(dir.path())?;
        std::fs::remove_file(dir.path().join("verdicts.json"))?;
        let verification = verify_bundle(dir.path(), &key_dir().join("cnf-signing.pub.asc"))?;
        assert!(!verification.is_clean());
        assert_eq!(
            verification.findings(),
            vec!["verdicts.json: named by the manifest, absent".to_owned()]
        );
        Ok(())
    }

    #[test]
    fn a_tampered_manifest_fails_the_signature() -> Result<(), Box<dyn std::error::Error>> {
        let dir = assert_fs::TempDir::new()?;
        seal_into(dir.path())?;
        let mut manifest_text = std::fs::read_to_string(dir.path().join(MANIFEST_FILE))?;
        manifest_text = manifest_text.replace("veredictum", "veredictum-forged");
        std::fs::write(dir.path().join(MANIFEST_FILE), &manifest_text)?;
        let verification = verify_bundle(dir.path(), &key_dir().join("cnf-signing.pub.asc"))?;
        assert_eq!(verification.signature, SignatureOutcome::Rejected);
        assert!(!verification.is_clean());
        Ok(())
    }

    #[test]
    fn a_wrong_public_key_fails() -> Result<(), Box<dyn std::error::Error>> {
        let dir = assert_fs::TempDir::new()?;
        seal_into(dir.path())?;
        let other = dir.path().join("other.pub.asc");
        std::fs::write(&other, other_public_key()?)?;
        let verification = verify_bundle(dir.path(), &other)?;
        assert_eq!(verification.signature, SignatureOutcome::Rejected);
        assert!(!verification.is_clean());
        // The digests still reproduce: only the origin claim failed.
        assert!(
            verification
                .files
                .iter()
                .all(|file| file.outcome == DigestOutcome::Matches)
        );
        Ok(())
    }

    #[test]
    fn a_signature_over_the_manifest_verifies_directly() -> Result<(), Box<dyn std::error::Error>> {
        let rendered = manifest(&documents())?;
        let armored = sign(rendered.as_bytes(), &secret_key()?, None)?;
        assert!(armored.starts_with("-----BEGIN PGP SIGNATURE-----"));
        let outcome = verify(rendered.as_bytes(), &armored, &public_key()?)?;
        let SignatureOutcome::Accepted(record) = outcome else {
            panic!("the committed keypair must verify its own signature");
        };
        assert!(record.signed_at.as_second() > 0);
        // A different payload under the same signature must be refused.
        assert_eq!(
            verify(b"tampered", &armored, &public_key()?)?,
            SignatureOutcome::Rejected
        );
        Ok(())
    }

    #[test]
    fn a_malformed_public_key_is_an_error_not_a_finding() -> Result<(), Box<dyn std::error::Error>>
    {
        let rendered = manifest(&documents())?;
        let armored = sign(rendered.as_bytes(), &secret_key()?, None)?;
        assert!(matches!(
            verify(rendered.as_bytes(), &armored, "not a key"),
            Err(RecordError::PublicKey { .. })
        ));
        Ok(())
    }
}
