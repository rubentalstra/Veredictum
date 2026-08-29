// SPDX-FileCopyrightText: Veredictum contributors
// SPDX-License-Identifier: Apache-2.0

//! The relative index: the one number a bench record can carry across
//! machines.
//!
//! An absolute millisecond describes a system AND the machine it ran on, so
//! two records taken on different hosts cannot be read against one another.
//! The ratio can. For each phase, each operation and each metric the index is
//! the target's cross-repetition median divided by the same baseline
//! statistic, taken from a run on the same host in the same session, so the
//! machine cancels.
//!
//! The derivation is disclosed with the numbers: every ratio carries the two
//! medians it came from, and every place where no ratio could be formed is
//! recorded as a gap with its reason. An operation the baseline never
//! measured is a gap, never a silently omitted row: silence in a comparison
//! reads as agreement.

use std::collections::{BTreeMap, BTreeSet};
use std::fmt;

use serde::{Deserialize, Serialize};

use crate::bench::BenchError;
use crate::bench::compare::Metric;
use crate::bench::result::{BaselineRecord, CrossPhase, LoopRegime};

/// How every ratio in a relative-index block was derived, carried verbatim in
/// the artifact so the number is never read without its definition.
pub const RELATIVE_DERIVATION: &str = "The relative index is the target's cross-repetition median divided by the baseline's cross-repetition median for the same phase, operation and metric, both measured on the same host in the same session. It is dimensionless. On a latency metric a value below 1.0 means the target answered faster than the baseline and above 1.0 slower; on throughput the sense inverts, because there a larger number is the faster system.";

/// Why no index could be formed for one phase, operation or metric.
///
/// A closed vocabulary: an unrecognized reason is a defect in this enum, and
/// never an omitted row.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum GapReason {
    /// The baseline carries no such phase.
    PhaseAbsentFromBaseline,
    /// The target carries no such phase.
    PhaseAbsentFromTarget,
    /// The baseline carries the phase but never measured this operation.
    OperationAbsentFromBaseline,
    /// The target carries the phase but never measured this operation.
    OperationAbsentFromTarget,
    /// The baseline's median is zero, so the ratio is undefined.
    ZeroBaselineMedian,
}

impl GapReason {
    /// Every reason, in the order the schema enumerates them.
    pub const ALL: &[GapReason] = &[
        GapReason::PhaseAbsentFromBaseline,
        GapReason::PhaseAbsentFromTarget,
        GapReason::OperationAbsentFromBaseline,
        GapReason::OperationAbsentFromTarget,
        GapReason::ZeroBaselineMedian,
    ];

    /// The token the record names the reason by.
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

    /// Reads one token from the closed vocabulary.
    ///
    /// # Errors
    /// [`BenchError::UnknownToken`] listing the accepted tokens.
    pub fn parse(token: &str) -> Result<Self, BenchError> {
        Self::ALL
            .iter()
            .copied()
            .find(|reason| reason.as_str() == token)
            .ok_or_else(|| BenchError::UnknownToken {
                vocabulary: "relative-index gap reason",
                token: token.to_owned(),
                accepted: Self::ALL
                    .iter()
                    .map(|reason| reason.as_str())
                    .collect::<Vec<_>>()
                    .join(", "),
            })
    }
}

impl fmt::Display for GapReason {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl Serialize for GapReason {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(self.as_str())
    }
}

impl<'de> Deserialize<'de> for GapReason {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let token = String::deserialize(deserializer)?;
        GapReason::parse(&token).map_err(serde::de::Error::custom)
    }
}

/// One ratio, with the two medians it was derived from.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RelativeRatio {
    /// The target's cross-repetition median for this metric.
    pub target_median: f64,
    /// The baseline's cross-repetition median for the same metric.
    pub baseline_median: f64,
    /// The target median divided by the baseline median.
    pub index: f64,
}

/// One operation's ratios, keyed by the metric token.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RelativeOperation {
    /// One entry per metric an index could be formed for.
    pub metrics: BTreeMap<String, RelativeRatio>,
}

/// One phase's ratios, keyed by the operation token.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RelativePhase {
    /// The discipline that produced both sides of every ratio below.
    pub regime: LoopRegime,
    /// One entry per operation both sides measured.
    pub operations: BTreeMap<String, RelativeOperation>,
}

/// One place where no index could be formed, and why.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct IndexGap {
    /// The phase the gap belongs to.
    pub phase: String,
    /// The operation the gap belongs to.
    pub operation: String,
    /// The metric, when only one metric is missing. Absent when the whole
    /// operation or phase is.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub metric: Option<String>,
    /// Why no index exists here.
    pub reason: GapReason,
}

