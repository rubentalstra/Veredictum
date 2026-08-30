// SPDX-FileCopyrightText: Veredictum contributors
// SPDX-License-Identifier: Apache-2.0

//! The #391 acceptance gates: one composed submission over a real driven run.
//!
//! Nobody can hand a test a GitHub App, so the network is the one thing these
//! gates cannot reach. Everything up to it is proved here: the submission
//! document against the PUBLISHED registry-entry schema, the file layout, the
//! entry id's date against the run's own start, the named refusal of every
//! mandatory field, and the property the whole credential posture rests on —
//! a run driven with a Basic credential submits no byte of it, anywhere.
//!
//! The client's own request sequence is proved beside them, against a stub
//! API: `github_client.rs`.

#![allow(
    clippy::print_stderr,
    reason = "the skip-with-reason lines ARE this gate's report, the same shape run_live.rs uses"
)]

use std::path::Path;
use std::sync::{Arc, Mutex};

use veredictum_console::engine::{Credential, Engine, Secret};
use veredictum_console::github::AppConfig;
use veredictum_console::run_api::{AuthChoice, RunDraft, StartOutcome};
use veredictum_console::run_job::{JobSlot, JobStatus, RunId};
use veredictum_console::state::ConsoleState;
use veredictum_console::submit_api::read::{Composed, SubmitError, compose_with, screen_with};
use veredictum_console::submit_api::{DisclosureForm, SubmitScreen};

use crate::engine_gate;

/// The case the gate drives: one small isolated case, so the run is seconds.
const FILTER: &str = "I_EHR_SERVICE.create_ehr-main";

/// Blanks one mandatory field of an otherwise complete disclosure.
type Blank = fn(&mut DisclosureForm);

/// The drafted Basic credentials. Distinctive enough that a search over the
/// submitted bytes proves absence rather than coincidence.
const SUT_USER: &str = "registry-operator";
const SUT_PASS: &str = "hunter3-never-submitted";

/// A registry identity the gate hands the seam, so no test mutates the
/// process environment out from under another.
fn identity() -> AppConfig {
    AppConfig {
        app_id: String::from("1234567"),
        key_file: engine_gate::repo_root().join("party/smart/cnf-smart-test.key.pem"),
        installation_id: String::from("89012345"),
        repo: String::from("rubentalstra/Veredictum"),
        api_base: String::from("https://api.github.com"),
    }
}

/// A disclosure with every mandatory field filled, which is what the refusal
/// tests below blank one at a time.
fn filled_form() -> DisclosureForm {
    DisclosureForm {
        submitter_name: String::from("A Person"),
        submitter_contact: String::from("mailto:person@example.invalid"),
        relationship: String::from("independent"),
        system: String::from("gate-cdr"),
        display_name: String::from("Gate CDR"),
        version: String::from("1.2.3"),
        reproduction_authorized: String::from("no"),
        environment_os: String::from("Linux 6.8"),
        environment_arch: String::from("x86_64"),
        environment_host_class: String::from("8 vCPU cloud VM"),
        environment_cpu_model: String::from("Fictional Xeon"),
        environment_cores: String::from("8"),
        environment_memory_bytes: String::from("17179869184"),
        sut_configuration: String::from("Basic authentication on, validation strict, no audit."),
        conflict_of_interest: String::from("None: the submitter operates no CDR."),
    }
}

/// A console state over the repository's own mounts, holding one credentialed
/// draft for the gate's submitter.
fn state_over(out: &Path, port: u16, statement: String) -> ConsoleState {
    let root = engine_gate::repo_root().join("artifacts");
    ConsoleState {
        root: root.clone(),
        specs: engine_gate::repo_root().join("specs/openehr"),
        party: engine_gate::repo_root().join("party"),
        out: out.to_path_buf(),
        sign_key: None,
        verify_key: None,
        // The submission seam judges through the published lib, which loads
        // the catalogue itself; the startup validation is not what it reads.
        catalogue: Arc::new(Err(String::from("unused by the submission seam"))),
        draft: Arc::new(Mutex::new(engine_gate::drafts_of(RunDraft {
            base_url: format!("http://127.0.0.1:{port}"),
            sut_name: String::from("Gate CDR"),
            sut_version: String::from("1.2.3"),
            auth: AuthChoice::Basic,
            credentials: vec![
                Credential {
                    name: String::from("CONSOLE_SUT_USER"),
                    value: Secret::new(String::from(SUT_USER)),
                },
                Credential {
                    name: String::from("CONSOLE_SUT_PASS"),
                    value: Secret::new(String::from(SUT_PASS)),
                },
            ],
            probed_ok: true,
            statement_json: Some(statement),
            statement_product: Some(String::from("EHRbase 2.34.0")),
            filter: Some(String::from(FILTER)),
            // The property the console tier rests on: CI re-derives the
            // judgement from the recorded exchanges, so a submission without
            // them could never be checked.
            record_exchanges: true,
        }))),
        jobs: JobSlot::default(),
        client_ip_header: None,
        capture: false,
    }
}

