// SPDX-FileCopyrightText: Veredictum contributors
// SPDX-License-Identifier: Apache-2.0

//! The record seam for S6 Results and S7 Verdicts (#67).
//!
//! Everything renders from the finished job's own record, read through the
//! published lib. The verdicts come from the lib's `judgement::judge` — the
//! SAME pure function the CLI runs, so the matrix and the rendered documents
//! are the CLI's bodies by construction, never a console re-computation.

use serde::{Deserialize, Serialize};

/// One outcome row for the results table.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ResultRow {
    /// The case id.
    pub case: String,
    /// The wire format, when format-parameterized.
    pub format: Option<String>,
    /// The status token (`passed` / `failed` / `errored` / `skipped` /
    /// `not_applicable`).
    pub status: String,
    /// Rows driven / rows selected — the printed coverage bound.
    pub rows: String,
    /// The failure or error reason, verbatim, when any.
    pub reason: Option<String>,
}

/// The results screen: identity, tallies, and the rows red-first.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ResultsScreen {
    /// The SUT identity the record carries.
    pub sut: String,
    /// passed / failed / errored / excused counts.
    pub tallies: (u64, u64, u64, u64),
    /// Every outcome row, failures and errors first.
    pub rows: Vec<ResultRow>,
}

/// One failing row's evidence, verbatim from the record.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FailedRowView {
    /// The row's identity as the record spells it.
    pub row: String,
    /// The row's failure evidence, verbatim.
    pub evidence: String,
}

/// The result drawer: the outcome joined to its catalogue case.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ResultDetail {
    /// The outcome row.
    pub row: ResultRow,
    /// The excusing citation, mandatory on excused statuses.
    pub citation: Option<String>,
    /// The first failing or erroring step, when any.
    pub failing_step: Option<u32>,
    /// Per-row failure evidence for table-driven cases.
    pub failed_rows: Vec<FailedRowView>,
    /// The case's test purpose, from the catalogue.
    pub test_purpose: Option<String>,
    /// The case's citations, from the catalogue.
    pub spec_refs: Vec<String>,
}

/// One rendered judgement document, name and body verbatim.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DocumentView {
    /// The document's file name.
    pub name: String,
    /// The body, byte-for-byte what the CLI writes.
    pub body: String,
}

/// The verdicts screen, or the honest reasons there is none.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum VerdictsScreen {
    /// The judgement ran.
    Judged {
        /// Profile verdicts: tier token → verdict token.
        profiles: Vec<(String, String)>,
        /// Capability evidence: name → evidence token.
        capabilities: Vec<(String, String)>,
        /// The rendered documents.
        documents: Vec<DocumentView>,
    },
    /// The run was driven without a statement, so no claim exists to judge.
    NoStatement,
    /// No finished run exists yet.
    NoRun,
}

#[cfg(feature = "ssr")]
pub mod read {
    //! The ssr readers over the finished job and the lib's judgement.

    use super::{
        DocumentView, FailedRowView, ResultDetail, ResultRow, ResultsScreen, VerdictsScreen,
    };
    use crate::state::ConsoleState;

    /// A serde token without its quotes — the lib's own vocabulary, never a
    /// mirrored one.
    fn token<T: serde::Serialize>(value: &T) -> String {
        serde_json::to_string(value)
            .unwrap_or_default()
            .trim_matches('"')
            .to_owned()
    }

    /// The finished job's results record, through the published lib.
    fn finished_results(
        state: &ConsoleState,
    ) -> Result<Option<(veredictum::party::Results, std::path::PathBuf)>, String> {
        let Some(view) = state.jobs.view().map_err(|e| e.to_string())? else {
            return Ok(None);
        };
        let Some(finished) = view.finished else {
            return Ok(None);
        };
        let path = std::path::PathBuf::from(finished.results_path);
        let body =
            std::fs::read_to_string(&path).map_err(|e| format!("{}: {e}", path.display()))?;
        let results: veredictum::party::Results =
            serde_json::from_str(&body).map_err(|e| format!("{}: {e}", path.display()))?;
        Ok(Some((results, path)))
    }

    /// Maps one outcome to its table row.
    fn row_of(outcome: &veredictum::party::OutcomeRecord) -> ResultRow {
        ResultRow {
            case: outcome.case.to_string(),
            format: outcome.format.as_ref().map(token),
            status: token(&outcome.status),
            rows: format!("{}/{}", outcome.rows_driven, outcome.rows_total),
            reason: outcome.reason.clone(),
        }
    }

    /// The red-first ordering: failures, then errors, then the rest, id-tied.
    fn red_first(rows: &mut [ResultRow]) {
        rows.sort_by(|a, b| {
            let rank = |status: &str| match status {
                "failed" => 0_u8,
                "errored" => 1,
                "passed" => 2,
                _ => 3,
            };
            rank(&a.status)
                .cmp(&rank(&b.status))
                .then_with(|| a.case.cmp(&b.case))
        });
    }

