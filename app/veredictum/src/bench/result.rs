// SPDX-FileCopyrightText: Veredictum contributors
// SPDX-License-Identifier: Apache-2.0

//! The bench result artifact: one JSON document per run.
//!
//! Every map is a [`BTreeMap`], so the document's field order is fixed by the
//! keys rather than by insertion, and re-serializing the same values yields
//! the same bytes. Latency lives in the document twice: as the interpolated
//! percentiles a reader wants, and as the standard `HdrHistogram` V2 encoding
//! a reader can recompute every one of them from.

#![expect(
    clippy::disallowed_types,
    reason = "the one untyped carrier here is the reserved `posture` extension point, whose shape a posture profile declares; typing it now would pin a contract that is not settled"
)]

use std::collections::BTreeMap;
use std::fmt;

use hdrhistogram::Histogram;
use serde::{Deserialize, Serialize};

use crate::bench::BenchError;
use crate::bench::fingerprint::EnvironmentFingerprint;
use crate::bench::pack::BenchPack;

/// The file stem a bench run writes its result under.
pub const RESULT_FILE_STEM: &str = "bench-result";

/// How many repetitions a result must carry before it may be submitted for
/// comparison. One repetition measures a moment, not a system.
pub const SUBMITTABLE_REPETITIONS: usize = 3;

/// Which load regime produced a phase's numbers.
///
/// A closed-loop phase's throughput is bounded by its own worker pool, so it
/// is never presented as a latency measurement; an open-loop phase's arrivals
/// fire on schedule regardless of completion.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum LoopRegime {
    /// Arrivals wait for a worker, so offered load is an outcome.
    ClosedLoop,
    /// Arrivals fire at their planned instants, so offered load is an input.
    OpenLoop,
}

impl LoopRegime {
    /// Every regime, in the order the schema enumerates them.
    pub const ALL: &[LoopRegime] = &[LoopRegime::ClosedLoop, LoopRegime::OpenLoop];

    /// The serialized token.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            LoopRegime::ClosedLoop => "closed-loop",
            LoopRegime::OpenLoop => "open-loop",
        }
    }
}

impl fmt::Display for LoopRegime {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// How one arrival failed. A closed vocabulary: an unrecognized failure is a
/// defect in this enum, never a silently uncounted arrival.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum ErrorClass {
    /// A 2xx the operation does not accept, such as `200` where the binding
    /// requires `201`.
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

impl ErrorClass {
    /// Every class, in the order the schema enumerates them.
    pub const ALL: &[ErrorClass] = &[
        ErrorClass::Http2xx,
        ErrorClass::Http3xx,
        ErrorClass::Http4xx,
        ErrorClass::Http5xx,
        ErrorClass::HttpOther,
        ErrorClass::Timeout,
        ErrorClass::Transport,
    ];

    /// The token the result records the class under.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            ErrorClass::Http2xx => "http_2xx",
            ErrorClass::Http3xx => "http_3xx",
            ErrorClass::Http4xx => "http_4xx",
            ErrorClass::Http5xx => "http_5xx",
            ErrorClass::HttpOther => "http_other",
            ErrorClass::Transport => "transport",
            ErrorClass::Timeout => "timeout",
        }
    }

    /// Reads one token from the closed vocabulary.
    ///
    /// # Errors
    /// [`BenchError::UnknownToken`] listing the accepted tokens.
    pub fn parse(token: &str) -> Result<Self, BenchError> {
        Self::ALL
            .iter()
            .copied()
            .find(|class| class.as_str() == token)
            .ok_or_else(|| BenchError::UnknownToken {
                vocabulary: "error class",
                token: token.to_owned(),
                accepted: Self::ALL
                    .iter()
                    .map(|class| class.as_str())
                    .collect::<Vec<_>>()
                    .join(", "),
            })
    }

    /// The class an observed status falls into.
    #[must_use]
    pub fn of_status(status: reqwest::StatusCode) -> Self {
        if status.is_success() {
            ErrorClass::Http2xx
        } else if status.is_redirection() {
            ErrorClass::Http3xx
        } else if status.is_client_error() {
            ErrorClass::Http4xx
        } else if status.is_server_error() {
            ErrorClass::Http5xx
        } else {
            ErrorClass::HttpOther
        }
    }
}

