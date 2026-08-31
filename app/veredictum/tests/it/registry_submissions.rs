// SPDX-FileCopyrightText: Veredictum contributors
// SPDX-License-Identifier: Apache-2.0

//! The gate every committed registry entry passes before it reaches a public
//! board.
//!
//! A registry entry is a published claim about somebody's product, so nothing
//! about it is taken on trust. Each entry is held to the published
//! registry-entry schema, to the rules its own fields declare, to the path its
//! kind and system name, and to the bytes of every artifact it pins. The tier
//! is checked from the other direction too: a submitter cannot promote their
//! own entry, because the reproduced tier names a workflow of this repository
//! and a deployment this repository composes itself.
//!
//! The pure half of the gate — everything checkable from one document alone —
//! is [`veredictum::registry::entry_defects`], which the library tests
//! exercise against seeded defects. What lives here is the half that needs the
//! committed tree.
//!
//! Every assertion over a tree of entries runs over TWO trees: the committed
//! registry, and a fixture tree that mirrors the repository's own layout. An
//! entry names every path it stands on relative to the repository root, so a
//! tree with that shape is scored by exactly the same assertions, and each
//! one refuses to pass having read nothing.

#![expect(
    clippy::expect_used,
    clippy::panic,
    reason = "the helpers below are not `#[test]` fns, so the clippy.toml in-tests scoping does not reach them; an entry this gate cannot even read must abort the gate loudly, Book ch11"
)]

use std::collections::BTreeMap;
use std::fmt::Write as _;
use std::path::{Path, PathBuf};
use std::process::Command;

use serde_json::json;
use sha2::{Digest as _, Sha256};

use veredictum::ixit::{AuthMode, Ixit};
use veredictum::registry::{
    ArtifactRole, DeploymentKind, EntryDefect, EntryId, EntryKind, Provenance,
    READABLE_REGISTRY_SCHEMA_VERSIONS, READABLE_RULES_VERSIONS, REGISTRY_SCHEMA_VERSION,
    RULES_VERSION, RegistryEntry, Tier, entry_defects,
};

/// The committed entries tree, relative to the repository root.
const ENTRIES: &str = "registry/entries";

/// The committed reproducible topologies, relative to the repository root.
const TOPOLOGIES: &str = "registry/topologies";

/// The benchmark records the bench board renders its numbers from.
const BENCH_SUBMISSIONS: &str = "benchmarks/submissions";

/// The sub-tree of bench records that demonstrate the submission pipe rather
/// than claim a board place. Those records are not published claims, so no
/// registry entry stands behind them.
const BENCH_EXAMPLES: &str = "examples";

/// The fixture tree that gives every assertion below material of its own.
///
/// It mirrors the repository's layout — entries, records, topologies and the
/// benchmark submissions tree — because an entry states every path it stands
/// on relative to a repository root, so a tree with that shape is scored
/// without loosening a single assertion. It sits outside `registry/`
/// deliberately: merging a file there IS publication, and a fixture row on a
/// public board would be a claim nobody made.
const FIXTURES: &str = "app/veredictum/tests/fixtures/registry/publishable";

/// The entry documents the gate must REFUSE, each named for its defect.
///
/// A refusal cannot live in a tree the assertions above require to be clean,
/// so it lives here and is read one document at a time.
const REFUSALS: &str = "app/veredictum/tests/fixtures/registry/refused";

/// How many entries the fixture tree carries.
const FIXTURE_ENTRIES: usize = 4;

/// How many artifacts those entries pin by digest.
const FIXTURE_ARTIFACTS: usize = 16;

/// The repository root, from this crate's manifest directory.
fn repo_root() -> PathBuf {
    Path::new(concat!(env!("CARGO_MANIFEST_DIR"), "/../..")).to_path_buf()
}

/// One tree of entries an assertion runs over, and how a failure names it.
#[derive(Debug)]
struct Root {
    /// What a failure message calls this tree.
    label: &'static str,
    /// What a repository-relative path resolves against.
    dir: PathBuf,
}

/// Every tree the gate assertions score: the committed registry first, then
/// the fixture tree.
///
/// The committed registry carries nothing until the first submission merges,
/// and an assertion that iterated nothing reads exactly like an assertion that
/// held. Keeping both trees in every loop means a real entry is scored the
/// moment one exists, and the logic is exercised until then.
fn roots() -> Vec<Root> {
    vec![
        Root {
            label: "the committed registry:",
            dir: repo_root(),
        },
        Root {
            label: "the fixture tree:",
            dir: repo_root().join(FIXTURES),
        },
    ]
}

/// Every JSON file under one directory of `root`, sorted, as paths relative to
/// `root` itself.
fn json_files_under(root: &Path, relative: &str) -> Vec<String> {
    let dir = root.join(relative);
    let mut found = Vec::new();
    if dir.is_dir() {
        collect(root, &dir, &mut found);
    }
    found.sort();
    found
}

