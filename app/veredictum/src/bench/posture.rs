// SPDX-FileCopyrightText: Veredictum contributors
// SPDX-License-Identifier: Apache-2.0

//! Posture profiles, the disclosure block, and the canaries that check it.
//!
//! Two speed numbers are comparable only when the same features were switched
//! on behind them. A pack therefore defines NAMED posture profiles, a run
//! declares exactly one, and the record carries every disclosed item with the
//! value that was declared for it.
//!
//! A declaration alone is a promise. Each item is therefore probed black-box
//! where an observable exists, and the record labels the item
//! [`Assurance::Verified`] or [`Assurance::DeclaredOnly`] so a reader never has
//! to guess which. The probes run BEFORE and AFTER the measured window: a
//! reading that contradicts the declaration, and a pair of readings that
//! disagree with each other, both refuse the whole run with a typed error
//! naming the item. Nothing here is ever a footnote on a published number.
//!
//! What each probe can and cannot see:
//!
//! - **Version signing** — versions committed by the run's OWN seed traffic
//!   are read back and their `signature` inspected. Sampling the run's own
//!   commits means signing cannot be switched on for a probe alone. `signature`
//!   is `0..1` on `VERSION` and holds an "OpenPGP digital signature or digest"
//!   (openEHR RM `UML/classes/version.adoc` §Attributes), so the armor header
//!   separates the two schemes.
//! - **Commit validation** — the pack's own invalid twin is committed inside
//!   the run window. ITS-REST `specifications/responses/422.yaml` defines the
//!   refusal as "semantic validation errors, such as the underlying template is
//!   not known or is not validating the supplied resource", and
//!   `specifications/operations/composition_create.yaml` lists `422` on the
//!   commit, so a server validating against the template refuses the twin.
//! - **Authentication** — one request with no credential at all, which is the
//!   only way to see whether the declared mode is ENFORCED.
//! - **Compression** — one request stating `Accept-Encoding` explicitly, read
//!   over a client that does not decompress, so `Content-Encoding` survives.
//! - **TLS** — the recorded base URL's scheme, which is first-hand.
//! - **Audit and tenancy** — released ITS-REST surfaces no read resource for
//!   either, so both are honestly declared-only.
//!
//! [`PostureItem::is_observable`] is that list as a table, and
//! [`submission_defects`] is what a publication gate reads it through: an item
//! a canary observes carries [`Assurance::Verified`] or the block is refused,
//! and an item nothing discloses may never claim first-hand verification.

#![expect(
    clippy::disallowed_types,
    reason = "the approved wire-body seam: the signing canary reads one attribute out of a version document the SUT answered with"
)]

use std::fmt;

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::bench::BenchError;
use crate::bench::client::{BenchClient, PreferReturn, created_identifier};
use crate::bench::pack::Fixture;

