// SPDX-FileCopyrightText: Veredictum contributors
// SPDX-License-Identifier: Apache-2.0

//! Same-machine reference baselines: the anchor an absolute millisecond needs.
//!
//! A bench number taken on one machine says nothing about a number taken on
//! another, because the machines differ. The anchor is a reference CDR
//! measured on the SAME host, in the same session, by the same pack at the
//! same seed: the ratio between the two travels where the milliseconds do
//! not. This module composes each reference deployment from its own pinned
//! image digests, drives the pack against it, and tears the stack down with
//! its volumes, so the next baseline starts from an empty database.
//!
//! Every reference image is pinned BY DIGEST, never by tag, so two submitters
//! measure the same bytes (docker's own guidance for a reproducible build,
//! <https://docs.docker.com/build/building/best-practices/>). The compose
//! document is written by this module rather than fetched, and each pin
//! discloses the upstream deployment recipe whose topology, service names,
//! environment and credentials it follows, at an immutable tag.
//!
//! The container runtime is reached through the `docker` CLI as a
//! subprocess, which is how the measured-performance instrument already
//! reaches it. [`DockerCli`] carries the binary path so a test can point it
//! at something that does not exist and assert the refusal.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{Duration, Instant};

use crate::bench::BenchError;
use crate::bench::client::{AuthKind, BenchClient, PreferReturn};
use crate::bench::compare::summarize;
use crate::bench::pack::BenchPack;
use crate::bench::result::{BaselineRecord, BaselineResources, RecipeReference};
use crate::bench::run::{self, BenchRun};

/// The CPU ceiling every baseline's server container runs under.
///
/// Both baselines take the same ceilings, because a baseline handed more CPU
/// than its sibling measures the ceiling rather than the CDR.
const SERVER_CPUS: &str = "4";

/// The memory ceiling every baseline's server container runs under.
const SERVER_MEMORY: &str = "4G";

/// The CPU ceiling every baseline's database container runs under.
const DATABASE_CPUS: &str = "4";

/// The memory ceiling every baseline's database container runs under.
const DATABASE_MEMORY: &str = "4G";

/// The shared-memory floor every baseline's database container runs under.
///
/// Docker's default 64 MB starves PostgreSQL's dynamic shared memory under
/// load, which shows up as an error rather than as a slow answer, so both
/// stacks share the same floor.
const DATABASE_SHM_SIZE: &str = "1gb";

/// How long `docker compose up --wait` may take before the baseline is
/// refused, in seconds.
const COMPOSE_WAIT_S: u64 = 300;

/// How long the external readiness probe polls the composed base URL before
/// the baseline is refused.
const READINESS_TIMEOUT: Duration = Duration::from_mins(5);

/// How long the readiness probe sleeps between attempts.
const READINESS_INTERVAL: Duration = Duration::from_secs(2);

/// The path the readiness probe reads. An authenticated read that touches the
/// database, so a server whose schema migration is still running fails it.
const READINESS_PATH: &str = "/definition/template/adl1.4";

/// The address every baseline's published port binds to. A baseline stack is
/// reachable from the host that composed it and from nowhere else.
const BIND_HOST: &str = "127.0.0.1";

/// The reference CDRs a baseline run composes.
///
/// A closed vocabulary: an unknown token is a loud error rather than a
/// silently skipped baseline, because a missing baseline changes whether a
/// record may be submitted at all.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum ReferenceCdr {
    /// EHRbase, the openEHR Foundation's reference Java implementation.
    EhrBase,
    /// FerroEHR.
    FerroEhr,
}

impl ReferenceCdr {
    /// Every reference CDR, in the order a baseline run composes them.
    pub const ALL: &[ReferenceCdr] = &[ReferenceCdr::EhrBase, ReferenceCdr::FerroEhr];

    /// The token the record names the baseline by.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            ReferenceCdr::EhrBase => "ehrbase",
            ReferenceCdr::FerroEhr => "ferroehr",
        }
    }

    /// The human-readable name a rendered view prints.
    #[must_use]
    pub const fn display_name(self) -> &'static str {
        match self {
            ReferenceCdr::EhrBase => "EHRbase",
            ReferenceCdr::FerroEhr => "FerroEHR",
        }
    }

    /// Reads one token from the closed vocabulary.
    ///
    /// # Errors
    /// [`BenchError::UnknownToken`] listing the accepted tokens.
    pub fn parse(token: &str) -> Result<Self, BenchError> {
        Self::ALL
            .iter()
            .copied()
            .find(|cdr| cdr.as_str() == token)
            .ok_or_else(|| BenchError::UnknownToken {
                vocabulary: "reference CDR",
                token: token.to_owned(),
                accepted: Self::ALL
                    .iter()
                    .map(|cdr| cdr.as_str())
                    .collect::<Vec<_>>()
                    .join(", "),
            })
    }

    /// The pinned deployment this reference CDR composes from.
    #[must_use]
    pub const fn pin(self) -> &'static ReferencePin {
        match self {
            ReferenceCdr::EhrBase => &EHRBASE_PIN,
            ReferenceCdr::FerroEhr => &FERROEHR_PIN,
        }
    }
}

