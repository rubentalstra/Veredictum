// SPDX-FileCopyrightText: Veredictum contributors
// SPDX-License-Identifier: Apache-2.0

//! The party artifacts — the two canonical JSON interchange documents a
//! conformance submission carries, plus their supporting value types.
//!
//! ISO/IEC 9646 splits a conformance submission into the supplier's
//! *statement* (the ICS — Implementation Conformance Statement — declaring
//! what is claimed, plus the `SDoC` self-declaration) and the *results* the
//! test campaign produced. This module models both as typed, round-trippable
//! JSON: [`Statement`] and [`Results`]. Verdicts are never asserted here —
//! they are computed from these two documents by [`crate::verdict`].
//!
//! Unlike the schedule artifacts (which are the published norm and carry
//! bespoke closed grammars), the party artifacts are our own interchange
//! contract, so they use plain `serde` derives with authored-order
//! preservation (`serde_json`'s `preserve_order`).

#![expect(
    clippy::disallowed_types,
    reason = "dev/verification tooling over JSON artifacts (the catalogue, results, wire \
              exchanges), whose shapes belong to the artifacts and the SUT"
)]

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::exec::{CaseRecord, RowOutcome};
use crate::ids::{AmbiguityId, CapabilityName, CaseId, OptionTag};
use crate::vocab::{FormatName, ItsName, Tier};

/// A party-artifact invariant violation.
#[derive(Debug, Error, PartialEq, Eq)]
pub enum PartyError {
    /// A `skipped`/`not_applicable` outcome carries no citation (the
    /// schedule's honesty rule: every non-executed verdict names the spec
    /// text or register entry that excuses it).
    #[error("outcome for case {case} (status {status}) is missing its mandatory citation")]
    MissingCitation {
        /// The offending case id.
        case: String,
        /// The status token that requires a citation.
        status: &'static str,
    },
}

// ── the statement (ICS + SDoC) ──────────────────────────────────────────────

/// The system under test as the supplier identifies it.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Product {
    /// Product/solution name.
    pub name: String,
    /// Product version/release.
    pub version: String,
    /// Vendor/organization.
    pub vendor: String,
    /// A stable product identifier (URI, SKU, …).
    pub identifier: String,
}

/// Declared spec-component versions the SUT implements — the right-hand side
/// of the per-case `applies` version filter. Keyed by the same components as
/// [`crate::model::case::Applies`].
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct SpecVersions {
    /// Reference Model version.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rm: Option<String>,
    /// BASE version.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub base: Option<String>,
    /// Archetype Model version.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub am: Option<String>,
    /// AQL/QUERY version.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub aql: Option<String>,
    /// ITS-REST version.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub its_rest: Option<String>,
    /// TERM version.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub term: Option<String>,
}

impl SpecVersions {
    /// The declared version string for a spec component, if any.
    #[must_use]
    pub fn get(&self, component: crate::vocab::SpecComponent) -> Option<&str> {
        use crate::vocab::SpecComponent;
        match component {
            SpecComponent::Rm => self.rm.as_deref(),
            SpecComponent::Base => self.base.as_deref(),
            SpecComponent::Am => self.am.as_deref(),
            SpecComponent::Aql => self.aql.as_deref(),
            SpecComponent::ItsRest => self.its_rest.as_deref(),
            SpecComponent::Term => self.term.as_deref(),
        }
    }
}

/// The verdict-bearing claims (the ICS core): the capabilities and the
/// profile tiers the supplier asserts.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Claims {
    /// Verdict-bearing capability names (must resolve in the capability
    /// matrix).
    #[serde(default)]
    pub capabilities: Vec<CapabilityName>,
    /// The profile tiers claimed (e.g. `CORE`, `STANDARD`, `SEC-BASIC`).
    #[serde(default)]
    pub profiles: Vec<Tier>,
}

/// A declared technology profile: one ITS and the wire formats exercised on
/// it.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TechProfile {
    /// The ITS the profile realizes.
    pub its: ItsName,
    /// The wire formats claimed for this ITS.
    #[serde(default)]
    pub formats: Vec<FormatName>,
}

