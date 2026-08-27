// SPDX-FileCopyrightText: Veredictum contributors
// SPDX-License-Identifier: Apache-2.0

//! The engine seam (#54): the one place the console touches the instrument.
//!
//! Runs execute through the pinned, published `veredictum` binary as a
//! subprocess, so a run's output stays byte-identical to the same run from a
//! terminal — the property `tests/it/engine_gate.rs` checks by diffing the
//! two paths' emitted documents. Reads parse through the published lib's
//! typed model, so no schema is reimplemented console-side. Nothing here
//! speaks to a CDR: the spawned instrument is the only thing that touches
//! the SUT, and the SUT credentials the console holds in memory reach only
//! the spawned process's environment — never a file, a log line, or any
//! client-visible state.

use std::io::BufRead as _;
use std::path::{Path, PathBuf};

/// The exact engine version this console is built against.
///
/// One fact with the manifest's crates.io pin and the lib-level
/// [`crate::ENGINE_PIN`] the chrome displays; the unit test below locks the
/// pin to the manifest, and [`Engine::verified`] refuses a binary that
/// reports anything else, so the "one engine" property cannot rot into
/// "whichever binary was on PATH".
pub const ENGINE_VERSION: &str = crate::ENGINE_PIN;

/// The environment variable that names the engine binary explicitly. Without
/// it, [`locate`] falls back to `veredictum` on `PATH`.
pub const ENGINE_ENV: &str = "VEREDICTUM_ENGINE";

/// A secret value on its way into a spawned run's environment.
///
/// The wrapper exists for what it refuses: `Debug` prints a redaction, the
/// value is never serialized, and the only read path is the environment of
/// the child process (`Engine::run`).
pub struct Secret(String);

impl Secret {
    /// Wraps a secret value.
    #[must_use]
    pub fn new(value: String) -> Self {
        Self(value)
    }
}

impl std::fmt::Debug for Secret {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("Secret(«redacted»)")
    }
}

/// One credential the ixit declares by environment-variable NAME and the
/// console supplies by value, in memory, for exactly one spawned run.
#[derive(Debug)]
pub struct Credential {
    /// The environment-variable name the ixit references (`user_env`,
    /// `password_env`, `token_env`).
    pub name: String,
    /// The value, redacted from every rendering.
    pub value: Secret,
}

/// What to drive, mirroring the `veredictum run` CLI surface one to one —
/// this type ADDS nothing to it, which is the point of the seam.
#[derive(Debug)]
pub struct RunSpec {
    /// The artifact root the run reads.
    pub root: PathBuf,
    /// The ixit topology document (already written; carries env var NAMES,
    /// never values).
    pub ixit: PathBuf,
    /// The output directory the run writes into.
    pub out_dir: PathBuf,
    /// The SUT display name recorded in the results.
    pub sut_name: String,
    /// The SUT version label recorded in the results.
    pub sut_version: String,
    /// The party statement, when test selection should apply.
    pub statement: Option<PathBuf>,
    /// Drive only cases whose id contains this substring.
    pub filter: Option<String>,
    /// The secret values for the environment-variable names the ixit
    /// declares. They reach the child process environment and nothing else.
    pub credentials: Vec<Credential>,
}

/// One line of the running engine's own output, as it happens.
#[derive(Debug)]
pub enum Line {
    /// A stdout line.
    Out(String),
    /// A stderr line.
    Err(String),
}

/// A finished run: the exit status, where the documents landed, and the
/// results record parsed through the published lib's typed model.
#[derive(Debug)]
pub struct Finished {
    /// Whether the engine reported a clean campaign (exit status success).
    pub clean_exit: bool,
    /// The party results record, typed by the published `veredictum` lib —
    /// never a console-side re-parse.
    pub results: veredictum::party::Results,
    /// Where `results.json` landed.
    pub results_path: PathBuf,
    /// Where `run-exceptions.json` landed.
    pub exceptions_path: PathBuf,
}

