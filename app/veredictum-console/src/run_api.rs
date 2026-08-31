// SPDX-FileCopyrightText: Veredictum contributors
// SPDX-License-Identifier: Apache-2.0

//! The run wizard's server seam for S3 Connect and S4 Scope (#65).
//!
//! The wizard's memory is a server-side draft PER SUBMITTER (#389): the
//! connection facts, the credential VALUES (memory only — never persisted,
//! never rendered back, never logged), the statement pick and the filter. Two
//! visitors composing a connection at once each have their own, and what the
//! client can read back is [`DraftView`], which carries no secret by
//! construction.
//!
//! The reachability probe is the ONE console-originated request to a CDR: a
//! diagnostic rendered verbatim and never judged, so conformance traffic
//! stays the spawned instrument's alone (#54).

use serde::{Deserialize, Serialize};

/// The authentication choice, exactly the ixit's `AuthMode` vocabulary the
/// first cut supports (`bearer_mint` is deferred, #70).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum AuthChoice {
    /// No Authorization header at all.
    None,
    /// HTTP Basic.
    Basic,
    /// A static OAuth2 bearer token.
    Bearer,
}

impl AuthChoice {
    /// The declaration token, matching the ixit's `mode`.
    #[must_use]
    pub fn token(self) -> &'static str {
        match self {
            Self::None => "none",
            Self::Basic => "basic",
            Self::Bearer => "bearer",
        }
    }
}

/// The version-signing posture the operator declares — the mode half.
///
/// The mode is a closed vocabulary (RM common
/// `master06-change_control_package.adoc` §Digital Signature: a deployment
/// signs by digest or by openPGP). `Undeclared` is a first-class answer: a
/// run whose ixit declares no posture records every `verifiable` case
/// not-applicable with the engine's own citation, which is the honest
/// outcome for a deployment fact nobody supplied.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum SigningChoice {
    /// The operator declares no signing posture.
    #[default]
    Undeclared,
    /// Plain digest, no public-key infrastructure.
    Digest,
    /// openPGP: a detached signature over the canonical form, verified
    /// against a declared public key.
    Pgp,
}

impl SigningChoice {
    /// Every choice, in the order the control renders.
    pub const ALL: [SigningChoice; 3] = [
        SigningChoice::Undeclared,
        SigningChoice::Digest,
        SigningChoice::Pgp,
    ];

    /// The label the control carries.
    #[must_use]
    pub fn label(self) -> &'static str {
        match self {
            Self::Undeclared => "not declared",
            Self::Digest => "digest",
            Self::Pgp => "openPGP",
        }
    }

    /// The id of this choice's button.
    #[must_use]
    pub fn control_id(self) -> &'static str {
        match self {
            Self::Undeclared => "signing-undeclared",
            Self::Digest => "signing-digest",
            Self::Pgp => "signing-pgp",
        }
    }
}

/// How a digest-mode signature is encoded on the wire.
///
/// Exactly the encodings the engine's own verifier implements, so a posture
/// the console composes can never name one the run cannot verify under.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum DigestEncoding {
    /// Standard base64 with padding.
    #[default]
    Base64,
    /// URL-safe base64 without padding.
    Base64Url,
}

impl DigestEncoding {
    /// Every encoding, in the order the control renders.
    pub const ALL: [DigestEncoding; 2] = [DigestEncoding::Base64, DigestEncoding::Base64Url];

    /// The ixit declaration token.
    #[must_use]
    pub fn token(self) -> &'static str {
        match self {
            Self::Base64 => "base64",
            Self::Base64Url => "base64url",
        }
    }

    /// The id of this encoding's button.
    #[must_use]
    pub fn control_id(self) -> &'static str {
        match self {
            Self::Base64 => "digest-base64",
            Self::Base64Url => "digest-base64url",
        }
    }
}

/// The openEHR generation set the deployment runs (`spec_profile`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum SpecProfileChoice {
    /// The operator declares no generation set.
    #[default]
    Undeclared,
    /// The latest RELEASED openEHR generations.
    Stable,
    /// The development generations.
    Development,
}

impl SpecProfileChoice {
    /// Every choice, in the order the control renders.
    pub const ALL: [SpecProfileChoice; 3] = [
        SpecProfileChoice::Undeclared,
        SpecProfileChoice::Stable,
        SpecProfileChoice::Development,
    ];

    /// The label the control carries.
    #[must_use]
    pub fn label(self) -> &'static str {
        match self {
            Self::Undeclared => "not declared",
            Self::Stable => "stable",
            Self::Development => "development",
        }
    }

    /// The id of this choice's button.
    #[must_use]
    pub fn control_id(self) -> &'static str {
        match self {
            Self::Undeclared => "profile-undeclared",
            Self::Stable => "profile-stable",
            Self::Development => "profile-development",
        }
    }

    /// The declared generation set, `None` for an undeclared one.
    #[cfg(feature = "ssr")]
    #[must_use]
    pub fn declared(self) -> Option<veredictum::ixit::SpecProfile> {
        match self {
            Self::Undeclared => None,
            Self::Stable => Some(veredictum::ixit::SpecProfile::Stable),
            Self::Development => Some(veredictum::ixit::SpecProfile::Development),
        }
    }
}

/// What the browser sends for the deployment postures: one flat form, so the
/// public boundary carries no nested enum and every field is a scalar or a
/// closed vocabulary.
///
/// The form is UNTRUSTED input like every other server-fn argument. It is
/// narrowed to the typed [`DeclaredPostures`] at that boundary, where an
/// unusable declaration is refused by name rather than composed into an ixit
/// the run cannot use.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct PostureForm {
    /// The deployment's configured system identifier (`system_id`).
    pub system_id: String,
    /// A location on the SUT's OWN file system the admin dump/load operations
    /// may write to (`dump_location`). The console never opens it: it travels
    /// to the SUT in the request body those operations carry.
    pub dump_location: String,
    /// The signing mode.
    pub signing: SigningChoice,
    /// The digest encoding, read only for the digest mode.
    pub digest_encoding: DigestEncoding,
    /// The fixed prefix a digest-mode signature carries, read only for the
    /// digest mode. May be empty.
    pub digest_prefix: String,
    /// The armored openPGP public key, read only for the pgp mode.
    pub pgp_public_key: String,
    /// The openEHR generation set.
    pub spec_profile: SpecProfileChoice,
}

/// The signing posture a draft carries, narrowed from the flat form.
///
/// The parameters of a mode travel with that mode, so a digest posture can
/// never hold a key and a pgp posture can never hold an encoding.
#[cfg(feature = "ssr")]
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SigningPosture {
    /// Plain digest, no public-key infrastructure.
    Digest {
        /// How the digest is encoded on the wire.
        encoding: DigestEncoding,
        /// The fixed prefix the wire form carries before the encoded digest.
        prefix: String,
    },
    /// openPGP, verified against this armored public key.
    Pgp {
        /// The armored public key.
        public_key: String,
    },
}

/// The deployment postures a draft declares, each absent by default.
///
/// An absent fact is a FIRST-CLASS state: it composes no key in the ixit, and
/// the engine's own selection law then records the cases that need it
/// not-applicable with a citation. Nothing here ever stands in for a
/// declaration the operator did not make.
#[cfg(feature = "ssr")]
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct DeclaredPostures {
    /// The deployment's configured system identifier.
    pub system_id: Option<String>,
    /// A location on the SUT's own file system for the admin dump/load
    /// operations. The console never opens it.
    pub dump_location: Option<String>,
    /// The version-signing posture.
    pub signing: Option<SigningPosture>,
    /// The openEHR generation set.
    pub spec_profile: Option<veredictum::ixit::SpecProfile>,
}

