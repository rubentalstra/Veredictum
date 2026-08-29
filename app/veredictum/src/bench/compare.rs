// SPDX-FileCopyrightText: Veredictum contributors
// SPDX-License-Identifier: Apache-2.0

//! Cross-repetition and cross-file comparison.
//!
//! Two jobs live here. The first is the summary statistic a single run
//! reports across its own repetitions: the median and the inter-quartile
//! range, which say what the system does typically and how far the
//! repetitions spread. The second is the alignment of several committed
//! results into one table, one column per file, with every disagreement about
//! pack version or host stated in the header rather than buried.
//!
//! Quantiles use the linear interpolation between order statistics that R's
//! `quantile(type = 7)` and `NumPy`'s `percentile` both take as their default
//! (<https://numpy.org/doc/stable/reference/generated/numpy.percentile.html>).
//! Repetition counts here are small, so the interpolation choice is visible
//! in the number and is therefore pinned rather than left to a library.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use crate::bench::BenchError;
use crate::bench::result::{BenchResult, CrossOperation, CrossPhase, CrossStat, RepetitionRecord};

/// The metrics a comparison aligns, in the order it renders them.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
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
    pub fn of(self, cross: &CrossOperation) -> &CrossStat {
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

/// The quantile of a sorted sample by linear interpolation between order
/// statistics.
///
/// Returns `None` for an empty sample. `q` is clamped to `0.0 ..= 1.0`.
#[must_use]
#[expect(
    clippy::as_conversions,
    clippy::cast_precision_loss,
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    clippy::indexing_slicing,
    reason = "the floor of a clamped non-negative position below the sample length, with both indices proven inside the slice by the min just below"
)]
pub fn quantile(sorted: &[f64], q: f64) -> Option<f64> {
    let last = sorted.len().checked_sub(1)?;
    if last == 0 {
        return sorted.first().copied();
    }
    let position = q.clamp(0.0, 1.0) * last as f64;
    let lower = position.floor();
    let lower_index = (lower as usize).min(last);
    let upper_index = lower_index.saturating_add(1).min(last);
    let fraction = position - lower;
    let low = sorted[lower_index];
    let high = sorted[upper_index];
    Some(low + fraction * (high - low))
}

/// The median and inter-quartile range of a sample, in any order.
///
/// An empty sample yields zeros, which is what a phase with no recorded
/// arrival honestly reports.
#[must_use]
pub fn cross_stat(values: &[f64]) -> CrossStat {
    let mut sorted: Vec<f64> = values.to_vec();
    sorted.sort_by(f64::total_cmp);
    let median = quantile(&sorted, 0.50).unwrap_or(0.0);
    let q1 = quantile(&sorted, 0.25).unwrap_or(0.0);
    let q3 = quantile(&sorted, 0.75).unwrap_or(0.0);
    CrossStat {
        median,
        iqr: q3 - q1,
    }
}

/// Summarizes every measured phase across the run's repetitions.
#[must_use]
#[expect(
    clippy::as_conversions,
    clippy::cast_precision_loss,
    reason = "recorded microsecond percentiles are far below 2^52"
)]
pub fn summarize(repetitions: &[RepetitionRecord]) -> BTreeMap<String, CrossPhase> {
    let mut phases: BTreeMap<String, BTreeMap<String, Vec<&crate::bench::result::OperationStats>>> =
        BTreeMap::new();
    for repetition in repetitions {
        for (phase_name, phase) in &repetition.phases {
            let operations = phases.entry(phase_name.clone()).or_default();
            for (op, stats) in &phase.operations {
                operations.entry(op.clone()).or_default().push(stats);
            }
        }
    }
    phases
        .into_iter()
        .map(|(phase_name, operations)| {
            let operations = operations
                .into_iter()
                .map(|(op, samples)| {
                    let cross = CrossOperation {
                        repetitions: u32::try_from(samples.len()).unwrap_or(u32::MAX),
                        p50_us: cross_stat(
                            &samples.iter().map(|s| s.p50_us as f64).collect::<Vec<_>>(),
                        ),
                        p75_us: cross_stat(
                            &samples.iter().map(|s| s.p75_us as f64).collect::<Vec<_>>(),
                        ),
                        p90_us: cross_stat(
                            &samples.iter().map(|s| s.p90_us as f64).collect::<Vec<_>>(),
                        ),
                        p99_us: cross_stat(
                            &samples.iter().map(|s| s.p99_us as f64).collect::<Vec<_>>(),
                        ),
                        p999_us: cross_stat(
                            &samples.iter().map(|s| s.p999_us as f64).collect::<Vec<_>>(),
                        ),
                        throughput_ops_s: cross_stat(
                            &samples
                                .iter()
                                .map(|s| s.throughput_ops_s)
                                .collect::<Vec<_>>(),
                        ),
                    };
                    (op, cross)
                })
                .collect();
            (phase_name, CrossPhase { operations })
        })
        .collect()
}

