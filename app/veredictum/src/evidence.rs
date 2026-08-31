// SPDX-FileCopyrightText: Veredictum contributors
// SPDX-License-Identifier: Apache-2.0

//! The evidence bundle: the recorded exchanges of a named set of cases,
//! carved out of a finished run's transcript for a triage to read.
//!
//! NOTE: no openEHR spec governs this — our own design; the attribution law
//! reproduces an exchange before it accuses anybody, and a whole run's
//! transcript is too large to hand to a reader.
//!
//! Two properties separate this document from the transcript it comes from.
//! It is SELECTED, so the bundle carries the question it answers and names
//! every selected case the recording turned out to have nothing for. And it
//! is REFUSED when it would be empty: a bundle of the right shape with no
//! content in it is the failure this module exists to make impossible,
//! because valid JSON of the expected size is exactly what nobody checks.
//!
//! What a party CLAIMS is not part of this. A statement declares a claim,
//! and the exchanges a run recorded are readable whether or not anyone
//! claimed anything over them.

use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::party::{OutcomeRecord, OutcomeStatus};
use crate::transcript::{CaseTranscript, RecordedExchange, RunTranscript, recorded_headers};

/// The bundle's conventional file name.
pub const EVIDENCE_FILE: &str = "evidence.json";

/// What a caller asked the export for.
///
/// The three selectors union rather than intersect: a case is selected when
/// ANY of them names it, so one command asks for the red rows plus the two
/// green cases a reader wants beside them.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct EvidenceSelection {
    /// Case ids named outright, sorted and de-duplicated.
    #[serde(default)]
    pub only: Vec<String>,
    /// A case-id substring.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub filter: Option<String>,
    /// Outcome statuses selected out of the run's results record.
    #[serde(default)]
    pub statuses: Vec<OutcomeStatus>,
}

impl EvidenceSelection {
    /// Whether this selection names nothing at all.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.only.is_empty() && self.filter.is_none() && self.statuses.is_empty()
    }

    /// The selection as one diagnostic clause, for a refusal to quote back.
    #[must_use]
    pub fn describe(&self) -> String {
        let mut parts = Vec::new();
        if !self.only.is_empty() {
            parts.push(format!("--only {}", self.only.join(", ")));
        }
        if let Some(filter) = &self.filter {
            parts.push(format!("--filter {filter}"));
        }
        if !self.statuses.is_empty() {
            let tokens: Vec<&str> = self.statuses.iter().map(|s| s.token()).collect();
            parts.push(format!("status {}", tokens.join(", ")));
        }
        if parts.is_empty() {
            String::from("nothing")
        } else {
            parts.join(" + ")
        }
    }

    /// Whether this selection names `case`, given the run's outcome rows.
    fn selects(&self, case: &str, outcomes: &[OutcomeRecord]) -> bool {
        if self.only.iter().any(|id| id == case) {
            return true;
        }
        if self
            .filter
            .as_ref()
            .is_some_and(|needle| case.contains(needle.as_str()))
        {
            return true;
        }
        !self.statuses.is_empty()
            && outcomes
                .iter()
                .any(|row| row.case.as_str() == case && self.statuses.contains(&row.status))
    }
}

/// Every exchange one selected case×format execution drove, with the row the
/// run recorded for it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CaseEvidence {
    /// The case id.
    pub case: crate::ids::CaseId,
    /// The wire format, when the case is format-parameterized.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub format: Option<crate::vocab::FormatName>,
    /// The outcome row the run recorded, when a results record was supplied.
    ///
    /// The row is what a triage compares the exchanges against — the reason
    /// the runner gave, and the per-row evidence behind it — so the bundle
    /// carries it rather than making a reader hold two documents open.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub outcome: Option<OutcomeRecord>,
    /// The exchanges, in send order.
    pub exchanges: Vec<RecordedExchange>,
}

