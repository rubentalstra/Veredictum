// SPDX-FileCopyrightText: Veredictum contributors
// SPDX-License-Identifier: Apache-2.0

//! The universal-benchmark engine: comparative SPEED against any reachable
//! openEHR CDR, with no catalogue, no ixit and no artifact root.
//!
//! A bench run takes a base URL plus one credential and drives an EMBEDDED
//! pack: a seed phase that bulk-loads a fixed corpus through the public API
//! (closed-loop, reported as bulk-load throughput and labelled as such), then
//! measured phases that offer a seeded open-loop arrival schedule over a
//! closed operation vocabulary. Every latency is measured from the PLANNED
//! arrival instant, so coordinated omission cannot hide a stall. The measured
//! phases repeat, and the result carries every repetition plus the
//! cross-repetition median and inter-quartile range.
//!
//! Module map: [`pack`] the embedded packs, their pinned fixtures and the
//! operation vocabulary · [`client`] targeting and credentials · [`run`] the
//! preflight, the seed phase and the open-loop dispatcher · [`result`] the
//! emitted artifact · [`fingerprint`] the host environment record ·
//! [`compare`] the cross-file alignment and its median/IQR math ·
//! [`render`] the console and Markdown views.
//!
//! What this engine is NOT is stated in [`BOUNDARY_STATEMENT`], which every
//! artifact and every rendered view carries verbatim.

pub mod client;
pub mod compare;
pub mod fingerprint;
pub mod pack;
pub mod render;
pub mod result;
pub mod run;

use std::path::PathBuf;

use thiserror::Error;

/// What a bench result is, and what it is never. Carried verbatim in every
/// emitted artifact, on the `bench` console output, and in every rendered
/// comparison.
pub const BOUNDARY_STATEMENT: &str = "This is a benchmark record for comparative speed. It is not a conformance record, not a certificate, and not a performance-class rating; a bench result may motivate a class run, never substitute for one.";

/// The methodology every bench run follows, stated in the artifact so a
/// reader never has to infer it from the numbers.
pub const METHODOLOGY: &str = "Seed once, measure N times. Measured phases are open-loop: arrivals fire at their planned instants regardless of any other request's completion, and every latency is measured from the planned arrival instant, so coordinated omission cannot hide a stall. Seed phases are closed-loop by construction and are reported as bulk-load throughput only.";

/// A failure of the benchmark engine.
///
/// Each variant renders the diagnostic the command line reports, so a caller
/// that only needs to show the problem prints the error and stops. Individual
/// arrival faults inside a measured phase are NOT errors here: they are
/// counted by class in the result.
#[derive(Debug, Error)]
pub enum BenchError {
    /// The requested pack is not one this binary embeds.
    #[error("unknown bench pack {requested:?} (embedded: {known})")]
    UnknownPack {
        /// The token the caller asked for.
        requested: String,
        /// The embedded pack ids, comma-separated.
        known: String,
    },
    /// A token outside a closed vocabulary. Never a silent fallback: an
    /// unknown token in a conformance instrument manufactures a wrong row.
    #[error("unknown {vocabulary} token {token:?} (accepted: {accepted})")]
    UnknownToken {
        /// Which vocabulary rejected the token.
        vocabulary: &'static str,
        /// The token as written.
        token: String,
        /// The accepted tokens, comma-separated.
        accepted: String,
    },
    /// An embedded fixture's bytes do not hash to the pin the pack declares.
    #[error(
        "bench pack {pack}: fixture {fixture} is pinned at sha256 {expected} but the embedded bytes hash to {actual}"
    )]
    FixturePin {
        /// The pack carrying the fixture.
        pack: String,
        /// The fixture key.
        fixture: String,
        /// The declared pin.
        expected: String,
        /// What the embedded bytes actually hash to.
        actual: String,
    },
    /// `--auth basic` was selected without the user the header needs.
    #[error("--auth basic needs --user")]
    MissingUser,
    /// A credential environment variable is unset. Secrets never ride argv.
    #[error("credential environment variable {name} is unset: {source}")]
    Credential {
        /// The variable that was consulted.
        name: &'static str,
        /// The lookup failure.
        #[source]
        source: std::env::VarError,
    },
    /// The HTTP client could not be built.
    #[error("http client: {source}")]
    Client {
        /// The underlying failure.
        #[source]
        source: reqwest::Error,
    },
    /// A request never reached a response.
    #[error("{exchange}: transport: {source}")]
    Transport {
        /// The exchange that failed, named the way the preflight names it.
        exchange: String,
        /// The underlying failure.
        #[source]
        source: reqwest::Error,
    },
    /// The preflight refused the run. Nothing is measured after this.
    #[error("preflight refused the run at {exchange}: {detail}")]
    Preflight {
        /// The exchange that failed.
        exchange: String,
        /// What was wrong with it.
        detail: String,
    },
    /// A seed phase could not complete, so no measured phase may follow.
    #[error("seed phase {phase}: {detail}")]
    Seed {
        /// The phase name from the pack.
        phase: String,
        /// What went wrong.
        detail: String,
    },
    /// A measured phase could not be aggregated.
    #[error("measure phase {phase}: {detail}")]
    Measure {
        /// The phase name from the pack.
        phase: String,
        /// What went wrong.
        detail: String,
    },
    /// The repetition count is outside the engine's range.
    #[error("--repetitions must be at least 1 (got {0})")]
    Repetitions(u32),
    /// A histogram could not be created, recorded into, or encoded.
    #[error("histogram: {0}")]
    Histogram(String),
    /// A comparison was asked for with fewer than two result files.
    #[error("bench-compare needs at least two result files (got {0})")]
    TooFewResults(usize),
    /// A file could not be read.
    #[error("cannot read {path}: {source}")]
    Read {
        /// The file that could not be read.
        path: PathBuf,
        /// The underlying filesystem error.
        #[source]
        source: std::io::Error,
    },
    /// A document did not parse as a bench result.
    #[error("{path}: not a bench result: {message}")]
    Parse {
        /// The file that did not parse.
        path: PathBuf,
        /// The parser's own diagnostic.
        message: String,
    },
    /// A value could not be serialized back to JSON.
    #[error("serialize {context}: {source}")]
    Serialize {
        /// What was being serialized.
        context: &'static str,
        /// The serializer's own diagnostic.
        #[source]
        source: serde_json::Error,
    },
}
