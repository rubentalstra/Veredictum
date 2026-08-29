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
use std::process::Command;

use serde_json::json;

use sha2::{Digest as _, Sha256};
use veredictum::bench::pack::{self, EMBEDDED};
use veredictum::bench::posture::{
    Assurance, Bracket, CanaryOutcome, CanaryReading, PostureDefectKind, PostureDisclosure,
    PostureItem, PostureRecord, submission_defects,
};
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
///
/// The engine's own error-share requirement is the standing rule and refuses a
/// far smaller contamination than this floor does; the two are asserted
/// together here, so a record claiming `submittable` while the engine's
/// arithmetic disagrees fails the gate as loudly as one this floor catches.
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
        if record.submittable {
            assert!(
                record.failed_share_breaches().is_empty(),
                "{relative} claims submittable while the engine's failed-arrival ceiling of \
                 {} is crossed: {}",
                record.pack.max_failed_share,
                record
                    .failed_share_breaches()
                    .iter()
                    .map(|breach| breach.sentence(record.pack.max_failed_share))
                    .collect::<Vec<_>>()
                    .join("; ")
            );
        }
    }
}

/// A posture block a canary could not check is a claim, and the board ranks
/// numbers rather than claims. Every item the machinery observes — the signing
/// scheme read off versions the run itself committed, the fate of the pack's
/// invalid twin, the uncredentialed read, the base URL scheme, the encoded
/// response — is verified on both brackets or the submission is refused with
/// the item named. Audit and tenancy stay declared-only, because released
/// ITS-REST defines no read operation for either.
#[test]
fn every_submission_carries_the_verification_its_canaries_can_give() {
    for (relative, path) in submissions() {
        let record: BenchResult = serde_json::from_value(read_document(&path))
            .unwrap_or_else(|e| panic!("{relative} does not parse as a bench result: {e}"));
        assert_posture_is_publishable(&relative, "the target", &record.posture);
        for baseline in &record.baselines {
            assert_posture_is_publishable(
                &relative,
                &format!("the {} baseline", baseline.cdr),
                &baseline.posture,
            );
        }
    }
}

/// Asserts that one posture block carries every verification the canaries can
/// give, naming the item and the reason when it does not.
fn assert_posture_is_publishable(relative: &str, side: &str, posture: &PostureRecord) {
    let defects = submission_defects(posture);
    assert!(
        defects.is_empty(),
        "{relative}: on {side}, the posture block declared `{}` falls short of the \
         verification the canaries can give: {}",
        posture.profile,
        defects
            .iter()
            .map(ToString::to_string)
            .collect::<Vec<_>>()
            .join("; ")
    );
}

/// One canary reading, for the fixtures below.
fn reading(bracket: Bracket, outcome: CanaryOutcome, observed: &str) -> CanaryReading {
    CanaryReading {
        bracket,
        outcome,
        observed: observed.to_owned(),
        evidence: "a reading this fixture states rather than takes".to_owned(),
    }
}

/// A posture block shaped exactly as a passing submission carries one: every
/// observable item verified on both brackets, the two nothing discloses
/// carried as claims.
fn publishable_posture() -> PostureRecord {
    PostureRecord {
        profile: "minimal".to_owned(),
        summary: "the fixture profile".to_owned(),
        items: PostureItem::ALL
            .iter()
            .copied()
            .map(|item| {
                let (outcome, assurance) = if item.is_observable() {
                    (CanaryOutcome::Confirmed, Assurance::Verified)
                } else {
                    (CanaryOutcome::NotObservable, Assurance::DeclaredOnly)
                };
                PostureDisclosure {
                    item,
                    declared: "off".to_owned(),
                    assurance,
                    readings: vec![
                        reading(Bracket::Before, outcome, "off"),
                        reading(Bracket::After, outcome, "off"),
                    ],
                }
            })
            .collect(),
        comparability: Vec::new(),
    }
}

