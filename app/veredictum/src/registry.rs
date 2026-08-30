// SPDX-FileCopyrightText: Veredictum contributors
// SPDX-License-Identifier: Apache-2.0

//! The public results registry: one append-only entry per published result.
//!
//! An entry, conformance or benchmark, carries who submitted it, what they
//! disclosed, which artifacts it stands on, and how far anyone here verified
//! it.
//!
//! No openEHR spec governs this — our own design. The shape follows the
//! conformity rungs the architecture already names: a self-reported entry is
//! the supplier's-declaration rung (ISO/IEC 17050), a reproduced entry is the
//! witnessed-verification rung one step up, and neither is a certificate.
//!
//! Two properties make the registry worth reading, and both live in this
//! module rather than in prose:
//!
//! 1. **An entry is evidence plus identity, never a restated number.** Every
//!    figure a board prints comes out of the artifacts the entry points at,
//!    each pinned by digest, so an entry cannot claim a result its evidence
//!    does not carry.
//! 2. **A tier is a statement about verification, and the two tiers are
//!    produced by different machinery.** [`Provenance::SelfReported`] carries
//!    a signature the submitter made; [`Provenance::Reproduced`] carries the
//!    identity of the workflow that performed the run, which is the only thing
//!    this repository can say first-hand. No long-lived signing key exists on
//!    either side.
//!
//! [`entry_defects`] is the pure half of the gate: everything checkable from
//! one entry document alone. The filesystem half — digests recomputed, paths
//! resolved, ids unique across the tree, superseded entries present — belongs
//! to the integration gate, because it reads the committed tree.

use std::collections::BTreeMap;
use std::fmt;

use serde::{Deserialize, Serialize};
use thiserror::Error;

/// The registry entry format's own version, as every entry declares it.
///
/// A submitted entry naming a different version is refused rather than read
/// under this release's field meanings.
pub const REGISTRY_SCHEMA_VERSION: &str = "1.0.0";

/// The version of the published submission rules an entry is accepted under.
///
/// Rules change prospectively: a merged entry is never re-scored, so the
/// version it was accepted under travels with it.
pub const RULES_VERSION: &str = "1.0.0";

/// A failure of the registry machinery.
///
/// Every variant means a value could not be READ. A well-formed entry that
/// falls short of the rules produces [`EntryDefect`]s instead, because those
/// are findings a submitter can act on.
#[derive(Debug, Error, PartialEq, Eq)]
pub enum RegistryError {
    /// A token outside a closed vocabulary.
    #[error("{vocabulary}: unknown token {token:?} (accepted: {accepted})")]
    UnknownToken {
        /// The vocabulary that refused the token.
        vocabulary: &'static str,
        /// The token as it was written.
        token: String,
        /// Every token the vocabulary accepts.
        accepted: String,
    },
    /// An entry id outside the accepted grammar.
    #[error(
        "entry id {id:?} is not `<YYYY-MM-DD>-<slug>` with a lowercase alphanumeric slug: {reason}"
    )]
    EntryId {
        /// The rejected id.
        id: String,
        /// What the grammar wanted.
        reason: &'static str,
    },
    /// A digest outside the accepted grammar.
    #[error("digest {value:?} is not 64 lowercase hexadecimal characters")]
    Digest {
        /// The rejected value.
        value: String,
    },
}

/// The kind of result an entry publishes.
///
/// A closed vocabulary, and the discriminant of [`ResultBlock`]: the two
/// boards are separate surfaces, so a token this set does not carry is a loud
/// error rather than a row rendered onto whichever board happened to match.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum EntryKind {
    /// A catalogue run judged into verdicts.
    Conformance,
    /// A benchmark pack driven for comparative speed.
    Bench,
}

impl EntryKind {
    /// Every kind, in the order the registry directories are laid out.
    pub const ALL: &[EntryKind] = &[EntryKind::Conformance, EntryKind::Bench];

    /// The token an entry names this kind by, which is also its directory.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            EntryKind::Conformance => "conformance",
            EntryKind::Bench => "bench",
        }
    }

    /// Reads one token from the closed vocabulary.
    ///
    /// # Errors
    /// [`RegistryError::UnknownToken`] listing the accepted tokens.
    pub fn parse(token: &str) -> Result<Self, RegistryError> {
        parse_token("registry entry kind", token, Self::ALL, EntryKind::as_str)
    }
}

impl fmt::Display for EntryKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// How far anyone here verified an entry.
///
/// A closed vocabulary. The tier is never written by a submitter: it is the
/// discriminant of [`Provenance`], so claiming [`Tier::Reproduced`] means
/// carrying the workflow identity that performed the run, which only this
/// repository's own lane can produce.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Tier {
    /// This repository's workflow performed the run and attested its output
    /// from the workflow's own OIDC identity.
    Reproduced,
    /// The submitter performed the run and signed its output.
    SelfReported,
}

impl Tier {
    /// Every tier, strongest first.
    pub const ALL: &[Tier] = &[Tier::Reproduced, Tier::SelfReported];

    /// The token an entry names this tier by.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Tier::Reproduced => "reproduced",
            Tier::SelfReported => "self-reported",
        }
    }

    /// Reads one token from the closed vocabulary.
    ///
    /// # Errors
    /// [`RegistryError::UnknownToken`] listing the accepted tokens.
    pub fn parse(token: &str) -> Result<Self, RegistryError> {
        parse_token("registry tier", token, Self::ALL, Tier::as_str)
    }
}

impl fmt::Display for Tier {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// What the submitter is to the system they measured.
///
/// A closed vocabulary, because the reader's first question about any
/// published number is who produced it. `independent` is a claim like any
/// other; the board prints the token and lets the reader weigh it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Relationship {
    /// The submitter builds or sells the system.
    Vendor,
    /// The submitter deploys or integrates the system for others.
    Integrator,
    /// The submitter has no commercial relationship with the system.
    Independent,
    /// A maintainer of this repository.
    Maintainer,
}

