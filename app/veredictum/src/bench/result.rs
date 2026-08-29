// SPDX-FileCopyrightText: Veredictum contributors
// SPDX-License-Identifier: Apache-2.0

//! The bench result artifact: one JSON document per run.
//!
//! Every map is a [`BTreeMap`], so the document's field order is fixed by the
//! keys rather than by insertion, and re-serializing the same values yields
//! the same bytes. Latency lives in the document twice: as the interpolated
//! percentiles a reader wants, and as the standard `HdrHistogram` V2 encoding
//! a reader can recompute every one of them from.

use std::collections::BTreeMap;
use std::fmt;

use hdrhistogram::Histogram;
use serde::{Deserialize, Serialize};

use crate::bench::BenchError;
use crate::bench::fingerprint::EnvironmentFingerprint;
use crate::bench::pack::BenchPack;
use crate::bench::posture::PostureRecord;
use crate::bench::relative::RelativeIndex;

/// The file stem a bench run writes its result under.
pub const RESULT_FILE_STEM: &str = "bench-result";

/// How many repetitions a result must carry before it may be submitted for
/// comparison. One repetition measures a moment, not a system.
pub const SUBMITTABLE_REPETITIONS: usize = 3;

/// How many same-machine baselines a result must carry before it may be
/// submitted. Without one, an absolute millisecond describes the machine as
/// much as the system.
pub const SUBMITTABLE_BASELINES: usize = 1;

/// What a record must carry before it may be offered for ranking.
///
/// A closed vocabulary, so a non-submittable record says WHICH requirement it
/// misses rather than only that it misses one.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum SubmissionRequirement {
    /// At least [`SUBMITTABLE_REPETITIONS`] repetitions.
    Repetitions,
    /// At least [`SUBMITTABLE_BASELINES`] same-machine baseline block.
    Baseline,
    /// Every repetition, phase and operation, on the target and on every
    /// baseline, kept its failed-arrival share at or below the ceiling its
    /// pack pins.
    ErrorShare,
}

impl SubmissionRequirement {
    /// Every requirement, in the order the schema enumerates them.
    pub const ALL: &[SubmissionRequirement] = &[
        SubmissionRequirement::Repetitions,
        SubmissionRequirement::Baseline,
        SubmissionRequirement::ErrorShare,
    ];

    /// The token the record names the requirement by.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            SubmissionRequirement::Repetitions => "repetitions",
            SubmissionRequirement::Baseline => "baseline",
            SubmissionRequirement::ErrorShare => "error_share",
        }
    }

    /// What the requirement asks for, as one sentence a rendered summary
    /// prints beside the refusal.
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

    /// Reads one token from the closed vocabulary.
    ///
    /// # Errors
    /// [`BenchError::UnknownToken`] listing the accepted tokens.
    pub fn parse(token: &str) -> Result<Self, BenchError> {
        Self::ALL
            .iter()
            .copied()
            .find(|requirement| requirement.as_str() == token)
            .ok_or_else(|| BenchError::UnknownToken {
                vocabulary: "submission requirement",
                token: token.to_owned(),
                accepted: Self::ALL
                    .iter()
                    .map(|requirement| requirement.as_str())
                    .collect::<Vec<_>>()
                    .join(", "),
            })
    }
}

impl fmt::Display for SubmissionRequirement {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl Serialize for SubmissionRequirement {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(self.as_str())
    }
}

impl<'de> Deserialize<'de> for SubmissionRequirement {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let token = String::deserialize(deserializer)?;
        SubmissionRequirement::parse(&token).map_err(serde::de::Error::custom)
    }
}

/// The upstream deployment recipe a baseline's topology follows.
///
/// The reference is an immutable tag rather than a branch, so the topology a
/// reader fetches is the one the baseline ran.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RecipeReference {
    /// The repository the recipe lives in.
    pub repository: String,
    /// The immutable tag the recipe is read at.
    pub git_ref: String,
    /// The recipe file within that repository.
    pub file: String,
}

/// The container ceilings a baseline was composed under.
///
/// Every baseline in a record takes the same ceilings; a baseline handed more
/// CPU than its sibling measures the ceiling rather than the CDR.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BaselineResources {
    /// The server container's CPU limit, as compose states it.
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

