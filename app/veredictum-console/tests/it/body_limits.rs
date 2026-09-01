// SPDX-FileCopyrightText: Veredictum contributors
// SPDX-License-Identifier: Apache-2.0

//! What each endpoint's body limit actually is, over real HTTP (#496).
//!
//! A layer's reach is invisible to a direct call on the function it wraps, so
//! every request here travels a socket into the router the server binary
//! builds. The gate the earlier suite could not hold: a payload the console's
//! own code accepts must reach that code.

use std::sync::{Arc, Mutex};

use leptos::server_fn::ServerFn;
use veredictum_console::run_api::fns::SaveScope;

/// A bound console, and the scratch tree its state points at.
struct Bound {
    /// The origin every request in a test is sent to.
    origin: String,
    /// Kept alive for the test's length: dropping it removes the tree.
    _scratch: assert_fs::TempDir,
}

/// Binds the router the server binary builds on an ephemeral loopback port.
async fn bind() -> Result<Bound, Box<dyn std::error::Error>> {
    let scratch = assert_fs::TempDir::new()?;
    let state = veredictum_console::state::ConsoleState {
        root: crate::engine_gate::repo_root().join("artifacts"),
        specs: crate::engine_gate::repo_root().join("specs/openehr"),
        out: scratch.path().to_path_buf(),
        sign_key: None,
        verify_key: None,
        // The body caps are read before any screen reads the catalogue, so a
        // stated absence keeps this gate off the validation pass entirely.
        catalogue: Arc::new(Err(String::from(
            "no catalogue: this gate drives body caps",
        ))),
        draft: Arc::new(Mutex::new(veredictum_console::run_api::Drafts::new())),
        jobs: veredictum_console::run_job::JobSlot::default(),
        client_ip_header: None,
        posture: veredictum_console::posture::Posture::Local,
        rates: veredictum_console::rate_limit::RateLimiter::default(),
        capture: false,
    };
    let options = leptos::prelude::LeptosOptions::builder()
        .output_name("veredictum-console")
        .build();
    let app = veredictum_console::router::router(&state, options)?;

    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await?;
    let addr = listener.local_addr()?;
    drop(tokio::spawn(async move {
        drop(
            axum::serve(
                listener,
                app.into_make_service_with_connect_info::<std::net::SocketAddr>(),
            )
            .await,
        );
    }));
    Ok(Bound {
        origin: format!("http://{addr}"),
        _scratch: scratch,
    })
}

/// A client that reports a redirect instead of following it.
fn client() -> Result<reqwest::Client, reqwest::Error> {
    reqwest::Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .build()
}

/// Posts one URL-encoded `save_scope` call carrying a statement of `bytes`.
///
/// The value is spelled from an unreserved character, so the body on the wire
/// is the statement's own length plus the rest of the form: the band a test
/// aims at is the band the request lands in.
async fn save_scope_with_a_statement_of(
    bound: &Bound,
    bytes: usize,
) -> Result<(reqwest::StatusCode, String), Box<dyn std::error::Error>> {
    let form = format!(
        "statement_json={}&filter=&record_exchanges=false\
         &postures[system_id]=&postures[dump_location]=\
         &postures[signing]=Undeclared&postures[digest_encoding]=Base64\
         &postures[digest_prefix]=&postures[pgp_public_key]=\
         &postures[spec_profile]=Undeclared",
        "0".repeat(bytes)
    );
    let response = client()?
        .post(format!("{}{}", bound.origin, <SaveScope as ServerFn>::PATH))
        .header("accept", "application/json")
        .header("content-type", "application/x-www-form-urlencoded")
        .body(form)
        .send()
        .await?;
    let status = response.status();
    Ok((status, response.text().await?))
}

/// The statement cap `save_scope` enforces, as a `usize` for building bodies.
fn statement_cap() -> Result<usize, std::num::TryFromIntError> {
    usize::try_from(veredictum_console::run_api::read::STATEMENT_CAP_BYTES)
}

#[expect(
    clippy::panic_in_result_fn,
    reason = "the Book ch11 Result-returning test shape: assertions panic, plumbing propagates with ? (https://doc.rust-lang.org/book/ch11-01-writing-tests.html)"
)]
#[tokio::test(flavor = "multi_thread")]
async fn a_statement_past_its_cap_answers_the_endpoints_own_sentence()
-> Result<(), Box<dyn std::error::Error>> {
    let bound = bind().await?;
    let size = statement_cap()? + 1024;
    let (status, body) = save_scope_with_a_statement_of(&bound, size).await?;

    assert_ne!(
        status,
        reqwest::StatusCode::PAYLOAD_TOO_LARGE,
        "the transport answered instead of the handler: {body}"
    );
    assert!(
        body.contains(&format!("the statement is {size} bytes")),
        "the endpoint's own sentence did not come back ({status}): {body}"
    );
    Ok(())
}

