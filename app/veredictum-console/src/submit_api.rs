// SPDX-FileCopyrightText: Veredictum contributors
// SPDX-License-Identifier: Apache-2.0

//! S10 — a finished run submits itself to the public results registry (#391).
//!
//! The console AUTHORS the submission document and reads everything it stands
//! on through the published lib's typed API, exactly as the run seam authors
//! the ixit it drives with. The entry model, the entry-id grammar, the digest
//! grammar and every closed vocabulary in it are `veredictum::registry`'s, so
//! the console restates none of them.
//!
//! **The submitted entry carries NO provenance block.** Everything the
//! `console` tier asserts is something this repository established rather than
//! something the performer claimed, so the re-derivation lane writes that block
//! after it has recomputed the judgement and signed the record. A document
//! without provenance is a SUBMISSION; CI is what turns it into an entry.
//!
//! **No credential value reaches the branch.** The ixit the run was driven
//! under names environment variables and never values, the transcript withholds
//! the credential header, and the drafted values were MOVED into the spawned
//! engine's environment at start time. Nothing in this module reads a
//! credential, and a gate over a run driven with one proves the submitted bytes
//! carry no occurrence of it.

use serde::{Deserialize, Serialize};

/// The tree a conformance entry lives under.
pub const ENTRIES_ROOT: &str = "registry/entries/conformance";

/// The tree a conformance entry's evidence lives under.
pub const RECORDS_ROOT: &str = "registry/records";

/// The five record files a console submission carries, in the order the entry
/// lists them.
///
/// The three beyond `results` and `verdicts` are what a re-derivation reads:
/// the recorded exchanges, the topology they were driven under, and the claim
/// they were judged against.
pub const RECORD_FILES: [&str; 5] = [
    "results.json",
    "verdicts.json",
    "transcript.json",
    "ixit.json",
    "statement.json",
];

/// How many hexadecimal characters of the run id the entry-id slug carries.
pub const SLUG_HEX_CHARS: usize = 12;

/// What the submitter fills in, as the form sends it.
///
/// Every member is text, because a form sends text: the typed reading — the
/// relationship vocabulary, the two integers, the authorization flag — happens
/// once, server-side, in [`read::compose`], and each refusal names the field it
/// is about.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct DisclosureForm {
    /// Who is publishing the entry.
    pub submitter_name: String,
    /// Where the entry can be discussed.
    pub submitter_contact: String,
    /// `vendor`, `integrator`, `independent` or `maintainer`.
    pub relationship: String,
    /// The lowercase system id, which is also the entry's directory.
    pub system: String,
    /// The name a board prints.
    pub display_name: String,
    /// The version that was measured.
    pub version: String,
    /// Whether this repository may drive the deployment again, `yes` or `no`.
    pub reproduction_authorized: String,
    /// The operating system the SUT runs on.
    pub environment_os: String,
    /// Its architecture.
    pub environment_arch: String,
    /// How the submitter describes the host.
    pub environment_host_class: String,
    /// The CPU model, when the platform discloses one.
    pub environment_cpu_model: String,
    /// Cores available to the deployment, when the platform discloses them.
    pub environment_cores: String,
    /// Memory available to it in bytes, when the platform discloses it.
    pub environment_memory_bytes: String,
    /// What was switched on behind the result.
    pub sut_configuration: String,
    /// Any interest the submitter holds in the outcome, in words.
    pub conflict_of_interest: String,
}

/// The facts the run already knows, so the form asks only for what it cannot.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SubmissionFacts {
    /// The run's own identity, which is also the branch's.
    pub run_id: String,
    /// The entry id the submission will carry.
    pub entry_id: String,
    /// The branch the submission arrives on.
    pub branch: String,
    /// The registry repository, `owner/name`.
    pub repo: String,
    /// The SUT name the run recorded, proposed as the display name.
    pub display_name: String,
    /// The SUT version the run recorded.
    pub version: String,
    /// The lowercase system id proposed from the recorded name.
    pub system: String,
    /// The endpoint the run drove, read back from the run's own ixit.
    pub endpoint: String,
    /// The engine version the console links.
    pub instrument_version: String,
    /// When the run started, RFC 3339 in UTC.
    pub run_started_at: String,
    /// The catalogue revision the run executed, from the results record's own
    /// `schedule_release`.
    pub catalogue_revision: String,
    /// The repository-relative paths the submission adds.
    pub files: Vec<String>,
}

