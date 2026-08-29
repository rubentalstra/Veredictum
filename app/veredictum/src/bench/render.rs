// SPDX-FileCopyrightText: Veredictum contributors
// SPDX-License-Identifier: Apache-2.0

//! The rendered views: a run summary and an aligned cross-file comparison.
//!
//! Both are Markdown, which reads as plain text on a console and as a
//! document in a repository, so one rendering serves the terminal and the
//! written file. Both open with the boundary statement, because a table of
//! speed numbers is exactly the artifact someone quotes out of context.

use std::fmt::Write as _;

use crate::bench::compare::Comparison;
use crate::bench::result::BenchResult;
use crate::bench::{BOUNDARY_STATEMENT, METHODOLOGY};

/// The file name a rendered comparison is written under.
pub const COMPARISON_FILE: &str = "bench-comparison.md";

/// The opening block: what was driven, against what, in which configuration.
fn preamble(result: &BenchResult) -> String {
    let mut out = String::new();
    let _written = writeln!(out, "# Bench result: {}", result.pack.id);
    let _written = writeln!(out);
    let _written = writeln!(out, "{BOUNDARY_STATEMENT}");
    let _written = writeln!(out);
    let _written = writeln!(
        out,
        "Target `{}`, pack `{}` version {}, seed `{:#018x}`, {} repetition(s), submittable: {}.",
        result.target.base_url,
        result.pack.id,
        result.pack.version,
        result.pack.seed,
        result.repetitions.len(),
        result.submittable
    );
    if let Some(version) = &result.target.sut_version {
        let _written = writeln!(out, "The system reports its version as `{version}`.");
    }
    let _written = writeln!(
        out,
        "Scale factor {:.3}, seed workers as declared: {}. Reference configuration: {}.",
        result.scale.factor, result.scale.declared_workers, result.scale.reference_configuration
    );
    if !result.scale.reference_configuration {
        let _written = writeln!(
            out,
            "This run is off the pack's pinned configuration, so its numbers are not comparable with the reference figures the pack describes."
        );
    }
    if let Some(instant) = &result.version_at_time {
        let _written = writeln!(
            out,
            "Every `version_at_time` read addressed `{instant}`, captured after the seed phases finished."
        );
    }
    out
}

/// Renders one finished run as a Markdown summary.
#[must_use]
pub fn run_summary(result: &BenchResult) -> String {
    let mut out = preamble(result);
    let _written = writeln!(out);
    for seed in &result.seed_phases {
        let _written = writeln!(
            out,
            "Seed phase `{}` ({}): {} EHRs x {} compositions on {} worker(s) in {:.1}s, {:.1} writes/s, {:.2} ms/composition whole-loop ({}).",
            seed.name,
            seed.regime,
            seed.ehrs,
            seed.compositions_per_ehr,
            seed.workers,
            seed.elapsed_s,
            seed.bulk_load_writes_per_s,
            seed.whole_loop_ms_per_composition,
            seed.regime
        );
    }
    for repetition in &result.repetitions {
        for (name, sweep) in &repetition.sweeps {
            let _written = writeln!(
                out,
                "Sweep `{name}` ({}) repetition {}: {} request(s) over {} composition(s) on {} worker(s) in {:.1}s, {:.1} us/request whole-loop ({}).",
                sweep.regime,
                repetition.repetition,
                sweep.requests,
                sweep.compositions,
                sweep.workers,
                sweep.elapsed_s,
                sweep.whole_loop_us_per_request,
                sweep.regime
            );
        }
    }
    if !result.seed_phases.is_empty() {
        let _written = writeln!(out);
    }
    for (phase, cross) in &result.cross {
        let _written = writeln!(out, "## Phase `{phase}` ({})", cross.regime);
        let _written = writeln!(out);
        let _written = writeln!(
            out,
            "| Operation | p50 us | p90 us | p99 us | p99.9 us | ops/s | IQR of p99 us |"
        );
        let _written = writeln!(out, "|---|---:|---:|---:|---:|---:|---:|");
        for (operation, stat) in &cross.operations {
            let _written = writeln!(
                out,
                "| `{operation}` | {:.0} | {:.0} | {:.0} | {:.0} | {:.1} | {:.0} |",
                stat.p50_us.median,
                stat.p90_us.median,
                stat.p99_us.median,
                stat.p999_us.median,
                stat.throughput_ops_s.median,
                stat.p99_us.iqr
            );
        }
        let _written = writeln!(out);
    }
    let errors = error_lines(result);
    if !errors.is_empty() {
        let _written = writeln!(out, "## Errors by class");
        let _written = writeln!(out);
        for line in errors {
            let _written = writeln!(out, "- {line}");
        }
        let _written = writeln!(out);
    }
    let _written = writeln!(out, "## Methodology");
    let _written = writeln!(out);
    let _written = writeln!(out, "{METHODOLOGY}");
    out
}