/// Walks one directory, appending every JSON file under it.
fn collect(root: &Path, dir: &Path, found: &mut Vec<String>) {
    for entry in std::fs::read_dir(dir).expect("registry directory is readable") {
        let path = entry.expect("directory entry").path();
        if path.is_dir() {
            collect(root, &path, found);
        } else if path.extension().is_some_and(|ext| ext == "json") {
            found.push(
                path.strip_prefix(root)
                    .expect("every found path is under the repository root")
                    .to_string_lossy()
                    .replace('\\', "/"),
            );
        }
    }
}

/// Reads one document of `root` as JSON.
fn read_document(root: &Path, relative: &str) -> serde_json::Value {
    let path = root.join(relative);
    let text = std::fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("{} is unreadable: {e}", path.display()));
    serde_json::from_str(&text).unwrap_or_else(|e| panic!("{relative} is not valid JSON: {e}"))
}

/// Every entry of one root, paired with its path relative to that root.
fn entries(root: &Path) -> Vec<(String, RegistryEntry)> {
    json_files_under(root, ENTRIES)
        .into_iter()
        .map(|relative| {
            let parsed: RegistryEntry = serde_json::from_value(read_document(root, &relative))
                .unwrap_or_else(|e| panic!("{relative} does not parse as a registry entry: {e}"));
            (relative, parsed)
        })
        .collect()
}

/// The lowercase-hex SHA-256 of one file under `root`.
fn digest_of(root: &Path, relative: &str) -> String {
    let path = root.join(relative);
    let bytes = std::fs::read(&path)
        .unwrap_or_else(|e| panic!("{relative} is pinned by an entry and unreadable: {e}"));
    Sha256::digest(&bytes)
        .iter()
        .fold(String::with_capacity(64), |mut out: String, byte: &u8| {
            let _written = write!(out, "{byte:02x}");
            out
        })
}

/// Compiles one published schema into a validator.
fn validator_for(schema_file: &str) -> jsonschema::Validator {
    let text = std::fs::read_to_string(repo_root().join("schemas").join(schema_file))
        .unwrap_or_else(|e| panic!("{schema_file} is not committed: {e}"));
    let schema: serde_json::Value =
        serde_json::from_str(&text).unwrap_or_else(|e| panic!("{schema_file} is not JSON: {e}"));
    jsonschema::validator_for(&schema)
        .unwrap_or_else(|e| panic!("{schema_file} does not compile: {e}"))
}

/// Every schema violation of one document, as readable sentences.
fn violations(validator: &jsonschema::Validator, document: &serde_json::Value) -> Vec<String> {
    validator
        .iter_errors(document)
        .map(|error| format!("{}: {error}", error.instance_path()))
        .collect()
}

#[test]
fn every_entry_validates_against_the_published_schema() {
    let validator = validator_for("registry-entry.schema.json");
    let mut scored = 0_usize;
    for root in roots() {
        for relative in json_files_under(&root.dir, ENTRIES) {
            let document = read_document(&root.dir, &relative);
            let found = violations(&validator, &document);
            assert!(
                found.is_empty(),
                "{} {relative} violates the published registry-entry schema: {}",
                root.label,
                found.join("; ")
            );
            let parsed: Result<RegistryEntry, _> = serde_json::from_value(document);
            assert!(
                parsed.is_ok(),
                "{} {relative} does not parse as a registry entry: {:?}",
                root.label,
                parsed.err()
            );
            scored += 1;
        }
    }
    assert!(
        scored >= FIXTURE_ENTRIES,
        "the published schema was applied to {scored} entries and the fixture tree carries \
         {FIXTURE_ENTRIES}, so this assertion read less than the material committed for it"
    );
}

#[test]
fn every_entry_is_publishable_by_the_rules_it_declares() {
    let mut scored = 0_usize;
    for root in roots() {
        for (relative, entry) in entries(&root.dir) {
            let defects = entry_defects(&entry);
            assert!(
                defects.is_empty(),
                "{} {relative} falls short of the published submission rules: {}",
                root.label,
                defects
                    .iter()
                    .map(ToString::to_string)
                    .collect::<Vec<_>>()
                    .join("; ")
            );
            assert!(
                READABLE_REGISTRY_SCHEMA_VERSIONS.contains(&entry.registry_schema_version.as_str()),
                "{} {relative} declares a registry format this release cannot read: {:?} is \
                 outside {READABLE_REGISTRY_SCHEMA_VERSIONS:?}",
                root.label,
                entry.registry_schema_version
            );
            assert!(
                READABLE_RULES_VERSIONS.contains(&entry.rules_version.as_str()),
                "{} {relative} declares rules this release does not accept: {:?} is outside \
                 {READABLE_RULES_VERSIONS:?}",
                root.label,
                entry.rules_version
            );
            scored += 1;
        }
    }
    assert!(
        scored >= FIXTURE_ENTRIES,
        "the submission rules were applied to {scored} entries and the fixture tree carries \
         {FIXTURE_ENTRIES}, so this assertion read less than the material committed for it"
    );
}