impl fmt::Display for ErrorClass {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// One operation's measured statistics within one repetition of one phase.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct OperationStats {
    /// Arrivals recorded in the measured span, failures included.
    pub count: u64,
    /// How many of those arrivals failed.
    pub errors: u64,
    /// The failures by class, keyed by the [`ErrorClass`] token.
    pub errors_by_class: BTreeMap<String, u64>,
    /// Recorded arrivals divided by the measured span, in operations per
    /// second.
    pub throughput_ops_s: f64,
    /// Median latency from the planned arrival instant, microseconds.
    pub p50_us: u64,
    /// 75th-percentile latency, microseconds.
    pub p75_us: u64,
    /// 90th-percentile latency, microseconds.
    pub p90_us: u64,
    /// 99th-percentile latency, microseconds.
    pub p99_us: u64,
    /// 99.9th-percentile latency, microseconds.
    pub p999_us: u64,
    /// The largest recorded latency, microseconds.
    pub max_us: u64,
    /// The standard `HdrHistogram` V2 encoding, base64, values in
    /// microseconds — every percentile above is recomputable from it.
    pub hdr_v2_base64: String,
}

impl OperationStats {
    /// Builds the record from a recorded histogram and its failure counts.
    ///
    /// # Errors
    /// [`BenchError::Histogram`] when the histogram cannot be encoded.
    pub fn from_histogram(
        histogram: &Histogram<u64>,
        errors_by_class: BTreeMap<String, u64>,
        measured_span_s: f64,
    ) -> Result<Self, BenchError> {
        let count = histogram.len();
        let errors = errors_by_class
            .values()
            .fold(0_u64, |a, b| a.saturating_add(*b));
        #[expect(
            clippy::as_conversions,
            clippy::cast_precision_loss,
            reason = "arrival counts are far below 2^52, so the throughput divisor loses nothing"
        )]
        let throughput_ops_s = if measured_span_s > 0.0 {
            count as f64 / measured_span_s
        } else {
            0.0
        };
        Ok(Self {
            count,
            errors,
            errors_by_class,
            throughput_ops_s,
            p50_us: histogram.value_at_quantile(0.50),
            p75_us: histogram.value_at_quantile(0.75),
            p90_us: histogram.value_at_quantile(0.90),
            p99_us: histogram.value_at_quantile(0.99),
            p999_us: histogram.value_at_quantile(0.999),
            max_us: histogram.max(),
            hdr_v2_base64: crate::perf::encode_hdr_v2(histogram).map_err(BenchError::Histogram)?,
        })
    }

    /// Decodes the embedded histogram, so any consumer recomputes every
    /// percentile from the artifact alone.
    ///
    /// # Errors
    /// [`BenchError::Histogram`] on a corrupt encoding.
    pub fn decode_histogram(&self) -> Result<Histogram<u64>, BenchError> {
        crate::perf::decode_hdr_v2(&self.hdr_v2_base64).map_err(BenchError::Histogram)
    }
}

/// One executed closed-loop bulk load.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SeedPhaseRecord {
    /// The phase name from the pack.
    pub name: String,
    /// Always [`LoopRegime::ClosedLoop`], stated rather than implied.
    pub regime: LoopRegime,
    /// EHRs created.
    pub ehrs: u64,
    /// Compositions committed into each EHR.
    pub compositions_per_ehr: u64,
    /// The worker pool the bulk load ran on.
    pub workers: u64,
    /// Wall-clock seconds the bulk load took.
    pub elapsed_s: f64,
    /// Writes divided by elapsed seconds. A closed-loop throughput bounded
    /// by the worker pool, never a latency claim.
    pub bulk_load_writes_per_s: f64,
    /// The whole loop's elapsed milliseconds divided by the compositions it
    /// committed, EHR creates included. The average a single-client bulk-load
    /// harness reports, and a closed-loop figure like every other number in
    /// this record.
    pub whole_loop_ms_per_composition: f64,
}

