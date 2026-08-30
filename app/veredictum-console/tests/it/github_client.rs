// SPDX-FileCopyrightText: Veredictum contributors
// SPDX-License-Identifier: Apache-2.0

//! The registry client's request sequence (#391), against a stub API.
//!
//! Nobody can hand a test a GitHub App, so this gate never speaks to GitHub.
//! What it does prove is everything the client controls: the App JWT is minted
//! and exchanged for an installation token, every write goes through the Git
//! Data API in the documented order, the tree names each file at the plain-file
//! mode over the base tree, the branch carries the run id, and a commit GitHub
//! declines to sign stops the push instead of landing unverified.
//!
//! The App key is the repository's own committed RSA test key, which holds no
//! account and signs nothing but this fixture.

use veredictum_console::github::{AppConfig, Client, GithubError, SubmissionFile};
use wiremock::matchers::{body_string_contains, method, path};
use wiremock::{Mock, MockServer, Request, ResponseTemplate};

use crate::engine_gate;

/// The stub repository every request below is built against.
const REPO: &str = "example-owner/example-registry";

/// The token the stub hands back. Distinctive, so its absence from every
/// recorded body is provable.
const TOKEN: &str = "ghs_stubinstallationtoken";

/// An identity pointing at the stub server, with the committed RSA test key.
fn identity(server: &MockServer) -> AppConfig {
    AppConfig {
        app_id: String::from("1234567"),
        key_file: engine_gate::repo_root().join("party/smart/cnf-smart-test.key.pem"),
        installation_id: String::from("89012345"),
        repo: String::from(REPO),
        api_base: server.uri(),
    }
}

/// The two files one submission adds, in miniature.
fn files() -> Vec<SubmissionFile> {
    vec![
        SubmissionFile {
            path: String::from("registry/entries/conformance/gate/2026-08-31-console-abc.json"),
            body: String::from("{\n  \"entry_id\": \"2026-08-31-console-abc\"\n}\n"),
        },
        SubmissionFile {
            path: String::from("registry/records/gate/2026-08-31-console-abc/results.json"),
            body: String::from("{\n  \"sut\": {}\n}\n"),
        },
    ]
}

/// Registers the whole happy path on the stub, with the commit reported as
/// verified.
async fn stub_api(server: &MockServer, verified: bool) {
    Mock::given(method("POST"))
        .and(path("/app/installations/89012345/access_tokens"))
        .respond_with(ResponseTemplate::new(201).set_body_string(format!(
            "{{\"token\":\"{TOKEN}\",\"expires_at\":\"2026-08-31T12:00:00Z\"}}"
        )))
        .mount(server)
        .await;
    Mock::given(method("GET"))
        .and(path(format!("/repos/{REPO}")))
        .respond_with(ResponseTemplate::new(200).set_body_string("{\"default_branch\":\"main\"}"))
        .mount(server)
        .await;
    Mock::given(method("GET"))
        .and(path(format!("/repos/{REPO}/git/ref/heads/main")))
        .respond_with(
            ResponseTemplate::new(200).set_body_string("{\"object\":{\"sha\":\"basecommit\"}}"),
        )
        .mount(server)
        .await;
    Mock::given(method("GET"))
        .and(path(format!("/repos/{REPO}/git/commits/basecommit")))
        .respond_with(
            ResponseTemplate::new(200).set_body_string("{\"tree\":{\"sha\":\"basetree\"}}"),
        )
        .mount(server)
        .await;
    Mock::given(method("POST"))
        .and(path(format!("/repos/{REPO}/git/blobs")))
        .respond_with(ResponseTemplate::new(201).set_body_string("{\"sha\":\"blobsha\"}"))
        .mount(server)
        .await;
    Mock::given(method("POST"))
        .and(path(format!("/repos/{REPO}/git/trees")))
        .respond_with(ResponseTemplate::new(201).set_body_string("{\"sha\":\"newtree\"}"))
        .mount(server)
        .await;
    Mock::given(method("POST"))
        .and(path(format!("/repos/{REPO}/git/commits")))
        .respond_with(ResponseTemplate::new(201).set_body_string(format!(
            "{{\"sha\":\"newcommit\",\"verification\":{{\"verified\":{verified}}}}}"
        )))
        .mount(server)
        .await;
    Mock::given(method("POST"))
        .and(path(format!("/repos/{REPO}/git/refs")))
        .respond_with(ResponseTemplate::new(201).set_body_string(
            "{\"ref\":\"refs/heads/console-run/run-1\",\"object\":{\"sha\":\"newcommit\"}}",
        ))
        .mount(server)
        .await;
    Mock::given(method("POST"))
        .and(path(format!("/repos/{REPO}/pulls")))
        .and(body_string_contains("console-run/run-1"))
        .respond_with(ResponseTemplate::new(201).set_body_string(
            "{\"number\":7,\"html_url\":\"https://github.com/example-owner/example-registry/pull/7\"}",
        ))
        .mount(server)
        .await;
}

