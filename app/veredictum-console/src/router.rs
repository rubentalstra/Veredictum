// SPDX-FileCopyrightText: Veredictum contributors
// SPDX-License-Identifier: Apache-2.0

//! The console's HTTP surface: every route the server answers on, the body
//! cap each one carries, and the layer that gives a malformed call a caller's
//! status.
//!
//! It lives in the lib rather than in the binary so the whole router can be
//! bound to a listener and driven over real HTTP by a test, which is the only
//! place a layer's actual reach is observable — the thin-main-over-testable-lib
//! shape the Book recommends
//! (<https://doc.rust-lang.org/book/ch12-03-improving-error-handling-and-testing.html>).

use axum::extract::DefaultBodyLimit;
use leptos::prelude::LeptosOptions;
use leptos_axum::{LeptosRoutes as _, generate_route_list};

use crate::app::{App, shell};
use crate::state::ConsoleState;

/// The framing a body carries on top of the payload its handler judges.
///
/// A multipart post spends its envelope on a boundary per part, the part
/// headers and the line breaks between them, and none of that reaches the
/// reader's own size check. One mebibyte is far above what the entries an
/// upload may carry can spend on framing.
const FRAMING_SLACK_BYTES: u64 = 1024 * 1024;

/// How far URL encoding can inflate the payload a server function judges.
///
/// A `#[server]` call posts its arguments URL-encoded, and percent-encoding
/// spends three bytes on every byte outside the unreserved set, so a JSON
/// statement arrives at up to three times its own length. Four is that worst
/// case with room above it.
const SERVER_FN_INFLATION: u64 = 4;

/// A cap the pages enforce that this host's `usize` cannot hold.
///
/// The server refuses to start on it: the one value a body cap must never
/// fall back to is "unlimited".
#[derive(Debug, thiserror::Error)]
#[error("the {route} body cap of {bytes} bytes does not fit this host's usize")]
pub struct BodyCapTooLarge {
    /// Which reach the cap belongs to.
    pub route: &'static str,
    /// The cap, in bytes.
    pub bytes: u64,
    /// The conversion's own refusal.
    #[source]
    pub source: std::num::TryFromIntError,
}

/// The transport body caps the router installs, one per reach.
///
/// Each is derived from the number the endpoint's own code enforces and sits
/// above it, so every payload an endpoint accepts reaches the code that judges
/// it, and a size refusal within the transport cap is that endpoint's own
/// sentence.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BodyCaps {
    /// The verify upload route's cap.
    pub verify_upload: usize,
    /// The bench upload route's cap.
    pub bench_upload: usize,
    /// The cap every other route carries, the server functions among them.
    pub server_fn: usize,
}

impl BodyCaps {
    /// Derives the three caps from the numbers the pages enforce.
    ///
    /// # Errors
    /// [`BodyCapTooLarge`] when a cap does not fit this host's `usize`.
    pub fn derived() -> Result<Self, BodyCapTooLarge> {
        Ok(Self {
            verify_upload: fits(
                "verify upload",
                crate::verify_api::unpack::MAX_UPLOAD_BYTES + FRAMING_SLACK_BYTES,
            )?,
            bench_upload: fits(
                "bench upload",
                crate::bench_api::upload::MAX_BATCH_BYTES + FRAMING_SLACK_BYTES,
            )?,
            server_fn: fits(
                "server function",
                SERVER_FN_INFLATION * crate::run_api::read::STATEMENT_CAP_BYTES,
            )?,
        })
    }
}

/// The one u64 to `usize` conversion, named so a refusal says which cap failed.
fn fits(route: &'static str, bytes: u64) -> Result<usize, BodyCapTooLarge> {
    usize::try_from(bytes).map_err(|source| BodyCapTooLarge {
        route,
        bytes,
        source,
    })
}

