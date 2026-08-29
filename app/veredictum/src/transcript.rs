// SPDX-FileCopyrightText: Veredictum contributors
// SPDX-License-Identifier: Apache-2.0

//! The run wire transcript: the exchanges a run drove, persisted beside its
//! `results.json` when the operator asks for them (`run --record-exchanges`).
//!
//! NOTE: no openEHR spec governs this — our own design; the attribution law
//! starts from the observed exchange, and without this artifact a triage
//! against a finished run has nothing to read.
//!
//! Two properties make the document safe to hand around and safe to trust.
//! It is a SERIALIZATION of what the driver already holds, so recording adds
//! no wire traffic and cannot change a verdict. And it is ordered by case id,
//! then by the sequence the driver sent, so the same run re-emits the same
//! bytes.
//!
//! The artifact can carry real patient data: a SUT's response body is
//! recorded verbatim, and a test EHR on a live deployment is still somebody's
//! clinical record. It is operator-controlled output, never a log — the one
//! value the recorder withholds is the `authorization` request header, whose
//! credential belongs to the run's environment and to nothing on disk.

#![expect(
    clippy::disallowed_types,
    reason = "dev/verification tooling over JSON artifacts (the catalogue, results, wire \
              exchanges), whose shapes belong to the artifacts and the SUT"
)]

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use serde_json::Value;

/// The transcript's file name, beside the `results.json` it belongs to.
pub const TRANSCRIPT_FILE: &str = "transcript.json";

/// What replaces a credential-bearing request header value.
pub const REDACTED: &str = "«redacted»";

/// The request header whose value never lands in the artifact.
const CREDENTIAL_HEADER: &str = "authorization";

/// Whether a run records the exchanges it drives.
///
/// A closed two-token vocabulary rather than a bare `bool`, so the call sites
/// carrying it from the CLI flag down to the driver read as what they are.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Recording {
    /// The exchanges are classified and dropped (the default).
    #[default]
    Off,
    /// The exchanges are kept, for the transcript artifact.
    On,
}

impl Recording {
    /// Returns whether recording is on.
    #[must_use]
    pub fn is_on(self) -> bool {
        matches!(self, Self::On)
    }
}

impl From<bool> for Recording {
    fn from(on: bool) -> Self {
        if on { Self::On } else { Self::Off }
    }
}

/// One recorded request, as the driver sent it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RecordedRequest {
    /// The HTTP method.
    pub method: String,
    /// The absolute request URL, query string included.
    pub url: String,
    /// The request headers, names lower-cased so the ordering is stable.
    pub headers: BTreeMap<String, String>,
    /// The request body, when the request carried one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub body: Option<Value>,
}

/// One recorded response, as the SUT answered.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RecordedResponse {
    /// The status code.
    pub status: u16,
    /// The response headers, names lower-cased.
    pub headers: BTreeMap<String, String>,
    /// The response body, when the SUT sent one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub body: Option<Value>,
}

/// One exchange of the transcript: what went out, and what came back.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RecordedExchange {
    /// The exchange's ordinal within its case, counting from 1 in the order
    /// the driver sent them — provisioning exchanges included, because a
    /// precondition that went wrong is exactly what a triage needs to see.
    pub seq: u32,
    /// The decision-table row the driver was on when it sent this exchange.
    pub row: u32,
    /// The request.
    pub request: RecordedRequest,
    /// The response.
    pub response: RecordedResponse,
}

/// Every exchange one case×format execution drove.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CaseTranscript {
    /// The case id.
    pub case: crate::ids::CaseId,
    /// The wire format the case ran on, when format-parameterized.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub format: Option<crate::vocab::FormatName>,
    /// The exchanges, in send order.
    pub exchanges: Vec<RecordedExchange>,
}

/// The whole run's transcript.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RunTranscript {
    /// The system under test the exchanges were driven against.
    pub sut: crate::party::Sut,
    /// The schedule release the campaign ran, as the results record spells it.
    pub schedule_release: String,
    /// One entry per case that drove at least one exchange, case-id sorted.
    pub cases: Vec<CaseTranscript>,
}

impl RunTranscript {
    /// Sorts the cases by id and renumbers each case's exchanges from 1, so
    /// two runs of the same campaign emit the same bytes.
    pub fn canonicalize(&mut self) {
        self.cases.sort_by(|a, b| {
            a.case
                .cmp(&b.case)
                .then_with(|| a.format.cmp(&b.format))
                .then_with(|| a.exchanges.len().cmp(&b.exchanges.len()))
        });
        for case in &mut self.cases {
            for (index, exchange) in case.exchanges.iter_mut().enumerate() {
                exchange.seq = u32::try_from(index).unwrap_or(u32::MAX).saturating_add(1);
            }
        }
    }

