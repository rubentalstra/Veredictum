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

/// The environment variable naming the party-declaration tree (default
/// `party`; the image sets it to the documented `/work` mount).
pub const PARTY_ENV: &str = "VEREDICTUM_PARTY";

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
    /// [`crate::posture::UnknownPosture`] when
    /// [`crate::posture::POSTURE_ENV`] names no posture. That is the ONE
    /// startup value this console refuses to guess at: a missing mount is a
    /// first-class state the screens explain, but a public instance falling
    /// back to `local` on a typo would drive whatever address a visitor
    /// named, so the process does not start at all.
    pub fn load() -> Result<Self, crate::posture::UnknownPosture> {
        let posture = crate::posture::from_env()?;
        let root =
            PathBuf::from(std::env::var(ROOT_ENV).unwrap_or_else(|_| String::from("artifacts")));
        let specs = PathBuf::from(
            std::env::var(SPECS_ENV).unwrap_or_else(|_| String::from("specs/openehr")),
        );
        let party =
            PathBuf::from(std::env::var(PARTY_ENV).unwrap_or_else(|_| String::from("party")));
        let out = PathBuf::from(std::env::var(OUT_ENV).unwrap_or_else(|_| String::from("out")));
        let sign_key = std::env::var(SIGN_KEY_ENV).ok().map(PathBuf::from);
        let verify_key = std::env::var(VERIFY_KEY_ENV).ok().map(PathBuf::from);
        let catalogue = veredictum::pipeline::catalogue::validate_tree(&root, Some(&specs))
            .map_err(|e| e.to_string());
        Ok(Self {
            root,
            specs,
            party,
            out,
            sign_key,
            verify_key,
            catalogue: Arc::new(catalogue),
            draft: Arc::new(std::sync::Mutex::new(crate::run_api::Drafts::new())),
            jobs: crate::run_job::JobSlot::default(),
            client_ip_header: std::env::var(CLIENT_IP_HEADER_ENV)
                .ok()
                .filter(|name| !name.trim().is_empty()),
            posture,
            rates: crate::rate_limit::RateLimiter::default(),
            capture: crate::capture::enabled(),
        })
    }
}