/// Builds the console's router: the server-owned routes, the Leptos route
/// tree, and the body cap each reach carries.
///
/// The arrangement is PER-ROUTE caps rather than one router-wide maximum.
/// One maximum is the loosest of the options — it hands every endpoint the
/// largest number any endpoint needs — and the property this owes a caller is
/// that the limit they meet is the one the code they called enforces.
///
/// # Errors
/// [`BodyCapTooLarge`] when a cap does not fit this host's `usize`. The caller
/// stops startup on it.
pub fn router(
    state: &ConsoleState,
    leptos_options: LeptosOptions,
) -> Result<axum::Router, BodyCapTooLarge> {
    let caps = BodyCaps::derived()?;
    let routes = generate_route_list(App);

    Ok(axum::Router::new()
        // Outside the Leptos route tree so it answers even when the WASM
        // bundle or the app shell is broken: it claims only "the server
        // accepts connections".
        .route("/healthz", axum::routing::get(|| async { "ok" }))
        // Server-owned because it answers with bytes rather than a view: the
        // sealed record, streamed as an archive.
        .route(
            crate::export_api::DOWNLOAD_PATH,
            axum::routing::get(crate::export_api::route::record_zip),
        )
        // The red rows' exchanges, carved by the engine on request: the
        // triage input, downloaded rather than rendered.
        .route(
            crate::evidence_api::DOWNLOAD_PATH,
            axum::routing::get(crate::evidence_api::route::evidence_json),
        )
        // A plain multipart form post, which uploads with zero JavaScript and
        // before WASM loads. The cap sits above the largest BODY the reader
        // accepts, so its size refusal is the reader's own sentence.
        .route(
            crate::verify_api::UPLOAD_PATH,
            axum::routing::post(crate::verify_api::route::upload)
                .layer(DefaultBodyLimit::max(caps.verify_upload)),
        )
        // The bench batch upload (#166): read and listed, never stored. Its
        // cap is the batch cap the reader enforces, plus the framing.
        .route(
            crate::bench_api::UPLOAD_PATH,
            axum::routing::post(crate::bench_api::route::upload)
                .layer(DefaultBodyLimit::max(caps.bench_upload)),
        )
        // These four handlers are outside the reactive route tree, so they
        // take the state as an extension rather than through `expect_context`.
        .layer(axum::Extension(state.clone()))
        .leptos_routes_with_context(
            &leptos_options,
            routes,
            {
                let state = state.clone();
                move || leptos::prelude::provide_context(state.clone())
            },
            {
                let leptos_options = leptos_options.clone();
                move || shell(leptos_options.clone())
            },
        )
        .fallback(leptos_axum::file_and_error_handler(shell))
        // AFTER the server functions and the fallback, which is the whole
        // point: "the middleware is only applied to existing routes"
        // (<https://docs.rs/axum/latest/axum/routing/struct.Router.html#method.layer>),
        // so this is the first placement from which a `#[server]` call's body
        // limit is anything but axum's 2 MiB default (#496). The two upload
        // routes keep their own, larger caps: the limit is a request extension
        // and the layer closest to the route writes it last
        // (<https://docs.rs/axum/latest/axum/extract/struct.DefaultBodyLimit.html>).
        .layer(DefaultBodyLimit::max(caps.server_fn))
        // Also after the server functions, for the same reason: a layer placed
        // higher up would cover the five routes above and none of the
        // endpoints whose refusals it exists to rewrite (#484).
        .layer(axum::middleware::from_fn(caller_faults_are_4xx))
        .with_state(leptos_options))
}

/// Gives a malformed call a caller's status and a caller's sentence (#484).
///
/// `server_fn` answers every error it raises with 500, argument decoding
/// included, and offers no hook to change it
/// (<https://docs.rs/leptos/latest/leptos/server_fn/response/trait.Res.html>).
/// So the encoded error is read back here: a decoding failure becomes 400 with
/// this crate's own wording, and the server's own errors pass through with
/// their status and body untouched.
async fn caller_faults_are_4xx(
    request: axum::extract::Request,
    next: axum::middleware::Next,
) -> axum::response::Response {
    use axum::response::IntoResponse as _;

    let response = next.run(request).await;
    if response.status() != axum::http::StatusCode::INTERNAL_SERVER_ERROR {
        return response;
    }

    let (parts, body) = response.into_parts();
    // NOTE: no openEHR spec governs this — our own design; a body too large
    // to read back is not an encoded refusal, so it keeps the framework's own
    // status and says nothing more.
    let Ok(bytes) = axum::body::to_bytes(body, MAX_REFUSAL_BYTES).await else {
        return axum::http::StatusCode::INTERNAL_SERVER_ERROR.into_response();
    };

    let Ok(encoded) = std::str::from_utf8(&bytes) else {
        return (parts, bytes).into_response();
    };
    match crate::arg_refusal::caller_fault(encoded) {
        Some(sentence) => (axum::http::StatusCode::BAD_REQUEST, sentence).into_response(),
        None => (parts, bytes).into_response(),
    }
}