/// Where the submit screen stands.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum SubmitScreen {
    /// The instrument carries no registry App identity, naming every variable
    /// that is unset. The screen explains what to configure and offers no
    /// button.
    NotConfigured {
        /// The unset variables, by name.
        missing: Vec<String>,
    },
    /// No finished run exists to submit.
    NoRun,
    /// The run was driven without a statement, so there is no claim to
    /// publish.
    NoStatement,
    /// The run recorded no wire exchanges, so its judgement could never be
    /// re-derived.
    NoTranscript,
    /// Ready: what the run knows, and what the submitter must still say.
    Ready(Box<SubmissionFacts>),
}

/// What opening a submission produced.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SubmitOutcome {
    /// The entry id that was submitted.
    pub entry_id: String,
    /// The branch it arrived on.
    pub branch: String,
    /// The pull request's browser URL.
    pub pull_request_url: String,
    /// Its number.
    pub pull_request: u64,
    /// The repository-relative paths it added.
    pub files: Vec<String>,
}

/// The lowercase system id proposed from a recorded SUT name.
///
/// The schema's grammar is `[a-z0-9]([a-z0-9-]*[a-z0-9])?`, so anything else
/// collapses to a single `-` and the ends are trimmed. An empty proposal is
/// returned empty rather than invented: the submitter is asked for it, and an
/// empty field is refused by name.
#[must_use]
pub fn proposed_system(recorded_name: &str) -> String {
    let mut out = String::with_capacity(recorded_name.len());
    for c in recorded_name.chars() {
        if c.is_ascii_alphanumeric() {
            out.extend(c.to_lowercase());
        } else if !out.ends_with('-') {
            out.push('-');
        }
    }
    out.trim_matches('-').to_owned()
}

/// The entry-id slug one run carries: `console-` and the run id's leading
/// hexadecimal.
///
/// The slug names the run rather than a word the submitter chose, so the id a
/// reader sees resolves back to the branch the lane read the run out of.
#[must_use]
pub fn slug_of(run_id: &str) -> String {
    let hex: String = run_id
        .chars()
        .filter(char::is_ascii_hexdigit)
        .take(SLUG_HEX_CHARS)
        .flat_map(char::to_lowercase)
        .collect();
    format!("console-{hex}")
}

/// The repository-relative paths one submission adds, entry first.
///
/// Both targets: the screen states the paths it is about to add, and the
/// documentation capture pins them, so the one derivation lives here.
#[must_use]
pub fn submission_paths(system: &str, entry_id: &str) -> Vec<String> {
    let mut paths = vec![format!("{ENTRIES_ROOT}/{system}/{entry_id}.json")];
    paths.extend(
        RECORD_FILES
            .iter()
            .map(|name| format!("{RECORDS_ROOT}/{system}/{entry_id}/{name}")),
    );
    paths
}

#[cfg(feature = "ssr")]
pub mod read {
    //! The component-free ssr side: what the screen shows, and the document
    //! the submission commits.

    use std::path::Path;

    use sha2::{Digest as _, Sha256};

    use super::{
        DisclosureForm, ENTRIES_ROOT, RECORD_FILES, RECORDS_ROOT, SubmissionFacts, SubmitOutcome,
        SubmitScreen, proposed_system, slug_of, submission_paths,
    };
    use crate::github::{AppConfig, Client, GithubError, SubmissionFile, branch_of};
    use crate::state::ConsoleState;
    use crate::submitter::Submitter;

