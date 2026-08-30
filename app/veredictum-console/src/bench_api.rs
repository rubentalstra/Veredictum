// SPDX-FileCopyrightText: Veredictum contributors
// SPDX-License-Identifier: Apache-2.0

//! S10 — the benchmark surface (#166) over the published bench-result family.
//!
//! A bench record is a speed measurement, and the boundary statement it
//! carries says so in its own words. Every surface here renders that sentence
//! verbatim from the record rather than from a constant this crate owns, so a
//! page can never claim less than the document it is showing.
//!
//! The console renders and never decides. Submittability, the unmet
//! requirements, the relative index and the posture assurance labels are read
//! out of the record as written; nothing here recomputes an index or a
//! verdict. The comparison alignment, the failed-arrival readings and the
//! mismatch warnings are derived from disclosed numbers, which is
//! presentation, and each carries the issue that moves it onto the engine.
//!
//! The read types below mirror the published `bench-result` document because
//! the pinned engine version predates the bench module. The mirror is
//! deserialize-only and the fixture gate holds it to
//! `schemas/bench-result.schema.json`, so it cannot drift from the family it
//! reads.

use serde::{Deserialize, Serialize};

/// The server-owned route a batch of bench records is posted to.
pub const UPLOAD_PATH: &str = "/benchmarks/upload";

/// The command-line equivalent of the detail surface.
pub const CLI_DETAIL: &str = "veredictum bench --pack <id> --base-url <url> --out <dir>";

/// The command-line equivalent of the comparison surface.
pub const CLI_COMPARE: &str = "veredictum bench-compare --result <a.json> --result <b.json>";

/// Why the recorded distributions are tabulated rather than drawn.
///
/// Every operation carries the standard `HdrHistogram` V2 encoding of its own
/// latencies, so a reader recomputes every percentile from the artifact. The
/// console does not decode it: the histogram reader is the engine's, and the
/// engine version this console pins predates the bench module.
// TODO(#179): draw each operation's decoded HDR distribution once the engine
// pin carries `veredictum::bench`.
pub const HISTOGRAM_NOTE: &str = "Each operation below carries the standard HdrHistogram V2 encoding of its recorded latencies, so every percentile here is recomputable from the record itself. The distribution is not drawn on this page: decoding it is the engine's own histogram reader, which this console reaches only once its engine pin carries the bench module.";

/// Which mount a listed record was read from.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum BenchSource {
    /// A record sitting in the operator's mounted output directory.
    Mounted,
    /// A record uploaded to this page, which is transient and swept.
    Uploaded,
}

impl BenchSource {
    /// The token the surface labels the record with.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            BenchSource::Mounted => "mounted",
            BenchSource::Uploaded => "uploaded",
        }
    }
}

/// One record in the listing: what it is, and enough to decide whether to open
/// it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BenchRecordRef {
    /// The address this record is opened and compared by.
    pub key: String,
    /// The operator's label, falling back to the file name.
    pub label: String,
    /// The file name the record sits under.
    pub file: String,
    /// Which mount it came from.
    pub source: BenchSource,
    /// The pack, as `id@version`.
    pub pack: String,
    /// The system the run was pointed at.
    pub target: String,
    /// The generator host, as one `key=value` line.
    pub machine: String,
    /// When the run started, RFC 3339, verbatim.
    pub started_at: String,
    /// How many repetitions the run executed.
    pub repetitions: u32,
    /// Whether the record meets every submission requirement, as written.
    pub submittable: bool,
    /// The requirement tokens it does not meet, as written.
    pub unmet: Vec<String>,
    /// The posture profile the run declared.
    pub posture_profile: String,
}

/// The listing surface: every bench record the console can see.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BenchListing {
    /// The mounted output root the scan walked, verbatim.
    pub out: String,
    /// Every distinct boundary statement the listed records carry.
    pub boundary_statements: Vec<String>,
    /// The records, newest start first.
    pub records: Vec<BenchRecordRef>,
    /// One line per file that looked like a record and would not read, so a
    /// broken document is never silently absent.
    pub unreadable: Vec<String>,
}

/// One operation's cross-repetition figures, in the CLI's own column order.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct OperationRow {
    /// The operation token.
    pub operation: String,
    /// How many repetitions carried it.
    pub repetitions: u32,
    /// Cross-repetition median of the median latency, microseconds.
    pub p50_us: f64,
    /// Cross-repetition median of the 90th percentile, microseconds.
    pub p90_us: f64,
    /// Cross-repetition median of the 99th percentile, microseconds.
    pub p99_us: f64,
    /// Cross-repetition median of the 99.9th percentile, microseconds.
    pub p999_us: f64,
    /// Cross-repetition median throughput, operations per second.
    pub throughput_ops_s: f64,
    /// The inter-quartile range of the 99th percentile, microseconds.
    pub p99_iqr_us: f64,
}

/// One phase's table, labelled with the discipline that produced it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PhaseTable {
    /// The phase name.
    pub phase: String,
    /// The regime token, so a closed-loop average is never read as an
    /// open-loop percentile.
    pub regime: String,
    /// One row per operation.
    pub rows: Vec<OperationRow>,
}

/// One executed bulk load, which is closed-loop throughput and nothing else.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SeedPhaseRow {
    /// The phase name.
    pub name: String,
    /// The regime token.
    pub regime: String,
    /// EHRs created.
    pub ehrs: u64,
    /// Compositions committed into each EHR.
    pub compositions_per_ehr: u64,
    /// The worker pool the bulk load ran on.
    pub workers: u64,
    /// Wall-clock seconds.
    pub elapsed_s: f64,
    /// Writes divided by elapsed seconds.
    pub writes_per_s: f64,
    /// Whole-loop milliseconds per committed composition.
    pub ms_per_composition: f64,
}

/// One executed closed-loop sweep within one repetition.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SweepRow {
    /// The phase name.
    pub name: String,
    /// The regime token.
    pub regime: String,
    /// The one-based repetition ordinal.
    pub repetition: u32,
    /// Requests the walk issued.
    pub requests: u64,
    /// Compositions the walk visited.
    pub compositions: u64,
    /// The worker pool the walk ran on.
    pub workers: u64,
    /// Wall-clock seconds.
    pub elapsed_s: f64,
    /// Whole-loop microseconds per request.
    pub us_per_request: f64,
}

/// One disclosed posture item, with the label the record stands behind it by.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PostureLine {
    /// The item token.
    pub item: String,
    /// The declared value's token.
    pub declared: String,
    /// `verified` or `declared-only`, as the record wrote it.
    pub assurance: String,
    /// Whether the record stands behind the item first-hand.
    pub verified: bool,
    /// The bracketing canary evidence, one sentence per reading.
    pub evidence: Vec<String>,
}

/// One phase's failed-arrival reading, on one measured side.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FailedShareRow {
    /// Which side produced the reading.
    pub side: String,
    /// The one-based repetition ordinal.
    pub repetition: u32,
    /// The phase name.
    pub phase: String,
    /// The regime token.
    pub regime: String,
    /// Arrivals the phase recorded, failures included.
    pub count: u64,
    /// How many of them failed.
    pub errors: u64,
    /// The phase's own failed share.
    pub share: f64,
    /// The worst operation, when the phase recorded one.
    pub worst_operation: Option<String>,
    /// That operation's failed share.
    pub worst_share: f64,
    /// Whether the reading is above the pack's ceiling.
    pub breaches: bool,
}

/// One same-machine reference run, as the record discloses it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BaselineCard {
    /// The reference CDR token.
    pub cdr: String,
    /// Its human-readable name.
    pub display_name: String,
    /// The base URL it was driven over.
    pub base_url: String,
    /// The version it disclosed about itself, when it disclosed one.
    pub sut_version: Option<String>,
    /// The upstream recipe, as one line.
    pub recipe: String,
    /// The container ceilings, as one line.
    pub resources: String,
    /// The digest-pinned images, role then reference.
    pub images: Vec<(String, String)>,
    /// The posture profile the baseline ran under.
    pub posture_profile: String,
}

/// One ratio of the relative index, with the two medians behind it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RelativeRow {
    /// The phase name.
    pub phase: String,
    /// The regime token.
    pub regime: String,
    /// The operation token.
    pub operation: String,
    /// The metric token.
    pub metric: String,
    /// The target's cross-repetition median.
    pub target_median: f64,
    /// The baseline's cross-repetition median.
    pub baseline_median: f64,
    /// The ratio the record carries.
    pub index: f64,
}

/// The target measured against one baseline, as the record derived it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RelativeTable {
    /// The baseline token.
    pub baseline: String,
    /// The baseline's human-readable name.
    pub display_name: String,
    /// How every ratio was derived, verbatim from the record.
    pub derivation: String,
    /// The ratios, in record order.
    pub rows: Vec<RelativeRow>,
    /// Every place no ratio exists, one sentence each.
    pub gaps: Vec<String>,
}