#[cfg(feature = "ssr")]
impl DeclaredPostures {
    /// The declared facts, one line each, for the interface to state back —
    /// empty when the operator declared none.
    ///
    /// A pgp key is summarized rather than echoed: it is public material, and
    /// a screen restating kilobytes of armor tells the reader nothing.
    #[must_use]
    pub fn summary(&self) -> Vec<String> {
        let mut lines = Vec::new();
        if let Some(system_id) = &self.system_id {
            lines.push(format!("system_id {system_id}"));
        }
        if let Some(dump_location) = &self.dump_location {
            lines.push(format!("dump_location {dump_location}"));
        }
        match &self.signing {
            None => {}
            Some(SigningPosture::Digest { encoding, prefix }) => lines.push(format!(
                "signing digest (sha256, {}, prefix {:?})",
                encoding.token(),
                prefix
            )),
            Some(SigningPosture::Pgp { .. }) => {
                lines.push(String::from("signing openPGP (public key declared)"));
            }
        }
        if let Some(profile) = self.spec_profile {
            lines.push(format!("spec_profile {}", profile.token()));
        }
        lines
    }
}

/// The postures the flat form declares, or the refusal naming the one field
/// that cannot be used.
///
/// Every value is UNTRUSTED input on a public endpoint, so each is capped and
/// each unusable declaration is an error rather than a silently dropped
/// field: a posture the operator believes they declared, and the run then
/// judged without, is the defect this whole seam exists to close.
///
/// # Errors
/// A value past its cap, a pgp mode with no armored key, and a digest prefix
/// past its cap — each naming the field.
#[cfg(feature = "ssr")]
pub fn declared_postures(form: &PostureForm) -> Result<DeclaredPostures, String> {
    /// A system identifier is a configured name, not a document.
    const SYSTEM_ID_CAP_BYTES: usize = 256;
    /// A path on the SUT's own file system, capped well past any real one.
    const DUMP_LOCATION_CAP_BYTES: usize = 4_096;
    /// A self-describing signature prefix (`sha256:`), never a payload.
    const DIGEST_PREFIX_CAP_BYTES: usize = 64;
    /// An armored public key, capped far above any real one.
    const PGP_KEY_CAP_BYTES: usize = 65_536;
    /// What the engine's own openPGP reader needs to see first.
    const PGP_ARMOR_HEADER: &str = "-----BEGIN PGP PUBLIC KEY BLOCK-----";

    fn capped(field: &str, value: &str, cap: usize) -> Result<Option<String>, String> {
        let value = value.trim();
        if value.len() > cap {
            return Err(format!(
                "{field} is {} bytes; the cap is {cap}",
                value.len()
            ));
        }
        Ok((!value.is_empty()).then(|| value.to_owned()))
    }

    let signing = match form.signing {
        SigningChoice::Undeclared => None,
        SigningChoice::Digest => Some(SigningPosture::Digest {
            encoding: form.digest_encoding,
            prefix: capped(
                "the digest prefix",
                &form.digest_prefix,
                DIGEST_PREFIX_CAP_BYTES,
            )?
            .unwrap_or_default(),
        }),
        SigningChoice::Pgp => {
            let key = capped(
                "the openPGP public key",
                &form.pgp_public_key,
                PGP_KEY_CAP_BYTES,
            )?
            .ok_or_else(|| {
                String::from(
                    "the openPGP signing posture declares no public key: paste the \
                     armored key the deployment signs with, or leave the posture undeclared",
                )
            })?;
            if !key.contains(PGP_ARMOR_HEADER) {
                return Err(format!(
                    "the openPGP public key carries no {PGP_ARMOR_HEADER} line, so the run's \
                     verifier cannot read it"
                ));
            }
            Some(SigningPosture::Pgp { public_key: key })
        }
    };
    Ok(DeclaredPostures {
        system_id: capped("the system id", &form.system_id, SYSTEM_ID_CAP_BYTES)?,
        dump_location: capped(
            "the dump location",
            &form.dump_location,
            DUMP_LOCATION_CAP_BYTES,
        )?,
        signing,
        spec_profile: form.spec_profile.declared(),
    })
}

/// The server-side draft (ssr only; never serialized to the client).
#[cfg(feature = "ssr")]
#[derive(Debug)]
pub struct RunDraft {
    /// The CDR base URL.
    pub base_url: String,
    /// The SUT display name for the record.
    pub sut_name: String,
    /// The SUT version label for the record.
    pub sut_version: String,
    /// The authentication mode.
    pub auth: AuthChoice,
    /// The credential values, redacted from every rendering.
    pub credentials: Vec<crate::engine::Credential>,
    /// Whether the probe answered 2xx for these facts.
    pub probed_ok: bool,
    /// The pasted statement DOCUMENT (the vendor's own claim), when any —
    /// validated content, written into the job's output directory at start.
    pub statement_json: Option<String>,
    /// The claim's product identity, parsed once at save time.
    pub statement_product: Option<String>,
    /// The case-id filter, when any.
    pub filter: Option<String>,
    /// Whether the run persists its wire exchanges as `transcript.json`
    /// beside the results (#96). Off unless the operator asks for it.
    pub record_exchanges: bool,
    /// The deployment postures the operator declared, each absent until they
    /// declare it (#456).
    pub postures: DeclaredPostures,
}

/// Every visitor's connection draft, one per submitter.
///
/// Bounded and evicted oldest-first exactly as the job map is
/// (`run_job::DRAFTS_KEPT`): these entries hold credential VALUES, so an
/// unbounded map of them is not an option. A draft lives in memory and in the
/// spawned run's environment, and reaches no file, no log line and no
/// client-visible state.
#[cfg(feature = "ssr")]
#[derive(Debug, Default)]
pub struct Drafts {
    entries: std::collections::BTreeMap<crate::submitter::Submitter, (u64, RunDraft)>,
    next: u64,
}

#[cfg(feature = "ssr")]
impl Drafts {
    /// An empty set of drafts.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// This submitter's draft, when they have one.
    #[must_use]
    pub fn get(&self, submitter: crate::submitter::Submitter) -> Option<&RunDraft> {
        self.entries.get(&submitter).map(|(_, draft)| draft)
    }

    /// This submitter's draft, to be edited in place.
    pub fn get_mut(&mut self, submitter: crate::submitter::Submitter) -> Option<&mut RunDraft> {
        self.entries.get_mut(&submitter).map(|(_, draft)| draft)
    }

    /// Stores this submitter's draft, replacing any prior one, and evicts the
    /// oldest drafts past the cap.
    pub fn insert(&mut self, submitter: crate::submitter::Submitter, draft: RunDraft) {
        let seq = self.next;
        self.next = self.next.saturating_add(1);
        self.entries.insert(submitter, (seq, draft));
        let excess = self
            .entries
            .len()
            .saturating_sub(crate::run_job::DRAFTS_KEPT);
        if excess == 0 {
            return;
        }
        let mut ages: Vec<(u64, crate::submitter::Submitter)> = self
            .entries
            .iter()
            .map(|(who, (seq, _))| (*seq, *who))
            .collect();
        ages.sort_unstable();
        for (_, who) in ages.into_iter().take(excess) {
            self.entries.remove(&who);
        }
    }
}

/// What the client may read back of the draft: no secret, by construction.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DraftView {
    /// The CDR base URL.
    pub base_url: String,
    /// The SUT display name.
    pub sut_name: String,
    /// The SUT version label.
    pub sut_version: String,
    /// The authentication mode token.
    pub auth: String,
    /// Whether the probe answered 2xx.
    pub probed_ok: bool,
    /// The saved claim's product identity, when a statement was pasted.
    pub statement: Option<String>,
    /// The case-id filter, when any.
    pub filter: Option<String>,
    /// Whether the run will record its wire exchanges.
    pub record_exchanges: bool,
    /// The deployment postures the draft declares, one line each — empty when
    /// the operator declared none.
    pub postures: Vec<String>,
}

