// SPDX-FileCopyrightText: Veredictum contributors
// SPDX-License-Identifier: Apache-2.0

//! The console's integration suite: one binary, one module per topic.
//!
//! Cargo compiles and links every top-level `tests/*.rs` as its own crate
//! (<https://doc.rust-lang.org/cargo/reference/cargo-targets.html#integration-tests>),
//! so one binary saves the link waste while nextest still runs each test in
//! its own process.

#[cfg(feature = "ssr")]
mod e2e_console;
#[cfg(feature = "ssr")]
mod engine_gate;
#[cfg(feature = "ssr")]
mod export_gate;
#[cfg(feature = "ssr")]
mod read_surfaces;
#[cfg(feature = "ssr")]
mod run_live;
#[cfg(feature = "ssr")]
mod run_scope;