/// Where the technology profile a results document records came from.
///
/// The recorded profile is what the verdict pipeline selects gating records
/// with, so a reader who cannot tell a declaration from a fallback cannot tell
/// how wide the record's claim is. A declared profile is the party's own
/// its-rest format claim; a defaulted one is every format the instrument
/// speaks, which is what a campaign nothing selected has to record so no red
/// row vanishes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TechProfileSource {
    /// Read from the party statement's `tech_profiles` entry for this ITS.
    Declared,
    /// No declaration named this ITS, so every format the instrument speaks
    /// was recorded.
    Defaulted,
}

impl TechProfileSource {
    /// All variants, in vocabulary order (schema emission derives from this).
    pub const ALL: &[TechProfileSource] =
        &[TechProfileSource::Declared, TechProfileSource::Defaulted];

    /// The source token.
    #[must_use]
    pub fn token(self) -> &'static str {
        match self {
            TechProfileSource::Declared => "declared",
            TechProfileSource::Defaulted => "defaulted",
        }
    }
}

/// The technology profile a results document covers, and where it came from.
///
/// The same two members a party declares ([`TechProfile`]), plus the
/// provenance a reader of the record alone cannot otherwise recover.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RecordedTechProfile {
    /// The ITS the campaign realized.
    pub its: ItsName,
    /// The wire formats the campaign recorded outcomes for.
    #[serde(default)]
    pub formats: Vec<FormatName>,
    /// Whether the formats were declared or defaulted.
    ///
    /// Absent only in a document written before the member existed (v0.1.4 and
    /// earlier), where absence is UNKNOWN and never either source — a reader
    /// that defaults it to `declared` presents a fallback as a party's claim,
    /// which is the misreading the member exists to prevent.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source: Option<TechProfileSource>,
}

/// A performance claim — a performance class plus a reference to the ixit
/// environment block the run was measured in (mandatory for a performance
/// claim).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Performance {
    /// The claimed performance class.
    pub class: String,
    /// The environment the class was demonstrated in (the ixit environment
    /// block id / reference).
    pub environment_ref: String,
}

/// One evidence pointer: a results document and its content digest.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Evidence {
    /// The results artifact path (relative to the submission bundle).
    pub results_path: String,
    /// The SHA-256 digest of the referenced results artifact.
    pub sha256: String,
}

/// The `SDoC` self-declaration signature block.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Attestation {
    /// Who signs the declaration.
    pub signatory: String,
    /// Their role.
    pub role: String,
    /// The declaration date (ISO-8601 text; never re-stamped by the runner).
    pub date: String,
    /// The self-declaration statement text.
    pub statement: String,
}

/// The CNF schedule release this catalogue models, which every run stamps into
/// its results and every claim targets.
///
/// ISO/IEC 9646-7 assigns the cells of an ICS proforma other than the support
/// and supported-values columns to the proforma specifier, so the release the
/// form belongs to is the instrument's own fact rather than a supplier's.
pub const SCHEDULE_RELEASE: &str = "cnf-2.0-w2";

/// The party statement: the ICS (claims) + the `SDoC` (self-declaration).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Statement {
    /// The system under test.
    pub product: Product,
    /// The schedule release the claim targets.
    pub schedule_release: String,
    /// Declared spec-component versions (the `applies` filter input).
    #[serde(default)]
    pub spec_versions: SpecVersions,
    /// The verdict-bearing claims.
    pub claims: Claims,
    /// Declared technology profiles (per ITS).
    #[serde(default)]
    pub tech_profiles: Vec<TechProfile>,
    /// The ICS option declarations selecting `option_select` register
    /// branches.
    #[serde(default)]
    pub options: Vec<OptionTag>,
    /// The `served_extensions` families THIS party declares it serves beyond
    /// the openEHR resource set — never a claim, never a verdict input.
    ///
    /// Each name resolves in the catalogue's `vocab/wire_surface.yaml`
    /// `served_extensions` axis, which carries the routes and configuration
    /// gate of the family; an unresolvable name is a validation finding. A
    /// party that serves nothing beyond the openEHR resources declares an
    /// empty list, and its statement says exactly that.
    #[serde(default)]
    pub served_extensions: Vec<String>,
    /// An optional performance claim.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub performance: Option<Performance>,
    /// Free-form non-functional declarations — never a verdict input.
    #[serde(default)]
    pub non_functional: BTreeMap<String, serde_json::Value>,
    /// Evidence pointers (results artifacts + digests).
    #[serde(default)]
    pub evidence: Vec<Evidence>,
    /// The `SDoC` attestation, if signed.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub attestation: Option<Attestation>,
}