impl std::fmt::Display for ReferenceCdr {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

/// One reference CDR's pinned deployment: the image digests, the upstream
/// recipe the topology follows, and the wire the pack is driven over.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ReferencePin {
    /// Which reference CDR this pin describes.
    pub cdr: ReferenceCdr,
    /// The server image, pinned by digest.
    pub server_image: &'static str,
    /// The database image, pinned by digest.
    pub database_image: &'static str,
    /// The upstream repository the deployment recipe lives in.
    pub recipe_repository: &'static str,
    /// The immutable tag the recipe is read at.
    pub recipe_ref: &'static str,
    /// The recipe file within that repository.
    pub recipe_file: &'static str,
    /// The host port the server's HTTP listener is published on.
    pub host_port: u16,
    /// The host port the database is published on.
    pub database_port: u16,
    /// The openEHR REST base path the server serves under.
    pub rest_path: &'static str,
    /// The clinical user the composed stack declares.
    pub user: &'static str,
    /// That user's password, as the composed stack declares it. A credential
    /// this module wrote into a stack it also tears down.
    pub password: &'static str,
}

/// EHRbase 2.35.1 with its companion PostgreSQL image, both resolved from the
/// registry on 2026-08-29.
///
/// The topology, service names, environment keys and Basic-auth posture
/// follow the upstream quickstart recipe named in [`ReferencePin::recipe`];
/// the Keycloak service that recipe also starts is left out, because the pack
/// authenticates with Basic and never reaches an authorization server.
const EHRBASE_PIN: ReferencePin = ReferencePin {
    cdr: ReferenceCdr::EhrBase,
    server_image: "ehrbase/ehrbase:2.35.1@sha256:a17cfdd7be7045a2abb75a37d33ae8c26c92f6e8acd832922cfc786d0791e8a8",
    database_image: "ehrbase/ehrbase-v2-postgres:16.2@sha256:abe14e8f9ba33cabc9946c6c17c5aa95b64b35387f266cd20a894149203196d7",
    recipe_repository: "https://github.com/ehrbase/ehrbase",
    recipe_ref: "v2.35.1",
    recipe_file: "docker-compose.yml",
    host_port: 18091,
    database_port: 15432,
    rest_path: "/ehrbase/rest/openehr/v1",
    user: "veredictum",
    password: "veredictum",
};

/// FerroEHR 4.0.10 with its companion PostgreSQL image, both resolved from
/// the registry on 2026-08-29.
///
/// The topology, environment keys and the dev Basic-auth user follow the
/// upstream conformance stack named in [`ReferencePin::recipe`], whose
/// configuration file this module reproduces beside the compose document
/// because a `[[auth.basic.users]]` array has no flat environment form.
const FERROEHR_PIN: ReferencePin = ReferencePin {
    cdr: ReferenceCdr::FerroEhr,
    server_image: "ghcr.io/rubentalstra/ferroehr:4.0.10@sha256:63d9ad3f1328680d0b78a08da345006c285990c82852715fcea7f8234263882b",
    database_image: "ghcr.io/rubentalstra/ferroehr-postgres:4.0.10@sha256:0309fe2962ba9913a93d679c389c5e852f029761b0e2af3260466679e829d5ad",
    recipe_repository: "https://github.com/rubentalstra/FerroEHR",
    recipe_ref: "v4.0.10",
    recipe_file: "docker/sut-ferroehr.yml",
    host_port: 18080,
    database_port: 15433,
    rest_path: "/ferroehr/rest/openehr/v1",
    user: "ferroehr",
    password: "ferroehr",
};

/// The Argon2id PHC hash of the FerroEHR dev password, verbatim from the
/// upstream configuration file [`FERROEHR_PIN`] names.
const FERROEHR_PASSWORD_HASH: &str = "$argon2id$v=19$m=19456,t=2,p=1$ZmVycm9laHJEZXZTYWx0$be5nPwWjfUl1qvSrvkvqvdMOuCgM0VFcN/VN4MFLjT8";

