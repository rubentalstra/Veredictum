// SPDX-FileCopyrightText: Veredictum contributors
// SPDX-License-Identifier: Apache-2.0

//! The served names of the compiled client bundle.
//!
//! cargo-leptos writes the content hash of every emitted `js`, `wasm` and
//! `css` file into one text file: the `hash-files` and `hash-file-name`
//! parameters of the manifest's `[package.metadata.leptos]`
//! (<https://github.com/leptos-rs/cargo-leptos>).
//! `leptos::prelude::HydrationScripts` reads the `js` and `wasm` lines of it
//! to name the bootstrap pair. The stylesheet has no such component, so this
//! module reads the `css` line out of THAT SAME file: served markup can then
//! only ever name a file the build actually emitted.

use std::path::PathBuf;

use leptos::prelude::LeptosOptions;

/// The hash file's line key for the emitted stylesheet.
const CSS_KEY: &str = "css";

/// The stylesheet the build emitted, as a document-root-relative URL.
///
/// With hashing off this is the plain `<site-pkg-dir>/<output-name>.css`.
/// With it on, the name carries the content hash read from the hash file, so
/// the URL changes whenever the bytes do and a cached copy of an older
/// stylesheet is unreachable rather than stale.
#[must_use]
pub fn stylesheet_href(options: &LeptosOptions) -> String {
    let mut href = options.site_pkg_dir_route_base();
    href.push_str(&options.output_name);
    if let Some(hash) = css_hash(options) {
        href.push('.');
        href.push_str(&hash);
    }
    href.push_str(".css");
    href
}

/// The hash file's path, resolved the way leptos resolves it: against the
/// directory of the running server binary, which an absolute `hash_file`
/// replaces outright
/// (<https://doc.rust-lang.org/std/path/struct.Path.html#method.join>).
fn hash_file_path(options: &LeptosOptions) -> Option<PathBuf> {
    let exe = std::env::current_exe().ok()?;
    Some(exe.parent()?.join(options.hash_file.as_ref()))
}

/// The content hash of the emitted stylesheet.
///
/// NOTE: `None` is legitimately absent, because an unhashed build emits no
/// hash file at all; a hashed build that cannot read one serves no page,
/// since `HydrationScripts` reads the same file and panics (leptos 0.8.20).
fn css_hash(options: &LeptosOptions) -> Option<String> {
    if !options.hash_files {
        return None;
    }
    let hashes = std::fs::read_to_string(hash_file_path(options)?).ok()?;
    hash_for(&hashes, CSS_KEY)
}

/// The hash a cargo-leptos hash file records under one key.
///
/// The format is one `<key>: <hash>` line per emitted file, which is what
/// leptos's own reader parses.
fn hash_for(hashes: &str, key: &str) -> Option<String> {
    hashes
        .lines()
        .filter_map(|line| line.split_once(':'))
        .find(|(name, _)| name.trim() == key)
        .map(|(_, hash)| hash.trim().to_owned())
        .filter(|hash| !hash.is_empty())
}

#[cfg(test)]
mod tests {
    use super::hash_for;

    /// A cargo-leptos hash file, verbatim from a `cargo leptos build` of this
    /// crate.
    const EMITTED: &str = "js: dCG_UiWiwilxr9Li_dygkg\n\
                           wasm: r1bjnaUfQ-l4jaFbq0S9MQ\n\
                           css: qCoxdkobUE8Pt_BhoK86rA\n";

    #[test]
    fn every_emitted_key_reads_back_its_own_hash() {
        assert_eq!(
            hash_for(EMITTED, "css").as_deref(),
            Some("qCoxdkobUE8Pt_BhoK86rA")
        );
        assert_eq!(
            hash_for(EMITTED, "js").as_deref(),
            Some("dCG_UiWiwilxr9Li_dygkg")
        );
        assert_eq!(
            hash_for(EMITTED, "wasm").as_deref(),
            Some("r1bjnaUfQ-l4jaFbq0S9MQ")
        );
    }

    #[test]
    fn a_key_the_file_does_not_carry_has_no_hash() {
        assert_eq!(hash_for(EMITTED, "map"), None);
        assert_eq!(hash_for("css:   \n", "css"), None);
        assert_eq!(hash_for("", "css"), None);
    }
}