/// Declares one closed posture vocabulary with its token table.
///
/// Every posture value is an enum with a fixed token, a `parse` that refuses
/// an unknown token loudly, and a serde representation that is the token
/// itself. A silent fallback to a default here would publish a posture the run
/// never had.
macro_rules! posture_vocabulary {
    (
        $(#[$meta:meta])*
        $name:ident, $vocabulary:literal, {
            $( $(#[$variant_meta:meta])* $variant:ident => $token:literal ),+ $(,)?
        }
    ) => {
        $(#[$meta])*
        #[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
        pub enum $name {
            $( $(#[$variant_meta])* $variant ),+
        }

        impl $name {
            /// Every value, in the order the schema enumerates them.
            pub const ALL: &[$name] = &[ $( $name::$variant ),+ ];

            /// The token this value is written as.
            #[must_use]
            pub const fn as_str(self) -> &'static str {
                match self {
                    $( $name::$variant => $token ),+
                }
            }

            /// Reads one token from the closed vocabulary.
            ///
            /// # Errors
            /// [`BenchError::UnknownToken`] listing the accepted tokens.
            pub fn parse(token: &str) -> Result<Self, BenchError> {
                Self::ALL
                    .iter()
                    .copied()
                    .find(|value| value.as_str() == token)
                    .ok_or_else(|| BenchError::UnknownToken {
                        vocabulary: $vocabulary,
                        token: token.to_owned(),
                        accepted: Self::ALL
                            .iter()
                            .map(|value| value.as_str())
                            .collect::<Vec<_>>()
                            .join(", "),
                    })
            }
        }

        impl fmt::Display for $name {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str(self.as_str())
            }
        }

        impl Serialize for $name {
            fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
                serializer.serialize_str(self.as_str())
            }
        }

        impl<'de> Deserialize<'de> for $name {
            fn deserialize<D: serde::Deserializer<'de>>(
                deserializer: D,
            ) -> Result<Self, D::Error> {
                let token = String::deserialize(deserializer)?;
                $name::parse(&token).map_err(serde::de::Error::custom)
            }
        }
    };
}

posture_vocabulary! {
    /// Which disclosed posture item a line of the block describes.
    PostureItem, "posture item", {
        /// Whether the deployment writes an audit trail, and to what sink.
        Audit => "audit",
        /// Whether committed versions carry a signature, and of which scheme.
        VersionSigning => "version_signing",
        /// How far the deployment validates a commit before accepting it.
        CommitValidation => "commit_validation",
        /// How the run authenticated, and whether the server enforces it.
        Authn => "authn",
        /// Whether the measured traffic rode TLS.
        Tls => "tls",
        /// Whether responses came back compressed.
        Compression => "compression",
        /// Whether the deployment serves one tenant or many.
        Tenancy => "tenancy",
    }
}

impl PostureItem {
    /// Whether a canary observes this item first-hand.
    ///
    /// An observable item has a wire surface that discloses it: committed
    /// versions carry `signature`, a commit of the pack's invalid twin is
    /// accepted or refused, an uncredentialed read is refused or answered, the
    /// base URL names its scheme, and a response is encoded or plain. Audit and
    /// tenancy have none, because released ITS-REST defines no read operation
    /// for either (`specifications/operations/`), so they are carried as claims
    /// and labelled as claims.
    #[must_use]
    pub const fn is_observable(self) -> bool {
        match self {
            PostureItem::Audit | PostureItem::Tenancy => false,
            PostureItem::VersionSigning
            | PostureItem::CommitValidation
            | PostureItem::Authn
            | PostureItem::Tls
            | PostureItem::Compression => true,
        }
    }
}

posture_vocabulary! {
    /// The audit sink a deployment declares.
    AuditSink, "audit sink", {
        /// No audit trail is written.
        Off => "off",
        /// An audit trail is written to the deployment's own log or store.
        Internal => "internal",
        /// An audit trail is shipped to an external sink such as a syslog
        /// collector.
        ExternalSink => "external-sink",
        /// An audit trail is written and served back over a read API.
        QueryableApi => "queryable-api",
    }
}

posture_vocabulary! {
    /// The version-signing scheme a deployment declares (openEHR RM
    /// `UML/classes/version.adoc` §Attributes: `signature` is an "OpenPGP
    /// digital signature or digest of content committed in this Version").
    SigningScheme, "signing scheme", {
        /// Committed versions carry no signature.
        None => "none",
        /// Committed versions carry a plain digest.
        Digest => "digest",
        /// Committed versions carry an openPGP signature.
        Pgp => "pgp",
    }
}

posture_vocabulary! {
    /// How far a deployment validates a commit before accepting it.
    ValidationDepth, "validation depth", {
        /// Nothing is validated beyond what the transport enforces.
        None => "none",
        /// The payload is checked for well-formedness only.
        Syntax => "syntax",
        /// The payload is validated against the reference model and the
        /// operational template it names.
        Template => "template",
    }
}

posture_vocabulary! {
    /// How the run presented itself, which is the authentication mode the
    /// canary then checks the server enforces.
    AuthnMode, "authn mode", {
        /// No credential was presented.
        None => "none",
        /// HTTP Basic.
        Basic => "basic",
        /// A bearer token.
        Bearer => "bearer",
    }
}

posture_vocabulary! {
    /// Whether the measured traffic rode TLS.
    TlsMode, "tls mode", {
        /// Plain HTTP.
        Off => "off",
        /// HTTPS.
        On => "on",
    }
}

posture_vocabulary! {
    /// Whether responses came back compressed.
    CompressionMode, "compression mode", {
        /// Responses are not compressed.
        Off => "off",
        /// Responses are compressed.
        Response => "response",
    }
}

posture_vocabulary! {
    /// Whether the deployment serves one tenant or many.
    Tenancy, "tenancy", {
        /// One tenant.
        Single => "single",
        /// Several tenants share the deployment.
        Multi => "multi",
    }
}

posture_vocabulary! {
    /// Which end of the measured window a canary reading was taken at.
    Bracket, "posture bracket", {
        /// Before the first measured repetition.
        Before => "before",
        /// After the last measured repetition.
        After => "after",
    }
}

posture_vocabulary! {
    /// What one canary reading concluded about the item it probed.
    CanaryOutcome, "canary outcome", {
        /// The observation agrees with the declaration.
        Confirmed => "confirmed",
        /// No observable exists, or the probe could not complete, so the
        /// declaration stands unchecked.
        NotObservable => "not-observable",
        /// The observation disagrees with the declaration, which refuses the
        /// run.
        Contradicted => "contradicted",
    }
}

posture_vocabulary! {
    /// Why one posture block may not be published beside a ranked number.
    PostureDefectKind, "posture defect", {
        /// The block carries no line for a disclosed item.
        Missing => "missing-item",
        /// A canary observes this item, and the block does not stand behind
        /// it first-hand.
        Unverified => "unverified-observable",
        /// Nothing on the wire discloses this item, and the block claims it
        /// was verified anyway.
        Unverifiable => "unverifiable-claim",
    }
}

posture_vocabulary! {
    /// How much of the declared value the record actually stands behind.
    Assurance, "assurance", {
        /// Both brackets observed the declared value first-hand.
        Verified => "verified",
        /// Nothing on the wire discloses the item, so the declaration is
        /// carried as a claim and labelled as one.
        DeclaredOnly => "declared-only",
    }
}

/// The token a reading records when nothing could be observed.
const NOT_OBSERVED: &str = "(not observable)";

/// The armor header that separates an openPGP signature from a plain digest
/// (openEHR RM `UML/classes/version.adoc` §Attributes names the two schemes
/// `signature` may hold).
const PGP_ARMOR: &str = "-----BEGIN PGP SIGNATURE-----";

/// How many committed versions the signing canary reads back per bracket.
const SIGNING_SAMPLES: usize = 3;

/// The `Accept-Encoding` the compression canary states, which is the same
/// offer the measured client makes.
const CANARY_ACCEPT_ENCODING: (&str, &str) = ("Accept-Encoding", "gzip, br");

/// The read the authentication and compression canaries probe over: an
/// authenticated list a run already drives in its preflight.
const CANARY_PATH: &str = "/definition/template/adl1.4";

/// A named posture profile: the deployment configuration a run declares.
///
/// The five items here are what an operator CONFIGURES, so they are part of
/// the versioned pack definition and changing one changes the pack version.
/// The two remaining disclosed items, authentication and TLS, are facts of the
/// invocation rather than of the profile, and the run supplies them.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PostureProfile {
    /// The profile name a run declares with `--posture`.
    pub name: &'static str,
    /// What the profile switches on, in one sentence, carried into the record.
    pub summary: &'static str,
    /// The declared audit sink.
    pub audit: AuditSink,
    /// The declared version-signing scheme.
    pub signing: SigningScheme,
    /// The declared commit-validation depth.
    pub validation: ValidationDepth,
    /// The declared response compression.
    pub compression: CompressionMode,
    /// The declared tenancy.
    pub tenancy: Tenancy,
}

/// The bare spec-conformant surface: nothing switched on beyond what a
/// conformant server must already do.
///
/// Validation is `template` rather than `none` because the specification puts
/// it there: ITS-REST `specifications/responses/422.yaml` defines the commit
/// refusal as the case where "the underlying template … is not validating the
/// supplied resource", so a server that accepts anything is below the floor
/// rather than merely lightly configured.
pub static MINIMAL: PostureProfile = PostureProfile {
    name: "minimal",
    summary: "The bare spec-conformant surface: no audit trail, unsigned versions, commits validated against the operational template, uncompressed responses, one tenant.",
    audit: AuditSink::Off,
    signing: SigningScheme::None,
    validation: ValidationDepth::Template,
    compression: CompressionMode::Off,
    tenancy: Tenancy::Single,
};

/// The configuration a clinical deployment typically runs: the minimal surface
/// with an audit trail written.
pub static CLINICAL_DEFAULT: PostureProfile = PostureProfile {
    name: "clinical-default",
    summary: "A clinical deployment's usual configuration: an audit trail written to the deployment's own store, unsigned versions, commits validated against the operational template, uncompressed responses, one tenant.",
    audit: AuditSink::Internal,
    signing: SigningScheme::None,
    validation: ValidationDepth::Template,
    compression: CompressionMode::Off,
    tenancy: Tenancy::Single,
};

impl PostureProfile {
    /// The declared value of one item, as its token.
    ///
    /// Authentication and TLS are invocation facts rather than profile
    /// choices, so the profile answers `None` for them and the run supplies
    /// the value.
    #[must_use]
    pub const fn declared(&self, item: PostureItem) -> Option<&'static str> {
        match item {
            PostureItem::Audit => Some(self.audit.as_str()),
            PostureItem::VersionSigning => Some(self.signing.as_str()),
            PostureItem::CommitValidation => Some(self.validation.as_str()),
            PostureItem::Compression => Some(self.compression.as_str()),
            PostureItem::Tenancy => Some(self.tenancy.as_str()),
            PostureItem::Authn | PostureItem::Tls => None,
        }
    }
}

