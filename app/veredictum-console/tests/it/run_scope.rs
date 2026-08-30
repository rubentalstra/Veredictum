// SPDX-FileCopyrightText: Veredictum contributors
// SPDX-License-Identifier: Apache-2.0

//! The Scope screen's acceptance gates: the preview equals what the engine
//! then actually processes (#65), the client-safe draft view carries no
//! secret, a pasted claim is schema-validated before it is stored (#101), and
//! a tier-composed claim is the lib's own matrix walk, judged as the tier the
//! operator checked (#100).

use veredictum_console::engine::{Credential, Engine, Secret};
use veredictum_console::run_api::{AuthChoice, RunDraft, ScopeTier};

use crate::engine_gate;

/// The filter the equality gate drives: a handful of cases, so the run is
/// seconds.
const SCOPE_FILTER: &str = "I_EHR_SERVICE.create_ehr";

/// One row of the engine's exception document — typed, per the banned
/// `serde_json::Value` carrier rule.
#[derive(serde::Deserialize)]
struct ExceptionRow {
    case: String,
}

#[expect(
    clippy::panic_in_result_fn,
    reason = "the Book ch11 Result-returning test shape: assertions panic, plumbing propagates with ? (https://doc.rust-lang.org/book/ch11-01-writing-tests.html)"
)]
#[test]
fn the_scope_preview_counts_what_the_engine_processes() -> Result<(), Box<dyn std::error::Error>> {
    let binary = engine_gate::gate_binary();
    if !binary.exists() {
        eprintln!(
            "SKIPPED(no engine binary at {}): build the workspace first",
            binary.display()
        );
        return Ok(());
    }
    let engine = Engine::verified(&binary)?;

    let root = engine_gate::repo_root().join("artifacts");
    let specs = engine_gate::repo_root().join("specs/openehr");
    let state = veredictum_console::state::ConsoleState {
        root: root.clone(),
        specs,
        party: engine_gate::repo_root().join("party"),
        out: engine_gate::repo_root().join("out"),
        catalogue: std::sync::Arc::new(
            veredictum::pipeline::catalogue::validate_tree(&root, None).map_err(|e| e.to_string()),
        ),
        draft: std::sync::Arc::new(std::sync::Mutex::new(
            veredictum_console::run_api::Drafts::new(),
        )),
        client_ip_header: None,
        sign_key: None,
        verify_key: None,
        jobs: veredictum_console::run_job::JobSlot::default(),
        posture: veredictum_console::posture::Posture::Local,
        rates: veredictum_console::rate_limit::RateLimiter::default(),
        capture: false,
    };
    let preview = veredictum_console::run_api::read::scope_preview(&state, SCOPE_FILTER)
        .map_err(|e| format!("preview: {e}"))?;

    // The engine drives the same scope against the deterministic fixture SUT;
    // every case in scope lands as an outcome or a recorded exception.
    let scratch = assert_fs::TempDir::new()?;
    let port = engine_gate::fixture_sut()?;
    let ixit = engine_gate::write_ixit(scratch.path(), port)?;
    let out = scratch.path().join("scope-run");
    std::fs::create_dir_all(&out)?;
    let finished = engine.run(
        &veredictum_console::engine::RunSpec {
            root,
            ixit,
            out_dir: out.clone(),
            sut_name: String::from("scope-gate"),
            sut_version: String::from("0.0.0-gate"),
            statement: None,
            filter: Some(String::from(SCOPE_FILTER)),
            progress: false,
            record_exchanges: false,
            credentials: vec![
                Credential {
                    name: String::from("GATE_SUT_USER"),
                    value: Secret::new(String::from("gate-user")),
                },
                Credential {
                    name: String::from("GATE_SUT_PASS"),
                    value: Secret::new(String::from("gate-pass")),
                },
            ],
        },
        |_line| {},
    )?;
    let exceptions: Vec<ExceptionRow> =
        serde_json::from_str(&std::fs::read_to_string(&finished.exceptions_path)?)?;
    // Outcome records are per case × FORMAT, so the case-level scope is the
    // distinct id set across outcomes and exceptions.
    let mut processed: std::collections::BTreeSet<String> = finished
        .results
        .outcomes
        .iter()
        .map(|outcome| outcome.case.to_string())
        .collect();
    for exception in &exceptions {
        processed.insert(exception.case.clone());
    }
    assert_eq!(
        u64::try_from(processed.len())?,
        preview.total,
        "the preview promised a scope the engine did not process"
    );
    assert!(preview.total > 0, "the gate filter selected nothing at all");
    Ok(())
}