// ── the results ─────────────────────────────────────────────────────────────

/// The SUT identity as recorded by the run.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Sut {
    /// SUT name.
    pub name: String,
    /// SUT version.
    pub version: String,
}

/// Whether the runner's own verification pack (the self-check that the runner
/// itself is sound) passed for this campaign.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum VerificationPackStatus {
    /// The verification pack ran and passed.
    Passed,
    /// The verification pack was not run.
    NotRun,
    /// The verification pack ran and failed (results are untrustworthy).
    Failed,
}

impl VerificationPackStatus {
    /// All variants, in vocabulary order (schema emission derives from this).
    pub const ALL: &[VerificationPackStatus] = &[
        VerificationPackStatus::Passed,
        VerificationPackStatus::NotRun,
        VerificationPackStatus::Failed,
    ];
}

/// The runner identity + self-check status.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Runner {
    /// Runner name.
    pub name: String,
    /// Runner version.
    pub version: String,
    /// The runner's verification-pack status for this campaign.
    pub verification_pack_status: VerificationPackStatus,
}

/// The outcome status of one case×format execution, as recorded in the results.
///
/// ISO/IEC 9646 verdicts: passed→pass, failed→fail, errored→
/// inconclusive; `skipped`/`not_applicable` are selection records each carrying a
/// mandatory citation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OutcomeStatus {
    /// Every driven row passed.
    Passed,
    /// At least one row failed.
    Failed,
    /// Inconclusive (transport fault / unmapped response).
    Errored,
    /// Not executed by choice (with citation).
    Skipped,
    /// Not applicable to this SUT/profile (with citation).
    NotApplicable,
}

impl OutcomeStatus {
    /// All variants, in vocabulary order (schema emission derives from this).
    pub const ALL: &[OutcomeStatus] = &[
        OutcomeStatus::Passed,
        OutcomeStatus::Failed,
        OutcomeStatus::Errored,
        OutcomeStatus::Skipped,
        OutcomeStatus::NotApplicable,
    ];

    /// The status token.
    #[must_use]
    pub fn token(self) -> &'static str {
        match self {
            OutcomeStatus::Passed => "passed",
            OutcomeStatus::Failed => "failed",
            OutcomeStatus::Errored => "errored",
            OutcomeStatus::Skipped => "skipped",
            OutcomeStatus::NotApplicable => "not_applicable",
        }
    }

    /// Whether this status mandates a citation.
    #[must_use]
    pub fn needs_citation(self) -> bool {
        matches!(self, OutcomeStatus::Skipped | OutcomeStatus::NotApplicable)
    }
}

/// One failing row of a table-driven case: the row index (0-based, the
/// content-table order), the failing step (0 = a postcondition/aggregate),
/// and the reason.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FailedRow {
    /// The 0-based row index in the case's parameter table.
    pub row: usize,
    /// The failing step number (0 for postcondition/aggregate failures).
    pub step: u32,
    /// The failure reason.
    pub reason: String,
}

/// One case×format outcome record — the executor's [`CaseRecord`] rolled up
/// into a single verdict for the results document.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OutcomeRecord {
    /// The case id.
    pub case: CaseId,
    /// The wire format, when the case is format-parameterized.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub format: Option<FormatName>,
    /// The rolled-up status.
    pub status: OutcomeStatus,
    /// Rows driven / rows selected (the printed coverage bound).
    pub rows_driven: usize,
    /// Rows selected.
    pub rows_total: usize,
    /// The first failing/erroring step, if any.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub failing_step: Option<u32>,
    /// The failure/error reason, if any.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
    /// The excusing citation — mandatory for `skipped`/`not_applicable`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub citation: Option<String>,
    /// Every failing row of a table-driven case (empty unless `failed`) —
    /// the per-row evidence a triage needs, not just the first reason.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub failed_rows: Vec<FailedRow>,
}

impl OutcomeRecord {
    /// The invariant: `skipped`/`not_applicable` records carry a citation.
    ///
    /// # Errors
    /// [`PartyError::MissingCitation`] when the status needs a citation and
    /// none is present.
    pub fn check_invariants(&self) -> Result<(), PartyError> {
        if self.status.needs_citation() && self.citation.as_deref().unwrap_or_default().is_empty() {
            return Err(PartyError::MissingCitation {
                case: self.case.to_string(),
                status: self.status.token(),
            });
        }
        Ok(())
    }
}