impl Relationship {
    /// Every relationship token.
    pub const ALL: &[Relationship] = &[
        Relationship::Vendor,
        Relationship::Integrator,
        Relationship::Independent,
        Relationship::Maintainer,
    ];

    /// The token an entry names this relationship by.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Relationship::Vendor => "vendor",
            Relationship::Integrator => "integrator",
            Relationship::Independent => "independent",
            Relationship::Maintainer => "maintainer",
        }
    }

    /// Reads one token from the closed vocabulary.
    ///
    /// # Errors
    /// [`RegistryError::UnknownToken`] listing the accepted tokens.
    pub fn parse(token: &str) -> Result<Self, RegistryError> {
        parse_token(
            "submitter relationship",
            token,
            Self::ALL,
            Relationship::as_str,
        )
    }
}

impl fmt::Display for Relationship {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// How the measured deployment was obtained.
///
/// A closed vocabulary, and the field the reproduction lane selects on: only
/// [`DeploymentKind::ReproducibleTopology`] names something this repository
/// can stand up from its own committed recipe, so only that kind is eligible
/// for a tier-1 attempt.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum DeploymentKind {
    /// One of the topologies committed under the registry, composed from a
    /// recipe this repository controls.
    ReproducibleTopology,
    /// A container image the submitter composed themselves.
    ContainerImage,
    /// A running service the submitter operates.
    HostedEndpoint,
    /// A build the submitter made from source.
    LocalBuild,
}

impl DeploymentKind {
    /// Every deployment kind.
    pub const ALL: &[DeploymentKind] = &[
        DeploymentKind::ReproducibleTopology,
        DeploymentKind::ContainerImage,
        DeploymentKind::HostedEndpoint,
        DeploymentKind::LocalBuild,
    ];

    /// The token an entry names this deployment kind by.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            DeploymentKind::ReproducibleTopology => "reproducible-topology",
            DeploymentKind::ContainerImage => "container-image",
            DeploymentKind::HostedEndpoint => "hosted-endpoint",
            DeploymentKind::LocalBuild => "local-build",
        }
    }

    /// Reads one token from the closed vocabulary.
    ///
    /// # Errors
    /// [`RegistryError::UnknownToken`] listing the accepted tokens.
    pub fn parse(token: &str) -> Result<Self, RegistryError> {
        parse_token("deployment kind", token, Self::ALL, DeploymentKind::as_str)
    }
}

impl fmt::Display for DeploymentKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// What one committed artifact is to the entry that points at it.
///
/// A closed vocabulary, because the gate's completeness rule is stated per
/// kind in roles: a conformance entry without a `verdicts` artifact publishes
/// a verdict nobody can recompute.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ArtifactRole {
    /// The run's `results.json`.
    Results,
    /// The judgement's `verdicts.json`.
    Verdicts,
    /// The recorded wire exchanges.
    Transcript,
    /// A bench record under the benchmark submissions tree.
    BenchResult,
    /// A sealed bundle's digest manifest.
    RecordManifest,
    /// The detached signature over a document in this entry.
    Signature,
    /// A rendered report or statement document.
    Report,
    /// The ixit declaration the run was driven under, which is what the
    /// results' `ixit_digest` is taken over and the only thing that explains
    /// which principals the deployment had.
    Ixit,
}

impl ArtifactRole {
    /// Every artifact role.
    pub const ALL: &[ArtifactRole] = &[
        ArtifactRole::Results,
        ArtifactRole::Verdicts,
        ArtifactRole::Transcript,
        ArtifactRole::BenchResult,
        ArtifactRole::RecordManifest,
        ArtifactRole::Signature,
        ArtifactRole::Report,
        ArtifactRole::Ixit,
    ];

    /// The token an entry names this role by.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            ArtifactRole::Results => "results",
            ArtifactRole::Verdicts => "verdicts",
            ArtifactRole::Transcript => "transcript",
            ArtifactRole::BenchResult => "bench-result",
            ArtifactRole::RecordManifest => "record-manifest",
            ArtifactRole::Signature => "signature",
            ArtifactRole::Report => "report",
            ArtifactRole::Ixit => "ixit",
        }
    }

    /// The roles an entry of this kind must carry.
    #[must_use]
    pub const fn required_for(kind: EntryKind) -> &'static [ArtifactRole] {
        match kind {
            EntryKind::Conformance => &[ArtifactRole::Results, ArtifactRole::Verdicts],
            EntryKind::Bench => &[ArtifactRole::BenchResult],
        }
    }

    /// Reads one token from the closed vocabulary.
    ///
    /// # Errors
    /// [`RegistryError::UnknownToken`] listing the accepted tokens.
    pub fn parse(token: &str) -> Result<Self, RegistryError> {
        parse_token("artifact role", token, Self::ALL, ArtifactRole::as_str)
    }
}

impl fmt::Display for ArtifactRole {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// The signature scheme a self-reported entry was signed with.
///
/// A closed vocabulary. Both schemes verify with a published command and
/// neither involves a key this repository holds.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum SignatureScheme {
    /// An armored RFC 9580 detached OpenPGP signature, verified with
    /// `gpg --verify`.
    OpenpgpDetached,
    /// A Sigstore bundle carrying the submitter's own OIDC identity, verified
    /// with `cosign verify-blob`.
    SigstoreBundle,
}

impl SignatureScheme {
    /// Every signature scheme.
    pub const ALL: &[SignatureScheme] = &[
        SignatureScheme::OpenpgpDetached,
        SignatureScheme::SigstoreBundle,
    ];

    /// The token an entry names this scheme by.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            SignatureScheme::OpenpgpDetached => "openpgp-detached",
            SignatureScheme::SigstoreBundle => "sigstore-bundle",
        }
    }

    /// Reads one token from the closed vocabulary.
    ///
    /// # Errors
    /// [`RegistryError::UnknownToken`] listing the accepted tokens.
    pub fn parse(token: &str) -> Result<Self, RegistryError> {
        parse_token(
            "signature scheme",
            token,
            Self::ALL,
            SignatureScheme::as_str,
        )
    }
}

