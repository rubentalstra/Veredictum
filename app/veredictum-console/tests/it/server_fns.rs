// SPDX-FileCopyrightText: Veredictum contributors
// SPDX-License-Identifier: Apache-2.0

//! Every `#[server]` endpoint, driven server-side over a real console state.
//!
//! A server function IS a public HTTP endpoint (rules §0), and the console
//! has no login, so what each one does with an idle console and with hostile
//! input is a security property rather than a nicety. On the `ssr` side the
//! macro expansion calls the body directly, so an ordinary call under a
//! reactive owner carrying the state exercises exactly the code a request
//! would (<https://book.leptos.dev/server/25_server_functions.html>).
//!
//! The gates that need a driven engine live in `export_gate`, `run_live` and
//! `run_scope`. What is pinned here is the OTHER half: an endpoint reached
//! before any run exists answers with its honest empty state or its verbatim
//! refusal, and never with a panic.

use std::path::Path;

use leptos::prelude::{Owner, provide_context};
use veredictum_console::state::ConsoleState;

use crate::engine_gate;

/// A console state over the committed mounts, writing into `out`.
fn state_over(out: &Path) -> ConsoleState {
    let root = engine_gate::repo_root().join("artifacts");
    let specs = engine_gate::repo_root().join("specs/openehr");
    let catalogue = veredictum::pipeline::catalogue::validate_tree(&root, Some(&specs))
        .map_err(|e| e.to_string());
    ConsoleState {
        root,
        specs,
        party: engine_gate::repo_root().join("party"),
        out: out.to_path_buf(),
        catalogue: std::sync::Arc::new(catalogue),
        draft: std::sync::Arc::new(std::sync::Mutex::new(None)),
        sign_key: None,
        verify_key: None,
        jobs: veredictum_console::run_job::JobSlot::default(),
        capture: false,
    }
}

/// Installs `state` as the request context the endpoints read, and hands back
/// the owner that holds it.
///
/// The RETURN VALUE is load-bearing: the reactive system tracks the current
/// owner through a `Weak`, so an owner dropped at the end of this function
/// leaves `Owner::current()` empty and every `expect_context` below panics.
/// Bind it for the whole test body.
fn provide(state: &ConsoleState) -> Owner {
    let owner = Owner::new();
    owner.set();
    provide_context(state.clone());
    owner
}

/// The status a probe answer carries, when the server answered at all.
///
/// The variant's own words, never a re-derivation: a probe is a diagnostic
/// about the network path and the console judges nothing about it.
fn probe_status(answer: &veredictum_console::run_api::ProbeAnswer) -> Option<String> {
    match answer {
        veredictum_console::run_api::ProbeAnswer::Answered { status, .. } => Some(status.clone()),
        veredictum_console::run_api::ProbeAnswer::Unreachable { .. } => None,
    }
}

/// The four catalogue endpoints answer from the committed catalogue, and each
/// answers the same thing its reader does — the server fn adds no arithmetic.
#[expect(
    clippy::panic_in_result_fn,
    reason = "the Book ch11 Result-returning test shape: assertions panic, plumbing propagates with ? (https://doc.rust-lang.org/book/ch11-01-writing-tests.html)"
)]
#[tokio::test]
async fn the_catalogue_endpoints_answer_what_their_readers_read()
-> Result<(), Box<dyn std::error::Error>> {
    use veredictum_console::catalogue_api::{fns, read};

    let scratch = assert_fs::TempDir::new()?;
    let state = state_over(scratch.path());
    let _owner = provide(&state);

    assert_eq!(
        fns::fetch_instrument().await.map_err(|e| e.to_string())?,
        read::instrument_view(&state)
    );
    assert_eq!(
        fns::fetch_chapters().await.map_err(|e| e.to_string())?,
        read::chapter_rows(&state)?
    );
    assert_eq!(
        fns::fetch_chapter_bands(String::from("ehr"), String::new(), String::from("CORE"))
            .await
            .map_err(|e| e.to_string())?,
        read::band_rows(&state, "ehr", "", "CORE")?
    );
    let id = String::from("I_EHR_SERVICE.create_ehr-clone_system_id");
    assert_eq!(
        fns::fetch_case_detail(id.clone())
            .await
            .map_err(|e| e.to_string())?,
        read::case_detail(&state, &id)?
    );
    assert_eq!(
        fns::fetch_case_detail(String::from("I_NO_SERVICE.nothing-here"))
            .await
            .map_err(|e| e.to_string())?,
        None,
        "an unknown id is an absent page, not a transport error"
    );
    Ok(())
}