/// The per-operation error counts across every repetition, one line each.
fn error_lines(result: &BenchResult) -> Vec<String> {
    let mut lines = Vec::new();
    for repetition in &result.repetitions {
        for (phase, record) in &repetition.phases {
            if record.generator_bound {
                lines.push(format!(
                    "repetition {} phase `{phase}`: the GENERATOR was the bottleneck, so the latencies understate the system",
                    repetition.repetition
                ));
            }
            for (operation, stats) in &record.operations {
                if stats.errors == 0 {
                    continue;
                }
                let classes = stats
                    .errors_by_class
                    .iter()
                    .map(|(class, count)| format!("{class}={count}"))
                    .collect::<Vec<_>>()
                    .join(" ");
                lines.push(format!(
                    "repetition {} phase `{phase}` ({}) operation `{operation}`: {} error(s) [{classes}]",
                    repetition.repetition, record.regime, stats.errors
                ));
            }
        }
        for (phase, sweep) in &repetition.sweeps {
            for (operation, stats) in &sweep.operations {
                if stats.errors == 0 {
                    continue;
                }
                let classes = stats
                    .errors_by_class
                    .iter()
                    .map(|(class, count)| format!("{class}={count}"))
                    .collect::<Vec<_>>()
                    .join(" ");
                lines.push(format!(
                    "repetition {} sweep `{phase}` ({}) operation `{operation}`: {} error(s) [{classes}]",
                    repetition.repetition, sweep.regime, stats.errors
                ));
            }
        }
    }
    lines
}