/// Everything the seam can refuse, typed at the boundary that branches.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    /// No engine binary was found at the override path or on `PATH`.
    #[error("no engine binary: {0}")]
    NotFound(String),
    /// The version probe could not run.
    #[error("the engine version probe failed: {0}")]
    Probe(#[source] std::io::Error),
    /// The binary answered the probe with a different identity.
    #[error(
        "engine version mismatch: expected `veredictum {ENGINE_VERSION}`, the binary reports `{reported}`"
    )]
    VersionMismatch {
        /// What `--version` printed, trimmed.
        reported: String,
    },
    /// The run could not be spawned or streamed.
    #[error("the engine run failed to execute: {0}")]
    Execute(#[source] std::io::Error),
    /// The run finished but left no readable results document.
    #[error("the run left no readable results document at {path}: {source}")]
    NoResults {
        /// Where the document was expected.
        path: PathBuf,
        /// Why it could not be read.
        #[source]
        source: std::io::Error,
    },
    /// The results document did not parse as the published record.
    #[error("the results document at {path} does not parse as the published record: {source}")]
    Malformed {
        /// The document that failed.
        path: PathBuf,
        /// The parse failure.
        #[source]
        source: serde_json::Error,
    },
}

/// The located, version-verified engine binary.
#[derive(Debug, Clone)]
pub struct Engine {
    binary: PathBuf,
}

impl Engine {
    /// Verifies the binary at `path` identifies as the pinned engine and
    /// wraps it.
    ///
    /// # Errors
    /// [`Error::Probe`] when `--version` cannot run, and
    /// [`Error::VersionMismatch`] when it reports anything other than
    /// `veredictum {ENGINE_VERSION}`.
    pub fn verified(path: &Path) -> Result<Self, Error> {
        let output = std::process::Command::new(path)
            .arg("--version")
            .output()
            .map_err(Error::Probe)?;
        let reported = String::from_utf8_lossy(&output.stdout).trim().to_owned();
        if reported == format!("veredictum {ENGINE_VERSION}") {
            Ok(Self {
                binary: path.to_path_buf(),
            })
        } else {
            Err(Error::VersionMismatch { reported })
        }
    }

    /// The verified binary's path.
    #[must_use]
    pub fn binary(&self) -> &Path {
        &self.binary
    }

    /// Drives one run through the engine, streaming each output line to
    /// `on_line` as it happens, then parses the results record through the
    /// published lib.
    ///
    /// The credentials reach the child process environment and nothing
    /// else; the child's environment is otherwise inherited, which is what
    /// lets an operator pass proxy settings the ordinary way.
    ///
    /// # Errors
    /// [`Error::Execute`] when the process cannot be spawned or its output
    /// cannot be read, [`Error::NoResults`] when the run leaves no results
    /// document, and [`Error::Malformed`] when that document does not parse
    /// as the published record.
    pub fn run(&self, spec: &RunSpec, mut on_line: impl FnMut(Line)) -> Result<Finished, Error> {
        let mut command = std::process::Command::new(&self.binary);
        command
            .args(args(spec))
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped());
        for credential in &spec.credentials {
            command.env(&credential.name, &credential.value.0);
        }

        let mut child = command.spawn().map_err(Error::Execute)?;
        // Both pipes are drained concurrently: a child that fills the
        // un-read pipe's buffer blocks forever, so stderr drains on its own
        // thread while this one reads stdout, and the lines merge over a
        // channel in arrival order.
        let (sender, receiver) = std::sync::mpsc::channel::<Line>();
        let stderr = child.stderr.take();
        let stderr_reader = std::thread::spawn({
            let sender = sender.clone();
            move || {
                if let Some(stderr) = stderr {
                    for line in std::io::BufReader::new(stderr).lines() {
                        let Ok(line) = line else { break };
                        if sender.send(Line::Err(line)).is_err() {
                            break;
                        }
                    }
                }
            }
        });
        if let Some(stdout) = child.stdout.take() {
            for line in std::io::BufReader::new(stdout).lines() {
                let line = line.map_err(Error::Execute)?;
                on_line(Line::Out(line));
                // Deliver whatever stderr produced meanwhile, keeping the
                // merged stream close to arrival order.
                while let Ok(err_line) = receiver.try_recv() {
                    on_line(err_line);
                }
            }
        }
        let status = child.wait().map_err(Error::Execute)?;
        // The guard is joined, never dropped mid-stream: the remaining
        // stderr lines are delivered before the run is reported finished.
        let joined = stderr_reader.join();
        drop(sender);
        while let Ok(err_line) = receiver.try_recv() {
            on_line(err_line);
        }
        if joined.is_err() {
            on_line(Line::Err(String::from(
                "console: the stderr reader thread panicked; its tail was lost",
            )));
        }

