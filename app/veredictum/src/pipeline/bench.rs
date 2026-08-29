// SPDX-FileCopyrightText: Veredictum contributors
// SPDX-License-Identifier: Apache-2.0

//! The universal-benchmark seams: drive a pack against a base URL, and align
//! several committed results into one comparison.
//!
//! Both seams return typed facts plus finished [`RenderedFile`] values, so a
//! second consumer writes and displays them however it likes. Nothing here
//! touches an artifact root, an ixit, or a party statement: a bench run is
//! deliberately free of the catalogue.

use std::path::PathBuf;

use crate::bench::client::AuthKind;
use crate::bench::compare::Comparison;
use crate::bench::pack::BenchPack;
use crate::bench::render::COMPARISON_FILE;
use crate::bench::result::BenchResult;
use crate::bench::run::BenchRun;
use crate::bench::{compare, render, run};
use crate::pipeline::{Error, RenderedFile};

/// What a `bench` invocation asks for.
#[derive(Debug)]
pub struct BenchRequest<'a> {
    /// The embedded pack to drive.
    pub pack: &'a BenchPack,
    /// The system's base URL.
    pub base_url: &'a str,
    /// How the client presents itself.
    pub auth: AuthKind,
    /// The user `--auth basic` needs.
    pub user: Option<&'a str>,
    /// How many times to repeat the measured phases.
    pub repetitions: u32,
    /// The operator's label for the run.
    pub label: Option<&'a str>,
}

/// One finished bench run: the record, plus the two files it emits.
#[derive(Debug)]
pub struct BenchOutcome {
    /// The record itself.
    pub result: BenchResult,
    /// The result document and its rendered summary, ready to write.
    pub documents: Vec<RenderedFile>,
}

/// Drives one pack against a live system and returns its record.
///
/// # Errors
/// [`Error::Instrument`] carrying the engine's own diagnostic, which already
/// names the exchange or the phase that failed.
pub fn run_bench(
    request: &BenchRequest<'_>,
    progress: &(dyn Fn(String) + Sync),
) -> Result<BenchOutcome, Error> {
    let result = run::execute(
        &BenchRun {
            pack: request.pack,
            base_url: request.base_url,
            auth: request.auth,
            user: request.user,
            repetitions: request.repetitions,
            label: request.label,
        },
        progress,
    )
    .map_err(|error| Error::Instrument(format!("bench: {error}")))?;
    let document = result
        .to_document()
        .map_err(|error| Error::Instrument(format!("bench: {error}")))?;
    let summary = render::run_summary(&result);
    let stem = result.file_name();
    let markdown = stem.replace(".json", ".md");
    Ok(BenchOutcome {
        documents: vec![
            RenderedFile {
                name: stem,
                body: document,
            },
            RenderedFile {
                name: markdown,
                body: summary,
            },
        ],
        result,
    })
}

/// One finished comparison: the aligned table plus its rendered document.
#[derive(Debug)]
pub struct ComparisonOutcome {
    /// The aligned table.
    pub comparison: Comparison,
    /// The rendered Markdown, ready to write and to print.
    pub document: RenderedFile,
}

/// Aligns two or more committed bench results.
///
/// # Errors
/// [`Error::Instrument`] when fewer than two files were given, or when one of
/// them is unreadable or is not a bench result.
pub fn compare_bench(paths: &[PathBuf]) -> Result<ComparisonOutcome, Error> {
    let comparison =
        compare::compare(paths).map_err(|error| Error::Instrument(format!("bench: {error}")))?;
    let body = render::comparison(&comparison);
    Ok(ComparisonOutcome {
        comparison,
        document: RenderedFile {
            name: COMPARISON_FILE.to_owned(),
            body,
        },
    })
}
