// SPDX-FileCopyrightText: Veredictum contributors
// SPDX-License-Identifier: Apache-2.0

//! The published visuals, rendered deterministically from committed
//! artifacts.
//!
//! Every asset here is a pure function of files already in a repository —
//! the catalogue, a results record, a verdict record, a stress report — so
//! regenerating one and diffing it is a build gate, and a hand-drawn number
//! is a build failure.

use std::path::Path;

use crate::party::Results;
use crate::perf::{Measurement, PerformanceCase};
use crate::pipeline::{Error, RenderedFile, load_clean_root, load_party_json, read_json};
use crate::schema::results_schema;
use crate::stress::StressReport;

/// The slice of a committed `verdicts.json` the conformance visuals read.
#[derive(Debug, serde::Deserialize)]
pub struct VerdictEvidence {
    /// The per-capability evidence, in the record's own order.
    pub capabilities: Vec<(String, crate::verdict::Evidence)>,
}

/// Renders the conformance visuals: the capability heat grid and the
/// per-chapter outcome bars.
///
/// `suffix` is appended to each file stem, which is how a comparison SUT's
/// copies sit beside the primary ones in the same directory.
///
/// # Errors
/// [`Error::Catalogue`] when the root cannot be opened, [`Error::Missing`]
/// when it carries no capability matrix, [`Error::Party`], [`Error::Read`]
/// or [`Error::Parse`] for the committed records, and
/// [`Error::Instrument`] when a case id maps to no chapter.
pub fn conformance_assets(
    root: &Path,
    results_path: &Path,
    verdicts_path: &Path,
    suffix: &str,
) -> Result<Vec<RenderedFile>, Error> {
    let loaded = crate::artifacts::load_root(root).map_err(Error::Catalogue)?;
    let Some((_, matrix)) = &loaded.set.matrix else {
        return Err(Error::Missing(
            "artifact set has no capability matrix".to_owned(),
        ));
    };
    let results: Results = load_party_json(results_path, &results_schema(), "results.schema.json")?;
    let verdicts: VerdictEvidence = read_json(verdicts_path, "verdicts")?;
    let sut_label = format!("{} {}", results.sut.name, results.sut.version);
    // An unmapped case id is a taxonomy gap, not a chart to publish: the
    // renderer fails loudly and names the id.
    let chapters = crate::conf_assets::chapter_counts(&results)
        .map_err(|e| Error::Instrument(e.to_string()))?;
    Ok(vec![
        RenderedFile {
            name: format!("conformance-heat-grid{suffix}.svg"),
            body: crate::conf_assets::heat_grid_svg(&sut_label, matrix, &verdicts.capabilities),
        },
        RenderedFile {
            name: format!("conformance-chapter-bars{suffix}.svg"),
            body: crate::conf_assets::chapter_bars_svg(&sut_label, &chapters),
        },
    ])
}

/// The performance visuals, with the inputs the Markdown summary is derived
/// from.
#[derive(Debug)]
pub struct PerformanceAssets {
    /// The rendered SVG files, in publication order.
    pub files: Vec<RenderedFile>,
    cases: Vec<PerformanceCase>,
    measurements: Vec<Measurement>,
}

impl PerformanceAssets {
    /// Renders the Markdown summary: the class ladder plus the measured
    /// detail behind each earned class.
    ///
    /// # Errors
    /// [`Error::Instrument`] when a measurement carries no re-checkable
    /// record for a class the summary reports.
    pub fn summary_markdown(&self) -> Result<String, Error> {
        crate::perf_assets::summary_markdown(&self.cases, &self.measurements)
            .map_err(|e| Error::Instrument(format!("summary: {e}")))
    }
}

/// Renders the performance visuals from a committed results record.
///
/// A stress report renders the latency-throughput curve beside them when the
/// caller supplies one; the resource series and the disk-growth chart render
/// only from records that actually carry those samples, because nothing here
/// is fabricated.
///
/// # Errors
/// [`Error::Catalogue`] or [`Error::Artifacts`] when the tree does not load,
/// [`Error::Party`], [`Error::Read`] or [`Error::Parse`] for the committed
/// records, and [`Error::Instrument`] when a chart's own input is
/// unrenderable.
pub fn performance_assets(
    root: &Path,
    results_path: &Path,
    stress_path: Option<&Path>,
) -> Result<PerformanceAssets, Error> {
    let loaded = load_clean_root(root)?;
    let results: Results = load_party_json(results_path, &results_schema(), "results.schema.json")?;
    let cases: Vec<PerformanceCase> = loaded
        .set
        .performance
        .iter()
        .map(|(_, c)| c.clone())
        .collect();
    let mut files = vec![RenderedFile {
        name: "perf-class-ladder.svg".to_owned(),
        body: crate::perf_assets::class_ladder_svg(&cases, &results.measurements),
    }];
    if let Some(path) = stress_path {
        let report: StressReport = read_json(path, &path.display().to_string())?;
        files.push(RenderedFile {
            name: "perf-stress-curve.svg".to_owned(),
            body: crate::perf_assets::stress_curve_svg(&report)
                .map_err(|e| Error::Instrument(format!("stress curve: {e}")))?,
        });
    }
    for measurement in &results.measurements {
        files.push(RenderedFile {
            name: format!("perf-latency-class-{}.svg", measurement.class.token()),
            body: crate::perf_assets::latency_percentiles_svg(measurement)
                .map_err(|e| Error::Instrument(format!("{}: {e}", measurement.case)))?,
        });
        // The resource time-series renders only from a record that carries
        // one (sampling is optional by capability; nothing is fabricated).
        if let Some(body) = crate::perf_assets::resources_timeseries_svg(measurement) {
            files.push(RenderedFile {
                name: format!("perf-resources-class-{}.svg", measurement.class.token()),
                body,
            });
        }
    }
    if let Some(body) = crate::perf_assets::disk_growth_svg(&results.measurements) {
        files.push(RenderedFile {
            name: "perf-disk-growth.svg".to_owned(),
            body,
        });
    }
    Ok(PerformanceAssets {
        files,
        cases,
        measurements: results.measurements,
    })
}

/// Renders the cross-SUT stress overlay from two committed stress reports.
///
/// Both systems are drawn on one canvas from their own recorded steps, which
/// puts the two directions on equal footing.
///
/// # Errors
/// [`Error::Read`] or [`Error::Parse`] for either report, and
/// [`Error::Instrument`] when the two cannot be drawn on one canvas.
pub fn stress_overlay(left: (&str, &Path), right: (&str, &Path)) -> Result<String, Error> {
    let (left_label, left_path) = left;
    let (right_label, right_path) = right;
    let left_report: StressReport = read_json(left_path, &left_path.display().to_string())?;
    let right_report: StressReport = read_json(right_path, &right_path.display().to_string())?;
    crate::perf_assets::stress_compare_svg((left_label, &left_report), (right_label, &right_report))
        .map_err(|e| Error::Instrument(format!("stress compare: {e}")))
}

/// Returns the published JSON-Schema set, one file per artifact family.
///
/// The rendering is byte-deterministic, so the emitted set is diffable
/// against the committed one.
#[must_use]
pub fn schema_files() -> Vec<RenderedFile> {
    crate::schema::emit_all()
        .into_iter()
        .map(|(name, schema)| RenderedFile {
            name: name.to_owned(),
            body: crate::schema::render(&schema),
        })
        .collect()
}
