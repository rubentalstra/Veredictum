// SPDX-FileCopyrightText: Veredictum contributors
// SPDX-License-Identifier: Apache-2.0

//! The instrument's end-to-end seams, as a consumable API.
//!
//! Each module here is one whole operation: loading and validating a
//! catalogue ([`catalogue`]), driving it against a running system under test
//! ([`conformance`]), judging the recorded outcomes ([`judgement`]),
//! rendering the deterministic published assets ([`assets`]), and running the
//! measured instruments ([`measured`]). Every seam returns typed facts —
//! never pre-rendered console text — so a second consumer renders its own
//! views over the same values the command line prints.
//!
//! Two things stay outside: argument parsing and the rendering of results to
//! a console. A seam writes to the filesystem only where the write is part of
//! the computation, such as the measured run's merge into an existing
//! `results.json`; artifacts a seam has finished are handed back as
//! [`RenderedFile`] values for the caller to serve or write.

#![expect(
    clippy::disallowed_types,
    reason = "dev/verification tooling over JSON artifacts (the catalogue, results, wire \
              exchanges) — not the application (FerroEHR#1694)"
)]

pub mod assets;
pub mod catalogue;
pub mod conformance;
pub mod judgement;
pub mod measured;

use std::path::{Path, PathBuf};

use thiserror::Error;

use crate::load::{LoadError, compile_schema};

/// A finished artifact, named relative to the caller's output directory.
#[derive(Debug, Clone)]
pub struct RenderedFile {
    /// The file name, relative to whatever directory the caller writes into.
    pub name: String,
    /// The complete file body.
    pub body: String,
}