/// The file name the FerroEHR baseline's configuration is written under and
/// mounted from.
const FERROEHR_CONFIG_FILE: &str = "ferroehr.toml";

/// The file name every baseline's compose document is written under.
const COMPOSE_FILE: &str = "compose.yaml";

impl ReferencePin {
    /// The base URL the pack is driven over.
    #[must_use]
    pub fn base_url(&self) -> String {
        format!("http://{BIND_HOST}:{}{}", self.host_port, self.rest_path)
    }

    /// The compose project name this baseline's containers live under.
    #[must_use]
    pub fn project(&self) -> String {
        format!("veredictum-baseline-{}", self.cdr.as_str())
    }

    /// The upstream deployment recipe this pin's topology follows.
    #[must_use]
    pub fn recipe(&self) -> RecipeReference {
        RecipeReference {
            repository: self.recipe_repository.to_owned(),
            git_ref: self.recipe_ref.to_owned(),
            file: self.recipe_file.to_owned(),
        }
    }

    /// The digest-pinned images this baseline composes, keyed by role.
    #[must_use]
    pub fn images(&self) -> BTreeMap<String, String> {
        let mut images = BTreeMap::new();
        let _replaced = images.insert("server".to_owned(), self.server_image.to_owned());
        let _replaced = images.insert("database".to_owned(), self.database_image.to_owned());
        images
    }

    /// The compose document this baseline is composed from.
    #[must_use]
    pub fn compose_document(&self) -> String {
        match self.cdr {
            ReferenceCdr::EhrBase => self.ehrbase_compose(),
            ReferenceCdr::FerroEhr => self.ferroehr_compose(),
        }
    }

    /// The extra files this baseline's compose document mounts, as
    /// (file name, contents) pairs written beside it.
    #[must_use]
    pub fn side_files(&self) -> Vec<(&'static str, String)> {
        match self.cdr {
            ReferenceCdr::EhrBase => Vec::new(),
            ReferenceCdr::FerroEhr => {
                vec![(FERROEHR_CONFIG_FILE, ferroehr_configuration())]
            }
        }
    }

    /// The EHRbase stack: the official server image over its companion
    /// PostgreSQL image, Basic auth, and the resource ceilings every baseline
    /// shares.
    fn ehrbase_compose(&self) -> String {
        let (server, database) = (self.server_image, self.database_image);
        let (port, db_port) = (self.host_port, self.database_port);
        let (user, password) = (self.user, self.password);
        format!(
            "services:\n\
             \x20 ehrbase-db:\n\
             \x20   image: {database}\n\
             \x20   shm_size: {DATABASE_SHM_SIZE}\n\
             \x20   environment:\n\
             \x20     POSTGRES_USER: postgres\n\
             \x20     POSTGRES_PASSWORD: postgres\n\
             \x20     EHRBASE_USER_ADMIN: ehrbase\n\
             \x20     EHRBASE_PASSWORD_ADMIN: ehrbase\n\
             \x20     EHRBASE_USER: ehrbase_restricted\n\
             \x20     EHRBASE_PASSWORD: ehrbase_restricted\n\
             \x20   healthcheck:\n\
             \x20     test: [\"CMD-SHELL\", \"pg_isready -U postgres\"]\n\
             \x20     interval: 5s\n\
             \x20     timeout: 5s\n\
             \x20     retries: 24\n\
             \x20   ports:\n\
             \x20     - \"{BIND_HOST}:{db_port}:5432\"\n\
             \x20   deploy:\n\
             \x20     resources:\n\
             \x20       limits:\n\
             \x20         cpus: \"{DATABASE_CPUS}\"\n\
             \x20         memory: {DATABASE_MEMORY}\n\
             \x20 ehrbase:\n\
             \x20   image: {server}\n\
             \x20   depends_on:\n\
             \x20     ehrbase-db:\n\
             \x20       condition: service_healthy\n\
             \x20   environment:\n\
             \x20     DB_URL: jdbc:postgresql://ehrbase-db:5432/ehrbase\n\
             \x20     DB_USER_ADMIN: ehrbase\n\
             \x20     DB_PASS_ADMIN: ehrbase\n\
             \x20     DB_USER: ehrbase_restricted\n\
             \x20     DB_PASS: ehrbase_restricted\n\
             \x20     SECURITY_AUTHTYPE: BASIC\n\
             \x20     SECURITY_AUTHUSER: {user}\n\
             \x20     SECURITY_AUTHPASSWORD: {password}\n\
             \x20     SECURITY_AUTHADMINUSER: {user}-admin\n\
             \x20     SECURITY_AUTHADMINPASSWORD: {password}\n\
             \x20     SYSTEM_ALLOWTEMPLATEOVERWRITE: \"false\"\n\
             \x20   ports:\n\
             \x20     - \"{BIND_HOST}:{port}:8080\"\n\
             \x20   deploy:\n\
             \x20     resources:\n\
             \x20       limits:\n\
             \x20         cpus: \"{SERVER_CPUS}\"\n\
             \x20         memory: {SERVER_MEMORY}\n"
        )
    }

