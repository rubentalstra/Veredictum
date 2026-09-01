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
use crate::pipeline::conformance::{OutcomeCounts, RecordedCampaign, RunWarning, Selection};
use crate::pipeline::{Error, load_clean_root, load_ixit, load_statement, read_json};
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
    /// against a record an ICS selected. The emitted document names this file
    /// by its digest, so [`selection_agreement`] also refuses a re-judgement
    /// handed a different statement that happens to declare the same formats.
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
    let selected_under: Option<(Statement, String)> = match request.statement {
        None => None,
        Some(path) => Some(load_statement(path)?),
    };
    let statement = selected_under.as_ref().map(|(declared, _)| declared);
    if let (Some(declared), Some((_, register))) = (statement, &set.register) {
        let gaps = crate::verdict::option_family_gaps(
            declared,
            set.cases.iter().map(|(_, case)| case),
            register,
        );
        if !gaps.is_empty() {
            warn(RunWarning::OptionFamilySelection { gaps: &gaps });
        }
    }
    let transcript: RunTranscript = read_json(request.transcript, "transcript")?;
    let report = crate::run::replay(&set, &ixit, statement, &transcript, progress)
        .map_err(|e| Error::Instrument(format!("replay defect: {e}")))?;
    let selection = Selection::of(
        selected_under
            .as_ref()
            .map(|(declared, text)| (declared, text.as_str())),
        &report.unestablished,
    );
    if let Some(advisory) = selection.advisory() {
        warn(advisory);
    }
    let outcomes: Vec<OutcomeRecord> = report.records.iter().map(OutcomeRecord::from).collect();
    let counts = tally(&outcomes);
    let results = RecordedCampaign {
        sut: transcript.sut.clone(),
        schedule_release: transcript.schedule_release.clone(),
        selection,
        register: set.register.as_ref().map(|(_, register)| register),
        ixit_text: &ixit_text,
        restapi_specs_version: report.restapi_specs_version.clone(),
        outcomes,
        measurements: Vec::new(),
    }
    .into_results();
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
/// A `results.json` identifies that claim by three recorded facts:
/// `selection_basis` says whether an ICS selected the campaign at all,
/// `statement_digest` names the statement itself, and `tech_profile.formats`
/// is the its-rest format list that statement declared (every format, when
/// nothing selected the campaign).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SelectionAgreement<'a> {
    /// The record and the re-judgement were selected under the same recorded
    /// facts.
    Same,
    /// The record's own provenance members cannot both be true, so no
    /// re-judgement reconstructs the campaign it describes. Refused: there is
    /// no claim to re-apply, and holding a re-derivation against it would
    /// publish agreement with a record that contradicts itself.
    RecordContradictsItself {
        /// Which two members cannot both be true.
        finding: crate::party::ProvenanceContradiction,
    },
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
    /// The record names no statement, so nothing in it identifies the claim
    /// it was selected under. Reported and never refused: a document written
    /// before `statement_digest` existed carries no identity to disagree
    /// with.
    RecordNamesNoStatement {
        /// The statement this re-judgement applied.
        rederived: &'a str,
    },
    /// The record names its statement and the re-judgement names none, so the
    /// pair identifies nothing either. Reported the same way, and it names
    /// the record's own value so a reader knows which declaration to pass.
    ReplayNamesNoStatement {
        /// The statement the record was selected under.
        submitted: &'a str,
    },
    /// The record and the re-judgement name two different statements, which
    /// no pair of digests over the same declaration can do.
    DifferentStatement {
        /// The statement digest the record carries.
        submitted: &'a str,
        /// The statement digest of the declaration this re-judgement was
        /// handed.
        rederived: &'a str,
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
            SelectionAgreement::RecordContradictsItself { .. }
                | SelectionAgreement::DifferentBasis { .. }
                | SelectionAgreement::DifferentStatement { .. }
                | SelectionAgreement::DifferentFormats { .. }
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
            SelectionAgreement::RecordContradictsItself { finding } => write!(
                f,
                "the record contradicts itself, so no re-judgement reconstructs the campaign it describes: {finding}"
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
            SelectionAgreement::RecordNamesNoStatement { rederived } => write!(
                f,
                "the record carries no statement_digest, so nothing in it names the statement it was selected under; this re-judgement applied {rederived}"
            ),
            SelectionAgreement::ReplayNamesNoStatement { submitted } => write!(
                f,
                "the record names statement {submitted}, and this re-judgement carries no statement_digest to hold against it"
            ),
            SelectionAgreement::DifferentStatement {
                submitted,
                rederived,
            } => write!(
                f,
                "the record was selected under statement {submitted}, and this re-judgement under {rederived}"
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
/// The recorded facts are compared in order: the record's own members must
/// first be consistent with each other, then an absent `selection_basis` on
/// either side identifies nothing, a differing basis is decisive on its own,
/// then the named statement separates two claims outright, and the declared
/// formats are what is left when neither side names a statement.
///
/// A campaign nothing selected names no statement, so a pair of blind
/// documents carries no digest on either side and falls through to the
/// formats. One side alone naming a statement is reported rather than
/// refused: a record written before the member existed carries no identity,
/// and refusing it would refuse every honest re-derivation of an older
/// record. Only the SUBMITTED record is read for self-consistency, because the
/// re-judgement is assembled by
/// [`crate::pipeline::conformance::RecordedCampaign`] and cannot contradict
/// itself.
#[must_use]
pub fn selection_agreement<'a>(
    submitted: &'a Results,
    rederived: &'a Results,
) -> SelectionAgreement<'a> {
    if let Some(finding) = submitted.provenance_contradiction() {
        return SelectionAgreement::RecordContradictsItself { finding };
    }
    match (submitted.selection_basis, rederived.selection_basis) {
        (None, _) | (_, None) => SelectionAgreement::Unidentified,
        (Some(recorded), Some(replayed)) if recorded != replayed => {
            SelectionAgreement::DifferentBasis {
                submitted: recorded,
                rederived: replayed,
            }
        }
        (Some(_), Some(_)) => statement_agreement(submitted, rederived),
    }
}

/// The statement identity, then the declared formats, for a pair the basis
/// already agrees on.
fn statement_agreement<'a>(
    submitted: &'a Results,
    rederived: &'a Results,
) -> SelectionAgreement<'a> {
    match (
        submitted.statement_digest.as_deref(),
        rederived.statement_digest.as_deref(),
    ) {
        (Some(recorded), Some(applied)) if recorded != applied => {
            SelectionAgreement::DifferentStatement {
                submitted: recorded,
                rederived: applied,
            }
        }
        (None, Some(applied)) => SelectionAgreement::RecordNamesNoStatement { rederived: applied },
        (Some(recorded), None) => SelectionAgreement::ReplayNamesNoStatement {
            submitted: recorded,
        },
        _ => {
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

    /// A record selected under `basis`, declaring `formats` on its-rest, and
    /// naming no statement.
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
            tech_profile: crate::party::RecordedTechProfile {
                its: crate::vocab::ItsName::ItsRest,
                formats: formats.to_vec(),
                source: None,
            },
            ixit_digest: String::from("0"),
            statement_digest: None,
            selection_basis: basis,
            restapi_specs_version: None,
            outcomes: Vec::new(),
            measurements: Vec::new(),
            ambiguity_dispositions: Vec::new(),
        }
    }

    /// The same record, naming the statement `digest` selected it.
    fn selected_under(digest: &str, formats: &[FormatName]) -> Results {
        Results {
            statement_digest: Some(digest.to_owned()),
            ..record(Some(SelectionBasis::Statement), formats)
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

    /// The defect #490 closes: two statements that declare the same its-rest
    /// formats are one value to every other recorded fact, so only the named
    /// statement separates a re-derivation of this record from a run under
    /// somebody else's claim.
    #[test]
    fn two_statements_declaring_the_same_formats_are_told_apart() {
        let submitted = selected_under("aedf8eec255f7847", &[FormatName::CanonicalJson]);
        let rederived = selected_under("a2ee0218f6815be4", &[FormatName::CanonicalJson]);
        let agreement = selection_agreement(&submitted, &rederived);
        assert_eq!(
            agreement,
            SelectionAgreement::DifferentStatement {
                submitted: "aedf8eec255f7847",
                rederived: "a2ee0218f6815be4",
            }
        );
        assert!(agreement.refuses());
        let rendered = agreement.to_string();
        assert!(rendered.contains("aedf8eec255f7847"), "{rendered}");
        assert!(rendered.contains("a2ee0218f6815be4"), "{rendered}");
    }

    /// The same statement re-applied agrees, which is what a re-derivation of
    /// a published record has to be able to say.
    #[test]
    fn the_same_statement_re_applied_agrees() {
        let submitted = selected_under("aedf8eec255f7847", &[FormatName::CanonicalJson]);
        let rederived = selected_under("aedf8eec255f7847", &[FormatName::CanonicalJson]);
        let agreement = selection_agreement(&submitted, &rederived);
        assert_eq!(agreement, SelectionAgreement::Same);
        assert!(!agreement.refuses());
    }

    /// A record written before `statement_digest` existed names no statement,
    /// so a re-judgement under any claim is reported and never refused:
    /// manufacturing a refusal there would refuse every honest re-derivation
    /// of an older record.
    #[test]
    fn a_record_predating_the_statement_digest_is_reported_not_refused() {
        let submitted = record(
            Some(SelectionBasis::Statement),
            &[FormatName::CanonicalJson],
        );
        let rederived = selected_under("aedf8eec255f7847", &[FormatName::CanonicalJson]);
        let agreement = selection_agreement(&submitted, &rederived);
        assert_eq!(
            agreement,
            SelectionAgreement::RecordNamesNoStatement {
                rederived: "aedf8eec255f7847",
            }
        );
        assert!(!agreement.refuses());
        let rendered = agreement.to_string();
        assert!(rendered.contains("statement_digest"), "{rendered}");
        assert!(rendered.contains("aedf8eec255f7847"), "{rendered}");
    }

    /// The mirror: a record naming its statement, held against a re-judgement
    /// that names none. Reported, and the record's own value is said out loud
    /// so a reader knows which declaration to pass.
    #[test]
    fn a_re_judgement_naming_no_statement_is_reported_against_a_record_that_does() {
        let submitted = selected_under("aedf8eec255f7847", &[FormatName::CanonicalJson]);
        let rederived = record(
            Some(SelectionBasis::Statement),
            &[FormatName::CanonicalJson],
        );
        let agreement = selection_agreement(&submitted, &rederived);
        assert_eq!(
            agreement,
            SelectionAgreement::ReplayNamesNoStatement {
                submitted: "aedf8eec255f7847",
            }
        );
        assert!(!agreement.refuses());
        assert!(
            agreement.to_string().contains("aedf8eec255f7847"),
            "{agreement}"
        );
    }

    /// A campaign nothing selected names no statement on either side, and
    /// that absence is the honest record rather than an unknown: the pair
    /// agrees on the formats a blind sweep covers.
    #[test]
    fn two_blind_campaigns_name_no_statement_and_still_agree() {
        let submitted = record(Some(SelectionBasis::StatementBlind), FormatName::ALL);
        let rederived = record(Some(SelectionBasis::StatementBlind), FormatName::ALL);
        assert_eq!(
            selection_agreement(&submitted, &rederived),
            SelectionAgreement::Same
        );
    }

    /// A record whose own provenance members cannot both be true is refused
    /// before anything is compared: a campaign nothing selected had no
    /// declaration to read a technology profile from, so no re-judgement
    /// reconstructs the campaign the document describes.
    #[test]
    fn a_record_contradicting_its_own_provenance_is_refused() {
        let submitted = Results {
            tech_profile: crate::party::RecordedTechProfile {
                source: Some(crate::party::TechProfileSource::Declared),
                ..record(Some(SelectionBasis::StatementBlind), FormatName::ALL).tech_profile
            },
            ..record(Some(SelectionBasis::StatementBlind), FormatName::ALL)
        };
        let rederived = record(Some(SelectionBasis::StatementBlind), FormatName::ALL);
        let agreement = selection_agreement(&submitted, &rederived);
        assert_eq!(
            agreement,
            SelectionAgreement::RecordContradictsItself {
                finding: crate::party::ProvenanceContradiction::DeclaredProfileWithoutSelection,
            }
        );
        assert!(agreement.refuses());
        let rendered = agreement.to_string();
        assert!(rendered.contains("tech_profile.source"), "{rendered}");
        assert!(rendered.contains("selection_basis"), "{rendered}");
    }

    /// A record written before `tech_profile.source` existed carries nothing to
    /// contradict, so it is judged on the members it does carry and re-derives
    /// as it always did.
    #[test]
    fn a_record_predating_the_profile_source_still_re_derives() {
        let submitted = record(
            Some(SelectionBasis::Statement),
            &[FormatName::CanonicalJson],
        );
        assert_eq!(
            submitted.tech_profile.source, None,
            "the fixture is the pre-member shape"
        );
        let rederived = record(
            Some(SelectionBasis::Statement),
            &[FormatName::CanonicalJson],
        );
        let agreement = selection_agreement(&submitted, &rederived);
        assert_eq!(agreement, SelectionAgreement::Same);
        assert!(!agreement.refuses());
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