#[expect(
    clippy::panic_in_result_fn,
    reason = "the Book ch11 Result-returning test shape: assertions panic, plumbing propagates with ? (https://doc.rust-lang.org/book/ch11-01-writing-tests.html)"
)]
#[test]
fn the_draft_view_carries_no_secret() -> Result<(), Box<dyn std::error::Error>> {
    let state = veredictum_console::state::ConsoleState {
        root: "artifacts".into(),
        specs: "specs/openehr".into(),
        party: "party".into(),
        out: "out".into(),
        catalogue: std::sync::Arc::new(Err(String::from("unused"))),
        draft: std::sync::Arc::new(std::sync::Mutex::new(
            veredictum_console::run_api::Drafts::new(),
        )),
        client_ip_header: None,
        sign_key: None,
        verify_key: None,
        jobs: veredictum_console::run_job::JobSlot::default(),
        posture: veredictum_console::posture::Posture::Local,
        rates: veredictum_console::rate_limit::RateLimiter::default(),
        capture: false,
    };
    veredictum_console::run_api::read::save_connection(
        &state,
        engine_gate::gate_submitter(),
        RunDraft {
            base_url: String::from("http://cdr.example"),
            sut_name: String::from("x"),
            sut_version: String::from("y"),
            auth: AuthChoice::Basic,
            credentials: vec![Credential {
                name: String::from("CONSOLE_SUT_PASS"),
                value: Secret::new(String::from("hunter2-super-secret")),
            }],
            probed_ok: true,
            statement_json: None,
            statement_product: None,
            filter: None,
            record_exchanges: false,
        },
    )
    .map_err(|e| format!("save: {e}"))?;
    let view = veredictum_console::run_api::read::draft_view(&state, engine_gate::gate_submitter())
        .ok_or("the draft must read back")?;
    let serialized = serde_json::to_string(&view)?;
    assert!(
        !serialized.contains("hunter2"),
        "a secret value reached the client-safe view: {serialized}"
    );
    // The debug rendering of the whole draft redacts too.
    let debugged = format!("{:?}", state.draft.lock().map_err(|e| e.to_string())?);
    assert!(
        !debugged.contains("hunter2"),
        "a secret value reached a Debug rendering: {debugged}"
    );
    Ok(())
}

/// A fresh state over the repository mounts with a connected draft, for the
/// claim gates below.
fn drafted_state() -> veredictum_console::state::ConsoleState {
    let root = engine_gate::repo_root().join("artifacts");
    veredictum_console::state::ConsoleState {
        root: root.clone(),
        specs: engine_gate::repo_root().join("specs/openehr"),
        party: engine_gate::repo_root().join("party"),
        out: engine_gate::repo_root().join("out"),
        catalogue: std::sync::Arc::new(
            veredictum::pipeline::catalogue::validate_tree(&root, None).map_err(|e| e.to_string()),
        ),
        draft: std::sync::Arc::new(std::sync::Mutex::new(engine_gate::drafts_of(RunDraft {
            base_url: String::from("http://unused"),
            sut_name: String::from("claim-gate"),
            sut_version: String::from("0.0.0-gate"),
            auth: AuthChoice::None,
            credentials: vec![],
            probed_ok: true,
            statement_json: None,
            statement_product: None,
            filter: None,
            record_exchanges: false,
        }))),
        sign_key: None,
        verify_key: None,
        jobs: veredictum_console::run_job::JobSlot::default(),
        client_ip_header: None,
        posture: veredictum_console::posture::Posture::Local,
        rates: veredictum_console::rate_limit::RateLimiter::default(),
        capture: false,
    }
}

