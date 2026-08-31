// SPDX-FileCopyrightText: Veredictum contributors
// SPDX-License-Identifier: Apache-2.0

//! The target guard (#390): under the hosted posture, an address only this
//! instance can reach is refused before a socket opens.
//!
//! A public console drives whatever endpoint a visitor names, so a target at
//! `127.0.0.1`, inside `10.0.0.0/8` or on `fc00::/7` is a request the visitor
//! could not have made themselves — the shape of a server-side request
//! forgery. Two seams reach a visitor-named endpoint and both come through
//! here: the reachability probe (`run_api::read::probe`, the one carved-out
//! console-originated request, #54) and the run start, before the engine is
//! spawned.
//!
//! **Resolution happens first and every resolved address is checked.** A
//! hostname under the visitor's control that resolves to a private address is
//! the whole attack, so checking the literal text would be no check at all.
//!
//! What this cannot do, stated rather than papered over: a name that resolves
//! to one address here and another at connect time defeats it, and the
//! spawned engine resolves the ixit's `base_url` again for itself. The guard
//! refuses what a guard can refuse.
//!
//! [`crate::posture::Posture::Local`] refuses nothing at all: an operator
//! driving a CDR at `localhost` from their own laptop is the normal case, and
//! the browser journeys drive exactly that.

use std::net::{IpAddr, Ipv4Addr, Ipv6Addr, ToSocketAddrs};

use crate::posture::Posture;

/// The port assumed for a URL whose scheme has no default and names none.
///
/// Only the resolver reads it; the console opens no socket here, and a
/// scheme with no default port is refused by the HTTP client anyway.
const ASSUMED_PORT: u16 = 443;

/// An address family a hosted instance refuses.
///
/// One variant per family the run wizard can be pointed at, each carrying the
/// released RFC that defines it, because the refusal a visitor reads has to
/// say what was refused and on whose authority.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RefusedFamily {
    /// `127.0.0.0/8` and `::1`.
    Loopback,
    /// `10/8`, `172.16/12` and `192.168/16`.
    Private,
    /// `169.254.0.0/16` and `fe80::/10`.
    LinkLocal,
    /// `fc00::/7`.
    UniqueLocal,
    /// `0.0.0.0` and `::`.
    Unspecified,
    /// `224.0.0.0/4` and `ff00::/8`.
    Multicast,
    /// `100.64.0.0/10`, the shared space a carrier hands its own subscribers.
    Shared,
    /// `255.255.255.255`.
    Broadcast,
}

impl RefusedFamily {
    /// What the family is, and the released RFC that defines it.
    #[must_use]
    pub fn phrase(self) -> &'static str {
        match self {
            Self::Loopback => "a loopback address (RFC 1122 §3.2.1.3, RFC 4291 §2.5.3)",
            Self::Private => "a private address (RFC 1918 §3)",
            Self::LinkLocal => "a link-local address (RFC 3927 §2.1, RFC 4291 §2.5.6)",
            Self::UniqueLocal => "a unique-local address (RFC 4193 §3)",
            Self::Unspecified => "the unspecified address (RFC 1122 §3.2.1.3, RFC 4291 §2.5.2)",
            Self::Multicast => "a multicast address (RFC 1112 §4, RFC 4291 §2.7)",
            Self::Shared => "a shared carrier address (RFC 6598 §7)",
            Self::Broadcast => "the broadcast address (RFC 919 §7)",
        }
    }
}

impl std::fmt::Display for RefusedFamily {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.phrase())
    }
}

/// Everything the target guard refuses, each naming what it refused and why.
///
/// Typed at the boundary that branches, exactly as `run_job::JobError` is:
/// the console's screens turn it into their own copy, and nothing downstream
/// matches on a message.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum TargetRefusal {
    /// The URL could not be read, so there was no host to check.
    #[error(
        "this instance could not read {url:?} as a URL ({reason}), so it opened no connection to it"
    )]
    Unreadable {
        /// The URL as the visitor typed it.
        url: String,
        /// The parser's own words.
        reason: String,
    },
    /// The URL carries no host at all (a `file:` or `data:` URL, say).
    #[error("{url:?} names no host, so this instance has nothing it could check before connecting")]
    NoHost {
        /// The URL as the visitor typed it.
        url: String,
    },
    /// The host does not resolve here, so no address could be checked.
    #[error(
        "{host:?} does not resolve from this instance ({reason}), so it opened no connection to it"
    )]
    Unresolvable {
        /// The host, as the URL carried it.
        host: String,
        /// The resolver's own words.
        reason: String,
    },
    /// The host resolves to an address only this instance can reach.
    #[error(
        "this public instance refuses {host:?}: it resolves to {address}, {family}, reachable only from inside the network this instance runs in. Name a target reachable from the public internet, or run the console yourself against a private one. A locally driven run cannot earn a console entry: for a deployment the internet cannot reach, the registry tiers are reproduced and self-reported."
    )]
    Refused {
        /// The host, as the URL carried it.
        host: String,
        /// The resolved address that was refused.
        address: IpAddr,
        /// Which family refused it.
        family: RefusedFamily,
    },
    /// The check itself could not run, so nothing was connected.
    #[error("the target check could not run ({0}); this instance opened no connection")]
    Unchecked(String),
}

