// SPDX-FileCopyrightText: Veredictum contributors
// SPDX-License-Identifier: Apache-2.0

//! Driving the catalogue against a live system under test.
//!
//! The seam loads the catalogue and the ixit topology, executes every
//! selected case over the SUT's own wire, and assembles the party
//! `results.json` record from what the exchanges produced. It writes
//! nothing: the finished documents come back as text for the caller to
//! serve or store.

use std::path::{Path, PathBuf};

use crate::party::{OutcomeRecord, OutcomeStatus, Results, Statement};
use crate::pipeline::{Error, load_clean_root, load_ixit, read_json, to_json_document};
use crate::run::RunReport;

/// What to drive, against which SUT, from which topology.
#[derive(Debug)]
pub struct RunRequest<'a> {
    /// The artifact root.
    pub root: &'a Path,
    /// The ixit topology document.
    pub ixit: &'a Path,
    /// The directory a prior `results.json` is read from for measurement
    /// carry-forward, and the one the caller will write this run into.
    pub out_dir: &'a Path,
    /// The SUT display name recorded in the results.
    pub sut_name: &'a str,
    /// The SUT version label recorded in the results.
    pub sut_version: &'a str,
    /// Drive only cases whose id contains this substring.
    pub filter: Option<&'a str>,
    /// The party statement, which turns on ISO/IEC 9646 test selection: an
    /// option-gated case whose option the statement does not declare is
    /// recorded not-applicable at drive time instead of driven.
    pub statement: Option<&'a Path>,
}

/// Something a run reports as it goes that is not a failure.
#[derive(Debug, Clone, Copy)]
pub enum RunWarning<'a> {
    /// Measurement records taken at one SUT version are being carried into
    /// a run against another, which the version-binding rule wants either
    /// re-measured or attested as an unchanged surface.
    CarriedMeasurements {
        /// How many records are being carried.
        count: usize,
        /// The SUT version they were measured at.
        measured_at: &'a str,
        /// The SUT version this run drives.
        running_at: &'a str,
    },
}

/// The recorded outcomes, tallied by status.
#[derive(Debug, Clone, Copy, Default)]
pub struct OutcomeCounts {
    /// Cases whose every assertion held.
    pub passed: usize,
    /// Cases with at least one failed assertion.
    pub failed: usize,
    /// Cases the runner could not drive to a verdict.
    pub errored: usize,
    /// Cases excluded from the campaign, skipped or not applicable.
    pub not_applicable: usize,
}

/// One completed campaign against a live SUT.
#[derive(Debug)]
pub struct RunOutcome {
    /// The party results record, ready to be judged.
    pub results: Results,
    /// The interpreter's own account of the run: records, exceptions and
    /// coverage.
    pub report: RunReport,
    /// The outcome tally.
    pub counts: OutcomeCounts,
    /// Where the results record belongs, under the requested output
    /// directory.
    pub results_path: PathBuf,
    /// Where the interpreter-exception record belongs, under the requested
    /// output directory.
    pub exceptions_path: PathBuf,
}

impl RunOutcome {
    /// Returns whether the campaign is clean, which means nothing failed and
    /// nothing errored.
    #[must_use]
    pub fn is_clean(&self) -> bool {
        self.counts.failed == 0 && self.counts.errored == 0
    }

    /// Renders the party `results.json` document.
    ///
    /// # Errors
    /// [`Error::Serialize`] when the record cannot be serialized.
    pub fn results_document(&self) -> Result<String, Error> {
        to_json_document(&self.results, "serialize")
    }

    /// Renders the interpreter-exception document: one entry per case the
    /// interpreter did not drive, with the reason it was excluded.
    ///
    /// # Errors
    /// [`Error::Serialize`] when the entries cannot be serialized.
    pub fn exceptions_document(&self) -> Result<String, Error> {
        let entries: Vec<serde_json::Value> = self
            .report
            .exceptions
            .iter()
            .map(|(case, e)| serde_json::json!({ "case": case.to_string(), "exception": e }))
            .collect();
        to_json_document(&entries, "serialize")
    }
}

/// Returns the ixit digest recorded with a campaign, which binds the results
/// to the exact topology they were driven from.
#[must_use]
pub fn ixit_digest(ixit_text: &str) -> String {
    use std::hash::{Hash, Hasher};
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    ixit_text.hash(&mut hasher);
    format!("{:016x}", hasher.finish())
}

