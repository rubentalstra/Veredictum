// SPDX-FileCopyrightText: Veredictum contributors
// SPDX-License-Identifier: Apache-2.0

//! Targeting and credentials: a base URL plus one credential is the whole
//! ceremony a bench run asks for.
//!
//! Secrets never ride argv. `--auth basic` takes its password from
//! [`PASSWORD_ENV`] and `--auth bearer` its token from [`TOKEN_ENV`], so a
//! credential is never visible to every process on the host through the
//! command line.

use std::fmt;
use std::io::Read as _;
use std::time::Duration;

use base64::Engine as _;

use crate::bench::BenchError;

/// Where `--auth basic` reads its password from.
pub const PASSWORD_ENV: &str = "VEREDICTUM_BENCH_PASSWORD";

/// Where `--auth bearer` reads its token from.
pub const TOKEN_ENV: &str = "VEREDICTUM_BENCH_TOKEN";

/// Per-request timeout. A response slower than this is a timeout arrival,
/// counted in its own error class rather than waited on forever.
pub const CLIENT_TIMEOUT: Duration = Duration::from_secs(30);

/// The closed `--auth` vocabulary.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AuthKind {
    /// No `Authorization` header.
    None,
    /// HTTP Basic, with the user from `--user` and the password from
    /// [`PASSWORD_ENV`].
    Basic,
    /// A bearer token from [`TOKEN_ENV`].
    Bearer,
}

impl AuthKind {
    /// Every mode, in the order `--auth` documents them.
    pub const ALL: &[AuthKind] = &[AuthKind::None, AuthKind::Basic, AuthKind::Bearer];

    /// The token as written on the command line.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            AuthKind::None => "none",
            AuthKind::Basic => "basic",
            AuthKind::Bearer => "bearer",
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
            .find(|mode| mode.as_str() == token)
            .ok_or_else(|| BenchError::UnknownToken {
                vocabulary: "auth mode",
                token: token.to_owned(),
                accepted: Self::ALL
                    .iter()
                    .map(|mode| mode.as_str())
                    .collect::<Vec<_>>()
                    .join(", "),
            })
    }
}

impl fmt::Display for AuthKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// The `Prefer: return=…` preference a write states.
///
/// The three tokens are the ones ITS-REST `docs/overview/Requests_and_responses`
/// §"Prefer minimal, identifier or full representation response" defines. The
/// same section notes that a client is encouraged to state the preference
/// explicitly, because a service MAY make its default configurable.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PreferReturn {
    /// No `Prefer` header at all, which the same section says the service
    /// treats as `return=minimal`.
    Unstated,
    /// `return=minimal`: identifying headers, and typically no body.
    Minimal,
    /// `return=identifier`: a body carrying only the affected resource's
    /// `uid`, with `201 Created` or `200 OK` and never `204 No Content`.
    Identifier,
}

impl PreferReturn {
    /// Every preference, in the order the specification section lists them.
    pub const ALL: &[PreferReturn] = &[
        PreferReturn::Unstated,
        PreferReturn::Minimal,
        PreferReturn::Identifier,
    ];

    /// The header value to send, or `None` when no header is sent.
    #[must_use]
    pub const fn header_value(self) -> Option<&'static str> {
        match self {
            PreferReturn::Unstated => None,
            PreferReturn::Minimal => Some("return=minimal"),
            PreferReturn::Identifier => Some("return=identifier"),
        }
    }
}

impl fmt::Display for PreferReturn {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.header_value().unwrap_or("(unstated)"))
    }
}

/// One wire response, as much of it as the engine reads.
#[derive(Debug)]
pub struct BenchReply {
    /// The status the SUT answered with.
    pub status: reqwest::StatusCode,
    /// The `ETag` header, when one was sent.
    pub etag: Option<String>,
    /// The `Location` header, when one was sent.
    pub location: Option<String>,
    /// The response body, drained so the pooled connection stays reusable.
    pub body: Vec<u8>,
}

/// The blocking client every bench exchange rides.
///
/// One connection pool, shared by every seeding worker and every measured
/// arrival, so the numbers describe the SUT rather than connection setup.
#[derive(Clone)]
pub struct BenchClient {
    client: reqwest::blocking::Client,
    base_url: String,
    authorization: Option<String>,
}

impl fmt::Debug for BenchClient {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("BenchClient")
            .field("base_url", &self.base_url)
            .finish_non_exhaustive()
    }
}