/// The same block with one item rewritten, which is how each refusal fixture
/// below states the single thing it changed.
fn with_item(
    posture: &PostureRecord,
    item: PostureItem,
    outcome: CanaryOutcome,
    assurance: Assurance,
) -> PostureRecord {
    let mut rewritten = posture.clone();
    for line in &mut rewritten.items {
        if line.item == item {
            line.assurance = assurance;
            line.readings = vec![
                reading(Bracket::Before, outcome, "(not observable)"),
                reading(Bracket::After, outcome, "(not observable)"),
            ];
        }
    }
    rewritten
}

/// The shape a passing submission carries produces no finding at all, so the
/// fixtures below fail for the reason each one states and not for a shape the
/// gate refuses anyway.
#[test]
fn the_publishable_fixture_carries_no_finding() {
    assert_eq!(submission_defects(&publishable_posture()), Vec::new());
}

/// Signing has an observable that always exists — the run commits versions of
/// its own and reads them back — so a bracket that saw nothing is a canary
/// that did not run, and the record is refused rather than published with the
/// declaration standing unchecked.
#[test]
fn a_signing_canary_that_observed_nothing_is_refused() {
    let posture = with_item(
        &publishable_posture(),
        PostureItem::VersionSigning,
        CanaryOutcome::NotObservable,
        Assurance::DeclaredOnly,
    );
    let defects = submission_defects(&posture);
    assert_eq!(defects.len(), 1, "{defects:?}");
    let defect = defects.first().expect("one defect was just asserted");
    assert_eq!(defect.item, PostureItem::VersionSigning);
    assert_eq!(defect.kind, PostureDefectKind::Unverified);
    assert!(defect.to_string().contains("version_signing"), "{defect}");
}

/// Commit validation has an observable that always exists too: every pack
/// embeds the invalid twin the canary offers, so a declared-only validation
/// item is a check that was skipped.
#[test]
fn a_declared_only_validation_item_is_refused() {
    let posture = with_item(
        &publishable_posture(),
        PostureItem::CommitValidation,
        CanaryOutcome::NotObservable,
        Assurance::DeclaredOnly,
    );
    let defects = submission_defects(&posture);
    assert_eq!(defects.len(), 1, "{defects:?}");
    let defect = defects.first().expect("one defect was just asserted");
    assert_eq!(defect.item, PostureItem::CommitValidation);
    assert_eq!(defect.kind, PostureDefectKind::Unverified);
}

/// The same rule holds for the three items the invocation settles: an
/// uncredentialed read, the base URL scheme and an encoded response are all
/// first-hand observations, so none of them may reach the board unchecked.
#[test]
fn an_unchecked_invocation_item_is_refused() {
    for item in [
        PostureItem::Authn,
        PostureItem::Tls,
        PostureItem::Compression,
    ] {
        let posture = with_item(
            &publishable_posture(),
            item,
            CanaryOutcome::NotObservable,
            Assurance::DeclaredOnly,
        );
        let defects = submission_defects(&posture);
        assert_eq!(defects.len(), 1, "{item}: {defects:?}");
        let defect = defects.first().expect("one defect was just asserted");
        assert_eq!(defect.item, item);
        assert_eq!(defect.kind, PostureDefectKind::Unverified);
    }
}

/// Nothing on the wire discloses an audit trail, so a block claiming one was
/// verified claims an observation the machinery cannot make, and that is
/// refused as loudly as a missing check.
#[test]
fn a_verified_claim_on_an_unobservable_item_is_refused() {
    let posture = with_item(
        &publishable_posture(),
        PostureItem::Audit,
        CanaryOutcome::Confirmed,
        Assurance::Verified,
    );
    let defects = submission_defects(&posture);
    assert_eq!(defects.len(), 1, "{defects:?}");
    let defect = defects.first().expect("one defect was just asserted");
    assert_eq!(defect.item, PostureItem::Audit);
    assert_eq!(defect.kind, PostureDefectKind::Unverifiable);
}