    /// Why a submission could not be composed or opened.
    ///
    /// Typed at every boundary that branches: an empty mandatory field is a
    /// different refusal from a malformed one, and both are different from the
    /// API declining the write.
    #[derive(Debug, thiserror::Error)]
    pub enum SubmitError {
        /// The instrument carries no registry App identity.
        #[error(
            "this instrument has no registry identity: set {} before submitting",
            .missing.join(", ")
        )]
        NotConfigured {
            /// The unset variables, by name.
            missing: Vec<String>,
        },
        /// There is no finished run to submit.
        #[error("no finished run: grade a server first")]
        NoRun,
        /// The run carried no claim.
        #[error(
            "the run was driven without a statement, so there is no claim to publish: pick one at the Scope step and run again"
        )]
        NoStatement,
        /// The run recorded no exchanges.
        #[error(
            "the run recorded no wire exchanges, and a console submission is re-derived from them: run again with exchange recording on"
        )]
        NoTranscript,
        /// A mandatory field arrived empty.
        #[error(
            "the submission is incomplete: {field} is empty, and the submission rules refuse an empty value there"
        )]
        Empty {
            /// The field, spelled as the entry spells it.
            field: &'static str,
        },
        /// A field arrived in a shape the entry cannot carry.
        #[error("{field}: {reason}")]
        Malformed {
            /// The field, spelled as the entry spells it.
            field: &'static str,
            /// What is wrong with it.
            reason: String,
        },
        /// The lib refused a value the entry model owns.
        #[error("the entry does not hold together: {0}")]
        Registry(#[from] veredictum::registry::RegistryError),
        /// A file the submission stands on could not be read.
        #[error("{path}: {source}")]
        Io {
            /// The file.
            path: String,
            /// The filesystem's own diagnostic.
            source: std::io::Error,
        },
        /// The console could not read its own run.
        #[error("{0}")]
        Run(String),
        /// The entry document could not be serialized.
        #[error("the submission document did not serialize: {0}")]
        Serialize(#[from] serde_json::Error),
        /// GitHub refused, or was never reached.
        #[error(transparent)]
        Github(#[from] GithubError),
    }

    /// The run this submitter would submit: its id and its own directory.
    ///
    /// The per-submitter lookup is the job map's ONE reader, and the directory
    /// comes from the run seam's one derivation, so neither claim is spelled
    /// twice here.
    fn finished_run(
        state: &ConsoleState,
        submitter: Submitter,
    ) -> Result<Option<(crate::run_job::RunId, std::path::PathBuf)>, SubmitError> {
        let latest = state
            .jobs
            .latest_of(submitter, crate::run_job::Latest::Finished)
            .map_err(|e| SubmitError::Run(e.to_string()))?;
        Ok(latest.map(|id| (id, crate::run_job::job_dir(&state.out, id))))
    }

    /// Reads one file of the run's own directory.
    fn read_file(dir: &Path, name: &str) -> Result<String, SubmitError> {
        let path = dir.join(name);
        std::fs::read_to_string(&path).map_err(|source| SubmitError::Io {
            path: path.display().to_string(),
            source,
        })
    }

    /// When the run started, in UTC.
    ///
    /// The run's ixit is written into its own directory immediately before the
    /// engine is spawned and is never touched again, so its modification time
    /// IS the moment the run started. The entry id's date is derived from this
    /// same timestamp, which is why the two can never disagree — the gate
    /// refuses a submission where they do.
    fn started_at(dir: &Path) -> Result<jiff::Timestamp, SubmitError> {
        let path = dir.join("ixit.json");
        let modified = std::fs::metadata(&path)
            .and_then(|meta| meta.modified())
            .map_err(|source| SubmitError::Io {
                path: path.display().to_string(),
                source,
            })?;
        jiff::Timestamp::try_from(modified).map_err(|e| SubmitError::Malformed {
            field: "disclosure.run_started_at",
            reason: format!("the run's own clock does not read as a timestamp: {e}"),
        })
    }

    /// The endpoint the run drove, read back from the run's own ixit through
    /// the published lib's typed reader.
    fn endpoint_of(dir: &Path) -> Result<String, SubmitError> {
        let body = read_file(dir, "ixit.json")?;
        let ixit: veredictum::ixit::Ixit =
            serde_json::from_str(&body).map_err(SubmitError::Serialize)?;
        ixit.instances
            .iter()
            .find(|(name, _)| name.as_str() == "sut")
            .map(|(_, instance)| instance.base_url.clone())
            .ok_or(SubmitError::Malformed {
                field: "subject.deployment.endpoint",
                reason: String::from("the run's ixit declares no `sut` instance"),
            })
    }

    /// Lowercase hex, the encoding every digest an entry pins carries.
    fn hex(bytes: &[u8]) -> String {
        use std::fmt::Write as _;
        bytes.iter().fold(
            String::with_capacity(bytes.len().saturating_mul(2)),
            |mut out, byte| {
                let _ = write!(out, "{byte:02x}");
                out
            },
        )
    }

    /// The five record bodies, in the order the entry lists them.
    ///
    /// `verdicts.json` is not a file in the run's directory: it is what the
    /// published lib's judgement renders over the run, byte for byte the same
    /// document the CLI writes, which is what lets CI recompute it and compare.
    fn record_bodies(
        state: &ConsoleState,
        submitter: Submitter,
        dir: &Path,
    ) -> Result<Vec<(&'static str, String)>, SubmitError> {
        if !dir.join("statement.json").is_file() {
            return Err(SubmitError::NoStatement);
        }
        if !dir.join(veredictum::transcript::TRANSCRIPT_FILE).is_file() {
            return Err(SubmitError::NoTranscript);
        }
        let judged = crate::record_api::read::judged(state, submitter).map_err(SubmitError::Run)?;
        let facts = match judged {
            crate::record_api::read::JudgedRun::Judged(facts) => facts,
            crate::record_api::read::JudgedRun::NoStatement => {
                return Err(SubmitError::NoStatement);
            }
            crate::record_api::read::JudgedRun::NoRun => return Err(SubmitError::NoRun),
        };
        let verdicts = facts
            .documents
            .iter()
            .find(|document| document.name == "verdicts.json")
            .map(|document| document.body.clone())
            .ok_or(SubmitError::Malformed {
                field: "artifacts[verdicts]",
                reason: String::from("the judgement rendered no verdicts document"),
            })?;
        let mut bodies = Vec::with_capacity(RECORD_FILES.len());
        for name in RECORD_FILES {
            let body = if name == "verdicts.json" {
                verdicts.clone()
            } else {
                read_file(dir, name)?
            };
            bodies.push((name, body));
        }
        Ok(bodies)
    }

    /// Where the submit screen stands, without composing anything.
    ///
    /// # Errors
    /// The console's own read failures. An unconfigured identity, an absent
    /// run, a claimless run and a recording-less run are all ANSWERS the
    /// screen renders, never errors.
    pub fn screen(state: &ConsoleState, submitter: Submitter) -> Result<SubmitScreen, SubmitError> {
        screen_with(state, submitter, AppConfig::from_env())
    }

    /// Where the submit screen stands, for an identity a caller already has.
    ///
    /// The split is the run and export seams' own: [`screen`] reads the
    /// identity the way the server does, and a test hands it one rather than
    /// mutating the process environment.
    ///
    /// # Errors
    /// The console's own read failures, exactly as [`screen`].
    pub fn screen_with(
        state: &ConsoleState,
        submitter: Submitter,
        identity: Result<AppConfig, Vec<String>>,
    ) -> Result<SubmitScreen, SubmitError> {
        let config = match identity {
            Ok(config) => config,
            Err(missing) => return Ok(SubmitScreen::NotConfigured { missing }),
        };
        let Some((id, dir)) = finished_run(state, submitter)? else {
            return Ok(SubmitScreen::NoRun);
        };
        if !dir.join("statement.json").is_file() {
            return Ok(SubmitScreen::NoStatement);
        }
        if !dir.join(veredictum::transcript::TRANSCRIPT_FILE).is_file() {
            return Ok(SubmitScreen::NoTranscript);
        }
        let results: veredictum::party::Results =
            serde_json::from_str(&read_file(&dir, "results.json")?)?;
        let started = started_at(&dir)?;
        let run_id = id.to_string();
        let entry_id = format!("{}-{}", started.strftime("%Y-%m-%d"), slug_of(&run_id));
        let system = proposed_system(&results.sut.name);
        Ok(SubmitScreen::Ready(Box::new(SubmissionFacts {
            files: submission_paths(&system, &entry_id),
            branch: branch_of(&run_id),
            repo: config.repo,
            display_name: results.sut.name.clone(),
            version: results.sut.version.clone(),
            system,
            endpoint: endpoint_of(&dir)?,
            instrument_version: String::from(crate::ENGINE_PIN),
            run_started_at: started.strftime("%Y-%m-%dT%H:%M:%SZ").to_string(),
            catalogue_revision: results.schedule_release.clone(),
            run_id,
            entry_id,
        })))
    }

    /// The submission document set, ready to commit.
    #[derive(Debug, Clone, PartialEq, Eq)]
    pub struct Composed {
        /// The run's own identity, which the branch carries.
        pub run_id: String,
        /// The entry id.
        pub entry_id: String,
        /// The branch the submission arrives on.
        pub branch: String,
        /// The commit subject.
        pub message: String,
        /// The pull request's title.
        pub title: String,
        /// The pull request's body.
        pub body: String,
        /// Every file the commit adds, entry first.
        pub files: Vec<SubmissionFile>,
    }

    /// One mandatory field, refused empty by name.
    fn required(value: &str, field: &'static str) -> Result<String, SubmitError> {
        let trimmed = value.trim();
        if trimmed.is_empty() {
            return Err(SubmitError::Empty { field });
        }
        Ok(trimmed.to_owned())
    }

    /// One optional integer field: absent when blank, refused by name when it
    /// is present and not a number.
    fn optional_number<T: std::str::FromStr>(
        value: &str,
        field: &'static str,
    ) -> Result<Option<T>, SubmitError> {
        let trimmed = value.trim();
        if trimmed.is_empty() {
            return Ok(None);
        }
        trimmed
            .parse::<T>()
            .map(Some)
            .map_err(|_unparseable| SubmitError::Malformed {
                field,
                reason: format!("{trimmed:?} is not a positive whole number"),
            })
    }

    /// The disclosure block, every mandatory value present and typed.
    fn disclosure(
        form: &DisclosureForm,
        run_started_at: String,
    ) -> Result<veredictum::registry::Disclosure, SubmitError> {
        Ok(veredictum::registry::Disclosure {
            instrument_version: String::from(crate::ENGINE_PIN),
            run_started_at,
            environment: veredictum::registry::EnvironmentDisclosure {
                os: required(&form.environment_os, "disclosure.environment.os")?,
                arch: required(&form.environment_arch, "disclosure.environment.arch")?,
                host_class: required(
                    &form.environment_host_class,
                    "disclosure.environment.host_class",
                )?,
                cpu_model: Some(form.environment_cpu_model.trim().to_owned())
                    .filter(|model| !model.is_empty()),
                cores: optional_number(&form.environment_cores, "disclosure.environment.cores")?,
                memory_bytes: optional_number(
                    &form.environment_memory_bytes,
                    "disclosure.environment.memory_bytes",
                )?,
            },
            sut_configuration: required(&form.sut_configuration, "disclosure.sut_configuration")?,
            // The rules give this one no "not applicable": the sentence that is
            // true is the sentence the submitter writes.
            conflict_of_interest: required(
                &form.conflict_of_interest,
                "disclosure.conflict_of_interest",
            )?,
        })
    }

    /// The subject block, with the endpoint the run actually drove.
    fn subject(
        form: &DisclosureForm,
        endpoint: String,
    ) -> Result<veredictum::registry::Subject, SubmitError> {
        let system = required(&form.system, "subject.system")?;
        if proposed_system(&system) != system {
            return Err(SubmitError::Malformed {
                field: "subject.system",
                reason: format!(
                    "{system:?} is not a lowercase id: the registry spells a system as [a-z0-9] separated by single hyphens"
                ),
            });
        }
        let authorized = match required(
            &form.reproduction_authorized,
            "subject.deployment.reproduction_authorized",
        )?
        .as_str()
        {
            "yes" => true,
            "no" => false,
            other => {
                return Err(SubmitError::Malformed {
                    field: "subject.deployment.reproduction_authorized",
                    reason: format!("{other:?} is neither yes nor no"),
                });
            }
        };
        Ok(veredictum::registry::Subject {
            system,
            display_name: required(&form.display_name, "subject.display_name")?,
            version: required(&form.version, "subject.version")?,
            deployment: veredictum::registry::Deployment {
                // The console drives a running service the submitter operates,
                // which is what this kind names and the only kind it can name.
                kind: veredictum::registry::DeploymentKind::HostedEndpoint,
                topology: None,
                images: std::collections::BTreeMap::new(),
                endpoint: Some(endpoint),
                reproduction_authorized: authorized,
            },
        })
    }

    /// The submission document, with NO provenance block.
    ///
    /// Field order here is the entry's own key order, and every value in it is
    /// one of the published lib's typed structures. The one thing this shape
    /// adds is the absence the rules require: a performer does not state its
    /// own provenance, so the lane writes that block after it has recomputed
    /// the judgement and signed the record.
    #[derive(Debug, serde::Serialize)]
    pub struct SubmittedEntry {
        /// The entry format version this document is written against.
        pub registry_schema_version: String,
        /// The entry's own identifier, which is also its file stem.
        pub entry_id: veredictum::registry::EntryId,
        /// The submission rules version it is written against.
        pub rules_version: String,
        /// Who is submitting it.
        pub submitter: veredictum::registry::Submitter,
        /// The system it is about.
        pub subject: veredictum::registry::Subject,
        /// The mandatory disclosure.
        pub disclosure: veredictum::registry::Disclosure,
        /// What was measured.
        pub result: veredictum::registry::ResultBlock,
        /// The committed artifacts it stands on.
        pub artifacts: Vec<veredictum::registry::ArtifactRef>,
    }

    /// Composes the whole submission for this submitter's finished run.
    ///
    /// Every mandatory field is validated BEFORE anything is opened, and the
    /// refusal names the field rather than failing at the API.
    ///
    /// # Errors
    /// [`SubmitError::NotConfigured`], [`SubmitError::NoRun`],
    /// [`SubmitError::NoStatement`], [`SubmitError::NoTranscript`],
    /// [`SubmitError::Empty`] naming the field, [`SubmitError::Malformed`],
    /// and the filesystem's or the lib's verbatim failure.
    pub fn compose(
        state: &ConsoleState,
        submitter: Submitter,
        form: &DisclosureForm,
    ) -> Result<Composed, SubmitError> {
        let config =
            AppConfig::from_env().map_err(|missing| SubmitError::NotConfigured { missing })?;
        compose_with(state, submitter, form, &config)
    }

    /// Composes the submission against an identity a caller already has.
    ///
    /// The split is the run and export seams' own: [`compose`] reads the
    /// identity the way the server does, and a test hands it one.
    ///
    /// # Errors
    /// Every refusal [`compose`] makes.
    pub fn compose_with(
        state: &ConsoleState,
        submitter: Submitter,
        form: &DisclosureForm,
        config: &AppConfig,
    ) -> Result<Composed, SubmitError> {
        let Some((id, dir)) = finished_run(state, submitter)? else {
            return Err(SubmitError::NoRun);
        };
        let bodies = record_bodies(state, submitter, &dir)?;
        let results: veredictum::party::Results =
            serde_json::from_str(&read_file(&dir, "results.json")?)?;
        let started = started_at(&dir)?;
        let run_id = id.to_string();
        let entry_id = veredictum::registry::EntryId::parse(&format!(
            "{}-{}",
            started.strftime("%Y-%m-%d"),
            slug_of(&run_id)
        ))?;

        let subject = subject(form, endpoint_of(&dir)?)?;
        let system = subject.system.clone();
        let disclosure = disclosure(form, started.strftime("%Y-%m-%dT%H:%M:%SZ").to_string())?;
        let submitter_block = veredictum::registry::Submitter {
            name: required(&form.submitter_name, "submitter.name")?,
            contact: required(&form.submitter_contact, "submitter.contact")?,
            relationship: veredictum::registry::Relationship::parse(&required(
                &form.relationship,
                "submitter.relationship",
            )?)?,
        };

        let mut artifacts = Vec::with_capacity(bodies.len());
        let mut files = Vec::with_capacity(bodies.len().saturating_add(1));
        for (name, body) in &bodies {
            let path = format!("{RECORDS_ROOT}/{system}/{entry_id}/{name}");
            artifacts.push(veredictum::registry::ArtifactRef {
                role: veredictum::registry::ArtifactRole::parse(
                    name.strip_suffix(".json").unwrap_or(name),
                )?,
                sha256: veredictum::registry::Digest::parse(&hex(&Sha256::digest(
                    body.as_bytes(),
                )))?,
                path: path.clone(),
            });
            files.push(SubmissionFile {
                path,
                body: body.clone(),
            });
        }

        let entry = SubmittedEntry {
            registry_schema_version: String::from(veredictum::registry::REGISTRY_SCHEMA_VERSION),
            entry_id: entry_id.clone(),
            rules_version: String::from(veredictum::registry::RULES_VERSION),
            submitter: submitter_block,
            subject,
            disclosure,
            result: veredictum::registry::ResultBlock::Conformance {
                catalogue_revision: results.schedule_release.clone(),
                statement: format!("{RECORDS_ROOT}/{system}/{entry_id}/statement.json"),
            },
            artifacts,
        };
        let mut document = serde_json::to_string_pretty(&entry)?;
        document.push('\n');
        files.insert(
            0,
            SubmissionFile {
                path: format!("{ENTRIES_ROOT}/{system}/{entry_id}.json"),
                body: document,
            },
        );

        let display = format!("{} {}", entry.subject.display_name, entry.subject.version);
        Ok(Composed {
            branch: branch_of(&run_id),
            message: format!("chore(registry): the console's run of {display}"),
            title: format!("Console submission: {display}"),
            body: pull_request_body(&entry_id.to_string(), &run_id, &config.repo, &files),
            files,
            entry_id: entry_id.to_string(),
            run_id,
        })
    }

    /// What the pull request says about itself.
    ///
    /// It states what arrived and what CI will do to it, and it claims nothing
    /// about provenance: the block that carries that claim is written by the
    /// lane, after the judgement has been recomputed here.
    fn pull_request_body(
        entry_id: &str,
        run_id: &str,
        repo: &str,
        files: &[SubmissionFile],
    ) -> String {
        let list = files
            .iter()
            .map(|file| format!("- `{}`", file.path))
            .collect::<Vec<String>>()
            .join("\n");
        format!(
            "The official hosted instrument drove a conformance run against an endpoint the \
submitter named, recorded every exchange, computed the verdicts, and opened this submission from \
its own App identity.\n\n\
Entry `{entry_id}`, from console run `{run_id}`.\n\n\
## What arrived\n\n{list}\n\n\
## What happens next\n\n\
The entry carries no provenance block, because a performer does not state its own. \
`{repo}`'s re-derivation lane replays the recorded exchanges against the catalogue, recomputes the \
verdicts from the resulting outcomes, refuses any mismatch, signs the record with the registry key \
from its protected environment, and writes the `console` provenance block stating what it \
established. The merge is the publication.\n\n\
No credential the run was driven under is anywhere in this branch: the ixit names environment \
variables and never values, and the recorded exchanges withhold the credential header.\n"
        )
    }

    /// Composes the submission and opens it.
    ///
    /// Nothing is opened until every mandatory field has been read: the
    /// composition above is what refuses an empty value, by name, before a
    /// single request leaves the process.
    ///
    /// # Errors
    /// Every refusal [`compose`] makes, plus the API's own, each naming the
    /// step it belongs to.
    pub async fn submit(
        state: &ConsoleState,
        submitter: Submitter,
        form: &DisclosureForm,
    ) -> Result<SubmitOutcome, SubmitError> {
        let config =
            AppConfig::from_env().map_err(|missing| SubmitError::NotConfigured { missing })?;
        submit_with(state, submitter, form, &config).await
    }

    /// Composes and opens the submission against an identity a caller has.
    ///
    /// # Errors
    /// Every refusal [`compose`] makes, plus the API's own, each naming the
    /// step it belongs to.
    pub async fn submit_with(
        state: &ConsoleState,
        submitter: Submitter,
        form: &DisclosureForm,
        config: &AppConfig,
    ) -> Result<SubmitOutcome, SubmitError> {
        let composed = compose_with(state, submitter, form, config)?;
        let client = Client::authenticate(config).await?;
        let opened = client
            .open_submission(
                &composed.branch,
                &composed.message,
                &composed.title,
                &composed.body,
                &composed.files,
            )
            .await?;
        Ok(SubmitOutcome {
            entry_id: composed.entry_id,
            branch: opened.branch,
            pull_request_url: opened.pull_request_url,
            pull_request: opened.pull_request,
            files: composed.files.into_iter().map(|file| file.path).collect(),
        })
    }
}

pub mod fns {
    //! The `#[server]` endpoints, one module for one inner suppression.
    #![allow(
        clippy::unused_async,
        missing_docs,
        reason = "fires only in some #[server] expansions; see catalogue_api::fns"
    )]

    use leptos::prelude::{ServerFnError, server};

    use super::{DisclosureForm, SubmitOutcome, SubmitScreen};

    /// Where the submit screen stands for this submitter's finished run.
    ///
    /// # Errors
    /// The console's own read failures. An unconfigured identity and a run
    /// that cannot be submitted are ANSWERS, not errors.
    #[server]
    pub async fn fetch_submission() -> Result<SubmitScreen, ServerFnError> {
        let state: crate::state::ConsoleState = leptos::prelude::expect_context();
        let who = crate::submitter::current(&state);
        let screen =
            super::read::screen(&state, who).map_err(|e| ServerFnError::new(e.to_string()))?;
        Ok(crate::capture::submit_screen(&state, screen))
    }

    /// Opens the submission for this submitter's finished run.
    ///
    /// # Errors
    /// The named-field refusal when a mandatory value is empty, and the API's
    /// own refusal naming the step it belongs to.
    #[server]
    pub async fn open_submission(
        disclosure: DisclosureForm,
    ) -> Result<SubmitOutcome, ServerFnError> {
        let state: crate::state::ConsoleState = leptos::prelude::expect_context();
        let who = crate::submitter::current(&state);
        super::read::submit(&state, who, &disclosure)
            .await
            .map_err(|e| ServerFnError::new(e.to_string()))
    }
}