/// The accepted claim, summarized for the overview the operator reads
/// before starting: who claims, which tiers, how many capabilities.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ClaimSummary {
    /// The product identity the statement declares.
    pub product: String,
    /// The claimed profile tiers, the statement's own tokens.
    pub profiles: Vec<String>,
    /// The number of claimed verdict-bearing capabilities.
    pub capabilities: u64,
}

/// A profile tier the Scope screen can build a claim from.
///
/// The four the verdict machinery computes a profile answer for; the
/// Enterprise rungs have no verdict rule, so they are not offerable.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ScopeTier {
    /// Platform CORE.
    Core,
    /// Platform STANDARD, which requires CORE.
    Standard,
    /// Platform OPTIONS: the optional Platform capabilities, rated when
    /// present.
    Options,
    /// The Security family's basic rung.
    SecBasic,
}

impl ScopeTier {
    /// Every offerable tier, in the order the row renders.
    pub const ALL: [ScopeTier; 4] = [
        ScopeTier::Core,
        ScopeTier::Standard,
        ScopeTier::Options,
        ScopeTier::SecBasic,
    ];

    /// The tier token, the statement's own vocabulary.
    #[must_use]
    pub fn token(self) -> &'static str {
        match self {
            Self::Core => "CORE",
            Self::Standard => "STANDARD",
            Self::Options => "OPTIONS",
            Self::SecBasic => "SEC-BASIC",
        }
    }

    /// The id of the tier's checkbox control.
    #[must_use]
    pub fn control_id(self) -> &'static str {
        match self {
            Self::Core => "tier-core",
            Self::Standard => "tier-standard",
            Self::Options => "tier-options",
            Self::SecBasic => "tier-sec-basic",
        }
    }
}

/// One tier row: what checking the tier claims, and how much of the catalogue
/// that claim reaches.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TierRow {
    /// The tier.
    pub tier: ScopeTier,
    /// The capabilities the matrix puts in the tier's member set.
    pub capabilities: u64,
    /// The distinct catalogue cases those capabilities gate.
    pub cases: u64,
}

/// The probe's verbatim answer.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ProbeAnswer {
    /// The server answered; the fields are its own words.
    Answered {
        /// The status line, verbatim.
        status: String,
        /// Round-trip time in milliseconds.
        elapsed_ms: u64,
        /// Whether the status was 2xx (what seed-gates Continue).
        ok: bool,
    },
    /// The connection itself failed; the field is the transport's own words.
    Unreachable {
        /// The error, verbatim.
        error: String,
    },
}

/// One pickable party statement.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StatementRow {
    /// The path under the party tree (`<dir>/statement.json`).
    pub path: String,
    /// The declared product name and version.
    pub product: String,
}

/// The honest scope preview: what a run over this scope will PROCESS.
///
/// Every case in filter scope lands as an outcome or a recorded exception,
/// and the statement decides how many end excused at drive time.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ScopePreview {
    /// Cases in scope (the count a run processes).
    pub total: u64,
    /// The per-chapter breakdown, chapter-sorted.
    pub chapters: Vec<(String, u64)>,
}

/// What the live screen is looking at (#386).
///
/// Four honest answers, so a reader can tell "this run is executing here"
/// from "this instance never heard of that run". A console instance answers
/// only about itself: it drives a run in its own memory, or it reads the
/// run's own directory under the mounted output tree, or it knows nothing.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum RunScreen {
    /// This process is driving the run: stream it.
    Live(Box<crate::run_job::JobView>),
    /// The run is not in this process's memory, and its artifacts answer
    /// instead.
    Recorded(Box<RecordedRun>),
    /// A run was named and this instance knows nothing of it.
    Unknown(crate::run_job::RunId),
    /// No run was named, and nothing is in flight here.
    NoRunNamed,
}

/// What a start request answered (#389).
///
/// The per-submitter refusal travels as an ANSWER rather than as a transport
/// error, because it carries the run the visitor already has and the screen's
/// whole job is to link them to it. The typed refusal at the boundary that
/// branches is `run_job::JobError::Busy`; this is how it reaches a browser.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum StartOutcome {
    /// The run was accepted under this id, driving or queued. Its own live
    /// screen states which.
    Accepted(crate::run_job::RunId),
    /// This submitter already has a run in flight, so nothing was started;
    /// the field is the run they already have.
    AlreadyInFlight(crate::run_job::RunId),
}

/// A run read back from its own directory, never from this process's memory.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RecordedRun {
    /// The run this directory belongs to.
    pub id: crate::run_job::RunId,
    /// The artifacts directory, as the screen shows it.
    pub dir: String,
    /// What the results document says; `None` when the directory holds none.
    pub results: Option<RecordedResults>,
}

/// The tally a recorded run's results document carries.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RecordedResults {
    /// The SUT display name the record itself names.
    pub sut_name: String,
    /// Passed case records.
    pub passed: u64,
    /// Failed case records.
    pub failed: u64,
    /// Errored case records.
    pub errored: u64,
    /// Not-applicable case records.
    pub not_applicable: u64,
    /// Where the results document sits.
    pub results_path: String,
}

#[cfg(feature = "ssr")]
pub mod read {
    //! The component-free ssr readers and writers behind the endpoints.

    use super::{
        ClaimSummary, DraftView, RecordedResults, RecordedRun, RunDraft, RunScreen, ScopePreview,
        ScopeTier, StartOutcome, StatementRow, TierRow,
    };
    use crate::run_job::{Latest, RunId};
    use crate::state::ConsoleState;
    use crate::submitter::Submitter;

    /// The pasted-claim size cap: far above any real ICS, far below abuse.
    const STATEMENT_CAP_BYTES: usize = 1_048_576;

    /// The refusal every step past Connect gives when no draft exists.
    const NO_DRAFT: &str = "no connection draft: complete the Connect step first";

    /// NOTE: no openEHR spec governs this — our own design; usize → u64 is
    /// lossless on every supported target (see `catalogue_api::read::count`).
    fn count(n: usize) -> u64 {
        u64::try_from(n).unwrap_or(u64::MAX)
    }

    /// The client-safe view of this submitter's draft.
    #[must_use]
    pub fn draft_view(state: &ConsoleState, submitter: Submitter) -> Option<DraftView> {
        let guard = state.draft.lock().ok()?;
        guard.get(submitter).map(|draft| DraftView {
            base_url: draft.base_url.clone(),
            sut_name: draft.sut_name.clone(),
            sut_version: draft.sut_version.clone(),
            auth: draft.auth.token().to_owned(),
            probed_ok: draft.probed_ok,
            statement: draft.statement_product.clone(),
            filter: draft.filter.clone(),
            record_exchanges: draft.record_exchanges,
            postures: draft.postures.summary(),
        })
    }

    /// Stores this submitter's connection draft, replacing any prior one.
    ///
    /// Another visitor's draft is untouched: the map is keyed by submitter,
    /// so two people composing a connection at once do not overwrite each
    /// other.
    ///
    /// # Errors
    /// The poisoned-lock diagnostic, verbatim.
    pub fn save_connection(
        state: &ConsoleState,
        submitter: Submitter,
        draft: RunDraft,
    ) -> Result<(), String> {
        let mut guard = state.draft.lock().map_err(|e| e.to_string())?;
        guard.insert(submitter, draft);
        Ok(())
    }