    /// The FerroEHR stack: the published server image over its companion
    /// PostgreSQL image, the dev Basic user from the upstream configuration,
    /// and the same ceilings the EHRbase stack runs under.
    fn ferroehr_compose(&self) -> String {
        let (server, database) = (self.server_image, self.database_image);
        let (port, db_port) = (self.host_port, self.database_port);
        format!(
            "services:\n\
             \x20 ferroehr-postgres:\n\
             \x20   image: {database}\n\
             \x20   shm_size: {DATABASE_SHM_SIZE}\n\
             \x20   environment:\n\
             \x20     POSTGRES_PASSWORD: postgres\n\
             \x20     PG_INIT_USER: ferroehr\n\
             \x20     PG_INIT_PASSWORD: ferroehr\n\
             \x20     PG_INIT_DB: ferroehr\n\
             \x20   healthcheck:\n\
             \x20     test: [\"CMD-SHELL\", \"pg_isready -U ferroehr -d ferroehr\"]\n\
             \x20     interval: 5s\n\
             \x20     timeout: 5s\n\
             \x20     retries: 24\n\
             \x20   ports:\n\
             \x20     - \"{BIND_HOST}:{db_port}:5432\"\n\
             \x20   deploy:\n\
             \x20     resources:\n\
             \x20       limits:\n\
             \x20         cpus: \"{DATABASE_CPUS}\"\n\
             \x20         memory: {DATABASE_MEMORY}\n\
             \x20 ferroehr:\n\
             \x20   image: {server}\n\
             \x20   depends_on:\n\
             \x20     ferroehr-postgres:\n\
             \x20       condition: service_healthy\n\
             \x20   environment:\n\
             \x20     FERROEHR__DB__URL: postgres://ferroehr:ferroehr@ferroehr-postgres:5432/ferroehr\n\
             \x20     FERROEHR__SERVER__RATE_LIMIT__ENABLED: \"false\"\n\
             \x20     FERROEHR__LOG__FILTER: warn\n\
             \x20   volumes:\n\
             \x20     - ./{FERROEHR_CONFIG_FILE}:/etc/ferroehr/ferroehr.toml:ro\n\
             \x20   ports:\n\
             \x20     - \"{BIND_HOST}:{port}:8080\"\n\
             \x20   deploy:\n\
             \x20     resources:\n\
             \x20       limits:\n\
             \x20         cpus: \"{SERVER_CPUS}\"\n\
             \x20         memory: {SERVER_MEMORY}\n"
        )
    }
}

/// The FerroEHR baseline's configuration file: authentication on, one Basic
/// user, and nothing else the pack needs.
///
/// The rate limiter is disabled by the compose document rather than here,
/// because a limiter that answers `429` under an offered arrival rate
/// measures the limiter instead of the server.
fn ferroehr_configuration() -> String {
    format!(
        "[auth]\n\
         enabled = true\n\
         \n\
         [[auth.basic.users]]\n\
         username = \"ferroehr\"\n\
         password_hash = \"{FERROEHR_PASSWORD_HASH}\"\n\
         roles = [\"ADMIN\", \"USER\"]\n"
    )
}

/// How the baseline orchestration reaches the container runtime.
///
/// The binary path is a field rather than a literal so a caller can point it
/// at a path that does not exist and get the refusal, which is the only way
/// to test the missing-runtime branch without uninstalling docker.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DockerCli {
    binary: PathBuf,
}

impl Default for DockerCli {
    fn default() -> Self {
        Self {
            binary: PathBuf::from("docker"),
        }
    }
}

impl DockerCli {
    /// The CLI as it is found on `PATH`.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// The CLI at an explicit path.
    #[must_use]
    pub fn at(binary: impl Into<PathBuf>) -> Self {
        Self {
            binary: binary.into(),
        }
    }

