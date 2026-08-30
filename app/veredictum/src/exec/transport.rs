// SPDX-FileCopyrightText: Veredictum contributors
// SPDX-License-Identifier: Apache-2.0

//! The one place bytes leave the process, behind one trait.
//!
//! [`HttpDriver`](crate::exec::driver::HttpDriver) composes every request
//! from the operation bindings, classifies every response and judges every
//! assertion. None of that touches a socket: [`Transport`] does, and it is
//! the only seam between the driver and the wire.
//!
//! That is what makes a re-judgement worth something. [`ReplayTransport`]
//! answers a composed request from a recorded exchange instead of from a
//! server, so a transcript replays through the same composition, the same
//! classification and the same assertion evaluators the live run used — a
//! second reading of the evidence rather than a second implementation.

use std::collections::BTreeMap;

use reqwest::StatusCode;
use serde_json::Value;

use crate::transcript::{CaseTranscript, RecordedExchange};
use crate::vocab::HttpMethod;

/// How long a live exchange may take before the driver calls it a transport
/// failure.
const TIMEOUT_SECS: u64 = 30;

/// One response as a transport delivered it, before the driver reads it.
#[derive(Debug, Clone)]
pub struct RawResponse {
    /// The status code the answer carried.
    pub status: StatusCode,
    /// The response headers, names as the answer spelled them.
    pub headers: BTreeMap<String, String>,
    /// The response body as bytes-turned-text; empty when there was none.
    pub text: String,
}

/// Where a composed request is sent, and where its answer comes from.
///
/// # Errors
/// Implementations return the verbatim reason an exchange could not be
/// performed. The driver turns that into a transport observation, never into
/// a pass.
pub trait Transport {
    /// Perform one exchange.
    ///
    /// # Errors
    /// The verbatim reason the exchange could not be performed.
    fn exchange(
        &mut self,
        method: HttpMethod,
        url: &str,
        headers: &BTreeMap<String, String>,
        body: Option<&Value>,
        body_is_json: bool,
    ) -> Result<RawResponse, String>;
}

/// The live transport: one blocking HTTP client against the SUT.
#[derive(Debug)]
pub struct HttpTransport {
    client: reqwest::blocking::Client,
}

impl HttpTransport {
    /// Builds the client the driver sends through.
    ///
    /// # Errors
    /// The client builder's verbatim failure.
    pub fn new() -> Result<Self, String> {
        let client = reqwest::blocking::Client::builder()
            .timeout(std::time::Duration::from_secs(TIMEOUT_SECS))
            .build()
            .map_err(|e| format!("http client: {e}"))?;
        Ok(Self { client })
    }
}

impl Transport for HttpTransport {
    fn exchange(
        &mut self,
        method: HttpMethod,
        url: &str,
        headers: &BTreeMap<String, String>,
        body: Option<&Value>,
        body_is_json: bool,
    ) -> Result<RawResponse, String> {
        let mut request = self.client.request(reqwest_method(method), url);
        for (name, value) in headers {
            request = request.header(name, value);
        }
        if let Some(payload) = body {
            request = if body_is_json {
                request.body(serde_json::to_vec(payload).map_err(|e| e.to_string())?)
            } else {
                match payload {
                    Value::String(text) => request.body(text.clone()),
                    other => request.body(serde_json::to_vec(other).map_err(|e| e.to_string())?),
                }
            };
        }
        let response = request.send().map_err(|e| format!("transport: {e}"))?;
        let status = response.status();
        let mut response_headers = BTreeMap::new();
        for (name, value) in response.headers() {
            if let Ok(v) = value.to_str() {
                response_headers.insert(name.as_str().to_owned(), v.to_owned());
            }
        }
        let text = response.text().map_err(|e| format!("transport: {e}"))?;
        Ok(RawResponse {
            status,
            headers: response_headers,
            text,
        })
    }
}

/// The method as the HTTP client spells it.
fn reqwest_method(method: HttpMethod) -> reqwest::Method {
    match method {
        HttpMethod::Get => reqwest::Method::GET,
        HttpMethod::Post => reqwest::Method::POST,
        HttpMethod::Put => reqwest::Method::PUT,
        HttpMethod::Delete => reqwest::Method::DELETE,
        HttpMethod::Head => reqwest::Method::HEAD,
        HttpMethod::Options => reqwest::Method::OPTIONS,
    }
}

/// The replay transport: one case's recorded exchanges, answered in order.
///
/// Matching is POSITIONAL, and the method is checked. The driver composes a
/// case's requests in a fixed order from the same catalogue and the same
/// ixit, so the Nth composed request is the Nth recorded one; a method that
/// disagrees means the replay diverged from the recording, and a divergence
/// is refused rather than judged. Paths are deliberately not compared: a
/// request can carry an identifier the instrument minted rather than one the
/// recording captured, and refusing on that would refuse a faithful replay.
#[derive(Debug)]
pub struct ReplayTransport<'a> {
    exchanges: &'a [RecordedExchange],
    cursor: usize,
}

impl<'a> ReplayTransport<'a> {
    /// Replays one case's recorded exchanges, oldest first.
    #[must_use]
    pub fn new(transcript: &'a CaseTranscript) -> Self {
        Self {
            exchanges: &transcript.exchanges,
            cursor: 0,
        }
    }

    /// Replays a bare exchange list.
    #[must_use]
    pub fn over(exchanges: &'a [RecordedExchange]) -> Self {
        Self {
            exchanges,
            cursor: 0,
        }
    }