/// A failure of one pipeline seam.
///
/// Each variant renders the diagnostic the command line reports for that
/// failure, so a caller that only needs to show the problem can print the
/// error and stop.
#[derive(Debug, Error)]
pub enum Error {
    /// The artifact root could not be opened at all, which is a defect in
    /// the runner rather than in the tree it was pointed at.
    #[error("runner defect: {0}")]
    Catalogue(#[source] LoadError),
    /// Individual artifact files failed to load, one diagnostic per file.
    #[error("{}", join_lines(.0))]
    Artifacts(Vec<LoadError>),
    /// The artifact tree does not carry something the seam requires.
    #[error("{0}")]
    Missing(String),
    /// A file could not be read.
    #[error("cannot read {path}: {source}")]
    Read {
        /// The file that could not be read.
        path: PathBuf,
        /// The underlying filesystem error.
        #[source]
        source: std::io::Error,
    },
    /// A file could not be written.
    #[error("cannot write {path}: {source}")]
    Write {
        /// The file that could not be written.
        path: PathBuf,
        /// The underlying filesystem error.
        #[source]
        source: std::io::Error,
    },
    /// A directory could not be created.
    #[error("cannot create {path}: {source}")]
    CreateDir {
        /// The directory that could not be created.
        path: PathBuf,
        /// The underlying filesystem error.
        #[source]
        source: std::io::Error,
    },
    /// A document did not parse, named by whatever the reader calls it.
    #[error("{context}: {message}")]
    Parse {
        /// What the caller was reading.
        context: String,
        /// The parser's own diagnostic.
        message: String,
    },
    /// A party artifact failed its schema or typed-model stage; the message
    /// already names the file.
    #[error("{0}")]
    Party(String),
    /// A value could not be serialized back to JSON.
    #[error("{context}: {source}")]
    Serialize {
        /// What the caller was serializing.
        context: String,
        /// The serializer's own diagnostic.
        #[source]
        source: serde_json::Error,
    },
    /// The party results violate their own invariants, as read by the
    /// judging seam. Rendered with the same prefix as
    /// [`Error::RecordedInvariants`] so both seams report one violation the
    /// same way.
    #[error("{}", join_prefixed(.0, "results invariant: "))]
    ResultsInvariants(Vec<crate::party::PartyError>),
    /// The results a live run just produced violate their own invariants.
    #[error("{}", join_prefixed(.0, "results invariant: "))]
    RecordedInvariants(Vec<crate::party::PartyError>),
    /// A caller-supplied selector names something this instrument does not
    /// define, such as a class token that is not on the ladder.
    #[error("{0}")]
    Selector(String),
    /// A sub-instrument reported a failure; the message carries its own
    /// prefix so the caller can print it unchanged.
    #[error("{0}")]
    Instrument(String),
}

fn join_lines<T: std::fmt::Display>(items: &[T]) -> String {
    items
        .iter()
        .map(ToString::to_string)
        .collect::<Vec<_>>()
        .join("\n")
}

fn join_prefixed<T: std::fmt::Display>(items: &[T], prefix: &str) -> String {
    items
        .iter()
        .map(|item| format!("{prefix}{item}"))
        .collect::<Vec<_>>()
        .join("\n")
}

/// Loads one artifact root, refusing a tree whose files did not all load.
///
/// The two failure directions are kept apart on purpose: a root that cannot
/// be opened at all is a runner defect, while files that failed their own
/// load stages are reported one diagnostic per file.
///
/// # Errors
/// [`Error::Catalogue`] when the root itself cannot be opened, or
/// [`Error::Artifacts`] when any file under it failed to load.
pub fn load_clean_root(root: &Path) -> Result<crate::artifacts::Loaded, Error> {
    let loaded = crate::artifacts::load_root(root).map_err(Error::Catalogue)?;
    if loaded.errors.is_empty() {
        Ok(loaded)
    } else {
        Err(Error::Artifacts(loaded.errors))
    }
}

/// Loads one JSON party artifact, validating it against its emitted schema
/// before the typed parse.
///
/// The schema stage runs first so a malformed document is reported against
/// the published contract rather than as a `serde` type error.
///
/// # Errors
/// [`Error::Party`] naming the file and the stage that rejected it.
pub fn load_party_json<T: serde::de::DeserializeOwned>(
    path: &Path,
    schema: &serde_json::Value,
    schema_name: &str,
) -> Result<T, Error> {
    let text = std::fs::read_to_string(path)
        .map_err(|e| Error::Party(format!("{}: {e}", path.display())))?;
    let value: serde_json::Value = serde_json::from_str(&text)
        .map_err(|e| Error::Party(format!("{}: JSON: {e}", path.display())))?;
    let validator = compile_schema(schema, schema_name).map_err(|e| Error::Party(e.to_string()))?;
    let violations: Vec<String> = validator
        .iter_errors(&value)
        .map(|e| format!("{}: {e}", e.instance_path()))
        .collect();
    if !violations.is_empty() {
        return Err(Error::Party(format!(
            "{}: schema: {}",
            path.display(),
            violations.join("; ")
        )));
    }
    serde_json::from_value(value)
        .map_err(|e| Error::Party(format!("{}: model: {e}", path.display())))
}

/// Reads one ixit topology document, with its own text.
///
/// File references inside the document — the SMART lane's signing key — are
/// relative to the document, not to the caller's working directory, so they
/// are rebased before the topology is handed back. The raw text travels with
/// it because the campaign's ixit digest is taken over exactly these bytes.
///
/// # Errors
/// [`Error::Read`] when the file cannot be read, [`Error::Parse`] when it
/// does not parse as a topology.
pub fn load_ixit(path: &Path) -> Result<(crate::ixit::Ixit, String), Error> {
    let text = std::fs::read_to_string(path).map_err(|source| Error::Read {
        path: path.to_owned(),
        source,
    })?;
    let mut ixit: crate::ixit::Ixit = serde_json::from_str(&text).map_err(|e| Error::Parse {
        context: "ixit".to_owned(),
        message: e.to_string(),
    })?;
    ixit.rebase_paths(path.parent().unwrap_or_else(|| Path::new(".")));
    Ok((ixit, text))
}

/// Reads and typed-parses one JSON document, naming it as `context` in any
/// diagnostic.
///
/// # Errors
/// [`Error::Read`] when the file cannot be read, [`Error::Parse`] when it
/// does not parse as `T`.
pub fn read_json<T: serde::de::DeserializeOwned>(path: &Path, context: &str) -> Result<T, Error> {
    let text = std::fs::read_to_string(path).map_err(|source| Error::Read {
        path: path.to_owned(),
        source,
    })?;
    serde_json::from_str(&text).map_err(|e| Error::Parse {
        context: context.to_owned(),
        message: e.to_string(),
    })
}

/// Renders a value as the pretty JSON-with-trailing-newline form every
/// artifact this instrument writes uses.
///
/// # Errors
/// [`Error::Serialize`] carrying `context` when the value cannot be
/// serialized.
pub fn to_json_document<T: serde::Serialize>(value: &T, context: &str) -> Result<String, Error> {
    let mut text = serde_json::to_string_pretty(value).map_err(|source| Error::Serialize {
        context: context.to_owned(),
        source,
    })?;
    text.push('\n');
    Ok(text)
}

/// Creates a file's parent directory when it has one.
///
/// # Errors
/// [`Error::CreateDir`] naming the directory.
pub fn ensure_parent_dir(path: &Path) -> Result<(), Error> {
    let Some(parent) = path.parent() else {
        return Ok(());
    };
    std::fs::create_dir_all(parent).map_err(|source| Error::CreateDir {
        path: parent.to_owned(),
        source,
    })
}

/// Writes one file, creating nothing.
///
/// # Errors
/// [`Error::Write`] naming the file.
pub fn write_file(path: &Path, body: &str) -> Result<(), Error> {
    std::fs::write(path, body).map_err(|source| Error::Write {
        path: path.to_owned(),
        source,
    })
}