    /// The binary this instance invokes.
    #[must_use]
    pub fn binary(&self) -> &Path {
        &self.binary
    }

    /// Proves the container runtime answers, and returns the server version
    /// it disclosed.
    ///
    /// # Errors
    /// [`BenchError::DockerUnavailable`] naming the binary that could not be
    /// invoked, or that answered with a failure.
    pub fn probe(&self) -> Result<String, BenchError> {
        let output = Command::new(&self.binary)
            .args(version_args())
            .output()
            .map_err(|source| BenchError::DockerUnavailable {
                binary: self.binary.display().to_string(),
                detail: source.to_string(),
            })?;
        if !output.status.success() {
            return Err(BenchError::DockerUnavailable {
                binary: self.binary.display().to_string(),
                detail: format!(
                    "`docker version` exited {:?}: {}",
                    output.status.code(),
                    String::from_utf8_lossy(&output.stderr).trim()
                ),
            });
        }
        Ok(String::from_utf8_lossy(&output.stdout).trim().to_owned())
    }

    /// Runs one docker invocation, returning its trimmed standard output.
    ///
    /// # Errors
    /// [`BenchError::DockerUnavailable`] when the binary could not be
    /// invoked, or [`BenchError::Baseline`] when it exited non-zero.
    fn run(&self, cdr: ReferenceCdr, args: &[String]) -> Result<String, BenchError> {
        let output = Command::new(&self.binary)
            .args(args)
            .output()
            .map_err(|source| BenchError::DockerUnavailable {
                binary: self.binary.display().to_string(),
                detail: source.to_string(),
            })?;
        if !output.status.success() {
            return Err(BenchError::Baseline {
                cdr: cdr.as_str().to_owned(),
                detail: format!(
                    "`docker {}` exited {:?}: {}",
                    args.join(" "),
                    output.status.code(),
                    String::from_utf8_lossy(&output.stderr).trim()
                ),
            });
        }
        Ok(String::from_utf8_lossy(&output.stdout).trim().to_owned())
    }
}

/// The arguments that ask the runtime for its server version.
#[must_use]
pub fn version_args() -> Vec<String> {
    ["version", "--format", "{{.Server.Version}}"]
        .into_iter()
        .map(str::to_owned)
        .collect()
}

/// The arguments that bring one baseline stack up and wait for it.
///
/// `--wait` returns once every service is running or healthy, which is the
/// runtime's own readiness signal; the server's own answer is proved
/// separately, because a container that is running has not necessarily
/// finished its schema migration.
#[must_use]
pub fn compose_up_args(project: &str, compose_file: &Path) -> Vec<String> {
    vec![
        "compose".to_owned(),
        "--project-name".to_owned(),
        project.to_owned(),
        "--file".to_owned(),
        compose_file.display().to_string(),
        "up".to_owned(),
        "--detach".to_owned(),
        "--wait".to_owned(),
        "--wait-timeout".to_owned(),
        COMPOSE_WAIT_S.to_string(),
    ]
}

/// The arguments that tear one baseline stack down, volumes included.
///
/// Fresh volumes per baseline is the fairness rule: a database that already
/// carries the previous baseline's population measures a different system.
#[must_use]
pub fn compose_down_args(project: &str, compose_file: &Path) -> Vec<String> {
    vec![
        "compose".to_owned(),
        "--project-name".to_owned(),
        project.to_owned(),
        "--file".to_owned(),
        compose_file.display().to_string(),
        "down".to_owned(),
        "--volumes".to_owned(),
        "--remove-orphans".to_owned(),
    ]
}

/// What a baseline sweep asks for: the same pack, the same seed, the same
/// repetitions the target run used.
#[derive(Debug)]
pub struct BaselineRun<'a> {
    /// The pack to drive against every reference CDR.
    pub pack: &'a BenchPack,
    /// How many times to repeat the measured phases, matching the target.
    pub repetitions: u32,
    /// The scale factor the target ran at.
    pub scale: f64,
    /// The seed-worker override the target ran with.
    pub seed_workers: Option<usize>,
    /// How the orchestration reaches the container runtime.
    pub docker: &'a DockerCli,
}