    /// How many recorded exchanges the replay has not reached.
    #[must_use]
    pub fn remaining(&self) -> usize {
        self.exchanges.len().saturating_sub(self.cursor)
    }
}

impl Transport for ReplayTransport<'_> {
    fn exchange(
        &mut self,
        method: HttpMethod,
        _url: &str,
        _headers: &BTreeMap<String, String>,
        _body: Option<&Value>,
        _body_is_json: bool,
    ) -> Result<RawResponse, String> {
        let Some(recorded) = self.exchanges.get(self.cursor) else {
            return Err(format!(
                "the transcript records {} exchange(s) and the replay asked for one more",
                self.exchanges.len()
            ));
        };
        self.cursor = self.cursor.saturating_add(1);
        let sent = format!("{method:?}").to_uppercase();
        if !recorded.request.method.eq_ignore_ascii_case(&sent) {
            return Err(format!(
                "exchange {} was recorded as {} and the replay composed {sent}: the replay \
                 diverged from the recording",
                recorded.seq, recorded.request.method
            ));
        }
        let status = StatusCode::from_u16(recorded.response.status).map_err(|reason| {
            format!(
                "exchange {} recorded status {}, which is not an HTTP status code ({reason})",
                recorded.seq, recorded.response.status
            )
        })?;
        Ok(RawResponse {
            status,
            headers: recorded.response.headers.clone(),
            text: recorded_text(recorded.response.body.as_ref()),
        })
    }
}

/// The response text a recorded body stands for.
///
/// A transcript stores the body as the driver read it: a JSON document, or
/// the served text verbatim as a string when it did not parse. Reversing that
/// is exact for every document, and for a string it hands back the served
/// text, which is what the driver read off the wire in the first place.
fn recorded_text(body: Option<&Value>) -> String {
    match body {
        None | Some(Value::Null) => String::new(),
        Some(Value::String(text)) => text.clone(),
        Some(document) => document.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::{ReplayTransport, Transport, recorded_text};
    use crate::transcript::{RecordedExchange, RecordedRequest, RecordedResponse};
    use crate::vocab::HttpMethod;
    use serde_json::{Value, json};
    use std::collections::BTreeMap;

    fn exchange(seq: u32, method: &str, status: u16, body: Option<Value>) -> RecordedExchange {
        RecordedExchange {
            seq,
            row: 1,
            request: RecordedRequest {
                method: method.to_owned(),
                url: String::from("https://cdr.example/openehr/v1/ehr"),
                headers: BTreeMap::new(),
                body: None,
            },
            response: RecordedResponse {
                status,
                headers: BTreeMap::new(),
                body,
            },
        }
    }

    /// The recording answers in order, and the answer carries the recorded
    /// status and body back to the driver unchanged.
    #[test]
    fn the_replay_answers_each_composed_request_from_the_next_recorded_exchange() {
        let recorded = vec![
            exchange(1, "POST", 201, Some(json!({"uid": {"value": "a"}}))),
            exchange(2, "GET", 200, Some(json!({"uid": {"value": "a"}}))),
        ];
        let mut replay = ReplayTransport::over(&recorded);
        let first = replay
            .exchange(
                HttpMethod::Post,
                "https://x/ehr",
                &BTreeMap::new(),
                None,
                true,
            )
            .expect("the first recorded exchange answers");
        assert_eq!(first.status.as_u16(), 201);
        assert_eq!(replay.remaining(), 1);
        let second = replay
            .exchange(
                HttpMethod::Get,
                "https://x/ehr/a",
                &BTreeMap::new(),
                None,
                true,
            )
            .expect("the second recorded exchange answers");
        assert_eq!(second.status.as_u16(), 200);
        assert_eq!(replay.remaining(), 0);
    }

    /// A replay that composes a different method has diverged from the
    /// recording, and a divergence is refused rather than judged.
    #[test]
    fn a_method_the_recording_does_not_carry_is_refused() {
        let recorded = vec![exchange(1, "POST", 201, None)];
        let mut replay = ReplayTransport::over(&recorded);
        let refusal = replay
            .exchange(
                HttpMethod::Delete,
                "https://x/ehr",
                &BTreeMap::new(),
                None,
                true,
            )
            .expect_err("a divergent method must be refused");
        assert!(refusal.contains("diverged from the recording"), "{refusal}");
    }

    /// Asking past the end of a recording is a refusal too: a replay that
    /// needs an exchange nobody recorded cannot reproduce a verdict.
    #[test]
    fn a_replay_that_outruns_the_recording_is_refused() {
        let recorded = vec![exchange(1, "GET", 200, None)];
        let mut replay = ReplayTransport::over(&recorded);
        let _first = replay
            .exchange(HttpMethod::Get, "https://x", &BTreeMap::new(), None, true)
            .expect("the one recorded exchange answers");
        let refusal = replay
            .exchange(HttpMethod::Get, "https://x", &BTreeMap::new(), None, true)
            .expect_err("an exhausted recording must be refused");
        assert!(refusal.contains("asked for one more"), "{refusal}");
    }

    /// A recorded body reverses to the text the driver read: a document to
    /// its serialization, an unparsed body to the served text itself.
    #[test]
    fn a_recorded_body_reverses_to_what_was_served() {
        assert_eq!(recorded_text(None), "");
        assert_eq!(recorded_text(Some(&Value::Null)), "");
        assert_eq!(
            recorded_text(Some(&Value::String(String::from("<xml/>")))),
            "<xml/>"
        );
        assert_eq!(recorded_text(Some(&json!({"a": 1}))), r#"{"a":1}"#);
    }
}
