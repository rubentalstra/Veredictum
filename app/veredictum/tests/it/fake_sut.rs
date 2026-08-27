// SPDX-FileCopyrightText: Veredictum contributors
// SPDX-License-Identifier: Apache-2.0

//! The fake-SUT harness the wire-speaking modules are driven against.
//!
//! `wiremock` answers on a real socket, so the driver, the AQL probe and the
//! stress ladder run over the same `reqwest` path a live campaign uses —
//! request construction, transport, status, headers and body all real. What
//! the harness controls is the answer, which is the half a live SUT cannot be
//! asked to produce on demand.
//!
//! The control surface (`start`, `register`, `received_requests`) is async
//! while every client in this crate is `reqwest::blocking`, so [`FakeSut`]
//! owns one current-thread runtime for setup and readback and the instrument
//! is driven outside it. The mock server listens on its own thread with its
//! own runtime
//! (<https://docs.rs/wiremock/0.6.5/wiremock/struct.MockServer.html#method.start>),
//! so a blocking request never re-enters the harness runtime.

#![expect(
    clippy::unwrap_used,
    reason = "test-support helpers (not `#[test]` fns, so the clippy.toml in-tests scoping does not reach them) are panic-idiomatic: a broken harness must abort the test loudly, Book ch11"
)]

use std::path::PathBuf;

use serde_json::{Value, json};
use veredictum::artifacts::ArtifactSet;
use veredictum::ixit::Ixit;
use veredictum::model::case::CaseCore;

/// A running fake SUT: the mock server plus the runtime its async control
/// surface is driven on.
pub(crate) struct FakeSut {
    // Declaration order IS drop order: the server is torn down while its
    // runtime is still alive.
    server: wiremock::MockServer,
    runtime: tokio::runtime::Runtime,
}

impl FakeSut {
    /// Start a fresh server on an OS-assigned port.
    pub(crate) fn start() -> Self {
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        // `builder()` gives a dedicated server rather than a pooled one, so
        // nothing this harness holds outlives the test that made it.
        let server = runtime.block_on(wiremock::MockServer::builder().start());
        Self { server, runtime }
    }

    /// The server's base URL (`http://127.0.0.1:<port>`).
    pub(crate) fn base_url(&self) -> String {
        self.server.uri()
    }

    /// Mount one stubbed answer.
    pub(crate) fn mount(&self, mock: wiremock::Mock) {
        self.runtime.block_on(self.server.register(mock));
    }

    /// Every request the server received, in arrival order.
    pub(crate) fn requests(&self) -> Vec<wiremock::Request> {
        self.runtime
            .block_on(self.server.received_requests())
            .unwrap_or_default()
    }
}

/// A port nothing is listening on: bind it, read the address, drop the
/// listener. The next connection there is refused, which is the transport
/// fault the attribution law classifies as inconclusive.
pub(crate) fn closed_port_url() -> String {
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap();
    drop(listener);
    format!("http://{address}")
}

/// The selector vocabulary the driver consults for the route-table-wide
/// outcomes: 401 and 403 map for every operation, so no binding repeats them
/// (ITS-REST `Requests_and_responses.md` §HTTP status codes rows 401/403).
fn selectors() -> veredictum::model::vocab_files::SelectorsVocab {
    serde_saphyr::from_str(
        r#"
body_selectors: [prefer_conditional, error_loose, result_set_body, negotiated, present, absent]
header_matchers: ["present", "present?", "absent", "negotiated", "latest-version-uid", "pattern:<regex>", "<literal>"]
ignore_sets:
  server_assigned: { per_binding: true, source: "ITS-REST overview, HTTP headers" }
  ctx_defaults: { paths: [context/start_time], source: "ITS-REST overview, HTTP headers" }
universal_outcomes:
  unauthenticated: { status: 401, source: "ITS-REST overview, HTTP status codes" }
  forbidden: { status: 403, source: "ITS-REST overview, HTTP status codes" }
"#,
    )
    .unwrap()
}

/// An artifact set carrying the given operation bindings, the universal
/// outcome vocabulary, and the empty corpus the driver's resolver needs.
pub(crate) fn artifact_set(bindings: &[Value]) -> ArtifactSet {
    let mut set = ArtifactSet::default();
    for (index, binding) in bindings.iter().enumerate() {
        set.bindings.push((
            PathBuf::from(format!("bindings/its-rest/{index}.yaml")),
            serde_json::from_value(binding.clone()).unwrap(),
        ));
    }
    set.corpus = Some((
        PathBuf::from("corpus/MANIFEST.yaml"),
        serde_json::from_value(json!({})).unwrap(),
    ));
    set.corpus_dir = Some(PathBuf::from("corpus"));
    set.selectors = Some((PathBuf::from("vocab/selectors.yaml"), selectors()));
    set
}

/// A single-instance ixit addressing the fake SUT with no credential.
pub(crate) fn ixit(base_url: &str) -> Ixit {
    serde_json::from_value(json!({
        "instances": { "sut": { "base_url": base_url, "auth": { "mode": "none" } } }
    }))
    .unwrap()
}

/// A case core from its authored JSON form.
pub(crate) fn case(document: Value) -> CaseCore {
    serde_json::from_value(document).unwrap()
}