/// Composes, measures and tears down every reference CDR in turn.
///
/// # Errors
/// [`BenchError::DockerUnavailable`] when the container runtime does not
/// answer, [`BenchError::Baseline`] when a stack could not be composed or
/// never became ready, [`BenchError::Write`] when the compose document could
/// not be written, and whatever the engine itself reports for the measured
/// run.
pub fn run_baselines(
    run: &BaselineRun<'_>,
    progress: &(dyn Fn(String) + Sync),
) -> Result<Vec<BaselineRecord>, BenchError> {
    let version = run.docker.probe()?;
    progress(format!(
        "container runtime answers, server version {version}"
    ));
    let mut records = Vec::with_capacity(ReferenceCdr::ALL.len());
    for cdr in ReferenceCdr::ALL {
        records.push(run_one_baseline(*cdr, run, progress)?);
    }
    Ok(records)
}

/// Composes one reference CDR, measures it, and tears it down.
///
/// The teardown runs whether or not the measurement succeeded, so a failed
/// baseline never leaves containers and volumes behind for the next one.
fn run_one_baseline(
    cdr: ReferenceCdr,
    run: &BaselineRun<'_>,
    progress: &(dyn Fn(String) + Sync),
) -> Result<BaselineRecord, BenchError> {
    let pin = cdr.pin();
    let workspace = workspace_dir(pin);
    let compose_file = workspace.join(COMPOSE_FILE);
    write_workspace(pin, &workspace, &compose_file)?;

    progress(format!(
        "baseline {}: composing {} and {}",
        cdr.as_str(),
        pin.server_image,
        pin.database_image
    ));
    let measured = compose_and_measure(cdr, pin, run, &compose_file, progress);

    progress(format!("baseline {}: tearing down", cdr.as_str()));
    let torn_down = run
        .docker
        .run(cdr, &compose_down_args(&pin.project(), &compose_file))
        .map(|_stdout| ());
    let cleaned = std::fs::remove_dir_all(&workspace);
    if let Err(error) = cleaned {
        progress(format!(
            "baseline {}: the workspace {} could not be removed: {error}",
            cdr.as_str(),
            workspace.display()
        ));
    }
    let record = measured?;
    torn_down?;
    Ok(record)
}

/// Brings the stack up, waits for the server's own answer, and drives the
/// pack against it.
fn compose_and_measure(
    cdr: ReferenceCdr,
    pin: &ReferencePin,
    run: &BaselineRun<'_>,
    compose_file: &Path,
    progress: &(dyn Fn(String) + Sync),
) -> Result<BaselineRecord, BenchError> {
    let _stdout = run
        .docker
        .run(cdr, &compose_up_args(&pin.project(), compose_file))?;
    wait_until_ready(pin, progress)?;
    progress(format!(
        "baseline {}: driving pack {}@{} at seed {:#018x}",
        cdr.as_str(),
        run.pack.id.as_str(),
        run.pack.version,
        run.pack.seed
    ));
    let result = run::execute(
        &BenchRun {
            pack: run.pack,
            base_url: &pin.base_url(),
            auth: AuthKind::Basic,
            user: Some(pin.user),
            credential: Some(pin.password),
            repetitions: run.repetitions,
            label: Some(pin.cdr.display_name()),
            scale: run.scale,
            seed_workers: run.seed_workers,
        },
        progress,
    )?;
    let cross = summarize(&result.repetitions);
    Ok(BaselineRecord {
        cdr: cdr.as_str().to_owned(),
        display_name: cdr.display_name().to_owned(),
        images: pin.images(),
        recipe: pin.recipe(),
        resources: pinned_resources(),
        base_url: result.target.base_url.clone(),
        sut_version: result.target.sut_version.clone(),
        started_at: result.started_at.clone(),
        finished_at: result.finished_at.clone(),
        seed_phases: result.seed_phases.clone(),
        repetitions: result.repetitions.clone(),
        cross,
    })
}

/// The resource ceilings every baseline composes under.
#[must_use]
pub fn pinned_resources() -> BaselineResources {
    BaselineResources {
        server_cpus: SERVER_CPUS.to_owned(),
        server_memory: SERVER_MEMORY.to_owned(),
        database_cpus: DATABASE_CPUS.to_owned(),
        database_memory: DATABASE_MEMORY.to_owned(),
        database_shm_size: DATABASE_SHM_SIZE.to_owned(),
    }
}

/// The directory this baseline's compose document and its side files are
/// written into.
///
/// The process id keeps two concurrent runs on one host from sharing a
/// workspace, which would let one run's teardown delete the other's document.
fn workspace_dir(pin: &ReferencePin) -> PathBuf {
    std::env::temp_dir().join(format!("{}-{}", pin.project(), std::process::id()))
}

