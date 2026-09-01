// SPDX-FileCopyrightText: Veredictum contributors
// SPDX-License-Identifier: Apache-2.0

//! The Veredictum web console: a Leptos SSR frontend over the published
//! instrument.
//!
//! One crate, two compilation targets (the cargo-leptos model): this rlib
//! feeds the `ssr` server binary, and the cdylib build under the `hydrate`
//! feature is the WASM client bundle. The console adds no judgement of its
//! own — runs execute through the pinned `veredictum` binary and reads parse
//! through the published lib (#54); the design record is #52.

// Doctests are copy-paste templates and must use `?`, never unwrap
// (https://rust-lang.github.io/api-guidelines/documentation.html#c-question-mark).
#![doc(test(attr(deny(warnings))))]
// The lint fires once per `#[component]`, spanned on the macro invocation, so
// there is no smaller item to scope the suppression to
// (https://docs.rs/leptos/0.8/leptos/attr.component.html).
#![allow(
    clippy::same_name_method,
    reason = "emitted only by leptos's TypedBuilder derive inside #[component]; no hand-written method in this crate shadows a trait method"
)]

// cargo-leptos always builds the wasm client and the server separately
// (https://doc.rust-lang.org/cargo/reference/features.html#mutually-exclusive-features).
#[cfg(all(feature = "hydrate", feature = "ssr"))]
compile_error!("features \"hydrate\" and \"ssr\" cannot be enabled at the same time");

/// The exact engine version this console is built against.
///
/// One fact, read out of the manifest's crates.io pin (`veredictum = "=…"`)
/// by `build.rs` and substituted here, so there is no second copy to drift.
/// The shell footer shows it, and the ssr-side engine seam refuses a binary
/// reporting anything else.
pub const ENGINE_PIN: &str = env!("VEREDICTUM_ENGINE_PIN");

pub mod app;
pub mod arg_refusal;
pub mod bench_api;
pub mod capture;
pub mod catalogue_api;
pub mod components;
#[cfg(feature = "ssr")]
pub mod engine;
pub mod evidence_api;
pub mod export;
pub mod export_api;
#[cfg(feature = "ssr")]
pub mod github;
pub mod pages;
#[cfg(feature = "ssr")]
pub mod posture;
#[cfg(feature = "ssr")]
pub mod rate_limit;
pub mod record_api;
pub mod redirect;
pub mod run_api;
pub mod run_job;
pub mod site_bundle;
#[cfg(feature = "ssr")]
pub mod state;
pub mod submit_api;
pub mod submitter;
#[cfg(feature = "ssr")]
pub mod target_safety;
pub mod theme;
pub mod verify_api;

/// The body cap the server installs on the upload routes: the larger of the
/// two upload caps, as this host's `usize`.
///
/// # Errors
/// When the cap does not fit `usize` — a host too small to hold the limits
/// the pages enforce. The server refuses to start on it, because the one
/// value a body cap must never fall back to is "unlimited".
#[cfg(feature = "ssr")]
pub fn upload_body_cap() -> Result<usize, std::num::TryFromIntError> {
    usize::try_from(verify_api::unpack::MAX_UPLOAD_BYTES.max(bench_api::upload::MAX_BATCH_BYTES))
}

/// The browser entry point: installs the panic hook so a client-side panic
/// reports a real stack trace, then hydrates the server-rendered body.
#[cfg(feature = "hydrate")]
#[wasm_bindgen::prelude::wasm_bindgen]
pub fn hydrate() {
    console_error_panic_hook::set_once();
    leptos::mount::hydrate_body(app::App);
}

#[cfg(all(test, feature = "ssr"))]
mod cap_tests {
    //! The router body cap: the value main installs is the caps' own maximum,
    //! and the conversion succeeds on every host this ships to.

    #[test]
    fn the_upload_body_cap_is_the_larger_of_the_two_upload_caps() {
        let cap = super::upload_body_cap();
        assert_eq!(
            cap,
            usize::try_from(
                super::verify_api::unpack::MAX_UPLOAD_BYTES
                    .max(super::bench_api::upload::MAX_BATCH_BYTES)
            ),
            "the installed cap is derived from the caps the pages enforce"
        );
        assert_eq!(cap, Ok(33_554_432), "32 MiB, the bench batch cap");
    }
}