/// One record in full.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BenchDetail {
    /// The address this record was opened by.
    pub key: String,
    /// The operator's label, falling back to the file name.
    pub label: String,
    /// The file name the record sits under.
    pub file: String,
    /// Which mount it came from.
    pub source: BenchSource,
    /// What a bench result is and is not, verbatim from the record.
    pub boundary_statement: String,
    /// The methodology the run followed, verbatim from the record.
    pub methodology_statement: String,
    /// The pack, as `id@version`.
    pub pack: String,
    /// What the pack exercises, verbatim.
    pub pack_description: String,
    /// The seed every arrival stream drew from, as the CLI prints it.
    pub seed: String,
    /// The failed-arrival ceiling the pack version pins.
    pub max_failed_share: f64,
    /// The base URL the run drove.
    pub target: String,
    /// The version the system disclosed about itself, when it disclosed one.
    pub sut_version: Option<String>,
    /// The generator host, as one `key=value` line.
    pub machine: String,
    /// When the run started, RFC 3339, verbatim.
    pub started_at: String,
    /// When it finished, RFC 3339, verbatim.
    pub finished_at: String,
    /// The multiplier the run applied to the pack's seed population.
    pub scale_factor: f64,
    /// Whether every seed phase ran at its declared worker count.
    pub declared_workers: bool,
    /// Whether the run matched the pack's pinned configuration.
    pub reference_configuration: bool,
    /// The instant every `version_at_time` read addressed, when the pack
    /// drives one.
    pub version_at_time: Option<String>,
    /// How many repetitions the run executed.
    pub repetitions: u32,
    /// Whether the record meets every submission requirement, as written.
    pub submittable: bool,
    /// The unmet requirements: the token as written, and what it asks for.
    pub unmet: Vec<(String, String)>,
    /// The posture profile the run declared.
    pub posture_profile: String,
    /// What that profile switches on, verbatim from the record.
    pub posture_summary: String,
    /// One line per disclosed posture item.
    pub posture: Vec<PostureLine>,
    /// Every item on which the measured deployment departs from the profile.
    pub comparability: Vec<String>,
    /// The executed bulk loads, in execution order.
    pub seed_phases: Vec<SeedPhaseRow>,
    /// The executed closed-loop sweeps.
    pub sweeps: Vec<SweepRow>,
    /// The cross-repetition tables, one per phase.
    pub phases: Vec<PhaseTable>,
    /// Every phase's failed-arrival reading, target first.
    pub failed_shares: Vec<FailedShareRow>,
    /// The same-machine references, in recorded order.
    pub baselines: Vec<BaselineCard>,
    /// The relative index, one table per reference.
    pub relative: Vec<RelativeTable>,
}

/// One cell of the comparison body.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ComparisonCell {
    /// The cross-repetition median, absent where the column carries no such
    /// operation.
    pub median: Option<f64>,
    /// The inter-quartile range beside it.
    pub iqr: Option<f64>,
}

/// One column of a comparison: everything about the record that is not a
/// number in the body.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ComparisonColumn {
    /// The record's address.
    pub key: String,
    /// The operator's label, falling back to the file name.
    pub label: String,
    /// The pack, as `id@version`.
    pub pack: String,
    /// The version the system disclosed about itself.
    pub sut_version: Option<String>,
    /// The generator host, as one `key=value` line.
    pub machine: String,
    /// How many repetitions the run executed.
    pub repetitions: u32,
    /// Whether the record is submittable, as written.
    pub submittable: bool,
    /// The requirement tokens it does not meet, as written.
    pub unmet: Vec<String>,
    /// The failed-arrival ceiling the column's pack version pins.
    pub max_failed_share: f64,
    /// The largest failed share any one operation of the record recorded.
    pub worst_failed_share: f64,
    /// The multiplier the run applied to the pack's seed population.
    pub scale_factor: f64,
    /// Whether the run matched the pack's pinned configuration.
    pub reference_configuration: bool,
    /// The posture profile the run declared.
    pub posture_profile: String,
    /// The whole disclosure on one line, which is what decides whether two
    /// columns describe the same sport.
    pub posture_signature: String,
    /// The relative index the record carries, one table per reference.
    pub relative: Vec<RelativeTable>,
}

/// One aligned row: the same phase, operation and metric across every column.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ComparisonRow {
    /// The phase the row belongs to.
    pub phase: String,
    /// The discipline that produced the row's numbers.
    pub regime: String,
    /// The operation token.
    pub operation: String,
    /// The metric token.
    pub metric: String,
    /// One cell per column, in column order.
    pub cells: Vec<ComparisonCell>,
}

/// Several records, aligned into one table.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BenchComparison {
    /// Every distinct boundary statement the columns carry.
    pub boundary_statements: Vec<String>,
    /// The columns, in the order the keys were given.
    pub columns: Vec<ComparisonColumn>,
    /// Everything that makes the columns less than directly comparable.
    pub warnings: Vec<String>,
    /// The aligned rows, sorted by phase, then operation, then metric.
    pub rows: Vec<ComparisonRow>,
}

/// What the benchmarks surface shows.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum BenchScreen {
    /// The listing, with no record opened.
    Listing(Box<BenchListing>),
    /// One record in full.
    Record(Box<BenchDetail>),
    /// The record the address named is gone or was never here.
    Unknown {
        /// Why the address resolved to nothing.
        reason: String,
    },
}

/// The comparison pane's answer.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum CompareScreen {
    /// Nothing is selected, so nothing is compared.
    Idle,
    /// One record is selected; a comparison needs a second.
    NeedsMore {
        /// How many records are selected.
        selected: u32,
    },
    /// The aligned table.
    Aligned(Box<BenchComparison>),
    /// A selected address resolved to nothing.
    Unknown {
        /// Why the selection could not be aligned.
        reason: String,
    },
}

/// Microseconds as the CLI prints them.
#[must_use]
pub fn us(value: f64) -> String {
    format!("{value:.0}")
}

/// The same figure in milliseconds, which is the unit a reader compares in.
#[must_use]
pub fn ms(value: f64) -> String {
    format!("{:.3}", value / 1000.0)
}

/// A throughput, as the CLI prints it.
#[must_use]
pub fn ops(value: f64) -> String {
    format!("{value:.1}")
}

/// A share or a ratio, as the CLI prints it.
#[must_use]
pub fn ratio(value: f64) -> String {
    format!("{value:.3}")
}

#[cfg(feature = "ssr")]
pub mod mirror {
    //! The deserialize-only mirror of the published `bench-result` document.
    //!
    //! Every type here is the shape `schemas/bench-result.schema.json`
    //! publishes. Closed vocabularies are enums, so an unknown token is a loud
    //! parse failure rather than a silent default.
    // TODO(#179): replace with veredictum::bench types when the engine pin
    // catches up.

    use std::collections::BTreeMap;

    use serde::Deserialize;

    // NOTE: no openEHR spec governs this — our own design; the reader is
    // deliberately lenient about UNKNOWN FIELDS so a record written by a newer
    // engine still opens, while every closed vocabulary below stays strict.

    /// Which load regime produced a phase's numbers.
    #[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
    #[serde(rename_all = "kebab-case")]
    pub enum LoopRegime {
        /// Arrivals wait for a worker, so offered load is an outcome.
        ClosedLoop,
        /// Arrivals fire at their planned instants.
        OpenLoop,
    }