/// Polls the map until the named job leaves `Running`, bounded.
fn wait_terminal(slot: &JobSlot, id: RunId) -> Option<veredictum_console::run_job::JobView> {
    for _ in 0..600 {
        let view = slot.view_of(id).ok()??;
        if view.status.is_terminal() {
            return Some(view);
        }
        std::thread::sleep(std::time::Duration::from_millis(100));
    }
    None
}

/// One driven run and the submission composed over it, with the scratch tree
/// it lives in.
struct Driven {
    /// The output tree the run wrote into, held so it outlives every reader.
    _scratch: assert_fs::TempDir,
    /// The console state whose job map holds the run.
    state: ConsoleState,
    /// The composed submission.
    composed: Composed,
}

/// Drives one real run with a Basic credential, then composes its submission.
#[expect(
    clippy::panic_in_result_fn,
    reason = "the Book ch11 Result-returning shape: the run's terminal status is asserted, plumbing propagates with ?"
)]
fn submitted() -> Result<Option<Driven>, Box<dyn std::error::Error>> {
    let binary = engine_gate::gate_binary();
    if !binary.exists() {
        eprintln!(
            "SKIPPED(no engine binary at {}): build the workspace first",
            binary.display()
        );
        return Ok(None);
    }
    let engine = Engine::verified(&binary)?;
    let scratch = assert_fs::TempDir::new()?;
    let out = scratch.path().join("out");
    std::fs::create_dir_all(&out)?;
    let port = engine_gate::fixture_sut()?;
    let statement =
        std::fs::read_to_string(engine_gate::repo_root().join("party/ehrbase/statement.json"))?;
    let state = state_over(&out, port, statement);

    let StartOutcome::Accepted(id) = veredictum_console::run_api::read::start_run_with(
        &state,
        engine_gate::gate_submitter(),
        &engine,
    )?
    else {
        return Err("a first start from a submitter with no run in flight is accepted".into());
    };
    let terminal = wait_terminal(&state.jobs, id).ok_or("the job never left Running")?;
    assert_eq!(
        terminal.status,
        JobStatus::Finished,
        "tail: {:?}",
        terminal.tail
    );

    let composed = compose_with(
        &state,
        engine_gate::gate_submitter(),
        &filled_form(),
        &identity(),
    )?;
    // The scratch directory travels with the state: every caller re-reads the
    // run's own files out of it.
    Ok(Some(Driven {
        _scratch: scratch,
        state,
        composed,
    }))
}

/// The stub `console` provenance block standing in for the one CI writes.
///
/// The submitted document carries none: a performer does not state its own
/// provenance, and the published schema REQUIRES the block, which is exactly
/// what makes an unprovenance document a submission and not yet an entry.
#[expect(
    clippy::disallowed_types,
    reason = "the emitted-schemas family: the published JSON Schema is applied to the document as a value, which is how the gate itself applies it"
)]
fn with_stub_provenance(
    document: &str,
    record: &str,
) -> Result<serde_json::Value, serde_json::Error> {
    let mut value: serde_json::Value = serde_json::from_str(document)?;
    if let Some(object) = value.as_object_mut() {
        object.insert(
            String::from("provenance"),
            serde_json::json!({
                "tier": "console",
                "instrument_origin": "https://console.veredictum.eu",
                "console_run_id": "3f2504e0-4f89-41d3-9a0c-0305e82c3301",
                "workflow_ref": "rubentalstra/Veredictum/.github/workflows/registry-console.yml@refs/heads/main",
                "run_id": "42",
                "run_attempt": 1,
                "scheme": "openpgp-detached",
                "signature": format!("{record}/record-manifest.json.asc"),
                "signs": format!("{record}/record-manifest.json"),
                "identity": "0123456789ABCDEF",
                "verify_command": format!("veredictum verify-record --record {record} --key registry/keys/registry-signing.pub.asc"),
            }),
        );
    }
    Ok(value)
}