    /// The total number of recorded exchanges.
    #[must_use]
    pub fn exchange_count(&self) -> usize {
        self.cases.iter().map(|case| case.exchanges.len()).sum()
    }
}

/// Lower-cases the header names and withholds the credential header's value.
///
/// The names are lower-cased because a `BTreeMap` orders by the key it is
/// given, and the composed request headers are spelled in mixed case while a
/// SUT's response headers arrive lower-cased — one casing keeps the two sides
/// of the artifact comparable and the ordering stable.
#[must_use]
pub fn recorded_headers(headers: &BTreeMap<String, String>) -> BTreeMap<String, String> {
    headers
        .iter()
        .map(|(name, value)| {
            let name = name.to_ascii_lowercase();
            if name == CREDENTIAL_HEADER {
                (name, REDACTED.to_owned())
            } else {
                (name, value.clone())
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use super::{
        CaseTranscript, RecordedExchange, RecordedRequest, RecordedResponse, Recording,
        RunTranscript, recorded_headers,
    };

    fn header_map(pairs: &[(&str, &str)]) -> BTreeMap<String, String> {
        pairs
            .iter()
            .map(|(name, value)| ((*name).to_owned(), (*value).to_owned()))
            .collect()
    }

    fn exchange(seq: u32) -> RecordedExchange {
        RecordedExchange {
            seq,
            row: 0,
            request: RecordedRequest {
                method: String::from("GET"),
                url: String::from("http://sut.invalid/ehr"),
                headers: BTreeMap::new(),
                body: None,
            },
            response: RecordedResponse {
                status: 200,
                headers: BTreeMap::new(),
                body: None,
            },
        }
    }

    fn case_id(id: &str) -> crate::ids::CaseId {
        crate::ids::CaseId::parse(id).expect("a well-formed case id")
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

    /// The credential never lands in the artifact, and every other header
    /// does, verbatim.
    #[test]
    fn the_authorization_header_value_is_withheld() {
        let recorded = recorded_headers(&header_map(&[
            ("Authorization", "Basic dXNlcjpwYXNz"),
            ("Content-Type", "application/json"),
        ]));
        assert_eq!(
            recorded.get("authorization").map(String::as_str),
            Some(super::REDACTED)
        );
        assert!(
            !recorded
                .values()
                .any(|value| value.contains("dXNlcjpwYXNz")),
            "the credential leaked: {recorded:?}"
        );
        assert_eq!(
            recorded.get("content-type").map(String::as_str),
            Some("application/json")
        );
    }

    /// Case order is by id, and the sequence numbers are re-derived, so the
    /// same exchanges always render the same document.
    #[test]
    fn canonicalization_orders_by_case_and_renumbers_the_sequence() {
        let mut document = transcript(vec![
            CaseTranscript {
                case: case_id("I_EHR_SERVICE.get_ehr-main"),
                format: None,
                exchanges: vec![exchange(9), exchange(4)],
            },
            CaseTranscript {
                case: case_id("I_EHR_SERVICE.create_ehr-main"),
                format: Some(crate::vocab::FormatName::CanonicalJson),
                exchanges: vec![exchange(7)],
            },
        ]);
        document.canonicalize();
        let ids: Vec<&str> = document.cases.iter().map(|c| c.case.as_str()).collect();
        assert_eq!(
            ids,
            [
                "I_EHR_SERVICE.create_ehr-main",
                "I_EHR_SERVICE.get_ehr-main"
            ]
        );
        let seqs: Vec<u32> = document
            .cases
            .iter()
            .flat_map(|c| c.exchanges.iter().map(|e| e.seq))
            .collect();
        assert_eq!(seqs, [1, 1, 2]);
        assert_eq!(document.exchange_count(), 3);
    }

    /// Serializing and reading back is lossless: the console reads this
    /// document through these very types.
    #[expect(
        clippy::panic_in_result_fn,
        reason = "the Book ch11 Result-returning test shape: assertions panic, plumbing propagates with ? (https://doc.rust-lang.org/book/ch11-01-writing-tests.html)"
    )]
    #[test]
    fn the_document_round_trips() -> Result<(), serde_json::Error> {
        let document = transcript(vec![CaseTranscript {
            case: case_id("I_EHR_SERVICE.create_ehr-main"),
            format: None,
            exchanges: vec![exchange(1)],
        }]);
        let text = serde_json::to_string(&document)?;
        let parsed: RunTranscript = serde_json::from_str(&text)?;
        assert_eq!(parsed, document);
        Ok(())
    }

    #[test]
    fn recording_is_off_by_default() {
        assert_eq!(Recording::default(), Recording::Off);
        assert!(!Recording::default().is_on());
        assert!(Recording::from(true).is_on());
        assert!(!Recording::from(false).is_on());
    }
}
