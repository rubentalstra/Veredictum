// SPDX-FileCopyrightText: Veredictum contributors
// SPDX-License-Identifier: Apache-2.0

//! Integration tests for `veredictum`, the CNF 2.0 conformance runner: the
//! committed catalogue's artifact gates and schema-drift guards, the
//! claim/coverage completeness gates, the vendored corpus packs, defect-fixture
//! rejection, the measured-performance driver, and the self-verification pack.
//!
//! One binary per crate, split into topic modules: Cargo compiles and links
//! every top-level `tests/*.rs` as its own crate
//! (<https://doc.rust-lang.org/cargo/reference/cargo-targets.html#integration-tests>),
//! so one binary saves the link waste while nextest still runs each test in
//! its own process.

#![expect(
    clippy::disallowed_types,
    reason = "an independently authored wire input catches codec bugs a typed-then-serialized value cannot, so fixtures and wire assertions are raw JSON"
)]

mod artifact_gates;
mod claim_completeness;
mod corpus_packs;
mod defect_rejection;
mod perf_driver;
mod pipeline_seams;
mod schema_drift;
mod verification_pack;