impl fmt::Display for SignatureScheme {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Reads one token from a closed vocabulary, or names every accepted token.
fn parse_token<T: Copy>(
    vocabulary: &'static str,
    token: &str,
    all: &'static [T],
    render: fn(T) -> &'static str,
) -> Result<T, RegistryError> {
    all.iter()
        .copied()
        .find(|candidate| render(*candidate) == token)
        .ok_or_else(|| RegistryError::UnknownToken {
            vocabulary,
            token: token.to_owned(),
            accepted: all
                .iter()
                .copied()
                .map(render)
                .collect::<Vec<_>>()
                .join(", "),
        })
}

/// How many characters of an entry id are its calendar date.
const DATE_LEN: usize = "YYYY-MM-DD".len();

/// A registry entry's identifier, which is also its file stem.
///
/// The grammar is `<YYYY-MM-DD>-<slug>`: the date the run started, so the
/// registry sorts chronologically on disk, and a lowercase slug the submitter
/// chooses. Ids are unique across the whole registry, which is what makes
/// supersede-by-reference resolvable.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct EntryId(String);

impl EntryId {
    /// Reads one id, or says which part of the grammar it missed.
    ///
    /// # Errors
    /// [`RegistryError::EntryId`] naming the rule the id broke.
    pub fn parse(id: &str) -> Result<Self, RegistryError> {
        let refuse = |reason: &'static str| RegistryError::EntryId {
            id: id.to_owned(),
            reason,
        };
        let (date, slug) = id
            .split_at_checked(DATE_LEN)
            .ok_or_else(|| refuse("it is shorter than a calendar date"))?;
        if !is_calendar_date(date) {
            return Err(refuse("it does not open with a YYYY-MM-DD date"));
        }
        let slug = slug
            .strip_prefix('-')
            .ok_or_else(|| refuse("no `-` separates the date from the slug"))?;
        if slug.is_empty() {
            return Err(refuse("the slug is empty"));
        }
        if !slug
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
        {
            return Err(refuse("the slug carries something other than [a-z0-9-]"));
        }
        if slug.starts_with('-') || slug.ends_with('-') {
            return Err(refuse("the slug opens or closes on a `-`"));
        }
        Ok(Self(id.to_owned()))
    }

    /// The id as written.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// The calendar date the id opens with.
    #[must_use]
    pub fn date(&self) -> &str {
        self.0.get(..DATE_LEN).unwrap_or_default()
    }
}

impl TryFrom<String> for EntryId {
    type Error = RegistryError;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        Self::parse(&value)
    }
}

impl From<EntryId> for String {
    fn from(value: EntryId) -> Self {
        value.0
    }
}

impl fmt::Display for EntryId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

/// Whether a string is a `YYYY-MM-DD` calendar date by shape.
fn is_calendar_date(text: &str) -> bool {
    text.len() == DATE_LEN
        && text.chars().enumerate().all(|(position, c)| {
            if position == 4 || position == 7 {
                c == '-'
            } else {
                c.is_ascii_digit()
            }
        })
}

/// How many characters a SHA-256 digest renders as.
const DIGEST_LEN: usize = 64;

/// A SHA-256 digest over one artifact's exact bytes, lowercase hex.
///
/// Every artifact an entry points at is pinned this way, so the entry cannot
/// come to describe a file somebody later replaced.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct Digest(String);

impl Digest {
    /// Reads one lowercase-hex SHA-256 digest.
    ///
    /// # Errors
    /// [`RegistryError::Digest`] when the value is not 64 lowercase
    /// hexadecimal characters.
    pub fn parse(value: &str) -> Result<Self, RegistryError> {
        if value.len() == DIGEST_LEN
            && value
                .chars()
                .all(|c| c.is_ascii_digit() || ('a'..='f').contains(&c))
        {
            Ok(Self(value.to_owned()))
        } else {
            Err(RegistryError::Digest {
                value: value.to_owned(),
            })
        }
    }

    /// The digest as written.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl TryFrom<String> for Digest {
    type Error = RegistryError;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        Self::parse(&value)
    }
}

impl From<Digest> for String {
    fn from(value: Digest) -> Self {
        value.0
    }
}

impl fmt::Display for Digest {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

/// Who submitted an entry.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Submitter {
    /// The person or organization publishing the entry.
    pub name: String,
    /// A URL or `mailto:` address the entry can be discussed at.
    pub contact: String,
    /// What the submitter is to the system they measured.
    pub relationship: Relationship,
}

/// The deployment an entry measured.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Deployment {
    /// How the deployment was obtained.
    pub kind: DeploymentKind,
    /// The committed topology this deployment is, when the kind names one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub topology: Option<String>,
    /// The digest-pinned images the deployment ran, keyed by role.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub images: BTreeMap<String, String>,
    /// The base URL a hosted endpoint was driven over.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub endpoint: Option<String>,
    /// Whether the submitter authorizes this repository to drive the
    /// deployment for a reproduction run.
    ///
    /// The submission pull request IS that authorization, and the field is
    /// where the submitter records it in the entry itself.
    pub reproduction_authorized: bool,
}

/// The system an entry is about.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Subject {
    /// The lowercase system id, which is also the entry's directory.
    pub system: String,
    /// The name a board prints.
    pub display_name: String,
    /// The version of the system that was measured.
    pub version: String,
    /// How that version was deployed.
    pub deployment: Deployment,
}

/// The machine an entry's run was taken on.
///
/// Mandatory on every entry. A conformance verdict is far less sensitive to
/// the host than a latency is, and it is still the first thing a reader asks
/// about a published result.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EnvironmentDisclosure {
    /// The operating system the load generator ran on.
    pub os: String,
    /// Its architecture.
    pub arch: String,
    /// How the submitter describes the host.
    pub host_class: String,
    /// The CPU model, when the platform discloses one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cpu_model: Option<String>,
    /// Cores available to the run, when the platform discloses them.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cores: Option<u32>,
    /// Memory available to the run in bytes, when the platform discloses it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub memory_bytes: Option<u64>,
}