#[expect(
    clippy::panic_in_result_fn,
    reason = "the Book ch11 Result-returning test shape: assertions panic, plumbing propagates with ?"
)]
#[expect(
    clippy::disallowed_types,
    reason = "the emitted-schemas family: the published JSON Schema is read and applied as a value, which is what a schema validator takes"
)]
#[expect(
    clippy::too_many_lines,
    reason = "one gate walks the whole submission — the schema, the layout, the digests, the dated id and the credential absence — and splitting it would drive the run several times over"
)]
#[test]
fn a_composed_submission_holds_the_published_rules() -> Result<(), Box<dyn std::error::Error>> {
    let Some(driven) = submitted()? else {
        return Ok(());
    };
    let composed = &driven.composed;

    // The layout the rules define: one entry, and the five record files a
    // re-derivation reads.
    let paths: Vec<&str> = composed
        .files
        .iter()
        .map(|file| file.path.as_str())
        .collect();
    let record = format!("registry/records/gate-cdr/{}", composed.entry_id);
    assert_eq!(
        paths,
        vec![
            format!(
                "registry/entries/conformance/gate-cdr/{}.json",
                composed.entry_id
            )
            .as_str(),
            format!("{record}/results.json").as_str(),
            format!("{record}/verdicts.json").as_str(),
            format!("{record}/transcript.json").as_str(),
            format!("{record}/ixit.json").as_str(),
            format!("{record}/statement.json").as_str(),
        ],
        "the submission adds exactly the six files the rules define"
    );

    let entry_body = &composed.files.first().ok_or("no entry file")?.body;
    let document: serde_json::Value = serde_json::from_str(entry_body)?;

    // The submitted document carries NO provenance block. This is the whole
    // reason the console tier is worth anything: the performer does not get to
    // state its own provenance.
    assert!(
        document.get("provenance").is_none(),
        "the console wrote a provenance block for itself"
    );

    // With the block CI writes, the document is a valid registry entry against
    // the PUBLISHED schema — the same document `scripts/checks` applies.
    let schema_path = engine_gate::repo_root().join("schemas/registry-entry.schema.json");
    let schema: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(&schema_path)?)?;
    let validator = jsonschema::validator_for(&schema)?;
    let completed = with_stub_provenance(entry_body, &record)?;
    let findings: Vec<String> = validator
        .iter_errors(&completed)
        .map(|finding| format!("{}: {finding}", finding.instance_path()))
        .collect();
    assert!(
        findings.is_empty(),
        "the composed entry does not validate against {}:\n  {}",
        schema_path.display(),
        findings.join("\n  ")
    );

    // The entry id's date and the disclosed run start are the same fact: the
    // gate refuses a submission where they disagree.
    let started = document
        .pointer("/disclosure/run_started_at")
        .and_then(serde_json::Value::as_str)
        .ok_or("no run_started_at")?;
    assert!(
        composed
            .entry_id
            .starts_with(started.get(..10).ok_or("a run start shorter than a date")?),
        "entry id {} does not open on the run's start date {started}",
        composed.entry_id
    );

    // Every artifact is pinned by the SHA-256 of the exact bytes the branch
    // will carry.
    let artifacts = document
        .get("artifacts")
        .and_then(serde_json::Value::as_array)
        .ok_or("no artifacts")?;
    assert_eq!(artifacts.len(), 5, "five record artifacts");
    for artifact in artifacts {
        let path = artifact
            .get("path")
            .and_then(serde_json::Value::as_str)
            .ok_or("an artifact with no path")?;
        let pinned = artifact
            .get("sha256")
            .and_then(serde_json::Value::as_str)
            .ok_or("an artifact with no digest")?;
        let file = composed
            .files
            .iter()
            .find(|file| file.path == path)
            .ok_or_else(|| format!("{path} is pinned but not committed"))?;
        let computed = digest_of(file.body.as_bytes());
        assert_eq!(computed, pinned, "{path}");
    }

    // The instrument version is the engine the console links, and the
    // catalogue revision is the results record's own.
    assert_eq!(
        document
            .pointer("/disclosure/instrument_version")
            .and_then(serde_json::Value::as_str),
        Some(veredictum_console::ENGINE_PIN)
    );
    assert_eq!(
        document
            .pointer("/subject/deployment/kind")
            .and_then(serde_json::Value::as_str),
        Some("hosted-endpoint")
    );

    // The branch is the contract the re-derivation lane reads the run id out
    // of, and the pull request says what CI will do.
    assert_eq!(
        composed.branch,
        format!("console-run/{}", composed.run_id),
        "the lane reads the run id out of the branch name"
    );
    assert!(
        composed.body.contains(&composed.entry_id),
        "{}",
        composed.body
    );
    assert!(
        composed.body.contains("no provenance block"),
        "{}",
        composed.body
    );

    // The property the whole credential posture rests on: the run was driven
    // with a Basic credential, and no submitted byte carries it — not the
    // entry, not the record, not the branch, not the pull request.
    for file in &composed.files {
        assert!(
            !file.body.contains(SUT_PASS),
            "{} carries the credential value",
            file.path
        );
        assert!(
            !file.body.contains(SUT_USER),
            "{} carries the credential user",
            file.path
        );
    }
    for text in [
        &composed.branch,
        &composed.title,
        &composed.body,
        &composed.message,
    ] {
        assert!(!text.contains(SUT_PASS), "{text}");
        assert!(!text.contains(SUT_USER), "{text}");
    }
    // And the ixit that IS submitted still names the variables, so the record
    // says how the run was authenticated without saying with what.
    let ixit = composed
        .files
        .iter()
        .find(|file| file.path.ends_with("/ixit.json"))
        .ok_or("no ixit in the submission")?;
    assert!(ixit.body.contains("CONSOLE_SUT_PASS"), "{}", ixit.body);
    Ok(())
}