/// The path a recorded request was sent to.
fn route(request: &Request) -> String {
    format!("{} {}", request.method, request.url.path())
}

#[expect(
    clippy::panic_in_result_fn,
    reason = "the Book ch11 Result-returning test shape: assertions panic, plumbing propagates with ?"
)]
#[tokio::test]
async fn a_submission_is_written_blob_tree_commit_ref() -> Result<(), Box<dyn std::error::Error>> {
    let server = MockServer::start().await;
    stub_api(&server, true).await;
    let client = Client::authenticate(&identity(&server)).await?;
    let opened = client
        .open_submission(
            "console-run/run-1",
            "chore(registry): the console's run of Gate CDR 1.2.3",
            "Console submission: Gate CDR 1.2.3",
            "the body",
            &files(),
        )
        .await?;

    assert_eq!(opened.branch, "console-run/run-1");
    assert_eq!(opened.commit, "newcommit");
    assert_eq!(opened.pull_request, 7);
    assert!(
        opened.pull_request_url.ends_with("example-registry/pull/7"),
        "{}",
        opened.pull_request_url
    );

    let recorded = server.received_requests().await.unwrap_or_default();
    let routes: Vec<String> = recorded.iter().map(route).collect();
    assert_eq!(
        routes,
        vec![
            String::from("POST /app/installations/89012345/access_tokens"),
            format!("GET /repos/{REPO}"),
            format!("GET /repos/{REPO}/git/ref/heads/main"),
            format!("GET /repos/{REPO}/git/commits/basecommit"),
            format!("POST /repos/{REPO}/git/blobs"),
            format!("POST /repos/{REPO}/git/blobs"),
            format!("POST /repos/{REPO}/git/trees"),
            format!("POST /repos/{REPO}/git/commits"),
            format!("POST /repos/{REPO}/git/refs"),
            format!("POST /repos/{REPO}/pulls"),
        ],
        "the Git Data API order is blob, then tree, then commit, then ref: the contents API is never touched"
    );

    // Every write after the exchange carries the installation token, and the
    // API version is pinned so a default change cannot move the answers.
    for request in recorded.iter().skip(1) {
        let authorization = request
            .headers
            .get("authorization")
            .and_then(|value| value.to_str().ok())
            .unwrap_or_default();
        assert_eq!(
            authorization,
            format!("Bearer {TOKEN}"),
            "{}",
            route(request)
        );
        assert_eq!(
            request
                .headers
                .get("x-github-api-version")
                .and_then(|value| value.to_str().ok()),
            Some("2022-11-28"),
            "{}",
            route(request)
        );
    }

    // The blobs carry the exact bytes, as UTF-8 rather than base64.
    let blobs: Vec<String> = recorded
        .iter()
        .filter(|request| request.url.path().ends_with("/git/blobs"))
        .map(|request| String::from_utf8_lossy(&request.body).into_owned())
        .collect();
    assert_eq!(blobs.len(), 2);
    for blob in &blobs {
        assert!(blob.contains("\"encoding\":\"utf-8\""), "{blob}");
    }

    // The tree names each file at the plain-file mode, over the base tree the
    // default branch's commit pointed at.
    let tree = recorded
        .iter()
        .find(|request| request.url.path().ends_with("/git/trees"))
        .map(|request| String::from_utf8_lossy(&request.body).into_owned())
        .ok_or("no tree was written")?;
    assert!(tree.contains("\"base_tree\":\"basetree\""), "{tree}");
    assert!(tree.contains("\"mode\":\"100644\""), "{tree}");
    for file in files() {
        assert!(tree.contains(&file.path), "{tree}");
    }

    // The commit has the base as its only parent, and the ref is the branch
    // the re-derivation lane reads the run id out of.
    let commit = recorded
        .iter()
        .find(|request| request.url.path().ends_with("/git/commits"))
        .map(|request| String::from_utf8_lossy(&request.body).into_owned())
        .ok_or("no commit was written")?;
    assert!(commit.contains("\"parents\":[\"basecommit\"]"), "{commit}");
    assert!(commit.contains("\"tree\":\"newtree\""), "{commit}");
    let reference = recorded
        .iter()
        .find(|request| request.url.path().ends_with("/git/refs"))
        .map(|request| String::from_utf8_lossy(&request.body).into_owned())
        .ok_or("no ref was pushed")?;
    assert!(
        reference.contains("\"ref\":\"refs/heads/console-run/run-1\""),
        "{reference}"
    );
    Ok(())
}