impl From<&CaseRecord> for OutcomeRecord {
    /// Roll a per-row execution record up into a single results outcome:
    /// any failed row → `failed` (first failing step/reason); else any
    /// errored → `errored`; else any passed → `passed` (mixed passed/N-A is
    /// a pass); else all N/A → `not_applicable`; else all skipped →
    /// `skipped`; an empty record → `errored` (nothing was driven).
    fn from(record: &CaseRecord) -> Self {
        let mut failing: Option<(u32, String)> = None;
        let mut erroring: Option<(u32, String)> = None;
        let mut na_citation: Option<String> = None;
        let mut skip_citation: Option<String> = None;
        let mut has_passed = false;
        let mut failed_rows = Vec::new();

        for (index, row) in record.rows.iter().enumerate() {
            match row {
                RowOutcome::Passed => has_passed = true,
                RowOutcome::Failed { step, reason } => {
                    failed_rows.push(FailedRow {
                        row: index,
                        step: *step,
                        reason: reason.clone(),
                    });
                    if failing.is_none() {
                        failing = Some((*step, reason.clone()));
                    }
                }
                RowOutcome::Errored { step, reason } => {
                    if erroring.is_none() {
                        erroring = Some((*step, reason.clone()));
                    }
                }
                RowOutcome::NotApplicable { citation } => {
                    if na_citation.is_none() {
                        na_citation = Some(citation.clone());
                    }
                }
                RowOutcome::Skipped { citation } => {
                    if skip_citation.is_none() {
                        skip_citation = Some(citation.clone());
                    }
                }
            }
        }

        let base = |status, failing_step, reason, citation, failed_rows| OutcomeRecord {
            case: record.case.clone(),
            format: record.format,
            status,
            rows_driven: record.rows_driven,
            rows_total: record.rows_total,
            failing_step,
            reason,
            citation,
            failed_rows,
        };

        if let Some((step, reason)) = failing {
            base(
                OutcomeStatus::Failed,
                Some(step),
                Some(reason),
                None,
                failed_rows,
            )
        } else if let Some((step, reason)) = erroring {
            base(
                OutcomeStatus::Errored,
                Some(step),
                Some(reason),
                None,
                Vec::new(),
            )
        } else if has_passed {
            base(OutcomeStatus::Passed, None, None, None, Vec::new())
        } else if let Some(citation) = na_citation {
            base(
                OutcomeStatus::NotApplicable,
                None,
                None,
                Some(citation),
                Vec::new(),
            )
        } else if let Some(citation) = skip_citation {
            base(
                OutcomeStatus::Skipped,
                None,
                None,
                Some(citation),
                Vec::new(),
            )
        } else {
            base(
                OutcomeStatus::Errored,
                None,
                Some("no rows driven".to_owned()),
                None,
                Vec::new(),
            )
        }
    }
}

/// What ISO/IEC 9646 test selection had to select the campaign with.
///
/// The ICS is the list of components and capabilities a party is answerable
/// for (CNF profiles `master02-overview.adoc` §Overview), so a campaign
/// driven without one is a sweep of the whole catalogue rather than a
/// party-scoped record. A reader of the results document alone must be able
/// to tell the two apart, because the option arms, the extension routes, the
/// claimed capabilities and the release floors are all selected from the ICS.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SelectionBasis {
    /// A party statement was supplied, so selection applied the ICS.
    Statement,
    /// No party statement was supplied: nothing selected the party's option
    /// arms, extension routes, claimed capabilities or release floors.
    StatementBlind,
}

impl SelectionBasis {
    /// All variants, in vocabulary order (schema emission derives from this).
    pub const ALL: &[SelectionBasis] = &[SelectionBasis::Statement, SelectionBasis::StatementBlind];

    /// The basis token.
    #[must_use]
    pub fn token(self) -> &'static str {
        match self {
            SelectionBasis::Statement => "statement",
            SelectionBasis::StatementBlind => "statement_blind",
        }
    }
}

/// Two provenance members of one results document that cannot both be true, so
/// the conditions of the run cannot be reconstructed from the document at all.
///
/// A record with no provenance members is not this: absence is unknown, and a
/// document written before a member existed carries nothing to contradict.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProvenanceContradiction {
    /// `tech_profile.source` reads `declared` while `selection_basis` reads
    /// `statement_blind`: a campaign nothing selected had no declaration to
    /// read a technology profile from.
    DeclaredProfileWithoutSelection,
}

