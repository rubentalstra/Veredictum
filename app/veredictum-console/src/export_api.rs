// SPDX-FileCopyrightText: Veredictum contributors
// SPDX-License-Identifier: Apache-2.0

//! S8 — the export seam (#68): one step that seals the finished run's record
//! and renders what a party publishes beside it.
//!
//! The sealing is the ENGINE's. This module spawns the pinned CLI's
//! `verdicts --sign-key`, which writes the rendered documents plus
//! `record-manifest.json` and its detached signature, then reads that bundle
//! back through the published lib's own `verify_bundle`. Nothing here signs,
//! digests a manifest into existence, or judges: the console's only additions
//! are the three presentation files, which sit deliberately outside the
//! manifest and each carry the record digest prefix that ties them to it.

use serde::{Deserialize, Serialize};

/// The URL the sealed bundle downloads from — a server-owned axum route, so
/// every anchor to it carries `rel="external"` (rules §4).
pub const DOWNLOAD_PATH: &str = "/export/record.zip";

/// The placeholder badge URL the copy-paste snippets carry.
///
/// The console cannot know where a party hosts its own copy of the badge, so
/// the snippet names the file relative to wherever it is published and the
/// surface says exactly that.
pub const BADGE_PLACEHOLDER_URL: &str = "record-badge.svg";

/// The three presentation files the console renders beside the sealed set.
pub const PRESENTATION_FILES: [&str; 3] =
    ["seal-card.svg", "record-badge.svg", "record-report.html"];

/// What one prepared export established.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ExportSummary {
    /// The full lowercase-hex SHA-256 of `record-manifest.json`.
    pub digest: String,
    /// The first twelve characters of [`Self::digest`], as the artwork shows.
    pub digest_prefix: String,
    /// The fingerprint of the key component that signed the manifest.
    pub fingerprint: String,
    /// When the signature was made, from its own creation subpacket.
    pub signed_at: String,
    /// The system under test the record names.
    pub sut: String,
    /// The profile verdicts summarized as the seal card renders them.
    pub profile_summary: String,
    /// The files the manifest covers — the sealed set, in manifest order.
    pub sealed_files: Vec<String>,
    /// The presentation files rendered beside it, outside the manifest.
    pub presentation_files: Vec<String>,
    /// The copy-paste markdown badge snippet.
    pub badge_markdown: String,
    /// The copy-paste HTML badge snippet.
    pub badge_html: String,
}

/// What the export section shows, including every honest reason there is
/// nothing to download.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ExportScreen {
    /// A bundle is sealed and downloadable.
    Prepared(Box<ExportSummary>),
    /// A finished run with a claim exists, and nothing has been sealed yet.
    Ready,
    /// A key the export needs is not mounted; the field names the variables.
    NoKey {
        /// The environment variables that must name a key file.
        missing: Vec<String>,
    },
    /// The run was driven without a statement, so there is no claim to seal.
    NoStatement,
    /// No finished run exists yet.
    NoRun,
}

#[cfg(feature = "ssr")]
pub mod prepare {
    //! The ssr side: spawn the engine's judgement, read the seal back, render
    //! the presentation set.

    use std::path::{Path, PathBuf};

    use sha2::{Digest as _, Sha256};

    use super::{
        BADGE_PLACEHOLDER_URL, ExportScreen, ExportSummary, PRESENTATION_FILES, VERIFY_PATH,
    };
    use crate::export::{self, ReportFacts, SealFacts};
    use crate::state::{ConsoleState, SIGN_KEY_ENV, VERIFY_KEY_ENV};

    /// The subdirectory of the job the sealed bundle lands in.
    pub const EXPORT_DIR: &str = "export";

