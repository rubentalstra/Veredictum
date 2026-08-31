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

use crate::party::{OutcomeRecord, OutcomeStatus, Results, SelectionBasis, Statement};
use crate::pipeline::conformance::{OutcomeCounts, RunWarning, Selection};
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
    /// The party statement the re-judgement applies ISO/IEC 9646 test
    /// selection with: it decides which option arm, extension route, claimed
    /// capability and release floor the replay selects. With none the replay
    /// re-derives a whole-catalogue sweep and stamps `selection_basis:
    /// statement_blind`, which [`selection_agreement`] refuses to hold
    /// against a record an ICS selected.
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
/// `warn` receives everything the re-judgement reports that is not a failure,
/// in the order it happens. A replay driven with no statement raises the same
/// blind-selection advisory a live run raises, because it stamps the same
/// `selection_basis` on the document it emits.
///
/// # Errors
/// [`Error::Catalogue`] or [`Error::Artifacts`] when the tree does not load,
/// [`Error::Read`] or [`Error::Parse`] for the ixit, statement and transcript
/// documents, [`Error::Instrument`] for an interpreter defect, and
/// [`Error::RecordedInvariants`] when the re-judged record violates its own
/// invariants.
pub fn replay_run(
    request: &ReplayRequest<'_>,
    warn: &dyn Fn(RunWarning<'_>),
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
    let selection = Selection::of(statement.as_ref(), &report.unestablished);
    if let Some(advisory) = selection.advisory() {
        warn(advisory);
    }
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
        selection_basis: Some(selection.basis()),
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

/// Whether a re-judgement applied the claim the record it is held against
/// was selected under.
///
/// A `results.json` identifies that claim by two recorded facts and no
/// others: `selection_basis` says whether an ICS selected the campaign at
/// all, and `tech_profile.formats` is the its-rest format list the statement
/// declared (every format, when nothing selected the campaign). Nothing in
/// the document names the statement itself, so two different statements
/// declaring the same formats are one value here.
// TODO(#490): record a statement identity so a re-derivation proves it
// re-applied the same claim rather than a compatible one.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SelectionAgreement<'a> {
    /// The record and the re-judgement were selected under the same recorded
    /// facts.
    Same,
    /// The record carries no `selection_basis`, so it identifies nothing
    /// about what selected it. Reported and never refused: absence is
    /// unknown, and reading it as either basis invents a fact the document
    /// does not carry.
    Unidentified,
    /// One side had an ICS to select with and the other did not.
    DifferentBasis {
        /// What the record says selected it.
        submitted: SelectionBasis,
        /// What selected the re-judgement.
        rederived: SelectionBasis,
    },
    /// Both record the same basis, and the its-rest wire formats they carry
    /// differ, which two statements declaring the same profile cannot do.
    DifferentFormats {
        /// The formats the record carries.
        submitted: &'a [crate::vocab::FormatName],
        /// The formats the re-judgement derived from its statement.
        rederived: &'a [crate::vocab::FormatName],
    },
}

impl SelectionAgreement<'_> {
    /// Whether this agreement refuses the comparison, because a re-judgement
    /// selected under a different claim re-derives something other than the
    /// record it was held against.
    #[must_use]
    pub fn refuses(self) -> bool {
        matches!(
            self,
            SelectionAgreement::DifferentBasis { .. } | SelectionAgreement::DifferentFormats { .. }
        )
    }
}

impl std::fmt::Display for SelectionAgreement<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match *self {
            SelectionAgreement::Same => f.write_str(
                "the re-judgement applied the claim the record was selected under",
            ),
            SelectionAgreement::Unidentified => f.write_str(
                "the record carries no selection_basis, so nothing in it identifies what selected it",
            ),
            SelectionAgreement::DifferentBasis {
                submitted,
                rederived,
            } => write!(
                f,
                "the record was selected with {}, and this re-judgement with {}",
                submitted.token(),
                rederived.token()
            ),
            SelectionAgreement::DifferentFormats {
                submitted,
                rederived,
            } => write!(
                f,
                "the record carries the its-rest formats {}, and this re-judgement derived {}",
                format_list(submitted),
                format_list(rederived)
            ),
        }
    }
}

/// Whether the re-judgement applied the record's own claim.
///
/// The two recorded facts are compared in order: an absent `selection_basis`
/// on either side identifies nothing, a differing basis is decisive on its
/// own, and only then do the declared formats separate two statements.
#[must_use]
pub fn selection_agreement<'a>(
    submitted: &'a Results,
    rederived: &'a Results,
) -> SelectionAgreement<'a> {
    match (submitted.selection_basis, rederived.selection_basis) {
        (None, _) | (_, None) => SelectionAgreement::Unidentified,
        (Some(recorded), Some(replayed)) if recorded != replayed => {
            SelectionAgreement::DifferentBasis {
                submitted: recorded,
                rederived: replayed,
            }
        }
        (Some(_), Some(_)) => {
            if submitted.tech_profile.formats == rederived.tech_profile.formats {
                SelectionAgreement::Same
            } else {
                SelectionAgreement::DifferentFormats {
                    submitted: &submitted.tech_profile.formats,
                    rederived: &rederived.tech_profile.formats,
                }
            }
        }
    }
}