impl std::fmt::Display for ProvenanceContradiction {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match *self {
            ProvenanceContradiction::DeclaredProfileWithoutSelection => f.write_str(
                "tech_profile.source reads declared and selection_basis reads statement_blind, \
                 and a campaign no statement selected has no declaration to read a technology \
                 profile from",
            ),
        }
    }
}

/// One ambiguity disposition record: which register entry the run was subject
/// to, and (for `option_select`) which option branch the ICS selected.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AmbiguityDisposition {
    /// The register entry id.
    pub ambiguity: AmbiguityId,
    /// The selected option tag, for `option_select` entries.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub option: Option<OptionTag>,
}

/// The party results document.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Results {
    /// The system under test.
    pub sut: Sut,
    /// The runner + its self-check status.
    pub runner: Runner,
    /// The schedule release the campaign ran.
    pub schedule_release: String,
    /// The technology profile this results document covers, and whether it was
    /// declared or defaulted.
    pub tech_profile: RecordedTechProfile,
    /// The digest of the ixit topology the run drove (provenance).
    pub ixit_digest: String,
    /// The digest of the party statement that selected this campaign
    /// (provenance), so a reader holding that statement recomputes the
    /// recorded value with `sha256sum statement.json | cut -c1-16`.
    ///
    /// Absent for a campaign no statement selected, and absent in a document
    /// written before the member existed (v0.1.4 and earlier). Those two are
    /// told apart by `selection_basis`: `statement_blind` names the first, and
    /// anything else leaves the identity unknown, which a reader never reads
    /// as a match.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub statement_digest: Option<String>,
    /// What selection had to select this campaign with: the party's ICS, or
    /// nothing.
    ///
    /// Absent only in a document written before the member existed (v0.1.4 and
    /// earlier), where absence is UNKNOWN and never either basis — a reader
    /// that defaults it to `statement` credits a blind sweep with a scope it
    /// never had.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub selection_basis: Option<SelectionBasis>,
    /// The `restapi_specs_version` the SUT's own System OPTIONS manifest
    /// served during the campaign, when that exchange was driven (released
    /// OAS `system.openapi.yaml` `Options` — every member optional, so
    /// absence is normal). An independent CONFIRMATION of the statement's
    /// declared `spec_versions.its_rest`, never a source of truth: no
    /// `required` list binds it and a server could dodge every release-dated
    /// MUST by under-advertising — a divergence from the declaration is a
    /// static-review finding, not a re-declaration.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub restapi_specs_version: Option<String>,
    /// The per-case×format outcomes.
    #[serde(default)]
    pub outcomes: Vec<OutcomeRecord>,
    /// Performance measurements — the second (measured) verdict machinery's
    /// evidence records; each embeds its encoded HDR histograms and the ixit
    /// environment block it was taken in.
    #[serde(default)]
    pub measurements: Vec<crate::perf::Measurement>,
    /// The ambiguity dispositions the run applied: one record per
    /// `option_select` register entry arm the party's ICS declared, in the
    /// register's authored order.
    ///
    /// A campaign no statement selected declares no arm, so the list is
    /// legitimately empty there, which is a statement about the run rather
    /// than a member nothing writes.
    #[serde(default)]
    pub ambiguity_dispositions: Vec<AmbiguityDisposition>,
}

impl Results {
    /// Every outcome invariant across the document (currently: mandatory
    /// citations on `skipped`/`not_applicable`).
    ///
    /// # Errors
    /// Every [`PartyError`] found, one per offending outcome.
    pub fn check_invariants(&self) -> Result<(), Vec<PartyError>> {
        let errors: Vec<PartyError> = self
            .outcomes
            .iter()
            .filter_map(|o| o.check_invariants().err())
            .collect();
        if errors.is_empty() {
            Ok(())
        } else {
            Err(errors)
        }
    }

