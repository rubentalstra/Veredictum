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

/// Renders one finished run as a Markdown summary.
#[must_use]
pub fn run_summary(result: &BenchResult) -> String {
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
    let _written = writeln!(out);
    for seed in &result.seed_phases {
        let _written = writeln!(
            out,
            "Seed phase `{}` ({}): {} EHRs x {} compositions in {:.1}s, {:.1} writes/s.",
            seed.name,
            seed.regime,
            seed.ehrs,
            seed.compositions_per_ehr,
            seed.elapsed_s,
            seed.bulk_load_writes_per_s
        );
    }
    if !result.seed_phases.is_empty() {
        let _written = writeln!(out);
    }
    for (phase, cross) in &result.cross {
        let _written = writeln!(out, "## Phase `{phase}`");
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
                    "repetition {} phase `{phase}` operation `{operation}`: {} error(s) [{classes}]",
                    repetition.repetition, stats.errors
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
        "| Column | Pack | SUT version | Repetitions | Submittable | Source |"
    );
    let _written = writeln!(out, "|---|---|---|---:|---|---|");
    for column in &comparison.columns {
        let _written = writeln!(
            out,
            "| {} | `{}@{}` | {} | {} | {} | `{}` |",
            column.label,
            column.pack_id,
            column.pack_version,
            column.sut_version.as_deref().unwrap_or("(undisclosed)"),
            column.repetitions,
            column.submittable,
            column.source.display()
        );
    }
    let _written = writeln!(out);
    let _written = writeln!(out, "## Aligned metrics");
    let _written = writeln!(out);
    let _written = writeln!(
        out,
        "Each cell is the cross-repetition median with the inter-quartile range in parentheses."
    );
    let _written = writeln!(out);
    let mut header = String::from("| Phase | Operation | Metric |");
    let mut divider = String::from("|---|---|---|");
    for column in &comparison.columns {
        let _written = write!(header, " {} |", column.label);
        divider.push_str("---:|");
    }
    let _written = writeln!(out, "{header}");
    let _written = writeln!(out, "{divider}");
    for row in &comparison.rows {
        let mut line = format!(
            "| `{}` | `{}` | {} |",
            row.phase,
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
    use crate::bench::result::CrossStat;

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
            environment: BTreeMap::new(),
        };
        let rendered = comparison(&Comparison {
            columns: vec![column("left"), column("right")],
            warnings: Vec::new(),
            rows: vec![ComparisonRow {
                phase: "mixed".to_owned(),
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