/// One version the run's own seed traffic committed, addressed the way a read
/// addresses it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VersionSample {
    /// The EHR the version was committed into.
    pub ehr_id: String,
    /// The versioned object's uid.
    pub object_uid: String,
    /// The `OBJECT_VERSION_ID` the commit answered with.
    pub version_uid: String,
}

/// Everything one bracket of canaries probes against.
#[derive(Debug)]
pub struct CanaryTarget<'a> {
    /// The run's own client, credential and all.
    pub client: &'a BenchClient,
    /// The same target over a client that never decompresses, so
    /// `Content-Encoding` survives.
    pub raw: &'a BenchClient,
    /// The same target with no credential at all.
    pub anonymous: &'a BenchClient,
    /// The profile the run declared.
    pub profile: &'a PostureProfile,
    /// The authentication mode the run presents.
    pub authn: AuthnMode,
    /// Whether the recorded base URL is an HTTPS one.
    pub tls: TlsMode,
    /// The pack's invalid twin, which the validation canary commits.
    pub invalid_twin: Option<Fixture>,
    /// Versions the run's own seed phases committed.
    pub samples: &'a [VersionSample],
}

/// One canary reading, taken at one end of the measured window.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CanaryReading {
    /// Which end of the window the reading was taken at.
    pub bracket: Bracket,
    /// What the reading concluded.
    pub outcome: CanaryOutcome,
    /// What the probe actually saw, as a token.
    pub observed: String,
    /// The exchange the reading came from, in one sentence.
    pub evidence: String,
}

/// A canary reading that contradicts the run's declaration.
///
/// Carried boxed by [`BenchError::PostureContradiction`], so the error type
/// stays small enough for every `Result` in the engine to return it by value
/// (the `RootMismatch` shape, one module over).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PostureDisagreement {
    /// The disclosed item that disagrees.
    pub item: String,
    /// What the run declared for it.
    pub declared: String,
    /// Which bracket read it.
    pub bracket: String,
    /// What that bracket observed.
    pub observed: String,
    /// The exchange the reading came from.
    pub evidence: String,
}

/// One disclosed posture item: what was declared, how far it is stood behind,
/// and the two readings behind that label.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PostureDisclosure {
    /// Which item this line describes.
    pub item: PostureItem,
    /// The declared value, as its own closed vocabulary's token.
    pub declared: String,
    /// Whether the record stands behind the declaration first-hand.
    pub assurance: Assurance,
    /// The bracketing readings, before then after.
    pub readings: Vec<CanaryReading>,
}

/// One item on which the deployment that was measured departs from the
/// profile named in the same block.
///
/// A run declares the profile it is being read against, and a deployment
/// composed from somebody else's pinned recipe configures what that recipe
/// configures. Where the two disagree the run declares, and the canaries then
/// check, what the deployment actually does; this line is how the record says
/// so, rather than carrying a declaration the canaries would refuse.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PostureDivergence {
    /// Which item the two disagree on.
    pub item: PostureItem,
    /// The token the named profile assigns the item.
    pub profile_declares: String,
    /// The token the measured deployment's own configuration assigns it,
    /// which is what this block declared and the canaries checked.
    pub deployment_configures: String,
    /// Where that was read first-hand: the repository, the immutable tag, the
    /// file, and the element inside it.
    pub source: String,
}

/// The posture block one run's record carries.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PostureRecord {
    /// The declared profile's name.
    pub profile: String,
    /// What that profile switches on, verbatim from its own summary.
    pub summary: String,
    /// One line per item, in [`PostureItem::ALL`] order.
    pub items: Vec<PostureDisclosure>,
    /// Every item on which the measured deployment's own configuration
    /// departs from the named profile. Empty for a run that declared the
    /// profile as the pack defines it.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub comparability: Vec<PostureDivergence>,
}