/// The disclosure every entry carries, whatever it measured.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Disclosure {
    /// The instrument version that produced the artifacts.
    pub instrument_version: String,
    /// When the run started, RFC 3339 in UTC.
    pub run_started_at: String,
    /// The machine the run was taken on.
    pub environment: EnvironmentDisclosure,
    /// What was switched on behind the result: authentication, validation
    /// depth, signing, audit, tenancy, anything a reader needs to make sense
    /// of it.
    pub sut_configuration: String,
    /// Any interest the submitter holds in the outcome, stated in words.
    /// `none` is itself a statement, and an empty value is refused.
    pub conflict_of_interest: String,
}

/// What the entry measured, and the pins that make it comparable.
///
/// Internally tagged by [`EntryKind`], so the discriminant a board filters on
/// and the block it reads are one field. Unknown members are refused by the
/// published JSON Schema, which the gate applies before this type parses.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "kebab-case")]
pub enum ResultBlock {
    /// A catalogue run judged into verdicts.
    Conformance {
        /// The revision of the artifact tree the run executed.
        catalogue_revision: String,
        /// The party statement the claim was judged against.
        statement: String,
    },
    /// A benchmark pack driven for comparative speed.
    Bench {
        /// The pack id.
        pack_id: String,
        /// The pack version.
        pack_version: String,
        /// How many measured repetitions the record carries.
        repetitions: u32,
        /// The posture profile the run declared.
        posture_profile: String,
    },
}

impl ResultBlock {
    /// Which board this block belongs on.
    #[must_use]
    pub const fn kind(&self) -> EntryKind {
        match self {
            ResultBlock::Conformance { .. } => EntryKind::Conformance,
            ResultBlock::Bench { .. } => EntryKind::Bench,
        }
    }
}

/// One committed artifact an entry stands on.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ArtifactRef {
    /// What the artifact is to this entry.
    pub role: ArtifactRole,
    /// Its repository-relative path.
    pub path: String,
    /// SHA-256 over its exact bytes.
    pub sha256: Digest,
}

/// How far anyone here verified the entry, and what proves it.
///
/// Internally tagged by [`Tier`]: the tier a board prints and the evidence
/// behind it are one field, so a tier cannot be claimed without the evidence
/// its variant requires.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "tier", rename_all = "kebab-case")]
pub enum Provenance {
    /// This repository's workflow performed the run.
    ///
    /// The workflow's OIDC identity IS the signature: the artifacts carry a
    /// Sigstore-backed build attestation issued to `workflow_ref`, and no key
    /// is stored anywhere for anyone to steal.
    Reproduced {
        /// The workflow that performed the run, as the OIDC `workflow_ref`
        /// claim spells it.
        workflow_ref: String,
        /// The workflow run that produced the artifacts.
        run_id: String,
        /// Which attempt of that run.
        run_attempt: u32,
        /// The attestation predicate type the run issued.
        predicate_type: String,
        /// The command anybody can re-run to check the attestation.
        verify_command: String,
    },
    /// The submitter performed the run and signed its output.
    SelfReported {
        /// The scheme the signature was made with.
        scheme: SignatureScheme,
        /// The repository-relative path of the signature.
        signature: String,
        /// The repository-relative path of the artifact it covers.
        signs: String,
        /// The identity the signature is checked against: a key fingerprint,
        /// or the OIDC identity a Sigstore bundle carries.
        identity: String,
        /// The command anybody can re-run to check the signature.
        verify_command: String,
    },
}

impl Provenance {
    /// The tier this provenance establishes.
    #[must_use]
    pub const fn tier(&self) -> Tier {
        match self {
            Provenance::Reproduced { .. } => Tier::Reproduced,
            Provenance::SelfReported { .. } => Tier::SelfReported,
        }
    }
}

/// One published registry entry.
///
/// Field order here IS the JSON key order of a hand-authored entry, and every
/// collection renders in the order it was authored, so an entry round-trips
/// through this type without moving a byte a reviewer read.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RegistryEntry {
    /// The entry format version this document is written against.
    pub registry_schema_version: String,
    /// The entry's own identifier, which is also its file stem.
    pub entry_id: EntryId,
    /// The submission rules version the entry was accepted under.
    pub rules_version: String,
    /// Who submitted it.
    pub submitter: Submitter,
    /// The system it is about.
    pub subject: Subject,
    /// The mandatory disclosure.
    pub disclosure: Disclosure,
    /// What was measured.
    pub result: ResultBlock,
    /// The committed artifacts it stands on.
    pub artifacts: Vec<ArtifactRef>,
    /// How far anyone here verified it.
    pub provenance: Provenance,
    /// Entries this one replaces, by id.
    ///
    /// Supersede-by-reference points FORWARD only: a merged entry is never
    /// edited, so the replaced entry stays exactly as it was published and the
    /// board derives the backward edge.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub supersedes: Vec<EntryId>,
    /// Why this entry supersedes the ones it names. Required whenever
    /// [`Self::supersedes`] is non-empty.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub supersede_reason: Option<String>,
    /// Anything else the submitter wants a reader to know.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub notes: Option<String>,
}

impl RegistryEntry {
    /// Which board this entry belongs on.
    #[must_use]
    pub const fn kind(&self) -> EntryKind {
        self.result.kind()
    }

    /// How far anyone here verified it.
    #[must_use]
    pub const fn tier(&self) -> Tier {
        self.provenance.tier()
    }

    /// The repository-relative path this entry must be committed at.
    #[must_use]
    pub fn expected_path(&self) -> String {
        format!(
            "registry/entries/{}/{}/{}.json",
            self.kind(),
            self.subject.system,
            self.entry_id
        )
    }