/// The record endpoints reached before any run answer with the empty state
/// the screens render, never with an error the surface has to translate.
#[expect(
    clippy::panic_in_result_fn,
    reason = "the Book ch11 Result-returning test shape: assertions panic, plumbing propagates with ? (https://doc.rust-lang.org/book/ch11-01-writing-tests.html)"
)]
#[tokio::test]
async fn the_record_endpoints_are_empty_before_a_run() -> Result<(), Box<dyn std::error::Error>> {
    use veredictum_console::export_api::{ExportScreen, fns as export_fns};
    use veredictum_console::record_api::{VerdictsScreen, fns as record_fns};

    let scratch = assert_fs::TempDir::new()?;
    let state = state_over(scratch.path());
    let _owner = provide(&state);

    assert_eq!(
        record_fns::fetch_results()
            .await
            .map_err(|e| e.to_string())?,
        None
    );
    assert_eq!(
        record_fns::fetch_result_detail(String::from("I_EHR_SERVICE.create_ehr-main"), None)
            .await
            .map_err(|e| e.to_string())?,
        None
    );
    assert_eq!(
        record_fns::fetch_verdicts()
            .await
            .map_err(|e| e.to_string())?,
        VerdictsScreen::NoRun
    );
    assert_eq!(
        export_fns::fetch_export()
            .await
            .map_err(|e| e.to_string())?,
        ExportScreen::NoRun
    );
    // Sealing with nothing to seal is a refusal that says what to do next.
    let refusal = export_fns::prepare_export()
        .await
        .expect_err("there is no finished run to seal")
        .to_string();
    assert!(refusal.contains("grade a server first"), "{refusal}");
    Ok(())
}

/// The verification endpoint is the only one that can answer with no key
/// mounted at all, and it says so rather than pretending to have checked.
#[expect(
    clippy::panic_in_result_fn,
    reason = "the Book ch11 Result-returning test shape: assertions panic, plumbing propagates with ? (https://doc.rust-lang.org/book/ch11-01-writing-tests.html)"
)]
#[tokio::test]
async fn the_verification_endpoint_reports_the_missing_key()
-> Result<(), Box<dyn std::error::Error>> {
    use veredictum_console::verify_api::{VerifyScreen, fns};

    let scratch = assert_fs::TempDir::new()?;
    let mut state = state_over(scratch.path());
    let _owner = provide(&state);
    assert_eq!(
        fns::fetch_verification(None)
            .await
            .map_err(|e| e.to_string())?,
        VerifyScreen::NoKey
    );

    state.verify_key =
        Some(engine_gate::repo_root().join("artifacts/corpus/keys/cnf-signing.pub.asc"));
    let _owner = provide(&state);
    assert_eq!(
        fns::fetch_verification(None)
            .await
            .map_err(|e| e.to_string())?,
        VerifyScreen::Idle
    );
    // A bundle id is user input on a public endpoint: an unminted one is
    // refused before it can reach a path join.
    let VerifyScreen::Refused { reason } =
        fns::fetch_verification(Some(String::from("../../../etc/pas")))
            .await
            .map_err(|e| e.to_string())?
    else {
        panic!("a traversal never resolves to a bundle");
    };
    assert_eq!(reason, "not a bundle this console unpacked");
    Ok(())
}

