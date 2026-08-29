// SPDX-FileCopyrightText: Veredictum contributors
// SPDX-License-Identifier: Apache-2.0

//! The committed pack manifest (`website/landing/bench-packs.json`) is the
//! published description of what a benchmark run executes, and the public
//! legend page is generated from it.
//!
//! A pack is versioned data in the binary, so nothing outside the binary may
//! be the source of that description. These tests close both directions: the
//! committed document is byte-identical to what `bench-packs` emits, and it
//! validates against the published schema. A pack edit that leaves the file
//! behind fails here, before the legend can be served stale.

use std::path::PathBuf;

use veredictum::bench::manifest::MANIFEST_FILE;
use veredictum::pipeline::bench::describe_packs;

/// The repository root, from this crate's manifest directory.
fn repo_root() -> PathBuf {
    PathBuf::from(concat!(env!("CARGO_MANIFEST_DIR"), "/../.."))
}

/// Where the published copy lives. It sits under the landing tree because the
/// deployed site serves it beside the page generated from it, so a reader of
/// the page can fetch the document it was rendered from.
fn committed_manifest() -> PathBuf {
    repo_root().join("website/landing").join(MANIFEST_FILE)
}

#[test]
fn the_committed_manifest_is_byte_identical_to_emission() {
    let outcome = describe_packs().expect("the embedded packs describe themselves");
    let committed = std::fs::read_to_string(committed_manifest())
        .expect("website/landing/bench-packs.json is committed");
    assert_eq!(
        committed, outcome.document.body,
        "the committed pack manifest drifted — regenerate it with \
         `cargo run -- bench-packs --out website/landing`, then regenerate the \
         legend with `scripts/render/bench-legend.sh`"
    );
}

#[test]
fn the_committed_manifest_validates_against_the_published_schema() {
    let schema_text = std::fs::read_to_string(repo_root().join("schemas/bench-packs.schema.json"))
        .expect("the published bench-packs schema is committed");
    let schema: serde_json::Value =
        serde_json::from_str(&schema_text).expect("the published schema is valid JSON");
    let validator = jsonschema::validator_for(&schema).expect("the published schema compiles");
    let text = std::fs::read_to_string(committed_manifest()).expect("the manifest is committed");
    let document: serde_json::Value =
        serde_json::from_str(&text).expect("the manifest is valid JSON");
    let violations: Vec<String> = validator
        .iter_errors(&document)
        .map(|error| format!("{}: {error}", error.instance_path()))
        .collect();
    assert!(
        violations.is_empty(),
        "the committed pack manifest violates its published schema: {}",
        violations.join("; ")
    );
}