    /// The artifact carrying one role, when the entry declares exactly one.
    #[must_use]
    pub fn artifact(&self, role: ArtifactRole) -> Option<&ArtifactRef> {
        let mut matching = self.artifacts.iter().filter(|a| a.role == role);
        let first = matching.next()?;
        matching.next().is_none().then_some(first)
    }
}

/// Why one entry falls short of the published submission rules.
///
/// A typed vocabulary rather than a message: the gate branches on the kind to
/// decide what to print, and a submitter reads the same sentence CI did.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EntryDefect {
    /// The document declares a registry format this release does not read.
    SchemaVersion {
        /// What the entry declared.
        declared: String,
    },
    /// The document declares rules this release does not publish.
    RulesVersion {
        /// What the entry declared.
        declared: String,
    },
    /// A mandatory disclosure field is empty.
    EmptyField {
        /// The field, by its JSON path.
        field: &'static str,
    },
    /// The run timestamp is not RFC 3339 in UTC.
    Timestamp {
        /// The value as written.
        value: String,
    },
    /// The entry id's date and the run's date disagree.
    DateMismatch {
        /// The date the id carries.
        id_date: String,
        /// The date the run started on.
        run_date: String,
    },
    /// An artifact role the entry's kind requires is absent.
    MissingArtifact {
        /// The role that is missing.
        role: ArtifactRole,
        /// The kind that requires it.
        kind: EntryKind,
    },
    /// Two artifacts claim the same path.
    DuplicateArtifact {
        /// The repeated path.
        path: String,
    },
    /// An artifact path is not a plain repository-relative path.
    UnsafeArtifactPath {
        /// The rejected path.
        path: String,
    },
    /// A bench entry points its record somewhere other than the benchmark
    /// submissions tree the board renders from.
    MisplacedBenchRecord {
        /// The path as written.
        path: String,
    },
    /// A conformance entry points an artifact outside its own record
    /// directory.
    MisplacedRecord {
        /// The path as written.
        path: String,
        /// Where an artifact of this entry belongs.
        expected_prefix: String,
    },
    /// A self-reported entry signs something it does not carry.
    UnsignedArtifact {
        /// The path the signature claims to cover.
        path: String,
    },
    /// A self-reported entry's signature file is not among its artifacts.
    UndeclaredSignature {
        /// The path the provenance names.
        path: String,
    },
    /// A reproduced entry names a workflow outside this repository.
    ForeignWorkflow {
        /// The `workflow_ref` as written.
        workflow_ref: String,
    },
    /// A reproduced entry describes a deployment the reproduction lane cannot
    /// stand up.
    UnreproducibleDeployment {
        /// The deployment kind the entry declared.
        kind: DeploymentKind,
    },
    /// A deployment names no topology although its kind is one.
    MissingTopology,
    /// An entry supersedes itself.
    SelfSupersede,
    /// An entry names the same superseded id twice.
    DuplicateSupersede {
        /// The repeated id.
        id: EntryId,
    },
    /// An entry supersedes something without saying why.
    UnexplainedSupersede,
}

impl fmt::Display for EntryDefect {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            EntryDefect::SchemaVersion { declared } => write!(
                f,
                "the entry declares registry format {declared:?}, and this release reads \
                 {REGISTRY_SCHEMA_VERSION}"
            ),
            EntryDefect::RulesVersion { declared } => write!(
                f,
                "the entry declares rules version {declared:?}, and this release publishes \
                 {RULES_VERSION}"
            ),
            EntryDefect::EmptyField { field } => {
                write!(f, "{field} is empty, and the disclosure is mandatory")
            }
            EntryDefect::Timestamp { value } => write!(
                f,
                "run_started_at {value:?} is not an RFC 3339 timestamp in UTC"
            ),
            EntryDefect::DateMismatch { id_date, run_date } => write!(
                f,
                "the entry id opens on {id_date} and the run started on {run_date}"
            ),
            EntryDefect::MissingArtifact { role, kind } => write!(
                f,
                "a {kind} entry carries exactly one `{role}` artifact, and this one carries none \
                 or several"
            ),
            EntryDefect::DuplicateArtifact { path } => {
                write!(f, "{path} is declared as an artifact twice")
            }
            EntryDefect::UnsafeArtifactPath { path } => write!(
                f,
                "{path:?} is not a plain repository-relative path (no leading `/`, no `..`, no \
                 backslash)"
            ),
            EntryDefect::MisplacedBenchRecord { path } => write!(
                f,
                "{path} is outside benchmarks/submissions/, which is the tree the benchmark board \
                 renders from"
            ),
            EntryDefect::MisplacedRecord {
                path,
                expected_prefix,
            } => write!(f, "{path} is outside {expected_prefix}"),
            EntryDefect::UnsignedArtifact { path } => write!(
                f,
                "the signature covers {path}, which the entry does not carry as an artifact"
            ),
            EntryDefect::UndeclaredSignature { path } => write!(
                f,
                "the signature {path} is not declared as a `signature` artifact, so nothing pins \
                 its bytes"
            ),
            EntryDefect::ForeignWorkflow { workflow_ref } => write!(
                f,
                "the reproduced tier is issued by this repository's own workflow, and \
                 {workflow_ref:?} is not one"
            ),
            EntryDefect::UnreproducibleDeployment { kind } => write!(
                f,
                "the reproduced tier requires a deployment this repository composes itself, and \
                 this entry declares {kind}"
            ),
            EntryDefect::MissingTopology => f.write_str(
                "the deployment is a reproducible topology and names none, so no recipe stands \
                 behind it",
            ),
            EntryDefect::SelfSupersede => f.write_str("the entry supersedes itself"),
            EntryDefect::DuplicateSupersede { id } => {
                write!(f, "{id} is superseded twice by one entry")
            }
            EntryDefect::UnexplainedSupersede => f.write_str(
                "an entry that supersedes another states why, because the superseded one stays \
                 published",
            ),
        }
    }
}