/// Drives the catalogue against a live SUT and assembles the party results.
///
/// `warn` receives everything the run reports that is not a failure, in the
/// order it happens, so a caller sees a carry-forward warning even when a
/// later stage returns an error.
///
/// # Errors
/// [`Error::Catalogue`] or [`Error::Artifacts`] when the tree does not load,
/// [`Error::Read`] or [`Error::Parse`] for the ixit and statement documents,
/// [`Error::Instrument`] for an interpreter defect, and
/// [`Error::RecordedInvariants`] when the assembled record violates its own
/// invariants.
pub fn execute_run(
    request: &RunRequest<'_>,
    warn: &dyn Fn(RunWarning<'_>),
    progress: &mut dyn FnMut(crate::run::Progress<'_>),
) -> Result<RunOutcome, Error> {
    let loaded = load_clean_root(request.root)?;
    let (ixit, ixit_text) = load_ixit(request.ixit)?;
    let mut set = loaded.set;
    if let Some(needle) = request.filter {
        set.cases.retain(|(_, c)| c.id.as_str().contains(needle));
    }
    let statement: Option<Statement> = match request.statement {
        None => None,
        Some(path) => Some(read_json(path, "statement")?),
    };
    let report = crate::run::execute(&set, &ixit, statement.as_ref(), progress)
        .map_err(|e| Error::Instrument(format!("execution defect: {e}")))?;
    let outcomes: Vec<OutcomeRecord> = report.records.iter().map(OutcomeRecord::from).collect();
    let counts = tally(&outcomes);
    let carried = carried_measurements(request, warn)?;
    let results = Results {
        sut: crate::party::Sut {
            name: request.sut_name.to_owned(),
            version: request.sut_version.to_owned(),
        },
        runner: crate::party::Runner {
            name: "veredictum".to_owned(),
            version: env!("CARGO_PKG_VERSION").to_owned(),
            verification_pack_status: crate::party::VerificationPackStatus::Passed,
        },
        schedule_release: "cnf-2.0-w2".to_owned(),
        tech_profile: tech_profile(statement.as_ref()),
        ixit_digest: ixit_digest(&ixit_text),
        restapi_specs_version: report.restapi_specs_version.clone(),
        outcomes,
        measurements: carried,
        ambiguity_dispositions: Vec::new(),
    };
    results
        .check_invariants()
        .map_err(Error::RecordedInvariants)?;
    Ok(RunOutcome {
        results,
        report,
        counts,
        results_path: request.out_dir.join("results.json"),
        exceptions_path: request.out_dir.join("run-exceptions.json"),
    })
}

fn tally(outcomes: &[OutcomeRecord]) -> OutcomeCounts {
    let mut counts = OutcomeCounts::default();
    for outcome in outcomes {
        match outcome.status {
            OutcomeStatus::Passed => counts.passed += 1,
            OutcomeStatus::Failed => counts.failed += 1,
            OutcomeStatus::Errored => counts.errored += 1,
            _ => counts.not_applicable += 1,
        }
    }
    counts
}

// The recorded technology profile IS the claim the verdict pipeline selects
// gating records with (`verdict::rollup_results`): a narrow hardcoded list
// here silently deselects every other format's failed rows — the false-green
// shape that hid four red canonical-xml rows behind a PASS badge. The profile
// therefore comes from the party statement's its-rest claim; with no
// statement, EVERY format is selected so nothing red can vanish.
fn tech_profile(statement: Option<&Statement>) -> crate::party::TechProfile {
    crate::party::TechProfile {
        its: crate::vocab::ItsName::ItsRest,
        formats: statement
            .and_then(|s| {
                s.tech_profiles
                    .iter()
                    .find(|p| p.its == crate::vocab::ItsName::ItsRest)
            })
            .map_or_else(
                || crate::vocab::FormatName::ALL.to_vec(),
                |p| p.formats.clone(),
            ),
    }
}

// A functional run never re-measures: the measurement records of a prior
// results.json at the same path carry forward, for the same SUT name only.
// NOTE: no prior file is ABSENCE (the first run at this path); a file that
// exists but will not read or parse is a DEFECT — carrying zero measurements
// past it would silently drop the measured evidence.
fn carried_measurements(
    request: &RunRequest<'_>,
    warn: &dyn Fn(RunWarning<'_>),
) -> Result<Vec<crate::perf::Measurement>, Error> {
    let prior_path = request.out_dir.join("results.json");
    let prior = match std::fs::read_to_string(&prior_path) {
        Ok(text) => match serde_json::from_str::<Results>(&text) {
            Ok(prior) => Some(prior),
            Err(e) => {
                return Err(Error::Instrument(format!(
                    "runner defect: {} exists but does not parse as results.json ({e}) — \
                     its measurement records cannot be carried forward",
                    prior_path.display()
                )));
            }
        },
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => None,
        Err(e) => {
            return Err(Error::Instrument(format!(
                "runner defect: {} is unreadable ({e})",
                prior_path.display()
            )));
        }
    };
    let Some(prior) = prior.filter(|prior| prior.sut.name == request.sut_name) else {
        return Ok(Vec::new());
    };
    if prior.sut.version != request.sut_version && !prior.measurements.is_empty() {
        warn(RunWarning::CarriedMeasurements {
            count: prior.measurements.len(),
            measured_at: &prior.sut.version,
            running_at: request.sut_version,
        });
    }
    Ok(prior.measurements)
}