/// #101: a pasted claim is held to the PUBLISHED statement schema — a
/// committed example passes with its summary, a shape serde would tolerate
/// but the schema forbids is refused, and non-JSON is refused.
#[expect(
    clippy::panic_in_result_fn,
    reason = "the Book ch11 Result-returning test shape: assertions panic, plumbing propagates with ? (https://doc.rust-lang.org/book/ch11-01-writing-tests.html)"
)]
#[test]
fn a_pasted_claim_is_schema_validated_before_it_is_stored() -> Result<(), Box<dyn std::error::Error>>
{
    let state = drafted_state();
    let body =
        std::fs::read_to_string(engine_gate::repo_root().join("party/ehrbase/statement.json"))?;
    let summary = veredictum_console::run_api::read::save_scope(
        &state,
        engine_gate::gate_submitter(),
        Some(body),
        None,
        false,
    )
    .map_err(|e| format!("a committed example must pass: {e}"))?
    .ok_or("a pasted claim must yield a summary")?;
    assert_eq!(summary.product, "EHRbase 2.34.0");
    assert!(
        !summary.profiles.is_empty(),
        "the ehrbase example claims at least one tier"
    );

    // additionalProperties: false — serde would ignore the stray key, the
    // published schema refuses it.
    let stray = veredictum_console::run_api::read::save_scope(
        &state,
        engine_gate::gate_submitter(),
        Some(String::from(
            r#"{"product":{"name":"x","version":"1","vendor":"v","identifier":"urn:x"},"schedule_release":"cnf-2.0-w2","claims":{},"stray_key":true}"#,
        )),
        None,
        false,
    );
    assert!(
        stray.is_err(),
        "an undeclared key must be refused by the schema"
    );

    let not_json = veredictum_console::run_api::read::save_scope(
        &state,
        engine_gate::gate_submitter(),
        Some(String::from("not json")),
        None,
        false,
    );
    assert!(not_json.is_err(), "non-JSON must be refused");

    // The honest no-claim run stays legal: nothing pasted, no summary.
    let none = veredictum_console::run_api::read::save_scope(
        &state,
        engine_gate::gate_submitter(),
        None,
        None,
        false,
    )
    .map_err(|e| format!("no-claim save: {e}"))?;
    assert_eq!(none, None);
    Ok(())
}

/// The tier walk the Scope row renders, re-derived here through the same
/// published lib call the judgement uses.
fn expected_tier_cases(
    validation: &veredictum::pipeline::catalogue::Validation,
    tier: veredictum::vocab::Tier,
) -> std::collections::BTreeSet<String> {
    let Some((_, matrix)) = validation.loaded.set.matrix.as_ref() else {
        return std::collections::BTreeSet::new();
    };
    let members = veredictum::verdict::tier_members(tier, matrix);
    validation
        .loaded
        .set
        .cases
        .iter()
        .filter(|(_, case)| case.capabilities.iter().any(|cap| members.contains(cap)))
        .map(|(_, case)| case.id.to_string())
        .collect()
}