    impl LoopRegime {
        /// The token the record writes.
        #[must_use]
        pub const fn as_str(self) -> &'static str {
            match self {
                LoopRegime::ClosedLoop => "closed-loop",
                LoopRegime::OpenLoop => "open-loop",
            }
        }
    }

    /// How one arrival failed.
    #[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Deserialize)]
    #[serde(rename_all = "snake_case")]
    pub enum ErrorClass {
        /// A 2xx the operation does not accept.
        Http2xx,
        /// A redirection the engine does not follow.
        Http3xx,
        /// A client-error status.
        Http4xx,
        /// A server-error status.
        Http5xx,
        /// A status outside the 2xx-5xx range.
        HttpOther,
        /// The request never reached a response.
        Transport,
        /// The request exceeded the client timeout.
        Timeout,
    }

    /// What a record must carry before it may be offered for ranking.
    #[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
    #[serde(rename_all = "snake_case")]
    pub enum SubmissionRequirement {
        /// At least three repetitions.
        Repetitions,
        /// At least one same-machine baseline block.
        Baseline,
        /// Every reading at or below the pack's failed-arrival ceiling.
        ErrorShare,
    }

    impl SubmissionRequirement {
        /// The token the record writes.
        #[must_use]
        pub const fn as_str(self) -> &'static str {
            match self {
                SubmissionRequirement::Repetitions => "repetitions",
                SubmissionRequirement::Baseline => "baseline",
                SubmissionRequirement::ErrorShare => "error_share",
            }
        }

        /// What the requirement asks for, as one sentence beside the refusal.
        #[must_use]
        pub const fn statement(self) -> &'static str {
            match self {
                SubmissionRequirement::Repetitions => {
                    "at least 3 repetitions, because one repetition measures a moment rather than a system"
                }
                SubmissionRequirement::Baseline => {
                    "at least one same-machine baseline, because an absolute number without an anchor describes the machine as much as the system"
                }
                SubmissionRequirement::ErrorShare => {
                    "every repetition, phase and operation, on the target and on every baseline, at or below the pack's failed-arrival ceiling, because percentiles taken over failed arrivals measure the failure rather than the system"
                }
            }
        }
    }

    /// Which disclosed posture item a line describes.
    #[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
    #[serde(rename_all = "snake_case")]
    pub enum PostureItem {
        /// Whether the deployment writes an audit trail.
        Audit,
        /// Whether committed versions carry a signature.
        VersionSigning,
        /// How far the deployment validates a commit.
        CommitValidation,
        /// How the run authenticated.
        Authn,
        /// Whether the measured traffic rode TLS.
        Tls,
        /// Whether responses came back compressed.
        Compression,
        /// Whether the deployment serves one tenant or many.
        Tenancy,
    }

    impl PostureItem {
        /// The token the record writes.
        #[must_use]
        pub const fn as_str(self) -> &'static str {
            match self {
                PostureItem::Audit => "audit",
                PostureItem::VersionSigning => "version_signing",
                PostureItem::CommitValidation => "commit_validation",
                PostureItem::Authn => "authn",
                PostureItem::Tls => "tls",
                PostureItem::Compression => "compression",
                PostureItem::Tenancy => "tenancy",
            }
        }
    }

    /// How much of the declared value the record stands behind.
    #[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
    #[serde(rename_all = "kebab-case")]
    pub enum Assurance {
        /// Both brackets observed the declared value first-hand.
        Verified,
        /// Nothing on the wire discloses the item.
        DeclaredOnly,
    }

    impl Assurance {
        /// The token the record writes.
        #[must_use]
        pub const fn as_str(self) -> &'static str {
            match self {
                Assurance::Verified => "verified",
                Assurance::DeclaredOnly => "declared-only",
            }
        }
    }

    /// Which end of the measured window a canary reading was taken at.
    #[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
    #[serde(rename_all = "kebab-case")]
    pub enum Bracket {
        /// Before the first measured repetition.
        Before,
        /// After the last measured repetition.
        After,
    }

    impl Bracket {
        /// The token the record writes.
        #[must_use]
        pub const fn as_str(self) -> &'static str {
            match self {
                Bracket::Before => "before",
                Bracket::After => "after",
            }
        }
    }

    /// What one canary reading concluded.
    #[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
    #[serde(rename_all = "kebab-case")]
    pub enum CanaryOutcome {
        /// The observation agrees with the declaration.
        Confirmed,
        /// No observable exists, or the probe could not complete.
        NotObservable,
        /// The observation disagrees with the declaration.
        Contradicted,
    }

    /// Why no index could be formed for one phase, operation or metric.
    #[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
    #[serde(rename_all = "kebab-case")]
    pub enum GapReason {
        /// The baseline carries no such phase.
        PhaseAbsentFromBaseline,
        /// The target carries no such phase.
        PhaseAbsentFromTarget,
        /// The baseline never measured this operation.
        OperationAbsentFromBaseline,
        /// The target never measured this operation.
        OperationAbsentFromTarget,
        /// The baseline's median is zero, so the ratio is undefined.
        ZeroBaselineMedian,
    }

    impl GapReason {
        /// The token the record writes.
        #[must_use]
        pub const fn as_str(self) -> &'static str {
            match self {
                GapReason::PhaseAbsentFromBaseline => "phase-absent-from-baseline",
                GapReason::PhaseAbsentFromTarget => "phase-absent-from-target",
                GapReason::OperationAbsentFromBaseline => "operation-absent-from-baseline",
                GapReason::OperationAbsentFromTarget => "operation-absent-from-target",
                GapReason::ZeroBaselineMedian => "zero-baseline-median",
            }
        }
    }

    /// The metrics a comparison aligns, in the order it renders them.
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub enum Metric {
        /// Median latency, microseconds.
        P50Us,
        /// 75th-percentile latency, microseconds.
        P75Us,
        /// 90th-percentile latency, microseconds.
        P90Us,
        /// 99th-percentile latency, microseconds.
        P99Us,
        /// 99.9th-percentile latency, microseconds.
        P999Us,
        /// Throughput, operations per second.
        ThroughputOpsS,
    }

    impl Metric {
        /// Every metric, in render order.
        pub const ALL: &[Metric] = &[
            Metric::P50Us,
            Metric::P75Us,
            Metric::P90Us,
            Metric::P99Us,
            Metric::P999Us,
            Metric::ThroughputOpsS,
        ];

        /// The column label.
        #[must_use]
        pub const fn as_str(self) -> &'static str {
            match self {
                Metric::P50Us => "p50_us",
                Metric::P75Us => "p75_us",
                Metric::P90Us => "p90_us",
                Metric::P99Us => "p99_us",
                Metric::P999Us => "p999_us",
                Metric::ThroughputOpsS => "throughput_ops_s",
            }
        }

        /// This metric's cross-repetition summary within one operation.
        #[must_use]
        pub const fn of(self, cross: &CrossOperation) -> &CrossStat {
            match self {
                Metric::P50Us => &cross.p50_us,
                Metric::P75Us => &cross.p75_us,
                Metric::P90Us => &cross.p90_us,
                Metric::P99Us => &cross.p99_us,
                Metric::P999Us => &cross.p999_us,
                Metric::ThroughputOpsS => &cross.throughput_ops_s,
            }
        }
    }

    /// One operation's measured statistics within one repetition of a phase.
    #[derive(Debug, Clone, PartialEq, Deserialize)]
    pub struct OperationStats {
        /// Arrivals recorded in the measured span, failures included.
        pub count: u64,
        /// How many of those arrivals failed.
        pub errors: u64,
        /// The failures by class.
        pub errors_by_class: BTreeMap<ErrorClass, u64>,
        /// Recorded arrivals divided by the measured span.
        pub throughput_ops_s: f64,
        /// Median latency, microseconds.
        pub p50_us: u64,
        /// 99th-percentile latency, microseconds.
        pub p99_us: u64,
        /// The largest recorded latency, microseconds.
        pub max_us: u64,
        /// The standard `HdrHistogram` V2 encoding, base64.
        pub hdr_v2_base64: String,
    }

    /// One measured phase within one repetition.
    #[derive(Debug, Clone, PartialEq, Deserialize)]
    pub struct MeasuredPhaseRecord {
        /// The discipline that produced the phase.
        pub regime: LoopRegime,
        /// Whether the generator, rather than the system, was the bottleneck.
        pub generator_bound: bool,
        /// Per-operation statistics, keyed by the operation token.
        pub operations: BTreeMap<String, OperationStats>,
    }

    /// One executed closed-loop sweep.
    #[derive(Debug, Clone, PartialEq, Deserialize)]
    pub struct SweepPhaseRecord {
        /// The phase name from the pack.
        pub name: String,
        /// The discipline, stated rather than implied.
        pub regime: LoopRegime,
        /// The worker pool the walk ran on.
        pub workers: u64,
        /// Compositions the walk visited.
        pub compositions: u64,
        /// Requests the walk issued in total.
        pub requests: u64,
        /// Wall-clock seconds the walk took.
        pub elapsed_s: f64,
        /// Whole-loop microseconds per request.
        pub whole_loop_us_per_request: f64,
        /// Per-operation statistics, keyed by the operation token.
        pub operations: BTreeMap<String, OperationStats>,
    }

    /// One executed bulk load.
    #[derive(Debug, Clone, PartialEq, Deserialize)]
    pub struct SeedPhaseRecord {
        /// The phase name from the pack.
        pub name: String,
        /// The discipline, stated rather than implied.
        pub regime: LoopRegime,
        /// EHRs created.
        pub ehrs: u64,
        /// Compositions committed into each EHR.
        pub compositions_per_ehr: u64,
        /// The worker pool the bulk load ran on.
        pub workers: u64,
        /// Wall-clock seconds the bulk load took.
        pub elapsed_s: f64,
        /// Writes divided by elapsed seconds.
        pub bulk_load_writes_per_s: f64,
        /// Whole-loop milliseconds per committed composition.
        pub whole_loop_ms_per_composition: f64,
    }

    /// One repetition of every measured phase in the pack.
    #[derive(Debug, Clone, PartialEq, Deserialize)]
    pub struct RepetitionRecord {
        /// The one-based repetition ordinal.
        pub repetition: u32,
        /// The open-loop measured phases, keyed by phase name.
        pub phases: BTreeMap<String, MeasuredPhaseRecord>,
        /// The closed-loop sweeps, keyed by phase name.
        #[serde(default)]
        pub sweeps: BTreeMap<String, SweepPhaseRecord>,
    }

    /// A median and an inter-quartile range over one value.
    #[derive(Debug, Clone, PartialEq, Deserialize)]
    pub struct CrossStat {
        /// The median of the per-repetition values.
        pub median: f64,
        /// The inter-quartile range of the per-repetition values.
        pub iqr: f64,
    }

    /// The cross-repetition summary for one operation of one phase.
    #[derive(Debug, Clone, PartialEq, Deserialize)]
    pub struct CrossOperation {
        /// How many repetitions carried this operation.
        pub repetitions: u32,
        /// Cross-repetition median latency.
        pub p50_us: CrossStat,
        /// Cross-repetition 75th percentile.
        pub p75_us: CrossStat,
        /// Cross-repetition 90th percentile.
        pub p90_us: CrossStat,
        /// Cross-repetition 99th percentile.
        pub p99_us: CrossStat,
        /// Cross-repetition 99.9th percentile.
        pub p999_us: CrossStat,
        /// Cross-repetition throughput.
        pub throughput_ops_s: CrossStat,
    }

    /// The cross-repetition summary of one phase.
    #[derive(Debug, Clone, PartialEq, Deserialize)]
    pub struct CrossPhase {
        /// The discipline that produced this phase's numbers.
        pub regime: LoopRegime,
        /// Per-operation summaries, keyed by the operation token.
        pub operations: BTreeMap<String, CrossOperation>,
    }

    /// The pack a run drove, as the record writes it.
    #[derive(Debug, Clone, PartialEq, Deserialize)]
    pub struct PackRecord {
        /// The pack id.
        pub id: String,
        /// The pack version.
        pub version: String,
        /// What the pack exercises.
        pub description: String,
        /// The failed-arrival ceiling the pack version pins.
        pub max_failed_share: f64,
        /// The seed every arrival stream drew from.
        pub seed: u64,
    }

    /// The system the run was pointed at.
    #[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
    pub struct TargetRecord {
        /// The base URL, with any userinfo removed.
        pub base_url: String,
        /// The version the system disclosed about itself.
        #[serde(default)]
        pub sut_version: Option<String>,
    }

    /// What the engine could establish about the machine that offered the load.
    #[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
    pub struct EnvironmentFingerprint {
        /// The target architecture.
        pub arch: String,
        /// The target operating system.
        pub os: String,
        /// The parallelism the process may use.
        #[serde(default)]
        pub available_parallelism: Option<u32>,
        /// The CPU model string.
        #[serde(default)]
        pub cpu_model: Option<String>,
        /// Total physical memory in bytes.
        #[serde(default)]
        pub total_memory_bytes: Option<u64>,
    }

    impl EnvironmentFingerprint {
        /// The fingerprint as one ordered `key=value` line.
        ///
        /// The same ordering the engine's own label map has, so the console's
        /// header line and the CLI's read alike.
        #[must_use]
        pub fn line(&self) -> String {
            let mut labels = vec![format!("arch={}", self.arch), format!("os={}", self.os)];
            if let Some(cores) = self.available_parallelism {
                labels.push(format!("available_parallelism={cores}"));
            }
            if let Some(model) = &self.cpu_model {
                labels.push(format!("cpu_model={model}"));
            }
            if let Some(bytes) = self.total_memory_bytes {
                labels.push(format!("total_memory_bytes={bytes}"));
            }
            labels.sort();
            labels.join(" ")
        }
    }

    /// How far the run departed from the pack's pinned configuration.
    #[derive(Debug, Clone, PartialEq, Deserialize)]
    pub struct ScaleRecord {
        /// The multiplier applied to every seed phase's EHR count.
        pub factor: f64,
        /// Whether every seed phase ran at its declared worker count.
        pub declared_workers: bool,
        /// Whether the run matches the pack's pinned configuration.
        pub reference_configuration: bool,
    }

    /// The methodology block.
    #[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
    pub struct Methodology {
        /// The prose statement, verbatim.
        pub statement: String,
    }

    /// One canary reading.
    #[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
    pub struct CanaryReading {
        /// Which end of the window the reading was taken at.
        pub bracket: Bracket,
        /// What the reading concluded.
        pub outcome: CanaryOutcome,
        /// What the probe actually saw.
        pub observed: String,
        /// The exchange the reading came from, in one sentence.
        pub evidence: String,
    }

    /// One disclosed posture item.
    #[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
    pub struct PostureDisclosure {
        /// Which item this line describes.
        pub item: PostureItem,
        /// The declared value's token.
        pub declared: String,
        /// Whether the record stands behind the declaration first-hand.
        pub assurance: Assurance,
        /// The bracketing readings.
        pub readings: Vec<CanaryReading>,
    }

    /// One item on which the measured deployment departs from the profile.
    #[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
    pub struct PostureDivergence {
        /// Which item the two disagree on.
        pub item: PostureItem,
        /// The token the named profile assigns the item.
        pub profile_declares: String,
        /// The token the measured deployment configures.
        pub deployment_configures: String,
        /// Where that was read first-hand.
        pub source: String,
    }

    /// The posture block one record carries.
    #[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
    pub struct PostureRecord {
        /// The declared profile's name.
        pub profile: String,
        /// What that profile switches on, verbatim.
        pub summary: String,
        /// One line per item.
        pub items: Vec<PostureDisclosure>,
        /// Every item on which the deployment departs from the profile.
        #[serde(default)]
        pub comparability: Vec<PostureDivergence>,
    }

    impl PostureRecord {
        /// A one-line `item=value` rendering of the whole block.
        ///
        /// Two runs are the same sport exactly when their signatures match,
        /// which is what a comparison compares.
        #[must_use]
        pub fn signature(&self) -> String {
            self.items
                .iter()
                .map(|line| format!("{}={}", line.item.as_str(), line.declared))
                .collect::<Vec<_>>()
                .join(" ")
        }
    }

    /// The upstream deployment recipe a baseline's topology follows.
    #[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
    pub struct RecipeReference {
        /// The repository the recipe lives in.
        pub repository: String,
        /// The immutable tag the recipe is read at.
        pub git_ref: String,
        /// The recipe file within that repository.
        pub file: String,
    }

    /// The container ceilings a baseline was composed under.
    #[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
    pub struct BaselineResources {
        /// The server container's CPU limit.
        pub server_cpus: String,
        /// The server container's memory limit.
        pub server_memory: String,
        /// The database container's CPU limit.
        pub database_cpus: String,
        /// The database container's memory limit.
        pub database_memory: String,
        /// The database container's shared-memory size.
        pub database_shm_size: String,
    }

    /// One reference CDR measured on the same host, in the same session.
    #[derive(Debug, Clone, PartialEq, Deserialize)]
    pub struct BaselineRecord {
        /// The reference CDR token.
        pub cdr: String,
        /// That reference's human-readable name.
        pub display_name: String,
        /// The digest-pinned images, keyed by role.
        pub images: BTreeMap<String, String>,
        /// The upstream deployment recipe.
        pub recipe: RecipeReference,
        /// The container ceilings.
        pub resources: BaselineResources,
        /// The base URL the pack was driven over.
        pub base_url: String,
        /// The version the baseline disclosed about itself.
        #[serde(default)]
        pub sut_version: Option<String>,
        /// Every repetition, in execution order.
        pub repetitions: Vec<RepetitionRecord>,
        /// The baseline's own posture block.
        pub posture: PostureRecord,
    }

    /// One ratio, with the two medians it was derived from.
    #[derive(Debug, Clone, PartialEq, Deserialize)]
    pub struct RelativeRatio {
        /// The target's cross-repetition median.
        pub target_median: f64,
        /// The baseline's cross-repetition median.
        pub baseline_median: f64,
        /// The target median divided by the baseline median.
        pub index: f64,
    }

    /// One operation's ratios, keyed by the metric token.
    #[derive(Debug, Clone, PartialEq, Deserialize)]
    pub struct RelativeOperation {
        /// One entry per metric an index could be formed for.
        pub metrics: BTreeMap<String, RelativeRatio>,
    }

    /// One phase's ratios, keyed by the operation token.
    #[derive(Debug, Clone, PartialEq, Deserialize)]
    pub struct RelativePhase {
        /// The discipline that produced both sides of every ratio.
        pub regime: LoopRegime,
        /// One entry per operation both sides measured.
        pub operations: BTreeMap<String, RelativeOperation>,
    }

    /// One place where no index could be formed, and why.
    #[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
    pub struct IndexGap {
        /// The phase the gap belongs to.
        pub phase: String,
        /// The operation the gap belongs to.
        pub operation: String,
        /// The metric, when only one metric is missing.
        #[serde(default)]
        pub metric: Option<String>,
        /// Why no index exists here.
        pub reason: GapReason,
    }

    /// The target measured against one baseline.
    #[derive(Debug, Clone, PartialEq, Deserialize)]
    pub struct RelativeIndex {
        /// The baseline token.
        pub baseline: String,
        /// That baseline's human-readable name.
        pub display_name: String,
        /// How every ratio here was derived, verbatim.
        pub derivation: String,
        /// The ratios, keyed by phase name.
        pub phases: BTreeMap<String, RelativePhase>,
        /// Every place no ratio could be formed.
        pub gaps: Vec<IndexGap>,
    }

    /// One bench run's complete record.
    #[derive(Debug, Clone, PartialEq, Deserialize)]
    pub struct BenchResult {
        /// The artifact-family schema version.
        pub schema_version: String,
        /// What a bench result is and is not, verbatim.
        pub boundary_statement: String,
        /// The operator's label, when one was given.
        #[serde(default)]
        pub label: Option<String>,
        /// The pack that was driven.
        pub pack: PackRecord,
        /// The system that was driven.
        pub target: TargetRecord,
        /// The machine that offered the load.
        pub environment: EnvironmentFingerprint,
        /// When the run started, RFC 3339.
        pub started_at: String,
        /// When the run finished, RFC 3339.
        pub finished_at: String,
        /// How far the run departed from the pack's pinned configuration.
        pub scale: ScaleRecord,
        /// The instant every `version_at_time` read addressed.
        #[serde(default)]
        pub version_at_time: Option<String>,
        /// The executed bulk loads, in execution order.
        pub seed_phases: Vec<SeedPhaseRecord>,
        /// Every repetition, in execution order.
        pub repetitions: Vec<RepetitionRecord>,
        /// The cross-repetition summary, keyed by phase name.
        pub cross: BTreeMap<String, CrossPhase>,
        /// The same-machine reference runs.
        #[serde(default)]
        pub baselines: Vec<BaselineRecord>,
        /// The target measured against every baseline.
        #[serde(default)]
        pub relative: Vec<RelativeIndex>,
        /// What the numbers mean.
        pub methodology: Methodology,
        /// Whether the run meets every submission requirement.
        pub submittable: bool,
        /// The requirements it does not meet.
        #[serde(default)]
        pub submittable_unmet: Vec<SubmissionRequirement>,
        /// The posture block.
        pub posture: PostureRecord,
    }
}