/// A merged entry stays publishable at the version it was accepted under. The
/// fixture declares the earliest readable version and is otherwise untouched,
/// so both halves of the gate are pinned: the published schema admits it, and
/// the reader scores it with no version defect.
#[test]
fn an_entry_at_the_earliest_readable_version_stays_publishable() {
    let earliest_format = READABLE_REGISTRY_SCHEMA_VERSIONS
        .first()
        .copied()
        .unwrap_or(REGISTRY_SCHEMA_VERSION);
    let earliest_rules = READABLE_RULES_VERSIONS
        .first()
        .copied()
        .unwrap_or(RULES_VERSION);
    let (_reproduced, _console, self_reported) = board_provenances();
    let mut document = board_entry("beta", "2026-01-03-beta", &self_reported);
    document["registry_schema_version"] = json!(earliest_format);
    document["rules_version"] = json!(earliest_rules);
    pin_the_signature(&mut document, "beta", "2026-01-03-beta");

    let found = violations(&validator_for("registry-entry.schema.json"), &document);
    assert!(
        found.is_empty(),
        "the published schema refuses an entry at {earliest_format}: {}",
        found.join("; ")
    );
    let entry: RegistryEntry = serde_json::from_value(document)
        .unwrap_or_else(|e| panic!("an entry at {earliest_format} does not parse: {e}"));
    assert_eq!(
        entry_defects(&entry),
        Vec::new(),
        "an entry accepted under {earliest_rules} must stay publishable unedited"
    );
}

/// An entry naming a version this release does not carry is refused by the
/// published schema itself, so a submitter is told before any field of theirs
/// is read under the wrong meaning.
#[test]
fn the_published_schema_refuses_a_version_outside_the_readable_set() {
    let (_reproduced, _console, self_reported) = board_provenances();
    let mut document = board_entry("beta", "2026-01-03-beta", &self_reported);
    document["registry_schema_version"] = json!("0.9.0");
    document["rules_version"] = json!("0.9.0");
    let found = violations(&validator_for("registry-entry.schema.json"), &document);
    for field in ["/registry_schema_version", "/rules_version"] {
        assert!(
            found.iter().any(|violation| violation.starts_with(field)),
            "{field} outside the readable set must violate the published schema: {}",
            found.join("; ")
        );
    }
}

#[test]
fn every_entry_sits_at_the_path_its_own_fields_name() {
    let mut scored = 0_usize;
    for root in roots() {
        for (relative, entry) in entries(&root.dir) {
            assert_eq!(
                relative,
                entry.expected_path(),
                "{} an entry is filed under its kind, its system and its id, so a reader finds \
                 it without an index",
                root.label
            );
            scored += 1;
        }
    }
    assert!(
        scored >= FIXTURE_ENTRIES,
        "{scored} entry paths were checked and the fixture tree carries {FIXTURE_ENTRIES}, so \
         this assertion read less than the material committed for it"
    );
}

#[test]
fn entry_ids_are_unique_across_the_registry() {
    let mut scored = 0_usize;
    for root in roots() {
        let mut seen: BTreeMap<String, String> = BTreeMap::new();
        for (relative, entry) in entries(&root.dir) {
            let id = entry.entry_id.as_str().to_owned();
            if let Some(first) = seen.insert(id.clone(), relative.clone()) {
                panic!(
                    "{} {relative} reuses the entry id {id}, which {first} already carries; \
                     supersede-by-reference resolves ids, so a repeat makes an edge ambiguous",
                    root.label
                );
            }
            scored += 1;
        }
    }
    assert!(
        scored >= FIXTURE_ENTRIES,
        "{scored} entry ids were compared and the fixture tree carries {FIXTURE_ENTRIES}, so \
         this assertion read less than the material committed for it"
    );
}

#[test]
fn every_artifact_is_committed_at_the_digest_the_entry_pins() {
    let mut scored = 0_usize;
    for root in roots() {
        for (relative, entry) in entries(&root.dir) {
            for artifact in &entry.artifacts {
                let path = root.dir.join(&artifact.path);
                assert!(
                    path.is_file(),
                    "{} {relative} pins {} as its {} artifact, and nothing is committed there",
                    root.label,
                    artifact.path,
                    artifact.role
                );
                assert_eq!(
                    digest_of(&root.dir, &artifact.path),
                    artifact.sha256.as_str(),
                    "{} {relative} pins {} at a digest its committed bytes do not produce",
                    root.label,
                    artifact.path
                );
                scored += 1;
            }
        }
    }
    assert!(
        scored >= FIXTURE_ARTIFACTS,
        "{scored} pinned artifacts were re-digested and the fixture entries pin \
         {FIXTURE_ARTIFACTS}, so this assertion read less than the material committed for it"
    );
}

#[test]
fn every_superseded_id_is_an_entry_the_registry_carries() {
    let mut scored = 0_usize;
    for root in roots() {
        let published: Vec<EntryId> = entries(&root.dir)
            .into_iter()
            .map(|(_, e)| e.entry_id)
            .collect();
        for (relative, entry) in entries(&root.dir) {
            for superseded in &entry.supersedes {
                assert!(
                    published.contains(superseded),
                    "{} {relative} supersedes {superseded}, which the registry does not carry; a \
                     forward pointer to nothing leaves the replaced claim unfindable",
                    root.label
                );
                scored += 1;
            }
        }
    }
    assert_ne!(
        scored, 0,
        "no supersede edge was resolved at all, so this assertion proved nothing; the fixture \
         tree carries one entry that replaces another exactly so it cannot"
    );
}