/// The repository whose workflows may issue the reproduced tier.
const OWN_WORKFLOW_PREFIX: &str = "rubentalstra/Veredictum/.github/workflows/";

/// The tree the benchmark board renders its numbers from.
const BENCH_SUBMISSIONS: &str = "benchmarks/submissions/";

/// Every way one entry document falls short of the published rules.
///
/// Pure: this reads the entry and nothing else, so the same function runs in
/// the gate, in a submitter's local check, and in the unit tests below. The
/// checks that need the committed tree — digests recomputed, paths resolved,
/// ids unique, superseded entries present — belong to the integration gate.
///
/// An empty result means the document is publishable as far as one document
/// can say.
#[must_use]
pub fn entry_defects(entry: &RegistryEntry) -> Vec<EntryDefect> {
    let mut defects = Vec::new();
    if entry.registry_schema_version != REGISTRY_SCHEMA_VERSION {
        defects.push(EntryDefect::SchemaVersion {
            declared: entry.registry_schema_version.clone(),
        });
    }
    if entry.rules_version != RULES_VERSION {
        defects.push(EntryDefect::RulesVersion {
            declared: entry.rules_version.clone(),
        });
    }
    defects.extend(empty_field_defects(entry));
    defects.extend(timestamp_defects(entry));
    defects.extend(artifact_defects(entry));
    defects.extend(provenance_defects(entry));
    defects.extend(supersede_defects(entry));
    defects
}

/// Every mandatory disclosure field left blank.
fn empty_field_defects(entry: &RegistryEntry) -> Vec<EntryDefect> {
    let mandatory: [(&'static str, &str); 9] = [
        ("submitter.name", &entry.submitter.name),
        ("submitter.contact", &entry.submitter.contact),
        ("subject.system", &entry.subject.system),
        ("subject.display_name", &entry.subject.display_name),
        ("subject.version", &entry.subject.version),
        (
            "disclosure.instrument_version",
            &entry.disclosure.instrument_version,
        ),
        (
            "disclosure.environment.host_class",
            &entry.disclosure.environment.host_class,
        ),
        (
            "disclosure.sut_configuration",
            &entry.disclosure.sut_configuration,
        ),
        (
            "disclosure.conflict_of_interest",
            &entry.disclosure.conflict_of_interest,
        ),
    ];
    mandatory
        .into_iter()
        .filter(|(_, value)| value.trim().is_empty())
        .map(|(field, _)| EntryDefect::EmptyField { field })
        .collect()
}

/// The run timestamp, and its agreement with the id it is filed under.
fn timestamp_defects(entry: &RegistryEntry) -> Vec<EntryDefect> {
    let stamp = &entry.disclosure.run_started_at;
    let Some((date, _)) = stamp.split_once('T') else {
        return vec![EntryDefect::Timestamp {
            value: stamp.clone(),
        }];
    };
    if !stamp.ends_with('Z') || stamp.parse::<jiff::Timestamp>().is_err() {
        return vec![EntryDefect::Timestamp {
            value: stamp.clone(),
        }];
    }
    if date == entry.entry_id.date() {
        Vec::new()
    } else {
        vec![EntryDefect::DateMismatch {
            id_date: entry.entry_id.date().to_owned(),
            run_date: date.to_owned(),
        }]
    }
}

/// The artifact list: completeness per kind, uniqueness, and where each file
/// is allowed to live.
fn artifact_defects(entry: &RegistryEntry) -> Vec<EntryDefect> {
    let kind = entry.kind();
    let mut defects: Vec<EntryDefect> = ArtifactRole::required_for(kind)
        .iter()
        .copied()
        .filter(|role| entry.artifact(*role).is_none())
        .map(|role| EntryDefect::MissingArtifact { role, kind })
        .collect();

    let mut seen: Vec<&str> = Vec::new();
    let record_prefix = format!(
        "registry/records/{}/{}/",
        entry.subject.system, entry.entry_id
    );
    for artifact in &entry.artifacts {
        let path = artifact.path.as_str();
        if seen.contains(&path) {
            defects.push(EntryDefect::DuplicateArtifact {
                path: path.to_owned(),
            });
        } else {
            seen.push(path);
        }
        if !is_plain_relative_path(path) {
            defects.push(EntryDefect::UnsafeArtifactPath {
                path: path.to_owned(),
            });
            continue;
        }
        if artifact.role == ArtifactRole::BenchResult {
            if !path.starts_with(BENCH_SUBMISSIONS) {
                defects.push(EntryDefect::MisplacedBenchRecord {
                    path: path.to_owned(),
                });
            }
        } else if !path.starts_with(&record_prefix) {
            defects.push(EntryDefect::MisplacedRecord {
                path: path.to_owned(),
                expected_prefix: record_prefix.clone(),
            });
        }
    }
    defects
}

/// Whether a path is a plain repository-relative path.
fn is_plain_relative_path(path: &str) -> bool {
    !path.is_empty()
        && !path.starts_with('/')
        && !path.contains('\\')
        && !path.split('/').any(|segment| {
            segment.is_empty() || segment == "." || segment == ".." || segment.starts_with(' ')
        })
}

/// The tier and the evidence it requires.
fn provenance_defects(entry: &RegistryEntry) -> Vec<EntryDefect> {
    let mut defects = Vec::new();
    match &entry.provenance {
        Provenance::Reproduced { workflow_ref, .. } => {
            if !workflow_ref.starts_with(OWN_WORKFLOW_PREFIX) {
                defects.push(EntryDefect::ForeignWorkflow {
                    workflow_ref: workflow_ref.clone(),
                });
            }
            if entry.subject.deployment.kind != DeploymentKind::ReproducibleTopology {
                defects.push(EntryDefect::UnreproducibleDeployment {
                    kind: entry.subject.deployment.kind,
                });
            }
        }
        Provenance::SelfReported {
            signature, signs, ..
        } => {
            if !entry
                .artifacts
                .iter()
                .any(|artifact| artifact.path == *signs)
            {
                defects.push(EntryDefect::UnsignedArtifact {
                    path: signs.clone(),
                });
            }
            if !entry.artifacts.iter().any(|artifact| {
                artifact.role == ArtifactRole::Signature && artifact.path == *signature
            }) {
                defects.push(EntryDefect::UndeclaredSignature {
                    path: signature.clone(),
                });
            }
        }
    }
    if entry.subject.deployment.kind == DeploymentKind::ReproducibleTopology
        && entry.subject.deployment.topology.is_none()
    {
        defects.push(EntryDefect::MissingTopology);
    }
    defects
}

/// The supersede edges an entry declares.
fn supersede_defects(entry: &RegistryEntry) -> Vec<EntryDefect> {
    let mut defects = Vec::new();
    let mut seen: Vec<&EntryId> = Vec::new();
    for superseded in &entry.supersedes {
        if *superseded == entry.entry_id {
            defects.push(EntryDefect::SelfSupersede);
        }
        if seen.contains(&superseded) {
            defects.push(EntryDefect::DuplicateSupersede {
                id: superseded.clone(),
            });
        } else {
            seen.push(superseded);
        }
    }
    if !entry.supersedes.is_empty()
        && entry
            .supersede_reason
            .as_ref()
            .is_none_or(|reason| reason.trim().is_empty())
    {
        defects.push(EntryDefect::UnexplainedSupersede);
    }
    defects
}

#[cfg(test)]
#[expect(
    clippy::panic_in_result_fn,
    reason = "the Book's Result-test shape: assertions panic, plumbing propagates with `?`"
)]
mod tests {
    use super::*;

