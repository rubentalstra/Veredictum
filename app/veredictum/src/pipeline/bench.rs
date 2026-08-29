// SPDX-FileCopyrightText: Veredictum contributors
// SPDX-License-Identifier: Apache-2.0

//! The universal-benchmark seams: describe the embedded packs, drive one
//! against a base URL, and align several committed results into one
//! comparison.
//!
//! Both seams return typed facts plus finished [`RenderedFile`] values, so a
//! second consumer writes and displays them however it likes. Nothing here
//! touches an artifact root, an ixit, or a party statement: a bench run is
//! deliberately free of the catalogue.

use std::path::PathBuf;

use crate::bench::baselines::{BaselineRun, DockerCli, run_baselines};
use crate::bench::client::AuthKind;
use crate::bench::compare::Comparison;
use crate::bench::manifest::{MANIFEST_FILE, PackManifest};
use crate::bench::pack::BenchPack;
use crate::bench::posture::PostureProfile;
use crate::bench::render::COMPARISON_FILE;
use crate::bench::result::BenchResult;
use crate::bench::run::BenchRun;
use crate::bench::{compare, render, run};
use crate::pipeline::{Error, RenderedFile};

/// The described packs plus the document that carries them.
#[derive(Debug)]
pub struct ManifestOutcome {
    /// The manifest itself.
    pub manifest: PackManifest,
    /// `bench-packs.json`, ready to write.
    pub document: RenderedFile,
}

/// Describes every embedded pack as one byte-deterministic document.
///
/// The packs are versioned data compiled into this binary, so this seam is
/// the only source a rendered description may be built from: anything else is
/// a second copy that drifts the first time a pack version moves.
///
/// # Errors
/// [`Error::Instrument`] carrying the engine's diagnostic when an embedded
/// fixture's bytes no longer hash to their pin, or when the document cannot
/// be serialized.
pub fn describe_packs() -> Result<ManifestOutcome, Error> {
    let manifest = PackManifest::of_embedded()
        .map_err(|error| Error::Instrument(format!("bench: {error}")))?;
    let body = manifest
        .to_document()
        .map_err(|error| Error::Instrument(format!("bench: {error}")))?;
    Ok(ManifestOutcome {
        manifest,
        document: RenderedFile {
            name: MANIFEST_FILE.to_owned(),
            body,
        },
    })
}

/// What a `bench` invocation asks for.
#[derive(Debug)]
pub struct BenchRequest<'a> {
    /// The embedded pack to drive.
    pub pack: &'a BenchPack,
    /// The posture profile the run declares, out of the pack's own set.
    pub profile: &'a PostureProfile,
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
    /// Multiplies every seed phase's EHR count. `1.0` is the pack's pinned
    /// population.
    pub scale: f64,
    /// Overrides every seed phase's declared worker count.
    pub seed_workers: Option<usize>,
    /// Whether to compose and measure the pinned reference CDRs on this host
    /// after the target's run, which is what makes the record submittable.
    pub with_baselines: bool,
    /// How the baseline orchestration reaches the container runtime. `None`
    /// takes the `docker` CLI from `PATH`.
    pub docker: Option<DockerCli>,
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
/// With `with_baselines` set the same pack, at the same seed and the same
/// repetition count, is then driven against every pinned reference CDR on
/// this host, and the record carries all of them plus the relative index
/// derived from them.
///
/// # Errors
/// [`Error::Instrument`] carrying the engine's own diagnostic, which already
/// names the exchange, the phase, the posture item, or the baseline that
/// failed. A missing container runtime is refused before the target is
/// touched, so the flag never produces a half-anchored record.
pub fn run_bench(
    request: &BenchRequest<'_>,
    progress: &(dyn Fn(String) + Sync),
) -> Result<BenchOutcome, Error> {
    let docker = request.docker.clone().unwrap_or_default();
    if request.with_baselines {
        let _version = docker
            .probe()
            .map_err(|error| Error::Instrument(format!("bench: {error}")))?;
    }
    let mut result = run::execute(
        &BenchRun {
            pack: request.pack,
            base_url: request.base_url,
            profile: request.profile,
            auth: request.auth,
            user: request.user,
            credential: None,
            repetitions: request.repetitions,
            label: request.label,
            scale: request.scale,
            seed_workers: request.seed_workers,
        },
        progress,
    )
    .map_err(|error| Error::Instrument(format!("bench: {error}")))?;
    if request.with_baselines {
        let baselines = run_baselines(
            &BaselineRun {
                pack: request.pack,
                profile: request.profile,
                repetitions: request.repetitions,
                scale: request.scale,
                seed_workers: request.seed_workers,
                docker: &docker,
            },
            progress,
        )
        .map_err(|error| Error::Instrument(format!("bench: {error}")))?;
        result.attach_baselines(baselines);
    }
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