/// The largest error body the refusal layer reads back before giving up.
///
/// A server-function error is one encoded sentence, so anything past this is
/// not one and is answered as the framework wrote it.
const MAX_REFUSAL_BYTES: usize = 64 * 1024;

#[cfg(test)]
#[expect(
    clippy::panic_in_result_fn,
    reason = "the Book ch11 Result-returning test shape: assertions panic, plumbing propagates with ? (https://doc.rust-lang.org/book/ch11-01-writing-tests.html)"
)]
mod tests {
    use super::{BodyCapTooLarge, BodyCaps, FRAMING_SLACK_BYTES, SERVER_FN_INFLATION};

    #[test]
    fn every_cap_is_derived_from_the_number_its_own_endpoint_enforces()
    -> Result<(), BodyCapTooLarge> {
        let caps = BodyCaps::derived()?;
        assert_eq!(
            u64::try_from(caps.verify_upload),
            Ok(crate::verify_api::unpack::MAX_UPLOAD_BYTES + FRAMING_SLACK_BYTES),
            "the verify cap is the largest body its reader accepts, plus the framing"
        );
        assert_eq!(
            u64::try_from(caps.bench_upload),
            Ok(crate::bench_api::upload::MAX_BATCH_BYTES + FRAMING_SLACK_BYTES),
            "the bench cap is the batch total its reader judges, plus the framing"
        );
        assert_eq!(
            u64::try_from(caps.server_fn),
            Ok(SERVER_FN_INFLATION * crate::run_api::read::STATEMENT_CAP_BYTES),
            "the server-function cap is the statement cap with room for URL encoding"
        );
        Ok(())
    }

    #[test]
    fn the_caps_are_the_numbers_this_host_installs() -> Result<(), BodyCapTooLarge> {
        let caps = BodyCaps::derived()?;
        assert_eq!(caps.verify_upload, 17_825_792, "17 MiB");
        assert_eq!(caps.bench_upload, 34_603_008, "33 MiB");
        assert_eq!(caps.server_fn, 4_194_304, "4 MiB");
        Ok(())
    }

    #[test]
    fn every_cap_sits_above_the_payload_its_endpoint_accepts() -> Result<(), BodyCapTooLarge> {
        let caps = BodyCaps::derived()?;
        assert!(
            u64::try_from(caps.verify_upload)
                .is_ok_and(|cap| cap > crate::verify_api::unpack::MAX_UPLOAD_BYTES),
            "an upload at the page's own cap must reach the reader"
        );
        assert!(
            u64::try_from(caps.bench_upload)
                .is_ok_and(|cap| cap > crate::bench_api::upload::MAX_BATCH_BYTES),
            "a batch at the page's own cap must reach the reader"
        );
        assert!(
            u64::try_from(caps.server_fn)
                .is_ok_and(|cap| cap > crate::run_api::read::STATEMENT_CAP_BYTES),
            "a statement at the page's own cap must reach the handler"
        );
        Ok(())
    }

    #[test]
    fn the_statement_cap_is_the_largest_input_any_server_function_judges() {
        let largest = crate::run_api::SERVER_FN_INPUT_CAPS.iter().copied().max();
        assert_eq!(
            largest,
            Some(crate::run_api::read::STATEMENT_CAP_BYTES),
            "one transport cap covers every `#[server]` fn and it is derived from the \
             statement cap alone; an endpoint now judging a larger input needs that \
             derivation re-decided, or its callers meet the generic transport refusal"
        );
    }

    #[test]
    fn a_cap_that_does_not_fit_this_host_carries_the_conversion_as_its_source() {
        let source = u8::try_from(300_u32).expect_err("300 does not fit a u8");
        let refusal = BodyCapTooLarge {
            route: "server function",
            bytes: u64::MAX,
            source,
        };
        let cause = std::error::Error::source(&refusal).expect("the conversion is the cause");
        assert!(
            cause.downcast_ref::<std::num::TryFromIntError>().is_some(),
            "the cause downcasts to the conversion's own error, not to a wrapper"
        );
    }
}
