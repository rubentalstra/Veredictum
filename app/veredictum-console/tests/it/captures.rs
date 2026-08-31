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
    // The bare path and the run's own address render the same screen (#386),
    // so one capture photographs both.
    ("run/live/:run_id", "live"),
    ("run/results", "results"),
    ("run/verdicts", "verdicts"),
    ("run/submit", "submit"),
    ("verify", "verify"),
    ("benchmarks", "benchmarks"),
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

/// The tags that open a route declaration.
///
/// `<Routes …>` is deliberately absent: it is the fallback container, and a
/// prefix split on `<Route` reads it as a declaration, then parses whatever
/// route follows it a second time.
const ROUTE_TAGS: [&str; 2] = ["<Route", "<ParentRoute"];

/// Whether `tag` at byte offset `at` opens a route declaration.
///
/// The tag name must END at that offset, so `<Routes` never reads as
/// `<Route`: the character after the tag is the attribute separator, never
/// another name character.
fn opens_route(app_rs: &str, at: usize, tag: &str) -> bool {
    app_rs
        .get(at + tag.len()..)
        .and_then(|rest| rest.chars().next())
        .is_some_and(|next| !next.is_ascii_alphanumeric() && next != '_')
}

/// The source following each route declaration's tag, in source order.
///
/// One entry per `<Route>` and `<ParentRoute>`, so a nested parent is read
/// as its own route rather than swallowed by the sibling before it.
fn route_declarations(app_rs: &str) -> Vec<&str> {
    let mut starts = Vec::new();
    for tag in ROUTE_TAGS {
        let mut from = 0;
        while let Some(offset) = app_rs.get(from..).and_then(|rest| rest.find(tag)) {
            let at = from + offset;
            if opens_route(app_rs, at, tag) {
                starts.push(at + tag.len());
            }
            from = at + tag.len();
        }
    }
    starts.sort_unstable();
    starts
        .into_iter()
        .filter_map(|start| app_rs.get(start..))
        .collect()
}

/// Extracts the routed paths from `app.rs`'s segment literals.
///
/// The parse is deliberately narrow: it reads the `path=` attributes'
/// `StaticSegment("…")` and `ParamSegment("…")` literals in source order and
/// rebuilds each route's path. A router shape it cannot read fails the test,
/// which is the point — the manifest must move with the router.
fn routed_paths(app_rs: &str) -> BTreeSet<String> {
    let mut routes = BTreeSet::new();
    for route in route_declarations(app_rs) {
        let Some(path_attr) = route.split("path=").nth(1) else {
            continue;
        };
        let Some(view_end) = path_attr.find("view=") else {
            continue;
        };
        let Some(decl) = path_attr.get(..view_end) else {
            continue;
        };
        routes.insert(path_segments(decl).join("/"));
    }
    routes
}

/// The segment literals of one `path=` declaration, in declaration order.
///
/// One left-to-right scan reads the `StaticSegment("…")` and
/// `ParamSegment("…")` literals; a parameter segment is rendered with the
/// router's own `:name` spelling, and an empty literal contributes nothing.
fn path_segments(decl: &str) -> Vec<String> {
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
    segments
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
        routes.len() >= SLUG_OF.len(),
        "the router parse found only {} routes for {} manifest entries — the narrow parser no longer reads app.rs; fix the parser, never the manifest",
        routes.len(),
        SLUG_OF.len()
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

/// An `app.rs`-shaped router carrying the two shapes the earlier parser read
/// wrong: the `<Routes …>` container, and a second `<ParentRoute>` sibling.
const ROUTER_SHAPE: &str = r#"
    <Router>
        <Routes fallback=|| view! { <NotFound /> }>
            <ParentRoute path=StaticSegment("app") view=Shell>
                <Route path=(StaticSegment("app"), StaticSegment("cases")) view=Cases />
                <Route
                    path=(
                        StaticSegment("app"),
                        StaticSegment("cases"),
                        ParamSegment("case"),
                    )
                    view=Case
                />
            </ParentRoute>
            <ParentRoute path=StaticSegment("admin") view=Admin>
                <Route path=(StaticSegment("admin"), StaticSegment("keys")) view=Keys />
            </ParentRoute>
        </Routes>
    </Router>
"#;

/// The container is not a declaration: five route tags, five declarations.
///
/// A `<Routes …>` sibling counted as a sixth is the phantom route this pins
/// out, and it hides in the parsed paths because it re-reads the declaration
/// that follows it.
#[test]
fn a_routes_container_opens_no_route_declaration() {
    assert_eq!(route_declarations(ROUTER_SHAPE).len(), 5);
}

/// Every route tag is read, the second parent included.
#[test]
fn every_route_tag_is_read_including_a_second_parent() {
    let expected: BTreeSet<String> = ["admin", "admin/keys", "app", "app/cases", "app/cases/:case"]
        .into_iter()
        .map(str::to_owned)
        .collect();
    assert_eq!(routed_paths(ROUTER_SHAPE), expected);
}
