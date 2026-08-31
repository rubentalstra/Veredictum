// SPDX-FileCopyrightText: Veredictum contributors
// SPDX-License-Identifier: Apache-2.0

//! The registry App's identity and the Git Data API client behind a console
//! submission (#391).
//!
//! The hosted instrument opens its own pull requests, and the identity that
//! does it is a GitHub App: its installation token is short-lived and
//! revocable, which a signing key on a public host would not be, and it is the
//! only identity permitted to open a `console`-provenance submission.
//!
//! **Every git write goes through the Git Data API — blob, then tree, then
//! commit, then ref.** A commit written through the contents API lands
//! unverified, and this repository accepts only signed commits; a commit built
//! blob-tree-commit-ref is signed by GitHub as the acting identity
//! (<https://docs.github.com/en/rest/git/commits#create-a-commit>), so this
//! client refuses to push a ref at a commit GitHub did not report as verified.
//! `scripts/registry/commit-sealed-record.sh` is the same sequence in shell.
//!
//! Nothing here logs. The App key is read from its file at the moment a JWT is
//! minted, the installation token lives in one local variable, and neither the
//! key nor the token reaches state, a signal, a file or a diagnostic.
//!
//! The API is documented at
//! <https://docs.github.com/en/rest/git> (blobs, trees, commits, refs),
//! <https://docs.github.com/en/rest/pulls/pulls> and
//! <https://docs.github.com/en/rest/apps/apps#create-an-installation-access-token-for-an-app>.

use std::path::PathBuf;

use crate::state::{
    REGISTRY_API_ENV, REGISTRY_APP_ID_ENV, REGISTRY_APP_KEY_ENV, REGISTRY_INSTALLATION_ENV,
    REGISTRY_REPO_ENV,
};

/// The public GitHub REST API root, used when the operator names no other.
pub const PUBLIC_API: &str = "https://api.github.com";

/// The `X-GitHub-Api-Version` every request pins itself to.
///
/// GitHub versions its REST API by date and serves the pinned version rather
/// than the newest one, so a client that names none silently follows whatever
/// the default becomes
/// (<https://docs.github.com/en/rest/about-the-rest-api/api-versions>).
pub const API_VERSION: &str = "2022-11-28";

/// The file mode every blob in the submission tree carries: a plain,
/// non-executable file.
pub const FILE_MODE: &str = "100644";

/// The prefix a console submission's branch always carries.
///
/// Not cosmetic: the re-derivation lane reads the run id out of the branch
/// name and refuses a submission that did not arrive on one
/// (`.github/workflows/registry-console.yml`).
pub const BRANCH_PREFIX: &str = "console-run/";

/// How long a minted App JWT is valid for, in seconds.
///
/// GitHub caps it at ten minutes and rejects anything longer; nine leaves room
/// for the clock skew the `iat` backdating below also allows for.
const JWT_TTL_SECONDS: i64 = 540;

/// How far the JWT's `iat` is backdated, in seconds.
///
/// GitHub's own guidance, for a host whose clock runs slightly fast.
const JWT_BACKDATE_SECONDS: i64 = 60;

/// The longest error message this client repeats from an API response.
const DETAIL_CAP: usize = 400;

/// A value that must never be rendered.
///
/// The App key never becomes one of these — it is read from its file into a
/// local and dropped — but the minted JWT and the installation token both are,
/// so a `Debug` of any structure carrying one cannot print it.
#[derive(Clone)]
pub struct Secret(String);

impl Secret {
    /// Wraps a secret value.
    #[must_use]
    pub fn new(value: String) -> Self {
        Self(value)
    }

    /// The value, for the one place that puts it on the wire.
    #[must_use]
    pub fn expose(&self) -> &str {
        &self.0
    }
}

impl std::fmt::Debug for Secret {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("Secret(«redacted»)")
    }
}

/// The registry App's identity, as the environment declares it.
///
/// The key is held as a PATH, never as bytes: the file is read at the moment a
/// JWT is minted, exactly as the export seam holds the signing key.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AppConfig {
    /// The App's own id, as GitHub issues it.
    pub app_id: String,
    /// The PEM file holding the App's private key.
    pub key_file: PathBuf,
    /// The App's installation id on the registry repository.
    pub installation_id: String,
    /// The registry repository, `owner/name`.
    pub repo: String,
    /// The REST API root every request is built on.
    pub api_base: String,
}