impl BenchClient {
    /// Builds the client from the command line's targeting arguments.
    ///
    /// # Errors
    /// [`BenchError::MissingUser`] when `--auth basic` carries no user,
    /// [`BenchError::Credential`] when the credential variable is unset, and
    /// [`BenchError::Client`] when the HTTP client cannot be built.
    pub fn new(base_url: &str, auth: AuthKind, user: Option<&str>) -> Result<Self, BenchError> {
        let authorization = match auth {
            AuthKind::None => None,
            AuthKind::Basic => {
                let user = user.ok_or(BenchError::MissingUser)?;
                let password =
                    std::env::var(PASSWORD_ENV).map_err(|source| BenchError::Credential {
                        name: PASSWORD_ENV,
                        source,
                    })?;
                let encoded = base64::engine::general_purpose::STANDARD
                    .encode(format!("{user}:{password}").as_bytes());
                Some(format!("Basic {encoded}"))
            }
            AuthKind::Bearer => {
                let token = std::env::var(TOKEN_ENV).map_err(|source| BenchError::Credential {
                    name: TOKEN_ENV,
                    source,
                })?;
                Some(format!("Bearer {token}"))
            }
        };
        let client = reqwest::blocking::Client::builder()
            .timeout(CLIENT_TIMEOUT)
            .pool_max_idle_per_host(256)
            .redirect(reqwest::redirect::Policy::none())
            .build()
            .map_err(|source| BenchError::Client { source })?;
        Ok(Self {
            client,
            base_url: base_url.trim_end_matches('/').to_owned(),
            authorization,
        })
    }

    /// The base URL this client drives, with any userinfo removed.
    #[must_use]
    pub fn recorded_base_url(&self) -> String {
        strip_userinfo(&self.base_url)
    }

    /// Issues one request against an absolute path under the base URL.
    ///
    /// `exchange` names the call for the diagnostic a transport fault
    /// carries, which is what a refused preflight reports.
    ///
    /// # Errors
    /// [`BenchError::Transport`] when the request never reached a response.
    pub fn send(
        &self,
        exchange: &str,
        method: reqwest::Method,
        path: &str,
        body: Option<(&'static str, Vec<u8>)>,
        prefer: PreferReturn,
    ) -> Result<BenchReply, BenchError> {
        let accept = match &body {
            Some((media_type, _)) if media_type.contains("xml") => "application/xml",
            _ => "application/json",
        };
        let mut request = self
            .client
            .request(method, format!("{}{path}", self.base_url))
            .header("Accept", accept);
        if let Some(authorization) = &self.authorization {
            request = request.header("Authorization", authorization);
        }
        if let Some(preference) = prefer.header_value() {
            request = request.header("Prefer", preference);
        }
        if let Some((media_type, bytes)) = body {
            request = request.header("Content-Type", media_type).body(bytes);
        }
        let response = request.send().map_err(|source| BenchError::Transport {
            exchange: exchange.to_owned(),
            source,
        })?;
        let header = |name: &str| {
            response
                .headers()
                .get(name)
                .and_then(|value| value.to_str().ok())
                .map(str::to_owned)
        };
        let status = response.status();
        let etag = header("etag");
        let location = header("location");
        let mut sink = Vec::new();
        let mut reader = response;
        let _drained = reader.read_to_end(&mut sink);
        Ok(BenchReply {
            status,
            etag,
            location,
            body: sink,
        })
    }
}

/// Removes any `user:password@` from a URL's authority.
///
/// A recorded target must not republish a credential someone typed into a
/// base URL. Returns the URL unchanged when it carries no userinfo.
#[must_use]
pub fn strip_userinfo(url: &str) -> String {
    let Some((scheme, rest)) = url.split_once("://") else {
        return url.to_owned();
    };
    let (authority, path) = match rest.find('/') {
        Some(cut) => rest.split_at(cut),
        None => (rest, ""),
    };
    let Some((_userinfo, host)) = authority.rsplit_once('@') else {
        return url.to_owned();
    };
    format!("{scheme}://{host}{path}")
}

/// `W/"uid"` or `"uid"` becomes `uid` (ITS-REST overview §Concurrency
/// control describes the weak-validator form the create answers with).
#[must_use]
pub fn strip_weak_quotes(etag: &str) -> String {
    etag.trim_start_matches("W/").trim_matches('"').to_owned()
}

/// The last path segment of a `Location` header, which is the identifier a
/// `return=minimal` create discloses.
#[must_use]
pub fn location_last_segment(location: &str) -> Option<String> {
    location
        .rsplit('/')
        .next()
        .filter(|segment| !segment.is_empty())
        .map(str::to_owned)
}

/// The identifier a create discloses, whichever way it discloses one.
///
/// A `return=identifier` reply carries a body of one `uid` attribute
/// (ITS-REST `docs/overview/Requests_and_responses` §"Prefer only
/// identifier"), a `return=minimal` reply carries the identifying headers
/// instead, and the same operation answers differently across the two
/// preferences. The order below reads the body first, because it is the only
/// source the identifier preference guarantees, and falls back to the two
/// headers a create sends. Returns `None` when the reply discloses no
/// identifier at all, which is the caller's failure to report.
#[must_use]
#[expect(
    clippy::disallowed_types,
    reason = "the approved wire-body seam: an identifier reply is a JSON document read for one attribute"
)]
pub fn created_identifier(reply: &BenchReply) -> Option<String> {
    let from_body = serde_json::from_slice::<serde_json::Value>(&reply.body)
        .ok()
        .and_then(|document| {
            document.pointer("/uid").and_then(|uid| {
                uid.as_str().map(str::to_owned).or_else(|| {
                    uid.pointer("/value")
                        .and_then(serde_json::Value::as_str)
                        .map(str::to_owned)
                })
            })
        })
        .filter(|uid| !uid.is_empty());
    from_body
        .or_else(|| reply.etag.as_deref().map(strip_weak_quotes))
        .or_else(|| reply.location.as_deref().and_then(location_last_segment))
        .filter(|identifier| !identifier.is_empty())
}