/// The bench board reads its numbers from the submissions tree and its tier
/// from the registry, so the two trees are paired in both directions: a record
/// with no entry has nobody standing behind it, and an entry pointing at a
/// record nobody committed publishes a row with no evidence.
#[test]
fn every_published_bench_record_is_paired_with_exactly_one_entry() {
    let mut scored = 0_usize;
    for root in roots() {
        let records: Vec<String> = json_files_under(&root.dir, BENCH_SUBMISSIONS)
            .into_iter()
            .filter(|path| !path.starts_with(&format!("{BENCH_SUBMISSIONS}/{BENCH_EXAMPLES}/")))
            .collect();
        let mut pinned: BTreeMap<String, String> = BTreeMap::new();
        for (relative, entry) in entries(&root.dir) {
            if entry.kind() != EntryKind::Bench {
                continue;
            }
            let artifact = entry
                .artifact(ArtifactRole::BenchResult)
                .unwrap_or_else(|| panic!("{relative} is a bench entry with no bench record"));
            if let Some(first) = pinned.insert(artifact.path.clone(), relative.clone()) {
                panic!(
                    "{} {relative} and {first} both publish {}; one record is one claim",
                    root.label, artifact.path
                );
            }
        }
        for record in &records {
            assert!(
                pinned.contains_key(record),
                "{} {record} is committed as a board row and no registry entry stands behind it",
                root.label
            );
        }
        for (record, relative) in &pinned {
            assert!(
                records.contains(record),
                "{} {relative} publishes {record}, which is not committed as a board row",
                root.label
            );
            scored += 1;
        }
    }
    assert_ne!(
        scored, 0,
        "no bench record was paired at all, so this assertion proved nothing in either \
         direction; the fixture tree carries one record and the entry that publishes it"
    );
}

/// A reproduced entry is produced here, so the deployment it names has to be
/// one this repository composes from its own recipe.
#[test]
fn every_reproduced_entry_names_a_committed_topology() {
    let mut scored = 0_usize;
    for root in roots() {
        let declared: Vec<String> = topologies(&root.dir)
            .into_iter()
            .map(|(_, document)| {
                document
                    .get("id")
                    .and_then(serde_json::Value::as_str)
                    .unwrap_or_default()
                    .to_owned()
            })
            .collect();
        for (relative, entry) in entries(&root.dir) {
            if !matches!(entry.provenance, Provenance::Reproduced { .. }) {
                continue;
            }
            assert_eq!(
                entry.subject.deployment.kind,
                DeploymentKind::ReproducibleTopology,
                "{} {relative} claims the reproduced tier over a deployment the lane cannot \
                 compose",
                root.label
            );
            let named = entry
                .subject
                .deployment
                .topology
                .as_deref()
                .unwrap_or_default();
            assert!(
                declared.iter().any(|id| id == named),
                "{} {relative} names the topology {named:?}, which no committed declaration \
                 carries",
                root.label
            );
            scored += 1;
        }
    }
    assert_ne!(
        scored, 0,
        "no reproduced entry was resolved against a topology at all, so this assertion proved \
         nothing; the fixture tree carries one reproduced entry over a declared topology"
    );
}

/// The fixture tree is what stops every assertion above from passing having
/// read nothing, so what it carries is asserted here rather than assumed: one
/// entry per tier, both entry kinds, the supersede edge, and the artifact
/// count the digest assertion holds itself to.
#[test]
fn the_fixture_tree_carries_one_entry_per_tier() {
    let fixtures = repo_root().join(FIXTURES);
    let committed = entries(&fixtures);
    assert_eq!(
        committed.len(),
        FIXTURE_ENTRIES,
        "the assertions above hold themselves to {FIXTURE_ENTRIES} fixture entries"
    );

    let mut tiers: Vec<Tier> = committed.iter().map(|(_, entry)| entry.tier()).collect();
    tiers.sort_unstable();
    tiers.dedup();
    assert_eq!(
        tiers,
        Tier::ALL.to_vec(),
        "the tier is the discriminant of the provenance block and each variant requires \
         different evidence, so a tier with no fixture is a third of the gate nothing scores"
    );

    let mut kinds: Vec<EntryKind> = committed.iter().map(|(_, entry)| entry.kind()).collect();
    kinds.sort_unstable();
    kinds.dedup();
    assert_eq!(
        kinds,
        EntryKind::ALL.to_vec(),
        "the two boards are separate surfaces, and the bench pairing assertion has nothing to \
         pair without a bench entry"
    );

    let pinned: usize = committed
        .iter()
        .map(|(_, entry)| entry.artifacts.len())
        .sum();
    assert_eq!(
        pinned, FIXTURE_ARTIFACTS,
        "the digest assertion holds itself to {FIXTURE_ARTIFACTS} pinned artifacts"
    );

    let edges: usize = committed
        .iter()
        .map(|(_, entry)| entry.supersedes.len())
        .sum();
    assert_ne!(
        edges, 0,
        "the supersede assertion resolves nothing unless one fixture entry replaces another"
    );
}

