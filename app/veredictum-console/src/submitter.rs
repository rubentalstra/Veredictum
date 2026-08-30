// SPDX-FileCopyrightText: Veredictum contributors
// SPDX-License-Identifier: Apache-2.0

//! Who a request came from (#389), as far as a console with no login can
//! tell: the peer address, and nothing stronger.
//!
//! The console has no login by design (#52), so "one run in flight per
//! submitter" and "one connection draft per visitor" both mean one per peer
//! address. That identity is weak on purpose: it bounds accidental and casual
//! load, and it is never an authentication claim.
//!
//! Behind a proxy the peer IS the proxy, so a forwarded header is read ONLY
//! when the operator names one (`state::CLIENT_IP_HEADER_ENV`). Trusting
//! `X-Forwarded-For` unconditionally would let any visitor claim any
//! identity, which would defeat every cap that reads this.

use std::net::IpAddr;

/// One visitor, as far as this console can tell.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Submitter {
    /// The address the request arrived from.
    Peer(IpAddr),
    /// No address could be determined at all.
    ///
    /// One shared anonymous submitter, which is the STRICTEST reading: every
    /// such request competes for the same single per-submitter slot. The
    /// permissive reading — a fresh identity per unattributable request —
    /// would turn the cap off exactly where it is needed.
    Unknown,
}

impl std::fmt::Display for Submitter {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Peer(address) => std::fmt::Display::fmt(address, f),
            Self::Unknown => f.write_str("unattributed"),
        }
    }
}

/// The ONE derivation of a submitter from a request (#134's law).
///
/// `forwarded` is the value of the header the operator named, and it is
/// `None` whenever the operator named none — an unnamed header is never
/// read, however plausible its name. A named header that carries nothing
/// this can parse falls back to the socket peer, which is what a request
/// reaching the origin directly looks like.
///
/// A comma-separated list (`X-Forwarded-For`'s own shape) is read at its
/// FIRST element, which is the client the nearest proxy saw.
#[must_use]
pub fn of_request(forwarded: Option<&str>, peer: Option<IpAddr>) -> Submitter {
    let claimed = forwarded
        .and_then(|value| value.split(',').next())
        .map(str::trim)
        .filter(|first| !first.is_empty())
        .and_then(|first| first.parse::<IpAddr>().ok());
    match claimed.or(peer) {
        Some(address) => Submitter::Peer(address),
        None => Submitter::Unknown,
    }
}

/// The submitter of the request a `#[server]` function is answering.
///
/// The leptos axum integration provides the request's `Parts` in context, so
/// this needs no extractor plumbing per endpoint. Outside a request (a unit
/// test, a background thread) there are no parts and the answer is
/// [`Submitter::Unknown`].
#[cfg(feature = "ssr")]
#[must_use]
pub fn current(state: &crate::state::ConsoleState) -> Submitter {
    let Some(parts) = leptos::prelude::use_context::<axum::http::request::Parts>() else {
        return Submitter::Unknown;
    };
    of_request(
        header_value(state, &parts.headers),
        parts
            .extensions
            .get::<axum::extract::ConnectInfo<std::net::SocketAddr>>()
            .map(|connect| connect.0.ip()),
    )
}

/// The named header's value, when the operator named one and it is text.
///
/// The gatherer the server-fn path and the server-owned axum routes share,
/// so "which header does this deployment trust" is answered in one place.
#[cfg(feature = "ssr")]
#[must_use]
pub fn header_value<'h>(
    state: &crate::state::ConsoleState,
    headers: &'h axum::http::HeaderMap,
) -> Option<&'h str> {
    let name = state.client_ip_header.as_ref()?;
    headers.get(name.as_str())?.to_str().ok()
}

#[cfg(test)]
mod tests {
    use super::{Submitter, of_request};
    use std::net::{IpAddr, Ipv4Addr, Ipv6Addr};

    /// One fixed peer, so the tests below assert a decision and never an
    /// address they invented mid-assertion.
    const PEER: IpAddr = IpAddr::V4(Ipv4Addr::new(198, 51, 100, 7));

    /// With no header named, the socket peer is the whole answer: a header a
    /// visitor set themselves is never read.
    #[test]
    fn an_unnamed_header_is_never_read() {
        assert_eq!(of_request(None, Some(PEER)), Submitter::Peer(PEER));
    }

    /// A named header wins over the peer, because behind a proxy the peer IS
    /// the proxy.
    #[test]
    fn a_named_header_names_the_client() {
        assert_eq!(
            of_request(Some("203.0.113.9"), Some(PEER)),
            Submitter::Peer(IpAddr::V4(Ipv4Addr::new(203, 0, 113, 9)))
        );
        assert_eq!(
            of_request(Some("::1"), Some(PEER)),
            Submitter::Peer(IpAddr::V6(Ipv6Addr::LOCALHOST))
        );
    }

    /// `X-Forwarded-For`'s list shape reads at its first element.
    #[test]
    fn a_forwarded_list_reads_its_first_element() {
        assert_eq!(
            of_request(Some(" 203.0.113.9 , 10.0.0.1 "), Some(PEER)),
            Submitter::Peer(IpAddr::V4(Ipv4Addr::new(203, 0, 113, 9)))
        );
    }

    /// A named header carrying nothing parseable falls back to the peer
    /// rather than to a shared identity: a request reaching the origin
    /// directly is still attributable.
    #[test]
    fn an_unparseable_header_falls_back_to_the_peer() {
        for value in ["", "   ", "not-an-address", ","] {
            assert_eq!(
                of_request(Some(value), Some(PEER)),
                Submitter::Peer(PEER),
                "{value:?}"
            );
        }
    }

    /// No header and no peer is one shared anonymous submitter, never a
    /// fresh identity per request.
    #[test]
    fn an_undeterminable_address_is_one_shared_submitter() {
        assert_eq!(of_request(None, None), Submitter::Unknown);
        assert_eq!(of_request(Some("nonsense"), None), Submitter::Unknown);
        assert_eq!(of_request(None, None), of_request(Some(""), None));
        assert_eq!(Submitter::Unknown.to_string(), "unattributed");
    }
}