/// A commit GitHub did not sign stops the push. An unverified commit is not an
/// acceptable way to write to this repository, so the ref is never created and
/// the object stays dangling.
#[expect(
    clippy::panic_in_result_fn,
    reason = "the Book ch11 Result-returning test shape: assertions panic, plumbing propagates with ?"
)]
#[tokio::test]
async fn an_unverified_commit_pushes_no_branch() -> Result<(), Box<dyn std::error::Error>> {
    let server = MockServer::start().await;
    stub_api(&server, false).await;
    let client = Client::authenticate(&identity(&server)).await?;
    let refused = client
        .open_submission("console-run/run-1", "subject", "title", "body", &files())
        .await
        .err()
        .ok_or("an unverified commit was accepted")?;
    assert!(
        matches!(refused, GithubError::Unverified { ref sha } if sha == "newcommit"),
        "{refused}"
    );

    let recorded = server.received_requests().await.unwrap_or_default();
    assert!(
        !recorded
            .iter()
            .any(|request| request.url.path().ends_with("/git/refs")),
        "a branch was pushed at an unverified commit"
    );
    assert!(
        !recorded
            .iter()
            .any(|request| request.url.path().ends_with("/pulls")),
        "a pull request was opened for an unverified commit"
    );
    Ok(())
}

/// GitHub's refusal is a typed error naming the step, with GitHub's own
/// message and nothing else from the answer.
#[expect(
    clippy::panic_in_result_fn,
    reason = "the Book ch11 Result-returning test shape: assertions panic, plumbing propagates with ?"
)]
#[tokio::test]
async fn a_refused_write_names_its_step() -> Result<(), Box<dyn std::error::Error>> {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/app/installations/89012345/access_tokens"))
        .respond_with(ResponseTemplate::new(201).set_body_string(format!(
            "{{\"token\":\"{TOKEN}\",\"expires_at\":\"2026-08-31T12:00:00Z\"}}"
        )))
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path(format!("/repos/{REPO}")))
        .respond_with(ResponseTemplate::new(404).set_body_string(
            "{\"message\":\"Not Found\",\"documentation_url\":\"https://docs.github.com/rest\"}",
        ))
        .mount(&server)
        .await;
    let client = Client::authenticate(&identity(&server)).await?;
    let refused = client
        .open_submission("console-run/run-1", "subject", "title", "body", &files())
        .await
        .err()
        .ok_or("a 404 was accepted")?;
    let text = refused.to_string();
    assert!(text.contains("reading the registry repository"), "{text}");
    assert!(text.contains("404"), "{text}");
    assert!(text.contains("Not Found"), "{text}");
    assert!(
        !text.contains("documentation_url"),
        "the client repeats GitHub's message and no more of the answer: {text}"
    );
    Ok(())
}

/// A minted App JWT is an RS256 token whose issuer is the App, which is what
/// the token exchange authenticates with.
#[expect(
    clippy::panic_in_result_fn,
    reason = "the Book ch11 Result-returning test shape: assertions panic, plumbing propagates with ?"
)]
#[tokio::test]
async fn the_app_jwt_is_rs256_and_never_rendered() -> Result<(), Box<dyn std::error::Error>> {
    let server = MockServer::start().await;
    stub_api(&server, true).await;
    let jwt = veredictum_console::github::app_jwt(&identity(&server))?;
    assert_eq!(
        format!("{jwt:?}"),
        "Secret(«redacted»)",
        "a Debug of the token would put it in a diagnostic"
    );
    let header = jsonwebtoken::decode_header(jwt.expose())?;
    assert_eq!(header.alg, jsonwebtoken::Algorithm::RS256);
    assert_eq!(
        jwt.expose().split('.').count(),
        3,
        "a compact JWS has three parts"
    );
    Ok(())
}