/// The bench endpoints over an output root that carries no records answer
/// with the empty listing and the idle comparison.
#[expect(
    clippy::panic_in_result_fn,
    reason = "the Book ch11 Result-returning test shape: assertions panic, plumbing propagates with ? (https://doc.rust-lang.org/book/ch11-01-writing-tests.html)"
)]
#[tokio::test]
async fn the_bench_endpoints_answer_over_an_empty_output_root()
-> Result<(), Box<dyn std::error::Error>> {
    use veredictum_console::bench_api::{BenchScreen, CompareScreen, fns, read};

    let scratch = assert_fs::TempDir::new()?;
    let state = state_over(scratch.path());
    let _owner = provide(&state);

    assert_eq!(
        fns::fetch_bench_screen(None)
            .await
            .map_err(|e| e.to_string())?,
        read::screen(&state, None)
    );
    let BenchScreen::Listing(listing) = fns::fetch_bench_screen(None)
        .await
        .map_err(|e| e.to_string())?
    else {
        panic!("an output root with no records renders the listing");
    };
    assert!(listing.records.is_empty(), "{listing:?}");

    let BenchScreen::Unknown { reason } =
        fns::fetch_bench_screen(Some(String::from("no-such-record")))
            .await
            .map_err(|e| e.to_string())?
    else {
        panic!("an address naming nothing resolves to the unknown state");
    };
    assert!(!reason.is_empty());

    assert_eq!(
        fns::fetch_bench_comparison(None)
            .await
            .map_err(|e| e.to_string())?,
        CompareScreen::Idle
    );
    assert_eq!(
        fns::fetch_bench_comparison(Some(String::new()))
            .await
            .map_err(|e| e.to_string())?,
        CompareScreen::Idle,
        "an empty selection is idle, the same as no selection at all"
    );
    Ok(())
}

/// The run wizard's endpoints reached out of order refuse with the step that
/// has not run, and the idle slot refuses a cancel rather than pretending.
#[expect(
    clippy::panic_in_result_fn,
    reason = "the Book ch11 Result-returning test shape: assertions panic, plumbing propagates with ? (https://doc.rust-lang.org/book/ch11-01-writing-tests.html)"
)]
#[tokio::test]
async fn the_run_endpoints_refuse_an_out_of_order_wizard() -> Result<(), Box<dyn std::error::Error>>
{
    use veredictum_console::run_api::{RunScreen, fns};
    use veredictum_console::run_job::RunId;

    let scratch = assert_fs::TempDir::new()?;
    let state = state_over(scratch.path());
    let _owner = provide(&state);

    assert_eq!(
        fns::fetch_draft().await.map_err(|e| e.to_string())?,
        None,
        "no draft before Connect"
    );
    // "No run is in flight" is said only about a request that named no run.
    assert_eq!(
        fns::fetch_run(None).await.map_err(|e| e.to_string())?,
        RunScreen::NoRunNamed,
        "no job before Live"
    );
    // A run this instance never drove says so in its own words, and a run id
    // is untrusted input that can name nothing outside the output mount.
    let stranger: RunId = "3f2504e0-4f89-41d3-9a0c-0305e82c3301".parse()?;
    assert_eq!(
        fns::fetch_run(Some(stranger))
            .await
            .map_err(|e| e.to_string())?,
        RunScreen::Unknown(stranger),
        "an unknown run is not an idle console"
    );

    let scope = fns::save_scope(None, None, false)
        .await
        .expect_err("the scope step needs a connection draft")
        .to_string();
    assert!(scope.contains("no connection draft"), "{scope}");

    let cancel = fns::cancel_run(stranger)
        .await
        .expect_err("an idle slot has nothing to cancel")
        .to_string();
    assert!(cancel.contains("no run is in flight"), "{cancel}");

    // The tier selection is a closed vocabulary on a public endpoint: an
    // empty one reaches the composer's own refusal.
    let empty = fns::compose_claim(None)
        .await
        .expect_err("an empty tier selection composes no claim")
        .to_string();
    assert!(!empty.is_empty(), "{empty}");

    // A statement path is user input: only a statement.json under the party
    // tree loads at all.
    let outside = fns::fetch_statement_body(String::from("../../etc/passwd"))
        .await
        .expect_err("a path outside the party tree never loads")
        .to_string();
    assert!(!outside.is_empty(), "{outside}");
    Ok(())
}