/// The exchanges of one named set of cases, out of one finished run.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EvidenceBundle {
    /// The system under test the exchanges were driven against.
    pub sut: crate::party::Sut,
    /// The schedule release the campaign ran.
    pub schedule_release: String,
    /// What was asked for.
    pub selection: EvidenceSelection,
    /// Selected case ids the transcript carries no exchange for, sorted.
    ///
    /// A half-matched selection is the quiet failure: the bundle looks
    /// complete and answers less than it was asked. Naming the misses here
    /// puts them in front of whoever reads the document.
    #[serde(default)]
    pub without_exchanges: Vec<String>,
    /// One entry per selected case that drove at least one exchange,
    /// case-id sorted (the transcript's own canonical order).
    pub cases: Vec<CaseEvidence>,
}

impl EvidenceBundle {
    /// The total number of exchanges the bundle carries.
    #[must_use]
    pub fn exchange_count(&self) -> usize {
        self.cases.iter().map(|case| case.exchanges.len()).sum()
    }
}

/// Why an export produced nothing, typed so a caller branches on which.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum EvidenceError {
    /// A selection that names nothing would export the whole transcript,
    /// which is the file the caller already has.
    #[error(
        "the export names no cases: pass --only, --filter or --failing (the transcript itself is the unfiltered document)"
    )]
    SelectionEmpty,
    /// No case in the transcript answers the selection.
    #[error(
        "nothing selected: asked for {asked}, and none of the {available} case(s) the transcript carries match. The transcript's ids read: {sample}"
    )]
    NothingMatched {
        /// The selection, as [`EvidenceSelection::describe`] renders it.
        asked: String,
        /// How many case entries the transcript carries.
        available: usize,
        /// A few of the transcript's own case ids, so a typo is visible.
        sample: String,
    },
    /// Cases were selected and every one of them recorded nothing.
    #[error(
        "no exchanges: {asked} selected {selected} case(s), and every one carries zero recorded exchanges ({names}). A run records exchanges only with `run --record-exchanges`"
    )]
    RecordingEmpty {
        /// The selection, as [`EvidenceSelection::describe`] renders it.
        asked: String,
        /// How many cases the selection matched.
        selected: usize,
        /// The matched case ids.
        names: String,
    },
}

/// How many transcript ids a refusal quotes back, so a typo is visible
/// without printing a thousand-line list.
const SAMPLE_IDS: usize = 5;

/// Carves the selected cases' exchanges out of a finished run's transcript.
///
/// The credential withholding is re-applied here rather than trusted from
/// the transcript, so the bundle's redaction is a property of this function
/// however the input document was produced.
///
/// # Errors
/// [`EvidenceError::SelectionEmpty`] when the selection names nothing,
/// [`EvidenceError::NothingMatched`] when no recorded case answers it, and
/// [`EvidenceError::RecordingEmpty`] when every selected case recorded nothing.
pub fn assemble(
    transcript: &RunTranscript,
    outcomes: &[OutcomeRecord],
    selection: &EvidenceSelection,
) -> Result<EvidenceBundle, EvidenceError> {
    if selection.is_empty() {
        return Err(EvidenceError::SelectionEmpty);
    }
    let selected: Vec<&CaseTranscript> = transcript
        .cases
        .iter()
        .filter(|entry| selection.selects(entry.case.as_str(), outcomes))
        .collect();
    if selected.is_empty() {
        return Err(EvidenceError::NothingMatched {
            asked: selection.describe(),
            available: transcript.cases.len(),
            sample: sample_of(transcript),
        });
    }

    let mut without: Vec<String> = selected
        .iter()
        .filter(|entry| entry.exchanges.is_empty())
        .map(|entry| entry.case.to_string())
        .collect();
    without.sort_unstable();
    without.dedup();

    let cases: Vec<CaseEvidence> = selected
        .iter()
        .filter(|entry| !entry.exchanges.is_empty())
        .map(|entry| CaseEvidence {
            case: entry.case.clone(),
            format: entry.format,
            outcome: outcomes
                .iter()
                .find(|row| row.case == entry.case && row.format == entry.format)
                .cloned(),
            exchanges: entry.exchanges.iter().map(redacted).collect(),
        })
        .collect();
    if cases.is_empty() {
        return Err(EvidenceError::RecordingEmpty {
            asked: selection.describe(),
            selected: selected.len(),
            names: without.join(", "),
        });
    }

    Ok(EvidenceBundle {
        sut: transcript.sut.clone(),
        schedule_release: transcript.schedule_release.clone(),
        selection: selection.clone(),
        without_exchanges: without,
        cases,
    })
}

