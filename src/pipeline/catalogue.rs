// SPDX-FileCopyrightText: Veredictum contributors
// SPDX-License-Identifier: Apache-2.0

//! Loading and validating one catalogue tree.
//!
//! This is the gate the catalogue lives or dies by: every machine check over
//! the artifact tree, with zero findings as the only passing result. Files
//! that failed to load are not a hard failure here — they become findings of
//! their own, so one pass reports the whole tree.

use std::path::{Path, PathBuf};

use crate::artifacts::{ArtifactSet, Loaded};
use crate::pipeline::Error;
use crate::validate::{Context, Finding, render_coverage_report, validate};

/// One validation pass over an artifact root.
#[derive(Debug)]
pub struct Validation {
    /// Everything the tree loaded, whether or not the gates accepted it.
    pub loaded: Loaded,
    /// Every violation, in check order.
    pub findings: Vec<Finding>,
}

impl Validation {
    /// Returns whether the tree passed, which means no finding at all.
    #[must_use]
    pub fn is_clean(&self) -> bool {
        self.findings.is_empty()
    }
}

/// Loads an artifact root and runs every machine gate over it.
///
/// Supplying `specs` — the vendored openEHR spec tree — additionally enables
/// the citation-resolution and SM-operation gates, which cannot run without
/// the oracle to resolve against.
///
/// # Errors
/// [`Error::Catalogue`] when the root itself cannot be opened. Files that
/// fail their own load stages are reported as findings, not as an error.
pub fn validate_tree(root: &Path, specs: Option<&Path>) -> Result<Validation, Error> {
    let loaded = crate::artifacts::load_root(root).map_err(Error::Catalogue)?;
    let findings = validate(&Context {
        set: &loaded.set,
        load_errors: &loaded.errors,
        spec_root: specs,
    });
    Ok(Validation { loaded, findings })
}

/// Returns where the wire-surface coverage report belongs, derived from the
/// vendored spec tree it is measured against.
///
/// The report lives beside the committed conformance artifacts, two levels
/// above the spec component directory, so the path follows whichever tree
/// the caller validated against.
#[must_use]
pub fn coverage_report_path(specs: &Path) -> Option<PathBuf> {
    specs
        .parent()
        .and_then(Path::parent)
        .map(|docs| docs.join("conformance/coverage-report.md"))
}

/// Renders the wire-surface coverage report and writes it to `path`.
///
/// # Errors
/// [`Error::Write`] naming `path`, whether the directory or the file itself
/// could not be created.
pub fn write_coverage_report(set: &ArtifactSet, specs: &Path, path: &Path) -> Result<(), Error> {
    let body = render_coverage_report(set, Some(specs));
    path.parent()
        .map_or(Ok(()), std::fs::create_dir_all)
        .and_then(|()| std::fs::write(path, body))
        .map_err(|source| Error::Write {
            path: path.to_owned(),
            source,
        })
}
