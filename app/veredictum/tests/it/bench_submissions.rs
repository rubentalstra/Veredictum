// SPDX-FileCopyrightText: Veredictum contributors
// SPDX-License-Identifier: Apache-2.0

//! The gate every committed benchmark submission passes before it reaches the
//! public board.
//!
//! A submission is a bench result somebody else measured on a machine nobody
//! here controls, so nothing about it is taken on trust. Each record is held
//! to the published bench-result schema, to the embedded pack it names and
//! that pack's fixture pins, to its own submittability arithmetic, and to the
//! naming convention that binds a file name to the environment fingerprint
//! inside it. A record under `examples/` demonstrates the pipe rather than
//! claiming a board place, so the submittability requirement is the one thing
//! it is exempt from; every other check reads it like any submission.

#![expect(
    clippy::expect_used,
    clippy::panic,
    reason = "the helpers below are not `#[test]` fns, so the clippy.toml in-tests scoping does not reach them; a submission this gate cannot even read must abort the gate loudly, Book ch11"
)]

use std::collections::BTreeMap;
use std::fmt::Write as _;
use std::path::{Path, PathBuf};

use sha2::{Digest as _, Sha256};
use veredictum::bench::pack::{self, EMBEDDED};
use veredictum::bench::result::BenchResult;

/// The committed submissions tree, relative to the repository root.
const SUBMISSIONS: &str = "benchmarks/submissions";

/// The sub-tree whose records demonstrate the pipe rather than claim a place
/// on the board.
const EXAMPLES: &str = "examples";

/// How many hexadecimal characters of the environment digest a file name
/// carries.
const HOST_PREFIX_LEN: usize = 8;

/// The repository root, from this crate's manifest directory.
fn repo_root() -> PathBuf {
    Path::new(concat!(env!("CARGO_MANIFEST_DIR"), "/../..")).to_path_buf()
}

/// Every committed submission, as the path relative to the submissions tree
/// paired with the file itself, in a fixed order.
fn submissions() -> Vec<(String, PathBuf)> {
    let root = repo_root().join(SUBMISSIONS);
    let mut found = Vec::new();
    if root.is_dir() {
        collect(&root, &root, &mut found);
    }
    found.sort_by(|a, b| a.0.cmp(&b.0));
    found
}

/// Walks one directory, appending every JSON file under it.
fn collect(root: &Path, dir: &Path, found: &mut Vec<(String, PathBuf)>) {
    for entry in std::fs::read_dir(dir).expect("submissions directory is readable") {
        let path = entry.expect("directory entry").path();
        if path.is_dir() {
            collect(root, &path, found);
        } else if path.extension().is_some_and(|ext| ext == "json") {
            let relative = path
                .strip_prefix(root)
                .expect("every found path is under the tree root")
                .to_string_lossy()
                .replace('\\', "/");
            found.push((relative, path));
        }
    }
}

/// Whether a record demonstrates the pipe rather than claiming a board place.
fn is_example(relative: &str) -> bool {
    relative
        .split('/')
        .next()
        .is_some_and(|first| first == EXAMPLES)
}

/// The digest a file name's host prefix is taken from: the LINE that
/// `jq -cS '.environment' <record>` prints, which is the record's own
/// environment block serialized compactly with its keys in sorted order,
/// followed by the newline that command emits. The trailing newline is part of
/// the digest because the submission guide hands a submitter a plain
/// `jq … | shasum` pipeline, and that pipeline hashes it.
fn host_prefix(document: &serde_json::Value) -> String {
    let environment = document
        .get("environment")
        .expect("the schema makes the environment block mandatory");
    let sorted: BTreeMap<String, serde_json::Value> =
        serde_json::from_value(environment.clone()).expect("the environment block is an object");
    let mut canonical = serde_json::to_string(&sorted).expect("a map of JSON values serializes");
    canonical.push('\n');
    let digest = Sha256::digest(canonical.as_bytes());
    let hex = digest.iter().fold(String::new(), |mut out, byte| {
        let _written = write!(out, "{byte:02x}");
        out
    });
    hex.chars().take(HOST_PREFIX_LEN).collect()
}

/// Reads one submission's bytes as JSON.
fn read_document(path: &Path) -> serde_json::Value {
    let text = std::fs::read_to_string(path)
        .unwrap_or_else(|e| panic!("{} is unreadable: {e}", path.display()));
    serde_json::from_str(&text)
        .unwrap_or_else(|e| panic!("{} is not valid JSON: {e}", path.display()))
}