/// The family that refuses this address, or `None` when nothing does.
///
/// Driven BY ADDRESS, never by the text of a URL: `10.0.0.1`,
/// `http://[::ffff:10.0.0.1]/` and a hostname resolving to either are the
/// same decision, and only an address comparison makes them so.
#[must_use]
pub fn refused_family(address: IpAddr) -> Option<RefusedFamily> {
    match address {
        IpAddr::V4(v4) => refused_v4(v4),
        IpAddr::V6(v6) => refused_v6(v6),
    }
}

/// The IPv4 half, over the standard library's own range predicates.
fn refused_v4(address: Ipv4Addr) -> Option<RefusedFamily> {
    if address.is_unspecified() {
        return Some(RefusedFamily::Unspecified);
    }
    if address.is_loopback() {
        return Some(RefusedFamily::Loopback);
    }
    if address.is_private() {
        return Some(RefusedFamily::Private);
    }
    if address.is_link_local() {
        return Some(RefusedFamily::LinkLocal);
    }
    if address.is_multicast() {
        return Some(RefusedFamily::Multicast);
    }
    if address.is_broadcast() {
        return Some(RefusedFamily::Broadcast);
    }
    // RFC 6598 §7 assigns 100.64.0.0/10 to carrier-grade NAT, so an address in
    // it is reachable from inside one carrier's network and from nowhere else
    // — the same property as the private ranges, under a different registry.
    let [first, second, _, _] = address.octets();
    if first == 100 && (64..128).contains(&second) {
        return Some(RefusedFamily::Shared);
    }
    None
}

/// The IPv6 half, including the IPv4-in-IPv6 forms.
///
/// The v6-specific families are decided FIRST, because `::1` and `::` both
/// carry an embedded IPv4 address (`0.0.0.1` and `0.0.0.0`) that the v4
/// predicates would read as something else entirely.
fn refused_v6(address: Ipv6Addr) -> Option<RefusedFamily> {
    if address.is_unspecified() {
        return Some(RefusedFamily::Unspecified);
    }
    if address.is_loopback() {
        return Some(RefusedFamily::Loopback);
    }
    if address.is_multicast() {
        return Some(RefusedFamily::Multicast);
    }
    let leading = *address.segments().first()?;
    if leading & 0xfe00 == 0xfc00 {
        return Some(RefusedFamily::UniqueLocal);
    }
    if leading & 0xffc0 == 0xfe80 {
        return Some(RefusedFamily::LinkLocal);
    }
    // The mapped (`::ffff:a.b.c.d`) and compatible (`::a.b.c.d`) forms carry
    // an IPv4 address, and the IPv4 rules are what decide it.
    address.to_ipv4().and_then(refused_v4)
}

/// Refuses a visitor-named base URL under the hosted posture.
///
/// The blocking half: name resolution is synchronous I/O, so a test drives
/// this one with no runtime at all and [`check`] is what a server function
/// awaits.
///
/// # Errors
/// [`TargetRefusal`], naming the family and the address it refused, or the
/// URL or resolution failure that left nothing checkable. Under
/// [`Posture::Local`] this never refuses and never resolves anything.
pub fn check_blocking(posture: Posture, base_url: &str) -> Result<(), TargetRefusal> {
    if !posture.guards_targets() {
        return Ok(());
    }
    let url = reqwest::Url::parse(base_url).map_err(|e| TargetRefusal::Unreadable {
        url: base_url.to_owned(),
        reason: e.to_string(),
    })?;
    let host = url.host_str().ok_or_else(|| TargetRefusal::NoHost {
        url: base_url.to_owned(),
    })?;
    check_host_blocking(host, url.port_or_known_default().unwrap_or(ASSUMED_PORT))
}