/// The published schema one record document is held to, by its file name.
///
/// A closed vocabulary: a name this match does not carry aborts the gate,
/// because a document nobody validates is a fixture that can drift into
/// nonsense while every assertion above stays green.
fn record_schema(name: &str) -> Option<&'static str> {
    match name {
        "results.json" => Some("results.schema.json"),
        "transcript.json" => Some("run-transcript.schema.json"),
        "ixit.json" => Some("ixit.schema.json"),
        "statement.json" => Some("statement.schema.json"),
        // NOTE: no verdicts schema is published under `schemas/`, so the
        // verdicts document is the one record file with nothing to hold it to.
        "verdicts.json" => None,
        other => panic!("{other} is a record document no published schema is named for"),
    }
}

/// The evidence a fixture entry pins is a real instance of its own published
/// schema, so the tree the assertions above score cannot drift into documents
/// that only look like a record.
#[test]
fn every_fixture_record_document_matches_its_published_schema() {
    let fixtures = repo_root().join(FIXTURES);
    let mut scored = 0_usize;
    for relative in json_files_under(&fixtures, "registry/records") {
        let name = Path::new(&relative)
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or_default();
        let Some(schema_file) = record_schema(name) else {
            continue;
        };
        let found = violations(
            &validator_for(schema_file),
            &read_document(&fixtures, &relative),
        );
        assert!(
            found.is_empty(),
            "{relative} violates {schema_file}: {}",
            found.join("; ")
        );
        scored += 1;
    }
    for relative in json_files_under(&fixtures, BENCH_SUBMISSIONS) {
        let found = violations(
            &validator_for("bench-result.schema.json"),
            &read_document(&fixtures, &relative),
        );
        assert!(
            found.is_empty(),
            "{relative} violates the published bench-result schema: {}",
            found.join("; ")
        );
        scored += 1;
    }
    assert_ne!(
        scored, 0,
        "no fixture record document was validated at all, so this assertion proved nothing"
    );
}

/// Whether the published schema refuses a document as well as the gate does.
///
/// The two readers overlap without being the same reader: the schema judges
/// one document's shape, and the gate also judges what its fields say about
/// each other. Each refusal fixture declares which of the two catches it, so
/// neither can quietly stop catching it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SchemaVerdict {
    /// The published schema refuses the document on its own.
    Refuses,
    /// The schema admits it, because the rule it breaks is a relation between
    /// fields the published schema does not express.
    Admits,
}

/// Every refusal fixture, as the one defect the gate must report for it and
/// the verdict the published schema reaches on the same bytes.
fn refusals() -> Vec<(&'static str, EntryDefect, SchemaVerdict)> {
    vec![
        (
            "blank-conflict-of-interest.json",
            EntryDefect::EmptyField {
                field: "disclosure.conflict_of_interest",
            },
            SchemaVerdict::Refuses,
        ),
        (
            "unreadable-format-version.json",
            EntryDefect::SchemaVersion {
                declared: String::from("0.9.0"),
            },
            SchemaVerdict::Refuses,
        ),
        (
            "signature-nothing-pins.json",
            EntryDefect::UndeclaredSignature {
                path: String::from(
                    "registry/records/gamma/2026-08-27-gamma-self-reported/verdicts.json.asc",
                ),
            },
            SchemaVerdict::Admits,
        ),
        (
            "reproduced-over-a-local-build.json",
            EntryDefect::UnreproducibleDeployment {
                kind: DeploymentKind::LocalBuild,
            },
            SchemaVerdict::Admits,
        ),
    ]
}

/// The gate says no as well as yes, and it says no for the reason the fixture
/// was authored for: the defect is compared by kind, so rewording a diagnostic
/// cannot turn a refusal into a pass.
#[test]
fn every_refusal_fixture_is_refused_with_the_defect_it_carries() {
    let root = repo_root();
    let validator = validator_for("registry-entry.schema.json");
    let authored = refusals();
    assert_eq!(
        json_files_under(&root, REFUSALS).len(),
        authored.len(),
        "every document under {REFUSALS} names the defect it was authored for, so a fixture with \
         no expectation is a refusal nothing checks"
    );
    for (file, expected, verdict) in authored {
        let relative = format!("{REFUSALS}/{file}");
        let document = read_document(&root, &relative);
        let found = violations(&validator, &document);
        match verdict {
            SchemaVerdict::Refuses => assert_ne!(
                found.len(),
                0,
                "{file} must also violate the published registry-entry schema"
            ),
            SchemaVerdict::Admits => assert_eq!(
                found,
                Vec::<String>::new(),
                "{file} is refused by the gate alone, and the schema now refuses it too: say so \
                 rather than leaving the two readers disagreeing about which one caught it"
            ),
        }
        let entry: RegistryEntry = serde_json::from_value(document)
            .unwrap_or_else(|e| panic!("{relative} does not parse as a registry entry: {e}"));
        assert_eq!(
            entry_defects(&entry),
            vec![expected],
            "{file} is authored to carry exactly one defect"
        );
    }
}

/// Every topology declaration under one root, paired with its path.
fn topologies(root: &Path) -> Vec<(String, serde_json::Value)> {
    json_files_under(root, TOPOLOGIES)
        .into_iter()
        .filter(|path| path.ends_with("/topology.json"))
        .map(|relative| {
            let document = read_document(root, &relative);
            (relative, document)
        })
        .collect()
}

