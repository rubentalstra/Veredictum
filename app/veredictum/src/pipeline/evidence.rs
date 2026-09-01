// SPDX-FileCopyrightText: Veredictum contributors
// SPDX-License-Identifier: Apache-2.0

//! Exporting a finished run's recorded exchanges for a named set of cases.
//!
//! The seam reads two documents a run already wrote — its `transcript.json`
//! and, when the selection needs one, its `results.json` — and hands back the
//! bundle [`crate::evidence`] assembles from them. It reads no statement: a
//! claim is what a party publishes, and whether a recorded exchange can be
//! read has nothing to do with whether anyone claimed anything over it.

use std::path::Path;

use crate::evidence::{EvidenceBundle, EvidenceSelection, assemble};
use crate::party::{OutcomeStatus, Results};
use crate::pipeline::{Error, read_json};
use crate::transcript::RunTranscript;

/// The statuses `--failing` selects: the red rows of a run.
///
/// A failed row is a defect somewhere; an errored row is inconclusive, and
/// its exchanges are what decide which of the three suspects it belongs to.
pub const RED_STATUSES: [OutcomeStatus; 2] = [OutcomeStatus::Failed, OutcomeStatus::Errored];

/// Which run to export from, and which of its cases.
#[derive(Debug)]
pub struct EvidenceRequest<'a> {
    /// The run's `transcript.json`.
    pub transcript: &'a Path,
    /// The run's `results.json`, when the export needs its rows.
    pub results: Option<&'a Path>,
    /// Export these cases, by id.
    pub only: &'a [String],
    /// Export cases whose id contains this substring.
    pub filter: Option<&'a str>,
    /// Export the red rows the results record names.
    pub failing: bool,
}

/// Reads a finished run's documents and carves the selected evidence out.
///
/// # Errors
/// [`Error::Read`] or [`Error::Parse`] for the transcript and results
/// documents, [`Error::Selector`] when `--failing` was asked for without a
/// results record to read the red rows from, and [`Error::Evidence`] when the
/// bundle would carry nothing.
pub fn export_evidence(request: &EvidenceRequest<'_>) -> Result<EvidenceBundle, Error> {
    if request.failing && request.results.is_none() {
        return Err(Error::Selector(String::from(
            "--failing needs --results: the red rows are named by the run's results record",
        )));
    }
    let transcript: RunTranscript = read_json(request.transcript, "transcript")?;
    let results: Option<Results> = match request.results {
        None => None,
        Some(path) => Some(read_json(path, "results")?),
    };
    let mut only: Vec<String> = request.only.to_vec();
    only.sort_unstable();
    only.dedup();
    let selection = EvidenceSelection {
        only,
        filter: request.filter.map(ToOwned::to_owned),
        statuses: if request.failing {
            RED_STATUSES.to_vec()
        } else {
            Vec::new()
        },
    };
    let outcomes = results.as_ref().map_or(&[][..], |r| r.outcomes.as_slice());
    Ok(assemble(&transcript, outcomes, &selection)?)
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use super::{EvidenceRequest, export_evidence};
    use crate::pipeline::Error;

    /// `--failing` with no results record is refused as a selector fault,
    /// before either document is opened.
    #[test]
    fn failing_without_a_results_record_is_refused_as_a_selector_fault() {
        let request = EvidenceRequest {
            transcript: Path::new("/nonexistent/transcript.json"),
            results: None,
            only: &[],
            filter: None,
            failing: true,
        };
        let error = export_evidence(&request).expect_err("there is no record to read rows from");
        assert!(matches!(&error, Error::Selector(_)), "{error}");
        assert!(error.to_string().contains("--results"), "{error}");
    }
}