    /// The finished job's own directory, when a finished run exists.
    ///
    /// The path itself comes from the run seam's one derivation
    /// (`run_job::job_dir`), so the export reads exactly the directory the run
    /// wrote into rather than a second spelling of it (#134).
    ///
    /// # Errors
    /// The slot's verbatim refusal.
    pub fn job_dir(state: &ConsoleState) -> Result<Option<PathBuf>, String> {
        let Some(view) = state.jobs.view().map_err(|e| e.to_string())? else {
            return Ok(None);
        };
        if view.finished.is_none() {
            return Ok(None);
        }
        Ok(Some(crate::run_job::job_dir(&state.out, view.id)))
    }

    /// Removes any sealed bundle left in a job directory.
    ///
    /// Called when a run STARTS into that directory. A bundle certifies the
    /// documents of the run that produced it, and nothing inside it names the
    /// run — so a stale one inherited by a later run would be presented as
    /// that run's record. The job counter restarts with the console process
    /// while the output mount persists, which makes the inheritance real
    /// rather than theoretical.
    ///
    /// # Errors
    /// The verbatim filesystem failure, because a bundle that cannot be
    /// removed must stop the run rather than be silently kept.
    pub fn invalidate(job_dir: &Path) -> Result<(), String> {
        let bundle = job_dir.join(EXPORT_DIR);
        if !bundle.exists() {
            return Ok(());
        }
        std::fs::remove_dir_all(&bundle).map_err(|e| format!("{}: {e}", bundle.display()))
    }

    /// The keys the export needs but does not have, named by variable.
    fn missing_keys(state: &ConsoleState) -> Vec<String> {
        let mut missing = Vec::new();
        if state.sign_key.is_none() {
            missing.push(String::from(SIGN_KEY_ENV));
        }
        // The card states a signing time and a signer, and the console will
        // not print either without checking the seal it just made — which
        // needs the public half.
        if state.verify_key.is_none() {
            missing.push(String::from(VERIFY_KEY_ENV));
        }
        missing
    }

    /// Lowercase hex, the encoding every digest the surfaces show carries.
    fn hex(bytes: &[u8]) -> String {
        use std::fmt::Write as _;
        bytes.iter().fold(
            String::with_capacity(bytes.len().saturating_mul(2)),
            |mut out, byte| {
                let _ = write!(out, "{byte:02x}");
                out
            },
        )
    }

    /// Where the export section stands, without preparing anything.
    ///
    /// # Errors
    /// The verbatim read failures.
    pub fn screen(state: &ConsoleState) -> Result<ExportScreen, String> {
        let Some(dir) = job_dir(state)? else {
            return Ok(ExportScreen::NoRun);
        };
        if !dir.join("statement.json").is_file() {
            return Ok(ExportScreen::NoStatement);
        }
        let missing = missing_keys(state);
        if !missing.is_empty() {
            return Ok(ExportScreen::NoKey { missing });
        }
        let bundle = dir.join(EXPORT_DIR);
        if bundle.join(veredictum::record::MANIFEST_FILE).is_file() {
            return summarize(state, &bundle).map(|s| ExportScreen::Prepared(Box::new(s)));
        }
        Ok(ExportScreen::Ready)
    }

    /// Seals the finished run's record and renders the presentation set.
    ///
    /// # Errors
    /// The refusal that applies: no finished run, no claim, no key mounted,
    /// the engine's own diagnostic, or the verbatim filesystem failure.
    pub fn run(state: &ConsoleState) -> Result<ExportSummary, String> {
        let engine = crate::engine::locate().map_err(|e| e.to_string())?;
        run_with(state, &engine)
    }