/// One string field of a topology declaration.
fn field<'a>(document: &'a serde_json::Value, name: &str) -> Option<&'a str> {
    document.get(name).and_then(serde_json::Value::as_str)
}

/// A reproduction runs nothing a submitter wrote, so every part of a topology
/// is committed here and every part of it resolves: the compose recipe, the
/// ixit, the statement, and one credential per environment variable the ixit's
/// auth modes name. The declared base URL and the ixit's own `sut` instance
/// have to agree, because a topology whose two halves disagree drives the
/// catalogue somewhere nobody composed.
#[test]
fn every_topology_declares_a_deployment_the_lane_can_compose() {
    let validator = validator_for("registry-topology.schema.json");
    let mut scored = 0_usize;
    for root in roots() {
        for (relative, document) in topologies(&root.dir) {
            let found = violations(&validator, &document);
            assert!(
                found.is_empty(),
                "{} {relative} violates the published topology schema: {}",
                root.label,
                found.join("; ")
            );
            let id = field(&document, "id").unwrap_or_default();
            assert_eq!(
                relative,
                format!("{TOPOLOGIES}/{id}/topology.json"),
                "{} a topology is filed under its own id",
                root.label
            );
            for named in ["compose_file", "ixit", "statement"] {
                if let Some(path) = field(&document, named) {
                    assert!(
                        root.dir.join(path).is_file(),
                        "{} {relative} names {path} as its {named}, and nothing is committed there",
                        root.label
                    );
                }
            }

            let ixit_path = field(&document, "ixit").expect("the schema makes the ixit mandatory");
            let ixit: Ixit = serde_json::from_value(read_document(&root.dir, ixit_path))
                .unwrap_or_else(|e| panic!("{ixit_path} does not parse as an ixit: {e}"));
            let base_url =
                field(&document, "base_url").expect("the schema makes base_url mandatory");
            let Some((_, sut)) = ixit
                .instances
                .iter()
                .find(|(name, _)| name.as_str() == "sut")
            else {
                panic!("{ixit_path} declares no `sut` instance")
            };
            assert_eq!(
                sut.base_url, base_url,
                "{} {relative} composes {base_url} and its ixit drives {}",
                root.label, sut.base_url
            );

            let credentials = document
                .get("credentials")
                .and_then(serde_json::Value::as_object)
                .expect("the schema makes credentials mandatory");
            for (name, instance) in &ixit.instances {
                if let AuthMode::Basic {
                    user_env,
                    password_env,
                } = &instance.auth
                {
                    for variable in [user_env, password_env] {
                        assert!(
                            credentials.contains_key(variable),
                            "{} {relative} declares no {variable} for the `{name}` instance, so \
                             the reproduction lane would drive it with nothing",
                            root.label
                        );
                    }
                }
            }
            scored += 1;
        }
    }
    assert_ne!(
        scored, 0,
        "no topology declaration was read at all, so this assertion proved nothing"
    );
}

/// The renderer under test, and the tree it reads.
///
/// The script derives its own root from its path and reads the registry under
/// it, so a copy in a temporary tree renders a temporary board: the committed
/// page and the committed entries are never touched.
fn board_workspace(
    entries: &[(&str, serde_json::Value)],
    artifacts: &[(&str, serde_json::Value)],
) -> Result<assert_fs::TempDir, Box<dyn std::error::Error>> {
    let root = assert_fs::TempDir::new()?;
    let script_dir = root.path().join("scripts").join("render");
    std::fs::create_dir_all(&script_dir)?;
    std::fs::create_dir_all(root.path().join("website").join("landing"))?;
    let _copied = std::fs::copy(
        repo_root().join("scripts/render/conformance-board.sh"),
        script_dir.join("conformance-board.sh"),
    )?;
    for (name, document) in entries.iter().chain(artifacts) {
        let path = root.path().join(name);
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(&path, serde_json::to_string_pretty(document)?)?;
    }
    Ok(root)
}

/// One committed entry as the RENDERER reads it.
///
/// Hand-written JSON, because these bytes are the renderer's input rather than
/// a value the engine produced here; the entries the registry actually carries
/// are held to the published schema by the tests above.
fn board_entry(system: &str, id: &str, tier: &serde_json::Value) -> serde_json::Value {
    // The deployment follows the tier the way the rules bind them: a console
    // run reached an endpoint the submitter named, so a fixture row cannot
    // pair that tier with a deployment the instrument never drives.
    let deployment = if tier.get("tier").and_then(serde_json::Value::as_str) == Some("console") {
        json!({"kind": "hosted-endpoint", "endpoint": "https://cdr.example/openehr/v1", "reproduction_authorized": false})
    } else {
        json!({"kind": "container-image", "reproduction_authorized": false})
    };
    json!({
        "registry_schema_version": REGISTRY_SCHEMA_VERSION,
        "entry_id": id,
        "rules_version": RULES_VERSION,
        "submitter": {
            "name": "Fixture Author",
            "contact": "https://example.invalid",
            "relationship": "independent"
        },
        "subject": {
            "system": system,
            "display_name": system,
            "version": "1.0.0",
            "deployment": deployment
        },
        "disclosure": {
            "instrument_version": "0.1.1",
            "run_started_at": format!("{}T00:00:00Z", id.get(..10).unwrap_or_default()),
            "environment": {"os": "linux", "arch": "x86_64", "host_class": "a fixture host"},
            "sut_configuration": "basic auth, template validation",
            "conflict_of_interest": "none"
        },
        "result": {
            "kind": "conformance",
            "catalogue_revision": "abcdef1",
            "statement": format!("registry/records/{system}/{id}/statement.json")
        },
        "artifacts": [
            {"role": "results", "path": format!("registry/records/{system}/{id}/results.json"),
             "sha256": "0".repeat(64)},
            {"role": "verdicts", "path": format!("registry/records/{system}/{id}/verdicts.json"),
             "sha256": "1".repeat(64)}
        ],
        "provenance": tier
    })
}