    /// Validates and stores the scope half onto the existing draft.
    ///
    /// The pasted claim is UNTRUSTED input on a public endpoint: size-capped,
    /// parsed through the published lib's own statement type, and refused
    /// with the reader's finding verbatim — never stored unvalidated.
    ///
    /// # Errors
    /// "no connection draft" when S3 has not run, the size cap, the
    /// statement reader's finding verbatim, or the poisoned-lock diagnostic.
    #[expect(
        clippy::disallowed_types,
        reason = "the artifact-loader family: schema validation runs over the raw JSON value before the typed parse, exactly as the engine's own loaders do"
    )]
    pub fn save_scope(
        state: &ConsoleState,
        submitter: Submitter,
        statement_json: Option<String>,
        filter: Option<String>,
        record_exchanges: bool,
        postures: &super::PostureForm,
    ) -> Result<Option<ClaimSummary>, String> {
        let summary = statement_json
            .as_deref()
            .map(|body| {
                if body.len() > STATEMENT_CAP_BYTES {
                    return Err(format!(
                        "the statement is {} bytes; the cap is {STATEMENT_CAP_BYTES}",
                        body.len()
                    ));
                }
                let value: serde_json::Value = serde_json::from_str(body)
                    .map_err(|e| format!("the statement is not JSON: {e}"))?;
                // Held to the PUBLISHED statement schema — the same document
                // the engine emits and loads by — before the typed parse.
                let validator = jsonschema::validator_for(&veredictum::schema::statement_schema())
                    .map_err(|e| format!("the statement schema itself failed to compile: {e}"))?;
                if let Some(finding) = validator.iter_errors(&value).next() {
                    return Err(format!(
                        "the statement fails its published schema at {}: {finding}",
                        finding.instance_path()
                    ));
                }
                let statement: veredictum::party::Statement = serde_json::from_value(value)
                    .map_err(|e| format!("the statement does not parse: {e}"))?;
                Ok(ClaimSummary {
                    product: format!("{} {}", statement.product.name, statement.product.version),
                    profiles: statement
                        .claims
                        .profiles
                        .iter()
                        .map(crate::engine::token)
                        .collect::<Result<Vec<String>, serde_json::Error>>()
                        .map_err(|e| format!("a claimed profile tier did not render: {e}"))?,
                    capabilities: count(statement.claims.capabilities.len()),
                })
            })
            .transpose()?;
        // Narrowed BEFORE the draft is touched, so a refused declaration
        // leaves the stored scope exactly as it was.
        let declared = super::declared_postures(postures)?;
        let mut guard = state.draft.lock().map_err(|e| e.to_string())?;
        let draft = guard
            .get_mut(submitter)
            .ok_or_else(|| String::from(NO_DRAFT))?;
        draft.statement_product = summary.as_ref().map(|claim| claim.product.clone());
        draft.statement_json = statement_json;
        draft.filter = filter;
        draft.record_exchanges = record_exchanges;
        draft.postures = declared;
        Ok(summary)
    }

    /// One committed statement's body, for the example fillers.
    ///
    /// The path is untrusted client input: it must canonicalize to a
    /// `statement.json` under the mounted party tree, or it is refused.
    ///
    /// # Errors
    /// The refusal above, or the filesystem's verbatim failure.
    pub fn statement_body(state: &ConsoleState, path: &str) -> Result<String, String> {
        let candidate = std::path::Path::new(path)
            .canonicalize()
            .map_err(|e| format!("{path}: {e}"))?;
        let party = state
            .party
            .canonicalize()
            .map_err(|e| format!("{}: {e}", state.party.display()))?;
        if !candidate.starts_with(&party)
            || candidate.file_name() != Some(std::ffi::OsStr::new("statement.json"))
        {
            return Err(String::from(
                "refused: only a statement.json under the mounted party tree loads as an example",
            ));
        }
        std::fs::read_to_string(&candidate).map_err(|e| format!("{}: {e}", candidate.display()))
    }

    /// The pickable statements: every `*/statement.json` under the party
    /// tree, path-sorted, with the product identity read through the
    /// published lib.
    ///
    /// # Errors
    /// The verbatim read failure when the party tree cannot be listed.
    pub fn statement_rows(state: &ConsoleState) -> Result<Vec<StatementRow>, String> {
        Ok(committed_statements(state)?
            .into_iter()
            .map(|(path, statement)| StatementRow {
                path: path.display().to_string(),
                product: format!("{} {}", statement.product.name, statement.product.version),
            })
            .collect())
    }

    /// The scope preview over the loaded catalogue.
    ///
    /// The filter-scoped case set IS what a run processes, so the count is
    /// honest without re-implementing the drive-time selection.
    ///
    /// # Errors
    /// The verbatim load failure when the catalogue is absent.
    pub fn scope_preview(state: &ConsoleState, filter: &str) -> Result<ScopePreview, String> {
        let validation = state.catalogue.as_ref().as_ref().map_err(Clone::clone)?;
        let mut chapters: std::collections::BTreeMap<String, u64> =
            std::collections::BTreeMap::new();
        let mut total = 0_usize;
        for (path, case) in &validation.loaded.set.cases {
            if !filter.is_empty() && !case.id.to_string().contains(filter) {
                continue;
            }
            total += 1;
            let chapter = crate::catalogue_api::read::chapter_of(path);
            *chapters.entry(chapter).or_insert(0) += 1;
        }
        Ok(ScopePreview {
            total: count(total),
            chapters: chapters.into_iter().collect(),
        })
    }

    /// The lib's tier for one offerable tier.
    fn lib_tier(tier: ScopeTier) -> veredictum::vocab::Tier {
        match tier {
            ScopeTier::Core => veredictum::vocab::Tier::Core,
            ScopeTier::Standard => veredictum::vocab::Tier::Standard,
            ScopeTier::Options => veredictum::vocab::Tier::Options,
            ScopeTier::SecBasic => veredictum::vocab::Tier::SecBasic,
        }
    }

    /// The loaded capability matrix.
    fn capability_matrix(
        validation: &veredictum::pipeline::catalogue::Validation,
    ) -> Result<&veredictum::model::capability::CapabilityMatrix, String> {
        validation
            .loaded
            .set
            .matrix
            .as_ref()
            .map(|(_, matrix)| matrix)
            .ok_or_else(|| {
                String::from(
                    "the mounted catalogue carries no capability matrix, so no tier resolves to capabilities",
                )
            })
    }

    /// The tier row: each tier's member capabilities and the distinct cases
    /// they gate.
    ///
    /// The member set is the published lib's own `tier_members` walk, the one
    /// the judgement computes each profile verdict from, so the count cannot
    /// drift from the verdict's own answer.
    ///
    /// # Errors
    /// The catalogue's verbatim load failure, or the absent capability matrix.
    pub fn tier_rows(state: &ConsoleState) -> Result<Vec<TierRow>, String> {
        let validation = state.catalogue.as_ref().as_ref().map_err(Clone::clone)?;
        let matrix = capability_matrix(validation)?;
        let mut rows = Vec::with_capacity(ScopeTier::ALL.len());
        for tier in ScopeTier::ALL {
            let members = veredictum::verdict::tier_members(lib_tier(tier), matrix);
            let mut cases: std::collections::BTreeSet<&str> = std::collections::BTreeSet::new();
            for (_, case) in &validation.loaded.set.cases {
                if case.capabilities.iter().any(|cap| members.contains(cap)) {
                    cases.insert(case.id.as_str());
                }
            }
            rows.push(TierRow {
                tier,
                capabilities: count(members.len()),
                cases: count(cases.len()),
            });
        }
        Ok(rows)
    }

    /// Every committed statement under the mounted party tree, path-sorted
    /// and parsed through the published lib.
    ///
    /// # Errors
    /// The verbatim read or parse failure.
    fn committed_statements(
        state: &ConsoleState,
    ) -> Result<Vec<(std::path::PathBuf, veredictum::party::Statement)>, String> {
        let mut found = Vec::new();
        let entries = std::fs::read_dir(&state.party)
            .map_err(|e| format!("{}: {e}", state.party.display()))?;
        for entry in entries {
            let entry = entry.map_err(|e| e.to_string())?;
            let candidate = entry.path().join("statement.json");
            if !candidate.is_file() {
                continue;
            }
            let body = std::fs::read_to_string(&candidate)
                .map_err(|e| format!("{}: {e}", candidate.display()))?;
            let statement: veredictum::party::Statement =
                serde_json::from_str(&body).map_err(|e| format!("{}: {e}", candidate.display()))?;
            found.push((candidate, statement));
        }
        found.sort_by(|a, b| a.0.cmp(&b.0));
        Ok(found)
    }

    /// The schedule release a composed claim targets.
    ///
    /// The console never invents one: the committed statements under the
    /// mounted party tree declare it, and they must agree.
    ///
    /// # Errors
    /// The party tree's verbatim read failure, an empty party tree, or
    /// committed statements that disagree.
    fn schedule_release(state: &ConsoleState) -> Result<String, String> {
        let declared: std::collections::BTreeSet<String> = committed_statements(state)?
            .into_iter()
            .map(|(_, statement)| statement.schedule_release)
            .collect();
        let mut declared = declared.into_iter();
        match (declared.next(), declared.next()) {
            (Some(release), None) => Ok(release),
            (Some(first), Some(second)) => Err(format!(
                "the committed statements declare different schedule releases ({first} and {second}), so the console cannot pick the one a composed claim targets"
            )),
            _ => Err(format!(
                "{}: no committed statement declares the schedule release a composed claim targets",
                state.party.display()
            )),
        }
    }

    /// The spec-component versions a composed claim declares.
    ///
    /// An undeclared version fails the `applies` filter by design, putting
    /// every case gated on that component out of scope. The declaration is
    /// therefore derived from the catalogue: the highest floor its own case
    /// and operation ranges name, which is the release at which all of them
    /// are in scope. The operator may edit it down before saving.
    ///
    /// # Errors
    /// When the derived declaration does not satisfy some range after all
    /// (an upper-bounded range), naming the artifact — a silent narrowing is
    /// never an acceptable answer.
    fn catalogue_spec_versions(
        validation: &veredictum::pipeline::catalogue::Validation,
    ) -> Result<veredictum::party::SpecVersions, String> {
        use veredictum::vocab::SpecComponent;

        let set = &validation.loaded.set;
        let mut floors: std::collections::BTreeMap<SpecComponent, (u64, u64, u64)> =
            std::collections::BTreeMap::new();
        let mut raise = |applies: &veredictum::model::case::Applies| {
            for (component, range) in applies.entries() {
                for comparator in &range.req().comparators {
                    let candidate = (
                        comparator.major,
                        comparator.minor.unwrap_or(0),
                        comparator.patch.unwrap_or(0),
                    );
                    let floor = floors.entry(component).or_insert((0, 0, 0));
                    if candidate > *floor {
                        *floor = candidate;
                    }
                }
            }
        };
        for (_, case) in &set.cases {
            raise(&case.applies);
        }
        for (_, binding) in &set.bindings {
            if let Some(applies) = &binding.applies {
                raise(applies);
            }
        }

        let mut versions = veredictum::party::SpecVersions::default();
        for (component, (major, minor, patch)) in &floors {
            let text = format!("{major}.{minor}.{patch}");
            match component {
                SpecComponent::Rm => versions.rm = Some(text),
                SpecComponent::Base => versions.base = Some(text),
                SpecComponent::Am => versions.am = Some(text),
                SpecComponent::Aql => versions.aql = Some(text),
                SpecComponent::ItsRest => versions.its_rest = Some(text),
                SpecComponent::Term => versions.term = Some(text),
            }
        }

        let unsatisfied = set
            .cases
            .iter()
            .map(|(path, case)| (path, Some(&case.applies)))
            .chain(
                set.bindings
                    .iter()
                    .map(|(path, binding)| (path, binding.applies.as_ref())),
            )
            .find(|(_, applies)| applies.is_some_and(|a| !a.satisfied_by(&versions)));
        if let Some((path, _)) = unsatisfied {
            return Err(format!(
                "{}: no single spec-version declaration satisfies every range this catalogue names, so a composed claim cannot be derived from it",
                path.display()
            ));
        }
        Ok(versions)
    }

    /// The composed claim's product identifier, derived from the identity the
    /// Connect step collected.
    fn identifier_of(name: &str, version: &str) -> String {
        let slug = |text: &str| {
            text.chars()
                .map(|c| {
                    if c.is_ascii_alphanumeric() {
                        c.to_ascii_lowercase()
                    } else {
                        '-'
                    }
                })
                .collect::<String>()
        };
        format!("urn:veredictum:console:{}:{}", slug(name), slug(version))
    }

    /// Composes the ad-hoc claim for a tier selection.
    ///
    /// The product identity is the connection draft's own SUT name and
    /// version; the claimed capabilities are the published lib's member set
    /// for the checked tiers, in capability-matrix order. Nothing is stored:
    /// the answer is a document the operator saves like a pasted one.
    ///
    /// # Errors
    /// An empty selection, a missing connection draft or product identity,
    /// the catalogue's load failure, and the schedule-release and
    /// spec-version derivations above, each verbatim.
    pub fn compose_claim(
        state: &ConsoleState,
        submitter: Submitter,
        tiers: &[ScopeTier],
    ) -> Result<String, String> {
        // Normalized against the vocabulary itself, so a repeated or reordered
        // list from a public endpoint composes the same claim.
        let selected: Vec<ScopeTier> = ScopeTier::ALL
            .into_iter()
            .filter(|tier| tiers.contains(tier))
            .collect();
        if selected.is_empty() {
            return Err(String::from(
                "check at least one tier: a claim with no profile certifies nothing",
            ));
        }
        let validation = state.catalogue.as_ref().as_ref().map_err(Clone::clone)?;
        let matrix = capability_matrix(validation)?;
        let (name, version) = {
            let guard = state.draft.lock().map_err(|e| e.to_string())?;
            let draft = guard.get(submitter).ok_or_else(|| String::from(NO_DRAFT))?;
            (
                draft.sut_name.trim().to_owned(),
                draft.sut_version.trim().to_owned(),
            )
        };
        if name.is_empty() || version.is_empty() {
            return Err(String::from(
                "the composed claim's product identity is the Connect step's display name and version: fill both in first",
            ));
        }
        let members: std::collections::BTreeSet<String> = selected
            .iter()
            .flat_map(|tier| veredictum::verdict::tier_members(lib_tier(*tier), matrix))
            .map(|name| name.to_string())
            .collect();
        let capabilities: Vec<veredictum::ids::CapabilityName> = matrix
            .entries()
            .iter()
            .filter(|(name, _)| members.contains(name.as_str()))
            .map(|(name, _)| name.clone())
            .collect();
        let profiles: Vec<veredictum::vocab::Tier> =
            selected.iter().copied().map(lib_tier).collect();
        let statement = veredictum::party::Statement {
            product: veredictum::party::Product {
                identifier: identifier_of(&name, &version),
                name,
                version,
                vendor: String::from("unknown"),
            },
            schedule_release: schedule_release(state)?,
            spec_versions: catalogue_spec_versions(validation)?,
            claims: veredictum::party::Claims {
                capabilities,
                profiles,
            },
            tech_profiles: Vec::new(),
            options: Vec::new(),
            served_extensions: Vec::new(),
            performance: None,
            non_functional: std::collections::BTreeMap::new(),
            evidence: Vec::new(),
            attestation: None,
        };
        let mut document = serde_json::to_string_pretty(&statement)
            .map_err(|e| format!("the composed claim did not serialize: {e}"))?;
        document.push('\n');
        Ok(document)
    }

    /// One instance of the ixit document: where it is, and how the run
    /// authenticates to it.
    #[derive(Debug, Clone, Copy, serde::Serialize)]
    struct IxitInstance<'d> {
        /// The CDR base URL.
        base_url: &'d str,
        /// The authentication mode, carrying env-var NAMES only.
        auth: IxitAuth,
    }

    /// The authentication modes the console composes — env-var NAMES only, so
    /// a credential VALUE cannot reach a file by construction.
    #[derive(Debug, Clone, Copy, serde::Serialize)]
    #[serde(tag = "mode", rename_all = "snake_case")]
    enum IxitAuth {
        /// No Authorization header at all.
        None,
        /// HTTP Basic, from the named variables.
        Basic {
            /// The variable carrying the user.
            user_env: &'static str,
            /// The variable carrying the password.
            password_env: &'static str,
        },
        /// A static bearer token, from the named variable.
        Bearer {
            /// The variable carrying the token.
            token_env: &'static str,
        },
    }

    /// The version-signing posture as the ixit spells it (RM common
    /// `master06-change_control_package.adoc` §Digital Signature).
    #[derive(Debug, Clone, Copy, serde::Serialize)]
    #[serde(tag = "mode", rename_all = "snake_case")]
    enum IxitSigning<'d> {
        /// Plain digest: the wire form is `<prefix><encoding(hash(bytes))>`.
        Digest {
            /// The hash algorithm applied to the canonical bytes.
            algorithm: &'static str,
            /// How the digest is encoded on the wire.
            encoding: &'static str,
            /// The fixed prefix the wire form carries.
            prefix: &'d str,
        },
        /// openPGP, verified against the declared armored public key.
        Pgp {
            /// The armored public key.
            public_key: &'d str,
        },
    }

    /// The ixit document the console composes for a run: the instances it can
    /// reach, plus exactly the deployment postures the operator declared.
    ///
    /// Every posture is skipped when absent, because an absent key is what
    /// makes the engine record a case not-applicable with its own citation. A
    /// present key with a stand-in value would instead claim something about
    /// somebody else's deployment.
    #[derive(Debug, Clone, serde::Serialize)]
    struct IxitDocument<'d> {
        /// The named instances the flow addresses.
        instances: std::collections::BTreeMap<&'static str, IxitInstance<'d>>,
        /// The deployment's configured system identifier.
        #[serde(skip_serializing_if = "Option::is_none")]
        system_id: Option<&'d str>,
        /// A location on the SUT's own file system for the dump/load
        /// operations.
        #[serde(skip_serializing_if = "Option::is_none")]
        dump_location: Option<&'d str>,
        /// The openEHR generation set.
        #[serde(skip_serializing_if = "Option::is_none")]
        spec_profile: Option<&'static str>,
        /// The version-signing posture.
        #[serde(skip_serializing_if = "Option::is_none")]
        signing: Option<IxitSigning<'d>>,
    }

    /// Renders the draft's ixit document.
    ///
    /// The three instances point at the CDR, each carrying env-var NAMES
    /// only: the values live in the draft and reach the spawned run's
    /// environment alone. Beside them travel exactly the deployment postures
    /// the operator declared (#456) — a fact nobody declared composes no key,
    /// so the engine's selection law records the cases needing it
    /// not-applicable with its own citation instead of being driven under a
    /// guess.
    ///
    /// # Errors
    /// The serializer's verbatim failure, and the engine reader's refusal of a
    /// document the instrument cannot read.
    pub fn ixit_document(draft: &RunDraft) -> Result<String, String> {
        let auth = match draft.auth {
            super::AuthChoice::None => IxitAuth::None,
            super::AuthChoice::Basic => IxitAuth::Basic {
                user_env: "CONSOLE_SUT_USER",
                password_env: "CONSOLE_SUT_PASS",
            },
            super::AuthChoice::Bearer => IxitAuth::Bearer {
                token_env: "CONSOLE_SUT_TOKEN",
            },
        };
        let instance = |auth: IxitAuth| IxitInstance {
            base_url: &draft.base_url,
            auth,
        };
        let mut instances = std::collections::BTreeMap::new();
        instances.insert("sut", instance(auth));
        instances.insert("admin", instance(auth));
        instances.insert("unauthenticated", instance(IxitAuth::None));
        let signing = draft
            .postures
            .signing
            .as_ref()
            .map(|posture| match posture {
                // NOTE: RM common master06 §Digital Signature leaves the hash
                // to the deployment; sha256 is the one the engine's verifier
                // implements, so it is the one this console can declare.
                super::SigningPosture::Digest { encoding, prefix } => IxitSigning::Digest {
                    algorithm: "sha256",
                    encoding: encoding.token(),
                    prefix,
                },
                super::SigningPosture::Pgp { public_key } => IxitSigning::Pgp { public_key },
            });
        let document = IxitDocument {
            instances,
            system_id: draft.postures.system_id.as_deref(),
            dump_location: draft.postures.dump_location.as_deref(),
            spec_profile: draft
                .postures
                .spec_profile
                .map(veredictum::ixit::SpecProfile::token),
            signing,
        };
        let mut rendered = serde_json::to_string_pretty(&document)
            .map_err(|e| format!("the composed ixit did not serialize: {e}"))?;
        rendered.push('\n');
        // The composed bytes are held to the engine's OWN reader before a run
        // is built on them: a document the instrument cannot read is a console
        // defect, and it is refused here rather than at the spawned engine's
        // first line.
        serde_json::from_str::<veredictum::ixit::Ixit>(&rendered)
            .map_err(|e| format!("the composed ixit does not parse: {e}"))?;
        Ok(rendered)
    }

    /// Starts this submitter's drafted run.
    ///
    /// The public start seam: it spends this submitter's start budget and
    /// puts the drafted target through the posture's guard BEFORE the engine
    /// is located or spawned, then hands the drafted spec to
    /// [`start_run_with`].
    ///
    /// # Errors
    /// "no connection draft" before S3, the rate ledger's refusal with its
    /// retry, the target guard's refusal naming the address family, the
    /// engine's own locate/spawn refusals, and the filesystem's verbatim
    /// failures. The per-submitter concurrency refusal is an ANSWER
    /// ([`StartOutcome::AlreadyInFlight`]), not an error, because it names
    /// the run the screen must link to.
    pub async fn start_run(
        state: &ConsoleState,
        submitter: Submitter,
    ) -> Result<StartOutcome, String> {
        // The no-draft refusal precedes engine discovery, so an unconnected
        // wizard is refused for what it is on a host with no engine mounted.
        // The guard's lock is released before the awaits below: nothing here
        // holds it across one.
        let drafted = state
            .draft
            .lock()
            .map_err(|e| e.to_string())?
            .get(submitter)
            .map(|draft| draft.base_url.clone());
        let Some(base_url) = drafted else {
            return Err(String::from(NO_DRAFT));
        };
        state
            .rates
            .admit(submitter, crate::rate_limit::Metered::Start)
            .map_err(|e| e.to_string())?;
        // The target guard (#390), before the engine is spawned: a hosted
        // instance refuses an address only it can reach, resolving the name
        // first. The local posture refuses nothing.
        crate::target_safety::check(state.posture, &base_url)
            .await
            .map_err(|e| e.to_string())?;
        let engine = crate::engine::locate().map_err(|e| e.to_string())?;
        start_run_with(state, submitter, &engine)
    }

    /// Starts the drafted run through an already-located engine.
    ///
    /// Writes the ixit under the job's own output directory, invalidates any
    /// export sealed over that directory, and hands the spec to the job slot.
    /// The credentials MOVE out of the draft into the spawned run's
    /// environment: they reach the child process and nothing else — no file,
    /// no log line, no client-visible state.
    ///
    /// The split exists for the gate, exactly as the export seam's does:
    /// [`start_run`] finds the pinned binary the way the server does, and a
    /// test injects the one it verified itself rather than reaching for
    /// `PATH`.
    ///
    /// # Errors
    /// "no connection draft" before S3, the engine's own spawn refusals, and
    /// the filesystem's verbatim failures. A submitter who already has a run
    /// in flight gets [`StartOutcome::AlreadyInFlight`], which is an answer
    /// naming that run rather than an error.
    pub fn start_run_with(
        state: &ConsoleState,
        submitter: Submitter,
        engine: &crate::engine::Engine,
    ) -> Result<StartOutcome, String> {
        // The pre-flight, so nothing is written for a run that will be
        // refused. The AUTHORITATIVE check is inside the job map's own lock;
        // if a second start from this submitter wins the race in between, its
        // answer still names their run and the cost is a re-probe.
        if let Some(existing) = state
            .jobs
            .in_flight_of(submitter)
            .map_err(|e| e.to_string())?
        {
            return Ok(StartOutcome::AlreadyInFlight(existing));
        }
        let mut guard = state.draft.lock().map_err(|e| e.to_string())?;
        let draft = guard
            .get_mut(submitter)
            .ok_or_else(|| String::from(NO_DRAFT))?;
        let id = state.jobs.allocate_id();
        let out_dir = crate::run_job::job_dir(&state.out, id);
        std::fs::create_dir_all(&out_dir).map_err(|e| format!("{}: {e}", out_dir.display()))?;
        // A run into this directory invalidates any export of it (#68). The
        // seal lives INSIDE the job directory, so the seam that creates a
        // run's directory is what guarantees no bundle is in it.
        crate::export_api::prepare::invalidate(&out_dir)?;
        let ixit_path = out_dir.join("ixit.json");
        std::fs::write(&ixit_path, ixit_document(draft)?)
            .map_err(|e| format!("{}: {e}", ixit_path.display()))?;
        // The claim travels WITH the run: the job directory carries the exact
        // bytes the engine graded, and the verdicts read them back from there,
        // never from the mutable draft.
        let statement_path = draft
            .statement_json
            .as_deref()
            .map(|body| {
                let path = out_dir.join("statement.json");
                std::fs::write(&path, body).map_err(|e| format!("{}: {e}", path.display()))?;
                Ok::<_, String>(path)
            })
            .transpose()?;
        let spec = crate::engine::RunSpec {
            root: state.root.clone(),
            ixit: ixit_path,
            out_dir,
            sut_name: draft.sut_name.clone(),
            sut_version: draft.sut_version.clone(),
            statement: statement_path,
            filter: draft.filter.clone(),
            credentials: std::mem::take(&mut draft.credentials),
            progress: true,
            record_exchanges: draft.record_exchanges,
        };
        let sut_name = draft.sut_name.clone();
        drop(guard);
        match state.jobs.start(id, submitter, engine, spec, sut_name) {
            Ok(accepted) => Ok(StartOutcome::Accepted(accepted)),
            Err(crate::run_job::JobError::Busy(existing)) => {
                Ok(StartOutcome::AlreadyInFlight(existing))
            }
            Err(e) => Err(e.to_string()),
        }
    }

    /// What the live screen is looking at, for the run the URL names.
    ///
    /// A named run this process is driving streams; anything else falls back
    /// to the run's own directory under the mounted output tree, which is the
    /// durable half. `None` — the bare `/run/live` — asks about the run THIS
    /// SUBMITTER most recently started, never about somebody else's, and with
    /// none the answer is [`RunScreen::NoRunNamed`].
    ///
    /// The id is a parsed UUID, so the derived directory stays under the
    /// mounted output root by construction. A run this process never drove is
    /// still readable by id: the artifacts are the durable half, and nothing
    /// here is gated on who started it.
    ///
    /// # Errors
    /// The map's verbatim refusal, and the verbatim read or parse failure of
    /// a results document that exists but cannot be read.
    pub fn run_screen(
        state: &ConsoleState,
        submitter: Submitter,
        id: Option<RunId>,
    ) -> Result<RunScreen, String> {
        let named = match id {
            Some(named) => named,
            None => match state
                .jobs
                .latest_of(submitter, Latest::Any)
                .map_err(|e| e.to_string())?
            {
                Some(current) => current,
                None => return Ok(RunScreen::NoRunNamed),
            },
        };
        if let Some(view) = state.jobs.view_of(named).map_err(|e| e.to_string())? {
            return Ok(RunScreen::Live(Box::new(view)));
        }
        recorded_run(state, named)
    }

    /// The named run as its own directory describes it.
    ///
    /// The results document is parsed through the published lib, so the tally
    /// is the same one a live finished view carries.
    ///
    /// # Errors
    /// The verbatim read or parse failure of a results document that exists.
    fn recorded_run(state: &ConsoleState, id: RunId) -> Result<RunScreen, String> {
        let dir = crate::run_job::job_dir(&state.out, id);
        if !dir.is_dir() {
            return Ok(RunScreen::Unknown(id));
        }
        let results_path = dir.join("results.json");
        if !results_path.is_file() {
            return Ok(RunScreen::Recorded(Box::new(RecordedRun {
                id,
                dir: dir.display().to_string(),
                results: None,
            })));
        }
        let body = std::fs::read_to_string(&results_path)
            .map_err(|e| format!("{}: {e}", results_path.display()))?;
        let results: veredictum::party::Results =
            serde_json::from_str(&body).map_err(|e| format!("{}: {e}", results_path.display()))?;
        let (passed, failed, errored, not_applicable) = crate::run_job::tally(&results);
        Ok(RunScreen::Recorded(Box::new(RecordedRun {
            id,
            dir: dir.display().to_string(),
            results: Some(RecordedResults {
                sut_name: results.sut.name.clone(),
                passed,
                failed,
                errored,
                not_applicable,
                results_path: results_path.display().to_string(),
            }),
        })))
    }

    /// The reachability probe.
    ///
    /// ONE GET of the template list with the supplied credentials, the
    /// answer verbatim. A diagnostic, never a judgement — the one carved-out
    /// console-originated request to a CDR.
    ///
    /// # Errors
    /// Never: an unreachable server is an answer, not an error.
    pub async fn probe(
        base_url: &str,
        auth: super::AuthChoice,
        user: &str,
        password: &str,
        token: &str,
    ) -> super::ProbeAnswer {
        let url = format!(
            "{}/definition/template/adl1.4",
            base_url.trim_end_matches('/')
        );
        let client = match reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(5))
            .build()
        {
            Ok(client) => client,
            Err(e) => {
                return super::ProbeAnswer::Unreachable {
                    error: e.to_string(),
                };
            }
        };
        let mut request = client.get(&url);
        request = match auth {
            super::AuthChoice::None => request,
            super::AuthChoice::Basic => request.basic_auth(user, Some(password)),
            super::AuthChoice::Bearer => request.bearer_auth(token),
        };
        let started = std::time::Instant::now();
        match request.send().await {
            Ok(response) => {
                let elapsed = started.elapsed().as_millis();
                let status = response.status();
                super::ProbeAnswer::Answered {
                    status: format!(
                        "HTTP {} {}",
                        status.as_u16(),
                        status.canonical_reason().unwrap_or("")
                    ),
                    elapsed_ms: u64::try_from(elapsed).unwrap_or(u64::MAX),
                    ok: status.is_success(),
                }
            }
            Err(e) => super::ProbeAnswer::Unreachable {
                error: e.to_string(),
            },
        }
    }
}