/// One column of a comparison: everything about the file that is not a
/// number in the body.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ComparisonColumn {
    /// The operator's label, falling back to the file name.
    pub label: String,
    /// The file the column was read from.
    pub source: PathBuf,
    /// The pack the run drove.
    pub pack_id: String,
    /// The pack version, which must match across columns to be comparable.
    pub pack_version: String,
    /// The SUT's self-reported version, when it disclosed one.
    pub sut_version: Option<String>,
    /// How many repetitions the run carried.
    pub repetitions: u32,
    /// Whether the run carries enough repetitions to be offered.
    pub submittable: bool,
    /// The generator host, as an ordered label map.
    pub environment: BTreeMap<String, String>,
}

/// One aligned row: the same phase, operation and metric across every column.
#[derive(Debug, Clone, PartialEq)]
pub struct ComparisonRow {
    /// The phase the row belongs to.
    pub phase: String,
    /// The operation.
    pub operation: String,
    /// The metric.
    pub metric: Metric,
    /// One cell per column, in column order; `None` where that file carries
    /// no such operation.
    pub cells: Vec<Option<CrossStat>>,
}

/// Several committed results, aligned into one table.
#[derive(Debug, Clone, PartialEq)]
pub struct Comparison {
    /// The columns, in the order the files were given.
    pub columns: Vec<ComparisonColumn>,
    /// Everything that makes the columns less than directly comparable.
    pub warnings: Vec<String>,
    /// The aligned rows, sorted by phase, then operation, then metric.
    pub rows: Vec<ComparisonRow>,
}

/// Reads one committed bench result.
///
/// # Errors
/// [`BenchError::Read`] when the file cannot be read, or
/// [`BenchError::Parse`] when it is not a bench result.
pub fn read_result(path: &Path) -> Result<BenchResult, BenchError> {
    let text = std::fs::read_to_string(path).map_err(|source| BenchError::Read {
        path: path.to_owned(),
        source,
    })?;
    serde_json::from_str(&text).map_err(|error| BenchError::Parse {
        path: path.to_owned(),
        message: error.to_string(),
    })
}

/// Aligns two or more committed results into one comparison.
///
/// # Errors
/// [`BenchError::TooFewResults`] for fewer than two files, plus whatever
/// [`read_result`] reports for each one.
pub fn compare(paths: &[PathBuf]) -> Result<Comparison, BenchError> {
    if paths.len() < 2 {
        return Err(BenchError::TooFewResults(paths.len()));
    }
    let mut columns = Vec::with_capacity(paths.len());
    let mut results = Vec::with_capacity(paths.len());
    for path in paths {
        let result = read_result(path)?;
        columns.push(ComparisonColumn {
            label: result.label.clone().unwrap_or_else(|| {
                path.file_name()
                    .and_then(std::ffi::OsStr::to_str)
                    .unwrap_or("(unnamed)")
                    .to_owned()
            }),
            source: path.clone(),
            pack_id: result.pack.id.clone(),
            pack_version: result.pack.version.clone(),
            sut_version: result.target.sut_version.clone(),
            repetitions: u32::try_from(result.repetitions.len()).unwrap_or(u32::MAX),
            submittable: result.submittable,
            environment: result.environment.labels(),
        });
        results.push(result);
    }
    let warnings = warnings(&columns);
    let mut keys: BTreeSet<(String, String)> = BTreeSet::new();
    for result in &results {
        for (phase, cross) in &result.cross {
            for op in cross.operations.keys() {
                let _fresh = keys.insert((phase.clone(), op.clone()));
            }
        }
    }
    let mut rows = Vec::new();
    for (phase, operation) in keys {
        for metric in Metric::ALL {
            let cells = results
                .iter()
                .map(|result| {
                    result
                        .cross
                        .get(&phase)
                        .and_then(|cross| cross.operations.get(&operation))
                        .map(|cross| metric.of(cross).clone())
                })
                .collect();
            rows.push(ComparisonRow {
                phase: phase.clone(),
                operation: operation.clone(),
                metric: *metric,
                cells,
            });
        }
    }
    Ok(Comparison {
        columns,
        warnings,
        rows,
    })
}

/// Everything that makes a set of columns less than directly comparable.
fn warnings(columns: &[ComparisonColumn]) -> Vec<String> {
    let mut warnings = Vec::new();
    let packs: BTreeSet<String> = columns
        .iter()
        .map(|column| format!("{}@{}", column.pack_id, column.pack_version))
        .collect();
    if packs.len() > 1 {
        warnings.push(format!(
            "the columns ran DIFFERENT packs ({}), so the numbers describe different work",
            packs.into_iter().collect::<Vec<_>>().join(", ")
        ));
    }
    let hosts: BTreeSet<String> = columns
        .iter()
        .map(|column| {
            column
                .environment
                .iter()
                .map(|(key, value)| format!("{key}={value}"))
                .collect::<Vec<_>>()
                .join(" ")
        })
        .collect();
    if hosts.len() > 1 {
        warnings.push(
            "the columns were generated from DIFFERENT hosts, so a latency difference may be the generator's".to_owned(),
        );
    }
    for column in columns {
        if !column.submittable {
            warnings.push(format!(
                "column {:?} carries {} repetition(s) and is not submittable",
                column.label, column.repetitions
            ));
        }
    }
    warnings
}