#[cfg(feature = "ssr")]
pub mod derive {
    //! The arithmetic a record discloses but does not carry.
    //!
    //! The failed-arrival readings, the comparison alignment and its mismatch
    //! warnings are pure functions of numbers the record already states,
    //! carrying the engine's own definitions. No index or verdict is
    //! recomputed here.
    // TODO(#179): call veredictum::bench::result and veredictum::bench::compare
    // when the engine pin catches up.

    use std::collections::{BTreeMap, BTreeSet};

    use super::mirror::{
        BenchResult, LoopRegime, Metric, OperationStats, PostureRecord, RepetitionRecord,
    };
    use super::{ComparisonCell, ComparisonColumn, ComparisonRow, FailedShareRow};

    /// One count as a float, for a ratio the record does not carry.
    #[expect(
        clippy::as_conversions,
        clippy::cast_precision_loss,
        reason = "arrival counts are far below 2^52, so neither side of the ratio loses a digit"
    )]
    fn wide(value: u64) -> f64 {
        value as f64
    }

    /// The share of one operation's recorded arrivals that failed.
    ///
    /// An operation that recorded no arrival at all is fully failed rather
    /// than perfect: nothing answered, so nothing was measured.
    #[must_use]
    pub fn failed_share(count: u64, errors: u64) -> f64 {
        if count == 0 {
            return 1.0;
        }
        wide(errors.min(count)) / wide(count)
    }

    /// One phase's reading, over the operations it recorded.
    fn one_reading(
        side: &str,
        repetition: u32,
        phase: &str,
        regime: LoopRegime,
        operations: &BTreeMap<String, OperationStats>,
        ceiling: f64,
    ) -> FailedShareRow {
        let mut count = 0_u64;
        let mut errors = 0_u64;
        let mut worst_operation: Option<String> = None;
        let mut worst_share = 1.0_f64;
        for (operation, stats) in operations {
            count = count.saturating_add(stats.count);
            errors = errors.saturating_add(stats.errors);
            let share = failed_share(stats.count, stats.errors);
            if worst_operation.is_none() || share > worst_share {
                worst_operation = Some(operation.clone());
                worst_share = share;
            }
        }
        FailedShareRow {
            side: side.to_owned(),
            repetition,
            phase: phase.to_owned(),
            regime: regime.as_str().to_owned(),
            count,
            errors,
            share: failed_share(count, errors),
            worst_operation,
            worst_share,
            breaches: worst_share > ceiling,
        }
    }

    /// Every phase reading of one measured side.
    fn side_readings(
        side: &str,
        repetitions: &[RepetitionRecord],
        ceiling: f64,
    ) -> Vec<FailedShareRow> {
        let mut readings = Vec::new();
        for repetition in repetitions {
            for (phase, measured) in &repetition.phases {
                readings.push(one_reading(
                    side,
                    repetition.repetition,
                    phase,
                    measured.regime,
                    &measured.operations,
                    ceiling,
                ));
            }
            for (phase, sweep) in &repetition.sweeps {
                readings.push(one_reading(
                    side,
                    repetition.repetition,
                    phase,
                    sweep.regime,
                    &sweep.operations,
                    ceiling,
                ));
            }
        }
        readings
    }

    /// The failed-arrival reading of every phase a record carries, the
    /// target's first and then each baseline's, in recorded order.
    #[must_use]
    pub fn failed_shares(result: &BenchResult) -> Vec<FailedShareRow> {
        let ceiling = result.pack.max_failed_share;
        let mut readings = side_readings("the target", &result.repetitions, ceiling);
        for baseline in &result.baselines {
            readings.extend(side_readings(
                &format!("the {} baseline", baseline.cdr),
                &baseline.repetitions,
                ceiling,
            ));
        }
        readings
    }

    /// The largest failed share any one operation of any side recorded.
    #[must_use]
    pub fn worst_failed_share(result: &BenchResult) -> f64 {
        failed_shares(result)
            .iter()
            .map(|reading| reading.worst_share)
            .fold(0.0_f64, f64::max)
    }

    /// The aligned rows over several records: one row per phase, operation and
    /// metric, one cell per column.
    #[must_use]
    pub fn align(results: &[BenchResult]) -> Vec<ComparisonRow> {
        let mut keys: BTreeMap<(String, String), LoopRegime> = BTreeMap::new();
        for result in results {
            for (phase, cross) in &result.cross {
                for operation in cross.operations.keys() {
                    let _kept = keys
                        .entry((phase.clone(), operation.clone()))
                        .or_insert(cross.regime);
                }
            }
        }
        let mut rows = Vec::new();
        for ((phase, operation), regime) in keys {
            for metric in Metric::ALL {
                let cells = results
                    .iter()
                    .map(|result| {
                        let stat = result
                            .cross
                            .get(&phase)
                            .and_then(|cross| cross.operations.get(&operation))
                            .map(|cross| metric.of(cross));
                        ComparisonCell {
                            median: stat.map(|s| s.median),
                            iqr: stat.map(|s| s.iqr),
                        }
                    })
                    .collect();
                rows.push(ComparisonRow {
                    phase: phase.clone(),
                    regime: regime.as_str().to_owned(),
                    operation: operation.clone(),
                    metric: metric.as_str().to_owned(),
                    cells,
                });
            }
        }
        rows
    }

    /// The posture signature of one block, so a caller need not reach into the
    /// mirror to build a column.
    #[must_use]
    pub fn signature(posture: &PostureRecord) -> String {
        posture.signature()
    }

    /// Everything that makes a set of columns less than directly comparable.
    ///
    /// The engine's own rules, in the engine's own order: different packs,
    /// different hosts, different posture profiles, different disclosures,
    /// different scales, a cross-host set with no relative index, then each
    /// column's own submittability and configuration.
    #[must_use]
    pub fn warnings(columns: &[ComparisonColumn]) -> Vec<String> {
        let mut warnings = Vec::new();
        let packs: BTreeSet<&str> = columns.iter().map(|c| c.pack.as_str()).collect();
        if packs.len() > 1 {
            warnings.push(format!(
                "the columns ran DIFFERENT packs ({}), so the numbers describe different work",
                packs.into_iter().collect::<Vec<_>>().join(", ")
            ));
        }
        let hosts: BTreeSet<&str> = columns.iter().map(|c| c.machine.as_str()).collect();
        if hosts.len() > 1 {
            warnings.push(
                "the columns were generated from DIFFERENT hosts, so a latency difference may be the generator's".to_owned(),
            );
        }
        let profiles: BTreeSet<&str> = columns.iter().map(|c| c.posture_profile.as_str()).collect();
        if profiles.len() > 1 {
            warnings.push(format!(
                "the columns ran under DIFFERENT posture profiles ({}), so they measured systems with different features switched on",
                profiles.into_iter().collect::<Vec<_>>().join(", ")
            ));
        }
        let postures: BTreeSet<&str> = columns
            .iter()
            .map(|c| c.posture_signature.as_str())
            .collect();
        if postures.len() > 1 {
            warnings.push(format!(
                "the columns disclosed DIFFERENT postures ({}), so a difference between them may be a feature rather than the system",
                postures.into_iter().collect::<Vec<_>>().join(" | ")
            ));
        }
        let scales: BTreeSet<String> = columns
            .iter()
            .map(|c| format!("{:.3}", c.scale_factor))
            .collect();
        if scales.len() > 1 {
            warnings.push(format!(
                "the columns ran at DIFFERENT scale factors ({}), so they seeded populations of different sizes",
                scales.into_iter().collect::<Vec<_>>().join(", ")
            ));
        }
        if hosts.len() > 1 && columns.iter().any(|c| c.relative.is_empty()) {
            warnings.push(
                "the columns come from different hosts and at least one carries NO relative index, so nothing in this table is comparable across them".to_owned(),
            );
        }
        warnings.extend(columns.iter().flat_map(per_column));
        warnings
    }

    /// The warnings one column earns on its own account.
    fn per_column(column: &ComparisonColumn) -> Vec<String> {
        let mut warnings = Vec::new();
        if !column.submittable {
            warnings.push(format!(
                "column {:?} carries {} repetition(s) and is not submittable (unmet: {})",
                column.label,
                column.repetitions,
                column.unmet.join(", ")
            ));
        }
        if !column.reference_configuration {
            warnings.push(format!(
                "column {:?} ran at scale factor {:.3} off the pack's pinned configuration, so its numbers are not comparable with the reference figures the pack describes",
                column.label, column.scale_factor
            ));
        }
        warnings
    }
}