/// The target measured against one baseline.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RelativeIndex {
    /// The baseline token, matching the baseline block this was derived
    /// against.
    pub baseline: String,
    /// That baseline's human-readable name.
    pub display_name: String,
    /// How every ratio here was derived, verbatim from
    /// [`RELATIVE_DERIVATION`].
    pub derivation: String,
    /// The ratios, keyed by phase name.
    pub phases: BTreeMap<String, RelativePhase>,
    /// Every place no ratio could be formed, sorted, so a reader sees what is
    /// missing rather than inferring it from an absent row.
    pub gaps: Vec<IndexGap>,
}

/// Derives the relative index of a target's cross-repetition summary against
/// one baseline's.
///
/// Both sides are the same statistic over the same pack at the same seed, so
/// the ratio is the only thing the two hosts cannot both be blamed for: they
/// are the same host.
#[must_use]
pub fn derive(target: &BTreeMap<String, CrossPhase>, baseline: &BaselineRecord) -> RelativeIndex {
    let mut phases = BTreeMap::new();
    let mut gaps = Vec::new();
    for (phase_name, target_phase) in target {
        let Some(baseline_phase) = baseline.cross.get(phase_name) else {
            for operation in target_phase.operations.keys() {
                gaps.push(IndexGap {
                    phase: phase_name.clone(),
                    operation: operation.clone(),
                    metric: None,
                    reason: GapReason::PhaseAbsentFromBaseline,
                });
            }
            continue;
        };
        let mut operations = BTreeMap::new();
        for (operation, target_cross) in &target_phase.operations {
            let Some(baseline_cross) = baseline_phase.operations.get(operation) else {
                gaps.push(IndexGap {
                    phase: phase_name.clone(),
                    operation: operation.clone(),
                    metric: None,
                    reason: GapReason::OperationAbsentFromBaseline,
                });
                continue;
            };
            let mut metrics = BTreeMap::new();
            for metric in Metric::ALL {
                let target_median = metric.of(target_cross).median;
                let baseline_median = metric.of(baseline_cross).median;
                if baseline_median <= 0.0 {
                    gaps.push(IndexGap {
                        phase: phase_name.clone(),
                        operation: operation.clone(),
                        metric: Some(metric.as_str().to_owned()),
                        reason: GapReason::ZeroBaselineMedian,
                    });
                    continue;
                }
                let _replaced = metrics.insert(
                    metric.as_str().to_owned(),
                    RelativeRatio {
                        target_median,
                        baseline_median,
                        index: target_median / baseline_median,
                    },
                );
            }
            let _replaced = operations.insert(operation.clone(), RelativeOperation { metrics });
        }
        let _replaced = phases.insert(
            phase_name.clone(),
            RelativePhase {
                regime: target_phase.regime,
                operations,
            },
        );
    }
    gaps.extend(baseline_only(target, baseline));
    gaps.sort();
    gaps.dedup();
    RelativeIndex {
        baseline: baseline.cdr.clone(),
        display_name: baseline.display_name.clone(),
        derivation: RELATIVE_DERIVATION.to_owned(),
        phases,
        gaps,
    }
}

/// Everything the baseline measured that the target did not, recorded so a
/// reader sees both directions of the misalignment.
fn baseline_only(
    target: &BTreeMap<String, CrossPhase>,
    baseline: &BaselineRecord,
) -> Vec<IndexGap> {
    let mut gaps = Vec::new();
    for (phase_name, baseline_phase) in &baseline.cross {
        match target.get(phase_name) {
            None => {
                for operation in baseline_phase.operations.keys() {
                    gaps.push(IndexGap {
                        phase: phase_name.clone(),
                        operation: operation.clone(),
                        metric: None,
                        reason: GapReason::PhaseAbsentFromTarget,
                    });
                }
            }
            Some(target_phase) => {
                let measured: BTreeSet<&String> = target_phase.operations.keys().collect();
                for operation in baseline_phase.operations.keys() {
                    if !measured.contains(operation) {
                        gaps.push(IndexGap {
                            phase: phase_name.clone(),
                            operation: operation.clone(),
                            metric: None,
                            reason: GapReason::OperationAbsentFromTarget,
                        });
                    }
                }
            }
        }
    }
    gaps
}

#[cfg(test)]
#[expect(
    clippy::panic_in_result_fn,
    reason = "Result-returning tests in the Book ch11 shape that also assert; \
              clippy offers no allow-in-tests knob for this lint"
)]
mod tests {
    use super::*;
    use crate::bench::baselines::{ReferenceCdr, pinned_resources};
    use crate::bench::result::{CrossOperation, CrossStat, RecipeReference};

    /// A cross-operation summary whose every metric is the given value, so a
    /// ratio is readable by eye.
    fn flat_operation(value: f64) -> CrossOperation {
        let stat = || CrossStat {
            median: value,
            iqr: 0.0,
        };
        CrossOperation {
            repetitions: 3,
            p50_us: stat(),
            p75_us: stat(),
            p90_us: stat(),
            p99_us: stat(),
            p999_us: stat(),
            throughput_ops_s: stat(),
        }
    }