/// The reads the Scope screen renders come off the committed party tree and
/// the committed catalogue, and the tier rows cover the four tiers the
/// verdict machinery answers for.
#[expect(
    clippy::panic_in_result_fn,
    reason = "the Book ch11 Result-returning test shape: assertions panic, plumbing propagates with ? (https://doc.rust-lang.org/book/ch11-01-writing-tests.html)"
)]
#[tokio::test]
async fn the_scope_reads_come_off_the_committed_trees() -> Result<(), Box<dyn std::error::Error>> {
    use veredictum_console::run_api::{ScopeTier, fns};

    let scratch = assert_fs::TempDir::new()?;
    let state = state_over(scratch.path());
    let _owner = provide(&state);

    let statements = fns::fetch_statements().await.map_err(|e| e.to_string())?;
    assert!(
        !statements.is_empty(),
        "the party tree carries committed statements"
    );
    let first = statements.first().ok_or("one statement row")?;
    let body = fns::fetch_statement_body(first.path.clone())
        .await
        .map_err(|e| e.to_string())?;
    assert!(
        serde_json::from_str::<veredictum::party::Statement>(&body).is_ok(),
        "a committed statement parses through the published lib's own model"
    );

    let tiers = fns::fetch_tier_counts().await.map_err(|e| e.to_string())?;
    let named: Vec<ScopeTier> = tiers.iter().map(|row| row.tier).collect();
    assert_eq!(
        named,
        vec![
            ScopeTier::Core,
            ScopeTier::Standard,
            ScopeTier::Options,
            ScopeTier::SecBasic
        ],
        "the four tiers the verdict machinery answers for, in rung order"
    );
    assert!(
        tiers.iter().any(|row| row.cases > 0),
        "a tier gates catalogue cases: {tiers:?}"
    );

    let preview = fns::fetch_scope_preview(String::from("I_EHR_SERVICE.create_ehr"))
        .await
        .map_err(|e| e.to_string())?;
    let everything = fns::fetch_scope_preview(String::new())
        .await
        .map_err(|e| e.to_string())?;
    assert!(
        preview.total > 0 && preview.total < everything.total,
        "a filter narrows the selection: {preview:?} of {everything:?}"
    );
    Ok(())
}

/// The connect step stores what it probed, and the probe's own answer is the
/// server's verbatim words — never a judgement.
///
/// Every authentication mode is driven, because each one puts a different set
/// of credential variables into the draft, and a credential that reached a
/// client-readable field would be the console's worst defect.
#[expect(
    clippy::panic_in_result_fn,
    reason = "the Book ch11 Result-returning test shape: assertions panic, plumbing propagates with ? (https://doc.rust-lang.org/book/ch11-01-writing-tests.html)"
)]
#[tokio::test]
async fn the_connect_step_records_the_probe_and_keeps_the_secrets()
-> Result<(), Box<dyn std::error::Error>> {
    use veredictum_console::run_api::{AuthChoice, ProbeAnswer, fns};

    let scratch = assert_fs::TempDir::new()?;
    let state = state_over(scratch.path());
    let _owner = provide(&state);

    let port = engine_gate::fixture_sut()?;
    let base_url = format!("http://127.0.0.1:{port}");
    for (auth, user, password, token) in [
        (AuthChoice::None, "", "", ""),
        (AuthChoice::Basic, "clinical", "s3cret", ""),
        (AuthChoice::Bearer, "", "", "t0ken"),
    ] {
        let answer = fns::probe_and_save(
            base_url.clone(),
            String::from("fixture-sut"),
            String::from("0.0.0-gate"),
            auth,
            user.to_owned(),
            password.to_owned(),
            token.to_owned(),
        )
        .await
        .map_err(|e| e.to_string())?;
        // The fixture answers every request 500, so the probe answered and
        // the answer is not ok — an unreachable server would be the other
        // variant, and neither is an error.
        assert_eq!(
            probe_status(&answer).as_deref(),
            Some("HTTP 500 Internal Server Error"),
            "the probe reports the server's own status line, verbatim"
        );

        let draft = fns::fetch_draft()
            .await
            .map_err(|e| e.to_string())?
            .ok_or("the connect step stored a draft")?;
        assert_eq!(draft.base_url, base_url);
        assert_eq!(draft.sut_name, "fixture-sut");
        assert_eq!(draft.auth, auth.token());
        assert!(!draft.probed_ok, "a 500 is not a passing probe");
        // The client-safe view carries no credential field at all: the
        // secrets live in the server-side draft and the spawned run's
        // environment, nowhere else.
        let serialized = serde_json::to_string(&draft)?;
        for secret in ["s3cret", "t0ken"] {
            assert!(
                !serialized.contains(secret),
                "a credential reached the client-safe draft view: {serialized}"
            );
        }
    }

    // An address nothing listens on is UNREACHABLE, which is an answer too.
    let unreachable = fns::probe_and_save(
        String::from("http://127.0.0.1:1"),
        String::from("nothing-there"),
        String::from("0"),
        AuthChoice::None,
        String::new(),
        String::new(),
        String::new(),
    )
    .await
    .map_err(|e| e.to_string())?;
    assert!(
        matches!(unreachable, ProbeAnswer::Unreachable { .. }),
        "{unreachable:?}"
    );
    Ok(())
}