/// One reference CDR measured on the same host, in the same session, by the
/// same pack at the same seed.
///
/// The measured half is exactly the target's: the same seed phases, the same
/// repetitions, the same cross-repetition summary, so the two sides of a
/// ratio are the same statistic.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BaselineRecord {
    /// The reference CDR token.
    pub cdr: String,
    /// That reference's human-readable name.
    pub display_name: String,
    /// The digest-pinned images composed, keyed by role.
    pub images: BTreeMap<String, String>,
    /// The upstream deployment recipe the topology follows.
    pub recipe: RecipeReference,
    /// The container ceilings the stack was composed under.
    pub resources: BaselineResources,
    /// The base URL the pack was driven over.
    pub base_url: String,
    /// The version the baseline disclosed about itself, when it disclosed
    /// one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sut_version: Option<String>,
    /// When the baseline run started, RFC 3339.
    pub started_at: String,
    /// When it finished, RFC 3339.
    pub finished_at: String,
    /// The executed bulk loads, in execution order.
    pub seed_phases: Vec<SeedPhaseRecord>,
    /// Every repetition, in execution order.
    pub repetitions: Vec<RepetitionRecord>,
    /// The cross-repetition summary, keyed by phase name.
    pub cross: BTreeMap<String, CrossPhase>,
    /// The baseline's own posture block, taken under the profile the target
    /// declared and verified by the same canaries. A ratio between two
    /// different postures compares two different sports, so both sides carry
    /// their own block.
    pub posture: PostureRecord,
}

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
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PackRecord {
    /// The pack id.
    pub id: String,
    /// The pack version. Two results are comparable only when it matches.
    pub version: String,
    /// What the pack exercises.
    pub description: String,
    /// The failed-arrival ceiling the pack version pins, disclosed here so the
    /// submittability decision is a pure function of the record.
    pub max_failed_share: f64,
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
            max_failed_share: pack.max_failed_share,
            seed: pack.seed,
            fixtures: pack.fixture_pins(),
        }
    }
}

/// The share of one operation's recorded arrivals that failed.
///
/// An operation that recorded no arrival at all is fully failed rather than
/// perfect: nothing answered, so nothing was measured, and a zero divisor would
/// otherwise settle the question silently.
#[must_use]
pub fn failed_share(count: u64, errors: u64) -> f64 {
    if count == 0 {
        return 1.0;
    }
    #[expect(
        clippy::as_conversions,
        clippy::cast_precision_loss,
        reason = "arrival counts are far below 2^52, so neither side of the ratio loses a digit"
    )]
    let share = errors.min(count) as f64 / count as f64;
    share
}

/// Which measured side of a record one failed-arrival reading came from.
///
/// A baseline's numbers are the divisor of every index the board ranks by, so a
/// reading is never reported without saying which side produced it.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub enum MeasuredSide {
    /// The system the record is about.
    Target,
    /// One same-machine reference, by its CDR token.
    Baseline(String),
}

impl fmt::Display for MeasuredSide {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            MeasuredSide::Target => f.write_str("the target"),
            MeasuredSide::Baseline(cdr) => write!(f, "the {cdr} baseline"),
        }
    }
}

/// The failed-arrival reading of one phase, in one repetition, on one side.
///
/// The phase totals are what a summary prints; `worst_share` is what the
/// ceiling is applied to, because one contaminated operation inside an
/// otherwise healthy phase is exactly the case a phase average hides.
#[derive(Debug, Clone, PartialEq)]
pub struct FailedSharePhase {
    /// Which side the reading came from.
    pub side: MeasuredSide,
    /// The one-based repetition ordinal.
    pub repetition: u32,
    /// The phase name.
    pub phase: String,
    /// Which discipline produced the phase.
    pub regime: LoopRegime,
    /// Arrivals the phase recorded, failures included.
    pub count: u64,
    /// How many of them failed.
    pub errors: u64,
    /// The phase's own failed share, over every operation it recorded.
    pub share: f64,
    /// The operation with the largest failed share, and `None` for a phase
    /// that recorded no operation at all.
    pub worst_operation: Option<String>,
    /// That operation's failed share, and `1.0` for a phase that recorded no
    /// operation, which measured nothing.
    pub worst_share: f64,
}

impl FailedSharePhase {
    /// Whether this phase crossed `ceiling` in any one operation.
    #[must_use]
    pub fn breaches(&self, ceiling: f64) -> bool {
        self.worst_share > ceiling
    }