/// #100: the tier row's counts ARE the lib's own `tier_members` walk over the
/// committed catalogue, and the tier chain shows in the case sets — STANDARD
/// contains CORE, and the Security rung is its own family.
#[expect(
    clippy::panic_in_result_fn,
    reason = "the Book ch11 Result-returning test shape: assertions panic, plumbing propagates with ? (https://doc.rust-lang.org/book/ch11-01-writing-tests.html)"
)]
#[test]
fn the_tier_row_counts_are_the_libs_own_matrix_walk() -> Result<(), Box<dyn std::error::Error>> {
    let state = drafted_state();
    let validation = state.catalogue.as_ref().as_ref().map_err(Clone::clone)?;
    let rows = veredictum_console::run_api::read::tier_rows(&state)
        .map_err(|e| format!("tier rows: {e}"))?;
    assert_eq!(rows.len(), 4, "the row offers exactly the four tiers");

    for row in &rows {
        let tier = match row.tier {
            ScopeTier::Core => veredictum::vocab::Tier::Core,
            ScopeTier::Standard => veredictum::vocab::Tier::Standard,
            ScopeTier::Options => veredictum::vocab::Tier::Options,
            ScopeTier::SecBasic => veredictum::vocab::Tier::SecBasic,
        };
        let expected = expected_tier_cases(validation, tier);
        assert_eq!(
            row.cases,
            u64::try_from(expected.len())?,
            "{} counted cases the lib's own walk does not gate",
            row.tier.token()
        );
        assert!(
            row.capabilities > 0,
            "{} resolved to no capabilities at all",
            row.tier.token()
        );
    }

    let core = expected_tier_cases(validation, veredictum::vocab::Tier::Core);
    let standard = expected_tier_cases(validation, veredictum::vocab::Tier::Standard);
    let security = expected_tier_cases(validation, veredictum::vocab::Tier::SecBasic);
    assert!(
        core.is_subset(&standard),
        "STANDARD is the CORE chain plus its own capabilities, so it cannot gate fewer cases"
    );
    assert!(!security.is_empty(), "the Security rung gates cases");
    assert!(
        !security.is_subset(&standard),
        "the Security rung is its own family, so its cases are not the Platform chain's"
    );
    Ok(())
}

/// #100: a composed CORE claim drives a real run and is judged — the tier the
/// operator checked is the tier the verdict answers for, and the claim is
/// legal under the lib's own static review.
#[expect(
    clippy::panic_in_result_fn,
    reason = "the Book ch11 Result-returning test shape: assertions panic, plumbing propagates with ? (https://doc.rust-lang.org/book/ch11-01-writing-tests.html)"
)]
#[test]
fn a_composed_core_claim_is_judged_as_the_core_profile() -> Result<(), Box<dyn std::error::Error>> {
    let binary = engine_gate::gate_binary();
    if !binary.exists() {
        eprintln!(
            "SKIPPED(no engine binary at {}): build the workspace first",
            binary.display()
        );
        return Ok(());
    }
    let engine = Engine::verified(&binary)?;

    let state = drafted_state();
    let document = veredictum_console::run_api::read::compose_claim(
        &state,
        engine_gate::gate_submitter(),
        &[ScopeTier::Core],
    )
    .map_err(|e| format!("compose: {e}"))?;

    let scratch = assert_fs::TempDir::new()?;
    let port = engine_gate::fixture_sut()?;
    let ixit = engine_gate::write_ixit(scratch.path(), port)?;
    let out = scratch.path().join("tier-run");
    std::fs::create_dir_all(&out)?;
    // The claim travels with the run exactly as `start_run` writes it.
    let statement_path = out.join("statement.json");
    std::fs::write(&statement_path, &document)?;
    let root = engine_gate::repo_root().join("artifacts");
    let finished = engine.run(
        &veredictum_console::engine::RunSpec {
            root: root.clone(),
            ixit,
            out_dir: out.clone(),
            sut_name: String::from("claim-gate"),
            sut_version: String::from("0.0.0-gate"),
            statement: Some(statement_path.clone()),
            filter: Some(String::from(SCOPE_FILTER)),
            progress: false,
            record_exchanges: false,
            credentials: vec![
                Credential {
                    name: String::from("GATE_SUT_USER"),
                    value: Secret::new(String::from("gate-user")),
                },
                Credential {
                    name: String::from("GATE_SUT_PASS"),
                    value: Secret::new(String::from("gate-pass")),
                },
            ],
        },
        |_line| {},
    )?;
    assert!(
        !finished.results.outcomes.is_empty(),
        "the composed CORE claim selected nothing to drive"
    );

    let judgement = veredictum::pipeline::judgement::judge(
        &veredictum::pipeline::judgement::JudgementRequest {
            statement: &statement_path,
            results: &finished.results_path,
            root: &root,
        },
    )?;
    // A tier claim is legal as a CLAIM: every required capability of the
    // chain is claimed, and the tier is represented. What it cannot decide is
    // which branch of an `option_select` register entry the server realizes,
    // so those declaration findings are the honest residue and the only one.
    for finding in &judgement.report.review {
        assert!(
            finding.message.starts_with("option_select "),
            "a composed tier claim must raise no claim-legality finding: {}",
            finding.message
        );
    }
    let (_, core) = judgement
        .report
        .profiles
        .iter()
        .find(|(tier, _)| *tier == veredictum::vocab::Tier::Core)
        .ok_or("the judged profiles must carry CORE")?;
    assert_ne!(
        *core,
        veredictum::verdict::ProfileVerdict::NotClaimed,
        "a composed CORE claim must be judged as claimed"
    );
    Ok(())
}