/// A few of the transcript's case ids, so a refusal shows what was there.
fn sample_of(transcript: &RunTranscript) -> String {
    if transcript.cases.is_empty() {
        return String::from("(the transcript carries no cases at all)");
    }
    let mut names: Vec<&str> = transcript
        .cases
        .iter()
        .take(SAMPLE_IDS)
        .map(|entry| entry.case.as_str())
        .collect();
    if transcript.cases.len() > SAMPLE_IDS {
        names.push("…");
    }
    names.join(", ")
}

/// One exchange with the credential-bearing header values withheld again.
fn redacted(exchange: &RecordedExchange) -> RecordedExchange {
    let mut copy = exchange.clone();
    copy.request.headers = recorded_headers(&copy.request.headers);
    copy.response.headers = recorded_headers(&copy.response.headers);
    copy
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use super::{EvidenceError, EvidenceSelection, assemble};
    use crate::party::{OutcomeRecord, OutcomeStatus};
    use crate::transcript::{
        CaseTranscript, RecordedExchange, RecordedRequest, RecordedResponse, RunTranscript,
    };

    fn case_id(id: &str) -> crate::ids::CaseId {
        crate::ids::CaseId::parse(id).expect("a well-formed case id")
    }

    /// One exchange carrying a live credential, so a redaction test has
    /// something to catch.
    fn exchange(seq: u32) -> RecordedExchange {
        let mut headers = BTreeMap::new();
        headers.insert(
            String::from("Authorization"),
            String::from("Basic dXNlcjpwYXNz"),
        );
        headers.insert(String::from("Accept"), String::from("application/json"));
        RecordedExchange {
            seq,
            row: 0,
            request: RecordedRequest {
                method: String::from("GET"),
                url: String::from("http://sut.invalid/ehr"),
                headers,
                body: None,
            },
            response: RecordedResponse {
                status: 500,
                headers: BTreeMap::new(),
                body: None,
            },
        }
    }

    fn entry(id: &str, exchanges: usize) -> CaseTranscript {
        CaseTranscript {
            case: case_id(id),
            format: None,
            exchanges: (1..=exchanges)
                .map(|i| exchange(u32::try_from(i).unwrap_or(1)))
                .collect(),
        }
    }

    fn transcript(cases: Vec<CaseTranscript>) -> RunTranscript {
        RunTranscript {
            sut: crate::party::Sut {
                name: String::from("example-cdr"),
                version: String::from("0.0.0"),
            },
            schedule_release: String::from("cnf-2.0-w2"),
            cases,
        }
    }

    fn row(id: &str, status: OutcomeStatus) -> OutcomeRecord {
        OutcomeRecord {
            case: case_id(id),
            format: None,
            status,
            rows_driven: 1,
            rows_total: 1,
            failing_step: None,
            reason: Some(String::from("expected 201, observed 500")),
            citation: None,
            failed_rows: Vec::new(),
        }
    }

    const RED: &str = "I_EHR_SERVICE.create_ehr-main";
    const GREEN: &str = "I_EHR_SERVICE.get_ehr-main";

    /// The red rows of a results record select their own exchanges, and the
    /// outcome row travels with them.
    #[test]
    fn the_failing_statuses_select_their_own_exchanges() {
        let document = transcript(vec![entry(RED, 2), entry(GREEN, 1)]);
        let outcomes = vec![
            row(RED, OutcomeStatus::Failed),
            row(GREEN, OutcomeStatus::Passed),
        ];
        let selection = EvidenceSelection {
            statuses: vec![OutcomeStatus::Failed, OutcomeStatus::Errored],
            ..EvidenceSelection::default()
        };
        let bundle = assemble(&document, &outcomes, &selection).expect("the red row was recorded");
        assert_eq!(bundle.cases.len(), 1);
        assert_eq!(bundle.exchange_count(), 2);
        let case = bundle.cases.first().expect("one case");
        assert_eq!(case.case.as_str(), RED);
        assert_eq!(
            case.outcome.as_ref().map(|o| o.status),
            Some(OutcomeStatus::Failed),
            "the bundle carries the row the exchanges are compared against"
        );
        assert_eq!(bundle.sut.name, "example-cdr");
        assert!(bundle.without_exchanges.is_empty());
    }

    /// A selection naming nothing that exists is refused, and the refusal
    /// states both what was asked and what the transcript actually carries.
    #[test]
    fn a_selection_matching_nothing_is_refused_with_both_sides_named() {
        let document = transcript(vec![entry(RED, 1)]);
        let selection = EvidenceSelection {
            only: vec![String::from("I_EHR_SERVICE.no_such_case-main")],
            ..EvidenceSelection::default()
        };
        let error =
            assemble(&document, &[], &selection).expect_err("no such case is in the transcript");
        assert!(
            matches!(&error, EvidenceError::NothingMatched { available, .. } if *available == 1),
            "{error}"
        );
        let text = error.to_string();
        assert!(text.contains("no_such_case"), "{text}");
        assert!(
            text.contains(RED),
            "the refusal shows what was there: {text}"
        );
    }

    /// Cases that matched but recorded nothing are refused rather than
    /// emitted as a bundle of the right shape with no content in it.
    #[test]
    fn matched_cases_with_no_recording_are_refused() {
        let document = transcript(vec![entry(RED, 0)]);
        let selection = EvidenceSelection {
            only: vec![String::from(RED)],
            ..EvidenceSelection::default()
        };
        let error = assemble(&document, &[], &selection).expect_err("nothing was recorded");
        assert!(
            matches!(&error, EvidenceError::RecordingEmpty { selected, .. } if *selected == 1),
            "{error}"
        );
        assert!(error.to_string().contains(RED), "{error}");
    }

    /// A selection naming nothing at all is refused before anything is read:
    /// the unfiltered document is the transcript the caller already has.
    #[test]
    fn an_empty_selection_is_refused() {
        let document = transcript(vec![entry(RED, 1)]);
        let error = assemble(&document, &[], &EvidenceSelection::default())
            .expect_err("no selector was passed");
        assert_eq!(error, EvidenceError::SelectionEmpty);
    }

    /// A half-matched selection still exports, and names its misses in the
    /// document so the gap is not silent.
    #[test]
    fn a_partly_recorded_selection_names_what_it_could_not_carry() {
        let document = transcript(vec![entry(RED, 1), entry(GREEN, 0)]);
        let selection = EvidenceSelection {
            only: vec![String::from(RED), String::from(GREEN)],
            ..EvidenceSelection::default()
        };
        let bundle = assemble(&document, &[], &selection).expect("one of the two was recorded");
        assert_eq!(bundle.cases.len(), 1);
        assert_eq!(bundle.without_exchanges, vec![String::from(GREEN)]);
    }

    /// The credential is withheld by the export itself, whatever the input
    /// document carried.
    #[test]
    fn the_authorization_value_never_reaches_the_bundle() {
        let document = transcript(vec![entry(RED, 1)]);
        let selection = EvidenceSelection {
            filter: Some(String::from("create_ehr")),
            ..EvidenceSelection::default()
        };
        let bundle = assemble(&document, &[], &selection).expect("the filter matches");
        let headers = &bundle
            .cases
            .first()
            .expect("one case")
            .exchanges
            .first()
            .expect("one exchange")
            .request
            .headers;
        assert_eq!(
            headers.get("authorization").map(String::as_str),
            Some(crate::transcript::REDACTED)
        );
        assert!(
            !headers.values().any(|v| v.contains("dXNlcjpwYXNz")),
            "the credential leaked: {headers:?}"
        );
        assert_eq!(
            headers.get("accept").map(String::as_str),
            Some("application/json"),
            "every other header lands verbatim"
        );
    }

    /// The refusal quotes the selection back in the words the operator typed.
    #[test]
    fn the_selection_describes_itself_in_the_operators_own_terms() {
        let selection = EvidenceSelection {
            only: vec![String::from(RED)],
            filter: Some(String::from("ehr")),
            statuses: vec![OutcomeStatus::Failed],
        };
        assert_eq!(
            selection.describe(),
            format!("--only {RED} + --filter ehr + status failed")
        );
        assert_eq!(EvidenceSelection::default().describe(), "nothing");
    }
}