/// Lowercase-hex SHA-256, derived here rather than trusted from the seam it
/// is checking.
fn digest_of(bytes: &[u8]) -> String {
    use sha2::{Digest as _, Sha256};
    use std::fmt::Write as _;
    Sha256::digest(bytes)
        .iter()
        .fold(String::with_capacity(64), |mut out, byte| {
            let _ = write!(out, "{byte:02x}");
            out
        })
}

/// Every mandatory field is refused empty BY NAME, before anything is opened.
#[expect(
    clippy::panic_in_result_fn,
    reason = "the Book ch11 Result-returning test shape: assertions panic, plumbing propagates with ?"
)]
#[test]
fn an_empty_mandatory_field_is_refused_by_name() -> Result<(), Box<dyn std::error::Error>> {
    let Some(driven) = submitted()? else {
        return Ok(());
    };
    let state = &driven.state;
    let blanks: [(Blank, &str); 11] = [
        (|f| f.submitter_name.clear(), "submitter.name"),
        (|f| f.submitter_contact.clear(), "submitter.contact"),
        (|f| f.relationship.clear(), "submitter.relationship"),
        (|f| f.system.clear(), "subject.system"),
        (|f| f.display_name.clear(), "subject.display_name"),
        (|f| f.version.clear(), "subject.version"),
        (
            |f| f.reproduction_authorized.clear(),
            "subject.deployment.reproduction_authorized",
        ),
        (|f| f.environment_os.clear(), "disclosure.environment.os"),
        (
            |f| f.environment_host_class.clear(),
            "disclosure.environment.host_class",
        ),
        (
            |f| f.sut_configuration.clear(),
            "disclosure.sut_configuration",
        ),
        (
            |f| f.conflict_of_interest.clear(),
            "disclosure.conflict_of_interest",
        ),
    ];
    for (blank, field) in blanks {
        let mut form = filled_form();
        blank(&mut form);
        let refusal = compose_with(state, engine_gate::gate_submitter(), &form, &identity())
            .err()
            .ok_or_else(|| format!("an empty {field} was accepted"))?;
        assert!(
            matches!(refusal, SubmitError::Empty { field: named } if named == field),
            "an empty {field} was refused as: {refusal}"
        );
        assert!(refusal.to_string().contains(field), "{refusal}");
    }

    // A whitespace-only value is empty too: the rules refuse an empty value,
    // and a space is not a disclosure.
    let mut form = filled_form();
    form.conflict_of_interest = String::from("   ");
    let refusal = compose_with(state, engine_gate::gate_submitter(), &form, &identity())
        .err()
        .ok_or("a blank conflict-of-interest sentence was accepted")?;
    assert!(matches!(refusal, SubmitError::Empty { .. }), "{refusal}");

    // A malformed value is a different refusal, and it names the field too.
    let mut form = filled_form();
    form.environment_cores = String::from("many");
    let refusal = compose_with(state, engine_gate::gate_submitter(), &form, &identity())
        .err()
        .ok_or("a non-numeric core count was accepted")?;
    assert!(
        refusal.to_string().contains("disclosure.environment.cores"),
        "{refusal}"
    );

    let mut form = filled_form();
    form.system = String::from("Gate CDR");
    let refusal = compose_with(state, engine_gate::gate_submitter(), &form, &identity())
        .err()
        .ok_or("a system id that is not a registry slug was accepted")?;
    assert!(refusal.to_string().contains("subject.system"), "{refusal}");
    Ok(())
}