/// Renders an aligned comparison as a Markdown document.
#[must_use]
pub fn comparison(comparison: &Comparison) -> String {
    let mut out = String::new();
    let _written = writeln!(out, "# Bench comparison");
    let _written = writeln!(out);
    let _written = writeln!(out, "{BOUNDARY_STATEMENT}");
    let _written = writeln!(out);
    if comparison.warnings.is_empty() {
        let _written = writeln!(
            out,
            "Every column ran the same pack version from the same generator host."
        );
    } else {
        let _written = writeln!(out, "**Read these before reading the numbers.**");
        let _written = writeln!(out);
        for warning in &comparison.warnings {
            let _written = writeln!(out, "- {warning}");
        }
    }
    let _written = writeln!(out);
    let _written = writeln!(out, "## Columns");
    let _written = writeln!(out);
    let _written = writeln!(
        out,
        "| Column | Pack | SUT version | Repetitions | Submittable | Scale | Reference config | Source |"
    );
    let _written = writeln!(out, "|---|---|---|---:|---|---:|---|---|");
    for column in &comparison.columns {
        let _written = writeln!(
            out,
            "| {} | `{}@{}` | {} | {} | {} | {:.3} | {} | `{}` |",
            column.label,
            column.pack_id,
            column.pack_version,
            column.sut_version.as_deref().unwrap_or("(undisclosed)"),
            column.repetitions,
            column.submittable,
            column.scale_factor,
            column.reference_configuration,
            column.source.display()
        );
    }
    let _written = writeln!(out);
    let _written = writeln!(out, "## Aligned metrics");
    let _written = writeln!(out);
    let _written = writeln!(
        out,
        "Each cell is the cross-repetition median with the inter-quartile range in parentheses. The discipline column says which regime produced the row: a closed-loop average and an open-loop percentile answer different questions and are never read against one another."
    );
    let _written = writeln!(out);
    let mut header = String::from("| Phase | Discipline | Operation | Metric |");
    let mut divider = String::from("|---|---|---|---|");
    for column in &comparison.columns {
        let _written = write!(header, " {} |", column.label);
        divider.push_str("---:|");
    }
    let _written = writeln!(out, "{header}");
    let _written = writeln!(out, "{divider}");
    for row in &comparison.rows {
        let mut line = format!(
            "| `{}` | {} | `{}` | {} |",
            row.phase,
            row.regime,
            row.operation,
            row.metric.as_str()
        );
        for cell in &row.cells {
            match cell {
                Some(stat) => {
                    let _written = write!(line, " {:.1} ({:.1}) |", stat.median, stat.iqr);
                }
                None => line.push_str(" — |"),
            }
        }
        let _written = writeln!(out, "{line}");
    }
    out
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;
    use std::path::PathBuf;

    use super::*;
    use crate::bench::compare::{ComparisonColumn, ComparisonRow, Metric};
    use crate::bench::result::{CrossStat, LoopRegime};

    /// A comparison always carries the boundary statement, whatever else it
    /// says.
    #[test]
    fn a_rendered_comparison_carries_the_boundary_statement() {
        let column = |label: &str| ComparisonColumn {
            label: label.to_owned(),
            source: PathBuf::from(format!("{label}.json")),
            pack_id: "smoke".to_owned(),
            pack_version: "1.0.0".to_owned(),
            sut_version: None,
            repetitions: 3,
            submittable: true,
            scale_factor: 1.0,
            reference_configuration: true,
            environment: BTreeMap::new(),
        };
        let rendered = comparison(&Comparison {
            columns: vec![column("left"), column("right")],
            warnings: Vec::new(),
            rows: vec![ComparisonRow {
                phase: "mixed".to_owned(),
                regime: LoopRegime::OpenLoop,
                operation: "get_ehr".to_owned(),
                metric: Metric::P99Us,
                cells: vec![
                    Some(CrossStat {
                        median: 1200.0,
                        iqr: 40.0,
                    }),
                    None,
                ],
            }],
        });
        assert!(rendered.contains(BOUNDARY_STATEMENT), "{rendered}");
        assert!(rendered.contains("| left |"), "{rendered}");
        assert!(rendered.contains("1200.0 (40.0)"), "{rendered}");
        assert!(rendered.contains(" — |"), "{rendered}");
    }

    /// A mismatched pack version is stated in the header, never buried in a
    /// footnote nobody reads.
    #[test]
    fn a_pack_mismatch_is_stated_before_the_numbers() {
        let mut left = ComparisonColumn {
            label: "left".to_owned(),
            source: PathBuf::from("left.json"),
            pack_id: "smoke".to_owned(),
            pack_version: "1.0.0".to_owned(),
            sut_version: None,
            repetitions: 1,
            submittable: false,
            scale_factor: 1.0,
            reference_configuration: true,
            environment: BTreeMap::new(),
        };
        let right = ComparisonColumn {
            pack_version: "2.0.0".to_owned(),
            label: "right".to_owned(),
            ..left.clone()
        };
        left.submittable = true;
        left.repetitions = 3;
        let rendered = comparison(&Comparison {
            columns: vec![left, right],
            warnings: vec![
                "the columns ran DIFFERENT packs (smoke@1.0.0, smoke@2.0.0), so the numbers describe different work".to_owned(),
                "column \"right\" carries 1 repetition(s) and is not submittable".to_owned(),
            ],
            rows: Vec::new(),
        });
        let warning_at = rendered.find("DIFFERENT packs");
        let table_at = rendered.find("## Aligned metrics");
        assert!(warning_at < table_at, "{rendered}");
        assert!(rendered.contains("not submittable"), "{rendered}");
    }
}