    /// Seals the finished run's record through an already-located engine.
    ///
    /// The split exists for the gate: [`run`] finds the pinned binary the way
    /// the server does, and a test injects the one it verified itself rather
    /// than reaching for `PATH`.
    ///
    /// # Errors
    /// The refusal that applies: no finished run, no claim, no key mounted,
    /// the engine's own diagnostic, or the verbatim filesystem failure.
    pub fn run_with(
        state: &ConsoleState,
        engine: &crate::engine::Engine,
    ) -> Result<ExportSummary, String> {
        let dir =
            job_dir(state)?.ok_or_else(|| String::from("no finished run: grade a server first"))?;
        let statement = dir.join("statement.json");
        if !statement.is_file() {
            return Err(String::from(
                "the run was driven without a statement, so there is no claim to seal: pick one at the Scope step and run again",
            ));
        }
        let missing = missing_keys(state);
        if !missing.is_empty() {
            return Err(format!(
                "no signing posture: set {} to an armored OpenPGP key file",
                missing.join(" and ")
            ));
        }
        let bundle = dir.join(EXPORT_DIR);
        std::fs::create_dir_all(&bundle).map_err(|e| format!("{}: {e}", bundle.display()))?;

        engine
            .verdicts(&crate::engine::VerdictsSpec {
                statement,
                results: dir.join("results.json"),
                root: state.root.clone(),
                out_dir: bundle.clone(),
                sign_key: state.sign_key.clone(),
            })
            .map_err(|e| e.to_string())?;

        let summary = summarize(state, &bundle)?;
        render_presentation(state, &bundle, &summary)?;
        Ok(summary)
    }

    /// Reads one sealed bundle back through the published lib and derives
    /// everything the surfaces and the artwork state about it.
    fn summarize(state: &ConsoleState, bundle: &Path) -> Result<ExportSummary, String> {
        let manifest_path = bundle.join(veredictum::record::MANIFEST_FILE);
        let manifest_bytes = std::fs::read(&manifest_path)
            .map_err(|e| format!("{}: {e}", manifest_path.display()))?;
        let digest = hex(&Sha256::digest(&manifest_bytes));
        let digest_prefix = export::prefix_of(&digest);

        let verify_key = state
            .verify_key
            .as_ref()
            .ok_or_else(|| format!("no public key: set {VERIFY_KEY_ENV}"))?;
        let verification =
            veredictum::record::verify_bundle(bundle, verify_key).map_err(|e| e.to_string())?;
        let veredictum::record::SignatureOutcome::Accepted(record) = &verification.signature else {
            return Err(String::from(
                "the console sealed the bundle and then could not verify it against the mounted public key: the two keys are not a pair",
            ));
        };
        if !verification.is_clean() {
            return Err(format!(
                "the sealed bundle does not verify: {}",
                verification.findings().join("; ")
            ));
        }

        let (sut, profile_summary, _) = record_facts(state)?;
        Ok(ExportSummary {
            digest,
            digest_prefix: digest_prefix.clone(),
            fingerprint: record.signer_fingerprint.clone(),
            signed_at: record.signed_at.to_string(),
            sut,
            profile_summary,
            sealed_files: verification
                .files
                .iter()
                .map(|file| file.name.clone())
                .collect(),
            presentation_files: PRESENTATION_FILES.iter().map(|f| (*f).to_owned()).collect(),
            badge_markdown: export::render::badge_markdown(BADGE_PLACEHOLDER_URL, VERIFY_PATH),
            badge_html: export::render::badge_html(BADGE_PLACEHOLDER_URL, VERIFY_PATH),
        })
    }