        let results_path = spec.out_dir.join("results.json");
        let exceptions_path = spec.out_dir.join("run-exceptions.json");
        let body = std::fs::read_to_string(&results_path).map_err(|source| Error::NoResults {
            path: results_path.clone(),
            source,
        })?;
        let results: veredictum::party::Results =
            serde_json::from_str(&body).map_err(|source| Error::Malformed {
                path: results_path.clone(),
                source,
            })?;
        Ok(Finished {
            clean_exit: status.success(),
            results,
            results_path,
            exceptions_path,
        })
    }
}

/// Locates the engine: the [`ENGINE_ENV`] override when set, otherwise
/// `veredictum` on `PATH` — verified either way.
///
/// # Errors
/// [`Error::NotFound`] when neither names an existing binary, plus
/// everything [`Engine::verified`] refuses.
pub fn locate() -> Result<Engine, Error> {
    if let Ok(explicit) = std::env::var(ENGINE_ENV) {
        return Engine::verified(Path::new(&explicit));
    }
    match Engine::verified(Path::new("veredictum")) {
        Err(Error::Probe(source)) if source.kind() == std::io::ErrorKind::NotFound => {
            Err(Error::NotFound(format!(
                "`veredictum` is not on PATH and {ENGINE_ENV} is unset"
            )))
        }
        other => other,
    }
}

/// Assembles the exact `veredictum run` argument vector for a spec — pure,
/// so the mapping the gate test relies on is itself unit-tested.
#[must_use]
pub fn args(spec: &RunSpec) -> Vec<std::ffi::OsString> {
    let mut args: Vec<std::ffi::OsString> = vec![
        "run".into(),
        "--root".into(),
        spec.root.clone().into(),
        "--ixit".into(),
        spec.ixit.clone().into(),
        "--out".into(),
        spec.out_dir.clone().into(),
        "--sut-name".into(),
        spec.sut_name.clone().into(),
        "--sut-version".into(),
        spec.sut_version.clone().into(),
    ];
    if let Some(statement) = &spec.statement {
        args.push("--statement".into());
        args.push(statement.clone().into());
    }
    if let Some(filter) = &spec.filter {
        args.push("--filter".into());
        args.push(filter.clone().into());
    }
    args
}

#[cfg(test)]
mod tests {
    use super::{Credential, ENGINE_VERSION, RunSpec, Secret, args};

    /// The manifest's crates.io pin and [`ENGINE_VERSION`] are one fact in
    /// two places; this is the lock between them.
    #[test]
    fn the_pin_and_the_constant_agree() {
        let manifest = include_str!("../Cargo.toml");
        let line = manifest
            .lines()
            .find(|l| l.starts_with("veredictum = "))
            .expect("the manifest pins the engine");
        assert!(
            line.contains(&format!("\"={ENGINE_VERSION}\"")),
            "ENGINE_VERSION ({ENGINE_VERSION}) must equal the manifest's exact pin: {line}"
        );
    }

    #[test]
    fn the_argument_vector_mirrors_the_cli_surface() {
        let spec = RunSpec {
            root: "artifacts".into(),
            ixit: "out/ixit.json".into(),
            out_dir: "out".into(),
            sut_name: "my-cdr".into(),
            sut_version: "1.2.3".into(),
            statement: Some("party/mine/statement.json".into()),
            filter: Some("create_ehr-main".into()),
            credentials: vec![],
        };
        let rendered: Vec<String> = args(&spec)
            .into_iter()
            .map(|a| a.to_string_lossy().into_owned())
            .collect();
        assert_eq!(
            rendered,
            [
                "run",
                "--root",
                "artifacts",
                "--ixit",
                "out/ixit.json",
                "--out",
                "out",
                "--sut-name",
                "my-cdr",
                "--sut-version",
                "1.2.3",
                "--statement",
                "party/mine/statement.json",
                "--filter",
                "create_ehr-main",
            ]
        );
    }

    #[test]
    fn a_secret_debug_rendering_carries_no_value() {
        let credential = Credential {
            name: String::from("SUT_PASS"),
            value: Secret::new(String::from("hunter2")),
        };
        let rendered = format!("{credential:?}");
        assert!(!rendered.contains("hunter2"), "leaked: {rendered}");
        assert!(rendered.contains("redacted"));
    }
}