impl PostureRecord {
    /// The declared value of one item, when the block carries it.
    #[must_use]
    pub fn declared(&self, item: PostureItem) -> Option<&str> {
        self.items
            .iter()
            .find(|line| line.item == item)
            .map(|line| line.declared.as_str())
    }

    /// A one-line `item=value` rendering of the whole block, in item order.
    ///
    /// Two runs are the same sport exactly when their signatures match, which
    /// is what a comparison compares.
    #[must_use]
    pub fn signature(&self) -> String {
        self.items
            .iter()
            .map(|line| format!("{}={}", line.item, line.declared))
            .collect::<Vec<_>>()
            .join(" ")
    }

    /// The items the block stands behind first-hand.
    #[must_use]
    pub fn verified_items(&self) -> Vec<PostureItem> {
        self.items
            .iter()
            .filter(|line| line.assurance == Assurance::Verified)
            .map(|line| line.item)
            .collect()
    }

    /// The items the block carries as a claim, because nothing on the wire
    /// discloses them.
    #[must_use]
    pub fn declared_only_items(&self) -> Vec<PostureItem> {
        self.items
            .iter()
            .filter(|line| line.assurance == Assurance::DeclaredOnly)
            .map(|line| line.item)
            .collect()
    }
}

/// One reason a posture block may not be published beside a ranked number.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PostureDefect {
    /// Which disclosed item the defect is about.
    pub item: PostureItem,
    /// What kind of defect it is.
    pub kind: PostureDefectKind,
    /// What the block says about the item, in one sentence.
    pub detail: String,
}

impl fmt::Display for PostureDefect {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "`{}` ({}): {}", self.item, self.kind, self.detail)
    }
}

/// Every reason this posture block falls short of the verification the
/// canaries can give.
///
/// A submission carries a number somebody will read as comparable, so the
/// posture behind it is held to what the machinery can actually establish:
/// every item [`PostureItem::is_observable`] names is [`Assurance::Verified`]
/// or the block is refused, an item nothing discloses may not claim
/// verification it cannot have, and a missing line is a defect rather than a
/// silently shorter block. An empty result is the only publishable one.
#[must_use]
pub fn submission_defects(record: &PostureRecord) -> Vec<PostureDefect> {
    let mut defects = Vec::new();
    for item in PostureItem::ALL.iter().copied() {
        let Some(line) = record.items.iter().find(|line| line.item == item) else {
            defects.push(PostureDefect {
                item,
                kind: PostureDefectKind::Missing,
                detail: "the block carries no line for this item".to_owned(),
            });
            continue;
        };
        match (item.is_observable(), line.assurance) {
            (true, Assurance::DeclaredOnly) => defects.push(PostureDefect {
                item,
                kind: PostureDefectKind::Unverified,
                detail: format!(
                    "a canary observes this item, and the block declares `{}` without standing behind it: {}",
                    line.declared,
                    unverified_evidence(line)
                ),
            }),
            (false, Assurance::Verified) => defects.push(PostureDefect {
                item,
                kind: PostureDefectKind::Unverifiable,
                detail: format!(
                    "nothing on the wire discloses this item, and the block claims `{}` was verified",
                    line.declared
                ),
            }),
            (true, Assurance::Verified) | (false, Assurance::DeclaredOnly) => {}
        }
    }
    defects
}

/// What the readings behind an unverified observable item actually said.
fn unverified_evidence(line: &PostureDisclosure) -> String {
    if line.readings.is_empty() {
        return "the block carries no reading at all".to_owned();
    }
    line.readings
        .iter()
        .map(|reading| {
            format!(
                "{} read {} ({})",
                reading.bracket, reading.outcome, reading.evidence
            )
        })
        .collect::<Vec<_>>()
        .join("; ")
}

/// Takes one bracket of readings, one per [`PostureItem`], in that order.
#[must_use]
pub fn bracket(target: &CanaryTarget<'_>, bracket: Bracket) -> Vec<(PostureItem, CanaryReading)> {
    PostureItem::ALL
        .iter()
        .copied()
        .map(|item| {
            let reading = match item {
                PostureItem::Audit => unobservable(
                    bracket,
                    "released ITS-REST defines no audit-trail read operation (specifications/operations/), so nothing on the wire discloses whether one is written or to which sink",
                ),
                PostureItem::Tenancy => unobservable(
                    bracket,
                    "released ITS-REST defines no tenancy resource (specifications/operations/), so nothing on the wire discloses how many tenants share the deployment",
                ),
                PostureItem::VersionSigning => probe_signing(target, bracket),
                PostureItem::CommitValidation => probe_validation(target, bracket),
                PostureItem::Authn => probe_authn(target, bracket),
                PostureItem::Tls => probe_tls(target, bracket),
                PostureItem::Compression => probe_compression(target, bracket),
            };
            (item, reading)
        })
        .collect()
}

/// A reading for an item nothing on the wire discloses.
fn unobservable(bracket: Bracket, evidence: &str) -> CanaryReading {
    CanaryReading {
        bracket,
        outcome: CanaryOutcome::NotObservable,
        observed: NOT_OBSERVED.to_owned(),
        evidence: evidence.to_owned(),
    }
}

/// A reading whose observation is compared against a declared token.
fn compared(bracket: Bracket, declared: &str, observed: &str, evidence: String) -> CanaryReading {
    let outcome = if declared == observed {
        CanaryOutcome::Confirmed
    } else {
        CanaryOutcome::Contradicted
    };
    CanaryReading {
        bracket,
        outcome,
        observed: observed.to_owned(),
        evidence,
    }
}