/// Writes the compose document and every file it mounts.
fn write_workspace(
    pin: &ReferencePin,
    workspace: &Path,
    compose_file: &Path,
) -> Result<(), BenchError> {
    std::fs::create_dir_all(workspace).map_err(|source| BenchError::Write {
        path: workspace.to_owned(),
        source,
    })?;
    std::fs::write(compose_file, pin.compose_document()).map_err(|source| BenchError::Write {
        path: compose_file.to_owned(),
        source,
    })?;
    for (name, body) in pin.side_files() {
        let path = workspace.join(name);
        std::fs::write(&path, body).map_err(|source| BenchError::Write { path, source })?;
    }
    Ok(())
}

/// Polls the composed server until it answers an authenticated read.
///
/// A running container has not necessarily finished its schema migration, so
/// the runtime's own readiness signal is not enough to start measuring on.
fn wait_until_ready(
    pin: &ReferencePin,
    progress: &(dyn Fn(String) + Sync),
) -> Result<(), BenchError> {
    let base_url = pin.base_url();
    let client = BenchClient::with_credential(
        &base_url,
        AuthKind::Basic,
        Some(pin.user),
        Some(pin.password),
    )?;
    let deadline = Instant::now() + READINESS_TIMEOUT;
    let mut last = "no attempt completed".to_owned();
    while Instant::now() < deadline {
        let probe = client.send(
            "baseline readiness",
            reqwest::Method::GET,
            READINESS_PATH,
            None,
            PreferReturn::Unstated,
        );
        last = match probe {
            Ok(reply) if reply.status.is_success() => {
                progress(format!(
                    "baseline {}: ready at {base_url}",
                    pin.cdr.as_str()
                ));
                return Ok(());
            }
            Ok(reply) => format!("GET {READINESS_PATH} answered {}", reply.status),
            Err(error) => error.to_string(),
        };
        std::thread::sleep(READINESS_INTERVAL);
    }
    Err(BenchError::Baseline {
        cdr: pin.cdr.as_str().to_owned(),
        detail: format!(
            "the stack never became ready within {}s at {base_url}: {last}",
            READINESS_TIMEOUT.as_secs()
        ),
    })
}

#[cfg(test)]
#[expect(
    clippy::panic_in_result_fn,
    reason = "Result-returning tests in the Book ch11 shape that also assert; \
              clippy offers no allow-in-tests knob for this lint"
)]
mod tests {
    use super::*;

    /// Every reference token round-trips, and an unknown one is refused
    /// rather than falling back to a default baseline.
    #[test]
    fn every_reference_token_round_trips() -> Result<(), BenchError> {
        for cdr in ReferenceCdr::ALL {
            assert_eq!(ReferenceCdr::parse(cdr.as_str())?, *cdr);
        }
        assert!(ReferenceCdr::parse("arcehr").is_err());
        Ok(())
    }

    /// Every reference image is pinned by digest, never by a tag a registry
    /// can re-point.
    #[test]
    fn every_reference_image_is_digest_pinned() {
        for cdr in ReferenceCdr::ALL {
            let pin = cdr.pin();
            for image in [pin.server_image, pin.database_image] {
                assert!(image.contains("@sha256:"), "{image}");
                let digest = image.split_once("@sha256:").map(|(_, hex)| hex);
                assert_eq!(digest.map(str::len), Some(64), "{image}");
                assert!(
                    digest.is_some_and(|hex| hex.chars().all(|c| c.is_ascii_hexdigit())),
                    "{image}"
                );
            }
        }
    }

    /// The two baselines never share a published port, so both stacks can be
    /// composed on one host without colliding.
    #[test]
    fn the_baselines_publish_distinct_ports() {
        let ports: Vec<u16> = ReferenceCdr::ALL
            .iter()
            .flat_map(|cdr| [cdr.pin().host_port, cdr.pin().database_port])
            .collect();
        let mut unique = ports.clone();
        unique.sort_unstable();
        unique.dedup();
        assert_eq!(unique.len(), ports.len(), "{ports:?}");
    }

    /// The compose document names the pinned digests, the shared ceilings and
    /// the published port, so what ran is readable from the document itself.
    #[test]
    fn a_compose_document_carries_the_pins_and_the_ceilings() {
        for cdr in ReferenceCdr::ALL {
            let pin = cdr.pin();
            let document = pin.compose_document();
            assert!(document.contains(pin.server_image), "{document}");
            assert!(document.contains(pin.database_image), "{document}");
            assert!(document.contains(SERVER_CPUS), "{document}");
            assert!(document.contains(SERVER_MEMORY), "{document}");
            assert!(document.contains(DATABASE_SHM_SIZE), "{document}");
            assert!(
                document.contains(&format!("{BIND_HOST}:{}:8080", pin.host_port)),
                "{document}"
            );
        }
    }