/// The download route with nothing prepared answers 404 with the reason,
/// rather than an empty archive a reader would take for a record.
#[expect(
    clippy::panic_in_result_fn,
    reason = "the Book ch11 Result-returning test shape: assertions panic, plumbing propagates with ? (https://doc.rust-lang.org/book/ch11-01-writing-tests.html)"
)]
#[tokio::test]
async fn the_download_route_refuses_when_nothing_is_prepared()
-> Result<(), Box<dyn std::error::Error>> {
    let scratch = assert_fs::TempDir::new()?;
    let state = state_over(scratch.path());
    let response = veredictum_console::export_api::route::record_zip(axum::Extension(state)).await;
    assert_eq!(response.status(), axum::http::StatusCode::NOT_FOUND);
    Ok(())
}

/// The bench upload route is a public endpoint taking an anonymous document:
/// what it refuses matters more than what it accepts.
#[expect(
    clippy::panic_in_result_fn,
    reason = "the Book ch11 Result-returning test shape: assertions panic, plumbing propagates with ? (https://doc.rust-lang.org/book/ch11-01-writing-tests.html)"
)]
#[tokio::test]
async fn the_bench_upload_route_refuses_a_document_that_is_not_a_result()
-> Result<(), Box<dyn std::error::Error>> {
    use axum::extract::FromRequest as _;

    let scratch = assert_fs::TempDir::new()?;
    let state = state_over(scratch.path());
    let boundary = "veredictumbench";
    let mut wire = Vec::new();
    wire.extend_from_slice(
        format!(
            "--{boundary}\r\nContent-Disposition: form-data; name=\"result\"; filename=\"r.json\"\r\n\r\n"
        )
        .as_bytes(),
    );
    wire.extend_from_slice(b"{\"not\":\"a bench result\"}");
    wire.extend_from_slice(format!("\r\n--{boundary}--\r\n").as_bytes());
    let request = axum::http::Request::builder()
        .method(axum::http::Method::POST)
        .uri("/benchmarks/upload")
        .header(
            axum::http::header::CONTENT_TYPE,
            format!("multipart/form-data; boundary={boundary}"),
        )
        .body(axum::body::Body::from(wire))?;
    let form = axum::extract::Multipart::from_request(request, &())
        .await
        .map_err(|e| e.to_string())?;
    let response = veredictum_console::bench_api::route::upload(axum::Extension(state), form).await;
    let location = response
        .headers()
        .get(axum::http::header::LOCATION)
        .and_then(|value| value.to_str().ok())
        .unwrap_or_default();
    assert!(
        location.contains("refused=") || response.status().is_client_error(),
        "status {} location {location}",
        response.status()
    );
    Ok(())
}

