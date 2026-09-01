// SPDX-FileCopyrightText: Veredictum contributors
// SPDX-License-Identifier: Apache-2.0

//! The evidence seam (#463): a red run hands over the exchanges behind its
//! own red rows, so a triage reads the wire rather than a summary of it.
//!
//! The carving is the ENGINE's. This module spawns the pinned CLI's
//! `evidence --failing`, which refuses rather than write a bundle that would
//! carry nothing, and serves the bytes it wrote. Nothing here filters,
//! redacts or judges: a second implementation of the selection would be a
//! second thing to get wrong.
//!
//! No statement is involved. Sealing a RECORD needs a claim, and reading the
//! exchanges a run recorded does not.

use serde::{Deserialize, Serialize};

/// The URL the bundle downloads from — a server-owned axum route, so every
/// anchor to it carries `rel="external"` (rules §4).
pub const DOWNLOAD_PATH: &str = "/export/evidence.json";

/// Whether the finished run can hand over the evidence behind its red rows.
///
/// Every variant is a state a reader can act on, and none carries a count:
/// the results screen already states the tallies, and a second copy of them
/// would be a second thing that can disagree.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum EvidenceOffer {
    /// The run went red and recorded its wire: the bundle is downloadable.
    Available,
    /// The run recorded its wire and no row went red.
    NoRedRows,
    /// The run was driven without recording, so there is no wire to hand
    /// over. The next run records it by ticking the box at the Run step.
    NotRecorded,
    /// No finished run exists yet.
    NoRun,
}

#[cfg(feature = "ssr")]
pub mod prepare {
    //! The ssr side: decide what the run can offer, spawn the engine's own
    //! carving, read the bundle back.

    use std::path::PathBuf;

    use super::EvidenceOffer;
    use crate::state::ConsoleState;

    /// The bundle's name inside the finished run's own directory.
    pub const EVIDENCE_FILE: &str = veredictum::evidence::EVIDENCE_FILE;

    /// The two documents an export reads, when the run wrote both.
    struct RunDocuments {
        /// The run's `results.json`, as the engine recorded its path.
        results: PathBuf,
        /// The `transcript.json` beside it.
        transcript: PathBuf,
        /// The directory holding both, which the bundle is written into.
        dir: PathBuf,
    }

    /// This submitter's most recent finished run's documents.
    ///
    /// The results path comes from the job map's own record of where the
    /// engine wrote it, so nothing here re-derives a run directory.
    ///
    /// # Errors
    /// The map's verbatim refusal.
    fn documents(
        state: &ConsoleState,
        submitter: crate::submitter::Submitter,
    ) -> Result<Option<RunDocuments>, String> {
        let latest = state
            .jobs
            .latest_of(submitter, crate::run_job::Latest::Finished)
            .map_err(|e| e.to_string())?;
        let Some(id) = latest else {
            return Ok(None);
        };
        let Some(finished) = state
            .jobs
            .view_of(id)
            .map_err(|e| e.to_string())?
            .and_then(|view| view.finished)
        else {
            return Ok(None);
        };
        let results = PathBuf::from(finished.results_path);
        let Some(dir) = results.parent().map(std::path::Path::to_path_buf) else {
            return Ok(None);
        };
        Ok(Some(RunDocuments {
            transcript: dir.join(veredictum::transcript::TRANSCRIPT_FILE),
            results,
            dir,
        }))
    }

    /// What the finished run can offer, without spawning anything.
    ///
    /// # Errors
    /// The verbatim read failures.
    pub fn offer(
        state: &ConsoleState,
        submitter: crate::submitter::Submitter,
    ) -> Result<EvidenceOffer, String> {
        let Some(documents) = documents(state, submitter)? else {
            return Ok(EvidenceOffer::NoRun);
        };
        if !documents.transcript.is_file() {
            return Ok(EvidenceOffer::NotRecorded);
        }
        let Some(screen) = crate::record_api::read::results_screen(state, submitter)? else {
            return Ok(EvidenceOffer::NoRun);
        };
        let (_, failed, errored, _) = screen.tallies;
        if failed == 0 && errored == 0 {
            return Ok(EvidenceOffer::NoRedRows);
        }
        Ok(EvidenceOffer::Available)
    }