    /// The FerroEHR baseline mounts its configuration file, and the EHRbase
    /// one mounts nothing.
    #[test]
    fn only_the_ferroehr_baseline_carries_a_side_file() {
        let ferroehr = ReferenceCdr::FerroEhr.pin();
        let names: Vec<&str> = ferroehr
            .side_files()
            .into_iter()
            .map(|(name, _body)| name)
            .collect();
        assert_eq!(names, vec![FERROEHR_CONFIG_FILE]);
        assert!(ferroehr.compose_document().contains(&format!(
            "./{FERROEHR_CONFIG_FILE}:/etc/ferroehr/ferroehr.toml"
        )),);
        assert!(ReferenceCdr::EhrBase.pin().side_files().is_empty());
    }

    /// The compose arguments are assembled from the project and the document,
    /// and the teardown always removes the volumes.
    #[test]
    fn the_compose_arguments_name_the_project_and_the_document() {
        let file = Path::new("/tmp/veredictum/compose.yaml");
        let up = compose_up_args("veredictum-baseline-ehrbase", file);
        assert_eq!(
            up,
            vec![
                "compose",
                "--project-name",
                "veredictum-baseline-ehrbase",
                "--file",
                "/tmp/veredictum/compose.yaml",
                "up",
                "--detach",
                "--wait",
                "--wait-timeout",
                "300",
            ]
        );
        let down = compose_down_args("veredictum-baseline-ferroehr", file);
        assert!(down.contains(&"--volumes".to_owned()), "{down:?}");
        assert!(down.contains(&"--remove-orphans".to_owned()), "{down:?}");
        assert_eq!(down.first().map(String::as_str), Some("compose"));
    }

    /// An absent container runtime refuses with the typed error that names
    /// docker as the missing ground, rather than reporting an empty baseline
    /// set as if none were asked for.
    #[test]
    fn an_absent_container_runtime_is_refused_by_name() {
        let docker = DockerCli::at("/nonexistent/veredictum/docker");
        let error = docker.probe().unwrap_err();
        assert!(
            matches!(&error, BenchError::DockerUnavailable { binary, .. }
                if binary == "/nonexistent/veredictum/docker"),
            "{error}"
        );
        assert!(error.to_string().contains("docker"), "{error}");
    }

    /// The ceilings are the same for both baselines, which is what makes the
    /// two numbers readable against one another.
    #[test]
    fn both_baselines_run_under_the_same_ceilings() {
        let resources = pinned_resources();
        assert_eq!(resources.server_cpus, SERVER_CPUS);
        assert_eq!(resources.database_memory, DATABASE_MEMORY);
        for cdr in ReferenceCdr::ALL {
            let document = cdr.pin().compose_document();
            assert!(
                document.contains(&format!("cpus: \"{SERVER_CPUS}\"")),
                "{document}"
            );
            assert!(document.contains(DATABASE_MEMORY), "{document}");
        }
    }

    /// The recipe reference names the repository, the immutable tag and the
    /// file, so a reader can fetch the topology this stack follows.
    #[test]
    fn every_pin_discloses_its_upstream_recipe() {
        for cdr in ReferenceCdr::ALL {
            let recipe = cdr.pin().recipe();
            assert!(recipe.repository.starts_with("https://"), "{recipe:?}");
            assert!(recipe.git_ref.starts_with('v'), "{recipe:?}");
            let extension = Path::new(&recipe.file)
                .extension()
                .and_then(std::ffi::OsStr::to_str);
            assert!(matches!(extension, Some("yml" | "yaml")), "{recipe:?}");
        }
    }

    /// A base URL carries the REST path the reference serves under, and the
    /// project name is stable per reference.
    #[test]
    fn the_base_url_and_the_project_follow_the_pin() {
        let pin = ReferenceCdr::EhrBase.pin();
        assert_eq!(
            pin.base_url(),
            "http://127.0.0.1:18091/ehrbase/rest/openehr/v1"
        );
        assert_eq!(pin.project(), "veredictum-baseline-ehrbase");
        assert_eq!(
            ReferenceCdr::FerroEhr.pin().project(),
            "veredictum-baseline-ferroehr"
        );
    }
}
