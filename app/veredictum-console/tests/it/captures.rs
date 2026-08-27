// SPDX-FileCopyrightText: Veredictum contributors
// SPDX-License-Identifier: Apache-2.0

//! The route↔capture manifest (#98): every routed surface has its light and
//! dark capture in the book's image tree, so a new screen cannot ship
//! unphotographed. `ui-screenshot-guard` catches captures that should have
//! CHANGED; this gate catches captures that do not EXIST.

use std::collections::BTreeSet;
use std::path::PathBuf;

/// Maps the router's leaf routes to the capture slugs the book carries.
///
/// The keys are derived from `app.rs`'s own segments, so a new `<Route>`
/// fails the derivation below until it is mapped here — mapping it is the
/// moment to add its journey capture, not to invent an exception.
const SLUG_OF: &[(&str, &str)] = &[
    ("", "landing"),
    ("catalogue", "catalogue"),
    ("catalogue/:chapter", "chapter"),
    ("catalogue/:chapter/:case", "case"),
    ("run/connect", "connect"),
    ("run/scope", "scope"),
    ("run/live", "live"),
    ("run/results", "results"),
    ("run/verdicts", "verdicts"),
    ("verify", "verify"),
];

/// Routes whose captures are honestly pending, each naming the issue that
/// delivers them. An entry here is a debt with an owner, never a waiver.
const PENDING: &[(&str, &str)] = &[];

/// The repository root, from this crate's manifest directory.
///
/// # Errors
/// The canonicalize failure when the crate does not live two levels under
/// the repository root.
fn repo_root() -> Result<PathBuf, std::io::Error> {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
}

/// Extracts the routed paths from `app.rs`'s segment literals.
///
/// The parse is deliberately narrow: it reads the `path=` attributes'
/// `StaticSegment("…")` and `ParamSegment("…")` literals in source order and
/// rebuilds each route's path. A router shape it cannot read fails the test,
/// which is the point — the manifest must move with the router.
fn routed_paths(app_rs: &str) -> BTreeSet<String> {
    let mut routes = BTreeSet::new();
    for route in app_rs.split("<Route").skip(1) {
        let Some(path_attr) = route.split("path=").nth(1) else {
            continue;
        };
        let Some(view_end) = path_attr.find("view=") else {
            continue;
        };
        let Some(decl) = path_attr.get(..view_end) else {
            continue;
        };
        // One left-to-right scan keeps the segments in declaration order.
        let mut segments = Vec::new();
        let mut rest = decl;
        loop {
            let next_static = rest.find("StaticSegment(\"");
            let next_param = rest.find("ParamSegment(\"");
            let (at, is_param) = match (next_static, next_param) {
                (Some(s), Some(q)) if q < s => (q, true),
                (Some(s), _) => (s, false),
                (None, Some(q)) => (q, true),
                (None, None) => break,
            };
            let open = at
                + if is_param {
                    "ParamSegment(\"".len()
                } else {
                    "StaticSegment(\"".len()
                };
            let Some(tail) = rest.get(open..) else { break };
            let Some(end) = tail.find('"') else { break };
            let Some(literal) = tail.get(..end) else {
                break;
            };
            if !literal.is_empty() {
                if is_param {
                    segments.push(format!(":{literal}"));
                } else {
                    segments.push(literal.to_owned());
                }
            }
            let Some(next) = rest.get(open + end + 1..) else {
                break;
            };
            rest = next;
        }
        routes.insert(segments.join("/"));
    }
    routes
}

#[expect(
    clippy::panic_in_result_fn,
    reason = "the Book ch11 Result-returning test shape: assertions panic, plumbing propagates with ?"
)]
#[test]
fn every_routed_surface_has_its_captures() -> Result<(), std::io::Error> {
    let root = repo_root()?;
    let app_rs = std::fs::read_to_string(root.join("app/veredictum-console/src/app.rs"))?;
    let img = root.join("website/book/src/console/img");

    let routes = routed_paths(&app_rs);
    assert!(
        routes.len() >= 9,
        "the router parse found only {} routes — the narrow parser no longer reads app.rs; fix the parser, never the manifest",
        routes.len()
    );

    let mut missing = Vec::new();
    for route in &routes {
        // The bare /run entry is a redirect, not a surface.
        if route == "run" {
            continue;
        }
        let Some((_, slug)) = SLUG_OF.iter().find(|(path, _)| path == route) else {
            missing.push(format!(
                "route /{route} has no manifest entry — map it in SLUG_OF and give its journey a capture"
            ));
            continue;
        };
        if let Some((_, why)) = PENDING.iter().find(|(pending, _)| pending == route) {
            println!("pending: /{route} ({why})");
            continue;
        }
        for theme in ["light", "dark"] {
            let file = img.join(format!("{slug}-{theme}.png"));
            if !file.is_file() {
                missing.push(format!(
                    "route /{route}: {slug}-{theme}.png is absent from website/book/src/console/img"
                ));
            }
        }
    }
    assert!(
        missing.is_empty(),
        "unphotographed surfaces (#98):\n  {}",
        missing.join("\n  ")
    );
    Ok(())
}