/// A block that simply omits an item is refused by name, so a shorter block
/// can never read as a passing one.
#[test]
fn a_posture_block_missing_an_item_is_refused() {
    let mut posture = publishable_posture();
    posture.items.retain(|line| line.item != PostureItem::Tls);
    let defects = submission_defects(&posture);
    assert_eq!(defects.len(), 1, "{defects:?}");
    let defect = defects.first().expect("one defect was just asserted");
    assert_eq!(defect.item, PostureItem::Tls);
    assert_eq!(defect.kind, PostureDefectKind::Missing);
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

/// The renderer under test, and the tree it reads.
///
/// The script derives its own root from its path and reads
/// `benchmarks/submissions` under it, so a copy in a temporary tree renders a
/// temporary board: the committed page and the committed records are never
/// touched.
fn board_workspace(
    records: &[(&str, serde_json::Value)],
) -> Result<assert_fs::TempDir, Box<dyn std::error::Error>> {
    let root = assert_fs::TempDir::new()?;
    let script_dir = root.path().join("scripts").join("render");
    std::fs::create_dir_all(&script_dir)?;
    std::fs::create_dir_all(root.path().join("website").join("landing"))?;
    let _copied = std::fs::copy(
        repo_root().join("scripts/render/bench-board.sh"),
        script_dir.join("bench-board.sh"),
    )?;
    for (name, document) in records {
        let path = root.path().join(SUBMISSIONS).join(name);
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(&path, serde_json::to_string_pretty(document)?)?;
    }
    Ok(root)
}

/// One posture block as a record carries it: every observable item verified,
/// audit and tenancy carried as claims.
fn board_posture(profile: &str, audit: &str) -> serde_json::Value {
    let items: Vec<serde_json::Value> = PostureItem::ALL
        .iter()
        .map(|item| {
            let (outcome, assurance) = if item.is_observable() {
                ("confirmed", "verified")
            } else {
                ("not-observable", "declared-only")
            };
            let declared = if *item == PostureItem::Audit {
                audit
            } else {
                "off"
            };
            json!({
                "item": item.as_str(),
                "declared": declared,
                "assurance": assurance,
                "readings": [
                    {"bracket": "before", "outcome": outcome, "observed": declared,
                     "evidence": "a reading this fixture states rather than takes"},
                    {"bracket": "after", "outcome": outcome, "observed": declared,
                     "evidence": "a reading this fixture states rather than takes"}
                ]
            })
        })
        .collect();
    json!({
        "profile": profile,
        "summary": format!("the {profile} fixture profile"),
        "items": items
    })
}

/// One committed submission as the RENDERER reads it.
///
/// Hand-written JSON, because these bytes are the renderer's input rather than
/// a value the engine produced here; the records the engine actually writes are
/// held to the published schema by the tests above.
fn board_record(label: &str, profile: &str, audit: &str, index: f64) -> serde_json::Value {
    let stats = json!({
        "p50_us": {"median": 1200.0},
        "p90_us": {"median": 2400.0},
        "p99_us": {"median": 4800.0},
        "throughput_ops_s": {"median": 42.0}
    });
    let repetition = json!({
        "repetition": 1,
        "phases": {"steady": {"operations": {"composition_create": {"count": 100, "errors": 1}}}}
    });
    let relative = |baseline: &str, name: &str| {
        json!({
            "baseline": baseline,
            "display_name": name,
            "phases": {"steady": {"operations": {"composition_create":
                {"metrics": {"p50_us": {"index": index}}}}}},
            "gaps": []
        })
    };
    let baseline = |cdr: &str, name: &str, comparability: serde_json::Value| {
        let mut posture = board_posture(profile, audit);
        if let Some(block) = posture.as_object_mut() {
            let _replaced = block.insert("comparability".to_owned(), comparability);
        }
        json!({
            "cdr": cdr,
            "display_name": name,
            "images": {"server": format!("{cdr}/server:1.0@sha256:{}", "0".repeat(64))},
            "posture": posture
        })
    };
    json!({
        "label": label,
        "pack": {"id": "community-vitals", "version": "1.0.0"},
        "target": {"base_url": "https://cdr.example/openehr/v1", "sut_version": "1.2.3"},
        "environment": {
            "os": "linux", "arch": "x86_64", "available_parallelism": 8,
            "cpu_model": "Fixture CPU", "total_memory_bytes": 34_359_738_368_i64
        },
        "started_at": "2026-01-02T03:04:05Z",
        "scale": {"factor": 1.0, "reference_configuration": true},
        "repetitions": [repetition.clone(), repetition.clone(), repetition],
        "cross": {"steady": {"regime": "open-loop", "operations": {"composition_create": stats}}},
        "baselines": [
            baseline("ehrbase", "EHRbase", json!([])),
            baseline("ferroehr", "FerroEHR", json!([{
                "item": "version_signing",
                "profile_declares": "none",
                "deployment_configures": "digest",
                "source": "https://example.invalid/ferroehr at v4.0.10: its pinned recipe"
            }]))
        ],
        "relative": [relative("ehrbase", "EHRbase"), relative("ferroehr", "FerroEHR")],
        "posture": board_posture(profile, audit)
    })
}

/// How many times one fragment occurs in the rendered page.
fn occurrences(page: &str, fragment: &str) -> usize {
    page.matches(fragment).count()
}

/// The board renders one section per declared posture profile, ranks rows only
/// inside their own section, and states each row's profile beside its numbers.
///
/// Ranking a `minimal` row against a `clinical-default` one would republish the
/// incomparability the posture block exists to close, so the grouping is a
/// property of the page rather than a convention a submitter is asked to
/// respect.
#[test]
#[expect(
    clippy::panic_in_result_fn,
    reason = "the Book's Result-test shape: assertions panic, plumbing propagates with `?`"
)]
fn the_board_groups_its_rows_by_declared_posture() -> Result<(), Box<dyn std::error::Error>> {
    if Command::new("jq").arg("--version").output().is_err() {
        eprintln!("SKIP the_board_groups_its_rows_by_declared_posture: no `jq` on PATH");
        return Ok(());
    }
    let root = board_workspace(&[
        (
            "alpha/2026-01-02-aaaaaaaa.json",
            board_record("Alpha CDR", "minimal", "off", 0.5),
        ),
        (
            "beta/2026-01-02-bbbbbbbb.json",
            board_record("Beta CDR", "clinical-default", "internal", 2.0),
        ),
        (
            "gamma/2026-01-02-cccccccc.json",
            board_record("Gamma CDR", "minimal", "off", 1.5),
        ),
    ])?;
    let rendered = Command::new("bash")
        .arg(root.path().join("scripts/render/bench-board.sh"))
        .output()?;
    assert!(
        rendered.status.success(),
        "the renderer failed: {}",
        String::from_utf8_lossy(&rendered.stderr)
    );
    let page = std::fs::read_to_string(root.path().join("website/landing/benchmarks.html"))?;

    assert_eq!(occurrences(&page, "class=\"board-group\""), 2, "{page}");
    assert!(page.contains("Posture <code>minimal</code>"), "{page}");
    assert!(
        page.contains("Posture <code>clinical-default</code>"),
        "{page}"
    );
    assert_eq!(
        occurrences(&page, "class=\"board-rank\" aria-hidden=\"true\">1<"),
        2,
        "each group ranks from one, so the first rank appears once per group: {page}"
    );
    let alpha = page
        .find("Alpha CDR")
        .ok_or("the first minimal row is missing")?;
    let gamma = page
        .find("Gamma CDR")
        .ok_or("the second minimal row is missing")?;
    let beta = page
        .find("Beta CDR")
        .ok_or("the clinical-default row is missing")?;
    assert!(alpha < gamma, "the faster minimal row ranks first: {page}");
    assert!(
        beta < alpha,
        "the two profiles never share a ranking, and clinical-default sorts first: {page}"
    );
    assert_eq!(
        occurrences(&page, "class=\"board-posture\">Posture <code>"),
        3,
        "every row states the profile it declared: {page}"
    );
    assert!(
        page.contains("verified version_signing, commit_validation, authn, tls, compression"),
        "{page}"
    );
    assert!(page.contains("declared-only audit, tenancy"), "{page}");
    assert!(
        page.contains(
            "Reference deployments that ran a different posture: FerroEHR: version_signing \
             declared digest"
        ),
        "{page}"
    );
    Ok(())
}