    /// The one sentence a refusal prints, naming where the ceiling went.
    #[must_use]
    pub fn sentence(&self, ceiling: f64) -> String {
        let where_it_went = match &self.worst_operation {
            Some(operation) => format!(
                "lost {:.3} of the arrivals it recorded for `{operation}`, above the pack ceiling of {ceiling:.2}",
                self.worst_share
            ),
            None => "recorded no operation at all, so it measured nothing".to_owned(),
        };
        format!(
            "on {}, repetition {} of phase `{}` ({}) {where_it_went}",
            self.side, self.repetition, self.phase, self.regime
        )
    }
}

/// The failed-arrival reading of every phase of one measured side.
fn side_readings(side: &MeasuredSide, repetitions: &[RepetitionRecord]) -> Vec<FailedSharePhase> {
    let mut readings = Vec::new();
    for repetition in repetitions {
        for (phase, measured) in &repetition.phases {
            readings.push(one_reading(
                side,
                repetition.repetition,
                phase,
                measured.regime,
                &measured.operations,
            ));
        }
        for (phase, sweep) in &repetition.sweeps {
            readings.push(one_reading(
                side,
                repetition.repetition,
                phase,
                sweep.regime,
                &sweep.operations,
            ));
        }
    }
    readings
}

/// One phase's reading, over the operations it recorded.
fn one_reading(
    side: &MeasuredSide,
    repetition: u32,
    phase: &str,
    regime: LoopRegime,
    operations: &BTreeMap<String, OperationStats>,
) -> FailedSharePhase {
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
    FailedSharePhase {
        side: side.clone(),
        repetition,
        phase: phase.to_owned(),
        regime,
        count,
        errors,
        share: failed_share(count, errors),
        worst_operation,
        worst_share,
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
    /// The same-machine reference runs, in the order they were composed.
    /// Empty for a run that measured only its target.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub baselines: Vec<BaselineRecord>,
    /// The target measured against every baseline, one block each.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub relative: Vec<RelativeIndex>,
    /// What the numbers mean.
    pub methodology: Methodology,
    /// Whether the run meets every submission requirement.
    pub submittable: bool,
    /// The requirements it does not meet, so a refusal names its reasons
    /// rather than leaving a reader to guess which one fired. Empty exactly
    /// when `submittable` is true.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub submittable_unmet: Vec<SubmissionRequirement>,
    /// The posture profile this run declared, its full disclosure, and the
    /// bracketing canary evidence behind each item's verified or declared-only
    /// label.
    pub posture: PostureRecord,
}

impl BenchResult {
    /// The failed-arrival reading of every phase this record carries, the
    /// target's first and then each baseline's, in recorded order.
    ///
    /// A pure function of the document, so a reader recomputes exactly what the
    /// engine decided by.
    #[must_use]
    pub fn failed_share_readings(&self) -> Vec<FailedSharePhase> {
        let mut readings = side_readings(&MeasuredSide::Target, &self.repetitions);
        for baseline in &self.baselines {
            readings.extend(side_readings(
                &MeasuredSide::Baseline(baseline.cdr.clone()),
                &baseline.repetitions,
            ));
        }
        readings
    }

    /// Every reading above the pack's ceiling, which is what makes a record
    /// unrankable and what a refusal names.
    #[must_use]
    pub fn failed_share_breaches(&self) -> Vec<FailedSharePhase> {
        let ceiling = self.pack.max_failed_share;
        self.failed_share_readings()
            .into_iter()
            .filter(|reading| reading.breaches(ceiling))
            .collect()
    }

    /// The largest failed share any one operation of any side recorded, and
    /// `0.0` for a record that measured no phase at all.
    #[must_use]
    pub fn worst_failed_share(&self) -> f64 {
        self.failed_share_readings()
            .iter()
            .map(|reading| reading.worst_share)
            .fold(0.0_f64, f64::max)
    }

    /// The submission requirements this run does not meet, in the order
    /// [`SubmissionRequirement::ALL`] lists them.
    #[must_use]
    pub fn unmet_requirements(&self) -> Vec<SubmissionRequirement> {
        SubmissionRequirement::ALL
            .iter()
            .copied()
            .filter(|requirement| match requirement {
                SubmissionRequirement::Repetitions => {
                    self.repetitions.len() < SUBMITTABLE_REPETITIONS
                }
                SubmissionRequirement::Baseline => self.baselines.len() < SUBMITTABLE_BASELINES,
                SubmissionRequirement::ErrorShare => !self.failed_share_breaches().is_empty(),
            })
            .collect()
    }

    /// Whether the run meets every submission requirement.
    #[must_use]
    pub fn is_submittable(&self) -> bool {
        self.unmet_requirements().is_empty()
    }

    /// Records the same-machine baselines, derives the relative index against
    /// each, and re-decides submittability.
    ///
    /// The relative index is derived here rather than measured, so it stays a
    /// pure function of the two cross-repetition summaries already in the
    /// document.
    pub fn attach_baselines(&mut self, baselines: Vec<BaselineRecord>) {
        self.relative = baselines
            .iter()
            .map(|baseline| crate::bench::relative::derive(&self.cross, baseline))
            .collect();
        self.baselines = baselines;
        self.settle_submittability();
    }

    /// Re-decides `submittable` and its unmet list from what the record now
    /// carries.
    pub fn settle_submittability(&mut self) {
        self.submittable_unmet = self.unmet_requirements();
        self.submittable = self.submittable_unmet.is_empty();
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
    use crate::bench::posture::{
        Assurance, Bracket, CanaryOutcome, CanaryReading, MINIMAL, PostureDisclosure, PostureItem,
    };

    /// A posture block whose every item is declared and none verified, which
    /// is the shape a record carries when no canary could observe anything.
    fn declared_only_posture() -> PostureRecord {
        let reading = |bracket: Bracket| CanaryReading {
            bracket,
            outcome: CanaryOutcome::NotObservable,
            observed: "(not observable)".to_owned(),
            evidence: "a record built for this test observes nothing".to_owned(),
        };
        PostureRecord {
            profile: MINIMAL.name.to_owned(),
            summary: MINIMAL.summary.to_owned(),
            items: PostureItem::ALL
                .iter()
                .copied()
                .map(|item| PostureDisclosure {
                    item,
                    declared: MINIMAL.declared(item).unwrap_or("none").to_owned(),
                    assurance: Assurance::DeclaredOnly,
                    readings: vec![reading(Bracket::Before), reading(Bracket::After)],
                })
                .collect(),
        }
    }

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

    /// Every submission-requirement token round-trips, and an unknown one is
    /// refused rather than read as a met requirement.
    #[test]
    fn every_submission_requirement_token_round_trips() -> Result<(), Box<dyn std::error::Error>> {
        for requirement in SubmissionRequirement::ALL {
            assert_eq!(
                SubmissionRequirement::parse(requirement.as_str())?,
                *requirement
            );
            let text = serde_json::to_string(requirement)?;
            let back: SubmissionRequirement = serde_json::from_str(&text)?;
            assert_eq!(back, *requirement);
            assert!(!requirement.statement().is_empty());
        }
        assert!(SubmissionRequirement::parse("vibes").is_err());
        Ok(())
    }

    /// A record with three repetitions and no baseline names the baseline
    /// requirement, and attaching one clears it.
    #[test]
    fn submittability_names_the_requirement_it_misses() {
        let mut result = minimal_result(3);
        result.settle_submittability();
        assert!(!result.submittable);
        assert_eq!(
            result.submittable_unmet,
            vec![SubmissionRequirement::Baseline]
        );
        result.attach_baselines(vec![empty_baseline()]);
        assert!(result.submittable, "{:?}", result.submittable_unmet);
        assert!(result.submittable_unmet.is_empty());
    }

    /// A single-repetition record with no baseline misses BOTH requirements,
    /// and says so rather than reporting one bare false.
    #[test]
    fn a_thin_record_names_every_requirement_it_misses() {
        let mut result = minimal_result(1);
        result.settle_submittability();
        assert_eq!(
            result.submittable_unmet,
            vec![
                SubmissionRequirement::Repetitions,
                SubmissionRequirement::Baseline
            ]
        );
        assert!(!result.is_submittable());
    }

    /// Attaching a baseline derives one relative-index block per baseline,
    /// which is what a reader compares across machines.
    #[test]
    fn attaching_a_baseline_derives_its_relative_index() {
        let mut result = minimal_result(3);
        result.attach_baselines(vec![empty_baseline()]);
        assert_eq!(result.relative.len(), 1);
        assert_eq!(
            result.relative.first().map(|index| index.baseline.as_str()),
            Some("ehrbase")
        );
    }

    /// A result whose baselines are absent serializes without the new keys,
    /// so a consumer of the previous shape still reads it.
    #[test]
    fn a_baseline_free_record_omits_the_new_keys() -> Result<(), Box<dyn std::error::Error>> {
        let mut result = minimal_result(3);
        result.settle_submittability();
        let text = result.to_document()?;
        assert!(!text.contains("\"baselines\""), "{text}");
        assert!(!text.contains("\"relative\""), "{text}");
        assert!(text.contains("\"submittable_unmet\""), "{text}");
        let back: BenchResult = serde_json::from_str(&text)?;
        assert_eq!(back, result);
        Ok(())
    }

    /// The ceiling every test record is judged by: the conservative default
    /// each embedded pack pins.
    const CEILING: f64 = crate::bench::pack::DEFAULT_MAX_FAILED_SHARE;

    /// One operation's recorded arrivals, with every failure in one class.
    ///
    /// The latency members carry constants, because what these tests read is
    /// the arrival arithmetic rather than any percentile.
    fn stats(count: u64, errors: u64) -> OperationStats {
        let mut errors_by_class = BTreeMap::new();
        if errors > 0 {
            let _replaced = errors_by_class.insert(ErrorClass::Http5xx.as_str().to_owned(), errors);
        }
        OperationStats {
            count,
            errors,
            errors_by_class,
            throughput_ops_s: 1.0,
            p50_us: 1,
            p75_us: 1,
            p90_us: 1,
            p99_us: 1,
            p999_us: 1,
            max_us: 1,
            hdr_v2_base64: String::new(),
        }
    }

    /// One measured phase carrying `operations`.
    fn measured(operations: BTreeMap<String, OperationStats>) -> MeasuredPhaseRecord {
        MeasuredPhaseRecord {
            regime: LoopRegime::OpenLoop,
            rate_per_s: 1.0,
            warmup_s: 0,
            duration_s: 1,
            planned_measured_arrivals: 0,
            dispatched_measured_arrivals: 0,
            warmup_arrivals: 0,
            offered_load_sustained_per_s: 0.0,
            generator_bound: false,
            operations,
        }
    }

    /// One phase named `mixed` recording one operation at `count` arrivals of
    /// which `errors` failed, repeated across every repetition.
    fn phases_of(count: u64, errors: u64) -> BTreeMap<String, MeasuredPhaseRecord> {
        let mut operations = BTreeMap::new();
        let _replaced = operations.insert("get_ehr".to_owned(), stats(count, errors));
        let mut phases = BTreeMap::new();
        let _replaced = phases.insert("mixed".to_owned(), measured(operations));
        phases
    }

    /// A three-repetition record with one clean baseline, whose target phase
    /// recorded `count` arrivals of which `errors` failed.
    fn record_of(count: u64, errors: u64) -> BenchResult {
        let mut result = minimal_result(3);
        for repetition in &mut result.repetitions {
            repetition.phases = phases_of(count, errors);
        }
        result.attach_baselines(vec![empty_baseline()]);
        result
    }

    /// An operation that recorded no arrival at all is fully failed, and the
    /// ratio never divides by zero.
    #[test]
    fn a_zero_arrival_operation_is_fully_failed() {
        assert!((failed_share(0, 0) - 1.0).abs() < f64::EPSILON);
        assert!((failed_share(0, 7) - 1.0).abs() < f64::EPSILON);
        assert!((failed_share(100, 1) - 0.01).abs() < f64::EPSILON);
        assert!(failed_share(100, 0).abs() < f64::EPSILON);
        assert!((failed_share(444, 444) - 1.0).abs() < f64::EPSILON);
    }

    /// A target exactly at the ceiling stays submittable, and one arrival more
    /// refuses the record by name.
    #[test]
    fn the_target_passes_at_the_ceiling_and_refuses_above_it() {
        let at = record_of(100, 1);
        assert!(at.submittable, "{:?}", at.submittable_unmet);
        assert!(at.failed_share_breaches().is_empty());
        assert!((at.worst_failed_share() - CEILING).abs() < f64::EPSILON);

        let above = record_of(100, 2);
        assert!(!above.submittable);
        assert_eq!(
            above.submittable_unmet,
            vec![SubmissionRequirement::ErrorShare]
        );
        let breaches = above.failed_share_breaches();
        assert_eq!(breaches.len(), 3, "one per repetition");
        let Some(first) = breaches.first() else {
            panic!("the breach list lost its entries");
        };
        assert_eq!(first.side, MeasuredSide::Target);
        let sentence = first.sentence(CEILING);
        assert!(sentence.contains("the target"), "{sentence}");
        assert!(sentence.contains("`mixed`"), "{sentence}");
        assert!(sentence.contains("`get_ehr`"), "{sentence}");
        assert!(sentence.contains("0.01"), "{sentence}");
    }

    /// A baseline above the ceiling refuses the record just as the target
    /// does: every index on a board divides by one of its medians.
    #[test]
    fn a_baseline_above_the_ceiling_refuses_the_record() {
        let mut result = minimal_result(3);
        for repetition in &mut result.repetitions {
            repetition.phases = phases_of(100, 0);
        }
        let mut baseline = empty_baseline();
        baseline.repetitions = vec![RepetitionRecord {
            repetition: 1,
            phases: phases_of(100, 1),
            sweeps: BTreeMap::new(),
        }];
        result.attach_baselines(vec![baseline.clone()]);
        assert!(result.submittable, "{:?}", result.submittable_unmet);

        baseline.repetitions = vec![RepetitionRecord {
            repetition: 1,
            phases: phases_of(100, 2),
            sweeps: BTreeMap::new(),
        }];
        result.attach_baselines(vec![baseline]);
        assert_eq!(
            result.submittable_unmet,
            vec![SubmissionRequirement::ErrorShare]
        );
        let breaches = result.failed_share_breaches();
        let Some(breach) = breaches.first() else {
            panic!("the baseline breach was not recorded");
        };
        assert_eq!(breach.side, MeasuredSide::Baseline("ehrbase".to_owned()));
        assert!(
            breach.sentence(CEILING).contains("the ehrbase baseline"),
            "{}",
            breach.sentence(CEILING)
        );
    }

    /// An operation that recorded arrivals and answered none of them refuses
    /// the record, which is the run this requirement exists for.
    #[test]
    fn a_wholly_failed_operation_refuses_the_record() {
        let result = record_of(444, 444);
        assert!(!result.submittable);
        assert!(
            result
                .submittable_unmet
                .contains(&SubmissionRequirement::ErrorShare)
        );
        assert!((result.worst_failed_share() - 1.0).abs() < f64::EPSILON);
    }

    /// A record carrying nothing but the fields the submittability decision
    /// reads, at the requested repetition count.
    fn minimal_result(repetitions: usize) -> BenchResult {
        BenchResult {
            schema_version: "0".to_owned(),
            boundary_statement: crate::bench::BOUNDARY_STATEMENT.to_owned(),
            label: None,
            pack: PackRecord {
                id: "smoke".to_owned(),
                version: "1.0.0".to_owned(),
                description: "a pack".to_owned(),
                max_failed_share: CEILING,
                seed: 1,
                fixtures: BTreeMap::new(),
            },
            target: TargetRecord {
                base_url: "http://sut.invalid/openehr/v1".to_owned(),
                sut_version: None,
            },
            environment: EnvironmentFingerprint::detect(),
            started_at: "2026-08-29T00:00:00Z".to_owned(),
            finished_at: "2026-08-29T00:01:00Z".to_owned(),
            scale: ScaleRecord::default(),
            version_at_time: None,
            seed_phases: Vec::new(),
            repetitions: (1..=repetitions)
                .map(|index| RepetitionRecord {
                    repetition: u32::try_from(index).unwrap_or(1),
                    phases: BTreeMap::new(),
                    sweeps: BTreeMap::new(),
                })
                .collect(),
            cross: BTreeMap::new(),
            baselines: Vec::new(),
            relative: Vec::new(),
            methodology: Methodology {
                statement: crate::bench::METHODOLOGY.to_owned(),
                open_loop: true,
                coordinated_omission_free: true,
                seed_once_measure_n: true,
                repetitions: u32::try_from(repetitions).unwrap_or(1),
            },
            submittable: false,
            submittable_unmet: Vec::new(),
            posture: declared_only_posture(),
        }
    }

    /// A baseline block carrying only the provenance a record must disclose.
    fn empty_baseline() -> BaselineRecord {
        BaselineRecord {
            cdr: "ehrbase".to_owned(),
            display_name: "EHRbase".to_owned(),
            images: BTreeMap::new(),
            recipe: RecipeReference {
                repository: "https://example.invalid/cdr".to_owned(),
                git_ref: "v1.0.0".to_owned(),
                file: "docker-compose.yml".to_owned(),
            },
            resources: crate::bench::baselines::pinned_resources(),
            base_url: "http://127.0.0.1:18091/ehrbase/rest/openehr/v1".to_owned(),
            sut_version: None,
            started_at: "2026-08-29T00:02:00Z".to_owned(),
            finished_at: "2026-08-29T00:03:00Z".to_owned(),
            seed_phases: Vec::new(),
            repetitions: Vec::new(),
            cross: BTreeMap::new(),
            posture: declared_only_posture(),
        }
    }
}
