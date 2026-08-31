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
use crate::transcript::{Recording, RunTranscript, TRANSCRIPT_FILE};

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
    /// recorded not-applicable at drive time instead of driven. With no
    /// statement NO arm of a mutually exclusive branch is selected, so those
    /// cases and every extension route are recorded not-applicable too
    /// ([`crate::run::UnestablishedFact`]), and the recorded
    /// `selection_basis` says the campaign ran blind.
    pub statement: Option<&'a Path>,
    /// Whether the run keeps its wire exchanges for the transcript artifact
    /// ([`crate::transcript::TRANSCRIPT_FILE`], written beside the results).
    pub recording: Recording,
}

/// Something a run reports as it goes that is not a failure.
#[derive(Debug, Clone, Copy)]
pub enum RunWarning<'a> {
    /// The campaign carried no party statement, so ISO/IEC 9646 test
    /// selection had no ICS to select with: reported once per run, naming
    /// every fact it could not establish and the cases each excused.
    StatementBlindSelection {
        /// Cases excused per unestablished fact, in vocabulary order. A fact
        /// absent here excused nothing, either because no case turned on it
        /// or because it only narrows a sweep the catalogue can honestly
        /// drive ([`crate::run::UnestablishedFact::excuses_case`]).
        excused: &'a std::collections::BTreeMap<crate::run::UnestablishedFact, usize>,
    },
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
    /// Where the wire transcript belongs, when the run recorded one.
    pub transcript_path: Option<PathBuf>,
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

    /// Renders the wire transcript, or `None` when the run recorded nothing.
    ///
    /// The document is canonicalized before rendering, so the same exchanges
    /// always produce the same bytes.
    ///
    /// # Errors
    /// [`Error::Serialize`] when the transcript cannot be serialized.
    pub fn transcript_document(&self) -> Result<Option<String>, Error> {
        if self.report.transcripts.is_empty() {
            return Ok(None);
        }
        let mut transcript = RunTranscript {
            sut: self.results.sut.clone(),
            schedule_release: self.results.schedule_release.clone(),
            cases: self.report.transcripts.clone(),
        };
        transcript.canonicalize();
        to_json_document(&transcript, "serialize").map(Some)
    }
}

/// The digest's width in bytes, which renders as twice that many hex
/// characters.
const IXIT_DIGEST_BYTES: usize = 8;

