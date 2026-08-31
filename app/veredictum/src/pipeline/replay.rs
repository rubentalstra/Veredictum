// SPDX-FileCopyrightText: Veredictum contributors
// SPDX-License-Identifier: Apache-2.0

//! Re-judging a recorded run, and comparing the answer with what was
//! submitted.
//!
//! A published record claims two things a reader should not have to take on
//! trust: that the recorded exchanges support the outcomes, and that the
//! verdicts follow from the outcomes. The second is arithmetic anybody can
//! repeat by running `verdicts` again. The first is this module: the
//! catalogue is driven once more with the recording standing in for the
//! server, through the same composition, classification and assertion
//! evaluators the live run used, and the outcomes it reaches are compared
//! with the outcomes the submission carries.
//!
//! What that establishes is precise: the judgement follows from the
//! evidence. It does not establish the evidence, because a transcript is
//! what the instrument says it sent and received.

use std::path::Path;

use crate::party::{OutcomeRecord, OutcomeStatus, Results, Statement};
use crate::pipeline::conformance::OutcomeCounts;
use crate::pipeline::{Error, load_clean_root, load_ixit, read_json};
use crate::run::RunReport;
use crate::transcript::RunTranscript;

/// What to re-judge, and against which catalogue.
#[derive(Debug)]
pub struct ReplayRequest<'a> {
    /// The artifact root.
    pub root: &'a Path,
    /// The ixit topology the recorded run was driven under.
    pub ixit: &'a Path,
    /// The transcript of that run.
    pub transcript: &'a Path,
    /// The party statement the run was selected against, when it had one.
    pub statement: Option<&'a Path>,
    /// Re-judge only cases whose id contains this substring.
    pub filter: Option<&'a str>,
    /// Re-judge only these cases, by id.
    ///
    /// A re-derivation answers one question — do the rows this record claims
    /// follow from the exchanges it carries — so it drives the record's own
    /// cases and no others. Driving the whole catalogue instead would reach a
    /// case the run never selected, find no recording for it, and refuse a
    /// submission for evidence it never claimed to have.
    pub only: Option<&'a [String]>,
}

/// The re-judged run.
#[derive(Debug)]
pub struct ReplayOutcome {
    /// The results document the recording supports.
    pub results: Results,
    /// The interpreter's own report over the replay.
    pub report: RunReport,
    /// The re-judged outcomes, tallied by status.
    pub counts: OutcomeCounts,
}

/// One row on which a submitted record and its re-judgement disagree.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Divergence {
    /// The case the row is about.
    pub case: String,
    /// The wire format, when the case is format-parameterized.
    pub format: Option<String>,
    /// What the submitted record says.
    pub submitted: String,
    /// What re-judging the recorded exchanges says.
    pub rederived: String,
}

impl std::fmt::Display for Divergence {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let format = self
            .format
            .as_ref()
            .map_or_else(String::new, |name| format!(" [{name}]"));
        write!(
            f,
            "{}{format}: submitted {}, re-derived {}",
            self.case, self.submitted, self.rederived
        )
    }
}