/// One query-parameter value, percent-encoded for a URL.
///
/// An RFC 3339 instant carries `:` and `+`, and `+` in a query means a space
/// unless it is encoded (<https://url.spec.whatwg.org/#urlencoded-parsing>).
#[must_use]
pub fn query_value(value: &str) -> String {
    urlencoding::encode(value).into_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A credential typed into the base URL never reaches the artifact.
    #[test]
    fn userinfo_is_stripped_from_a_recorded_target() {
        assert_eq!(
            strip_userinfo("https://alice:s3cret@cdr.example/openehr/v1"),
            "https://cdr.example/openehr/v1"
        );
        assert_eq!(
            strip_userinfo("https://alice@cdr.example"),
            "https://cdr.example"
        );
        assert_eq!(
            strip_userinfo("http://127.0.0.1:8080/rest/openehr/v1"),
            "http://127.0.0.1:8080/rest/openehr/v1"
        );
        assert_eq!(strip_userinfo("not-a-url"), "not-a-url");
    }

    /// The two identifier captures follow the same forms the functional
    /// bindings capture from.
    #[test]
    fn identifier_captures_match_the_wire_forms() {
        assert_eq!(strip_weak_quotes("W/\"abc::sys::1\""), "abc::sys::1");
        assert_eq!(strip_weak_quotes("\"abc\""), "abc");
        assert_eq!(
            location_last_segment("http://sut/ehr/42").as_deref(),
            Some("42")
        );
        assert_eq!(location_last_segment("http://sut/ehr/"), None);
    }

    /// An unknown `--auth` token is refused rather than defaulting to `none`,
    /// which would silently measure an unauthenticated surface.
    #[test]
    fn an_unknown_auth_token_is_refused() {
        assert_eq!(AuthKind::parse("bearer").ok(), Some(AuthKind::Bearer));
        let error = AuthKind::parse("Bearer").unwrap_err();
        assert!(error.to_string().contains("none, basic, bearer"), "{error}");
    }

    /// Each preference sends the header value the specification names, and
    /// the unstated preference sends no header at all.
    #[test]
    fn every_preference_sends_its_own_header_value() {
        assert_eq!(PreferReturn::Unstated.header_value(), None);
        assert_eq!(PreferReturn::Minimal.header_value(), Some("return=minimal"));
        assert_eq!(
            PreferReturn::Identifier.header_value(),
            Some("return=identifier")
        );
        assert_eq!(PreferReturn::ALL.len(), 3);
    }

    /// A `return=identifier` body wins over the headers, and a reply that
    /// discloses nothing yields `None` rather than an empty identifier.
    #[test]
    fn a_created_identifier_reads_the_body_before_the_headers() {
        let reply = |body: &str, etag: Option<&str>, location: Option<&str>| BenchReply {
            status: reqwest::StatusCode::CREATED,
            etag: etag.map(str::to_owned),
            location: location.map(str::to_owned),
            body: body.as_bytes().to_vec(),
        };
        assert_eq!(
            created_identifier(&reply(
                r#"{"uid":"body::sys::1"}"#,
                Some("\"etag::sys::1\""),
                Some("http://sut/ehr/loc")
            ))
            .as_deref(),
            Some("body::sys::1")
        );
        assert_eq!(
            created_identifier(&reply(r#"{"uid":{"value":"nested::sys::1"}}"#, None, None))
                .as_deref(),
            Some("nested::sys::1")
        );
        assert_eq!(
            created_identifier(&reply("", Some("W/\"etag::sys::1\""), None)).as_deref(),
            Some("etag::sys::1")
        );
        assert_eq!(
            created_identifier(&reply("", None, Some("http://sut/ehr/EHR-9"))).as_deref(),
            Some("EHR-9")
        );
        assert_eq!(created_identifier(&reply("", None, None)), None);
    }

    /// An RFC 3339 instant survives a query parameter intact.
    #[test]
    fn a_query_value_is_percent_encoded() {
        assert_eq!(
            query_value("2026-08-29T10:11:12.5Z"),
            "2026-08-29T10%3A11%3A12.5Z"
        );
        assert!(!query_value("2026-08-29T10:11:12+02:00").contains('+'));
    }

    /// `--auth basic` without `--user` is refused before any request.
    #[test]
    fn basic_without_a_user_is_refused() {
        let error = BenchClient::new("http://stub", AuthKind::Basic, None).unwrap_err();
        assert!(matches!(error, BenchError::MissingUser), "{error}");
    }
}