/// Reads back versions the run's own seed traffic committed and reports the
/// signing scheme they actually carry.
///
/// Sampling the run's OWN commits is the point: a scheme switched on for a
/// dedicated probe would not reach versions the measured population already
/// holds.
fn probe_signing(target: &CanaryTarget<'_>, bracket: Bracket) -> CanaryReading {
    let mut signatures = Vec::new();
    for sample in target.samples.iter().take(SIGNING_SAMPLES) {
        let path = format!(
            "/ehr/{}/versioned_composition/{}/version/{}",
            sample.ehr_id, sample.object_uid, sample.version_uid
        );
        let Ok(reply) = target.client.send(
            "posture canary: signing",
            reqwest::Method::GET,
            &path,
            None,
            PreferReturn::Unstated,
        ) else {
            return unobservable(
                bracket,
                "the version read the signing canary samples never reached a response",
            );
        };
        if !reply.status.is_success() {
            return unobservable(
                bracket,
                &format!(
                    "the version read the signing canary samples answered {}, so no version could be inspected",
                    reply.status
                ),
            );
        }
        let Ok(document) = serde_json::from_slice::<Value>(&reply.body) else {
            return unobservable(
                bracket,
                "the sampled version did not parse as JSON, so its signature could not be inspected",
            );
        };
        signatures.push(
            document
                .pointer("/signature")
                .and_then(Value::as_str)
                .map(str::to_owned)
                .filter(|signature| !signature.is_empty()),
        );
    }
    let sampled = signatures.len();
    if sampled == 0 {
        return unobservable(
            bracket,
            "the run committed no version the signing canary could sample",
        );
    }
    let signed: Vec<&str> = signatures
        .iter()
        .filter_map(|signature| signature.as_deref())
        .collect();
    let observed = if signed.is_empty() {
        SigningScheme::None
    } else if signed.len() != sampled {
        return unobservable(
            bracket,
            &format!(
                "{} of {sampled} sampled versions carried a signature and the rest did not, so no single scheme is in force",
                signed.len()
            ),
        );
    } else if signed.iter().all(|signature| signature.contains(PGP_ARMOR)) {
        SigningScheme::Pgp
    } else if signed.iter().any(|signature| signature.contains(PGP_ARMOR)) {
        return unobservable(
            bracket,
            "the sampled versions mixed armored and unarmored signatures, so no single scheme is in force",
        );
    } else {
        SigningScheme::Digest
    };
    compared(
        bracket,
        target.profile.signing.as_str(),
        observed.as_str(),
        format!(
            "{sampled} version(s) committed by this run's own seed traffic were read back through GET /ehr/{{ehr_id}}/versioned_composition/{{uid}}/version/{{version_uid}} and their `signature` inspected (openEHR RM UML/classes/version.adoc §Attributes)"
        ),
    )
}

/// What the validation canary saw when it offered the pack's invalid twin.
const TWIN_REFUSED: &str = "refuses-the-invalid-twin";

/// The other half of that observation.
const TWIN_ACCEPTED: &str = "accepts-the-invalid-twin";

/// The twin's fate a declared depth predicts.
///
/// Only [`ValidationDepth::Template`] reaches a missing mandatory attribute:
/// the twin is well-formed JSON, so both shallower depths predict acceptance.
const fn predicted_twin_fate(depth: ValidationDepth) -> &'static str {
    match depth {
        ValidationDepth::None | ValidationDepth::Syntax => TWIN_ACCEPTED,
        ValidationDepth::Template => TWIN_REFUSED,
    }
}

/// Commits the pack's invalid twin into a scratch EHR and reports its fate.
fn probe_validation(target: &CanaryTarget<'_>, bracket: Bracket) -> CanaryReading {
    let Some(twin) = target.invalid_twin else {
        return unobservable(
            bracket,
            "this pack embeds no invalid twin, so no commit refusal could be offered",
        );
    };
    let Ok(created) = target.client.send(
        "posture canary: validation scratch ehr",
        reqwest::Method::POST,
        "/ehr",
        None,
        PreferReturn::Identifier,
    ) else {
        return unobservable(
            bracket,
            "the scratch EHR the validation canary commits into never reached a response",
        );
    };
    let Some(ehr_id) = created_identifier(&created) else {
        return unobservable(
            bracket,
            "the scratch EHR the validation canary commits into disclosed no identifier",
        );
    };
    let Ok(commit) = target.client.send(
        "posture canary: validation twin",
        reqwest::Method::POST,
        &format!("/ehr/{ehr_id}/composition"),
        Some((twin.kind.media_type(), twin.bytes.as_bytes().to_vec())),
        PreferReturn::Identifier,
    ) else {
        return unobservable(
            bracket,
            "the invalid twin's commit never reached a response",
        );
    };
    let observed = if commit.status.is_success() {
        TWIN_ACCEPTED
    } else if commit.status.is_client_error() {
        TWIN_REFUSED
    } else {
        return unobservable(
            bracket,
            &format!(
                "the invalid twin's commit answered {}, which is neither an acceptance nor a client-side refusal, so it says nothing about validation",
                commit.status
            ),
        );
    };
    compared(
        bracket,
        predicted_twin_fate(target.profile.validation),
        observed,
        format!(
            "the pinned invalid twin `{}` — this pack's own composition with the mandatory COMPOSITION.composer [1..1] removed (openEHR RM UML/classes/composition.adoc §Attributes) — was committed and answered {} (ITS-REST specifications/responses/422.yaml: a commit is refused when the template \"is not validating the supplied resource\")",
            twin.key, commit.status
        ),
    )
}

/// The token the authentication canary records when the server refused an
/// uncredentialed request.
const AUTHN_ENFORCED: &str = "enforced";

/// The other half of that observation.
const AUTHN_NOT_ENFORCED: &str = "not-enforced";

/// What a declared authentication mode predicts an uncredentialed read meets.
const fn predicted_authn(mode: AuthnMode) -> &'static str {
    match mode {
        AuthnMode::None => AUTHN_NOT_ENFORCED,
        AuthnMode::Basic | AuthnMode::Bearer => AUTHN_ENFORCED,
    }
}