    /// An entry shaped exactly as a passing self-reported bench submission,
    /// so every fixture below states the one thing it changed.
    fn bench_entry() -> RegistryEntry {
        RegistryEntry {
            registry_schema_version: REGISTRY_SCHEMA_VERSION.to_owned(),
            entry_id: EntryId(String::from("2026-01-02-example-cdr")),
            rules_version: RULES_VERSION.to_owned(),
            submitter: Submitter {
                name: String::from("Example Health"),
                contact: String::from("https://github.com/example"),
                relationship: Relationship::Vendor,
            },
            subject: Subject {
                system: String::from("example"),
                display_name: String::from("Example CDR"),
                version: String::from("1.2.3"),
                deployment: Deployment {
                    kind: DeploymentKind::ContainerImage,
                    topology: None,
                    images: BTreeMap::new(),
                    endpoint: None,
                    reproduction_authorized: false,
                },
            },
            disclosure: Disclosure {
                instrument_version: String::from("0.1.1"),
                run_started_at: String::from("2026-01-02T03:04:05Z"),
                environment: EnvironmentDisclosure {
                    os: String::from("linux"),
                    arch: String::from("x86_64"),
                    host_class: String::from("bare metal, 8 cores"),
                    cpu_model: None,
                    cores: Some(8),
                    memory_bytes: None,
                },
                sut_configuration: String::from("basic auth, template validation, no audit"),
                conflict_of_interest: String::from("the submitter builds the system"),
            },
            result: ResultBlock::Bench {
                pack_id: String::from("community-vitals"),
                pack_version: String::from("1.0.0"),
                repetitions: 3,
                posture_profile: String::from("minimal"),
            },
            artifacts: vec![
                ArtifactRef {
                    role: ArtifactRole::BenchResult,
                    path: String::from("benchmarks/submissions/example/2026-01-02-aaaaaaaa.json"),
                    sha256: Digest("a".repeat(DIGEST_LEN)),
                },
                ArtifactRef {
                    role: ArtifactRole::Signature,
                    path: String::from(
                        "registry/records/example/2026-01-02-example-cdr/bench-result.json.asc",
                    ),
                    sha256: Digest("b".repeat(DIGEST_LEN)),
                },
            ],
            provenance: Provenance::SelfReported {
                scheme: SignatureScheme::OpenpgpDetached,
                signature: String::from(
                    "registry/records/example/2026-01-02-example-cdr/bench-result.json.asc",
                ),
                signs: String::from("benchmarks/submissions/example/2026-01-02-aaaaaaaa.json"),
                identity: String::from("0123456789ABCDEF"),
                verify_command: String::from("gpg --verify bench-result.json.asc"),
            },
            supersedes: Vec::new(),
            supersede_reason: None,
            notes: None,
        }
    }

    #[test]
    fn the_publishable_fixture_carries_no_defect() {
        assert_eq!(entry_defects(&bench_entry()), Vec::new());
    }

    #[test]
    fn an_entry_id_states_a_date_then_a_lowercase_slug() {
        assert!(EntryId::parse("2026-01-02-example-cdr").is_ok());
        assert!(EntryId::parse("2026-01-02").is_err());
        assert!(EntryId::parse("2026-01-02-Example").is_err());
        assert!(EntryId::parse("20260102-example").is_err());
        assert!(EntryId::parse("2026-01-02-").is_err());
        assert!(EntryId::parse("2026-01-02-a-").is_err());
    }

    #[test]
    fn a_digest_is_sixty_four_lowercase_hex_characters() {
        assert!(Digest::parse(&"a".repeat(DIGEST_LEN)).is_ok());
        assert!(Digest::parse(&"A".repeat(DIGEST_LEN)).is_err());
        assert!(Digest::parse(&"a".repeat(DIGEST_LEN - 1)).is_err());
        assert!(Digest::parse(&"g".repeat(DIGEST_LEN)).is_err());
    }

    #[test]
    fn every_closed_vocabulary_refuses_an_unknown_token() {
        assert!(EntryKind::parse("perf").is_err());
        assert!(Tier::parse("verified").is_err());
        assert!(Relationship::parse("partner").is_err());
        assert!(DeploymentKind::parse("kubernetes").is_err());
        assert!(ArtifactRole::parse("summary").is_err());
        assert!(SignatureScheme::parse("ssh").is_err());
    }