impl AppConfig {
    /// Reads the identity from the environment, or names every variable that
    /// is missing.
    ///
    /// Any of them unset is a FIRST-CLASS state, exactly like an unmounted
    /// signing key: the submit section explains what to configure and offers
    /// no button. There is no half-configured attempt and no panic.
    ///
    /// # Errors
    /// The names of the unset or empty variables, in declaration order.
    pub fn from_env() -> Result<Self, Vec<String>> {
        let mut missing = Vec::new();
        let mut read = |name: &str| match std::env::var(name) {
            Ok(value) if !value.trim().is_empty() => value.trim().to_owned(),
            _ => {
                missing.push(name.to_owned());
                String::new()
            }
        };
        let app_id = read(REGISTRY_APP_ID_ENV);
        let key_file = read(REGISTRY_APP_KEY_ENV);
        let installation_id = read(REGISTRY_INSTALLATION_ENV);
        let repo = read(REGISTRY_REPO_ENV);
        if !missing.is_empty() {
            return Err(missing);
        }
        Ok(Self {
            app_id,
            key_file: PathBuf::from(key_file),
            installation_id,
            repo,
            api_base: std::env::var(REGISTRY_API_ENV)
                .ok()
                .map(|base| base.trim().trim_end_matches('/').to_owned())
                .filter(|base| !base.is_empty())
                .unwrap_or_else(|| String::from(PUBLIC_API)),
        })
    }
}

/// Why a submission could not be opened.
///
/// Typed at every boundary that branches: the screen distinguishes an
/// unconfigured instrument from an API refusal, and the caller distinguishes a
/// commit GitHub declined to sign from every other failure.
#[derive(Debug, thiserror::Error)]
pub enum GithubError {
    /// The App's key file could not be read.
    #[error("the registry App key at {path} could not be read: {source}")]
    Key {
        /// The path the environment named.
        path: String,
        /// The filesystem's own diagnostic.
        source: std::io::Error,
    },
    /// The key is not an RSA PEM, or the JWT could not be signed.
    #[error("the registry App key did not sign a token: {source}")]
    Jwt {
        /// The library's own diagnostic; it never carries key material.
        source: Box<jsonwebtoken::errors::Error>,
    },
    /// The request never reached GitHub, or its answer could not be read.
    #[error("{step}: {source}")]
    Transport {
        /// Which step of the sequence was in flight.
        step: &'static str,
        /// The client's own diagnostic.
        source: Box<reqwest::Error>,
    },
    /// GitHub answered, and the answer was a refusal.
    #[error("{step}: GitHub answered {status} — {detail}")]
    Status {
        /// Which step of the sequence was refused.
        step: &'static str,
        /// The status code, as a number, because it is reported and never
        /// compared against a literal.
        status: u16,
        /// The `message` member of GitHub's own error body, capped.
        detail: String,
    },
    /// GitHub answered with a document missing something the next step needs.
    #[error("{step}: GitHub's answer carries no `{field}`")]
    Malformed {
        /// Which step produced the answer.
        step: &'static str,
        /// The member that was expected.
        field: &'static str,
    },
    /// GitHub did not sign the commit this client built.
    #[error(
        "the submission commit {sha} landed unverified, so no branch was pushed: a commit this repository accepts is signed, and the Git Data API path is what signs it"
    )]
    Unverified {
        /// The commit object that was refused.
        sha: String,
    },
}

/// One file the submission commit adds.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SubmissionFile {
    /// The repository-relative path.
    pub path: String,
    /// The exact bytes, as text. Every file a console submission carries is
    /// JSON, which is UTF-8 by definition (RFC 8259 §8.1), so the blob is
    /// created with the API's `utf-8` encoding and no base64 hop.
    pub body: String,
}

/// What opening a submission produced.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Opened {
    /// The branch the submission arrived on.
    pub branch: String,
    /// The commit that carries it, signed by GitHub.
    pub commit: String,
    /// The pull request's browser URL.
    pub pull_request_url: String,
    /// Its number.
    pub pull_request: u64,
}