/// The scope step over a real connection draft: a committed statement is
/// accepted and summarized, an oversized one and a non-statement are refused
/// with the reason, and a tier selection composes a claim the published lib
/// parses back.
///
/// The composed claim is the console's only document-AUTHORING path, so what
/// it emits is held to the lib's own model rather than to a string compare.
#[expect(
    clippy::panic_in_result_fn,
    reason = "the Book ch11 Result-returning test shape: assertions panic, plumbing propagates with ? (https://doc.rust-lang.org/book/ch11-01-writing-tests.html)"
)]
#[tokio::test]
async fn the_scope_step_accepts_a_claim_and_refuses_a_non_claim()
-> Result<(), Box<dyn std::error::Error>> {
    use veredictum_console::run_api::{AuthChoice, ScopeTier, fns};

    let scratch = assert_fs::TempDir::new()?;
    let state = state_over(scratch.path());
    let _owner = provide(&state);

    // The Connect step first: every later step reads the draft it writes.
    let port = engine_gate::fixture_sut()?;
    let probed = fns::probe_and_save(
        format!("http://127.0.0.1:{port}"),
        String::from("scope-sut"),
        String::from("1.2.3"),
        AuthChoice::None,
        String::new(),
        String::new(),
        String::new(),
    )
    .await
    .map_err(|e| e.to_string())?;
    assert!(
        probe_status(&probed).is_some(),
        "the fixture answered, so the draft carries a probe result"
    );

    // A committed statement: accepted, and summarized from its own fields.
    let statements = fns::fetch_statements().await.map_err(|e| e.to_string())?;
    let first = statements.first().ok_or("one committed statement")?;
    let body = fns::fetch_statement_body(first.path.clone())
        .await
        .map_err(|e| e.to_string())?;
    let summary = fns::save_scope(Some(body.clone()), None, true)
        .await
        .map_err(|e| e.to_string())?
        .ok_or("a pasted statement yields a claim summary")?;
    let declared: veredictum::party::Statement = serde_json::from_str(&body)?;
    assert_eq!(
        summary.product,
        format!("{} {}", declared.product.name, declared.product.version),
        "the summary states the statement's own product identity"
    );
    assert!(!summary.profiles.is_empty(), "{summary:?}");

    let draft = fns::fetch_draft()
        .await
        .map_err(|e| e.to_string())?
        .ok_or("the draft survives the scope step")?;
    assert!(
        draft.statement.is_some(),
        "the accepted claim is on the draft the run will carry"
    );

    // A document that is not JSON at all, and one that is JSON but not a
    // statement: each refused with what is wrong, never silently dropped.
    let not_json = fns::save_scope(Some(String::from("{ not json")), None, false)
        .await
        .expect_err("a body that is not JSON is not a claim")
        .to_string();
    assert!(not_json.contains("not JSON"), "{not_json}");

    let not_a_statement = fns::save_scope(Some(String::from(r#"{"hello":"world"}"#)), None, false)
        .await
        .expect_err("a JSON document that is not a statement is not a claim")
        .to_string();
    assert!(!not_a_statement.is_empty(), "{not_a_statement}");

    // The cap is checked before the parse, so an abusive body never reaches
    // the schema validator at all.
    let oversized = fns::save_scope(Some("0".repeat(4 * 1024 * 1024)), None, false)
        .await
        .expect_err("a body past the cap is refused")
        .to_string();
    assert!(oversized.contains("the cap is"), "{oversized}");

    // A composed claim: the console's own document, held to the lib's model.
    let composed = fns::compose_claim(Some(vec![ScopeTier::Core, ScopeTier::Core]))
        .await
        .map_err(|e| e.to_string())?;
    let parsed: veredictum::party::Statement = serde_json::from_str(&composed)?;
    assert_eq!(parsed.product.name, "scope-sut");
    assert_eq!(parsed.product.version, "1.2.3");
    assert!(
        !parsed.schedule_release.is_empty(),
        "a composed claim names the schedule release it targets"
    );
    // The duplicate tier collapsed rather than claiming CORE twice.
    let composed_once = fns::compose_claim(Some(vec![ScopeTier::Core]))
        .await
        .map_err(|e| e.to_string())?;
    assert_eq!(composed, composed_once);

    // Every tier the Scope screen can offer composes, and each carries its own
    // token and its own control id — a closed vocabulary with no default.
    let mut tokens: Vec<&str> = ScopeTier::ALL.iter().map(|t| t.token()).collect();
    let mut controls: Vec<&str> = ScopeTier::ALL.iter().map(|t| t.control_id()).collect();
    tokens.sort_unstable();
    controls.sort_unstable();
    tokens.dedup();
    controls.dedup();
    assert_eq!(tokens.len(), ScopeTier::ALL.len());
    assert_eq!(controls.len(), ScopeTier::ALL.len());
    let every = fns::compose_claim(Some(ScopeTier::ALL.to_vec()))
        .await
        .map_err(|e| e.to_string())?;
    let parsed: veredictum::party::Statement = serde_json::from_str(&every)?;
    assert_eq!(
        parsed.claims.profiles.len(),
        ScopeTier::ALL.len(),
        "every offered tier reaches the composed claim: {parsed:?}"
    );
    assert!(
        !parsed.claims.capabilities.is_empty(),
        "a claimed tier brings the capabilities the matrix puts in it"
    );
    Ok(())
}
