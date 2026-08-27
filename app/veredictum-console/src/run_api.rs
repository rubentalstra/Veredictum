// SPDX-FileCopyrightText: Veredictum contributors
// SPDX-License-Identifier: Apache-2.0

//! The run wizard's server seam for S3 Connect and S4 Scope (#65).
//!
//! The wizard's memory is one server-side draft: the connection facts, the
//! credential VALUES (memory only — never persisted, never rendered back,
//! never logged), the statement pick and the filter. What the client can read
//! back is [`DraftView`], which carries no secret by construction.
//!
//! The reachability probe is the ONE console-originated request to a CDR, and
//! it is carved out deliberately: a diagnostic whose answer is rendered
//! verbatim, never judged — conformance traffic stays the spawned instrument's
//! alone (#54).

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

#[cfg(feature = "ssr")]
pub mod read {
    //! The component-free ssr readers and writers behind the endpoints.

    use super::{
        ClaimSummary, DraftView, RunDraft, ScopePreview, ScopeTier, StatementRow, TierRow,
    };
    use crate::state::ConsoleState;

    /// The pasted-claim size cap: far above any real ICS, far below abuse.
    const STATEMENT_CAP_BYTES: usize = 1_048_576;

    /// NOTE: no openEHR spec governs this — our own design; usize → u64 is
    /// lossless on every supported target (see `catalogue_api::read::count`).
    fn count(n: usize) -> u64 {
        u64::try_from(n).unwrap_or(u64::MAX)
    }

    /// The client-safe view of the draft.
    #[must_use]
    pub fn draft_view(state: &ConsoleState) -> Option<DraftView> {
        let guard = state.draft.lock().ok()?;
        guard.as_ref().map(|draft| DraftView {
            base_url: draft.base_url.clone(),
            sut_name: draft.sut_name.clone(),
            sut_version: draft.sut_version.clone(),
            auth: draft.auth.token().to_owned(),
            probed_ok: draft.probed_ok,
            statement: draft.statement_product.clone(),
            filter: draft.filter.clone(),
            record_exchanges: draft.record_exchanges,
        })
    }

    /// Stores the connection half of the draft, replacing any prior one.
    ///
    /// # Errors
    /// The poisoned-lock diagnostic, verbatim.
    pub fn save_connection(state: &ConsoleState, draft: RunDraft) -> Result<(), String> {
        let mut guard = state.draft.lock().map_err(|e| e.to_string())?;
        *guard = Some(draft);
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
        statement_json: Option<String>,
        filter: Option<String>,
        record_exchanges: bool,
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
        let mut guard = state.draft.lock().map_err(|e| e.to_string())?;
        let draft = guard
            .as_mut()
            .ok_or_else(|| String::from("no connection draft: complete the Connect step first"))?;
        draft.statement_product = summary.as_ref().map(|claim| claim.product.clone());
        draft.statement_json = statement_json;
        draft.filter = filter;
        draft.record_exchanges = record_exchanges;
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
    /// The filter-scoped case set IS what a run processes (each case lands
    /// as an outcome or a recorded exception), so this count is honest
    /// without re-implementing the drive-time selection — the integration
    /// test holds it to a real run.
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

    /// The lib's tier for one offerable tier — the single mapping every walk
    /// here goes through.
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
    /// The member set is the published lib's own `tier_members` walk, which
    /// is what the judgement computes each profile verdict from, so the count
    /// cannot drift from the answer the verdict gives.
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
    /// A statement that declares no version for a component puts every case
    /// gated on it OUT of scope, because an undeclared version fails the
    /// `applies` filter by design, and this catalogue dates nearly every case
    /// to a Reference Model floor. The declaration is therefore derived from
    /// the catalogue itself: the highest floor its own case and operation
    /// ranges name, which is the release at which every one of them is in
    /// scope. The operator sees it in the composed document and may edit it
    /// down before saving.
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
    /// version; the claimed capabilities are exactly the published lib's
    /// member set for the checked tiers, in capability-matrix order. The
    /// answer is a statement document the operator reads and saves like a
    /// pasted one — nothing is stored here.
    ///
    /// # Errors
    /// An empty selection, a missing connection draft or product identity,
    /// the catalogue's load failure, and the schedule-release and
    /// spec-version derivations above, each verbatim.
    pub fn compose_claim(state: &ConsoleState, tiers: &[ScopeTier]) -> Result<String, String> {
        // The selection is normalized against the vocabulary itself, so a
        // repeated or reordered list from a public endpoint composes the same
        // claim as the row that sent it.
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
            let draft = guard.as_ref().ok_or_else(|| {
                String::from("no connection draft: complete the Connect step first")
            })?;
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

    /// Renders the draft's ixit document.
    ///
    /// The three instances point at the CDR, each carrying env-var NAMES
    /// only — the values live in the draft and reach the spawned run's
    /// environment alone. The secrecy test pins it.
    #[must_use]
    pub fn ixit_document(draft: &RunDraft) -> String {
        let auth = match draft.auth {
            super::AuthChoice::None => String::from(r#"{ "mode": "none" }"#),
            super::AuthChoice::Basic => String::from(
                r#"{ "mode": "basic", "user_env": "CONSOLE_SUT_USER", "password_env": "CONSOLE_SUT_PASS" }"#,
            ),
            super::AuthChoice::Bearer => {
                String::from(r#"{ "mode": "bearer", "token_env": "CONSOLE_SUT_TOKEN" }"#)
            }
        };
        let base = &draft.base_url;
        format!(
            r#"{{
  "instances": {{
    "sut": {{ "base_url": "{base}", "auth": {auth} }},
    "admin": {{ "base_url": "{base}", "auth": {auth} }},
    "unauthenticated": {{ "base_url": "{base}", "auth": {{ "mode": "none" }} }}
  }}
}}
"#
        )
    }

    /// Starts the drafted run.
    ///
    /// Writes the ixit under the job's own output directory, locates and
    /// verifies the engine, and hands the spec to the job slot. The
    /// credentials MOVE out of the draft into the spawned run's environment.
    ///
    /// # Errors
    /// "no connection draft" before S3, the engine's own locate/spawn
    /// refusals, the slot's busy refusal, and the filesystem's verbatim
    /// failures.
    pub fn start_run(state: &ConsoleState) -> Result<u64, String> {
        let mut guard = state.draft.lock().map_err(|e| e.to_string())?;
        let draft = guard
            .as_mut()
            .ok_or_else(|| String::from("no connection draft: complete the Connect step first"))?;
        let id = state.jobs.allocate_id().map_err(|e| e.to_string())?;
        let out_dir = crate::run_job::job_dir(&state.out, id);
        std::fs::create_dir_all(&out_dir).map_err(|e| format!("{}: {e}", out_dir.display()))?;
        // A run into this directory invalidates any export of it (#68). The
        // job counter restarts with the console process while the output
        // mount persists, so a fresh run CAN land on an older run's
        // directory — and a sealed bundle left there certifies the documents
        // of the run before it. Leaving it would let the export surface
        // present one run's signature as another run's record.
        crate::export_api::prepare::invalidate(&out_dir)?;
        let ixit_path = out_dir.join("ixit.json");
        std::fs::write(&ixit_path, ixit_document(draft))
            .map_err(|e| format!("{}: {e}", ixit_path.display()))?;
        // The claim travels WITH the run: the job directory carries the
        // exact bytes the engine graded, and the verdicts read them back
        // from there — never from the mutable draft.
        let statement_path = draft
            .statement_json
            .as_deref()
            .map(|body| {
                let path = out_dir.join("statement.json");
                std::fs::write(&path, body).map_err(|e| format!("{}: {e}", path.display()))?;
                Ok::<_, String>(path)
            })
            .transpose()?;
        let engine = crate::engine::locate().map_err(|e| e.to_string())?;
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
        state
            .jobs
            .start(id, &engine, &spec, sut_name)
            .map_err(|e| e.to_string())
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

    use super::{
        AuthChoice, ClaimSummary, DraftView, ProbeAnswer, ScopePreview, ScopeTier, StatementRow,
        TierRow,
    };

    /// Probes the connection and, on any answer, stores the draft with these
    /// facts (the probe outcome seed-gates Continue client-side). The secret
    /// values enter the server-side draft and nothing else.
    ///
    /// # Errors
    /// The draft-store failure, verbatim; an unreachable server is an
    /// ANSWER, not an error.
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
            },
        )
        .map_err(ServerFnError::new)?;
        Ok(answer)
    }