/// Issues one uncredentialed read and reports whether the server refused it.
fn probe_authn(target: &CanaryTarget<'_>, bracket: Bracket) -> CanaryReading {
    let Ok(reply) = target.anonymous.send(
        "posture canary: authn",
        reqwest::Method::GET,
        CANARY_PATH,
        None,
        PreferReturn::Unstated,
    ) else {
        return unobservable(
            bracket,
            "the uncredentialed read the authentication canary offers never reached a response",
        );
    };
    let unauthorized = reply.status == reqwest::StatusCode::UNAUTHORIZED
        || reply.status == reqwest::StatusCode::FORBIDDEN;
    let observed = if unauthorized {
        AUTHN_ENFORCED
    } else if reply.status.is_success() {
        AUTHN_NOT_ENFORCED
    } else {
        return unobservable(
            bracket,
            &format!(
                "the uncredentialed read answered {}, which is neither a refusal nor an acceptance, so it says nothing about enforcement",
                reply.status
            ),
        );
    };
    compared(
        bracket,
        predicted_authn(target.authn),
        observed,
        format!(
            "GET {CANARY_PATH} with no Authorization header answered {}",
            reply.status
        ),
    )
}

/// Reads the declared transport off the recorded base URL.
fn probe_tls(target: &CanaryTarget<'_>, bracket: Bracket) -> CanaryReading {
    let base_url = target.client.recorded_base_url();
    let observed = tls_of(&base_url);
    compared(
        bracket,
        target.tls.as_str(),
        observed.as_str(),
        format!("the recorded base URL `{base_url}` names its own scheme"),
    )
}

/// The transport a base URL names.
#[must_use]
pub fn tls_of(base_url: &str) -> TlsMode {
    if base_url.starts_with("https://") {
        TlsMode::On
    } else {
        TlsMode::Off
    }
}

/// Asks for a compressed response and reports whether one came back.
fn probe_compression(target: &CanaryTarget<'_>, bracket: Bracket) -> CanaryReading {
    let Ok(reply) = target.raw.send_with_headers(
        "posture canary: compression",
        reqwest::Method::GET,
        CANARY_PATH,
        None,
        PreferReturn::Unstated,
        &[CANARY_ACCEPT_ENCODING],
    ) else {
        return unobservable(
            bracket,
            "the read the compression canary offers never reached a response",
        );
    };
    if !reply.status.is_success() {
        return unobservable(
            bracket,
            &format!(
                "the read the compression canary offers answered {}, so no response body was encoded either way",
                reply.status
            ),
        );
    }
    let encoding = reply
        .content_encoding
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty() && !value.eq_ignore_ascii_case("identity"));
    let observed = match encoding {
        Some(_) => CompressionMode::Response,
        None => CompressionMode::Off,
    };
    compared(
        bracket,
        target.profile.compression.as_str(),
        observed.as_str(),
        format!(
            "GET {CANARY_PATH} stating `Accept-Encoding: {}` answered Content-Encoding: {}",
            CANARY_ACCEPT_ENCODING.1,
            encoding.unwrap_or("(absent)")
        ),
    )
}

/// Settles the two brackets into the block the record carries.
///
/// A contradiction at either end, and a disagreement between the ends, are
/// both refusals: the first says the run measured a system other than the one
/// it declared, and the second says the posture moved while the numbers were
/// being taken. Neither is recordable as a footnote on a published figure.
///
/// # Errors
/// [`BenchError::PostureContradiction`] naming the item whose observation
/// disagrees with the declaration, [`BenchError::PostureFlip`] naming the item
/// whose two readings disagree with each other, and
/// [`BenchError::PostureBracket`] when a bracket is missing an item.
pub fn settle(
    profile: &PostureProfile,
    authn: AuthnMode,
    tls: TlsMode,
    before: &[(PostureItem, CanaryReading)],
    after: &[(PostureItem, CanaryReading)],
) -> Result<PostureRecord, BenchError> {
    let mut items = Vec::with_capacity(PostureItem::ALL.len());
    for item in PostureItem::ALL.iter().copied() {
        let declared = declared_value(profile, authn, tls, item);
        let first = reading_of(before, item, Bracket::Before)?;
        let second = reading_of(after, item, Bracket::After)?;
        for reading in [first, second] {
            if reading.outcome == CanaryOutcome::Contradicted {
                return Err(BenchError::PostureContradiction(Box::new(
                    PostureDisagreement {
                        item: item.as_str().to_owned(),
                        declared,
                        bracket: reading.bracket.as_str().to_owned(),
                        observed: reading.observed.clone(),
                        evidence: reading.evidence.clone(),
                    },
                )));
            }
        }
        if first.outcome != second.outcome || first.observed != second.observed {
            return Err(BenchError::PostureFlip {
                item: item.as_str().to_owned(),
                before: format!("{} ({})", first.observed, first.outcome),
                after: format!("{} ({})", second.observed, second.outcome),
            });
        }
        let assurance = if first.outcome == CanaryOutcome::Confirmed {
            Assurance::Verified
        } else {
            Assurance::DeclaredOnly
        };
        items.push(PostureDisclosure {
            item,
            declared,
            assurance,
            readings: vec![first.clone(), second.clone()],
        });
    }
    Ok(PostureRecord {
        profile: profile.name.to_owned(),
        summary: profile.summary.to_owned(),
        items,
        comparability: Vec::new(),
    })
}

/// The declared token of one item: the profile's for the five an operator
/// configures, and the invocation's for the two it settles.
///
/// The match is exhaustive on purpose, so a new [`PostureItem`] is a compile
/// error here rather than an item that silently inherits another's value.
fn declared_value(
    profile: &PostureProfile,
    authn: AuthnMode,
    tls: TlsMode,
    item: PostureItem,
) -> String {
    match item {
        PostureItem::Audit => profile.audit.as_str().to_owned(),
        PostureItem::VersionSigning => profile.signing.as_str().to_owned(),
        PostureItem::CommitValidation => profile.validation.as_str().to_owned(),
        PostureItem::Compression => profile.compression.as_str().to_owned(),
        PostureItem::Tenancy => profile.tenancy.as_str().to_owned(),
        PostureItem::Authn => authn.as_str().to_owned(),
        PostureItem::Tls => tls.as_str().to_owned(),
    }
}