#[cfg(feature = "ssr")]
pub mod scan {
    //! Finding the records: the mounted output tree plus what was uploaded.
    //!
    //! A record is addressed by a digest of its own absolute path, so nothing
    //! a reader types ever reaches a filesystem join. The address is stable
    //! while the file is, which is what makes a detail link shareable.

    use std::path::{Path, PathBuf};

    use sha2::{Digest as _, Sha256};

    use super::BenchSource;
    use crate::state::ConsoleState;

    /// The file-name prefix the engine writes a bench result under.
    pub const RESULT_PREFIX: &str = "bench-result";

    /// The prefix every uploaded batch's scratch directory carries.
    pub const SCRATCH_PREFIX: &str = "console-bench-";

    /// How deep under the output root the walk looks.
    const MAX_DEPTH: u32 = 4;

    /// How many records one listing renders.
    const MAX_RECORDS: usize = 200;

    /// How many digest bytes an address is spelled from.
    const KEY_BYTES: usize = 8;

    /// How many hex characters an address carries.
    const KEY_CHARS: usize = KEY_BYTES * 2;

    /// One record file the walk found.
    #[derive(Debug, Clone, PartialEq, Eq)]
    pub struct Found {
        /// The address the surfaces open it by.
        pub key: String,
        /// Where it sits.
        pub path: PathBuf,
        /// Which mount it came from.
        pub source: BenchSource,
    }

