// SPDX-FileCopyrightText: Veredictum contributors
// SPDX-License-Identifier: Apache-2.0

//! The console's server state: the mounted roots, and the catalogue loaded
//! through the published lib ONCE at startup.
//!
//! The console stores nothing of its own: this
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

/// The environment variable naming the run-output tree (default `out`; the
/// image sets it to the documented `/work` mount). Runs write here exactly
/// as a terminal run would.
pub const OUT_ENV: &str = "VEREDICTUM_OUT";

/// The environment variable naming the armored OpenPGP secret key the export
/// seals with.
///
/// Unset is a first-class state: the export section then renders the
/// instruction to mount a key, and offers no button.
pub const SIGN_KEY_ENV: &str = "VEREDICTUM_SIGN_KEY";

/// The environment variable naming the armored OpenPGP public key `/verify`
/// checks a bundle against.
///
/// Unset is a first-class state: the page explains what to configure.
pub const VERIFY_KEY_ENV: &str = "VEREDICTUM_VERIFY_KEY";

/// The environment variable stating how many runs this host can drive at once.
///
/// Unset is [`crate::run_job::MAX_CONCURRENT_RUNS`], which #388 reasoned about
/// and called a starting value to re-derive by measuring on the chosen host.
/// This is how a host states what it actually has: a 2 GB box drives one run,
/// not two, and the queue covers the difference with a position rather than a
/// refusal.
///
/// A value that is not a positive integer refuses to start. A cap is a safety
/// property, and falling back to a larger default on a typo would let a box
/// admit work it cannot hold — which the OOM killer then resolves, halfway
/// through somebody's conformance run.
pub const MAX_CONCURRENT_RUNS_ENV: &str = "VEREDICTUM_MAX_CONCURRENT_RUNS";

/// The environment variable naming the request header that carries the real
/// client address, for a deployment behind a proxy.
///
/// Unset is the default and the safe state: the socket peer is then the whole
/// answer, and no forwarded header is read at all. A header is trusted only
/// because the operator named it (the hosted deployment sets `Fly-Client-IP`),
/// since an unconditionally trusted `X-Forwarded-For` would let any visitor
/// claim any identity and defeat every per-submitter cap.
pub const CLIENT_IP_HEADER_ENV: &str = "VEREDICTUM_CLIENT_IP_HEADER";

/// The environment variable carrying the passphrase that unlocks
/// [`SIGN_KEY_ENV`].
///
/// The console never reads this into its own state: it is read at spawn time
/// and placed in the child process's environment, which is the same variable
/// the pinned CLI already documents. It reaches no signal, no file and no log
/// line.
pub const SIGN_PASSPHRASE_ENV: &str = "VEREDICTUM_SIGN_PASSPHRASE";

/// The environment variable naming the registry App's numeric app id (#391).
///
/// Unset is a first-class state: the submit screen then explains what to
/// configure and offers no button. The four registry variables are read at the
/// moment a submission is composed rather than cached here, because the App
/// identity is a credential posture like the signing key and an operator who
/// mounts it must not have to restart the instance to use it.
pub const REGISTRY_APP_ID_ENV: &str = "VEREDICTUM_GITHUB_APP_ID";

/// The environment variable naming the PEM file holding the registry App's
/// private key.
///
/// The console holds the PATH, never the key bytes: the file is read at the
/// moment a JWT is minted and the material reaches no state, no signal and no
/// log line, exactly as [`SIGN_KEY_ENV`] does.
pub const REGISTRY_APP_KEY_ENV: &str = "VEREDICTUM_GITHUB_APP_KEY";

/// The environment variable naming the App's installation id on the registry
/// repository.
pub const REGISTRY_INSTALLATION_ENV: &str = "VEREDICTUM_GITHUB_INSTALLATION_ID";

/// The environment variable naming the registry repository, `owner/name`.
pub const REGISTRY_REPO_ENV: &str = "VEREDICTUM_REGISTRY_REPO";

/// The environment variable naming the GitHub REST API root.
///
/// Optional, defaulting to the public API. It exists so a GitHub Enterprise
/// deployment can name its own root, and so the client's request sequence is
/// assertable against a stub server with no network at all.
pub const REGISTRY_API_ENV: &str = "VEREDICTUM_GITHUB_API";

/// Why the console refused to start.
///
/// Two values, and only two, that this console will not guess at. Everything
/// else missing is a first-class state a screen explains; these two are safety
/// properties, and a wrong guess about either lets the instrument do something
/// it should not.
#[derive(Debug, thiserror::Error)]
pub enum StartupError {
    /// The posture is a value that names no posture.
    #[error(transparent)]
    Posture(#[from] crate::posture::UnknownPosture),
    /// The concurrency cap is not a positive integer.
    #[error(
        "{env}={value:?} is not a positive integer, and a concurrency cap is not a value to guess at: a box that admits more runs than it can hold has the OOM killer resolve the difference, halfway through somebody's conformance run"
    )]
    Concurrency {
        /// The variable that carried it.
        env: &'static str,
        /// The value, verbatim.
        value: String,
    },
}