/// Declares the signature a self-reported [`board_entry`] carries as an
/// artifact, so the gate can read the bytes the signature covers.
///
/// The renderer never looks at artifact roles, so the board fixture pins none;
/// the gate does, and refuses a signature nothing pins.
fn pin_the_signature(document: &mut serde_json::Value, system: &str, id: &str) {
    let artifacts = document["artifacts"]
        .as_array_mut()
        .expect("a fixture entry carries an artifact list");
    artifacts.push(json!({
        "role": "signature",
        "path": format!("registry/records/{system}/{id}/verdicts.json.asc"),
        "sha256": "2".repeat(64)
    }));
}

/// A verdicts document as the renderer reads it: the coverage bound and the
/// per-tier profile verdicts, which are the only fields a row prints.
fn board_verdicts(passed: u64, failed: u64) -> serde_json::Value {
    json!({
        "review": [],
        "capabilities": [],
        "capability_tallies": [],
        "profiles": [["CORE", "pass"], ["STANDARD", "fail"]],
        "security": null,
        "performance": [],
        "coverage": {
            "selected": passed + failed + 5,
            "driven": passed + failed,
            "passed": passed,
            "failed": failed,
            "inconclusive": 0
        }
    })
}

/// How many times one fragment occurs in the rendered page.
fn occurrences(page: &str, fragment: &str) -> usize {
    page.matches(fragment).count()
}

/// The three provenance blocks a board row can carry, each shaped as its tier
/// requires: the workflow identity, the re-derivation lane plus the signature
/// it made, and the submitter's own signature.
fn board_provenances() -> (serde_json::Value, serde_json::Value, serde_json::Value) {
    let reproduced = json!({
        "tier": "reproduced",
        "workflow_ref": "rubentalstra/Veredictum/.github/workflows/registry-reproduce.yml@refs/heads/main",
        "run_id": "42",
        "run_attempt": 1,
        "predicate_type": "https://slsa.dev/provenance/v1",
        "verify_command": "gh attestation verify verdicts.json --repo rubentalstra/Veredictum"
    });
    let console = json!({
        "tier": "console",
        "instrument_origin": "https://console.veredictum.eu",
        "console_run_id": "018f3b1e-6f0a-7c21-9a3d-6c2f5d4b8e77",
        "workflow_ref": "rubentalstra/Veredictum/.github/workflows/registry-console.yml@refs/heads/main",
        "run_id": "43",
        "run_attempt": 1,
        "scheme": "openpgp-detached",
        "signature": "registry/records/gamma/2026-01-04-gamma/verdicts.json.asc",
        "signs": "registry/records/gamma/2026-01-04-gamma/verdicts.json",
        "identity": "0123456789ABCDEF",
        "verify_command": "veredictum verify-record --record ."
    });
    let self_reported = json!({
        "tier": "self-reported",
        "scheme": "openpgp-detached",
        "signature": "registry/records/beta/2026-01-03-beta/verdicts.json.asc",
        "signs": "registry/records/beta/2026-01-03-beta/verdicts.json",
        "identity": "0123456789ABCDEF",
        "verify_command": "gpg --verify verdicts.json.asc"
    });
    (reproduced, console, self_reported)
}