/// Refuses a host, resolving it first and checking every address it answers.
///
/// A literal address is checked as itself; anything else is resolved, and
/// EVERY answer has to pass — a name resolving to one public and one private
/// address is refused, because the connection could take either.
///
/// # Errors
/// [`TargetRefusal::Refused`] for a refused family, or
/// [`TargetRefusal::Unresolvable`] when the name answers with an error or
/// with nothing at all.
pub fn check_host_blocking(host: &str, port: u16) -> Result<(), TargetRefusal> {
    // `Url::host_str` serializes an IPv6 literal in brackets, which is URL
    // syntax rather than address syntax.
    let bare = host
        .strip_prefix('[')
        .and_then(|inner| inner.strip_suffix(']'))
        .unwrap_or(host);
    if let Ok(address) = bare.parse::<IpAddr>() {
        return decide(host, address);
    }
    // NOTE: RFC 1918 §3 defines the ranges, not the binding of a name to one
    // — a name that resolves differently at connect time defeats this, and
    // the spawned engine resolves the ixit's `base_url` again for itself.
    let resolved = (bare, port)
        .to_socket_addrs()
        .map_err(|e| TargetRefusal::Unresolvable {
            host: host.to_owned(),
            reason: e.to_string(),
        })?;
    let mut answered = false;
    for socket in resolved {
        answered = true;
        decide(host, socket.ip())?;
    }
    if answered {
        Ok(())
    } else {
        Err(TargetRefusal::Unresolvable {
            host: host.to_owned(),
            reason: String::from("the resolver answered with no address"),
        })
    }
}

/// One address's verdict, named by the host the visitor typed.
fn decide(host: &str, address: IpAddr) -> Result<(), TargetRefusal> {
    match refused_family(address) {
        Some(family) => Err(TargetRefusal::Refused {
            host: host.to_owned(),
            address,
            family,
        }),
        None => Ok(()),
    }
}

/// Refuses a visitor-named base URL under the hosted posture, off the
/// runtime.
///
/// Resolution is blocking I/O and a visitor picks the name, so it runs on the
/// blocking pool rather than on a runtime thread an unresponsive resolver
/// could hold.
///
/// # Errors
/// [`TargetRefusal`] as [`check_blocking`] gives it, plus
/// [`TargetRefusal::Unchecked`] when the blocking task itself could not run —
/// a check that did not happen refuses, never admits.
pub async fn check(posture: Posture, base_url: &str) -> Result<(), TargetRefusal> {
    if !posture.guards_targets() {
        return Ok(());
    }
    let owned = base_url.to_owned();
    match tokio::task::spawn_blocking(move || check_blocking(posture, &owned)).await {
        Ok(outcome) => outcome,
        Err(e) => Err(TargetRefusal::Unchecked(e.to_string())),
    }
}

#[cfg(test)]
mod tests {
    use super::{
        Posture, RefusedFamily, TargetRefusal, check_blocking, check_host_blocking, refused_family,
    };
    use std::net::IpAddr;

    /// A refused target has to tell the visitor what to do instead, and the
    /// thing they most need to know is the one they cannot infer: a run they
    /// drive themselves earns no console entry, so the routes are the other
    /// two tiers (#441).
    #[test]
    fn a_refusal_names_the_tiers_a_private_deployment_can_still_reach() {
        let refusal = TargetRefusal::Refused {
            host: "cdr.internal".to_owned(),
            address: "10.0.0.5".parse::<IpAddr>().expect("a literal address"),
            family: RefusedFamily::Private,
        };
        let shown = refusal.to_string();
        for expected in ["reproduced", "self-reported", "public internet"] {
            assert!(
                shown.contains(expected),
                "the refusal must name {expected:?}: {shown}"
            );
        }
    }

    /// One address per refused family, driven as an ADDRESS: a string-prefix
    /// test would pass on `10.0.0.1` and miss `::ffff:10.0.0.1`.
    #[test]
    fn every_refused_family_is_refused_by_address() {
        let cases: [(&str, RefusedFamily); 21] = [
            ("127.0.0.1", RefusedFamily::Loopback),
            ("127.255.255.254", RefusedFamily::Loopback),
            ("::1", RefusedFamily::Loopback),
            ("10.0.0.1", RefusedFamily::Private),
            ("172.16.0.1", RefusedFamily::Private),
            ("172.31.255.255", RefusedFamily::Private),
            ("192.168.1.1", RefusedFamily::Private),
            ("169.254.169.254", RefusedFamily::LinkLocal),
            ("fe80::1", RefusedFamily::LinkLocal),
            ("febf::1", RefusedFamily::LinkLocal),
            ("fc00::1", RefusedFamily::UniqueLocal),
            ("fd00::1", RefusedFamily::UniqueLocal),
            ("0.0.0.0", RefusedFamily::Unspecified),
            ("::", RefusedFamily::Unspecified),
            ("224.0.0.1", RefusedFamily::Multicast),
            ("ff02::1", RefusedFamily::Multicast),
            ("100.64.0.1", RefusedFamily::Shared),
            ("100.127.255.254", RefusedFamily::Shared),
            ("::ffff:100.64.0.1", RefusedFamily::Shared),
            ("255.255.255.255", RefusedFamily::Broadcast),
            ("::ffff:255.255.255.255", RefusedFamily::Broadcast),
        ];
        for (text, family) in cases {
            let address: IpAddr = text.parse().expect("a literal address");
            assert_eq!(refused_family(address), Some(family), "{text}");
        }
    }

