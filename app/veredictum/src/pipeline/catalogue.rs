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
    validate_tree_reviewing(root, specs, None)
}

/// Loads an artifact root, adds one supplied DECLARATION to the pass, and runs
/// every machine gate over both.
///
/// `declaration` is the path of a submitted `statement.json`; the `ixit.json`
/// beside it joins the pass too. The gates that are relations between a claim
/// and the catalogue — the static conformance review — have a subject only
/// when one is supplied. ISO/IEC 9646-7 assigns the support and
/// supported-values columns of an ICS proforma to the supplier of the
/// implementation, so this repository authors the proforma
/// (`vocab/capability_matrix.yaml`) and never the answers.
///
/// # Errors
/// [`Error::Catalogue`] when the root itself cannot be opened, or on a
/// schema-compilation defect. A declaration that fails to load is reported as
/// a finding, not as an error.
pub fn validate_tree_reviewing(
    root: &Path,
    specs: Option<&Path>,
    declaration: Option<&Path>,
) -> Result<Validation, Error> {
    let mut loaded = crate::artifacts::load_root(root).map_err(Error::Catalogue)?;
    if let Some(path) = declaration {
        loaded.review_declaration(path).map_err(Error::Catalogue)?;
    }
    let findings = validate(&Context {
        set: &loaded.set,
        load_errors: &loaded.errors,
        spec_root: specs,
    });
    Ok(Validation { loaded, findings })
}

/// Returns where the wire-surface coverage report belongs for the catalogue
/// rooted at `root`: `<root>/coverage-report.md`.
///
/// The report enumerates what that artifact tree covers, so it lands beside
/// the artifact families it measures. Deriving it by climbing out of the spec
/// tree instead would put it wherever the caller happened to mount the specs,
/// which is a directory this repository does not own.
#[must_use]
pub fn coverage_report_path(root: &Path) -> PathBuf {
    root.join("coverage-report.md")
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

#[cfg(test)]
#[expect(
    clippy::panic_in_result_fn,
    reason = "Result-returning tests in the Book ch11 shape, each asserting; \
              clippy offers no allow-in-tests knob for this lint"
)]
mod tests {
    use super::{coverage_report_path, write_coverage_report};
    use crate::artifacts::load_root;
    use std::path::{Path, PathBuf};

    /// This package's directory is `app/veredictum`, so the repository root is
    /// two levels above it.
    fn repo_root() -> PathBuf {
        Path::new(concat!(env!("CARGO_MANIFEST_DIR"), "/../..")).to_path_buf()
    }

    #[test]
    fn the_report_home_is_a_directory_this_repository_has() {
        let root = repo_root().join("artifacts");
        let path = coverage_report_path(&root);

        assert_eq!(path, root.join("coverage-report.md"));
        let parent = path.parent().expect("a joined path should have a parent");
        assert!(
            parent.is_dir(),
            "the coverage report's home {} must be a directory that exists",
            parent.display()
        );
    }

    #[test]
    fn the_report_never_escapes_the_root_it_describes() {
        for root in [
            Path::new("artifacts"),
            Path::new("/srv/some/other/catalogue"),
            Path::new("../relative/root"),
        ] {
            let path = coverage_report_path(root);
            assert!(
                path.starts_with(root),
                "{} escaped its root {}",
                path.display(),
                root.display()
            );
            assert_eq!(path.parent(), Some(root));
        }
    }

    #[test]
    fn write_coverage_report_writes_at_the_derived_path() -> Result<(), Box<dyn std::error::Error>>
    {
        let out = assert_fs::TempDir::new()?;
        let loaded = load_root(&repo_root().join("artifacts"))?;
        let path = coverage_report_path(out.path());

        write_coverage_report(&loaded.set, &repo_root().join("specs/openehr"), &path)?;

        assert!(path.is_file(), "{} was not written", path.display());
        assert!(!std::fs::read_to_string(&path)?.is_empty());
        Ok(())
    }
}