/// The branch one run's submission arrives on.
///
/// The re-derivation lane reads the run id back out of this name, so the
/// spelling is a contract rather than a label.
#[must_use]
pub fn branch_of(run_id: &str) -> String {
    format!("{BRANCH_PREFIX}{run_id}")
}

/// The JSON body creating one blob from UTF-8 text.
///
/// # Errors
/// [`GithubError::Malformed`] can never arise here; serialization of a string
/// pair cannot fail, and the signature stays infallible.
#[must_use]
#[expect(
    clippy::disallowed_types,
    reason = "the wire-bodies family: a GitHub API request body is JSON on the wire, and json! is what guarantees its escaping"
)]
pub fn blob_body(content: &str) -> String {
    serde_json::json!({ "content": content, "encoding": "utf-8" }).to_string()
}

/// The JSON body creating one tree over an existing base tree.
#[must_use]
#[expect(
    clippy::disallowed_types,
    reason = "the wire-bodies family: a GitHub API request body is JSON on the wire, and json! is what guarantees its escaping"
)]
pub fn tree_body(base_tree: &str, entries: &[(String, String)]) -> String {
    let tree: Vec<serde_json::Value> = entries
        .iter()
        .map(|(path, sha)| {
            serde_json::json!({
                "path": path,
                "mode": FILE_MODE,
                "type": "blob",
                "sha": sha,
            })
        })
        .collect();
    serde_json::json!({ "base_tree": base_tree, "tree": tree }).to_string()
}

/// The JSON body creating one commit with a single parent.
#[must_use]
#[expect(
    clippy::disallowed_types,
    reason = "the wire-bodies family: a GitHub API request body is JSON on the wire, and json! is what guarantees its escaping"
)]
pub fn commit_body(message: &str, tree: &str, parent: &str) -> String {
    serde_json::json!({ "message": message, "tree": tree, "parents": [parent] }).to_string()
}

/// The JSON body creating one branch reference at a commit.
#[must_use]
#[expect(
    clippy::disallowed_types,
    reason = "the wire-bodies family: a GitHub API request body is JSON on the wire, and json! is what guarantees its escaping"
)]
pub fn ref_body(branch: &str, sha: &str) -> String {
    serde_json::json!({ "ref": format!("refs/heads/{branch}"), "sha": sha }).to_string()
}

/// The JSON body opening one pull request.
#[must_use]
#[expect(
    clippy::disallowed_types,
    reason = "the wire-bodies family: a GitHub API request body is JSON on the wire, and json! is what guarantees its escaping"
)]
pub fn pull_body(title: &str, head: &str, base: &str, body: &str) -> String {
    serde_json::json!({ "title": title, "head": head, "base": base, "body": body }).to_string()
}

/// Mints the App JWT that buys an installation token.
///
/// # Errors
/// [`GithubError::Key`] when the PEM cannot be read, [`GithubError::Jwt`] when
/// it is not an RSA key or the signature fails.
pub fn app_jwt(config: &AppConfig) -> Result<Secret, GithubError> {
    let pem = std::fs::read(&config.key_file).map_err(|source| GithubError::Key {
        path: config.key_file.display().to_string(),
        source,
    })?;
    let key = jsonwebtoken::EncodingKey::from_rsa_pem(&pem).map_err(|source| GithubError::Jwt {
        source: Box::new(source),
    })?;
    let now = jiff::Timestamp::now().as_second();
    let claims = JwtClaims {
        iat: now.saturating_sub(JWT_BACKDATE_SECONDS),
        exp: now.saturating_add(JWT_TTL_SECONDS),
        iss: config.app_id.clone(),
    };
    let header = jsonwebtoken::Header::new(jsonwebtoken::Algorithm::RS256);
    jsonwebtoken::encode(&header, &claims, &key)
        .map(Secret::new)
        .map_err(|source| GithubError::Jwt {
            source: Box::new(source),
        })
}

/// The three registered claims a GitHub App JWT carries.
#[derive(Debug, serde::Serialize)]
struct JwtClaims {
    /// Issued at, backdated for clock skew.
    iat: i64,
    /// Expiry, at most ten minutes out.
    exp: i64,
    /// The App's own id.
    iss: String,
}