/// The board labels the tier of every row from the entry's own provenance, and
/// never from a default, so a self-reported row can never read as a reproduced
/// one. The report-not-certificate boundary is on the page whatever the rows
/// say.
#[test]
#[expect(
    clippy::panic_in_result_fn,
    reason = "the Book's Result-test shape: assertions panic, plumbing propagates with `?`"
)]
fn the_conformance_board_labels_every_row_with_its_tier() -> Result<(), Box<dyn std::error::Error>>
{
    if Command::new("jq").arg("--version").output().is_err() {
        eprintln!("SKIP the_conformance_board_labels_every_row_with_its_tier: no `jq` on PATH");
        return Ok(());
    }
    let (reproduced, console, self_reported) = board_provenances();
    let root = board_workspace(
        &[
            (
                "registry/entries/conformance/alpha/2026-01-02-alpha.json",
                board_entry("alpha", "2026-01-02-alpha", &reproduced),
            ),
            (
                "registry/entries/conformance/beta/2026-01-03-beta.json",
                board_entry("beta", "2026-01-03-beta", &self_reported),
            ),
            (
                "registry/entries/conformance/gamma/2026-01-04-gamma.json",
                board_entry("gamma", "2026-01-04-gamma", &console),
            ),
        ],
        &[
            (
                "registry/records/alpha/2026-01-02-alpha/verdicts.json",
                board_verdicts(90, 10),
            ),
            (
                "registry/records/beta/2026-01-03-beta/verdicts.json",
                board_verdicts(40, 60),
            ),
            (
                "registry/records/gamma/2026-01-04-gamma/verdicts.json",
                board_verdicts(70, 30),
            ),
        ],
    )?;
    let rendered = Command::new("bash")
        .arg(root.path().join("scripts/render/conformance-board.sh"))
        .output()?;
    assert!(
        rendered.status.success(),
        "the renderer failed: {}",
        String::from_utf8_lossy(&rendered.stderr)
    );
    let page = std::fs::read_to_string(root.path().join("website/landing/conformance-board.html"))?;

    assert_eq!(
        occurrences(&page, "<article class=\"board-row"),
        3,
        "{page}"
    );
    assert!(
        page.contains("<span class=\"tier tier-reproduced\">reproduced</span>"),
        "{page}"
    );
    assert!(
        page.contains("<span class=\"tier tier-console\">console</span>"),
        "{page}"
    );
    assert!(
        page.contains("<span class=\"tier tier-self-reported\">self-reported</span>"),
        "{page}"
    );
    assert!(
        page.contains("An entry is a report, never a certificate."),
        "the boundary is on every rendered surface: {page}"
    );
    assert!(
        page.contains("<span class=\"n\">90%</span>"),
        "the share is taken over driven cases, not selected: {page}"
    );
    let alpha = page
        .find("2026-01-02-alpha")
        .ok_or("the reproduced row is missing")?;
    let beta = page
        .find("2026-01-03-beta")
        .ok_or("the self-reported row is missing")?;
    let gamma = page
        .find("2026-01-04-gamma")
        .ok_or("the console row is missing")?;
    assert!(
        alpha < gamma && gamma < beta,
        "who performed a run orders the page: the two official tiers first, and the tier whose \
         environment was composed here ahead of the one whose endpoint the submitter chose: \
         {page}"
    );
    Ok(())
}

/// A registry with nothing merged renders the empty board rather than
/// failing, and still carries the boundary the rules publish.
#[test]
#[expect(
    clippy::panic_in_result_fn,
    reason = "the Book's Result-test shape: assertions panic, plumbing propagates with `?`"
)]
fn an_empty_registry_renders_the_empty_board() -> Result<(), Box<dyn std::error::Error>> {
    if Command::new("jq").arg("--version").output().is_err() {
        eprintln!("SKIP an_empty_registry_renders_the_empty_board: no `jq` on PATH");
        return Ok(());
    }
    let root = board_workspace(&[], &[])?;
    let rendered = Command::new("bash")
        .arg(root.path().join("scripts/render/conformance-board.sh"))
        .output()?;
    assert!(
        rendered.status.success(),
        "the renderer failed: {}",
        String::from_utf8_lossy(&rendered.stderr)
    );
    let page = std::fs::read_to_string(root.path().join("website/landing/conformance-board.html"))?;
    assert!(
        page.contains("No conformance entry has been merged yet"),
        "{page}"
    );
    assert!(
        page.contains("An entry is a report, never a certificate."),
        "{page}"
    );
    Ok(())
}

/// An entry whose verdicts artifact is not committed stops the render rather
/// than producing a row with no numbers behind it.
#[test]
#[expect(
    clippy::panic_in_result_fn,
    reason = "the Book's Result-test shape: assertions panic, plumbing propagates with `?`"
)]
fn a_row_without_its_evidence_stops_the_render() -> Result<(), Box<dyn std::error::Error>> {
    if Command::new("jq").arg("--version").output().is_err() {
        eprintln!("SKIP a_row_without_its_evidence_stops_the_render: no `jq` on PATH");
        return Ok(());
    }
    let root = board_workspace(
        &[(
            "registry/entries/conformance/alpha/2026-01-02-alpha.json",
            board_entry(
                "alpha",
                "2026-01-02-alpha",
                &json!({
                    "tier": "self-reported",
                    "scheme": "openpgp-detached",
                    "signature": "registry/records/alpha/2026-01-02-alpha/verdicts.json.asc",
                    "signs": "registry/records/alpha/2026-01-02-alpha/verdicts.json",
                    "identity": "0123456789ABCDEF",
                    "verify_command": "gpg --verify verdicts.json.asc"
                }),
            ),
        )],
        &[],
    )?;
    let rendered = Command::new("bash")
        .arg(root.path().join("scripts/render/conformance-board.sh"))
        .output()?;
    assert!(
        !rendered.status.success(),
        "a board row with no committed evidence must stop the render"
    );
    assert!(
        String::from_utf8_lossy(&rendered.stderr).contains("pins no readable verdicts artifact"),
        "{}",
        String::from_utf8_lossy(&rendered.stderr)
    );
    Ok(())
}