#[expect(
    clippy::panic_in_result_fn,
    reason = "the Book ch11 Result-returning test shape: assertions panic, plumbing propagates with ? (https://doc.rust-lang.org/book/ch11-01-writing-tests.html)"
)]
#[tokio::test(flavor = "multi_thread")]
async fn a_body_in_the_band_axums_default_cut_off_reaches_the_handler()
-> Result<(), Box<dyn std::error::Error>> {
    let bound = bind().await?;
    // Past axum's own 2 MiB default and inside this route's cap: the band
    // that answered nothing the console wrote while the limit sat above the
    // server functions instead of over them.
    let size = 3 * 1024 * 1024;
    let (status, body) = save_scope_with_a_statement_of(&bound, size).await?;

    assert_ne!(
        status,
        reqwest::StatusCode::PAYLOAD_TOO_LARGE,
        "the transport answered instead of the handler: {body}"
    );
    assert!(
        body.contains(&format!("the statement is {size} bytes")),
        "the handler never saw the body ({status}): {body}"
    );
    Ok(())
}

#[expect(
    clippy::panic_in_result_fn,
    reason = "the Book ch11 Result-returning test shape: assertions panic, plumbing propagates with ? (https://doc.rust-lang.org/book/ch11-01-writing-tests.html)"
)]
#[tokio::test(flavor = "multi_thread")]
async fn a_body_past_the_transport_cap_is_refused_as_the_callers_fault()
-> Result<(), Box<dyn std::error::Error>> {
    let bound = bind().await?;
    let (status, body) = save_scope_with_a_statement_of(&bound, 5 * 1024 * 1024).await?;

    // The deliberate backstop. `server_fn` reads the body through axum's
    // limited reader and raises its `Deserialization` variant on the overrun
    // (<https://docs.rs/axum/latest/axum/struct.RequestExt.html#tymethod.into_limited_body>),
    // which the caller-fault layer answers as a caller's mistake (#484).
    assert_eq!(
        status,
        reqwest::StatusCode::BAD_REQUEST,
        "a body past the transport cap is the caller's mistake: {body}"
    );
    assert!(
        body.contains("This call could not be read"),
        "the backstop answered without a sentence: {body}"
    );
    assert!(
        !body.contains("the statement is"),
        "the handler was reached past the transport cap: {body}"
    );
    Ok(())
}

#[expect(
    clippy::panic_in_result_fn,
    reason = "the Book ch11 Result-returning test shape: assertions panic, plumbing propagates with ? (https://doc.rust-lang.org/book/ch11-01-writing-tests.html)"
)]
#[tokio::test(flavor = "multi_thread")]
async fn the_upload_route_keeps_its_own_cap_above_the_router_wide_one()
-> Result<(), Box<dyn std::error::Error>> {
    let bound = bind().await?;
    // Past the server functions' cap and inside the verify route's own, so
    // the answer says which layer won: the one closest to the route.
    let filler = "0".repeat(5 * 1024 * 1024);
    let body = format!(
        "--boundary\r\nContent-Disposition: form-data; name=\"bundle\"; \
         filename=\"record.zip\"\r\nContent-Type: application/zip\r\n\r\n{filler}\r\n\
         --boundary--\r\n"
    );
    let response = client()?
        .post(format!(
            "{}{}",
            bound.origin,
            veredictum_console::verify_api::UPLOAD_PATH
        ))
        .header("content-type", "multipart/form-data; boundary=boundary")
        .body(body)
        .send()
        .await?;

    let status = response.status();
    let location = response
        .headers()
        .get("location")
        .and_then(|value| value.to_str().ok())
        .unwrap_or_default()
        .to_owned();
    assert_eq!(
        status,
        reqwest::StatusCode::SEE_OTHER,
        "the upload never reached its reader: {location}"
    );
    // The reader's own verdict on the bytes, which it can only reach once the
    // whole body arrived. A transport refusal reads as the multipart field
    // ending early, and says so instead.
    let refused = format!(
        "refused={}",
        veredictum_console::redirect::percent_encode("not a readable zip archive")
    );
    assert!(
        location.contains(&refused),
        "the reader never saw the whole body: {location}"
    );
    Ok(())
}
