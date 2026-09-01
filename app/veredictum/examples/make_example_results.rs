// SPDX-FileCopyrightText: Veredictum contributors
// SPDX-License-Identifier: Apache-2.0

//! Generates `examples/results.example.json` — the committed EXAMPLE results
//! document (#48).
//!
//! It is an explicit example, never a conformance record: the SUT is named
//! `example-cdr`, the runner's verification-pack status is `not_run`, and the
//! numbers are synthetic. What makes it worth committing is that every part is
//! produced by the crate's own machinery — the outcome records satisfy
//! `Results::check_invariants`, the histograms are real `HdrHistogram` V2
//! encodings from the same serializer the runner uses, and the measurement
//! verdict is COMPUTED by [`veredictum::perf::class_verdict`] against the real
//! catalogue's POC case, never asserted. The fuzz seed sets (`fuzz/seeds.sh`)
//! pull the document and its embedded histogram, so the `party_document` and
//! `hdr_v2` targets start from a real record instead of mutations.
//!
//! Regenerate with `cargo run --example make_example_results` from the
//! repository root; the output is deterministic, and
//! `scripts/checks/example-results-drift.sh` fails when the committed copy is
//! not what this run writes. One optional argument names another output path,
//! which is how that gate renders the document without touching the tree.

use hdrhistogram::Histogram;
use veredictum::ids::CaseId;
use veredictum::ixit::Environment;
use veredictum::party::{
    OutcomeRecord, OutcomeStatus, Results, Runner, SelectionBasis, Sut, TechProfile,
    VerificationPackStatus,
};
use veredictum::perf::{Measurement, OperationMeasurement, PerfClass, class_verdict};
use veredictum::pipeline::conformance::{ixit_digest, statement_digest};
use veredictum::pipeline::load_clean_root;
use veredictum::pipeline::measured::performance_case_of_class;
use veredictum::vocab::{FormatName, ItsName};

fn outcome(
    case: &str,
    status: OutcomeStatus,
    reason: Option<&str>,
    citation: Option<&str>,
) -> Result<OutcomeRecord, Box<dyn std::error::Error>> {
    Ok(OutcomeRecord {
        case: CaseId::parse(case)?,
        format: Some(FormatName::CanonicalJson),
        status,
        rows_driven: 1,
        rows_total: 1,
        failing_step: match status {
            OutcomeStatus::Failed | OutcomeStatus::Errored => Some(2),
            _ => None,
        },
        reason: reason.map(str::to_owned),
        citation: citation.map(str::to_owned),
        failed_rows: Vec::new(),
    })
}

// Deterministic synthetic latencies, microseconds: a plausible unimodal
// distribution with a slow tail, fixed so re-runs are byte-identical.
fn histogram(base_us: u64) -> Result<Histogram<u64>, Box<dyn std::error::Error>> {
    let mut h = Histogram::<u64>::new_with_bounds(1, 60_000_000, 3)?;
    for i in 0_u64..400 {
        h.record(base_us + (i % 40) * 150)?;
    }
    for i in 0_u64..90 {
        h.record(base_us * 3 + i * 400)?;
    }
    for i in 0_u64..10 {
        h.record(base_us * 12 + i * 5_000)?;
    }
    Ok(h)
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let loaded = load_clean_root(std::path::Path::new("artifacts"))?;
    let (_, poc) = performance_case_of_class(&loaded, PerfClass::Poc, "POC")?;

    let operations = vec![
        OperationMeasurement::from_histogram("create_composition", &histogram(4_000)?, 1)?,
        OperationMeasurement::from_histogram("execute_stored_query", &histogram(9_000)?, 0)?,
    ];
    let offered_load_sustained = 25.0;
    let (verdict, violations) = class_verdict(poc, offered_load_sustained, &operations)?;

    let results = Results {
        sut: Sut {
            name: "example-cdr".to_owned(),
            version: "0.0.0-example".to_owned(),
        },
        runner: Runner {
            name: "veredictum".to_owned(),
            version: env!("CARGO_PKG_VERSION").to_owned(),
            verification_pack_status: VerificationPackStatus::NotRun,
        },
        schedule_release: "cnf-2.0-w2".to_owned(),
        tech_profile: TechProfile {
            its: ItsName::ItsRest,
            formats: vec![FormatName::CanonicalJson],
        },
        ixit_digest: ixit_digest("{\"example\":true}"),
        statement_digest: Some(statement_digest("{\"example\":\"statement\"}")),
        selection_basis: Some(SelectionBasis::Statement),
        restapi_specs_version: Some("1.1.0".to_owned()),
        outcomes: vec![
            outcome(
                "I_EHR_SERVICE.create_ehr-main",
                OutcomeStatus::Passed,
                None,
                None,
            )?,
            outcome(
                "I_EHR_SERVICE.create_ehr-conflict",
                OutcomeStatus::Failed,
                Some("expected 409, observed 500"),
                None,
            )?,
            outcome(
                "I_EHR_COMPOSITION.get_composition-main",
                OutcomeStatus::NotApplicable,
                None,
                Some(
                    "ITS-REST composition §Requirements — the profile does not declare the optional branch",
                ),
            )?,
        ],
        measurements: vec![Measurement {
            case: poc_case_id(poc),
            class: PerfClass::Poc,
            environment: Environment {
                exclusive_server: false,
                hardware_class: "example laptop".to_owned(),
                cores: 8,
                memory_gb: 16,
                storage_class: "nvme ssd".to_owned(),
                topology: "single node".to_owned(),
            },
            offered_load_sustained,
            warmup_s: 60,
            duration_s: 3600,
            operations,
            verdict,
            violations,
            resources: None,
        }],
        ambiguity_dispositions: Vec::new(),
    };

    results
        .check_invariants()
        .map_err(|errors| format!("example violates its own invariants: {errors:?}"))?;

    let json = serde_json::to_string_pretty(&results)?;
    // The catalogue is read from the working directory and the document is
    // written beside this file, so the one documented invocation reaches both.
    // One optional argument names another target, which is how the drift gate
    // renders the document without writing into the tree.
    let out = match std::env::args().nth(1) {
        Some(path) => std::path::PathBuf::from(path),
        None => std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("examples")
            .join("results.example.json"),
    };
    std::fs::write(out, json + "\n")?;
    Ok(())
}

fn poc_case_id(case: &veredictum::perf::PerformanceCase) -> CaseId {
    case.id.clone()
}