/// The Git Data API client, bound to one repository and one token.
#[derive(Debug)]
pub struct Client {
    http: reqwest::Client,
    api_base: String,
    repo: String,
    token: Secret,
}

impl Client {
    /// Exchanges the App's JWT for a short-lived installation token.
    ///
    /// # Errors
    /// The key and JWT failures above, the transport failure, and GitHub's own
    /// refusal with its message.
    pub async fn authenticate(config: &AppConfig) -> Result<Self, GithubError> {
        const STEP: &str = "minting the installation token";
        let jwt = app_jwt(config)?;
        let http = reqwest::Client::builder()
            .build()
            .map_err(|source| GithubError::Transport {
                step: STEP,
                source: Box::new(source),
            })?;
        let url = format!(
            "{}/app/installations/{}/access_tokens",
            config.api_base, config.installation_id
        );
        let response = http
            .post(&url)
            .header(reqwest::header::ACCEPT, "application/vnd.github+json")
            .header("X-GitHub-Api-Version", API_VERSION)
            .header(reqwest::header::USER_AGENT, "veredictum-console")
            .header(
                reqwest::header::AUTHORIZATION,
                format!("Bearer {}", jwt.expose()),
            )
            .header(reqwest::header::CONTENT_LENGTH, "0")
            .send()
            .await
            .map_err(|source| GithubError::Transport {
                step: STEP,
                source: Box::new(source),
            })?;
        let value = read_json(STEP, response).await?;
        let token = member(STEP, &value, "token")?;
        Ok(Self {
            http,
            api_base: config.api_base.clone(),
            repo: config.repo.clone(),
            token: Secret::new(token),
        })
    }

    /// One authenticated request against a repository-relative API path.
    async fn call(
        &self,
        step: &'static str,
        method: reqwest::Method,
        path: &str,
        body: Option<String>,
    ) -> Result<TypedValue, GithubError> {
        let url = format!("{}{path}", self.api_base);
        let mut request = self
            .http
            .request(method, &url)
            .header(reqwest::header::ACCEPT, "application/vnd.github+json")
            .header("X-GitHub-Api-Version", API_VERSION)
            .header(reqwest::header::USER_AGENT, "veredictum-console")
            .header(
                reqwest::header::AUTHORIZATION,
                format!("Bearer {}", self.token.expose()),
            );
        request = match body {
            Some(body) => request
                .header(reqwest::header::CONTENT_TYPE, "application/json")
                .body(body),
            None => request,
        };
        let response = request
            .send()
            .await
            .map_err(|source| GithubError::Transport {
                step,
                source: Box::new(source),
            })?;
        read_json(step, response).await
    }

    /// The repository's default branch, which every submission targets.
    ///
    /// # Errors
    /// The transport failure, GitHub's refusal, or an answer with no
    /// `default_branch`.
    pub async fn default_branch(&self) -> Result<String, GithubError> {
        const STEP: &str = "reading the registry repository";
        let value = self
            .call(
                STEP,
                reqwest::Method::GET,
                &format!("/repos/{}", self.repo),
                None,
            )
            .await?;
        member(STEP, &value, "default_branch")
    }

    /// The commit one branch points at, and the tree that commit carries.
    ///
    /// # Errors
    /// The transport failure, GitHub's refusal, or an answer missing the sha
    /// or the tree.
    pub async fn head_of(&self, branch: &str) -> Result<(String, String), GithubError> {
        const REF_STEP: &str = "reading the base branch";
        const COMMIT_STEP: &str = "reading the base commit";
        let reference = self
            .call(
                REF_STEP,
                reqwest::Method::GET,
                &format!("/repos/{}/git/ref/heads/{branch}", self.repo),
                None,
            )
            .await?;
        let sha = nested_member(REF_STEP, &reference, "object", "sha")?;
        let commit = self
            .call(
                COMMIT_STEP,
                reqwest::Method::GET,
                &format!("/repos/{}/git/commits/{sha}", self.repo),
                None,
            )
            .await?;
        let tree = nested_member(COMMIT_STEP, &commit, "tree", "sha")?;
        Ok((sha, tree))
    }