    #[test]
    fn an_empty_disclosure_field_is_named() {
        let mut entry = bench_entry();
        entry.disclosure.conflict_of_interest = String::from("   ");
        assert_eq!(
            entry_defects(&entry),
            vec![EntryDefect::EmptyField {
                field: "disclosure.conflict_of_interest"
            }]
        );
    }

    #[test]
    fn a_run_date_that_disagrees_with_the_id_is_refused() {
        let mut entry = bench_entry();
        entry.disclosure.run_started_at = String::from("2026-01-03T00:00:00Z");
        assert_eq!(
            entry_defects(&entry),
            vec![EntryDefect::DateMismatch {
                id_date: String::from("2026-01-02"),
                run_date: String::from("2026-01-03"),
            }]
        );
    }

    #[test]
    fn a_timestamp_outside_utc_is_refused() {
        let mut entry = bench_entry();
        entry.disclosure.run_started_at = String::from("2026-01-02T03:04:05+02:00");
        assert_eq!(
            entry_defects(&entry),
            vec![EntryDefect::Timestamp {
                value: String::from("2026-01-02T03:04:05+02:00")
            }]
        );
    }

    #[test]
    fn a_conformance_entry_without_verdicts_is_refused() {
        let mut entry = bench_entry();
        entry.result = ResultBlock::Conformance {
            catalogue_revision: String::from("4cee001c"),
            statement: String::from("party/example/statement.json"),
        };
        let defects = entry_defects(&entry);
        assert!(
            defects.contains(&EntryDefect::MissingArtifact {
                role: ArtifactRole::Verdicts,
                kind: EntryKind::Conformance,
            }),
            "{defects:?}"
        );
        assert!(
            defects.contains(&EntryDefect::MissingArtifact {
                role: ArtifactRole::Results,
                kind: EntryKind::Conformance,
            }),
            "{defects:?}"
        );
    }

    #[test]
    fn an_artifact_path_that_escapes_the_repository_is_refused() {
        let mut entry = bench_entry();
        if let Some(artifact) = entry.artifacts.first_mut() {
            artifact.path = String::from("../../etc/passwd");
        }
        let defects = entry_defects(&entry);
        assert!(
            defects.contains(&EntryDefect::UnsafeArtifactPath {
                path: String::from("../../etc/passwd")
            }),
            "{defects:?}"
        );
    }

    #[test]
    fn a_bench_record_outside_the_submissions_tree_is_refused() {
        let mut entry = bench_entry();
        let moved = String::from("registry/records/example/2026-01-02-example-cdr/bench.json");
        if let Some(artifact) = entry.artifacts.first_mut() {
            artifact.path = moved.clone();
        }
        let defects = entry_defects(&entry);
        assert!(
            defects.contains(&EntryDefect::MisplacedBenchRecord { path: moved }),
            "{defects:?}"
        );
    }

    #[test]
    fn a_signature_over_something_the_entry_does_not_carry_is_refused() {
        let mut entry = bench_entry();
        entry.provenance = Provenance::SelfReported {
            scheme: SignatureScheme::OpenpgpDetached,
            signature: String::from(
                "registry/records/example/2026-01-02-example-cdr/bench-result.json.asc",
            ),
            signs: String::from("benchmarks/submissions/example/somebody-elses.json"),
            identity: String::from("0123456789ABCDEF"),
            verify_command: String::from("gpg --verify bench-result.json.asc"),
        };
        let defects = entry_defects(&entry);
        assert!(
            defects.contains(&EntryDefect::UnsignedArtifact {
                path: String::from("benchmarks/submissions/example/somebody-elses.json")
            }),
            "{defects:?}"
        );
    }

    /// A submitter cannot promote their own entry: the reproduced tier names
    /// a workflow of this repository, and a deployment this repository knows
    /// how to compose.
    #[test]
    fn a_self_declared_reproduced_tier_is_refused() {
        let mut entry = bench_entry();
        entry.provenance = Provenance::Reproduced {
            workflow_ref: String::from("example/ci/.github/workflows/bench.yml@refs/heads/main"),
            run_id: String::from("1"),
            run_attempt: 1,
            predicate_type: String::from("https://slsa.dev/provenance/v1"),
            verify_command: String::from("gh attestation verify"),
        };
        let defects = entry_defects(&entry);
        assert!(
            defects.contains(&EntryDefect::ForeignWorkflow {
                workflow_ref: String::from(
                    "example/ci/.github/workflows/bench.yml@refs/heads/main"
                )
            }),
            "{defects:?}"
        );
        assert!(
            defects.contains(&EntryDefect::UnreproducibleDeployment {
                kind: DeploymentKind::ContainerImage
            }),
            "{defects:?}"
        );
    }

    #[test]
    fn a_supersede_without_a_reason_is_refused() {
        let mut entry = bench_entry();
        entry.supersedes = vec![EntryId(String::from("2025-12-01-example-cdr"))];
        assert_eq!(
            entry_defects(&entry),
            vec![EntryDefect::UnexplainedSupersede]
        );
    }

    #[test]
    fn an_entry_that_supersedes_itself_is_refused() {
        let mut entry = bench_entry();
        entry.supersedes = vec![entry.entry_id.clone()];
        entry.supersede_reason = Some(String::from("a correction"));
        assert_eq!(entry_defects(&entry), vec![EntryDefect::SelfSupersede]);
    }

    #[test]
    fn an_entry_round_trips_through_its_own_serialization() -> Result<(), Box<dyn std::error::Error>>
    {
        let entry = bench_entry();
        let rendered = serde_json::to_string(&entry)?;
        let parsed: RegistryEntry = serde_json::from_str(&rendered)?;
        assert_eq!(parsed, entry);
        assert_eq!(parsed.kind(), EntryKind::Bench);
        assert_eq!(parsed.tier(), Tier::SelfReported);
        assert_eq!(
            parsed.expected_path(),
            "registry/entries/bench/example/2026-01-02-example-cdr.json"
        );
        Ok(())
    }
}