    /// The address of one path: the first eight bytes of its SHA-256, hex.
    ///
    /// A path-derived address rather than the path itself, so a reader's own
    /// bytes never reach a join and a stale link resolves to nothing rather
    /// than to a file somebody chose.
    #[must_use]
    pub fn key_of(path: &Path) -> String {
        use std::fmt::Write as _;
        let mut hasher = Sha256::new();
        hasher.update(path.to_string_lossy().as_bytes());
        hasher.finalize().iter().take(KEY_BYTES).fold(
            String::with_capacity(KEY_CHARS),
            |mut out, byte| {
                let _written = write!(out, "{byte:02x}");
                out
            },
        )
    }

    /// Whether a file name is one the engine writes a bench result under.
    #[must_use]
    pub fn is_result_name(name: &str) -> bool {
        name.starts_with(RESULT_PREFIX)
            && Path::new(name)
                .extension()
                .is_some_and(|extension| extension.eq_ignore_ascii_case("json"))
    }

    /// Every bench record under the mounted output root, sorted by path.
    ///
    /// The walk is depth-bounded and count-bounded: the output root is an
    /// operator mount, and a listing that walks an unbounded tree is a page
    /// that stops answering.
    #[must_use]
    pub fn records(state: &ConsoleState) -> Vec<Found> {
        let mut found = Vec::new();
        let mut stack = vec![(state.out.clone(), 0_u32, false)];
        while let Some((dir, depth, uploaded)) = stack.pop() {
            let Ok(entries) = std::fs::read_dir(&dir) else {
                continue;
            };
            for entry in entries.flatten() {
                let name = entry.file_name();
                let Some(name) = name.to_str() else { continue };
                let path = entry.path();
                if path.is_dir() {
                    if depth < MAX_DEPTH {
                        let uploaded = uploaded || name.starts_with(SCRATCH_PREFIX);
                        stack.push((path, depth.saturating_add(1), uploaded));
                    }
                } else if is_result_name(name) {
                    found.push(Found {
                        key: key_of(&path),
                        path,
                        source: if uploaded {
                            BenchSource::Uploaded
                        } else {
                            BenchSource::Mounted
                        },
                    });
                }
            }
        }
        found.sort_by(|a, b| a.path.cmp(&b.path));
        found.truncate(MAX_RECORDS);
        found
    }

    /// The record one address names, when the scan still carries it.
    #[must_use]
    pub fn find(state: &ConsoleState, key: &str) -> Option<Found> {
        records(state).into_iter().find(|found| found.key == key)
    }
}

#[cfg(feature = "ssr")]
pub mod upload {
    //! Taking bench records from an anonymous stranger, safely.
    //!
    //! The same posture the record-verification upload takes: a size cap
    //! before anything is written, a file name rebuilt from an allowlist, a
    //! scratch directory whose name the caller cannot choose, and a sweep on a
    //! short timer. An uploaded record is transient; the console keeps no
    //! state of its own.

    use std::path::PathBuf;

    use crate::state::ConsoleState;

    /// The largest single record the page accepts, in mebibytes.
    pub const MAX_RECORD_MIB: u64 = 8;

    /// The largest single record the page accepts, in bytes.
    pub const MAX_RECORD_BYTES: u64 = MAX_RECORD_MIB * 1024 * 1024;

    /// How many records one upload may carry.
    pub const MAX_RECORDS: usize = 8;

    /// The largest whole batch the page accepts, in bytes.
    ///
    /// The router's body limit is this number, so a batch past it is refused
    /// by a sentence here rather than by a bare `413` from the transport.
    pub const MAX_BATCH_BYTES: u64 = 32 * 1024 * 1024;

    /// How long an uploaded batch survives before the next upload sweeps it.
    pub const TTL: std::time::Duration = std::time::Duration::from_hours(1);

    /// The scratch directory one batch lands in, under the output root.
    ///
    /// Named from the process id and a counter rather than from anything the
    /// caller sent, so no uploader byte reaches a path join.
    fn mint_dir(state: &ConsoleState) -> PathBuf {
        use std::sync::atomic::{AtomicU64, Ordering};
        static COUNTER: AtomicU64 = AtomicU64::new(0);

        let seq = COUNTER.fetch_add(1, Ordering::Relaxed);
        state.out.join(format!(
            "{}{}-{seq}",
            super::scan::SCRATCH_PREFIX,
            std::process::id()
        ))
    }

    /// Rebuilds one uploaded record's file name from an allowlist.
    ///
    /// The returned string is CONSTRUCTED character by checked character, so
    /// nothing the uploader typed reaches a path join, and the result always
    /// carries the prefix the listing scan looks for.
    ///
    /// # Errors
    /// The actionable refusal naming the offending name.
    pub fn safe_name(name: &str, ordinal: usize) -> Result<String, String> {
        let stem: String = name
            .chars()
            .filter(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '-' | '_'))
            .take(96)
            .collect();
        let stem = stem.trim_matches('.').to_owned();
        let json = std::path::Path::new(&stem)
            .extension()
            .is_some_and(|extension| extension.eq_ignore_ascii_case("json"));
        if !json {
            return Err(format!(
                "{name:?} is not a `.json` file — a bench record is the JSON document the engine writes"
            ));
        }
        let body = std::path::Path::new(&stem)
            .file_stem()
            .and_then(std::ffi::OsStr::to_str)
            .unwrap_or_default();
        if body.is_empty() {
            return Err(format!("{name:?} carries no file name before its suffix"));
        }
        // A record the engine wrote already carries the prefix, so the rebuild
        // keeps one copy of it rather than stacking a second.
        let tail = body
            .strip_prefix(super::scan::RESULT_PREFIX)
            .map_or(body, |rest| rest.trim_start_matches(['-', '_']));
        let tail = if tail.is_empty() { "record" } else { tail };
        Ok(format!(
            "{}-{ordinal}-{tail}.json",
            super::scan::RESULT_PREFIX
        ))
    }

    /// Removes every uploaded batch older than [`TTL`].
    ///
    /// Best effort by design: a directory that cannot be read or removed is
    /// skipped rather than turned into an upload failure.
    pub fn sweep(state: &ConsoleState) {
        let Ok(entries) = std::fs::read_dir(&state.out) else {
            return;
        };
        let now = jiff::Timestamp::now();
        for entry in entries.flatten() {
            let name = entry.file_name();
            let Some(name) = name.to_str() else { continue };
            if !name.starts_with(super::scan::SCRATCH_PREFIX) || name.contains("..") {
                continue;
            }
            let expired = entry
                .metadata()
                .and_then(|meta| meta.modified())
                .ok()
                .and_then(|at| jiff::Timestamp::try_from(at).ok())
                .and_then(|at| now.since(at).ok())
                .and_then(|age| std::time::Duration::try_from(age).ok())
                .is_some_and(|age| age > TTL);
            if expired {
                drop(std::fs::remove_dir_all(entry.path()));
            }
        }
    }

    /// Writes one uploaded batch into a fresh scratch directory.
    ///
    /// # Errors
    /// The actionable refusal: nothing chosen, too many records, a record too
    /// large, a name outside the alphabet, or a filesystem failure.
    pub fn batch(state: &ConsoleState, files: &[(String, Vec<u8>)]) -> Result<u32, String> {
        if files.is_empty() {
            return Err(String::from("no file was chosen"));
        }
        if files.len() > MAX_RECORDS {
            return Err(format!(
                "{} records were offered; the page accepts at most {MAX_RECORDS} at a time",
                files.len()
            ));
        }
        sweep(state);
        let dir = mint_dir(state);
        drop(std::fs::remove_dir_all(&dir));
        std::fs::create_dir_all(&dir).map_err(|e| format!("{}: {e}", dir.display()))?;
        let written = fill(&dir, files);
        match written {
            Ok(count) => Ok(count),
            Err(reason) => {
                drop(std::fs::remove_dir_all(&dir));
                Err(reason)
            }
        }
    }

    /// Writes every offered record into `dir`, refusing the whole batch on the
    /// first one that is not acceptable.
    fn fill(dir: &std::path::Path, files: &[(String, Vec<u8>)]) -> Result<u32, String> {
        let mut written = 0_u32;
        let mut total = 0_u64;
        for (ordinal, (name, body)) in files.iter().enumerate() {
            let size = u64::try_from(body.len()).unwrap_or(u64::MAX);
            if size > MAX_RECORD_BYTES {
                return Err(format!(
                    "{name} is {size} bytes; the page accepts at most {MAX_RECORD_BYTES} per record"
                ));
            }
            total = total.saturating_add(size);
            if total > MAX_BATCH_BYTES {
                return Err(format!(
                    "the batch is larger than {MAX_BATCH_BYTES} bytes; upload the records in smaller groups"
                ));
            }
            let on_disk = safe_name(name, ordinal)?;
            if on_disk.contains("..") || on_disk.contains('/') || on_disk.contains('\\') {
                return Err(format!("{name:?} is not a plain file name"));
            }
            std::fs::write(dir.join(&on_disk), body).map_err(|e| format!("{on_disk}: {e}"))?;
            written = written.saturating_add(1);
        }
        Ok(written)
    }
}

#[cfg(feature = "ssr")]
pub mod read {
    //! The ssr readers: the listing, one record, and an aligned comparison.

    use super::mirror::{BenchResult, PostureDisclosure, RelativeIndex, RepetitionRecord};
    use super::scan::Found;
    use super::{
        BaselineCard, BenchComparison, BenchDetail, BenchListing, BenchRecordRef, BenchScreen,
        CompareScreen, ComparisonColumn, OperationRow, PhaseTable, PostureLine, RelativeRow,
        RelativeTable, SeedPhaseRow, SweepRow,
    };
    use crate::state::ConsoleState;

    /// How many addresses one comparison aligns.
    const MAX_COLUMNS: usize = 6;