    /// One phase carrying the named operations at the given flat value.
    fn flat_phase(operations: &[(&str, f64)]) -> CrossPhase {
        CrossPhase {
            regime: LoopRegime::OpenLoop,
            operations: operations
                .iter()
                .map(|(name, value)| ((*name).to_owned(), flat_operation(*value)))
                .collect(),
        }
    }

    /// A baseline block carrying only the cross summary the derivation reads.
    fn baseline_with(cross: BTreeMap<String, CrossPhase>) -> BaselineRecord {
        BaselineRecord {
            cdr: ReferenceCdr::EhrBase.as_str().to_owned(),
            display_name: ReferenceCdr::EhrBase.display_name().to_owned(),
            images: BTreeMap::new(),
            recipe: RecipeReference {
                repository: "https://example.invalid/cdr".to_owned(),
                git_ref: "v1.0.0".to_owned(),
                file: "docker-compose.yml".to_owned(),
            },
            resources: pinned_resources(),
            base_url: "http://127.0.0.1:18091/ehrbase/rest/openehr/v1".to_owned(),
            sut_version: None,
            started_at: "2026-08-29T00:00:00Z".to_owned(),
            finished_at: "2026-08-29T00:10:00Z".to_owned(),
            seed_phases: Vec::new(),
            repetitions: Vec::new(),
            cross,
        }
    }

    /// The index is the plain quotient of the two medians, and it carries
    /// both of them so a reader recomputes it from the artifact.
    #[test]
    fn the_index_is_the_quotient_of_the_two_medians() -> Result<(), Box<dyn std::error::Error>> {
        let target: BTreeMap<String, CrossPhase> =
            [("mixed".to_owned(), flat_phase(&[("get_ehr", 400.0)]))]
                .into_iter()
                .collect();
        let baseline = baseline_with(
            [("mixed".to_owned(), flat_phase(&[("get_ehr", 1000.0)]))]
                .into_iter()
                .collect(),
        );
        let index = derive(&target, &baseline);
        let ratio = index
            .phases
            .get("mixed")
            .and_then(|phase| phase.operations.get("get_ehr"))
            .and_then(|operation| operation.metrics.get("p99_us"))
            .ok_or("the derivation lost the operation")?;
        assert!((ratio.index - 0.4).abs() < 1e-9, "{ratio:?}");
        assert!((ratio.target_median - 400.0).abs() < 1e-9, "{ratio:?}");
        assert!((ratio.baseline_median - 1000.0).abs() < 1e-9, "{ratio:?}");
        assert!(index.gaps.is_empty(), "{:?}", index.gaps);
        assert_eq!(index.derivation, RELATIVE_DERIVATION);
        Ok(())
    }

    /// Every metric the comparison aligns gets its own ratio, so the index is
    /// available at every percentile rather than only at the median.
    #[test]
    fn every_metric_carries_its_own_ratio() -> Result<(), Box<dyn std::error::Error>> {
        let target: BTreeMap<String, CrossPhase> =
            [("mixed".to_owned(), flat_phase(&[("get_ehr", 2.0)]))]
                .into_iter()
                .collect();
        let baseline = baseline_with(
            [("mixed".to_owned(), flat_phase(&[("get_ehr", 1.0)]))]
                .into_iter()
                .collect(),
        );
        let index = derive(&target, &baseline);
        let operation = index
            .phases
            .get("mixed")
            .and_then(|phase| phase.operations.get("get_ehr"))
            .ok_or("the derivation lost the operation")?;
        for metric in Metric::ALL {
            let ratio = operation
                .metrics
                .get(metric.as_str())
                .ok_or("a metric lost its ratio")?;
            assert!((ratio.index - 2.0).abs() < 1e-9, "{metric:?} {ratio:?}");
        }
        assert_eq!(operation.metrics.len(), Metric::ALL.len());
        Ok(())
    }

    /// An operation the baseline never measured yields no index and IS
    /// recorded, because a silently missing row reads as agreement.
    #[test]
    fn an_operation_absent_from_the_baseline_is_recorded_as_a_gap() {
        let target: BTreeMap<String, CrossPhase> = [(
            "mixed".to_owned(),
            flat_phase(&[("get_ehr", 100.0), ("post_composition", 200.0)]),
        )]
        .into_iter()
        .collect();
        let baseline = baseline_with(
            [("mixed".to_owned(), flat_phase(&[("get_ehr", 100.0)]))]
                .into_iter()
                .collect(),
        );
        let index = derive(&target, &baseline);
        assert_eq!(
            index.gaps,
            vec![IndexGap {
                phase: "mixed".to_owned(),
                operation: "post_composition".to_owned(),
                metric: None,
                reason: GapReason::OperationAbsentFromBaseline,
            }]
        );
        assert!(
            index
                .phases
                .get("mixed")
                .is_some_and(|phase| !phase.operations.contains_key("post_composition"))
        );
    }