/// #100: the composed claim is a real statement — it passes the PUBLISHED
/// schema through the same save path a pasted one takes, and it claims
/// exactly the checked tiers.
#[expect(
    clippy::panic_in_result_fn,
    reason = "the Book ch11 Result-returning test shape: assertions panic, plumbing propagates with ? (https://doc.rust-lang.org/book/ch11-01-writing-tests.html)"
)]
#[test]
fn a_composed_tier_claim_saves_through_the_pasted_claim_path()
-> Result<(), Box<dyn std::error::Error>> {
    let state = drafted_state();
    let document = veredictum_console::run_api::read::compose_claim(
        &state,
        engine_gate::gate_submitter(),
        &[ScopeTier::Core, ScopeTier::SecBasic],
    )
    .map_err(|e| format!("compose: {e}"))?;
    let summary = veredictum_console::run_api::read::save_scope(
        &state,
        engine_gate::gate_submitter(),
        Some(document.clone()),
        None,
        false,
    )
    .map_err(|e| format!("the composed claim must pass its own schema: {e}"))?
    .ok_or("a composed claim must yield a summary")?;
    assert_eq!(summary.product, "claim-gate 0.0.0-gate");
    assert_eq!(summary.profiles, vec!["CORE", "SEC-BASIC"]);

    let validation = state.catalogue.as_ref().as_ref().map_err(Clone::clone)?;
    let (_, matrix) = validation
        .loaded
        .set
        .matrix
        .as_ref()
        .ok_or("the catalogue must carry its capability matrix")?;
    let expected = veredictum::verdict::tier_members(veredictum::vocab::Tier::Core, matrix).len()
        + veredictum::verdict::tier_members(veredictum::vocab::Tier::SecBasic, matrix).len();
    assert_eq!(summary.capabilities, u64::try_from(expected)?);

    // The empty selection certifies nothing, so it is refused rather than
    // composed into a claim with no profile.
    assert!(
        veredictum_console::run_api::read::compose_claim(
            &state,
            engine_gate::gate_submitter(),
            &[]
        )
        .is_err()
    );
    Ok(())
}

/// #101: the example loader serves only a statement.json under the mounted
/// party tree — anything else is refused, path traversal included.
#[expect(
    clippy::panic_in_result_fn,
    reason = "the Book ch11 Result-returning test shape: assertions panic, plumbing propagates with ? (https://doc.rust-lang.org/book/ch11-01-writing-tests.html)"
)]
#[test]
fn the_example_loader_refuses_paths_outside_the_party_tree()
-> Result<(), Box<dyn std::error::Error>> {
    let state = drafted_state();
    let good = veredictum_console::run_api::read::statement_body(
        &state,
        &engine_gate::repo_root()
            .join("party/ehrbase/statement.json")
            .display()
            .to_string(),
    )
    .map_err(|e| format!("the committed example must load: {e}"))?;
    assert!(good.contains("EHRbase"));

    for refused in [
        engine_gate::repo_root().join("Cargo.toml"),
        engine_gate::repo_root().join("party/ehrbase/../../Cargo.toml"),
        engine_gate::repo_root().join("party/ehrbase/ixit.json"),
    ] {
        let answer = veredictum_console::run_api::read::statement_body(
            &state,
            &refused.display().to_string(),
        );
        assert!(answer.is_err(), "{} must be refused", refused.display());
    }
    Ok(())
}