    /// Reads and parses one found record.
    ///
    /// # Errors
    /// The read or parse failure, prefixed with the file it came from.
    fn parse(found: &Found) -> Result<BenchResult, String> {
        let body = std::fs::read_to_string(&found.path)
            .map_err(|e| format!("{}: {e}", found.path.display()))?;
        serde_json::from_str(&body).map_err(|e| format!("{}: {e}", found.path.display()))
    }

    /// The file name one found record sits under.
    fn file_name(found: &Found) -> String {
        found
            .path
            .file_name()
            .and_then(std::ffi::OsStr::to_str)
            .unwrap_or("(unnamed)")
            .to_owned()
    }

    /// The listing over every record the console can see.
    #[must_use]
    pub fn listing(state: &ConsoleState) -> BenchListing {
        let mut records = Vec::new();
        let mut unreadable = Vec::new();
        let mut statements: Vec<String> = Vec::new();
        for found in super::scan::records(state) {
            match parse(&found) {
                Ok(result) => {
                    if !statements.contains(&result.boundary_statement) {
                        statements.push(result.boundary_statement.clone());
                    }
                    let file = file_name(&found);
                    records.push(BenchRecordRef {
                        key: found.key.clone(),
                        label: result.label.clone().unwrap_or_else(|| file.clone()),
                        file,
                        source: found.source,
                        pack: format!("{}@{}", result.pack.id, result.pack.version),
                        target: result.target.base_url.clone(),
                        machine: result.environment.line(),
                        started_at: result.started_at.clone(),
                        repetitions: u32::try_from(result.repetitions.len()).unwrap_or(u32::MAX),
                        submittable: result.submittable,
                        unmet: result
                            .submittable_unmet
                            .iter()
                            .map(|r| r.as_str().to_owned())
                            .collect(),
                        posture_profile: result.posture.profile.clone(),
                    });
                }
                Err(reason) => unreadable.push(reason),
            }
        }
        records.sort_by(|a, b| b.started_at.cmp(&a.started_at).then(a.file.cmp(&b.file)));
        BenchListing {
            out: state.out.display().to_string(),
            boundary_statements: statements,
            records,
            unreadable,
        }
    }

    /// The screen one address asks for: the listing, or one record in full.
    #[must_use]
    pub fn screen(state: &ConsoleState, key: Option<&str>) -> BenchScreen {
        let Some(key) = key.filter(|key| !key.is_empty()) else {
            return BenchScreen::Listing(Box::new(listing(state)));
        };
        let Some(found) = super::scan::find(state, key) else {
            return BenchScreen::Unknown {
                reason: String::from(
                    "no record here carries that address. An uploaded record is transient and is swept on a timer; a mounted one may have been moved. Upload it again, or pick one from the list.",
                ),
            };
        };
        match parse(&found) {
            Ok(result) => BenchScreen::Record(Box::new(detail(&found, &result))),
            Err(reason) => BenchScreen::Unknown { reason },
        }
    }

    /// One record's posture block, item by item.
    fn posture_lines(items: &[PostureDisclosure]) -> Vec<PostureLine> {
        items
            .iter()
            .map(|line| PostureLine {
                item: line.item.as_str().to_owned(),
                declared: line.declared.clone(),
                assurance: line.assurance.as_str().to_owned(),
                verified: matches!(line.assurance, super::mirror::Assurance::Verified),
                evidence: line
                    .readings
                    .iter()
                    .map(|reading| format!("{}: {}", reading.bracket.as_str(), reading.evidence))
                    .collect(),
            })
            .collect()
    }

    /// The cross-repetition tables, one per phase, each labelled with the
    /// discipline that produced it.
    fn phase_tables(result: &BenchResult) -> Vec<PhaseTable> {
        result
            .cross
            .iter()
            .map(|(phase, cross)| PhaseTable {
                phase: phase.clone(),
                regime: cross.regime.as_str().to_owned(),
                rows: cross
                    .operations
                    .iter()
                    .map(|(operation, stat)| OperationRow {
                        operation: operation.clone(),
                        repetitions: stat.repetitions,
                        p50_us: stat.p50_us.median,
                        p90_us: stat.p90_us.median,
                        p99_us: stat.p99_us.median,
                        p999_us: stat.p999_us.median,
                        throughput_ops_s: stat.throughput_ops_s.median,
                        p99_iqr_us: stat.p99_us.iqr,
                    })
                    .collect(),
            })
            .collect()
    }

    /// Every closed-loop sweep the repetitions carry, in execution order.
    fn sweep_rows(repetitions: &[RepetitionRecord]) -> Vec<SweepRow> {
        repetitions
            .iter()
            .flat_map(|repetition| {
                repetition.sweeps.values().map(|sweep| SweepRow {
                    name: sweep.name.clone(),
                    regime: sweep.regime.as_str().to_owned(),
                    repetition: repetition.repetition,
                    requests: sweep.requests,
                    compositions: sweep.compositions,
                    workers: sweep.workers,
                    elapsed_s: sweep.elapsed_s,
                    us_per_request: sweep.whole_loop_us_per_request,
                })
            })
            .collect()
    }

    /// The relative index as one table per reference, gaps stated rather than
    /// left as absent rows.
    fn relative_tables(indices: &[RelativeIndex]) -> Vec<RelativeTable> {
        indices
            .iter()
            .map(|index| RelativeTable {
                baseline: index.baseline.clone(),
                display_name: index.display_name.clone(),
                derivation: index.derivation.clone(),
                rows: relative_rows(index),
                gaps: index
                    .gaps
                    .iter()
                    .map(|gap| {
                        let metric = gap
                            .metric
                            .clone()
                            .unwrap_or_else(|| String::from("every metric"));
                        format!(
                            "phase `{}` operation `{}`, {metric}: {}",
                            gap.phase,
                            gap.operation,
                            gap.reason.as_str()
                        )
                    })
                    .collect(),
            })
            .collect()
    }

    /// Every ratio one index carries, in record order.
    fn relative_rows(index: &RelativeIndex) -> Vec<RelativeRow> {
        let mut rows = Vec::new();
        for (phase, block) in &index.phases {
            for (operation, ratios) in &block.operations {
                for metric in super::mirror::Metric::ALL {
                    let Some(ratio) = ratios.metrics.get(metric.as_str()) else {
                        continue;
                    };
                    rows.push(RelativeRow {
                        phase: phase.clone(),
                        regime: block.regime.as_str().to_owned(),
                        operation: operation.clone(),
                        metric: metric.as_str().to_owned(),
                        target_median: ratio.target_median,
                        baseline_median: ratio.baseline_median,
                        index: ratio.index,
                    });
                }
            }
        }
        rows
    }

    /// Each same-machine reference, as the record discloses it.
    fn baseline_cards(result: &BenchResult) -> Vec<BaselineCard> {
        result
            .baselines
            .iter()
            .map(|baseline| BaselineCard {
                cdr: baseline.cdr.clone(),
                display_name: baseline.display_name.clone(),
                base_url: baseline.base_url.clone(),
                sut_version: baseline.sut_version.clone(),
                recipe: format!(
                    "{} at {} file {}",
                    baseline.recipe.repository, baseline.recipe.git_ref, baseline.recipe.file
                ),
                resources: format!(
                    "server {} CPU / {} memory, database {} CPU / {} memory / {} shared memory",
                    baseline.resources.server_cpus,
                    baseline.resources.server_memory,
                    baseline.resources.database_cpus,
                    baseline.resources.database_memory,
                    baseline.resources.database_shm_size
                ),
                images: baseline
                    .images
                    .iter()
                    .map(|(role, image)| (role.clone(), image.clone()))
                    .collect(),
                posture_profile: baseline.posture.profile.clone(),
            })
            .collect()
    }

    /// One record in full, every figure carrying the discipline that produced
    /// it and every claim read out of the document as written.
    fn detail(found: &Found, result: &BenchResult) -> BenchDetail {
        let file = file_name(found);
        BenchDetail {
            key: found.key.clone(),
            label: result.label.clone().unwrap_or_else(|| file.clone()),
            file,
            source: found.source,
            boundary_statement: result.boundary_statement.clone(),
            methodology_statement: result.methodology.statement.clone(),
            pack: format!("{}@{}", result.pack.id, result.pack.version),
            pack_description: result.pack.description.clone(),
            seed: format!("{:#018x}", result.pack.seed),
            max_failed_share: result.pack.max_failed_share,
            target: result.target.base_url.clone(),
            sut_version: result.target.sut_version.clone(),
            machine: result.environment.line(),
            started_at: result.started_at.clone(),
            finished_at: result.finished_at.clone(),
            scale_factor: result.scale.factor,
            declared_workers: result.scale.declared_workers,
            reference_configuration: result.scale.reference_configuration,
            version_at_time: result.version_at_time.clone(),
            repetitions: u32::try_from(result.repetitions.len()).unwrap_or(u32::MAX),
            submittable: result.submittable,
            unmet: result
                .submittable_unmet
                .iter()
                .map(|r| (r.as_str().to_owned(), r.statement().to_owned()))
                .collect(),
            posture_profile: result.posture.profile.clone(),
            posture_summary: result.posture.summary.clone(),
            posture: posture_lines(&result.posture.items),
            comparability: result
                .posture
                .comparability
                .iter()
                .map(|divergence| {
                    format!(
                        "profile `{}` declares `{}` for `{}`, and the deployment measured here configures `{}` ({})",
                        result.posture.profile,
                        divergence.profile_declares,
                        divergence.item.as_str(),
                        divergence.deployment_configures,
                        divergence.source
                    )
                })
                .collect(),
            seed_phases: result
                .seed_phases
                .iter()
                .map(|seed| SeedPhaseRow {
                    name: seed.name.clone(),
                    regime: seed.regime.as_str().to_owned(),
                    ehrs: seed.ehrs,
                    compositions_per_ehr: seed.compositions_per_ehr,
                    workers: seed.workers,
                    elapsed_s: seed.elapsed_s,
                    writes_per_s: seed.bulk_load_writes_per_s,
                    ms_per_composition: seed.whole_loop_ms_per_composition,
                })
                .collect(),
            sweeps: sweep_rows(&result.repetitions),
            phases: phase_tables(result),
            failed_shares: super::derive::failed_shares(result),
            baselines: baseline_cards(result),
            relative: relative_tables(&result.relative),
        }
    }