    /// Carves the red rows' exchanges out of the finished run and reads the
    /// bundle back as bytes.
    ///
    /// # Errors
    /// The refusal that applies, verbatim: no finished run, no recording, no
    /// engine mounted, the engine's own diagnostic when the bundle would
    /// carry nothing, or the filesystem failure.
    pub fn bundle(
        state: &ConsoleState,
        submitter: crate::submitter::Submitter,
    ) -> Result<Vec<u8>, String> {
        let engine = crate::engine::locate().map_err(|e| e.to_string())?;
        bundle_with(state, submitter, &engine)
    }

    /// Carves the bundle through an already-located engine.
    ///
    /// The split exists for the gate: [`bundle`] finds the pinned binary the
    /// way the server does, and a test injects the one it verified itself
    /// rather than reaching for `PATH`.
    ///
    /// # Errors
    /// The refusal that applies, verbatim.
    pub fn bundle_with(
        state: &ConsoleState,
        submitter: crate::submitter::Submitter,
        engine: &crate::engine::Engine,
    ) -> Result<Vec<u8>, String> {
        let documents = documents(state, submitter)?
            .ok_or_else(|| String::from("no finished run: grade a server first"))?;
        if !documents.transcript.is_file() {
            return Err(String::from(
                "the run was driven without recording its wire, so there are no exchanges to hand over: tick `record exchanges` at the Run step and grade again",
            ));
        }
        let out = documents.dir.join(EVIDENCE_FILE);
        engine
            .evidence(&crate::engine::EvidenceSpec {
                transcript: documents.transcript,
                results: documents.results,
                out: out.clone(),
            })
            .map_err(|e| e.to_string())?;
        std::fs::read(&out).map_err(|e| format!("{}: {e}", out.display()))
    }
}

#[cfg(feature = "ssr")]
pub mod route {
    //! The server-owned download route, registered beside `/healthz`.

    use axum::response::IntoResponse as _;

    /// Serves the red rows' evidence bundle as one JSON document.
    ///
    /// Outside the Leptos route tree because it answers with bytes rather
    /// than a view, so every anchor pointing at it carries `rel="external"`
    /// and the client router does not intercept it.
    pub async fn evidence_json(
        axum::Extension(state): axum::Extension<crate::state::ConsoleState>,
        connect: Option<axum::Extension<axum::extract::ConnectInfo<std::net::SocketAddr>>>,
        headers: axum::http::HeaderMap,
    ) -> axum::response::Response {
        // Outside the Leptos route tree there are no request Parts in
        // context, so this handler gathers what the ONE submitter derivation
        // needs from its own extractors.
        let who = crate::submitter::of_request(
            crate::submitter::header_value(&state, &headers),
            connect.map(|axum::Extension(axum::extract::ConnectInfo(peer))| peer.ip()),
        );
        match super::prepare::bundle(&state, who) {
            Ok(bytes) => (
                axum::http::StatusCode::OK,
                [
                    (
                        axum::http::header::CONTENT_TYPE,
                        "application/json".to_owned(),
                    ),
                    (
                        axum::http::header::CONTENT_DISPOSITION,
                        format!("attachment; filename=\"{}\"", super::prepare::EVIDENCE_FILE),
                    ),
                ],
                bytes,
            )
                .into_response(),
            // The engine's refusal travels verbatim: an operator who asked
            // for evidence that does not exist is told which of the two
            // reasons applies, never handed an empty document.
            Err(reason) => (
                axum::http::StatusCode::NOT_FOUND,
                format!("no evidence bundle: {reason}\n"),
            )
                .into_response(),
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

    use super::EvidenceOffer;

    /// Whether the finished run can hand over its red rows' exchanges.
    ///
    /// # Errors
    /// The verbatim read failures.
    #[server]
    pub async fn fetch_evidence_offer() -> Result<EvidenceOffer, ServerFnError> {
        let state: crate::state::ConsoleState = leptos::prelude::expect_context();
        let who = crate::submitter::current(&state);
        super::prepare::offer(&state, who).map_err(ServerFnError::new)
    }
}