/// One executed closed-loop sweep: a sequential walk over the whole seeded
/// population.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SweepPhaseRecord {
    /// The phase name from the pack.
    pub name: String,
    /// Always [`LoopRegime::ClosedLoop`], stated rather than implied.
    pub regime: LoopRegime,
    /// The worker pool the walk ran on. One worker is a sequential client.
    pub workers: u64,
    /// Compositions the walk visited.
    pub compositions: u64,
    /// Requests offered against each visited composition.
    pub requests_per_composition: u64,
    /// Requests the walk issued in total.
    pub requests: u64,
    /// Wall-clock seconds the walk took.
    pub elapsed_s: f64,
    /// The whole loop's elapsed microseconds divided by the requests it
    /// issued. A closed-loop average, never a percentile.
    pub whole_loop_us_per_request: f64,
    /// Per-operation statistics, keyed by the operation token. Latency here
    /// is the request's own duration, which is what a closed-loop client
    /// observes.
    pub operations: BTreeMap<String, OperationStats>,
}

/// How far the run departed from the pack's pinned configuration.
///
/// A pack's reference figures describe the configuration the pack declares.
/// A run that scales the population down, or that moves a seed phase off its
/// declared worker count, measures different work, so the record says so
/// rather than leaving a reader to notice.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ScaleRecord {
    /// The multiplier applied to every seed phase's EHR count.
    pub factor: f64,
    /// Whether every seed phase ran at the worker count its pack declares.
    pub declared_workers: bool,
    /// Whether the run matches the pack's pinned configuration in every
    /// respect the operator can change, which is the only configuration whose
    /// numbers may be read against the pack's reference figures.
    pub reference_configuration: bool,
}

impl ScaleRecord {
    /// Records a run's departure from the pack's pinned configuration.
    #[must_use]
    pub fn new(factor: f64, declared_workers: bool) -> Self {
        let reference_scale = (factor - 1.0).abs() < f64::EPSILON;
        Self {
            factor,
            declared_workers,
            reference_configuration: reference_scale && declared_workers,
        }
    }
}

impl Default for ScaleRecord {
    fn default() -> Self {
        Self {
            factor: 1.0,
            declared_workers: true,
            reference_configuration: true,
        }
    }
}

/// One measured phase within one repetition.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MeasuredPhaseRecord {
    /// Always [`LoopRegime::OpenLoop`].
    pub regime: LoopRegime,
    /// The pack's pinned aggregate arrival rate.
    pub rate_per_s: f64,
    /// The warmup span, whose arrivals are dispatched and then discarded.
    pub warmup_s: u64,
    /// The measured span.
    pub duration_s: u64,
    /// Arrivals the schedule planned inside the measured span.
    pub planned_measured_arrivals: u64,
    /// Arrivals the dispatcher actually fired inside the measured span.
    pub dispatched_measured_arrivals: u64,
    /// Arrivals dispatched during warmup and excluded from every statistic
    /// in this record.
    pub warmup_arrivals: u64,
    /// Measured arrivals divided by the actual measured span.
    pub offered_load_sustained_per_s: f64,
    /// Whether the GENERATOR, rather than the SUT, was the bottleneck:
    /// dispatch lagged more than 2% past the planned span.
    pub generator_bound: bool,
    /// Per-operation statistics, keyed by the operation token.
    pub operations: BTreeMap<String, OperationStats>,
}

/// One repetition of every measured phase in the pack.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RepetitionRecord {
    /// The one-based repetition ordinal.
    pub repetition: u32,
    /// The open-loop measured phases, keyed by phase name.
    pub phases: BTreeMap<String, MeasuredPhaseRecord>,
    /// The closed-loop sweeps, keyed by phase name. Absent from a pack that
    /// declares none.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub sweeps: BTreeMap<String, SweepPhaseRecord>,
}

/// A median and an inter-quartile range over one value across repetitions.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CrossStat {
    /// The median of the per-repetition values.
    pub median: f64,
    /// The inter-quartile range of the per-repetition values.
    pub iqr: f64,
}

/// The cross-repetition summary for one operation of one phase.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
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
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CrossPhase {
    /// Which discipline produced this phase's numbers. Carried here so every
    /// rendered figure can be labelled without consulting the pack.
    pub regime: LoopRegime,
    /// Per-operation summaries, keyed by the operation token.
    pub operations: BTreeMap<String, CrossOperation>,
}