    /// An operation only the BASELINE measured is recorded too, so the
    /// misalignment is visible from either side.
    #[test]
    fn an_operation_absent_from_the_target_is_recorded_as_a_gap() {
        let target: BTreeMap<String, CrossPhase> =
            [("mixed".to_owned(), flat_phase(&[("get_ehr", 100.0)]))]
                .into_iter()
                .collect();
        let baseline = baseline_with(
            [(
                "mixed".to_owned(),
                flat_phase(&[("get_ehr", 100.0), ("get_composition_latest", 50.0)]),
            )]
            .into_iter()
            .collect(),
        );
        let index = derive(&target, &baseline);
        assert_eq!(
            index.gaps,
            vec![IndexGap {
                phase: "mixed".to_owned(),
                operation: "get_composition_latest".to_owned(),
                metric: None,
                reason: GapReason::OperationAbsentFromTarget,
            }]
        );
    }

    /// A phase the baseline never ran yields one gap per target operation,
    /// never one silent omission for the whole phase.
    #[test]
    fn a_phase_absent_from_the_baseline_is_recorded_per_operation() {
        let target: BTreeMap<String, CrossPhase> = [
            ("mixed".to_owned(), flat_phase(&[("get_ehr", 100.0)])),
            (
                "walk".to_owned(),
                flat_phase(&[("get_composition_latest", 10.0), ("get_ehr", 20.0)]),
            ),
        ]
        .into_iter()
        .collect();
        let baseline = baseline_with(
            [("mixed".to_owned(), flat_phase(&[("get_ehr", 100.0)]))]
                .into_iter()
                .collect(),
        );
        let index = derive(&target, &baseline);
        assert_eq!(index.gaps.len(), 2, "{:?}", index.gaps);
        assert!(
            index
                .gaps
                .iter()
                .all(|gap| gap.reason == GapReason::PhaseAbsentFromBaseline && gap.phase == "walk"),
            "{:?}",
            index.gaps
        );
    }

    /// A zero baseline median has no quotient, so the metric is a gap rather
    /// than an infinity a reader would take for a number.
    #[test]
    fn a_zero_baseline_median_yields_a_gap_and_never_an_infinity() {
        let target: BTreeMap<String, CrossPhase> =
            [("mixed".to_owned(), flat_phase(&[("get_ehr", 100.0)]))]
                .into_iter()
                .collect();
        let baseline = baseline_with(
            [("mixed".to_owned(), flat_phase(&[("get_ehr", 0.0)]))]
                .into_iter()
                .collect(),
        );
        let index = derive(&target, &baseline);
        assert_eq!(index.gaps.len(), Metric::ALL.len(), "{:?}", index.gaps);
        assert!(
            index
                .gaps
                .iter()
                .all(|gap| gap.reason == GapReason::ZeroBaselineMedian && gap.metric.is_some()),
            "{:?}",
            index.gaps
        );
        assert!(
            index
                .phases
                .get("mixed")
                .and_then(|phase| phase.operations.get("get_ehr"))
                .is_some_and(|operation| operation.metrics.is_empty())
        );
    }

    /// Every gap-reason token round-trips through the serializer, and an
    /// unknown one is refused rather than silently defaulting.
    #[test]
    fn every_gap_reason_token_round_trips() -> Result<(), Box<dyn std::error::Error>> {
        for reason in GapReason::ALL {
            assert_eq!(GapReason::parse(reason.as_str())?, *reason);
            let text = serde_json::to_string(reason)?;
            let back: GapReason = serde_json::from_str(&text)?;
            assert_eq!(back, *reason);
        }
        assert!(GapReason::parse("operation-was-slow").is_err());
        assert!(serde_json::from_str::<GapReason>("\"nonsense\"").is_err());
        Ok(())
    }

    /// The whole block round-trips through JSON, which is how a consumer
    /// reads it back out of the record.
    #[test]
    fn a_relative_index_round_trips_through_json() -> Result<(), Box<dyn std::error::Error>> {
        let target: BTreeMap<String, CrossPhase> =
            [("mixed".to_owned(), flat_phase(&[("get_ehr", 250.0)]))]
                .into_iter()
                .collect();
        let baseline = baseline_with(
            [("mixed".to_owned(), flat_phase(&[("get_ehr", 500.0)]))]
                .into_iter()
                .collect(),
        );
        let index = derive(&target, &baseline);
        let text = serde_json::to_string(&index)?;
        let back: RelativeIndex = serde_json::from_str(&text)?;
        assert_eq!(back, index);
        Ok(())
    }
}