#[test]
fn every_submission_validates_against_the_published_schema() {
    let schema_text = std::fs::read_to_string(repo_root().join("schemas/bench-result.schema.json"))
        .expect("the published bench-result schema is committed");
    let schema: serde_json::Value =
        serde_json::from_str(&schema_text).expect("the published schema is valid JSON");
    let validator = jsonschema::validator_for(&schema).expect("the published schema compiles");

    for (relative, path) in submissions() {
        let document = read_document(&path);
        let violations: Vec<String> = validator
            .iter_errors(&document)
            .map(|error| format!("{}: {error}", error.instance_path()))
            .collect();
        assert!(
            violations.is_empty(),
            "{relative} violates the published bench-result schema: {}",
            violations.join("; ")
        );
        let parsed: Result<BenchResult, _> = serde_json::from_value(document);
        assert!(
            parsed.is_ok(),
            "{relative} does not parse as a bench result: {:?}",
            parsed.err()
        );
    }
}

#[test]
fn every_submission_names_an_embedded_pack_at_its_pinned_fixtures() {
    let embedded: Vec<&str> = EMBEDDED.iter().map(|id| id.as_str()).collect();
    for (relative, path) in submissions() {
        let record: BenchResult = serde_json::from_value(read_document(&path))
            .unwrap_or_else(|e| panic!("{relative} does not parse as a bench result: {e}"));
        assert!(
            embedded.contains(&record.pack.id.as_str()),
            "{relative} names pack {:?}, which this release does not embed (embedded: {})",
            record.pack.id,
            embedded.join(", ")
        );
        let declared = pack::load(&record.pack.id)
            .unwrap_or_else(|e| panic!("{relative}: the named pack does not load: {e}"));
        assert_eq!(
            record.pack.version, declared.version,
            "{relative} claims pack {} at version {}, which this release does not publish",
            record.pack.id, record.pack.version
        );
        assert_eq!(
            record.pack.fixtures,
            declared.fixture_pins(),
            "{relative} carries fixture pins the released pack does not declare"
        );
        assert_eq!(
            record.pack.seed, declared.seed,
            "{relative} was driven at a seed the released pack does not declare"
        );
    }
}

#[test]
fn every_submission_is_submittable_by_its_own_numbers() {
    for (relative, path) in submissions() {
        let record: BenchResult = serde_json::from_value(read_document(&path))
            .unwrap_or_else(|e| panic!("{relative} does not parse as a bench result: {e}"));
        let unmet = record.unmet_requirements();
        assert_eq!(
            record.submittable,
            unmet.is_empty(),
            "{relative} claims submittable={} while its own numbers say otherwise ({:?})",
            record.submittable,
            unmet
        );
        assert_eq!(
            record.submittable_unmet, unmet,
            "{relative} lists unmet requirements its own numbers do not produce"
        );
        if !is_example(&relative) {
            assert!(
                record.submittable,
                "{relative} is not submittable ({:?}); a record that misses a requirement \
                 belongs under {EXAMPLES}/, which the board does not rank",
                unmet
                    .iter()
                    .map(|requirement| requirement.statement())
                    .collect::<Vec<_>>()
            );
        }
    }
}

#[test]
fn every_submission_carries_the_relative_index_its_baselines_derive() {
    for (relative, path) in submissions() {
        let record: BenchResult = serde_json::from_value(read_document(&path))
            .unwrap_or_else(|e| panic!("{relative} does not parse as a bench result: {e}"));
        if is_example(&relative) && record.baselines.is_empty() {
            continue;
        }
        assert_eq!(
            record.baselines.len(),
            record.relative.len(),
            "{relative} carries {} baselines but {} relative-index blocks; the board reads the \
             ratio, so a baseline without one is unrankable",
            record.baselines.len(),
            record.relative.len()
        );
        for baseline in &record.baselines {
            assert!(
                record
                    .relative
                    .iter()
                    .any(|index| index.baseline.as_str() == baseline.cdr.as_str()),
                "{relative} measured the {} baseline without deriving its relative index",
                baseline.cdr
            );
        }
    }
}