    /// The provenance members that cannot both be true of one campaign, or
    /// `None` when the document reads consistently.
    ///
    /// A member the document does not carry is unknown and contradicts
    /// nothing, so a record written before these members existed reads
    /// consistently here and is judged on the members it does carry.
    #[must_use]
    pub fn provenance_contradiction(&self) -> Option<ProvenanceContradiction> {
        match (self.tech_profile.source, self.selection_basis) {
            (Some(TechProfileSource::Declared), Some(SelectionBasis::StatementBlind)) => {
                Some(ProvenanceContradiction::DeclaredProfileWithoutSelection)
            }
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn record(rows: Vec<RowOutcome>) -> CaseRecord {
        CaseRecord {
            case: CaseId::parse("I_EHR_SERVICE.create_ehr-main").unwrap(),
            format: Some(FormatName::CanonicalJson),
            rows_driven: rows.len(),
            rows_total: rows.len(),
            rows,
            advisories: Vec::new(),
        }
    }

    #[test]
    fn rollup_prefers_failure_then_error() {
        let r = OutcomeRecord::from(&record(vec![
            RowOutcome::Passed,
            RowOutcome::Failed {
                step: 3,
                reason: "boom".to_owned(),
            },
            RowOutcome::Errored {
                step: 1,
                reason: "conn".to_owned(),
            },
        ]));
        assert_eq!(r.status, OutcomeStatus::Failed);
        assert_eq!(r.failing_step, Some(3));

        let r = OutcomeRecord::from(&record(vec![
            RowOutcome::Passed,
            RowOutcome::Errored {
                step: 2,
                reason: "conn".to_owned(),
            },
        ]));
        assert_eq!(r.status, OutcomeStatus::Errored);
        assert_eq!(r.failing_step, Some(2));
    }

    #[test]
    fn rollup_passed_and_na_and_empty() {
        let r = OutcomeRecord::from(&record(vec![
            RowOutcome::Passed,
            RowOutcome::NotApplicable {
                citation: "c".to_owned(),
            },
        ]));
        assert_eq!(r.status, OutcomeStatus::Passed);

        let r = OutcomeRecord::from(&record(vec![
            RowOutcome::NotApplicable {
                citation: "AMB-17".to_owned(),
            },
            RowOutcome::NotApplicable {
                citation: "AMB-17b".to_owned(),
            },
        ]));
        assert_eq!(r.status, OutcomeStatus::NotApplicable);
        assert_eq!(r.citation.as_deref(), Some("AMB-17"));

        let r = OutcomeRecord::from(&record(vec![]));
        assert_eq!(r.status, OutcomeStatus::Errored);
    }

    /// A case whose every row was skipped rolls up to `skipped` carrying the
    /// FIRST skip citation, so the excuse travels with the published record
    /// rather than being flattened into a bare status.
    #[test]
    fn a_wholly_skipped_case_rolls_up_carrying_its_first_citation() {
        let r = OutcomeRecord::from(&record(vec![
            RowOutcome::Skipped {
                citation: "operator selection".to_owned(),
            },
            RowOutcome::Skipped {
                citation: "operator selection (second row)".to_owned(),
            },
        ]));
        assert_eq!(r.status, OutcomeStatus::Skipped);
        assert_eq!(r.citation.as_deref(), Some("operator selection"));
        assert_eq!(r.failing_step, None);
    }

    /// The document-level invariant collects EVERY offending outcome, so one
    /// pass names the whole repair list instead of stopping at the first.
    #[test]
    fn the_document_invariant_reports_every_uncited_outcome() {
        let document = |outcomes: serde_json::Value| -> Results {
            serde_json::from_value(serde_json::json!({
                "sut": { "name": "s", "version": "1" },
                "runner": { "name": "veredictum", "version": "0",
                             "verification_pack_status": "passed" },
                "schedule_release": "CNF-2.0",
                "tech_profile": { "its": "its-rest", "formats": ["canonical-json"] },
                "ixit_digest": "d",
                "outcomes": outcomes
            }))
            .unwrap()
        };
        let uncited = document(serde_json::json!([
            { "case": "A-x", "status": "not_applicable", "rows_driven": 0, "rows_total": 1 },
            { "case": "B-y", "status": "skipped", "rows_driven": 0, "rows_total": 1 },
            { "case": "C-z", "status": "passed", "rows_driven": 1, "rows_total": 1 }
        ]));
        let errors = uncited
            .check_invariants()
            .expect_err("two outcomes carry no citation");
        assert_eq!(errors.len(), 2, "{errors:?}");

        let cited = document(serde_json::json!([
            { "case": "A-x", "status": "not_applicable", "rows_driven": 0, "rows_total": 1,
              "citation": "AMB-32" },
            { "case": "C-z", "status": "passed", "rows_driven": 1, "rows_total": 1 }
        ]));
        assert!(cited.check_invariants().is_ok());
    }

    /// A reader of the results document ALONE tells a party-scoped record from
    /// a statement-blind sweep, and a document written before the member
    /// existed reads as unknown rather than as either basis.
    #[test]
    fn the_recorded_selection_basis_survives_the_document() {
        let document = |extra: &str| -> Results {
            serde_json::from_str(&format!(
                r#"{{
                    "sut": {{ "name": "s", "version": "1" }},
                    "runner": {{ "name": "veredictum", "version": "0",
                                 "verification_pack_status": "passed" }},
                    "schedule_release": "CNF-2.0",
                    "tech_profile": {{ "its": "its-rest", "formats": ["canonical-json"] }},
                    "ixit_digest": "d"{extra}
                }}"#
            ))
            .unwrap()
        };
        assert_eq!(
            document(r#", "selection_basis": "statement_blind""#).selection_basis,
            Some(SelectionBasis::StatementBlind)
        );
        assert_eq!(
            document(r#", "selection_basis": "statement""#).selection_basis,
            Some(SelectionBasis::Statement)
        );
        assert_eq!(
            document("").selection_basis,
            None,
            "absence is unknown, never either basis"
        );

        let blind = document(r#", "selection_basis": "statement_blind""#);
        let text = serde_json::to_string(&blind).unwrap();
        assert!(
            text.contains(r#""selection_basis":"statement_blind""#),
            "{text}"
        );
        for basis in SelectionBasis::ALL {
            assert_eq!(
                serde_json::to_value(basis).unwrap(),
                serde_json::Value::String(basis.token().to_owned())
            );
        }
    }

    /// A reader of the results document ALONE names the statement the
    /// campaign was selected under, and a document that carries no digest
    /// reads as unknown rather than as any particular claim.
    #[test]
    fn the_recorded_statement_digest_survives_the_document() {
        let document = |extra: &str| -> Results {
            serde_json::from_str(&format!(
                r#"{{
                    "sut": {{ "name": "s", "version": "1" }},
                    "runner": {{ "name": "veredictum", "version": "0",
                                 "verification_pack_status": "passed" }},
                    "schedule_release": "CNF-2.0",
                    "tech_profile": {{ "its": "its-rest", "formats": ["canonical-json"] }},
                    "ixit_digest": "d"{extra}
                }}"#
            ))
            .unwrap()
        };
        assert_eq!(
            document(r#", "statement_digest": "aedf8eec255f7847""#)
                .statement_digest
                .as_deref(),
            Some("aedf8eec255f7847")
        );
        assert_eq!(
            document("").statement_digest,
            None,
            "absence is unknown, never a claim"
        );

        let named = document(r#", "statement_digest": "aedf8eec255f7847""#);
        let text = serde_json::to_string(&named).unwrap();
        assert!(
            text.contains(r#""statement_digest":"aedf8eec255f7847""#),
            "{text}"
        );
        let blind = serde_json::to_string(&document("")).unwrap();
        assert!(
            !blind.contains("statement_digest"),
            "an unnamed statement writes no member at all: {blind}"
        );
    }

    /// A document carrying `tech_profile` as `{ its, formats }` and nothing
    /// else, plus whatever `extra` adds to the block.
    fn profile_document(extra: &str) -> Results {
        serde_json::from_str(&format!(
            r#"{{
                "sut": {{ "name": "s", "version": "1" }},
                "runner": {{ "name": "veredictum", "version": "0",
                             "verification_pack_status": "passed" }},
                "schedule_release": "CNF-2.0",
                "tech_profile": {{
                    "its": "its-rest", "formats": ["canonical-json"]{extra}
                }},
                "ixit_digest": "d"
            }}"#
        ))
        .unwrap()
    }

    /// A reader of the results document ALONE tells a party's declared format
    /// list from the fallback every format, and a document written before the
    /// member existed reads as unknown rather than as either source.
    #[test]
    fn the_recorded_profile_source_survives_the_document() {
        assert_eq!(
            profile_document(r#", "source": "declared""#)
                .tech_profile
                .source,
            Some(TechProfileSource::Declared)
        );
        assert_eq!(
            profile_document(r#", "source": "defaulted""#)
                .tech_profile
                .source,
            Some(TechProfileSource::Defaulted)
        );
        assert_eq!(
            profile_document("").tech_profile.source,
            None,
            "absence is unknown, never either source"
        );

        let declared =
            serde_json::to_string(&profile_document(r#", "source": "declared""#)).unwrap();
        assert!(declared.contains(r#""source":"declared""#), "{declared}");
        let unknown = serde_json::to_string(&profile_document("")).unwrap();
        assert!(
            !unknown.contains("source"),
            "an unknown source writes no member at all: {unknown}"
        );
        for source in TechProfileSource::ALL {
            assert_eq!(
                serde_json::to_value(source).unwrap(),
                serde_json::Value::String(source.token().to_owned())
            );
        }
    }

    /// A campaign no statement selected had no declaration to read a profile
    /// from, so `declared` and `statement_blind` cannot both be true of one
    /// document.
    #[test]
    fn a_declared_profile_on_a_blind_campaign_contradicts_itself() {
        let with = |source: TechProfileSource, basis: SelectionBasis| -> Results {
            Results {
                selection_basis: Some(basis),
                ..profile_document(&format!(r#", "source": "{}""#, source.token()))
            }
        };
        assert_eq!(
            with(TechProfileSource::Declared, SelectionBasis::StatementBlind)
                .provenance_contradiction(),
            Some(ProvenanceContradiction::DeclaredProfileWithoutSelection)
        );
        // A statement that names no its-rest profile still selected the
        // campaign, so `defaulted` under either basis reads consistently.
        for (source, basis) in [
            (TechProfileSource::Declared, SelectionBasis::Statement),
            (TechProfileSource::Defaulted, SelectionBasis::Statement),
            (TechProfileSource::Defaulted, SelectionBasis::StatementBlind),
        ] {
            assert_eq!(
                with(source, basis).provenance_contradiction(),
                None,
                "{} under {} is a campaign that can happen",
                source.token(),
                basis.token()
            );
        }
        assert_eq!(
            profile_document("").provenance_contradiction(),
            None,
            "a document carrying neither member contradicts nothing"
        );
    }

    #[test]
    fn citation_invariant_bites() {
        let mut r = OutcomeRecord::from(&record(vec![RowOutcome::NotApplicable {
            citation: "AMB-1".to_owned(),
        }]));
        assert!(r.check_invariants().is_ok());
        r.citation = None;
        assert!(matches!(
            r.check_invariants(),
            Err(PartyError::MissingCitation { .. })
        ));
    }

    #[test]
    fn statement_round_trips() {
        let json = serde_json::json!({
            "product": { "name": "FerroEHR", "version": "3.5.0",
                          "vendor": "Ruben Talstra", "identifier": "urn:rubentalstra:ferroehr" },
            "schedule_release": "CNF-2.0",
            "spec_versions": { "rm": "1.2.0", "its_rest": "1.1.0" },
            "claims": { "capabilities": ["EhrOperations"], "profiles": ["CORE"] },
            "tech_profiles": [ { "its": "its-rest", "formats": ["canonical-json"] } ],
            "options": ["adl14-duplicate-conflict"],
            "evidence": [ { "results_path": "results.json", "sha256": "abc" } ]
        });
        let s: Statement = serde_json::from_value(json).unwrap();
        assert_eq!(s.spec_versions.rm.as_deref(), Some("1.2.0"));
        assert_eq!(s.claims.capabilities.len(), 1);
        let back = serde_json::to_value(&s).unwrap();
        let s2: Statement = serde_json::from_value(back).unwrap();
        assert_eq!(s2.product.name, "FerroEHR");
    }

    #[test]
    fn results_round_trip_and_invariants() {
        let json = serde_json::json!({
            "sut": { "name": "ferroehr", "version": "3.5.0" },
            "runner": { "name": "veredictum", "version": "0.1.0",
                         "verification_pack_status": "passed" },
            "schedule_release": "CNF-2.0",
            "tech_profile": { "its": "its-rest", "formats": ["canonical-json"] },
            "ixit_digest": "deadbeef",
            "outcomes": [
                { "case": "I_EHR_SERVICE.create_ehr-main", "format": "canonical-json",
                  "status": "passed", "rows_driven": 2, "rows_total": 2 },
                { "case": "I_ADMIN_SERVICE.list_contributions-x",
                  "status": "not_applicable", "rows_driven": 0, "rows_total": 1,
                  "citation": "AMB-33" }
            ]
        });
        let r: Results = serde_json::from_value(json).unwrap();
        assert!(r.check_invariants().is_ok());
        assert_eq!(r.outcomes.len(), 2);
    }
}