/// A declared format list, as one comma-separated run of tokens.
fn format_list(formats: &[crate::vocab::FormatName]) -> String {
    if formats.is_empty() {
        return String::from("none");
    }
    formats
        .iter()
        .map(|format| format.token())
        .collect::<Vec<_>>()
        .join(", ")
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
#[cfg(test)]
mod tests {
    use crate::vocab::FormatName;

    use super::*;

    /// A record selected under `basis`, declaring `formats` on its-rest.
    fn record(basis: Option<SelectionBasis>, formats: &[FormatName]) -> Results {
        Results {
            sut: crate::party::Sut {
                name: String::from("selection-gate"),
                version: String::from("0"),
            },
            runner: crate::party::Runner {
                name: String::from("veredictum"),
                version: String::from("0"),
                verification_pack_status: crate::party::VerificationPackStatus::Passed,
            },
            schedule_release: String::from("cnf-2.0-w2"),
            tech_profile: crate::party::TechProfile {
                its: crate::vocab::ItsName::ItsRest,
                formats: formats.to_vec(),
            },
            ixit_digest: String::from("0"),
            selection_basis: basis,
            restapi_specs_version: None,
            outcomes: Vec::new(),
            measurements: Vec::new(),
            ambiguity_dispositions: Vec::new(),
        }
    }

    #[test]
    fn the_same_recorded_facts_agree() {
        let submitted = record(
            Some(SelectionBasis::Statement),
            &[FormatName::CanonicalJson],
        );
        let rederived = record(
            Some(SelectionBasis::Statement),
            &[FormatName::CanonicalJson],
        );
        let agreement = selection_agreement(&submitted, &rederived);
        assert_eq!(agreement, SelectionAgreement::Same);
        assert!(!agreement.refuses());
    }

    /// The case the re-derivation gate turns on: a record an ICS selected,
    /// re-judged with no statement at all. Those rows come out of a sweep of
    /// the whole catalogue, which is not the campaign the record describes.
    #[test]
    fn a_blind_replay_of_a_selected_record_is_refused() {
        let submitted = record(
            Some(SelectionBasis::Statement),
            &[FormatName::CanonicalJson],
        );
        let rederived = record(Some(SelectionBasis::StatementBlind), FormatName::ALL);
        let agreement = selection_agreement(&submitted, &rederived);
        assert_eq!(
            agreement,
            SelectionAgreement::DifferentBasis {
                submitted: SelectionBasis::Statement,
                rederived: SelectionBasis::StatementBlind,
            }
        );
        assert!(agreement.refuses());
    }

    /// The mirror: a blind record re-judged under somebody's claim.
    #[test]
    fn a_selected_replay_of_a_blind_record_is_refused() {
        let submitted = record(Some(SelectionBasis::StatementBlind), FormatName::ALL);
        let rederived = record(
            Some(SelectionBasis::Statement),
            &[FormatName::CanonicalJson],
        );
        assert!(selection_agreement(&submitted, &rederived).refuses());
    }

    /// Two statements that each selected their campaign, declaring different
    /// its-rest formats, are two different claims.
    #[test]
    fn two_statements_declaring_different_formats_are_refused() {
        let submitted = record(
            Some(SelectionBasis::Statement),
            &[FormatName::CanonicalJson],
        );
        let rederived = record(Some(SelectionBasis::Statement), &[FormatName::CanonicalXml]);
        let agreement = selection_agreement(&submitted, &rederived);
        assert!(agreement.refuses(), "{agreement}");
        let rendered = agreement.to_string();
        assert!(
            rendered.contains(FormatName::CanonicalJson.token()),
            "{rendered}"
        );
        assert!(
            rendered.contains(FormatName::CanonicalXml.token()),
            "{rendered}"
        );
    }

    /// A record written before `selection_basis` existed identifies nothing,
    /// and absence is unknown rather than either basis: reading it as
    /// `statement` credits a blind sweep with a scope it never had, and
    /// reading it as blind refuses every honest re-derivation of an older
    /// record.
    #[test]
    fn a_record_predating_the_recorded_basis_is_reported_not_refused() {
        let submitted = record(None, &[FormatName::CanonicalJson]);
        let rederived = record(
            Some(SelectionBasis::Statement),
            &[FormatName::CanonicalJson],
        );
        let agreement = selection_agreement(&submitted, &rederived);
        assert_eq!(agreement, SelectionAgreement::Unidentified);
        assert!(!agreement.refuses());
        assert!(
            agreement.to_string().contains("selection_basis"),
            "the report names the member the record is missing: {agreement}"
        );
    }
}
