// SPDX-FileCopyrightText: Veredictum contributors
// SPDX-License-Identifier: Apache-2.0

//! The served markup names the bundle the build emitted (#450).
//!
//! Content-hashed output names remove the stale-pair failure by construction:
//! an old `veredictum-console.<hash>.js` beside a new `.wasm` is an
//! unreachable name rather than a `LinkError`. That only holds while the
//! markup and the emitted files agree, so what these read is the agreement —
//! the document head is rendered over a site tree carrying a hash file and
//! the hashed files it names, and every `/pkg/` reference in the markup must
//! resolve to a file that tree actually carries.
//!
//! The browser journeys cannot produce the failure this guards: each pass
//! starts a fresh browser against a freshly built bundle, so no previous
//! build's JavaScript exists anywhere to pair with a new wasm.

use std::path::Path;
use std::sync::Arc;

use assert_fs::TempDir;
use assert_fs::prelude::{FileWriteStr, PathChild};
use leptos::prelude::{LeptosOptions, Owner, RenderHtml, view};
use veredictum_console::app::DocumentHead;
use veredictum_console::site_bundle::stylesheet_href;

/// The output name the manifest's `[package.metadata.leptos]` sets, which is
/// the stem of every emitted file.
const OUTPUT_NAME: &str = "veredictum-console";

/// One emitted bundle: the hashed `js`, `wasm` and `css` files under the site
/// tree's pkg directory, plus the hash file naming their hashes.
///
/// The shape is a `cargo leptos build`'s own — one `<key>: <hash>` line per
/// emitted file, and `<output-name>.<hash>.<ext>` file names.
fn built_site(hash: &str) -> Result<TempDir, Box<dyn std::error::Error>> {
    let site = TempDir::new()?;
    for extension in ["js", "wasm", "css"] {
        site.child(format!("pkg/{OUTPUT_NAME}.{hash}.{extension}"))
            .write_str("the emitted bytes")?;
    }
    site.child("hash.txt")
        .write_str(&format!("js: {hash}\nwasm: {hash}\ncss: {hash}\n"))?;
    Ok(site)
}

/// The configuration a server started over that site tree runs with.
///
/// `hash_file` is ABSOLUTE, which is how the image points the server at the
/// hash file inside its site bundle: leptos resolves the name against the
/// server binary's own directory, and an absolute path replaces that base
/// (<https://doc.rust-lang.org/std/path/struct.Path.html#method.join>).
fn options_over(site: &TempDir) -> LeptosOptions {
    LeptosOptions::builder()
        .output_name(OUTPUT_NAME)
        .site_root(site.path().to_string_lossy().into_owned())
        .site_pkg_dir("pkg")
        .hash_files(true)
        .hash_file(Arc::<str>::from(
            site.child("hash.txt").path().to_string_lossy().into_owned(),
        ))
        .build()
}

/// The rendered document head, as served HTML.
fn head_html(options: LeptosOptions) -> String {
    let owner = Owner::new();
    owner.with(|| view! { <DocumentHead options /> }.to_html())
}

/// Every `/pkg/` URL the markup tells the browser to fetch.
///
/// Attribute values are quoted, so each one is its own token when the
/// document is split on the quote character.
fn pkg_references(html: &str) -> Vec<&str> {
    html.split('"')
        .filter(|token| token.starts_with("/pkg/"))
        .collect()
}

/// The head names the hashed files the build emitted, and every name it
/// carries resolves to a file in that build's site tree.
#[expect(
    clippy::panic_in_result_fn,
    reason = "the Book ch11 Result-returning test shape: assertions panic, plumbing propagates with ? (https://doc.rust-lang.org/book/ch11-01-writing-tests.html)"
)]
#[test]
fn the_served_head_names_only_files_the_build_emitted() -> Result<(), Box<dyn std::error::Error>> {
    let hash = "TESTHASHfor450AAAAAAAA";
    let site = built_site(hash)?;
    let html = head_html(options_over(&site));

    let referenced = pkg_references(&html);
    for extension in ["js", "wasm", "css"] {
        assert!(
            referenced
                .iter()
                .any(|url| url.ends_with(&format!(".{extension}"))),
            "the head references no .{extension} at all, so the browser is told nothing about it: {referenced:?}"
        );
    }

    for url in &referenced {
        let relative = url.trim_start_matches('/');
        assert!(
            url.contains(hash),
            "`{url}` carries no content hash, so a cached copy of an older build could still answer to that name"
        );
        assert!(
            Path::new(site.path()).join(relative).is_file(),
            "the head references `{url}`, which this build did not emit — the served markup and the hash file disagree"
        );
    }
    Ok(())
}

/// Hashing is a BUILD parameter the server also reads at RUN time, so the
/// image's environment must agree with the manifest: a bundle emitted under
/// hashed names and served by a process that does not know it names files
/// nothing carries, and the console 404s its own bootstrap.
#[expect(
    clippy::panic_in_result_fn,
    reason = "the Book ch11 Result-returning test shape: assertions panic, plumbing propagates with ? (https://doc.rust-lang.org/book/ch11-01-writing-tests.html)"
)]
#[test]
fn the_image_serves_with_the_hashing_the_manifest_builds_with()
-> Result<(), Box<dyn std::error::Error>> {
    let repo_root = Path::new(concat!(env!("CARGO_MANIFEST_DIR"), "/../.."));
    let manifest = std::fs::read_to_string(repo_root.join("app/veredictum-console/Cargo.toml"))?;
    let dockerfile = std::fs::read_to_string(repo_root.join("docker/Dockerfile"))?;

    let builds_hashed = manifest
        .lines()
        .any(|line| line.trim() == "hash-files = true");
    let serves_hashed = dockerfile
        .lines()
        .any(|line| line.trim().starts_with("LEPTOS_HASH_FILES=true"));
    assert_eq!(
        builds_hashed, serves_hashed,
        "the manifest builds hashed names ({builds_hashed}) and the image serves with hashing ({serves_hashed}); one without the other serves names nothing carries"
    );

    if serves_hashed {
        assert!(
            dockerfile
                .lines()
                .any(|line| line.trim().starts_with("LEPTOS_HASH_FILE_NAME=/app/site/")),
            "the image must read the hash file from inside the site bundle it names, so the two travel as one artifact"
        );
    }
    Ok(())
}

/// The names follow the hash file rather than a constant: a second build,
/// with different bytes and so a different hash, is named differently
/// everywhere the markup mentions it.
#[expect(
    clippy::panic_in_result_fn,
    reason = "the Book ch11 Result-returning test shape: assertions panic, plumbing propagates with ? (https://doc.rust-lang.org/book/ch11-01-writing-tests.html)"
)]
#[test]
fn a_second_build_is_named_by_its_own_hash() -> Result<(), Box<dyn std::error::Error>> {
    let first = built_site("firstBUILDhash00000000")?;
    let second = built_site("secondBUILDhash0000000")?;

    let first_html = head_html(options_over(&first));
    let second_options = options_over(&second);
    let second_stylesheet = stylesheet_href(&second_options);

    assert_eq!(
        second_stylesheet,
        format!("/pkg/{OUTPUT_NAME}.secondBUILDhash0000000.css"),
        "the stylesheet URL must be the emitted name, hash and all"
    );
    assert!(
        !first_html.contains(&second_stylesheet),
        "the first build's markup names the second build's stylesheet, so the names cannot be following the hash file"
    );
    Ok(())
}