    /// The comparison over the selected addresses.
    ///
    /// Every refusal is an ANSWER the surface renders, so a stale link says
    /// what went missing rather than failing the read.
    #[must_use]
    pub fn compare(state: &ConsoleState, selection: &str) -> CompareScreen {
        let keys: Vec<&str> = selection
            .split(',')
            .map(str::trim)
            .filter(|key| !key.is_empty())
            .collect();
        match keys.len() {
            0 => return CompareScreen::Idle,
            1 => return CompareScreen::NeedsMore { selected: 1 },
            _ => {}
        }
        if keys.len() > MAX_COLUMNS {
            return CompareScreen::Unknown {
                reason: format!(
                    "{} records are selected; the comparison aligns at most {MAX_COLUMNS} columns",
                    keys.len()
                ),
            };
        }
        let mut columns = Vec::with_capacity(keys.len());
        let mut results = Vec::with_capacity(keys.len());
        let mut statements: Vec<String> = Vec::new();
        for key in keys {
            let Some(found) = super::scan::find(state, key) else {
                return CompareScreen::Unknown {
                    reason: String::from(
                        "one of the selected records is no longer here: an uploaded record is transient and is swept on a timer. Upload it again, or drop it from the selection.",
                    ),
                };
            };
            let result = match parse(&found) {
                Ok(result) => result,
                Err(reason) => return CompareScreen::Unknown { reason },
            };
            if !statements.contains(&result.boundary_statement) {
                statements.push(result.boundary_statement.clone());
            }
            columns.push(column_of(&found, &result));
            results.push(result);
        }
        CompareScreen::Aligned(Box::new(BenchComparison {
            boundary_statements: statements,
            warnings: super::derive::warnings(&columns),
            rows: super::derive::align(&results),
            columns,
        }))
    }

    /// One column: everything about the record that is not a number in the
    /// body.
    fn column_of(found: &Found, result: &BenchResult) -> ComparisonColumn {
        let file = file_name(found);
        ComparisonColumn {
            key: found.key.clone(),
            label: result.label.clone().unwrap_or(file),
            pack: format!("{}@{}", result.pack.id, result.pack.version),
            sut_version: result.target.sut_version.clone(),
            machine: result.environment.line(),
            repetitions: u32::try_from(result.repetitions.len()).unwrap_or(u32::MAX),
            submittable: result.submittable,
            unmet: result
                .submittable_unmet
                .iter()
                .map(|r| r.as_str().to_owned())
                .collect(),
            max_failed_share: result.pack.max_failed_share,
            worst_failed_share: super::derive::worst_failed_share(result),
            scale_factor: result.scale.factor,
            reference_configuration: result.scale.reference_configuration,
            posture_profile: result.posture.profile.clone(),
            posture_signature: super::derive::signature(&result.posture),
            relative: relative_tables(&result.relative),
        }
    }
}

#[cfg(feature = "ssr")]
pub mod route {
    //! The server-owned upload route.
    //!
    //! A plain `<form method="post" enctype="multipart/form-data">` posts
    //! here and is answered with a redirect back to the page: a file upload
    //! with zero JavaScript, working before the WASM bundle has loaded and
    //! with it disabled entirely.

    use crate::redirect::{percent_encode, see_other};

    /// Accepts one batch of bench records and redirects back to the listing.
    ///
    /// Every refusal is a redirect too, carrying its reason in the query, so
    /// the answer is always the page rather than a bare error body.
    pub async fn upload(
        axum::Extension(state): axum::Extension<crate::state::ConsoleState>,
        mut form: axum::extract::Multipart,
    ) -> axum::response::Response {
        let mut files: Vec<(String, Vec<u8>)> = Vec::new();
        loop {
            match form.next_field().await {
                Ok(Some(field)) => {
                    if field.name() != Some("records") {
                        continue;
                    }
                    let name = field.file_name().unwrap_or("record.json").to_owned();
                    match field.bytes().await {
                        Ok(bytes) if bytes.is_empty() => {}
                        Ok(bytes) => files.push((name, bytes.to_vec())),
                        Err(e) => return refused(&format!("the upload did not arrive whole: {e}")),
                    }
                }
                Ok(None) => break,
                Err(e) => return refused(&format!("the upload could not be read: {e}")),
            }
        }
        match crate::bench_api::upload::batch(&state, &files) {
            Ok(count) => see_other(&format!("/benchmarks?uploaded={count}")),
            Err(reason) => refused(&reason),
        }
    }

    /// Redirects back to the page carrying a refusal reason.
    fn refused(reason: &str) -> axum::response::Response {
        see_other(&format!("/benchmarks?refused={}", percent_encode(reason)))
    }
}

pub mod fns {
    //! The `#[server]` endpoints, one module for one inner suppression.
    #![allow(
        clippy::unused_async,
        missing_docs,
        reason = "fires only in some #[server] expansions; see catalogue_api::fns"
    )]

    use leptos::prelude::{ServerFnError, server};

    use super::{BenchScreen, CompareScreen};

    /// The listing, or one record in full.
    ///
    /// The address is user input and is treated as such: it selects among the
    /// records the scan already found, and never reaches a path join.
    ///
    /// # Errors
    /// Never on its own account — every refusal travels as a screen variant.
    #[server]
    pub async fn fetch_bench_screen(record: Option<String>) -> Result<BenchScreen, ServerFnError> {
        let state: crate::state::ConsoleState = leptos::prelude::expect_context();
        Ok(super::read::screen(&state, record.as_deref()))
    }

    /// The comparison over the selected addresses, comma-separated.
    ///
    /// # Errors
    /// Never on its own account — every refusal travels as a screen variant.
    #[server]
    pub async fn fetch_bench_comparison(
        selection: Option<String>,
    ) -> Result<CompareScreen, ServerFnError> {
        let state: crate::state::ConsoleState = leptos::prelude::expect_context();
        Ok(super::read::compare(
            &state,
            selection.as_deref().unwrap_or_default(),
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::{ms, ops, ratio, us};

    /// A latency reads in the CLI's microseconds and in the milliseconds a
    /// reader compares by, from the one recorded number.
    #[test]
    fn a_latency_reads_in_both_units() {
        assert_eq!(us(1234.4), "1234");
        assert_eq!(ms(1234.4), "1.234");
        assert_eq!(us(0.0), "0");
        assert_eq!(ms(0.0), "0.000");
        assert_eq!(ms(19_007.0), "19.007");
    }

    /// Throughput and ratios keep the precision the CLI prints, so a figure
    /// read here and a figure read there are the same figure.
    #[test]
    fn rates_and_ratios_keep_the_command_lines_precision() {
        assert_eq!(ops(123.456), "123.5");
        assert_eq!(ratio(0.4), "0.400");
        assert_eq!(ratio(1.0), "1.000");
    }
}

#[cfg(all(test, feature = "ssr"))]
mod ssr_tests {
    use super::scan::{is_result_name, key_of};
    use super::upload::safe_name;

    /// The scan recognizes exactly the names the engine writes a record under.
    #[test]
    fn only_a_bench_result_file_is_listed() {
        assert!(is_result_name("bench-result.json"));
        assert!(is_result_name("bench-result-ehrbase-2-x.json"));
        assert!(!is_result_name("results.json"));
        assert!(!is_result_name("bench-result.json.bak"));
        assert!(!is_result_name("transcript.json"));
    }

    /// An uploaded name is REBUILT, so nothing the uploader typed reaches a
    /// path join, and the result is always a name the scan then finds.
    #[test]
    fn an_uploaded_name_is_rebuilt_from_the_alphabet() {
        assert_eq!(
            safe_name("bench-result-alpha.json", 0).as_deref(),
            Ok("bench-result-0-alpha.json"),
            "a record the engine wrote keeps ONE copy of the prefix"
        );
        assert_eq!(
            safe_name("bench-result.json", 3).as_deref(),
            Ok("bench-result-3-record.json"),
            "the bare stem still yields a name the scan finds"
        );
        assert_eq!(
            safe_name("../../etc/passwd.json", 1).as_deref(),
            Ok("bench-result-1-etcpasswd.json")
        );
        assert!(safe_name("record.txt", 0).is_err());
        assert!(safe_name(".json", 0).is_err());
        assert!(safe_name("", 0).is_err());
    }

    /// An address is derived from the path, so two files never share one and
    /// nothing a reader types reaches the filesystem.
    #[test]
    fn an_address_is_a_digest_of_its_path() {
        let one = key_of(std::path::Path::new("/out/bench-result.json"));
        let two = key_of(std::path::Path::new("/out/other/bench-result.json"));
        assert_eq!(one.len(), 16);
        assert_ne!(one, two);
        assert!(one.chars().all(|c| c.is_ascii_hexdigit()));
        assert_eq!(one, key_of(std::path::Path::new("/out/bench-result.json")));
    }
}