    /// The results screen, or `Ok(None)` before a finished run.
    ///
    /// # Errors
    /// The verbatim read failures.
    pub fn results_screen(state: &ConsoleState) -> Result<Option<ResultsScreen>, String> {
        let Some((results, _)) = finished_results(state)? else {
            return Ok(None);
        };
        let mut tallies = (0_u64, 0_u64, 0_u64, 0_u64);
        let mut rows: Vec<ResultRow> = results
            .outcomes
            .iter()
            .map(|outcome| {
                let row = row_of(outcome);
                match row.status.as_str() {
                    "passed" => tallies.0 += 1,
                    "failed" => tallies.1 += 1,
                    "errored" => tallies.2 += 1,
                    _ => tallies.3 += 1,
                }
                row
            })
            .collect();
        red_first(&mut rows);
        Ok(Some(ResultsScreen {
            sut: format!("{} {}", results.sut.name, results.sut.version),
            tallies,
            rows,
        }))
    }

    /// One outcome joined to its catalogue case, or `Ok(None)` when the
    /// record does not carry the id.
    ///
    /// # Errors
    /// The verbatim read failures.
    pub fn result_detail(
        state: &ConsoleState,
        case: &str,
        format: Option<&str>,
    ) -> Result<Option<ResultDetail>, String> {
        let Some((results, _)) = finished_results(state)? else {
            return Ok(None);
        };
        let Some(outcome) = results.outcomes.iter().find(|outcome| {
            outcome.case.to_string() == case
                && outcome.format.as_ref().map(token).as_deref() == format
        }) else {
            return Ok(None);
        };
        let (test_purpose, spec_refs) = match state.catalogue.as_ref() {
            Ok(validation) => validation
                .loaded
                .set
                .cases
                .iter()
                .find(|(_, c)| c.id == outcome.case)
                .map_or((None, Vec::new()), |(_, c)| {
                    (Some(c.test_purpose.clone()), c.spec_refs.clone())
                }),
            Err(_) => (None, Vec::new()),
        };
        Ok(Some(ResultDetail {
            row: row_of(outcome),
            citation: outcome.citation.clone(),
            failing_step: outcome.failing_step,
            failed_rows: outcome
                .failed_rows
                .iter()
                .map(|failed| FailedRowView {
                    row: format!("row {} · step {}", failed.row, failed.step),
                    evidence: failed.reason.clone(),
                })
                .collect(),
            test_purpose,
            spec_refs,
        }))
    }

    /// The verdicts screen: the lib's own judgement over the finished run
    /// and the draft's statement — the CLI's bodies by construction.
    ///
    /// # Errors
    /// The verbatim judgement failure.
    pub fn verdicts_screen(state: &ConsoleState) -> Result<VerdictsScreen, String> {
        let Some((_, results_path)) = finished_results(state)? else {
            return Ok(VerdictsScreen::NoRun);
        };
        // The claim travels with the run: start_run writes the accepted
        // statement beside the results, so the judgement certifies exactly
        // the bytes the engine graded — never the mutable draft.
        let statement_path = results_path
            .parent()
            .map(|dir| dir.join("statement.json"))
            .filter(|path| path.is_file());
        let Some(statement_path) = statement_path else {
            return Ok(VerdictsScreen::NoStatement);
        };
        let judgement = veredictum::pipeline::judgement::judge(
            &veredictum::pipeline::judgement::JudgementRequest {
                statement: &statement_path,
                results: &results_path,
                root: &state.root,
            },
        )
        .map_err(|e| e.to_string())?;
        Ok(VerdictsScreen::Judged {
            profiles: judgement
                .report
                .profiles
                .iter()
                .map(|(tier, verdict)| (token(tier), token(verdict)))
                .collect(),
            capabilities: judgement
                .report
                .capabilities
                .iter()
                .map(|(name, evidence)| (name.to_string(), token(evidence)))
                .collect(),
            documents: judgement
                .documents
                .iter()
                .map(|document| DocumentView {
                    name: document.name.clone(),
                    body: document.body.clone(),
                })
                .collect(),
        })
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

    use super::{ResultDetail, ResultsScreen, VerdictsScreen};

    /// The results screen, `None` before a finished run.
    ///
    /// # Errors
    /// The verbatim read failures.
    #[server]
    pub async fn fetch_results() -> Result<Option<ResultsScreen>, ServerFnError> {
        let state: crate::state::ConsoleState = leptos::prelude::expect_context();
        super::read::results_screen(&state).map_err(ServerFnError::new)
    }

    /// One outcome's detail; `None` for an id the record does not carry.
    ///
    /// # Errors
    /// The verbatim read failures.
    #[server]
    pub async fn fetch_result_detail(
        case: String,
        format: Option<String>,
    ) -> Result<Option<ResultDetail>, ServerFnError> {
        let state: crate::state::ConsoleState = leptos::prelude::expect_context();
        super::read::result_detail(&state, &case, format.as_deref()).map_err(ServerFnError::new)
    }

    /// The verdicts screen.
    ///
    /// # Errors
    /// The verbatim judgement failure.
    #[server]
    pub async fn fetch_verdicts() -> Result<VerdictsScreen, ServerFnError> {
        let state: crate::state::ConsoleState = leptos::prelude::expect_context();
        super::read::verdicts_screen(&state).map_err(ServerFnError::new)
    }
}