/// The loaded catalogue, or the explanation of why there is none.
#[derive(Debug, Clone)]
pub struct ConsoleState {
    /// The artifact root the console reads.
    pub root: PathBuf,
    /// The vendored spec tree citations resolve against.
    pub specs: PathBuf,
    /// The run-output tree job artifacts land under.
    pub out: PathBuf,
    /// The armored secret key the export seals with, when one is mounted.
    pub sign_key: Option<PathBuf>,
    /// The armored public key `/verify` checks a bundle against, when one is
    /// mounted.
    pub verify_key: Option<PathBuf>,
    /// The one startup validation pass, shared by every request; `Err` is
    /// the verbatim reason the catalogue could not be opened.
    pub catalogue: Arc<Result<veredictum::pipeline::catalogue::Validation, String>>,
    /// The in-flight run drafts (the wizard's server-side memory), one per
    /// submitter (#389): two visitors composing a connection at once do not
    /// overwrite each other. A restart legitimately forgets every draft — no
    /// console-local store exists — and the map is capped and evicted
    /// oldest-first like the job map beside it.
    pub draft: Arc<std::sync::Mutex<crate::run_api::Drafts>>,
    /// The run-job map (#66, #389): every run this process is driving or has
    /// recently driven, addressed by its own id.
    pub jobs: crate::run_job::JobSlot,
    /// The request header the operator asked to be trusted for the client
    /// address ([`CLIENT_IP_HEADER_ENV`]), when they named one.
    pub client_ip_header: Option<String>,
    /// Whose network this instance sits in (#390), read once from
    /// [`crate::posture::POSTURE_ENV`]. The hosted posture refuses a target
    /// only this instance can reach; the local one refuses nothing.
    pub posture: crate::posture::Posture,
    /// The per-submitter rate ledger (#390) the probe and start seams ask
    /// before spending anything, over the same submitter identity the
    /// concurrency caps read.
    pub rates: crate::rate_limit::RateLimiter,
    /// Whether the documentation capture mode is on
    /// ([`crate::capture::CAPTURE_ENV`]): the facts a run stamps then render
    /// as fixed stand-ins. It changes what the surfaces DISPLAY and nothing
    /// that is written, sealed or signed.
    pub capture: bool,
}

impl ConsoleState {
    /// Reads the mounts from the environment and loads the catalogue once,
    /// through the published lib — the same call `validate` runs.
    ///
    /// # Errors
    /// [`StartupError`]: a posture that names no posture, or a concurrency cap
    /// that is not a positive integer. Those are the two startup values this
    /// console refuses to guess at — a missing mount is a first-class state the
    /// screens explain, while a wrong guess about either of these lets the
    /// instrument drive an address it should not or admit work it cannot hold.
    pub fn load() -> Result<Self, StartupError> {
        let posture = crate::posture::from_env()?;
        // What this host can actually drive at once. #388 reasoned two, and
        // called it a starting value to re-derive by measuring on the chosen
        // host; this is where the host says what it measured.
        let max_concurrent = match std::env::var(MAX_CONCURRENT_RUNS_ENV) {
            Err(_) => crate::run_job::MAX_CONCURRENT_RUNS,
            Ok(value) => match value.trim().parse::<usize>() {
                Ok(n) if n > 0 => n,
                _ => {
                    return Err(StartupError::Concurrency {
                        env: MAX_CONCURRENT_RUNS_ENV,
                        value,
                    });
                }
            },
        };
        let root =
            PathBuf::from(std::env::var(ROOT_ENV).unwrap_or_else(|_| String::from("artifacts")));
        let specs = PathBuf::from(
            std::env::var(SPECS_ENV).unwrap_or_else(|_| String::from("specs/openehr")),
        );
        let out = PathBuf::from(std::env::var(OUT_ENV).unwrap_or_else(|_| String::from("out")));
        let sign_key = std::env::var(SIGN_KEY_ENV).ok().map(PathBuf::from);
        let verify_key = std::env::var(VERIFY_KEY_ENV).ok().map(PathBuf::from);
        let catalogue = veredictum::pipeline::catalogue::validate_tree(&root, Some(&specs))
            .map_err(|e| e.to_string());
        Ok(Self {
            root,
            specs,
            out,
            sign_key,
            verify_key,
            catalogue: Arc::new(catalogue),
            draft: Arc::new(std::sync::Mutex::new(crate::run_api::Drafts::new())),
            jobs: crate::run_job::JobSlot::with_limits(crate::run_job::Limits {
                max_concurrent,
                ..crate::run_job::Limits::default()
            }),
            client_ip_header: std::env::var(CLIENT_IP_HEADER_ENV)
                .ok()
                .filter(|name| !name.trim().is_empty()),
            posture,
            rates: crate::rate_limit::RateLimiter::default(),
            capture: crate::capture::enabled(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::{MAX_CONCURRENT_RUNS_ENV, StartupError};

    /// The cap a host may state, and the values it may not.
    ///
    /// Driven over the parse itself rather than through `load()`: that reads
    /// process-wide environment, and a test that sets a variable races every
    /// other test in the binary.
    fn cap_of(value: Option<&str>) -> Result<usize, StartupError> {
        match value {
            None => Ok(crate::run_job::MAX_CONCURRENT_RUNS),
            Some(value) => match value.trim().parse::<usize>() {
                Ok(n) if n > 0 => Ok(n),
                _ => Err(StartupError::Concurrency {
                    env: MAX_CONCURRENT_RUNS_ENV,
                    value: value.to_owned(),
                }),
            },
        }
    }

    #[test]
    fn an_unset_cap_is_the_reasoned_default() {
        assert_eq!(cap_of(None).ok(), Some(crate::run_job::MAX_CONCURRENT_RUNS));
    }

    #[test]
    fn a_host_states_what_it_can_drive() {
        assert_eq!(cap_of(Some("1")).ok(), Some(1));
        assert_eq!(cap_of(Some(" 4 ")).ok(), Some(4));
    }

    /// Every one of these would otherwise fall back to a LARGER cap than the
    /// host stated, which is the direction that ends a run halfway through.
    #[test]
    fn a_cap_that_is_not_a_positive_integer_refuses_to_start() {
        for value in ["", "0", "two", "-1", "1.5", "1 2"] {
            let refused = cap_of(Some(value));
            assert!(
                matches!(refused, Err(StartupError::Concurrency { .. })),
                "{value:?} must refuse rather than fall back"
            );
        }
    }
}
