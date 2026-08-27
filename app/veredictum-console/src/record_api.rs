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

/// One recorded exchange, rendered as the two panes the drawer shows.
///
/// The bodies are the wire's own bytes, pretty-printed when they parsed as
/// JSON. The engine withholds the `authorization` request header before the
/// transcript is ever written, so nothing here can carry a credential.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ExchangeView {
    /// The exchange's ordinal within the case, in send order.
    pub seq: u32,
    /// The request line, `METHOD url`.
    pub request_line: String,
    /// The request headers, one `name: value` line each.
    pub request_headers: String,
    /// The request body, when the request carried one.
    pub request_body: Option<String>,
    /// The status line, `HTTP <status>`.
    pub status_line: String,
    /// The response headers, one `name: value` line each.
    pub response_headers: String,
    /// The response body, when the SUT sent one.
    pub response_body: Option<String>,
}

/// Whether the finished run carries a wire transcript, and what it holds for
/// the selected case.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum TranscriptView {
    /// The run was driven without `--record-exchanges`, so no transcript file
    /// sits beside the record.
    NotRecorded,
    /// A transcript exists; the exchanges are this case's, in send order
    /// (empty when the case drove none).
    Recorded(Vec<ExchangeView>),
    /// The transcript exists but could not be read; the message is verbatim.
    Unreadable(String),
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
    /// The wire this outcome was reached over, when the run recorded it.
    pub transcript: TranscriptView,
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
        DocumentView, ExchangeView, FailedRowView, ResultDetail, ResultRow, ResultsScreen,
        TranscriptView, VerdictsScreen,
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

    // TODO(#129): read the transcript through `veredictum::transcript` and
    // delete these mirrors; the console's engine pin predates that module, so
    // the published lib cannot supply the types yet.
    mod wire {
        //! The run wire transcript, as the engine writes it.

        #![expect(
            clippy::disallowed_types,
            reason = "the wire-bodies family: a recorded request or response body is whatever the SUT sent, which has no typed model here"
        )]

        use serde::Deserialize;

        /// One recorded request.
        #[derive(Debug, Deserialize)]
        pub(super) struct Request {
            pub(super) method: String,
            pub(super) url: String,
            pub(super) headers: std::collections::BTreeMap<String, String>,
            #[serde(default)]
            pub(super) body: Option<serde_json::Value>,
        }

        /// One recorded response.
        #[derive(Debug, Deserialize)]
        pub(super) struct Response {
            pub(super) status: u16,
            pub(super) headers: std::collections::BTreeMap<String, String>,
            #[serde(default)]
            pub(super) body: Option<serde_json::Value>,
        }

        /// One exchange: what went out, and what came back.
        #[derive(Debug, Deserialize)]
        pub(super) struct Exchange {
            pub(super) seq: u32,
            pub(super) request: Request,
            pub(super) response: Response,
        }

        /// Every exchange one case×format execution drove.
        #[derive(Debug, Deserialize)]
        pub(super) struct Entry {
            pub(super) case: String,
            #[serde(default)]
            pub(super) format: Option<veredictum::vocab::FormatName>,
            pub(super) exchanges: Vec<Exchange>,
        }

        /// The whole run's transcript.
        #[derive(Debug, Deserialize)]
        pub(super) struct Transcript {
            pub(super) cases: Vec<Entry>,
        }
    }

    /// The transcript file name the engine writes beside `results.json`.
    const TRANSCRIPT_FILE: &str = "transcript.json";

    /// One `name: value` line per header, name-sorted by the document's own
    /// `BTreeMap` ordering.
    fn header_lines(headers: &std::collections::BTreeMap<String, String>) -> String {
        headers
            .iter()
            .map(|(name, value)| format!("{name}: {value}"))
            .collect::<Vec<_>>()
            .join("\n")
    }

    /// A recorded body as text: pretty JSON when it is a JSON structure, the
    /// string itself when the wire carried text the reader kept as a string.
    #[expect(
        clippy::disallowed_types,
        reason = "the wire-bodies family: a recorded body is whatever the SUT sent, which has no typed model here"
    )]
    fn body_text(body: Option<&serde_json::Value>) -> Option<String> {
        match body? {
            serde_json::Value::String(text) => Some(text.clone()),
            value => serde_json::to_string_pretty(value).ok(),
        }
    }

    /// The transcript beside the run's `results.json`, narrowed to one case.
    ///
    /// An absent file is the honest [`TranscriptView::NotRecorded`]: the run
    /// was driven without `--record-exchanges`. A file that exists and will
    /// not read or parse is reported verbatim, never silently as absence.
    fn transcript_of(
        results_path: &std::path::Path,
        case: &str,
        format: Option<&str>,
    ) -> TranscriptView {
        let Some(path) = results_path.parent().map(|dir| dir.join(TRANSCRIPT_FILE)) else {
            return TranscriptView::NotRecorded;
        };
        let body = match std::fs::read_to_string(&path) {
            Ok(body) => body,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                return TranscriptView::NotRecorded;
            }
            Err(e) => return TranscriptView::Unreadable(format!("{}: {e}", path.display())),
        };
        narrow(&body, case, format)
            .unwrap_or_else(|e| TranscriptView::Unreadable(format!("{}: {e}", path.display())))
    }

    /// Narrows one transcript document to a single case's exchanges.
    ///
    /// A case the document does not carry yields an EMPTY
    /// [`TranscriptView::Recorded`]: the run recorded its wire, and this case
    /// drove nothing.
    ///
    /// # Errors
    /// The parse failure verbatim, when the body is not a transcript.
    pub fn narrow(
        body: &str,
        case: &str,
        format: Option<&str>,
    ) -> Result<TranscriptView, serde_json::Error> {
        let transcript: wire::Transcript = serde_json::from_str(body)?;
        let exchanges = transcript
            .cases
            .iter()
            .find(|entry| {
                entry.case == case && entry.format.as_ref().map(token).as_deref() == format
            })
            .map(|entry| {
                entry
                    .exchanges
                    .iter()
                    .map(|exchange| ExchangeView {
                        seq: exchange.seq,
                        request_line: format!(
                            "{} {}",
                            exchange.request.method, exchange.request.url
                        ),
                        request_headers: header_lines(&exchange.request.headers),
                        request_body: body_text(exchange.request.body.as_ref()),
                        status_line: format!("HTTP {}", exchange.response.status),
                        response_headers: header_lines(&exchange.response.headers),
                        response_body: body_text(exchange.response.body.as_ref()),
                    })
                    .collect()
            })
            .unwrap_or_default();
        Ok(TranscriptView::Recorded(exchanges))
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
        let Some((results, results_path)) = finished_results(state)? else {
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
            transcript: transcript_of(&results_path, case, format),
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

#[cfg(all(test, feature = "ssr"))]
mod tests {
    use super::TranscriptView;
    use super::read::narrow;

    /// One transcript document as the engine writes it, authored as raw bytes
    /// so the reader is held to the wire form rather than to a value this
    /// crate serialized itself.
    const DOCUMENT: &str = r#"{
      "sut": { "name": "example-cdr", "version": "0.0.0" },
      "schedule_release": "cnf-2.0-w2",
      "cases": [
        {
          "case": "I_EHR_SERVICE.create_ehr-main",
          "format": "canonical-json",
          "exchanges": [
            {
              "seq": 1,
              "row": 0,
              "request": {
                "method": "POST",
                "url": "http://cdr.example/ehr",
                "headers": { "authorization": "«redacted»", "content-type": "application/json" },
                "body": { "_type": "EHR_STATUS" }
              },
              "response": {
                "status": 201,
                "headers": { "etag": "\"abc\"" },
                "body": "created"
              }
            }
          ]
        },
        {
          "case": "I_EHR_SERVICE.get_ehr-main",
          "exchanges": [
            {
              "seq": 1,
              "row": 0,
              "request": { "method": "GET", "url": "http://cdr.example/ehr/x", "headers": {} },
              "response": { "status": 404, "headers": {} }
            }
          ]
        }
      ]
    }"#;

    #[expect(
        clippy::panic_in_result_fn,
        reason = "the Book ch11 Result-returning test shape: assertions panic, plumbing propagates with ? (https://doc.rust-lang.org/book/ch11-01-writing-tests.html)"
    )]
    #[test]
    fn a_recorded_case_renders_both_sides_of_its_wire() -> Result<(), serde_json::Error> {
        let view = narrow(
            DOCUMENT,
            "I_EHR_SERVICE.create_ehr-main",
            Some("canonical-json"),
        )?;
        let TranscriptView::Recorded(exchanges) = view else {
            panic!("a document that carries the case is recorded: {view:?}");
        };
        let first = exchanges.first().expect("one exchange");
        assert_eq!(first.seq, 1);
        assert_eq!(first.request_line, "POST http://cdr.example/ehr");
        assert!(
            first.request_headers.contains("authorization: «redacted»"),
            "{}",
            first.request_headers
        );
        assert_eq!(first.status_line, "HTTP 201");
        assert_eq!(first.response_headers, "etag: \"abc\"");
        // A JSON body pretty-prints; a body the reader kept as a string is
        // the string itself, never a re-quoted one.
        assert_eq!(
            first.request_body.as_deref(),
            Some("{\n  \"_type\": \"EHR_STATUS\"\n}")
        );
        assert_eq!(first.response_body.as_deref(), Some("created"));
        Ok(())
    }

    /// The format is part of the identity: the same case id on another format
    /// is a different row, and it selects nothing.
    #[expect(
        clippy::panic_in_result_fn,
        reason = "the Book ch11 Result-returning test shape: assertions panic, plumbing propagates with ? (https://doc.rust-lang.org/book/ch11-01-writing-tests.html)"
    )]
    #[test]
    fn the_format_discriminates_the_case() -> Result<(), serde_json::Error> {
        let TranscriptView::Recorded(matched) =
            narrow(DOCUMENT, "I_EHR_SERVICE.get_ehr-main", None)?
        else {
            panic!("the format-less case is carried by the document");
        };
        assert_eq!(matched.len(), 1);
        let mismatched = narrow(
            DOCUMENT,
            "I_EHR_SERVICE.get_ehr-main",
            Some("canonical-xml"),
        )?;
        assert_eq!(
            mismatched,
            TranscriptView::Recorded(vec![]),
            "another format is another row"
        );
        Ok(())
    }

    /// A case the transcript does not carry is an honest empty recording, not
    /// an error and not an absent transcript.
    #[expect(
        clippy::panic_in_result_fn,
        reason = "the Book ch11 Result-returning test shape: assertions panic, plumbing propagates with ? (https://doc.rust-lang.org/book/ch11-01-writing-tests.html)"
    )]
    #[test]
    fn an_unrecorded_case_is_an_empty_recording() -> Result<(), serde_json::Error> {
        assert_eq!(
            narrow(DOCUMENT, "I_EHR_SERVICE.delete_ehr-main", None)?,
            TranscriptView::Recorded(vec![])
        );
        Ok(())
    }

    /// A body that is not a transcript is a parse failure the caller reports
    /// verbatim, never silence.
    #[test]
    fn a_body_that_is_not_a_transcript_is_refused() {
        assert!(narrow("{ not json", "any", None).is_err());
        assert!(narrow(r#"{"cases":"nope"}"#, "any", None).is_err());
    }
}