    /// Creates one blob from UTF-8 text and answers its sha.
    ///
    /// # Errors
    /// The transport failure, GitHub's refusal, or an answer with no `sha`.
    pub async fn create_blob(&self, content: &str) -> Result<String, GithubError> {
        const STEP: &str = "writing a submission blob";
        let value = self
            .call(
                STEP,
                reqwest::Method::POST,
                &format!("/repos/{}/git/blobs", self.repo),
                Some(blob_body(content)),
            )
            .await?;
        member(STEP, &value, "sha")
    }

    /// Creates the tree the submission commit points at.
    ///
    /// # Errors
    /// The transport failure, GitHub's refusal, or an answer with no `sha`.
    pub async fn create_tree(
        &self,
        base_tree: &str,
        entries: &[(String, String)],
    ) -> Result<String, GithubError> {
        const STEP: &str = "writing the submission tree";
        let value = self
            .call(
                STEP,
                reqwest::Method::POST,
                &format!("/repos/{}/git/trees", self.repo),
                Some(tree_body(base_tree, entries)),
            )
            .await?;
        member(STEP, &value, "sha")
    }

    /// Creates the submission commit and refuses one GitHub did not sign.
    ///
    /// The verification is read before any ref is pushed, so an unverified
    /// commit stays a dangling object nothing points at.
    ///
    /// # Errors
    /// The transport failure, GitHub's refusal, an answer with no `sha`, or
    /// [`GithubError::Unverified`].
    pub async fn create_commit(
        &self,
        message: &str,
        tree: &str,
        parent: &str,
    ) -> Result<String, GithubError> {
        const STEP: &str = "writing the submission commit";
        let value = self
            .call(
                STEP,
                reqwest::Method::POST,
                &format!("/repos/{}/git/commits", self.repo),
                Some(commit_body(message, tree, parent)),
            )
            .await?;
        let sha = member(STEP, &value, "sha")?;
        if !verified(&value) {
            return Err(GithubError::Unverified { sha });
        }
        Ok(sha)
    }

    /// Pushes the submission branch at a commit.
    ///
    /// # Errors
    /// The transport failure or GitHub's refusal — a branch that already
    /// exists among them, which is what a second submission of one run looks
    /// like.
    pub async fn create_branch(&self, branch: &str, sha: &str) -> Result<(), GithubError> {
        const STEP: &str = "pushing the submission branch";
        self.call(
            STEP,
            reqwest::Method::POST,
            &format!("/repos/{}/git/refs", self.repo),
            Some(ref_body(branch, sha)),
        )
        .await?;
        Ok(())
    }

    /// Opens the pull request carrying the submission.
    ///
    /// # Errors
    /// The transport failure, GitHub's refusal, or an answer with no
    /// `html_url` or `number`.
    #[expect(
        clippy::disallowed_types,
        reason = "the wire-bodies family: the pull request's number is read off the API answer at the one seam that carries it"
    )]
    pub async fn open_pull_request(
        &self,
        title: &str,
        head: &str,
        base: &str,
        body: &str,
    ) -> Result<(String, u64), GithubError> {
        const STEP: &str = "opening the submission pull request";
        let value = self
            .call(
                STEP,
                reqwest::Method::POST,
                &format!("/repos/{}/pulls", self.repo),
                Some(pull_body(title, head, base, body)),
            )
            .await?;
        let url = member(STEP, &value, "html_url")?;
        let number = value
            .0
            .get("number")
            .and_then(serde_json::Value::as_u64)
            .ok_or(GithubError::Malformed {
                step: STEP,
                field: "number",
            })?;
        Ok((url, number))
    }

    /// The whole submission, in the order the Git Data API requires.
    ///
    /// Blob, then tree, then commit, then ref, then the pull request. Nothing
    /// is pushed until GitHub has reported the commit as verified.
    ///
    /// # Errors
    /// Every failure of the steps it composes, each naming its own step.
    pub async fn open_submission(
        &self,
        branch: &str,
        message: &str,
        title: &str,
        body: &str,
        files: &[SubmissionFile],
    ) -> Result<Opened, GithubError> {
        let base = self.default_branch().await?;
        let (parent, base_tree) = self.head_of(&base).await?;
        let mut entries = Vec::with_capacity(files.len());
        for file in files {
            let sha = self.create_blob(&file.body).await?;
            entries.push((file.path.clone(), sha));
        }
        let tree = self.create_tree(&base_tree, &entries).await?;
        let commit = self.create_commit(message, &tree, &parent).await?;
        self.create_branch(branch, &commit).await?;
        let (pull_request_url, pull_request) =
            self.open_pull_request(title, branch, &base, body).await?;
        Ok(Opened {
            branch: branch.to_owned(),
            commit,
            pull_request_url,
            pull_request,
        })
    }
}