/// An instrument with no registry identity says what to configure and offers
/// no button. It is a state, never a panic and never a half attempt.
#[expect(
    clippy::panic_in_result_fn,
    reason = "the Book ch11 Result-returning test shape: assertions panic, plumbing propagates with ?"
)]
#[test]
fn an_unconfigured_instrument_offers_no_submission() -> Result<(), Box<dyn std::error::Error>> {
    let scratch = assert_fs::TempDir::new()?;
    let state = state_over(scratch.path(), 1, String::from("{}"));
    let missing = vec![
        String::from("VEREDICTUM_GITHUB_APP_ID"),
        String::from("VEREDICTUM_REGISTRY_REPO"),
    ];
    let screen = screen_with(&state, engine_gate::gate_submitter(), Err(missing.clone()))?;
    assert_eq!(screen, SubmitScreen::NotConfigured { missing });

    // And with an identity but no run, the screen says THAT rather than
    // inventing a submission.
    assert_eq!(
        screen_with(&state, engine_gate::gate_submitter(), Ok(identity()))?,
        SubmitScreen::NoRun
    );
    Ok(())
}

/// The ready screen states what the run knows, and the same six paths the
/// composition commits.
#[expect(
    clippy::panic_in_result_fn,
    reason = "the Book ch11 Result-returning test shape: assertions panic, plumbing propagates with ?"
)]
#[test]
fn the_ready_screen_states_what_the_run_knows() -> Result<(), Box<dyn std::error::Error>> {
    let Some(driven) = submitted()? else {
        return Ok(());
    };
    let composed = &driven.composed;
    let state = &driven.state;
    let screen = screen_with(state, engine_gate::gate_submitter(), Ok(identity()))?;
    let SubmitScreen::Ready(facts) = screen else {
        panic!("a finished, recorded, claimed run is ready to submit: {screen:?}");
    };
    assert_eq!(facts.run_id, composed.run_id);
    assert_eq!(facts.branch, composed.branch);
    assert_eq!(facts.repo, identity().repo);
    assert_eq!(facts.instrument_version, veredictum_console::ENGINE_PIN);
    assert_eq!(facts.display_name, "Gate CDR");
    assert_eq!(facts.version, "1.2.3");
    assert_eq!(facts.system, "gate-cdr");
    assert!(
        facts.endpoint.starts_with("http://127.0.0.1:"),
        "the endpoint comes from the run's own ixit: {}",
        facts.endpoint
    );
    assert!(!facts.catalogue_revision.is_empty());
    assert_eq!(facts.files.len(), 6);
    // The screen names the same paths the composition commits, so a reader is
    // shown what will actually be added.
    let composed_paths: Vec<String> = composed
        .files
        .iter()
        .map(|file| file.path.clone())
        .collect();
    assert_eq!(facts.files, composed_paths);
    Ok(())
}
