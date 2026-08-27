// SPDX-FileCopyrightText: Veredictum contributors
// SPDX-License-Identifier: Apache-2.0

//! The console's server state: the mounted roots, and the catalogue loaded
//! through the published lib ONCE at startup.
//!
//! The console stores nothing of its own (the crate CLAUDE.md law): this
//! state is a read of the mounts, and a restart re-reads them. A missing or
//! unreadable catalogue is a FIRST-CLASS state the screens render as the
//! named-mount explanation — the server still serves.

use std::path::PathBuf;
use std::sync::Arc;

/// The environment variable naming the artifact root (default `artifacts`;
/// the image sets it to the documented `/work` mount).
pub const ROOT_ENV: &str = "VEREDICTUM_ROOT";

/// The environment variable naming the vendored spec tree (default
/// `specs/openehr`; the image sets it to the documented `/work` mount).
pub const SPECS_ENV: &str = "VEREDICTUM_SPECS";

/// The environment variable naming the party-declaration tree (default
/// `party`; the image sets it to the documented `/work` mount).
pub const PARTY_ENV: &str = "VEREDICTUM_PARTY";

/// The environment variable naming the run-output tree (default `out`; the
/// image sets it to the documented `/work` mount). Runs write here exactly
/// as a terminal run would.
pub const OUT_ENV: &str = "VEREDICTUM_OUT";

/// The loaded catalogue, or the explanation of why there is none.
#[derive(Debug, Clone)]
pub struct ConsoleState {
    /// The artifact root the console reads.
    pub root: PathBuf,
    /// The vendored spec tree citations resolve against.
    pub specs: PathBuf,
    /// The party-declaration tree statements are picked from.
    pub party: PathBuf,
    /// The run-output tree job artifacts land under.
    pub out: PathBuf,
    /// The one startup validation pass, shared by every request; `Err` is
    /// the verbatim reason the catalogue could not be opened.
    pub catalogue: Arc<Result<veredictum::pipeline::catalogue::Validation, String>>,
    /// The one in-flight run draft (the wizard's server-side memory): the
    /// console holds at most one, and a restart legitimately forgets it —
    /// no console-local store exists (the crate CLAUDE.md law).
    pub draft: Arc<std::sync::Mutex<Option<crate::run_api::RunDraft>>>,
    /// The one run-job slot (#66).
    pub jobs: crate::run_job::JobSlot,
}

impl ConsoleState {
    /// Reads the mounts from the environment and loads the catalogue once,
    /// through the published lib — the same call `validate` runs.
    #[must_use]
    pub fn load() -> Self {
        let root =
            PathBuf::from(std::env::var(ROOT_ENV).unwrap_or_else(|_| String::from("artifacts")));
        let specs = PathBuf::from(
            std::env::var(SPECS_ENV).unwrap_or_else(|_| String::from("specs/openehr")),
        );
        let party =
            PathBuf::from(std::env::var(PARTY_ENV).unwrap_or_else(|_| String::from("party")));
        let out = PathBuf::from(std::env::var(OUT_ENV).unwrap_or_else(|_| String::from("out")));
        let catalogue = veredictum::pipeline::catalogue::validate_tree(&root, Some(&specs))
            .map_err(|e| e.to_string());
        Self {
            root,
            specs,
            party,
            out,
            catalogue: Arc::new(catalogue),
            draft: Arc::new(std::sync::Mutex::new(None)),
            jobs: crate::run_job::JobSlot::default(),
        }
    }
}
