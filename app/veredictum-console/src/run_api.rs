// SPDX-FileCopyrightText: Veredictum contributors
// SPDX-License-Identifier: Apache-2.0

//! The run wizard's server seam for S3 Connect and S4 Scope (#65).
//!
//! The wizard's memory is one server-side draft: the connection facts, the
//! credential VALUES (memory only — never persisted, never rendered back,
//! never logged), the statement pick and the filter. What the client can read
//! back is [`DraftView`], which carries no secret by construction.
//!
//! The reachability probe is the ONE console-originated request to a CDR,
//! carved out explicitly in the crate CLAUDE.md: a diagnostic whose answer is
//! rendered verbatim, never judged — conformance traffic stays the spawned
//! instrument's alone (#54).

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

    use super::{ClaimSummary, DraftView, RunDraft, ScopePreview, StatementRow};
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
                        .map(|tier| {
                            serde_json::to_string(tier)
                                .unwrap_or_default()
                                .trim_matches('"')
                                .to_owned()
                        })
                        .collect(),
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
        let mut rows = Vec::new();
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
            rows.push(StatementRow {
                path: candidate.display().to_string(),
                product: format!("{} {}", statement.product.name, statement.product.version),
            });
        }
        rows.sort_by(|a, b| a.path.cmp(&b.path));
        Ok(rows)
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
        let out_dir = state.out.join(format!("console-job-{id}"));
        std::fs::create_dir_all(&out_dir).map_err(|e| format!("{}: {e}", out_dir.display()))?;
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
    /// answer verbatim. A diagnostic, never a judgement — the carve-out the
    /// crate CLAUDE.md records.
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

    use super::{AuthChoice, ClaimSummary, DraftView, ProbeAnswer, ScopePreview, StatementRow};

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
    ) -> Result<Option<ClaimSummary>, ServerFnError> {
        let state: crate::state::ConsoleState = leptos::prelude::expect_context();
        super::read::save_scope(
            &state,
            statement_json.filter(|s| !s.trim().is_empty()),
            filter.filter(|f| !f.is_empty()),
        )
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