/// Re-judges a recorded run from its transcript.
///
/// The SUT identity and the schedule release come from the transcript, so a
/// replay needs no facts the submission did not carry.
///
/// # Errors
/// [`Error::Catalogue`] or [`Error::Artifacts`] when the tree does not load,
/// [`Error::Read`] or [`Error::Parse`] for the ixit, statement and transcript
/// documents, [`Error::Instrument`] for an interpreter defect, and
/// [`Error::RecordedInvariants`] when the re-judged record violates its own
/// invariants.
pub fn replay_run(
    request: &ReplayRequest<'_>,
    progress: &mut dyn FnMut(crate::run::Progress<'_>),
) -> Result<ReplayOutcome, Error> {
    let loaded = load_clean_root(request.root)?;
    let (ixit, ixit_text) = load_ixit(request.ixit)?;
    let mut set = loaded.set;
    if let Some(needle) = request.filter {
        set.cases.retain(|(_, c)| c.id.as_str().contains(needle));
    }
    if let Some(ids) = request.only {
        set.cases
            .retain(|(_, c)| ids.iter().any(|id| id == c.id.as_str()));
    }
    let statement: Option<Statement> = match request.statement {
        None => None,
        Some(path) => Some(read_json(path, "statement")?),
    };
    let transcript: RunTranscript = read_json(request.transcript, "transcript")?;
    let report = crate::run::replay(&set, &ixit, statement.as_ref(), &transcript, progress)
        .map_err(|e| Error::Instrument(format!("replay defect: {e}")))?;
    let outcomes: Vec<OutcomeRecord> = report.records.iter().map(OutcomeRecord::from).collect();
    let counts = tally(&outcomes);
    let results = Results {
        sut: transcript.sut.clone(),
        runner: crate::party::Runner {
            name: "veredictum".to_owned(),
            version: env!("CARGO_PKG_VERSION").to_owned(),
            verification_pack_status: crate::party::VerificationPackStatus::Passed,
        },
        schedule_release: transcript.schedule_release.clone(),
        tech_profile: crate::pipeline::conformance::tech_profile(statement.as_ref()),
        ixit_digest: crate::pipeline::conformance::ixit_digest(&ixit_text),
        restapi_specs_version: report.restapi_specs_version.clone(),
        outcomes,
        measurements: Vec::new(),
        ambiguity_dispositions: Vec::new(),
    };
    results
        .check_invariants()
        .map_err(Error::RecordedInvariants)?;
    Ok(ReplayOutcome {
        results,
        report,
        counts,
    })
}

/// Every row on which the submitted record and the re-judgement disagree.
///
/// Three facts are compared per case and format: the status, and the two
/// row counts that bound it. The reason text is deliberately not compared —
/// a transport failure names what could not be reached, and a replay reaches
/// a recording rather than a server, so identical judgements can carry
/// different words. A submitted row the re-judgement does not reach, and a
/// re-judged row the submission does not carry, are both divergences.
#[must_use]
pub fn divergences(submitted: &Results, rederived: &Results) -> Vec<Divergence> {
    let key = |outcome: &OutcomeRecord| {
        (
            outcome.case.to_string(),
            outcome.format.map(|f| f.token().to_owned()),
        )
    };
    let mut found = Vec::new();
    for outcome in &submitted.outcomes {
        let id = key(outcome);
        match rederived
            .outcomes
            .iter()
            .find(|candidate| key(candidate) == id)
        {
            None => found.push(Divergence {
                case: id.0,
                format: id.1,
                submitted: describe(outcome),
                rederived: String::from("no row: the replay never reached this case"),
            }),
            Some(again) => {
                if describe(outcome) != describe(again) {
                    found.push(Divergence {
                        case: id.0,
                        format: id.1,
                        submitted: describe(outcome),
                        rederived: describe(again),
                    });
                }
            }
        }
    }
    for outcome in &rederived.outcomes {
        let id = key(outcome);
        if !submitted
            .outcomes
            .iter()
            .any(|candidate| key(candidate) == id)
        {
            found.push(Divergence {
                case: id.0,
                format: id.1,
                submitted: String::from("no row: the record does not carry this case"),
                rederived: describe(outcome),
            });
        }
    }
    found
}

/// The three compared facts of one row, as one comparable sentence.
fn describe(outcome: &OutcomeRecord) -> String {
    format!(
        "{} over {}/{} rows",
        outcome.status.token(),
        outcome.rows_driven,
        outcome.rows_total
    )
}

/// The re-judged outcomes, tallied by status.
fn tally(outcomes: &[OutcomeRecord]) -> OutcomeCounts {
    let mut counts = OutcomeCounts::default();
    for outcome in outcomes {
        match outcome.status {
            OutcomeStatus::Passed => counts.passed += 1,
            OutcomeStatus::Failed => counts.failed += 1,
            OutcomeStatus::Errored => counts.errored += 1,
            OutcomeStatus::Skipped | OutcomeStatus::NotApplicable => counts.not_applicable += 1,
        }
    }
    counts
}