#[cfg(test)]
#[expect(
    clippy::panic_in_result_fn,
    reason = "a Result-returning test in the Book ch11 shape that also asserts; \
              clippy offers no allow-in-tests knob for this lint"
)]
mod tests {
    use super::*;
    use crate::bench::result::MeasuredPhaseRecord;

    /// The quantile matches the interpolated order statistic by hand, which
    /// is the definition the module doc pins.
    #[test]
    fn quantiles_interpolate_between_order_statistics() {
        let sample = [1.0, 2.0, 3.0, 4.0];
        assert_eq!(quantile(&sample, 0.0), Some(1.0));
        assert_eq!(quantile(&sample, 0.25), Some(1.75));
        assert_eq!(quantile(&sample, 0.50), Some(2.5));
        assert_eq!(quantile(&sample, 0.75), Some(3.25));
        assert_eq!(quantile(&sample, 1.0), Some(4.0));
        assert_eq!(quantile(&[], 0.5), None);
        assert_eq!(quantile(&[7.0], 0.9), Some(7.0));
    }

    /// The median of an odd sample is its middle element, and the IQR of a
    /// constant sample is zero.
    #[test]
    fn the_cross_statistic_reports_median_and_spread() {
        let stat = cross_stat(&[3.0, 1.0, 2.0]);
        assert!((stat.median - 2.0).abs() < 1e-9, "{stat:?}");
        assert!((stat.iqr - 1.0).abs() < 1e-9, "{stat:?}");
        let flat = cross_stat(&[5.0, 5.0, 5.0, 5.0]);
        assert!((flat.median - 5.0).abs() < 1e-9, "{flat:?}");
        assert!(flat.iqr.abs() < 1e-9, "{flat:?}");
        let empty = cross_stat(&[]);
        assert!(empty.median.abs() < 1e-9, "{empty:?}");
    }

    /// Sample order never changes the answer, so a repetition arriving in a
    /// different order reports the same summary.
    #[test]
    fn the_cross_statistic_ignores_sample_order() {
        let forward = cross_stat(&[10.0, 20.0, 30.0, 40.0, 50.0]);
        let backward = cross_stat(&[50.0, 40.0, 30.0, 20.0, 10.0]);
        assert_eq!(forward, backward);
    }

    /// A single result file is refused: a comparison needs something to
    /// compare against.
    #[test]
    fn one_file_is_not_a_comparison() {
        let error = compare(&[PathBuf::from("a.json")]).unwrap_err();
        assert!(matches!(error, BenchError::TooFewResults(1)), "{error}");
    }

    /// Repetition summaries collect per phase and per operation.
    #[test]
    fn repetitions_summarize_per_phase_and_operation() -> Result<(), Box<dyn std::error::Error>> {
        let stats = |p50: u64| -> Result<_, BenchError> {
            let mut histogram = hdrhistogram::Histogram::<u64>::new_with_bounds(1, 600_000_000, 3)
                .map_err(|e| BenchError::Histogram(e.to_string()))?;
            for _ in 0..10 {
                let _saturated = histogram.record(p50);
            }
            crate::bench::result::OperationStats::from_histogram(&histogram, BTreeMap::new(), 1.0)
        };
        let phase = |p50: u64| -> Result<MeasuredPhaseRecord, BenchError> {
            let mut operations = BTreeMap::new();
            let _replaced = operations.insert("get_ehr".to_owned(), stats(p50)?);
            Ok(MeasuredPhaseRecord {
                regime: crate::bench::result::LoopRegime::OpenLoop,
                rate_per_s: 1.0,
                warmup_s: 0,
                duration_s: 1,
                planned_measured_arrivals: 10,
                dispatched_measured_arrivals: 10,
                warmup_arrivals: 0,
                offered_load_sustained_per_s: 10.0,
                generator_bound: false,
                operations,
            })
        };
        let repetitions: Vec<RepetitionRecord> = [100_u64, 200, 300]
            .into_iter()
            .enumerate()
            .map(|(index, p50)| {
                let mut phases = BTreeMap::new();
                let _replaced = phases.insert("mixed".to_owned(), phase(p50)?);
                Ok(RepetitionRecord {
                    repetition: u32::try_from(index).unwrap_or(0).saturating_add(1),
                    phases,
                })
            })
            .collect::<Result<_, BenchError>>()?;
        let cross = summarize(&repetitions);
        let operation = cross
            .get("mixed")
            .and_then(|phase| phase.operations.get("get_ehr"))
            .ok_or("the summary lost the operation")?;
        assert_eq!(operation.repetitions, 3);
        assert!(operation.p50_us.median > 190.0, "{operation:?}");
        assert!(operation.p50_us.median < 210.0, "{operation:?}");
        assert!(operation.p50_us.iqr > 0.0, "{operation:?}");
        Ok(())
    }
}
