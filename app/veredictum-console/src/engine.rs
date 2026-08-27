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
    /// Ask the engine for its `--progress` stream (#81); a binary predating
    /// the flag refuses the argument, so the caller decides.
    pub progress: bool,
    /// Ask the engine to persist the wire exchanges as `transcript.json`
    /// beside the results (#96). Off by default: the artifact can carry real
    /// patient data, so the operator opts in per run.
    pub record_exchanges: bool,
}

/// What to judge and seal, mirroring the `veredictum verdicts` CLI surface
/// one to one — this type ADDS nothing to it, which is the point of the seam.
///
/// The passphrase is deliberately absent: [`Engine::verdicts`] reads it from
/// the console's own environment at spawn time and puts it in the child's,
/// so it never lands in a struct anything could print or serialize.
#[derive(Debug)]
pub struct VerdictsSpec {
    /// The party statement the run was graded against.
    pub statement: PathBuf,
    /// The party results the run produced.
    pub results: PathBuf,
    /// The artifact root the judgement reads.
    pub root: PathBuf,
    /// The directory the rendered documents and the sealed set land in.
    pub out_dir: PathBuf,
    /// The armored secret key that seals the bundle, when one is mounted.
    pub sign_key: Option<PathBuf>,
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
    /// The judgement subcommand exited non-zero; the field is the engine's
    /// own diagnostic, verbatim.
    #[error("the engine refused the judgement: {diagnostic}")]
    Judgement {
        /// What the engine printed, trimmed.
        diagnostic: String,
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
    pub fn run(&self, spec: &RunSpec, on_line: impl FnMut(Line)) -> Result<Finished, Error> {
        let running = self.spawn(spec)?;
        running.stream(on_line)
    }

    /// Renders and seals the judgement documents by running the pinned
    /// binary's `verdicts` subcommand to completion.
    ///
    /// The sealing itself is the engine's (`veredictum::record`), never the
    /// console's: this call supplies the key path and reads back what the
    /// binary wrote. The passphrase is read from
    /// [`crate::state::SIGN_PASSPHRASE_ENV`] here and set on the child only,
    /// so it reaches no console state, no file and no log line.
    ///
    /// # Errors
    /// [`Error::Execute`] when the process cannot run, and
    /// [`Error::Judgement`] carrying the engine's own diagnostic when it
    /// exits non-zero.
    pub fn verdicts(&self, spec: &VerdictsSpec) -> Result<(), Error> {
        let mut command = std::process::Command::new(&self.binary);
        command
            .args(verdicts_args(spec))
            .stdin(std::process::Stdio::null());
        if let Ok(passphrase) = std::env::var(crate::state::SIGN_PASSPHRASE_ENV) {
            command.env(crate::state::SIGN_PASSPHRASE_ENV, passphrase);
        }
        let output = command.output().map_err(Error::Execute)?;
        if output.status.success() {
            return Ok(());
        }
        // stderr first, because the engine's refusals print there; stdout is
        // the fallback for a binary that reported on the other stream.
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_owned();
        let diagnostic = if stderr.is_empty() {
            String::from_utf8_lossy(&output.stdout).trim().to_owned()
        } else {
            stderr
        };
        Err(Error::Judgement { diagnostic })
    }

    /// Spawns one run and hands back the handle that streams it — the split
    /// [`Self::run`] composes, exposed so a job supervisor can hold the
    /// [`RunningEngine::canceller`] while another thread streams (#66).
    ///
    /// # Errors
    /// [`Error::Execute`] when the process cannot be spawned.
    pub fn spawn(&self, spec: &RunSpec) -> Result<RunningEngine, Error> {
        let mut command = std::process::Command::new(&self.binary);
        command
            .args(args(spec))
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped());
        for credential in &spec.credentials {
            command.env(&credential.name, &credential.value.0);
        }
        let child = command.spawn().map_err(Error::Execute)?;
        Ok(RunningEngine {
            child: std::sync::Arc::new(std::sync::Mutex::new(child)),
            out_dir: spec.out_dir.clone(),
        })
    }
}

/// A cancel handle onto a running engine, safe to hold on another thread.
#[derive(Debug, Clone)]
pub struct Canceller {
    child: std::sync::Arc<std::sync::Mutex<std::process::Child>>,
}

impl Canceller {
    /// Kills the subprocess; the streaming side then observes the exit.
    ///
    /// # Errors
    /// [`Error::Execute`] when the kill itself fails (an already-exited
    /// child kills cleanly, so this is rare).
    pub fn cancel(&self) -> Result<(), Error> {
        let mut child = self
            .child
            .lock()
            .map_err(|poison| Error::Execute(std::io::Error::other(poison.to_string())))?;
        child.kill().map_err(Error::Execute)
    }
}

/// One spawned run: stream it to completion, or cancel it from elsewhere.
#[derive(Debug)]
pub struct RunningEngine {
    child: std::sync::Arc<std::sync::Mutex<std::process::Child>>,
    out_dir: PathBuf,
}

impl RunningEngine {
    /// The cancel handle, cloneable across threads.
    #[must_use]
    pub fn canceller(&self) -> Canceller {
        Canceller {
            child: std::sync::Arc::clone(&self.child),
        }
    }