pub mod fns {
    //! The `#[server]` endpoints, one module for one inner suppression.
    #![allow(
        clippy::unused_async,
        missing_docs,
        reason = "fires only in some #[server] expansions; see catalogue_api::fns"
    )]

    use leptos::prelude::{ServerFnError, server};

    use super::{
        AuthChoice, ClaimSummary, DraftView, PostureForm, ProbeAnswer, RunScreen, ScopePreview,
        ScopeTier, StartOutcome, StatementRow, TierRow,
    };
    use crate::run_job::RunId;

    /// Probes the connection and, on any answer, stores the draft with these
    /// facts (the probe outcome seed-gates Continue client-side). The secret
    /// values enter the server-side draft and nothing else.
    ///
    /// The probe is the one console-originated request to a CDR, so it is
    /// also one of the two seams a visitor-named target reaches (#390): the
    /// submitter's probe budget is spent and the posture's guard runs BEFORE
    /// the request is built, and a refused target stores no draft at all.
    ///
    /// # Errors
    /// The rate ledger's refusal with its retry, the target guard's refusal
    /// naming the address family, and the draft-store failure — each
    /// verbatim. An unreachable server is an ANSWER, not an error.
    #[server]
    pub async fn probe_and_save(
        base_url: String,
        sut_name: String,
        sut_version: String,
        auth: AuthChoice,
        user: String,
        password: String,
        token: String,
    ) -> Result<ProbeAnswer, ServerFnError> {
        let state: crate::state::ConsoleState = leptos::prelude::expect_context();
        let who = crate::submitter::current(&state);
        state
            .rates
            .admit(who, crate::rate_limit::Metered::Probe)
            .map_err(ServerFnError::new)?;
        crate::target_safety::check(state.posture, &base_url)
            .await
            .map_err(ServerFnError::new)?;
        let answer = super::read::probe(&base_url, auth, &user, &password, &token).await;
        let probed_ok = matches!(answer, ProbeAnswer::Answered { ok: true, .. });
        let mut credentials = Vec::new();
        match auth {
            AuthChoice::None => {}
            AuthChoice::Basic => {
                credentials.push(crate::engine::Credential {
                    name: String::from("CONSOLE_SUT_USER"),
                    value: crate::engine::Secret::new(user),
                });
                credentials.push(crate::engine::Credential {
                    name: String::from("CONSOLE_SUT_PASS"),
                    value: crate::engine::Secret::new(password),
                });
            }
            AuthChoice::Bearer => {
                credentials.push(crate::engine::Credential {
                    name: String::from("CONSOLE_SUT_TOKEN"),
                    value: crate::engine::Secret::new(token),
                });
            }
        }
        super::read::save_connection(
            &state,
            who,
            super::RunDraft {
                base_url,
                sut_name,
                sut_version,
                auth,
                credentials,
                probed_ok,
                statement_json: None,
                statement_product: None,
                filter: None,
                record_exchanges: false,
                postures: super::DeclaredPostures::default(),
            },
        )
        .map_err(ServerFnError::new)?;
        Ok(answer)
    }

    /// The client-safe draft this submitter has, when they have one.
    ///
    /// # Errors
    /// The server-fn transport only.
    #[server]
    pub async fn fetch_draft() -> Result<Option<DraftView>, ServerFnError> {
        let state: crate::state::ConsoleState = leptos::prelude::expect_context();
        let who = crate::submitter::current(&state);
        Ok(super::read::draft_view(&state, who))
    }

    /// The pickable party statements.
    ///
    /// # Errors
    /// The verbatim read failure when the party tree cannot be listed.
    #[server]
    pub async fn fetch_statements() -> Result<Vec<StatementRow>, ServerFnError> {
        let state: crate::state::ConsoleState = leptos::prelude::expect_context();
        super::read::statement_rows(&state).map_err(ServerFnError::new)
    }

    /// The scope preview for a filter.
    ///
    /// # Errors
    /// The verbatim load failure when the catalogue is absent.
    #[server]
    pub async fn fetch_scope_preview(filter: String) -> Result<ScopePreview, ServerFnError> {
        let state: crate::state::ConsoleState = leptos::prelude::expect_context();
        super::read::scope_preview(&state, &filter).map_err(ServerFnError::new)
    }

    /// Starts this submitter's drafted run and answers with the run's id,
    /// which is the address `/run/live/{run_id}` carries.
    ///
    /// A submitter who already has a run in flight is answered with THAT
    /// run's id and nothing is started, so the screen can send them to it.
    ///
    /// # Errors
    /// "no connection draft" before S3, the start budget's refusal, the
    /// target guard's refusal, the engine's refusals, and filesystem
    /// failures — each verbatim.
    #[server]
    pub async fn start_run() -> Result<StartOutcome, ServerFnError> {
        let state: crate::state::ConsoleState = leptos::prelude::expect_context();
        let who = crate::submitter::current(&state);
        super::read::start_run(&state, who)
            .await
            .map_err(ServerFnError::new)
    }

    /// What the live screen is looking at, for the run the URL names.
    ///
    /// `None` asks about the run THIS SUBMITTER most recently started. The id
    /// is untrusted input, and a parsed UUID keeps the derived directory
    /// under the mounted output root.
    ///
    /// # Errors
    /// The map's poisoned-state diagnostic, or a results document that
    /// exists and cannot be read.
    #[server]
    pub async fn fetch_run(id: Option<RunId>) -> Result<RunScreen, ServerFnError> {
        let state: crate::state::ConsoleState = leptos::prelude::expect_context();
        let who = crate::submitter::current(&state);
        let screen = super::read::run_screen(&state, who, id).map_err(ServerFnError::new)?;
        Ok(crate::capture::run_screen(&state, screen))
    }

    /// Cancels the NAMED run.
    ///
    /// # Errors
    /// "no run is in flight" when this process is not driving that run, or
    /// the kill's own failure.
    #[server]
    pub async fn cancel_run(id: RunId) -> Result<(), ServerFnError> {
        let state: crate::state::ConsoleState = leptos::prelude::expect_context();
        state.jobs.cancel(id).map_err(ServerFnError::new)
    }

    /// Validates and stores the scope half onto the draft, answering with
    /// the claim's summary (`None` for an honest no-claim run).
    ///
    /// # Errors
    /// "no connection draft" when S3 has not run, the statement reader's
    /// refusal verbatim, or the draft-store failure.
    #[server]
    pub async fn save_scope(
        statement_json: Option<String>,
        filter: Option<String>,
        record_exchanges: bool,
        postures: PostureForm,
    ) -> Result<Option<ClaimSummary>, ServerFnError> {
        let state: crate::state::ConsoleState = leptos::prelude::expect_context();
        let who = crate::submitter::current(&state);
        super::read::save_scope(
            &state,
            who,
            statement_json.filter(|s| !s.trim().is_empty()),
            filter.filter(|f| !f.is_empty()),
            record_exchanges,
            &postures,
        )
        .map_err(ServerFnError::new)
    }

    /// The four tiers with the capabilities they claim and the cases those
    /// gate.
    ///
    /// # Errors
    /// The catalogue's verbatim load failure, or the absent capability
    /// matrix.
    #[server]
    pub async fn fetch_tier_counts() -> Result<Vec<TierRow>, ServerFnError> {
        let state: crate::state::ConsoleState = leptos::prelude::expect_context();
        super::read::tier_rows(&state).map_err(ServerFnError::new)
    }

    /// Composes the ad-hoc claim for a tier selection, answering with the
    /// statement document itself.
    ///
    /// The tier list is a closed vocabulary, so an unknown token from a public
    /// endpoint never decodes. Nothing is stored: the operator saves the
    /// document through the same schema-validated path a pasted one takes.
    ///
    /// The argument is optional because the default URL-encoded server-fn
    /// encoding carries an empty sequence as an ABSENT field
    /// (<https://book.leptos.dev/server/25_server_functions.html>), and an
    /// empty selection has to reach the composer's own refusal rather than a
    /// deserialization error.
    ///
    /// # Errors
    /// An empty selection, a missing connection draft, and the composer's
    /// catalogue failures, each verbatim.
    #[server]
    pub async fn compose_claim(tiers: Option<Vec<ScopeTier>>) -> Result<String, ServerFnError> {
        let state: crate::state::ConsoleState = leptos::prelude::expect_context();
        let who = crate::submitter::current(&state);
        super::read::compose_claim(&state, who, &tiers.unwrap_or_default())
            .map_err(ServerFnError::new)
    }

    /// One committed statement's body, for the example fillers.
    ///
    /// # Errors
    /// The path refusal (only a statement.json under the party tree loads),
    /// or the filesystem's verbatim failure.
    #[server]
    pub async fn fetch_statement_body(path: String) -> Result<String, ServerFnError> {
        let state: crate::state::ConsoleState = leptos::prelude::expect_context();
        super::read::statement_body(&state, &path).map_err(ServerFnError::new)
    }
}