/// One item's reading out of a bracket.
fn reading_of(
    readings: &[(PostureItem, CanaryReading)],
    item: PostureItem,
    bracket: Bracket,
) -> Result<&CanaryReading, BenchError> {
    readings
        .iter()
        .find(|(key, _)| *key == item)
        .map(|(_, reading)| reading)
        .ok_or_else(|| BenchError::PostureBracket {
            item: item.as_str().to_owned(),
            bracket: bracket.as_str().to_owned(),
        })
}

#[cfg(test)]
#[expect(
    clippy::panic_in_result_fn,
    reason = "Result-returning tests in the Book ch11 shape, each asserting; \
              clippy offers no allow-in-tests knob for this lint"
)]
mod tests {
    use super::*;

    /// One reading, for the settle tests below.
    fn reading(bracket: Bracket, outcome: CanaryOutcome, observed: &str) -> CanaryReading {
        CanaryReading {
            bracket,
            outcome,
            observed: observed.to_owned(),
            evidence: "a synthetic reading".to_owned(),
        }
    }

    /// A whole bracket whose every item confirms the declared profile.
    fn confirming(profile: &PostureProfile, bracket: Bracket) -> Vec<(PostureItem, CanaryReading)> {
        PostureItem::ALL
            .iter()
            .copied()
            .map(|item| {
                let entry = match item {
                    PostureItem::Audit | PostureItem::Tenancy => {
                        reading(bracket, CanaryOutcome::NotObservable, NOT_OBSERVED)
                    }
                    PostureItem::VersionSigning => {
                        reading(bracket, CanaryOutcome::Confirmed, profile.signing.as_str())
                    }
                    PostureItem::CommitValidation => reading(
                        bracket,
                        CanaryOutcome::Confirmed,
                        predicted_twin_fate(profile.validation),
                    ),
                    PostureItem::Authn => {
                        reading(bracket, CanaryOutcome::Confirmed, AUTHN_NOT_ENFORCED)
                    }
                    PostureItem::Tls => {
                        reading(bracket, CanaryOutcome::Confirmed, TlsMode::Off.as_str())
                    }
                    PostureItem::Compression => reading(
                        bracket,
                        CanaryOutcome::Confirmed,
                        profile.compression.as_str(),
                    ),
                };
                (item, entry)
            })
            .collect()
    }

    /// Every posture token round-trips, and an unknown one is refused rather
    /// than read as a default posture.
    #[test]
    fn every_posture_token_round_trips() -> Result<(), BenchError> {
        for item in PostureItem::ALL {
            assert_eq!(PostureItem::parse(item.as_str())?, *item);
        }
        for sink in AuditSink::ALL {
            assert_eq!(AuditSink::parse(sink.as_str())?, *sink);
        }
        for scheme in SigningScheme::ALL {
            assert_eq!(SigningScheme::parse(scheme.as_str())?, *scheme);
        }
        for depth in ValidationDepth::ALL {
            assert_eq!(ValidationDepth::parse(depth.as_str())?, *depth);
        }
        for mode in AuthnMode::ALL {
            assert_eq!(AuthnMode::parse(mode.as_str())?, *mode);
        }
        for mode in TlsMode::ALL {
            assert_eq!(TlsMode::parse(mode.as_str())?, *mode);
        }
        for mode in CompressionMode::ALL {
            assert_eq!(CompressionMode::parse(mode.as_str())?, *mode);
        }
        for tenancy in Tenancy::ALL {
            assert_eq!(Tenancy::parse(tenancy.as_str())?, *tenancy);
        }
        for kind in PostureDefectKind::ALL {
            assert_eq!(PostureDefectKind::parse(kind.as_str())?, *kind);
        }
        assert!(SigningScheme::parse("PGP").is_err());
        assert!(ValidationDepth::parse("strict").is_err());
        assert!(AuditSink::parse("on").is_err());
        Ok(())
    }

    /// Exactly the two items released ITS-REST surfaces no read resource for
    /// are the ones no canary observes.
    #[test]
    fn only_audit_and_tenancy_are_unobservable() {
        let unobservable: Vec<PostureItem> = PostureItem::ALL
            .iter()
            .copied()
            .filter(|item| !item.is_observable())
            .collect();
        assert_eq!(unobservable, vec![PostureItem::Audit, PostureItem::Tenancy]);
    }

    /// A block settled from two confirming brackets is publishable, and every
    /// item it stands behind is one a canary actually observes.
    #[test]
    fn a_settled_block_carries_the_verification_the_canaries_can_give()
    -> Result<(), Box<dyn std::error::Error>> {
        let record = settle(
            &MINIMAL,
            AuthnMode::None,
            TlsMode::Off,
            &confirming(&MINIMAL, Bracket::Before),
            &confirming(&MINIMAL, Bracket::After),
        )?;
        assert_eq!(submission_defects(&record), Vec::new());
        assert!(
            record
                .verified_items()
                .iter()
                .all(|item| item.is_observable())
        );
        assert_eq!(
            record.declared_only_items(),
            vec![PostureItem::Audit, PostureItem::Tenancy]
        );
        Ok(())
    }