/// Returns the ixit digest recorded with a campaign, which binds the results
/// to the exact declaration they were driven under.
///
/// The digest is the leading `IXIT_DIGEST_BYTES` (8) bytes of the SHA-256
/// over the ixit document's bytes exactly as they sit on disk, lowercase hex.
/// Nothing is canonicalized, reordered or reformatted first, so anyone
/// holding the declaration a published record was driven under re-derives the
/// recorded value with `sha256sum ixit.json | cut -c1-16`.
///
/// ```
/// use veredictum::pipeline::conformance::ixit_digest;
///
/// // `printf '{}' | sha256sum` prints 44136fa355b3678a…
/// assert_eq!(ixit_digest("{}"), "44136fa355b3678a");
/// ```
#[must_use]
pub fn ixit_digest(ixit_text: &str) -> String {
    use std::fmt::Write as _;

    use sha2::{Digest as _, Sha256};

    Sha256::digest(ixit_text.as_bytes())
        .iter()
        .take(IXIT_DIGEST_BYTES)
        .fold(
            String::with_capacity(IXIT_DIGEST_BYTES.saturating_mul(2)),
            |mut out, byte| {
                let _ = write!(out, "{byte:02x}");
                out
            },
        )
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
    let report = crate::run::execute(&set, &ixit, statement.as_ref(), request.recording, progress)
        .map_err(|e| Error::Instrument(format!("execution defect: {e}")))?;
    if statement.is_none() {
        warn(RunWarning::StatementBlindSelection {
            excused: &report.unestablished,
        });
    }
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
        selection_basis: Some(selection_basis(statement.as_ref())),
        restapi_specs_version: report.restapi_specs_version.clone(),
        outcomes,
        measurements: carried,
        ambiguity_dispositions: Vec::new(),
    };
    results
        .check_invariants()
        .map_err(Error::RecordedInvariants)?;
    let transcript_path =
        (!report.transcripts.is_empty()).then(|| request.out_dir.join(TRANSCRIPT_FILE));
    Ok(RunOutcome {
        results,
        report,
        counts,
        results_path: request.out_dir.join("results.json"),
        exceptions_path: request.out_dir.join("run-exceptions.json"),
        transcript_path,
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

// What selection had to select the campaign with, recorded in the results so
// a reader tells a party-scoped record from a whole-catalogue sweep without
// access to the invocation (`run::UnestablishedFact` carries what a blind
// sweep cannot decide).
pub(crate) fn selection_basis(statement: Option<&Statement>) -> crate::party::SelectionBasis {
    match statement {
        Some(_) => crate::party::SelectionBasis::Statement,
        None => crate::party::SelectionBasis::StatementBlind,
    }
}

// The recorded technology profile IS the claim the verdict pipeline selects
// gating records with (`verdict::rollup_results`): a narrow hardcoded list
// here silently deselects every other format's failed rows — the false-green
// shape that hid four red canonical-xml rows behind a PASS badge. The profile
// therefore comes from the party statement's its-rest claim; with no
// statement, EVERY format is selected so nothing red can vanish.
pub(crate) fn tech_profile(statement: Option<&Statement>) -> crate::party::TechProfile {
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

#[cfg(test)]
mod tests {
    use std::cell::RefCell;
    use std::path::{Path, PathBuf};

    use super::*;

    /// The committed example results document, the one real record in the
    /// tree (`examples/results.example.json`): one measurement, and one
    /// outcome of each rolled-up status.
    fn example_results() -> Results {
        let text = std::fs::read_to_string(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/examples/results.example.json"
        ))
        .expect("the committed example results document");
        serde_json::from_str(&text).expect("the example document parses as results")
    }

    fn request<'a>(out_dir: &'a Path, sut_name: &'a str, sut_version: &'a str) -> RunRequest<'a> {
        RunRequest {
            root: Path::new("artifacts"),
            ixit: Path::new("ixit.json"),
            out_dir,
            sut_name,
            sut_version,
            filter: None,
            statement: None,
            recording: Recording::Off,
        }
    }

    fn outcome(case: &str, status: OutcomeStatus) -> OutcomeRecord {
        OutcomeRecord {
            case: crate::ids::CaseId::parse(case).expect("a well-formed case id"),
            format: None,
            status,
            rows_driven: 1,
            rows_total: 1,
            failing_step: None,
            reason: None,
            citation: Some("citation".to_owned()),
            failed_rows: Vec::new(),
        }
    }

    /// Writes `results` as the prior record of `dir`, at the name
    /// [`carried_measurements`] reads.
    fn write_prior(dir: &Path, results: &Results) -> PathBuf {
        let path = dir.join("results.json");
        let text = serde_json::to_string(results).expect("the record serializes");
        std::fs::write(&path, text).expect("writing the prior record");
        path
    }

    /// A warning sink that records what the run reported, in order.
    fn sink() -> RefCell<Vec<String>> {
        RefCell::new(Vec::new())
    }

    #[test]
    fn skipped_and_not_applicable_tally_into_one_selection_bucket() {
        let outcomes = vec![
            outcome("I_EHR_SERVICE.create_ehr-a", OutcomeStatus::Passed),
            outcome("I_EHR_SERVICE.create_ehr-b", OutcomeStatus::Passed),
            outcome("I_EHR_SERVICE.create_ehr-c", OutcomeStatus::Failed),
            outcome("I_EHR_SERVICE.create_ehr-d", OutcomeStatus::Errored),
            outcome("I_EHR_SERVICE.create_ehr-e", OutcomeStatus::Skipped),
            outcome("I_EHR_SERVICE.create_ehr-f", OutcomeStatus::NotApplicable),
        ];
        let counts = tally(&outcomes);
        assert_eq!(counts.passed, 2);
        assert_eq!(counts.failed, 1);
        assert_eq!(counts.errored, 1);
        // `skipped` and `not_applicable` are both selection records, so they
        // share the bucket a verdict never counts as a driven outcome.
        assert_eq!(counts.not_applicable, 2);
    }

    #[test]
    fn a_campaign_is_clean_only_when_nothing_failed_or_errored() {
        let clean = OutcomeCounts {
            passed: 3,
            failed: 0,
            errored: 0,
            not_applicable: 4,
        };
        let outcome_of = |counts: OutcomeCounts| RunOutcome {
            results: example_results(),
            report: RunReport::default(),
            counts,
            results_path: PathBuf::from("results.json"),
            exceptions_path: PathBuf::from("run-exceptions.json"),
            transcript_path: None,
        };
        assert!(outcome_of(clean).is_clean());
        assert!(
            !outcome_of(OutcomeCounts { failed: 1, ..clean }).is_clean(),
            "a failed row is never clean"
        );
        assert!(
            !outcome_of(OutcomeCounts {
                errored: 1,
                ..clean
            })
            .is_clean(),
            "an inconclusive row is never clean either"
        );
    }

    /// With no statement there is no declared profile to narrow selection by,
    /// and the verdict pipeline selects gating records by the recorded
    /// profile — so every format is recorded, which is what keeps a red row
    /// in an unlisted format from vanishing behind a PASS.
    #[test]
    fn an_absent_statement_records_every_format() {
        let profile = tech_profile(None);
        assert_eq!(profile.its, crate::vocab::ItsName::ItsRest);
        assert_eq!(profile.formats, crate::vocab::FormatName::ALL.to_vec());
    }

    #[test]
    fn a_statement_records_its_own_declared_its_rest_formats() {
        let statement: Statement = serde_json::from_value(serde_json::json!({
            "product": {
                "vendor": "v", "name": "n", "version": "1", "identifier": "urn:test:n"
            },
            "schedule_release": "cnf-2.0-w2",
            "claims": { "profiles": [], "capabilities": [] },
            "tech_profiles": [
                { "its": "its-rest", "formats": ["canonical-json"] }
            ]
        }))
        .expect("a minimal statement parses");
        let profile = tech_profile(Some(&statement));
        assert_eq!(
            profile.formats,
            vec![crate::vocab::FormatName::CanonicalJson]
        );
        assert_ne!(
            profile.formats,
            crate::vocab::FormatName::ALL.to_vec(),
            "a declared profile narrows the recorded formats"
        );
    }

    /// The digest binds the results to the exact topology bytes, so equal
    /// text digests equally and one changed character does not.
    #[test]
    fn the_ixit_digest_is_a_function_of_the_topology_bytes() {
        let text = r#"{"instances":{}}"#;
        assert_eq!(ixit_digest(text), ixit_digest(text));
        assert_ne!(ixit_digest(text), ixit_digest(r#"{"instances":{ }}"#));
        assert_eq!(ixit_digest(text).len(), 16, "16 lowercase hex characters");
        assert!(ixit_digest(text).chars().all(|c| c.is_ascii_hexdigit()));
    }

    /// A declaration in the shape the reproduction lane feeds the runner: the
    /// clinical principal and the unauthenticated one, and nothing else.
    const FIXTURE_IXIT: &str = r#"{
  "instances": {
    "sut": {
      "base_url": "http://127.0.0.1:8080/rest/openehr/v1",
      "auth": { "mode": "basic", "user_env": "SUT_USER", "password_env": "SUT_PASS" }
    },
    "unauthenticated": {
      "base_url": "http://127.0.0.1:8080/rest/openehr/v1",
      "auth": { "mode": "none" }
    }
  }
}
"#;

    /// A published record's digest is worth something only if a reader
    /// holding the declaration re-derives it, so the recipe is pinned against
    /// values an outside tool produced: `sha256sum <fixture> | cut -c1-16`.
    #[test]
    fn the_ixit_digest_is_the_leading_sha256_bytes_of_the_declaration() {
        serde_json::from_str::<crate::ixit::Ixit>(FIXTURE_IXIT)
            .expect("the pinned fixture is a real declaration, not just bytes");
        assert_eq!(ixit_digest(r#"{"instances":{}}"#), "b6d92d2643a85d0c");
        assert_eq!(ixit_digest(FIXTURE_IXIT), "bfbf6ece2dea6ef0");
    }

    #[test]
    fn no_prior_record_carries_nothing_and_warns_about_nothing() {
        let dir = assert_fs::TempDir::new().expect("temp dir");
        let seen = sink();
        let carried = carried_measurements(
            &request(dir.path(), "example-cdr", "0.0.0-example"),
            &|warning| seen.borrow_mut().push(format!("{warning:?}")),
        )
        .expect("an absent prior record is absence, never a defect");
        assert!(carried.is_empty());
        assert!(seen.borrow().is_empty());
    }

    #[test]
    fn a_prior_record_of_another_sut_never_carries_forward() {
        let dir = assert_fs::TempDir::new().expect("temp dir");
        let prior = example_results();
        assert!(!prior.measurements.is_empty(), "the example is measured");
        write_prior(dir.path(), &prior);
        let seen = sink();
        let carried = carried_measurements(
            &request(dir.path(), "another-cdr", "0.0.0-example"),
            &|warning| seen.borrow_mut().push(format!("{warning:?}")),
        )
        .expect("a foreign prior record is not a defect");
        assert!(
            carried.is_empty(),
            "measurements never travel between systems under test"
        );
        assert!(seen.borrow().is_empty());
    }

    #[test]
    fn measurements_carry_forward_silently_at_the_same_version() {
        let dir = assert_fs::TempDir::new().expect("temp dir");
        let prior = example_results();
        write_prior(dir.path(), &prior);
        let seen = sink();
        let carried = carried_measurements(
            &request(dir.path(), &prior.sut.name, &prior.sut.version),
            &|warning| seen.borrow_mut().push(format!("{warning:?}")),
        )
        .expect("the same SUT at the same version");
        assert_eq!(carried.len(), prior.measurements.len());
        assert!(
            seen.borrow().is_empty(),
            "an unchanged version needs no attestation"
        );
    }

    /// The version-binding rule: a record measured at another version is
    /// carried, and the run says so, because it wants either a re-measure or
    /// an attested-unchanged surface.
    #[test]
    fn a_version_change_carries_the_records_and_warns() {
        let dir = assert_fs::TempDir::new().expect("temp dir");
        let prior = example_results();
        write_prior(dir.path(), &prior);
        let seen = sink();
        let carried = carried_measurements(
            &request(dir.path(), &prior.sut.name, "9.9.9-next"),
            &|warning| match warning {
                RunWarning::CarriedMeasurements {
                    count,
                    measured_at,
                    running_at,
                } => seen
                    .borrow_mut()
                    .push(format!("{count} {measured_at} {running_at}")),
                RunWarning::StatementBlindSelection { .. } => {
                    panic!("carry-forward reports no selection warning")
                }
            },
        )
        .expect("carry-forward across versions is a warning, not a refusal");
        assert_eq!(carried.len(), prior.measurements.len());
        assert_eq!(
            *seen.borrow(),
            vec![format!(
                "{} {} 9.9.9-next",
                prior.measurements.len(),
                prior.sut.version
            )]
        );
    }

    /// A prior file that exists but will not parse is a runner defect:
    /// carrying zero measurements past it would silently drop the measured
    /// evidence the party already holds.
    #[test]
    fn an_unparsable_prior_record_is_a_defect_not_an_empty_carry() {
        let dir = assert_fs::TempDir::new().expect("temp dir");
        std::fs::write(dir.path().join("results.json"), "{ not json")
            .expect("writing the broken record");
        let error = carried_measurements(&request(dir.path(), "example-cdr", "0.0.0"), &|_| {})
            .expect_err("a broken prior record must stop the run");
        let message = error.to_string();
        assert!(
            message.contains("does not parse as results.json"),
            "{message}"
        );
        assert!(message.contains("cannot be carried forward"), "{message}");
    }

    #[test]
    fn the_documents_render_the_record_and_one_entry_per_exception() {
        let results = example_results();
        let case = crate::ids::CaseId::parse("I_EHR_SERVICE.create_ehr-main").expect("case id");
        let outcome = RunOutcome {
            results,
            report: RunReport {
                exceptions: vec![(
                    case.clone(),
                    crate::run::Exception::Unrealized("no wire on this ITS".to_owned()),
                )],
                ..RunReport::default()
            },
            counts: OutcomeCounts::default(),
            results_path: PathBuf::from("results.json"),
            exceptions_path: PathBuf::from("run-exceptions.json"),
            transcript_path: None,
        };

        let document = outcome
            .results_document()
            .expect("the record serializes as a document");
        assert!(document.ends_with('\n'), "documents end with a newline");
        let parsed: Results =
            serde_json::from_str(&document).expect("the rendered document parses back");
        assert_eq!(parsed.sut.name, outcome.results.sut.name);

        let exceptions = outcome
            .exceptions_document()
            .expect("the exceptions serialize");
        let entries: Vec<serde_json::Value> =
            serde_json::from_str(&exceptions).expect("the exception document parses");
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0]["case"], case.to_string());
        assert_eq!(entries[0]["exception"]["kind"], "unrealized");
    }
}