#[cfg(test)]
mod tests {
    use super::{DisclosureForm, RECORD_FILES, SLUG_HEX_CHARS, proposed_system, slug_of};

    /// The proposed system id holds the schema's grammar for the names a run
    /// actually records, and stays empty rather than inventing one.
    #[test]
    fn a_proposed_system_id_is_a_registry_slug() {
        assert_eq!(proposed_system("EHRbase"), "ehrbase");
        assert_eq!(proposed_system("My CDR 2.0"), "my-cdr-2-0");
        assert_eq!(proposed_system("  --Ferro__EHR--  "), "ferro-ehr");
        assert_eq!(proposed_system("!!!"), "");
        assert_eq!(proposed_system(""), "");
    }

    /// The slug names the run, so a reader resolves an entry id back to the
    /// branch the lane read the run out of.
    #[test]
    fn a_slug_names_the_run() {
        assert_eq!(
            slug_of("3f2504e0-4f89-41d3-9a0c-0305e82c3301"),
            "console-3f2504e04f89"
        );
        let slug = slug_of("3f2504e0-4f89-41d3-9a0c-0305e82c3301");
        assert_eq!(slug.len(), "console-".len() + SLUG_HEX_CHARS, "{slug}");
        assert!(
            slug.chars()
                .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-'),
            "{slug}"
        );
    }

    /// The five record files are the ones a re-derivation reads, and the
    /// judgement's own document is among them.
    #[test]
    fn the_record_carries_what_a_rederivation_reads() {
        assert!(RECORD_FILES.contains(&"transcript.json"));
        assert!(RECORD_FILES.contains(&"ixit.json"));
        assert!(RECORD_FILES.contains(&"statement.json"));
        assert!(RECORD_FILES.contains(&"results.json"));
        assert!(RECORD_FILES.contains(&"verdicts.json"));
    }

    /// An empty form is empty in every mandatory field, which is what the
    /// composition refuses one at a time.
    #[test]
    fn an_empty_form_carries_nothing() {
        let form = DisclosureForm::default();
        assert!(form.conflict_of_interest.is_empty());
        assert!(form.submitter_name.is_empty());
        assert!(form.system.is_empty());
    }
}