    /// The record's own identity, its profile summary, and the top tier's
    /// verdict — all through the lib's judgement, never a console tally.
    fn record_facts(state: &ConsoleState) -> Result<(String, String, (String, bool)), String> {
        let results = crate::record_api::read::results_screen(state)?
            .ok_or_else(|| String::from("no finished run"))?;
        let screen = crate::record_api::read::verdicts_screen(state)?;
        let crate::record_api::VerdictsScreen::Judged { profiles, .. } = &screen else {
            return Err(String::from("the run carries no judgeable claim"));
        };
        // Only CLAIMED tiers reach the card: an unclaimed tier has no verdict
        // to state, and listing it pushes the slot's one line across the rule
        // the master draws for it. The console's own matrix still shows every
        // tier, so nothing is hidden — this is the certificate's line length.
        let claimed: Vec<&(String, String)> = profiles
            .iter()
            .filter(|(_, verdict)| verdict != "not_claimed")
            .collect();
        let mut summary: Vec<String> = claimed
            .iter()
            .map(|(tier, verdict)| format!("{tier} {verdict}"))
            .collect();
        for class in measured_classes(state)? {
            summary.push(class);
        }
        let badge = claimed.first().map_or_else(
            || (String::from("no profile claimed"), false),
            |(tier, verdict)| (format!("{tier} {verdict}"), verdict == "pass"),
        );
        if summary.is_empty() {
            summary.push(String::from("no profile claimed"));
        }
        Ok((results.sut, summary.join(" · "), badge))
    }

    /// The measured performance classes the judgement earned, when the
    /// campaign carried measured runs at all.
    fn measured_classes(state: &ConsoleState) -> Result<Vec<String>, String> {
        let Some(dir) = job_dir(state)? else {
            return Ok(Vec::new());
        };
        let statement = dir.join("statement.json");
        let results = dir.join("results.json");
        if !statement.is_file() {
            return Ok(Vec::new());
        }
        let judgement = veredictum::pipeline::judgement::judge(
            &veredictum::pipeline::judgement::JudgementRequest {
                statement: &statement,
                results: &results,
                root: &state.root,
            },
        )
        .map_err(|e| e.to_string())?;
        Ok(judgement
            .report
            .performance
            .iter()
            .map(|verdict| {
                let class = serde_json::to_string(&verdict.class).unwrap_or_default();
                let outcome = serde_json::to_string(&verdict.verdict).unwrap_or_default();
                format!(
                    "class {} {}",
                    class.trim_matches('"'),
                    outcome.trim_matches('"')
                )
            })
            .collect())
    }

    /// Writes the three presentation files beside the sealed set.
    fn render_presentation(
        state: &ConsoleState,
        bundle: &Path,
        summary: &ExportSummary,
    ) -> Result<(), String> {
        let (_, _, badge) = record_facts(state)?;
        let seal = SealFacts {
            sut: summary.sut.clone(),
            profile_summary: summary.profile_summary.clone(),
            signed_at: summary.signed_at.clone(),
            digest: summary.digest.clone(),
            digest_prefix: summary.digest_prefix.clone(),
            fingerprint: summary.fingerprint.clone(),
            badge_label: badge.0,
            badge_pass: badge.1,
        };
        let card = export::render::seal_card(&seal).map_err(|e| e.to_string())?;
        let results = crate::record_api::read::results_screen(state)?
            .ok_or_else(|| String::from("no finished run"))?;
        let screen = crate::record_api::read::verdicts_screen(state)?;
        let (profiles, capabilities) = match screen {
            crate::record_api::VerdictsScreen::Judged {
                profiles,
                capabilities,
                ..
            } => (profiles, capabilities),
            _ => (Vec::new(), Vec::new()),
        };
        let report = export::render::html_report(&ReportFacts {
            results,
            profiles,
            capabilities,
            seal: seal.clone(),
        });

        let bodies: [(&str, String); 3] = [
            ("seal-card.svg", card),
            ("record-badge.svg", export::render::badge(&seal)),
            ("record-report.html", report),
        ];
        for (name, body) in bodies {
            let path = bundle.join(name);
            std::fs::write(&path, body).map_err(|e| format!("{}: {e}", path.display()))?;
        }
        Ok(())
    }