    /// The IPv4-in-IPv6 forms carry an IPv4 address, and it is the IPv4 rules
    /// that decide them — the bypass a family list written only in v4 terms
    /// leaves open.
    #[test]
    fn the_ipv4_mapped_forms_are_refused_too() {
        let mapped: [(&str, RefusedFamily); 4] = [
            ("::ffff:127.0.0.1", RefusedFamily::Loopback),
            ("::ffff:10.0.0.1", RefusedFamily::Private),
            ("::ffff:169.254.169.254", RefusedFamily::LinkLocal),
            ("::127.0.0.1", RefusedFamily::Loopback),
        ];
        for (text, family) in mapped {
            let address: IpAddr = text.parse().expect("a literal address");
            assert_eq!(refused_family(address), Some(family), "{text}");
        }
    }

    /// A public address passes, so the guard refuses a family rather than
    /// refusing everything.
    #[test]
    fn a_public_address_passes() {
        // `100.63.x` and `100.128.x` sit either side of the shared block, so
        // the boundary is driven rather than assumed.
        for text in [
            "198.51.100.7",
            "203.0.113.9",
            "2001:db8::1",
            "8.8.8.8",
            "100.63.255.255",
            "100.128.0.0",
        ] {
            let address: IpAddr = text.parse().expect("a literal address");
            assert_eq!(refused_family(address), None, "{text}");
        }
    }

    /// The hosted posture refuses a literal private target through the whole
    /// URL path, and the refusal names the address and the family.
    #[test]
    fn the_hosted_posture_refuses_a_literal_target() {
        let refusal = check_blocking(
            Posture::Hosted,
            "http://10.0.0.7:8080/ehrbase/rest/openehr/v1",
        )
        .expect_err("a private literal must refuse");
        let TargetRefusal::Refused {
            address, family, ..
        } = &refusal
        else {
            panic!("expected a family refusal, got {refusal:?}");
        };
        assert_eq!(*family, RefusedFamily::Private);
        assert_eq!(address.to_string(), "10.0.0.7");
        let said = refusal.to_string();
        assert!(said.contains("RFC 1918"), "{said}");
        assert!(said.contains("10.0.0.7"), "{said}");
    }

    /// The bracketed IPv6 literal a URL carries is read as an address, not as
    /// a name to resolve.
    #[test]
    fn a_bracketed_ipv6_literal_is_refused() {
        let refusal =
            check_blocking(Posture::Hosted, "http://[::1]:8080/").expect_err("::1 must refuse");
        assert!(
            matches!(
                refusal,
                TargetRefusal::Refused {
                    family: RefusedFamily::Loopback,
                    ..
                }
            ),
            "{refusal:?}"
        );
    }

    /// Resolution happens FIRST: a NAME that resolves to a refused address is
    /// refused, which is the whole attack a literal-text check would miss.
    /// `localhost` resolves without leaving the machine.
    #[test]
    fn a_name_resolving_to_a_refused_address_is_refused() {
        let refusal =
            check_host_blocking("localhost", 8080).expect_err("localhost resolves to loopback");
        let TargetRefusal::Refused {
            host,
            address,
            family,
        } = refusal
        else {
            panic!("expected a family refusal");
        };
        assert_eq!(host, "localhost");
        assert_eq!(family, RefusedFamily::Loopback);
        assert!(address.is_loopback(), "{address}");
    }

    /// A URL with no host, and a URL that does not parse, are refused for
    /// what they are rather than silently admitted.
    #[test]
    fn an_uncheckable_url_is_refused_for_what_it_is() {
        assert!(matches!(
            check_blocking(Posture::Hosted, "not a url"),
            Err(TargetRefusal::Unreadable { .. })
        ));
        assert!(matches!(
            check_blocking(Posture::Hosted, "file:///etc/hosts"),
            Err(TargetRefusal::NoHost { .. })
        ));
    }

    /// The local posture refuses NOTHING, which is what keeps an operator
    /// driving their own `localhost` CDR and the browser journeys green.
    #[test]
    fn the_local_posture_refuses_nothing() {
        for url in [
            "http://127.0.0.1:8080/ehrbase/rest/openehr/v1",
            "http://localhost:8080/",
            "http://10.0.0.7/",
            "http://[fd00::1]/",
            "http://169.254.169.254/latest/meta-data/",
            "not a url",
            "file:///etc/hosts",
        ] {
            assert_eq!(check_blocking(Posture::Local, url), Ok(()), "{url}");
        }
    }
}