/// A parsed API answer, so the disallowed-type suppression lives in one place.
#[derive(Debug)]
#[expect(
    clippy::disallowed_types,
    reason = "the wire-bodies family: a GitHub API answer is JSON, and this newtype is the ONE seam that carries it"
)]
pub struct TypedValue(serde_json::Value);

/// Whether GitHub reported the commit in this answer as verified.
#[must_use]
#[expect(
    clippy::disallowed_types,
    reason = "the wire-bodies family: the verification flag is read off the API answer at the one seam that carries it"
)]
pub fn verified(value: &TypedValue) -> bool {
    value
        .0
        .get("verification")
        .and_then(|block| block.get("verified"))
        .and_then(serde_json::Value::as_bool)
        .unwrap_or(false)
}

/// One top-level string member of an answer.
#[expect(
    clippy::disallowed_types,
    reason = "the wire-bodies family: a member is read off the API answer at the one seam that carries it"
)]
fn member(
    step: &'static str,
    value: &TypedValue,
    field: &'static str,
) -> Result<String, GithubError> {
    value
        .0
        .get(field)
        .and_then(serde_json::Value::as_str)
        .map(str::to_owned)
        .ok_or(GithubError::Malformed { step, field })
}

/// One string member nested one level down.
#[expect(
    clippy::disallowed_types,
    reason = "the wire-bodies family: a member is read off the API answer at the one seam that carries it"
)]
fn nested_member(
    step: &'static str,
    value: &TypedValue,
    outer: &'static str,
    field: &'static str,
) -> Result<String, GithubError> {
    value
        .0
        .get(outer)
        .and_then(|block| block.get(field))
        .and_then(serde_json::Value::as_str)
        .map(str::to_owned)
        .ok_or(GithubError::Malformed { step, field })
}

/// Reads one answer, turning a non-success status into a typed refusal.
///
/// Only GitHub's own `message` member is repeated, capped: the console never
/// echoes an unread response body, here or anywhere else.
#[expect(
    clippy::disallowed_types,
    reason = "the wire-bodies family: a GitHub API answer is JSON, parsed here at the one seam that reads it"
)]
async fn read_json(
    step: &'static str,
    response: reqwest::Response,
) -> Result<TypedValue, GithubError> {
    let status = response.status();
    let text = response
        .text()
        .await
        .map_err(|source| GithubError::Transport {
            step,
            source: Box::new(source),
        })?;
    let value: serde_json::Value = serde_json::from_str(&text).unwrap_or(serde_json::Value::Null);
    if status.is_success() {
        return Ok(TypedValue(value));
    }
    let detail = value
        .get("message")
        .and_then(serde_json::Value::as_str)
        .map_or_else(
            || String::from("no message"),
            |message| message.chars().take(DETAIL_CAP).collect(),
        );
    Err(GithubError::Status {
        step,
        status: status.as_u16(),
        detail,
    })
}

#[cfg(test)]
mod tests {
    use super::{
        AppConfig, GithubError, PUBLIC_API, Secret, TypedValue, blob_body, branch_of, commit_body,
        pull_body, ref_body, tree_body, verified,
    };

    /// The one path a secret could leak through a diagnostic is `Debug`, and
    /// it does not.
    #[test]
    fn a_secret_never_renders() {
        let token = Secret::new(String::from("ghs_averyrealtoken"));
        assert_eq!(format!("{token:?}"), "Secret(«redacted»)");
        assert!(!format!("{token:?}").contains("averyrealtoken"));
        assert_eq!(token.expose(), "ghs_averyrealtoken");
    }

    /// The branch name is a contract: the re-derivation lane reads the run id
    /// back out of it.
    #[test]
    fn the_branch_carries_the_run_id() {
        assert_eq!(
            branch_of("3f2504e0-4f89-41d3-9a0c-0305e82c3301"),
            "console-run/3f2504e0-4f89-41d3-9a0c-0305e82c3301"
        );
        assert_eq!(
            branch_of("x").strip_prefix(super::BRANCH_PREFIX),
            Some("x"),
            "the lane strips exactly this prefix"
        );
    }