/// The pack a run drove, as the result records it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PackRecord {
    /// The pack id.
    pub id: String,
    /// The pack version. Two results are comparable only when it matches.
    pub version: String,
    /// What the pack exercises.
    pub description: String,
    /// The seed every arrival stream drew from.
    pub seed: u64,
    /// The fixture pins, keyed by fixture key.
    pub fixtures: BTreeMap<String, String>,
}

impl PackRecord {
    /// Records the pack a run is about to drive.
    #[must_use]
    pub fn of(pack: &BenchPack) -> Self {
        Self {
            id: pack.id.as_str().to_owned(),
            version: pack.version.clone(),
            description: pack.description.clone(),
            seed: pack.seed,
            fixtures: pack.fixture_pins(),
        }
    }
}

/// The system the run was pointed at.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TargetRecord {
    /// The base URL, with any userinfo removed.
    pub base_url: String,
    /// The version the SUT disclosed about itself, when it disclosed one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sut_version: Option<String>,
}

/// The methodology block: what the numbers in this document mean.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Methodology {
    /// The prose statement, verbatim from [`crate::bench::METHODOLOGY`].
    pub statement: String,
    /// Measured phases offer load open-loop.
    pub open_loop: bool,
    /// Latency is measured from the planned arrival instant, so coordinated
    /// omission cannot hide a stall.
    pub coordinated_omission_free: bool,
    /// The seed phases ran once, before the first repetition.
    pub seed_once_measure_n: bool,
    /// How many repetitions the run executed.
    pub repetitions: u32,
}

/// One bench run's complete record.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BenchResult {
    /// The artifact-family schema version this document is written against.
    pub schema_version: String,
    /// What a bench result is and is not, verbatim from
    /// [`crate::bench::BOUNDARY_STATEMENT`].
    pub boundary_statement: String,
    /// The operator's label for this run, when one was given.
    #[serde(default, skip_serializing_if = "Option::is_none")]
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
    /// The instant every `version_at_time` read in this run addressed,
    /// captured once after the seed phases finished, RFC 3339. Absent when
    /// the pack drives no such read.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub version_at_time: Option<String>,
    /// The executed bulk loads, in execution order.
    pub seed_phases: Vec<SeedPhaseRecord>,
    /// Every repetition, in execution order.
    pub repetitions: Vec<RepetitionRecord>,
    /// The cross-repetition summary, keyed by phase name.
    pub cross: BTreeMap<String, CrossPhase>,
    /// What the numbers mean.
    pub methodology: Methodology,
    /// Whether the run carries enough repetitions to be offered for
    /// comparison ([`SUBMITTABLE_REPETITIONS`] or more).
    pub submittable: bool,
    /// Reserved for the posture profile a run declares. Always absent here.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub posture: Option<serde_json::Value>,
}

impl BenchResult {
    /// Whether the run carries enough repetitions to be submittable.
    #[must_use]
    pub fn is_submittable(&self) -> bool {
        self.repetitions.len() >= SUBMITTABLE_REPETITIONS
    }

    /// The document's canonical text: two-space pretty print, trailing
    /// newline, exactly as every other emitted artifact family.
    ///
    /// # Errors
    /// [`BenchError::Serialize`] when the value cannot be serialized.
    pub fn to_document(&self) -> Result<String, BenchError> {
        let mut text =
            serde_json::to_string_pretty(self).map_err(|source| BenchError::Serialize {
                context: "bench result",
                source,
            })?;
        text.push('\n');
        Ok(text)
    }

    /// The file name this result is written under, distinguished by its
    /// label so two runs can share an output directory.
    #[must_use]
    pub fn file_name(&self) -> String {
        match self.label.as_deref().map(slug) {
            Some(slug) if !slug.is_empty() => format!("{RESULT_FILE_STEM}-{slug}.json"),
            _ => format!("{RESULT_FILE_STEM}.json"),
        }
    }
}

/// A file-name-safe rendering of an operator label.
fn slug(label: &str) -> String {
    label
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() {
                c.to_ascii_lowercase()
            } else {
                '-'
            }
        })
        .collect::<String>()
        .trim_matches('-')
        .to_owned()
}