    /// A disclosure line serializes as its tokens and reads back identically,
    /// so the emitted block and the model cannot drift apart.
    #[test]
    fn a_disclosure_serializes_as_its_tokens() -> Result<(), Box<dyn std::error::Error>> {
        let line = PostureDisclosure {
            item: PostureItem::VersionSigning,
            declared: SigningScheme::Digest.as_str().to_owned(),
            assurance: Assurance::Verified,
            readings: vec![reading(Bracket::Before, CanaryOutcome::Confirmed, "digest")],
        };
        let text = serde_json::to_string(&line)?;
        assert!(text.contains("\"item\":\"version_signing\""), "{text}");
        assert!(text.contains("\"assurance\":\"verified\""), "{text}");
        assert!(text.contains("\"bracket\":\"before\""), "{text}");
        let back: PostureDisclosure = serde_json::from_str(&text)?;
        assert_eq!(back, line);
        Ok(())
    }

    /// Two confirming brackets label every observable item verified and leave
    /// the two nothing discloses honestly declared-only.
    #[test]
    fn matching_brackets_label_each_item_by_what_was_seen() -> Result<(), Box<dyn std::error::Error>>
    {
        let record = settle(
            &MINIMAL,
            AuthnMode::None,
            TlsMode::Off,
            &confirming(&MINIMAL, Bracket::Before),
            &confirming(&MINIMAL, Bracket::After),
        )?;
        assert_eq!(record.profile, "minimal");
        assert_eq!(record.items.len(), PostureItem::ALL.len());
        assert_eq!(
            record.verified_items(),
            vec![
                PostureItem::VersionSigning,
                PostureItem::CommitValidation,
                PostureItem::Authn,
                PostureItem::Tls,
                PostureItem::Compression,
            ]
        );
        for item in [PostureItem::Audit, PostureItem::Tenancy] {
            let line = record
                .items
                .iter()
                .find(|line| line.item == item)
                .ok_or("the item is missing")?;
            assert_eq!(line.assurance, Assurance::DeclaredOnly);
            assert_eq!(line.readings.len(), 2);
        }
        assert_eq!(record.declared(PostureItem::Authn), Some("none"));
        assert_eq!(record.declared(PostureItem::Tls), Some("off"));
        assert!(record.signature().contains("commit_validation=template"));
        Ok(())
    }

    /// A canary that contradicts the declaration refuses the run and names the
    /// item, rather than recording a footnote beside a published number.
    #[test]
    fn a_contradicting_canary_refuses_the_run() {
        let mut after = confirming(&MINIMAL, Bracket::After);
        for (item, entry) in &mut after {
            if *item == PostureItem::CommitValidation {
                *entry = reading(Bracket::After, CanaryOutcome::Contradicted, TWIN_ACCEPTED);
            }
        }
        let error = settle(
            &MINIMAL,
            AuthnMode::None,
            TlsMode::Off,
            &confirming(&MINIMAL, Bracket::Before),
            &after,
        )
        .unwrap_err();
        assert!(
            matches!(error, BenchError::PostureContradiction(_)),
            "{error}"
        );
        assert!(error.to_string().contains("commit_validation"), "{error}");
        assert!(
            error.to_string().contains("accepts-the-invalid-twin"),
            "{error}"
        );
    }

    /// A posture that moved between the two brackets refuses the run, because
    /// the measured window then straddles two different systems.
    #[test]
    fn a_flip_between_brackets_refuses_the_run() {
        let mut after = confirming(&MINIMAL, Bracket::After);
        for (item, entry) in &mut after {
            if *item == PostureItem::Compression {
                *entry = reading(Bracket::After, CanaryOutcome::NotObservable, NOT_OBSERVED);
            }
        }
        let error = settle(
            &MINIMAL,
            AuthnMode::None,
            TlsMode::Off,
            &confirming(&MINIMAL, Bracket::Before),
            &after,
        )
        .unwrap_err();
        assert!(matches!(error, BenchError::PostureFlip { .. }), "{error}");
        assert!(error.to_string().contains("compression"), "{error}");
    }

    /// A bracket missing an item is a refusal rather than a silently short
    /// block.
    #[test]
    fn a_short_bracket_refuses_the_run() {
        let mut before = confirming(&MINIMAL, Bracket::Before);
        before.retain(|(item, _)| *item != PostureItem::Tls);
        let error = settle(
            &MINIMAL,
            AuthnMode::None,
            TlsMode::Off,
            &before,
            &confirming(&MINIMAL, Bracket::After),
        )
        .unwrap_err();
        assert!(
            matches!(error, BenchError::PostureBracket { .. }),
            "{error}"
        );
    }

    /// The two profiles differ in exactly the item their names promise, so a
    /// reader can tell what changed between them.
    #[test]
    fn the_two_profiles_differ_only_in_the_audit_item() {
        assert_eq!(MINIMAL.audit, AuditSink::Off);
        assert_eq!(CLINICAL_DEFAULT.audit, AuditSink::Internal);
        assert_eq!(MINIMAL.signing, CLINICAL_DEFAULT.signing);
        assert_eq!(MINIMAL.validation, CLINICAL_DEFAULT.validation);
        assert_eq!(MINIMAL.compression, CLINICAL_DEFAULT.compression);
        assert_eq!(MINIMAL.tenancy, CLINICAL_DEFAULT.tenancy);
        assert_ne!(MINIMAL.name, CLINICAL_DEFAULT.name);
    }

    /// A profile answers for the five items it configures and leaves the two
    /// the invocation settles to the run.
    #[test]
    fn a_profile_declares_only_what_an_operator_configures() {
        assert_eq!(MINIMAL.declared(PostureItem::Audit), Some("off"));
        assert_eq!(MINIMAL.declared(PostureItem::Authn), None);
        assert_eq!(MINIMAL.declared(PostureItem::Tls), None);
    }

    /// The transport is read off the URL scheme, never assumed.
    #[test]
    fn the_transport_follows_the_url_scheme() {
        assert_eq!(tls_of("https://cdr.example/openehr/v1"), TlsMode::On);
        assert_eq!(tls_of("http://127.0.0.1:8080/openehr/v1"), TlsMode::Off);
        assert_eq!(tls_of("cdr.example"), TlsMode::Off);
    }
}