    /// Zips the finished job's export directory in memory.
    ///
    /// Only plain file names are archived: a bundle is a flat directory by
    /// construction, and refusing anything else keeps the archive's entry
    /// names as safe as the manifest's own rule requires.
    ///
    /// # Errors
    /// The verbatim refusal when nothing is prepared or the archive cannot be
    /// written.
    pub fn archive(state: &ConsoleState) -> Result<Vec<u8>, String> {
        let dir = job_dir(state)?
            .ok_or_else(|| String::from("no finished run"))?
            .join(EXPORT_DIR);
        if !dir.join(veredictum::record::MANIFEST_FILE).is_file() {
            return Err(String::from("no prepared export"));
        }
        let mut names: Vec<String> = Vec::new();
        for entry in std::fs::read_dir(&dir).map_err(|e| format!("{}: {e}", dir.display()))? {
            let entry = entry.map_err(|e| format!("{}: {e}", dir.display()))?;
            if entry.file_type().is_ok_and(|kind| kind.is_dir()) {
                continue;
            }
            if let Some(name) = entry.file_name().to_str()
                && crate::verify_api::unpack::is_plain_file_name(name)
            {
                names.push(name.to_owned());
            }
        }
        // Sorted, so the same export directory always zips its entries in the
        // same order.
        names.sort();

        let mut buffer = std::io::Cursor::new(Vec::new());
        let mut writer = zip::ZipWriter::new(&mut buffer);
        let options = zip::write::SimpleFileOptions::default()
            .compression_method(zip::CompressionMethod::Deflated);
        for name in names {
            let body = std::fs::read(dir.join(&name)).map_err(|e| format!("{name}: {e}"))?;
            writer
                .start_file(name.clone(), options)
                .map_err(|e| format!("{name}: {e}"))?;
            std::io::Write::write_all(&mut writer, &body).map_err(|e| format!("{name}: {e}"))?;
        }
        writer.finish().map_err(|e| e.to_string())?;
        Ok(buffer.into_inner())
    }
}

/// Where the public verification page lives, for the snippets and the copy.
pub const VERIFY_PATH: &str = "/verify";

#[cfg(feature = "ssr")]
pub mod route {
    //! The server-owned download route, registered beside `/healthz`.

    use axum::response::IntoResponse as _;

    /// Serves the prepared bundle as one archive.
    ///
    /// Deliberately outside the Leptos route tree, like `/healthz`: it
    /// answers with bytes rather than a view, and every anchor pointing at it
    /// carries `rel="external"` so the client router does not intercept it.
    #[expect(
        clippy::unused_async,
        reason = "an axum handler is async by contract; this one only reads the filesystem"
    )]
    pub async fn record_zip(
        axum::Extension(state): axum::Extension<crate::state::ConsoleState>,
    ) -> axum::response::Response {
        match super::prepare::archive(&state) {
            Ok(bytes) => (
                axum::http::StatusCode::OK,
                [
                    (
                        axum::http::header::CONTENT_TYPE,
                        "application/zip".to_owned(),
                    ),
                    (
                        axum::http::header::CONTENT_DISPOSITION,
                        "attachment; filename=\"veredictum-record.zip\"".to_owned(),
                    ),
                ],
                bytes,
            )
                .into_response(),
            Err(reason) => (
                axum::http::StatusCode::NOT_FOUND,
                format!("no prepared export: {reason}\n"),
            )
                .into_response(),
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

    use super::{ExportScreen, ExportSummary};

    /// Where the export section stands.
    ///
    /// # Errors
    /// The verbatim read failures.
    #[server]
    pub async fn fetch_export() -> Result<ExportScreen, ServerFnError> {
        let state: crate::state::ConsoleState = leptos::prelude::expect_context();
        super::prepare::screen(&state).map_err(ServerFnError::new)
    }

    /// Seals the finished run's record and renders what a party publishes.
    ///
    /// # Errors
    /// The refusal that applies, verbatim: no finished run, no claim, no key
    /// mounted, or the engine's own diagnostic.
    #[server]
    pub async fn prepare_export() -> Result<ExportSummary, ServerFnError> {
        let state: crate::state::ConsoleState = leptos::prelude::expect_context();
        super::prepare::run(&state).map_err(ServerFnError::new)
    }
}