    /// Streams the run to completion, then parses the results record through
    /// the published lib.
    ///
    /// # Errors
    /// [`Error::Execute`] when the output cannot be read, [`Error::NoResults`]
    /// when the run leaves no results document (a cancelled run's ordinary
    /// shape), and [`Error::Malformed`] when that document does not parse as
    /// the published record.
    pub fn stream(self, mut on_line: impl FnMut(Line)) -> Result<Finished, Error> {
        // The pipes leave the child under a short lock; streaming itself
        // never holds it, so a canceller can take the lock and kill.
        let (stdout, stderr) = {
            let mut child = self
                .child
                .lock()
                .map_err(|poison| Error::Execute(std::io::Error::other(poison.to_string())))?;
            (child.stdout.take(), child.stderr.take())
        };
        // Both pipes are drained concurrently: a child that fills the
        // un-read pipe's buffer blocks forever, so stderr drains on its own
        // thread while this one reads stdout, and the lines merge over a
        // channel in arrival order.
        let (sender, receiver) = std::sync::mpsc::channel::<Line>();
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
        if let Some(stdout) = stdout {
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
        let status = {
            let mut child = self
                .child
                .lock()
                .map_err(|poison| Error::Execute(std::io::Error::other(poison.to_string())))?;
            child.wait().map_err(Error::Execute)?
        };
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

        let results_path = self.out_dir.join("results.json");
        let exceptions_path = self.out_dir.join("run-exceptions.json");
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
    if spec.record_exchanges {
        args.push("--record-exchanges".into());
    }
    if spec.progress {
        args.push("--progress".into());
    }
    args
}

/// Assembles the exact `veredictum verdicts` argument vector for a spec —
/// pure, so the mapping the export gate relies on is itself unit-tested.
#[must_use]
pub fn verdicts_args(spec: &VerdictsSpec) -> Vec<std::ffi::OsString> {
    let mut args: Vec<std::ffi::OsString> = vec![
        "verdicts".into(),
        "--statement".into(),
        spec.statement.clone().into(),
        "--results".into(),
        spec.results.clone().into(),
        "--root".into(),
        spec.root.clone().into(),
        "--out".into(),
        spec.out_dir.clone().into(),
    ];
    if let Some(key) = &spec.sign_key {
        args.push("--sign-key".into());
        args.push(key.clone().into());
    }
    args
}

#[cfg(test)]
mod tests {
    use super::{Credential, ENGINE_VERSION, RunSpec, Secret, VerdictsSpec, args, verdicts_args};

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

    /// One spec with every optional argument set, so the mapping is pinned
    /// argument for argument.
    fn full_spec() -> RunSpec {
        RunSpec {
            root: "artifacts".into(),
            ixit: "out/ixit.json".into(),
            out_dir: "out".into(),
            sut_name: "my-cdr".into(),
            sut_version: "1.2.3".into(),
            statement: Some("party/mine/statement.json".into()),
            filter: Some("create_ehr-main".into()),
            credentials: vec![],
            progress: true,
            record_exchanges: true,
        }
    }

    fn rendered(spec: &RunSpec) -> Vec<String> {
        args(spec)
            .into_iter()
            .map(|a| a.to_string_lossy().into_owned())
            .collect()
    }

    #[test]
    fn the_argument_vector_mirrors_the_cli_surface() {
        assert_eq!(
            rendered(&full_spec()),
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
                "--record-exchanges",
                "--progress",
            ]
        );
    }

    /// Recording is opt-in: an unchecked box passes no flag at all, so the
    /// engine's own default (no transcript) governs.
    #[test]
    fn an_unrecorded_run_passes_no_transcript_flag() {
        let spec = RunSpec {
            record_exchanges: false,
            ..full_spec()
        };
        assert!(
            !rendered(&spec).contains(&String::from("--record-exchanges")),
            "{:?}",
            rendered(&spec)
        );
    }

    #[test]
    fn the_judgement_argument_vector_mirrors_the_cli_surface() {
        let spec = VerdictsSpec {
            statement: "out/console-job-1/statement.json".into(),
            results: "out/console-job-1/results.json".into(),
            root: "artifacts".into(),
            out_dir: "out/console-job-1/export".into(),
            sign_key: Some("keys/cnf-signing.sec.asc".into()),
        };
        let rendered: Vec<String> = verdicts_args(&spec)
            .into_iter()
            .map(|a| a.to_string_lossy().into_owned())
            .collect();
        assert_eq!(
            rendered,
            [
                "verdicts",
                "--statement",
                "out/console-job-1/statement.json",
                "--results",
                "out/console-job-1/results.json",
                "--root",
                "artifacts",
                "--out",
                "out/console-job-1/export",
                "--sign-key",
                "keys/cnf-signing.sec.asc",
            ]
        );
    }

    /// No key mounted means no `--sign-key`, so the engine renders the
    /// documents unsealed rather than being handed an empty path.
    #[test]
    fn the_judgement_vector_omits_the_key_when_none_is_mounted() {
        let spec = VerdictsSpec {
            statement: "s.json".into(),
            results: "r.json".into(),
            root: "artifacts".into(),
            out_dir: "export".into(),
            sign_key: None,
        };
        let rendered: Vec<String> = verdicts_args(&spec)
            .into_iter()
            .map(|a| a.to_string_lossy().into_owned())
            .collect();
        assert!(
            !rendered.contains(&String::from("--sign-key")),
            "{rendered:?}"
        );
    }

    /// The passphrase never becomes a command-line argument: it is visible to
    /// every process on the host there, which is why the CLI takes it from
    /// the environment.
    #[test]
    fn the_judgement_vector_never_carries_a_passphrase() {
        let spec = VerdictsSpec {
            statement: "s.json".into(),
            results: "r.json".into(),
            root: "artifacts".into(),
            out_dir: "export".into(),
            sign_key: Some("k.asc".into()),
        };
        let rendered: Vec<String> = verdicts_args(&spec)
            .into_iter()
            .map(|a| a.to_string_lossy().into_owned())
            .collect();
        assert!(
            !rendered.iter().any(|a| a.contains("passphrase")),
            "{rendered:?}"
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