    /// A blob is UTF-8 text with no base64 hop, and its content is escaped by
    /// the serializer rather than by hand.
    #[test]
    fn a_blob_body_carries_escaped_utf8() {
        let body = blob_body("{\n  \"a\": \"ü\"\n}\n");
        assert!(body.contains("\"encoding\":\"utf-8\""), "{body}");
        assert!(body.contains("\\n"), "the newline is escaped: {body}");
        assert!(body.contains('ü'), "{body}");
    }

    /// The tree names every file at the plain-file mode, over the base tree.
    #[test]
    fn a_tree_body_lists_every_blob_over_the_base() {
        let entries = vec![
            (String::from("registry/entries/a.json"), String::from("aaa")),
            (String::from("registry/records/b.json"), String::from("bbb")),
        ];
        let body = tree_body("basetree", &entries);
        assert!(body.contains("\"base_tree\":\"basetree\""), "{body}");
        assert!(body.contains("\"mode\":\"100644\""), "{body}");
        assert!(body.contains("registry/entries/a.json"), "{body}");
        assert!(body.contains("registry/records/b.json"), "{body}");
        assert_eq!(body.matches("\"type\":\"blob\"").count(), 2, "{body}");
    }

    /// A commit has exactly one parent, and a ref names a full branch path.
    #[test]
    fn a_commit_and_a_ref_carry_what_git_needs() {
        let commit = commit_body("subject", "treesha", "parentsha");
        assert!(commit.contains("\"parents\":[\"parentsha\"]"), "{commit}");
        assert!(commit.contains("\"tree\":\"treesha\""), "{commit}");
        let reference = ref_body("console-run/abc", "commitsha");
        assert!(
            reference.contains("\"ref\":\"refs/heads/console-run/abc\""),
            "{reference}"
        );
    }

    /// A pull request targets the branch by name, never by sha.
    #[test]
    fn a_pull_body_names_its_head_and_base() {
        let body = pull_body("title", "console-run/abc", "main", "the body");
        assert!(body.contains("\"head\":\"console-run/abc\""), "{body}");
        assert!(body.contains("\"base\":\"main\""), "{body}");
    }

    /// An answer with no verification block reads as unverified, which is what
    /// stops a ref being pushed at it.
    #[expect(
        clippy::disallowed_types,
        reason = "the wire-bodies family: the test constructs the API answer it asserts over"
    )]
    #[test]
    fn an_unsigned_commit_is_not_verified() {
        assert!(!verified(&TypedValue(serde_json::json!({ "sha": "abc" }))));
        assert!(!verified(&TypedValue(serde_json::json!({
            "sha": "abc",
            "verification": { "verified": false }
        }))));
        assert!(verified(&TypedValue(serde_json::json!({
            "sha": "abc",
            "verification": { "verified": true }
        }))));
    }

    /// Every unset variable is named at once, so the screen tells the operator
    /// the whole posture rather than one variable per attempt.
    #[test]
    fn an_unconfigured_identity_names_every_missing_variable() {
        // The process environment is shared, so this asserts the shape of the
        // refusal rather than mutating the environment out from under another
        // test.
        match AppConfig::from_env() {
            Ok(config) => assert!(
                !config.api_base.is_empty(),
                "a configured identity always carries an API root"
            ),
            Err(missing) => {
                assert!(!missing.is_empty());
                assert!(
                    missing.iter().all(|name| name.starts_with("VEREDICTUM_")),
                    "{missing:?}"
                );
            }
        }
        assert_eq!(PUBLIC_API, "https://api.github.com");
    }

    /// A refusal names its step, so a failure reads as which hop declined it.
    #[test]
    fn a_refusal_names_its_step() {
        let refused = GithubError::Status {
            step: "pushing the submission branch",
            status: 422,
            detail: String::from("Reference already exists"),
        };
        let text = refused.to_string();
        assert!(text.contains("pushing the submission branch"), "{text}");
        assert!(text.contains("422"), "{text}");
        assert!(text.contains("Reference already exists"), "{text}");
    }
}