#[cfg(test)]
#[expect(
    clippy::panic_in_result_fn,
    reason = "three Result-returning tests in the Book ch11 shape, each asserting; \
              clippy offers no allow-in-tests knob for this lint"
)]
mod tests {
    use super::*;

    /// Status classing follows the wire ranges, never a numeric literal
    /// comparison that a one-character typo would silently change.
    #[test]
    fn statuses_class_by_their_wire_range() {
        assert_eq!(
            ErrorClass::of_status(reqwest::StatusCode::CREATED),
            ErrorClass::Http2xx
        );
        assert_eq!(
            ErrorClass::of_status(reqwest::StatusCode::FOUND),
            ErrorClass::Http3xx
        );
        assert_eq!(
            ErrorClass::of_status(reqwest::StatusCode::NOT_FOUND),
            ErrorClass::Http4xx
        );
        assert_eq!(
            ErrorClass::of_status(reqwest::StatusCode::INTERNAL_SERVER_ERROR),
            ErrorClass::Http5xx
        );
        assert_eq!(
            ErrorClass::of_status(reqwest::StatusCode::CONTINUE),
            ErrorClass::HttpOther
        );
    }

    /// Every error-class token round-trips, and an unknown one is refused.
    #[test]
    fn every_error_class_token_round_trips() -> Result<(), BenchError> {
        for class in ErrorClass::ALL {
            assert_eq!(ErrorClass::parse(class.as_str())?, *class);
        }
        assert!(ErrorClass::parse("http_6xx").is_err());
        Ok(())
    }

    /// Percentiles and the encoded histogram describe the same values, so a
    /// consumer that recomputes from the artifact gets what is printed.
    #[test]
    fn the_encoded_histogram_reproduces_the_recorded_percentiles()
    -> Result<(), Box<dyn std::error::Error>> {
        let mut histogram = Histogram::<u64>::new_with_bounds(1, 600_000_000, 3)?;
        for value in 1..=1000_u64 {
            histogram.record(value * 100)?;
        }
        let stats = OperationStats::from_histogram(&histogram, BTreeMap::new(), 10.0)?;
        let decoded = stats.decode_histogram()?;
        assert_eq!(decoded.value_at_quantile(0.50), stats.p50_us);
        assert_eq!(decoded.value_at_quantile(0.99), stats.p99_us);
        assert_eq!(decoded.value_at_quantile(0.999), stats.p999_us);
        assert_eq!(decoded.max(), stats.max_us);
        assert_eq!(stats.count, 1000);
        assert!((stats.throughput_ops_s - 100.0).abs() < 1e-9);
        Ok(())
    }

    /// The failure counts sum into the total, so no class is uncounted.
    #[test]
    fn errors_sum_from_their_classes() -> Result<(), Box<dyn std::error::Error>> {
        let mut histogram = Histogram::<u64>::new_with_bounds(1, 600_000_000, 3)?;
        histogram.record(10)?;
        let mut classes = BTreeMap::new();
        let _replaced = classes.insert(ErrorClass::Http5xx.as_str().to_owned(), 3_u64);
        let _replaced = classes.insert(ErrorClass::Timeout.as_str().to_owned(), 1_u64);
        let stats = OperationStats::from_histogram(&histogram, classes, 1.0)?;
        assert_eq!(stats.errors, 4);
        Ok(())
    }

    /// A run at the pinned scale on the declared workers is the reference
    /// configuration; either departure clears the flag.
    #[test]
    fn only_the_pinned_configuration_is_the_reference_one() {
        assert!(ScaleRecord::new(1.0, true).reference_configuration);
        assert!(!ScaleRecord::new(0.1, true).reference_configuration);
        assert!(!ScaleRecord::new(1.0, false).reference_configuration);
        assert!(!ScaleRecord::new(0.1, false).reference_configuration);
        assert_eq!(ScaleRecord::default(), ScaleRecord::new(1.0, true));
    }

    /// A label becomes a file-name-safe slug; no label keeps the bare stem.
    #[test]
    fn the_file_name_follows_the_label() {
        assert_eq!(slug("EHRbase 2.x / run A"), "ehrbase-2-x---run-a");
        assert_eq!(slug("---"), "");
    }
}
