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
    ArtifactRole, DeploymentKind, EntryId, EntryKind, Provenance,
    READABLE_REGISTRY_SCHEMA_VERSIONS, READABLE_RULES_VERSIONS, REGISTRY_SCHEMA_VERSION,
    RULES_VERSION, RegistryEntry, entry_defects,
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

/// The repository root, from this crate's manifest directory.
fn repo_root() -> PathBuf {
    Path::new(concat!(env!("CARGO_MANIFEST_DIR"), "/../..")).to_path_buf()
}

/// Every JSON file under one repository-relative directory, sorted, as paths
/// relative to the repository root.
fn json_files_under(relative: &str) -> Vec<String> {
    let root = repo_root();
    let dir = root.join(relative);
    let mut found = Vec::new();
    if dir.is_dir() {
        collect(&root, &dir, &mut found);
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

/// Reads one committed document as JSON.
fn read_document(relative: &str) -> serde_json::Value {
    let path = repo_root().join(relative);
    let text = std::fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("{} is unreadable: {e}", path.display()));
    serde_json::from_str(&text).unwrap_or_else(|e| panic!("{relative} is not valid JSON: {e}"))
}

/// Every committed entry, paired with its repository-relative path.
fn entries() -> Vec<(String, RegistryEntry)> {
    json_files_under(ENTRIES)
        .into_iter()
        .map(|relative| {
            let parsed: RegistryEntry = serde_json::from_value(read_document(&relative))
                .unwrap_or_else(|e| panic!("{relative} does not parse as a registry entry: {e}"));
            (relative, parsed)
        })
        .collect()
}

/// The lowercase-hex SHA-256 of one committed file.
fn digest_of(relative: &str) -> String {
    let path = repo_root().join(relative);
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
    for relative in json_files_under(ENTRIES) {
        let document = read_document(&relative);
        let found = violations(&validator, &document);
        assert!(
            found.is_empty(),
            "{relative} violates the published registry-entry schema: {}",
            found.join("; ")
        );
        let parsed: Result<RegistryEntry, _> = serde_json::from_value(document);
        assert!(
            parsed.is_ok(),
            "{relative} does not parse as a registry entry: {:?}",
            parsed.err()
        );
    }
}

#[test]
fn every_entry_is_publishable_by_the_rules_it_declares() {
    for (relative, entry) in entries() {
        let defects = entry_defects(&entry);
        assert!(
            defects.is_empty(),
            "{relative} falls short of the published submission rules: {}",
            defects
                .iter()
                .map(ToString::to_string)
                .collect::<Vec<_>>()
                .join("; ")
        );
        assert!(
            READABLE_REGISTRY_SCHEMA_VERSIONS.contains(&entry.registry_schema_version.as_str()),
            "{relative} declares a registry format this release cannot read: {:?} is outside \
             {READABLE_REGISTRY_SCHEMA_VERSIONS:?}",
            entry.registry_schema_version
        );
        assert!(
            READABLE_RULES_VERSIONS.contains(&entry.rules_version.as_str()),
            "{relative} declares rules this release does not accept: {:?} is outside \
             {READABLE_RULES_VERSIONS:?}",
            entry.rules_version
        );
    }
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
    for (relative, entry) in entries() {
        assert_eq!(
            relative,
            entry.expected_path(),
            "an entry is filed under its kind, its system and its id, so a reader finds it \
             without an index"
        );
    }
}

#[test]
fn entry_ids_are_unique_across_the_registry() {
    let mut seen: BTreeMap<String, String> = BTreeMap::new();
    for (relative, entry) in entries() {
        let id = entry.entry_id.as_str().to_owned();
        if let Some(first) = seen.insert(id.clone(), relative.clone()) {
            panic!(
                "{relative} reuses the entry id {id}, which {first} already carries; \
                 supersede-by-reference resolves ids, so a repeat makes an edge ambiguous"
            );
        }
    }
}

#[test]
fn every_artifact_is_committed_at_the_digest_the_entry_pins() {
    for (relative, entry) in entries() {
        for artifact in &entry.artifacts {
            let path = repo_root().join(&artifact.path);
            assert!(
                path.is_file(),
                "{relative} pins {} as its {} artifact, and nothing is committed there",
                artifact.path,
                artifact.role
            );
            assert_eq!(
                digest_of(&artifact.path),
                artifact.sha256.as_str(),
                "{relative} pins {} at a digest its committed bytes do not produce",
                artifact.path
            );
        }
    }
}

#[test]
fn every_superseded_id_is_an_entry_the_registry_carries() {
    let published: Vec<EntryId> = entries().into_iter().map(|(_, e)| e.entry_id).collect();
    for (relative, entry) in entries() {
        for superseded in &entry.supersedes {
            assert!(
                published.contains(superseded),
                "{relative} supersedes {superseded}, which the registry does not carry; a \
                 forward pointer to nothing leaves the replaced claim unfindable"
            );
        }
    }
}

/// The bench board reads its numbers from the submissions tree and its tier
/// from the registry, so the two trees are paired in both directions: a record
/// with no entry has nobody standing behind it, and an entry pointing at a
/// record nobody committed publishes a row with no evidence.
#[test]
fn every_published_bench_record_is_paired_with_exactly_one_entry() {
    let records: Vec<String> = json_files_under(BENCH_SUBMISSIONS)
        .into_iter()
        .filter(|path| !path.starts_with(&format!("{BENCH_SUBMISSIONS}/{BENCH_EXAMPLES}/")))
        .collect();
    let mut pinned: BTreeMap<String, String> = BTreeMap::new();
    for (relative, entry) in entries() {
        if entry.kind() != EntryKind::Bench {
            continue;
        }
        let artifact = entry
            .artifact(ArtifactRole::BenchResult)
            .unwrap_or_else(|| panic!("{relative} is a bench entry with no bench record"));
        if let Some(first) = pinned.insert(artifact.path.clone(), relative.clone()) {
            panic!(
                "{relative} and {first} both publish {}; one record is one claim",
                artifact.path
            );
        }
    }
    for record in &records {
        assert!(
            pinned.contains_key(record),
            "{record} is committed as a board row and no registry entry stands behind it"
        );
    }
    for (record, relative) in &pinned {
        assert!(
            records.contains(record),
            "{relative} publishes {record}, which is not committed as a board row"
        );
    }
}

/// A reproduced entry is produced here, so the deployment it names has to be
/// one this repository composes from its own recipe.
#[test]
fn every_reproduced_entry_names_a_committed_topology() {
    let declared: Vec<String> = topologies()
        .into_iter()
        .map(|(_, document)| {
            document
                .get("id")
                .and_then(serde_json::Value::as_str)
                .unwrap_or_default()
                .to_owned()
        })
        .collect();
    for (relative, entry) in entries() {
        if !matches!(entry.provenance, Provenance::Reproduced { .. }) {
            continue;
        }
        assert_eq!(
            entry.subject.deployment.kind,
            DeploymentKind::ReproducibleTopology,
            "{relative} claims the reproduced tier over a deployment the lane cannot compose"
        );
        let named = entry
            .subject
            .deployment
            .topology
            .as_deref()
            .unwrap_or_default();
        assert!(
            declared.iter().any(|id| id == named),
            "{relative} names the topology {named:?}, which no committed declaration carries"
        );
    }
}

/// Every committed topology declaration, paired with its path.
fn topologies() -> Vec<(String, serde_json::Value)> {
    json_files_under(TOPOLOGIES)
        .into_iter()
        .filter(|path| path.ends_with("/topology.json"))
        .map(|relative| {
            let document = read_document(&relative);
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
    for (relative, document) in topologies() {
        let found = violations(&validator, &document);
        assert!(
            found.is_empty(),
            "{relative} violates the published topology schema: {}",
            found.join("; ")
        );
        let id = field(&document, "id").unwrap_or_default();
        assert_eq!(
            relative,
            format!("{TOPOLOGIES}/{id}/topology.json"),
            "a topology is filed under its own id"
        );
        for named in ["compose_file", "ixit", "statement"] {
            if let Some(path) = field(&document, named) {
                assert!(
                    repo_root().join(path).is_file(),
                    "{relative} names {path} as its {named}, and nothing is committed there"
                );
            }
        }

        let ixit_path = field(&document, "ixit").expect("the schema makes the ixit mandatory");
        let ixit: Ixit = serde_json::from_value(read_document(ixit_path))
            .unwrap_or_else(|e| panic!("{ixit_path} does not parse as an ixit: {e}"));
        let base_url = field(&document, "base_url").expect("the schema makes base_url mandatory");
        let Some((_, sut)) = ixit
            .instances
            .iter()
            .find(|(name, _)| name.as_str() == "sut")
        else {
            panic!("{ixit_path} declares no `sut` instance")
        };
        assert_eq!(
            sut.base_url, base_url,
            "{relative} composes {base_url} and its ixit drives {}",
            sut.base_url
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
                        "{relative} declares no {variable} for the `{name}` instance, so the \
                         reproduction lane would drive it with nothing"
                    );
                }
            }
        }
    }
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
            "statement": format!("party/{system}/statement.json")
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