    /// The client-safe draft, when one exists.
    ///
    /// # Errors
    /// The server-fn transport only.
    #[server]
    pub async fn fetch_draft() -> Result<Option<DraftView>, ServerFnError> {
        let state: crate::state::ConsoleState = leptos::prelude::expect_context();
        Ok(super::read::draft_view(&state))
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

    /// Starts the drafted run and answers with the job id.
    ///
    /// # Errors
    /// "no connection draft" before S3, the engine's refusals, the busy
    /// slot, and filesystem failures — each verbatim.
    #[server]
    pub async fn start_run() -> Result<u64, ServerFnError> {
        let state: crate::state::ConsoleState = leptos::prelude::expect_context();
        super::read::start_run(&state).map_err(ServerFnError::new)
    }

    /// The live job view, when a job exists.
    ///
    /// # Errors
    /// The slot's poisoned-state diagnostic only.
    #[server]
    pub async fn fetch_job() -> Result<Option<crate::run_job::JobView>, ServerFnError> {
        let state: crate::state::ConsoleState = leptos::prelude::expect_context();
        state.jobs.view().map_err(ServerFnError::new)
    }

    /// Cancels the in-flight run.
    ///
    /// # Errors
    /// "no run is in flight", or the kill's own failure.
    #[server]
    pub async fn cancel_run() -> Result<(), ServerFnError> {
        let state: crate::state::ConsoleState = leptos::prelude::expect_context();
        state.jobs.cancel().map_err(ServerFnError::new)
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
    ) -> Result<Option<ClaimSummary>, ServerFnError> {
        let state: crate::state::ConsoleState = leptos::prelude::expect_context();
        super::read::save_scope(
            &state,
            statement_json.filter(|s| !s.trim().is_empty()),
            filter.filter(|f| !f.is_empty()),
            record_exchanges,
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
    /// The tier list is untrusted input on a public endpoint: it is a closed
    /// vocabulary, so an unknown token never decodes, and duplicates collapse
    /// into the same claim. Nothing is stored — the operator saves the
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
        super::read::compose_claim(&state, &tiers.unwrap_or_default()).map_err(ServerFnError::new)
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
