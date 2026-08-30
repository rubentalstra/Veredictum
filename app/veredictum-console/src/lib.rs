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

// Doctests are copy-paste templates: they must use `?`, never unwrap
// (C-QUESTION-MARK, https://rust-lang.github.io/api-guidelines/documentation.html#c-question-mark).
#![doc(test(attr(deny(warnings))))]
// Every `#[component]` in this crate expands `#[derive(TypedBuilder)]` on its
// generated props struct, and that derive emits an inherent `builder()` whose
// name matches a trait method already in scope — so `same_name_method` fires
// once per component with the macro invocation as its only span, never on
// hand-written code. Crate-level because there is no smaller item to scope it
// to: the finding does not exist in this crate's source
// (https://docs.rs/leptos/0.8/leptos/attr.component.html).
#![allow(
    clippy::same_name_method,
    reason = "emitted only by leptos's TypedBuilder derive inside #[component]; no hand-written method in this crate shadows a trait method"
)]

// `hydrate` (wasm client) and `ssr` (server) are mutually exclusive build
// modes — cargo-leptos always builds them separately. Guarded per the Cargo
// book's prescription for genuinely exclusive features
// (https://doc.rust-lang.org/cargo/reference/features.html#mutually-exclusive-features).
#[cfg(all(feature = "hydrate", feature = "ssr"))]
compile_error!("features \"hydrate\" and \"ssr\" cannot be enabled at the same time");

/// The exact engine version this console is built against.
///
/// One fact, in lock-step with the manifest's crates.io pin
/// (`veredictum = "=…"`; the engine module's unit test holds the two
/// together). The shell footer shows it, and the ssr-side engine seam
/// refuses a binary reporting anything else.
pub const ENGINE_PIN: &str = "0.1.1";

pub mod app;
pub mod bench_api;
pub mod catalogue_api;
pub mod components;
#[cfg(feature = "ssr")]
pub mod engine;
pub mod export;
pub mod export_api;
pub mod pages;
pub mod record_api;
pub mod redirect;
pub mod run_api;
pub mod run_job;
#[cfg(feature = "ssr")]
pub mod state;
pub mod theme;
pub mod verify_api;

/// The browser entry point: installs the panic hook so a client-side panic
/// reports a real stack trace, then hydrates the server-rendered body.
#[cfg(feature = "hydrate")]
#[wasm_bindgen::prelude::wasm_bindgen]
pub fn hydrate() {
    console_error_panic_hook::set_once();
    leptos::mount::hydrate_body(app::App);
}