#[test]
fn every_submission_file_name_states_its_date_and_host() {
    for (relative, path) in submissions() {
        let document = read_document(&path);
        let mut segments: Vec<&str> = relative.split('/').collect();
        let file = segments.pop().expect("a relative path ends in a file name");
        assert!(
            !segments.is_empty(),
            "{relative} sits at the tree root; every submission lives under a directory naming \
             the system it measured"
        );
        let stem = file
            .strip_suffix(".json")
            .expect("the walk only collects .json files");
        let (date, host) = stem
            .rsplit_once('-')
            .unwrap_or_else(|| panic!("{relative} is not named <YYYY-MM-DD>-<host prefix>.json"));
        assert_eq!(
            date.len(),
            "YYYY-MM-DD".len(),
            "{relative} does not open with an ISO 8601 calendar date"
        );
        assert!(
            date.chars()
                .enumerate()
                .all(|(position, c)| if position == 4 || position == 7 {
                    c == '-'
                } else {
                    c.is_ascii_digit()
                }),
            "{relative} does not open with an ISO 8601 calendar date"
        );
        assert_eq!(
            host,
            host_prefix(&document),
            "{relative} names a host prefix its own environment fingerprint does not digest to"
        );
    }
}

// NOTE: the engine reads no host beyond `std` and `/proc` and never spawns a
// process to learn one, so `cpu_model` and `total_memory_bytes` are absent on
// a platform that discloses neither and the board prints that absence.
#[test]
fn every_submission_carries_a_readable_environment_fingerprint() {
    for (relative, path) in submissions() {
        let record: BenchResult = serde_json::from_value(read_document(&path))
            .unwrap_or_else(|e| panic!("{relative} does not parse as a bench result: {e}"));
        assert!(
            !record.environment.arch.is_empty() && !record.environment.os.is_empty(),
            "{relative} carries no machine, so its absolute numbers describe nothing"
        );
        assert!(
            record.environment.available_parallelism.is_some(),
            "{relative} discloses no core count, which `std::thread::available_parallelism` \
             establishes on every supported platform, so the record was not written by this \
             engine"
        );
    }
}

/// A percentile computed over a run in which nothing succeeded describes the
/// failures. A record that measured an operation without ever getting an
/// answer out of it is not a speed measurement, and the board would rank it
/// anyway, so the gate refuses it here.
///
/// The baselines are held to the same rule, because every index on the board
/// divides by one of their medians: a denominator taken from an operation that
/// never answered is worse than a missing row, which the record would at least
/// have recorded as a typed gap.
// TODO(#197): submittability counts repetitions and baselines and never reads
// an error count, so the engine stamps `submittable: true` on a run whose every
// arrival failed. Until it does, this is the only thing standing between such a
// record and the public board.
#[test]
fn no_submission_ranks_an_operation_that_never_answered() {
    for (relative, path) in submissions() {
        let record: BenchResult = serde_json::from_value(read_document(&path))
            .unwrap_or_else(|e| panic!("{relative} does not parse as a bench result: {e}"));
        assert_every_arrival_had_a_chance(&relative, "the target", &record.repetitions);
        for baseline in &record.baselines {
            assert_every_arrival_had_a_chance(
                &relative,
                &format!("the {} baseline", baseline.cdr),
                &baseline.repetitions,
            );
        }
    }
}

/// Asserts that no operation of one measured side recorded arrivals without a
/// single success.
fn assert_every_arrival_had_a_chance(
    relative: &str,
    side: &str,
    repetitions: &[veredictum::bench::result::RepetitionRecord],
) {
    for repetition in repetitions {
        for (phase, measured) in &repetition.phases {
            for (operation, stats) in &measured.operations {
                assert!(
                    stats.errors < stats.count,
                    "{relative}: on {side}, repetition {} of phase {phase} recorded {} arrivals \
                     for {operation} and every one of them failed ({:?}), so its percentiles \
                     describe the failures",
                    repetition.repetition,
                    stats.count,
                    stats.errors_by_class
                );
            }
        }
        for (phase, sweep) in &repetition.sweeps {
            for (operation, stats) in &sweep.operations {
                assert!(
                    stats.errors < stats.count,
                    "{relative}: on {side}, repetition {} of sweep {phase} recorded {} requests \
                     for {operation} and every one of them failed ({:?})",
                    repetition.repetition,
                    stats.count,
                    stats.errors_by_class
                );
            }
        }
    }
}
