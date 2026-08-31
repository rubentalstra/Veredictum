// SPDX-FileCopyrightText: Veredictum contributors
// SPDX-License-Identifier: Apache-2.0

//! The loaded artifact set — one root directory laid out per the artifact
//! families:
//!
//! ```text
//! <root>/schedule/**/*.yaml        case cores
//! <root>/bindings/<its>/*.yaml     operation bindings
//! <root>/vocab/outcomes.yaml       outcome vocabulary
//! <root>/vocab/selectors.yaml      selector vocabulary + ignore-sets
//! <root>/vocab/capability_matrix.yaml
//! <root>/vocab/journey_catalogue.yaml   the hospital-simulation journeys
//! <root>/corpus/MANIFEST.yaml      corpus manifest
//! <root>/registers/ambiguities.yaml
//! ```
//!
//! A DECLARATION is never swept from the tree. ISO/IEC 9646-7 assigns the
//! support and supported-values columns of an ICS proforma to "the supplier
//! of the implementation", so a declaration is submitted rather
//! than committed here, and [`Loaded::review_declaration`] adds the one a
//! caller supplies. With none supplied, the checks that are relations between
//! a claim and the catalogue have no subject and report nothing; the
//! catalogue-side gates are unaffected.
//!
//! Loading never fails fast: every file error becomes a finding, so one
//! validation run reports the whole tree.

#![expect(
    clippy::disallowed_types,
    reason = "dev/verification tooling over JSON artifacts (the catalogue, results, wire \
              exchanges), whose shapes belong to the artifacts and the SUT"
)]

use std::path::{Path, PathBuf};
use std::sync::LazyLock;

use crate::load::{LoadError, compile_schema, load_artifact};
use crate::model::binding::OperationBinding;
use crate::model::capability::CapabilityMatrix;
use crate::model::case::CaseCore;
use crate::model::corpus::CorpusManifest;
use crate::model::register::AmbiguityRegister;
use crate::model::vocab_files::{OutcomesVocab, SelectorsVocab};
use crate::model::wire_surface::WireSurface;
use crate::schema;

/// The fully-typed artifact set (files that failed to load are absent; their
/// errors travel alongside).
#[derive(Debug, Default)]
pub struct ArtifactSet {
    /// The loaded functional/content case cores, with their source paths.
    pub cases: Vec<(PathBuf, CaseCore)>,
    /// `kind: performance` cases (their own schema family; measured, not
    /// asserted).
    pub performance: Vec<(PathBuf, crate::perf::PerformanceCase)>,
    /// The wire realizations, one file per SM operation (plus variants).
    pub bindings: Vec<(PathBuf, OperationBinding)>,
    /// The outcome-kind vocabulary.
    pub outcomes: Option<(PathBuf, OutcomesVocab)>,
    /// The selector vocabulary (server-assigned sets, ctx defaults).
    pub selectors: Option<(PathBuf, SelectorsVocab)>,
    /// The capability matrix: the certificate's rating dimensions.
    pub matrix: Option<(PathBuf, CapabilityMatrix)>,
    /// The clinical journey catalogue the performance workloads decompose
    /// into (`vocab/journey_catalogue.yaml`).
    pub journeys: Option<(PathBuf, crate::perf::JourneyCatalogue)>,
    /// The corpus manifest: every fixture, its format and its adjudication.
    pub corpus: Option<(PathBuf, CorpusManifest)>,
    /// The ambiguity register: spec silences with a typed disposition.
    pub register: Option<(PathBuf, AmbiguityRegister)>,
    /// The wire-surface coverage register (`vocab/wire_surface.yaml`) — the
    /// authored, spec-cited exceptions + cross-cutting elements the
    /// `surface-coverage` gate measures the catalogue against.
    pub wire_surface: Option<(PathBuf, WireSurface)>,
    /// The corpus manifest's directory (source paths resolve against it).
    pub corpus_dir: Option<PathBuf>,
    /// The declarations this pass reviews, in the order they were supplied —
    /// the ICS side of the static conformance review. Empty unless a caller
    /// supplied one through [`Loaded::review_declaration`].
    pub parties: Vec<(PathBuf, crate::party::Statement)>,
    /// The ixit topology supplied beside each declaration (`ixit.json` in the
    /// declaration's own directory). A declaration supplied without one is
    /// absent here, so a check pairing the two reads the directory both
    /// entries share.
    pub party_ixits: Vec<(PathBuf, crate::ixit::Ixit)>,
}

/// A load pass over one artifact root.
#[derive(Debug, Default)]
pub struct Loaded {
    /// Everything that loaded and typed successfully.
    pub set: ArtifactSet,
    /// One error per file that did not, in discovery order.
    pub errors: Vec<LoadError>,
}

impl Loaded {
    /// Adds one SUPPLIED declaration to the set this pass reviews: the ICS at
    /// `statement` plus the `ixit.json` beside it, each schema-validated like
    /// every other artifact.
    ///
    /// The ixit is read from the declaration's own directory because a
    /// capability the ICS claims and the topology never declares is a relation
    /// between the two documents, invisible from either one alone. An absent
    /// ixit is silent: there is no second declaration to contradict.
    ///
    /// A file error becomes a [`Loaded::errors`] entry rather than a return,
    /// keeping this module's never-fail-fast law.
    ///
    /// # Errors
    /// Only on a schema-compilation defect in [`crate::schema`] itself.
    pub fn review_declaration(&mut self, statement: &Path) -> Result<(), LoadError> {
        let schemas = compiled_schemas()?;
        let ixit_path = statement.with_file_name("ixit.json");
        if ixit_path.is_file() {
            match load_json_document::<crate::ixit::Ixit>(&ixit_path, schemas.ixit) {
                Ok(ixit) => self.set.party_ixits.push((ixit_path, ixit)),
                Err(e) => self.errors.push(e),
            }
        }
        match load_json_document::<crate::party::Statement>(statement, schemas.statement) {
            Ok(parsed) => self.set.parties.push((statement.to_owned(), parsed)),
            Err(e) => self.errors.push(e),
        }
        Ok(())
    }
}

fn yaml_files_under(dir: &Path) -> Vec<PathBuf> {
    let mut files = Vec::new();
    let mut stack = vec![dir.to_owned()];
    while let Some(current) = stack.pop() {
        let Ok(entries) = std::fs::read_dir(&current) else {
            continue;
        };
        for entry in entries.flatten() {
            // `entry.file_type()` reads the kind the `read_dir` iterator
            // already carries; `Path::is_dir` would re-`stat` every entry.
            // It reports the ENTRY's own kind and does NOT follow symlinks
            // (<https://doc.rust-lang.org/std/fs/struct.DirEntry.html#method.file_type>),
            // so a symlink still costs the following stat — the seeded-defect
            // harness overlays the catalogue with symlinked directories, and
            // treating one as a leaf would silently truncate the walk.
            let Ok(kind) = entry.file_type() else {
                continue;
            };
            let path = entry.path();
            let is_dir = if kind.is_symlink() {
                path.is_dir()
            } else {
                kind.is_dir()
            };
            if is_dir {
                stack.push(path);
            } else if path.extension().is_some_and(|e| e == "yaml" || e == "yml") {
                files.push(path);
            }
        }
    }
    files.sort();
    files
}

/// Load one JSON submission document (a party statement or its ixit
/// topology), schema-validated like every artifact.
fn load_json_document<T: serde::de::DeserializeOwned>(
    path: &Path,
    validator: &jsonschema::Validator,
) -> Result<T, LoadError> {
    let text = std::fs::read_to_string(path).map_err(|source| LoadError::Io {
        path: path.to_owned(),
        source,
    })?;
    let value: serde_json::Value = serde_json::from_str(&text).map_err(|e| LoadError::Model {
        path: path.to_owned(),
        message: format!("JSON: {e}"),
    })?;
    crate::load::validate_against(validator, &value, path)?;
    serde_json::from_value(value).map_err(|e| LoadError::Model {
        path: path.to_owned(),
        message: e.to_string(),
    })
}

/// Load one `kind: performance` case (its own schema family; the typed
/// model + invariants are the validation).
fn load_performance_case(path: &Path) -> Result<crate::perf::PerformanceCase, LoadError> {
    let value = crate::load::yaml_file_to_value(path)?;
    let case: crate::perf::PerformanceCase =
        serde_json::from_value(value).map_err(|e| LoadError::Model {
            path: path.to_owned(),
            message: e.to_string(),
        })?;
    case.check_invariants()
        .map_err(|message| LoadError::Model {
            path: path.to_owned(),
            message,
        })?;
    Ok(case)
}

/// The ten artifact-family JSON Schemas, compiled once per process.
///
/// The schemas are compile-time constants built by [`crate::schema`], so
/// compiling them on every [`load_root`] call was pure repeated work — and
/// `load_root` is the dominant per-invocation cost of the seeded-defect
/// battery, which calls it once per defect.
struct Schemas {
    /// `case-core.schema.json`.
    case: &'static jsonschema::Validator,
    /// `operation-binding.schema.json`.
    binding: &'static jsonschema::Validator,
    /// `outcomes.schema.json`.
    outcomes: &'static jsonschema::Validator,
    /// `selectors.schema.json`.
    selectors: &'static jsonschema::Validator,
    /// `capability-matrix.schema.json`.
    matrix: &'static jsonschema::Validator,
    /// `corpus-manifest.schema.json`.
    corpus: &'static jsonschema::Validator,
    /// `ambiguity-register.schema.json`.
    register: &'static jsonschema::Validator,
    /// `journey-catalogue.schema.json`.
    journeys: &'static jsonschema::Validator,
    /// `wire-surface.schema.json`.
    wire_surface: &'static jsonschema::Validator,
    /// `statement.schema.json`.
    statement: &'static jsonschema::Validator,
    /// `ixit.schema.json`.
    ixit: &'static jsonschema::Validator,
}

/// The process-wide compiled schema set.
///
/// A compilation failure is a defect in [`crate::schema`] itself, so it is
/// stored as the (name, message) pair [`LoadError::Schema`] is rebuilt from —
/// `LoadError` is not `Clone` (it carries `std::io::Error`), and the error is
/// deterministic, so re-raising it per call is exactly equivalent to the
/// previous per-call compile.
static SCHEMAS: LazyLock<Result<Schemas, (PathBuf, String)>> = LazyLock::new(|| {
    fn one(
        schema: &serde_json::Value,
        name: &'static str,
    ) -> Result<&'static jsonschema::Validator, (PathBuf, String)> {
        match compile_schema(schema, name) {
            Ok(v) => Ok(Box::leak(Box::new(v))),
            Err(LoadError::Schema { path, message }) => Err((path, message)),
            Err(other) => Err((PathBuf::from(name), other.to_string())),
        }
    }
    Ok(Schemas {
        case: one(&schema::case_core_schema(), "case-core.schema.json")?,
        binding: one(
            &schema::operation_binding_schema(),
            "operation-binding.schema.json",
        )?,
        outcomes: one(&schema::outcomes_schema(), "outcomes.schema.json")?,
        selectors: one(&schema::selectors_schema(), "selectors.schema.json")?,
        matrix: one(
            &schema::capability_matrix_schema(),
            "capability-matrix.schema.json",
        )?,
        corpus: one(
            &schema::corpus_manifest_schema(),
            "corpus-manifest.schema.json",
        )?,
        register: one(
            &schema::ambiguity_register_schema(),
            "ambiguity-register.schema.json",
        )?,
        journeys: one(
            &schema::journey_catalogue_schema(),
            "journey-catalogue.schema.json",
        )?,
        wire_surface: one(&schema::wire_surface_schema(), "wire-surface.schema.json")?,
        statement: one(&schema::statement_schema(), "statement.schema.json")?,
        ixit: one(&schema::ixit_schema(), "ixit.schema.json")?,
    })
});

/// Borrow the process-wide compiled schemas, re-raising a compilation defect.
fn compiled_schemas() -> Result<&'static Schemas, LoadError> {
    SCHEMAS
        .as_ref()
        .map_err(|(path, message)| LoadError::Schema {
            path: path.clone(),
            message: message.clone(),
        })
}

/// Load every artifact under `root`.
///
/// # Errors
/// Only on a schema-compilation defect in [`crate::schema`] itself — a bug
/// in this crate, not in the artifact tree. Tree problems come back as
/// [`Loaded::errors`].
#[expect(
    clippy::too_many_lines,
    reason = "one singleton-loading block per artifact family"
)]
pub fn load_root(root: &Path) -> Result<Loaded, LoadError> {
    let schemas = compiled_schemas()?;
    let Schemas {
        case: case_schema,
        binding: binding_schema,
        outcomes: outcomes_schema,
        selectors: selectors_schema,
        matrix: matrix_schema,
        corpus: corpus_schema,
        register: register_schema,
        journeys: journeys_schema,
        wire_surface: wire_surface_schema,
        statement: _,
        ixit: _,
    } = schemas;

    let mut loaded = Loaded::default();

    let performance_dir = root.join("schedule/performance");
    for path in yaml_files_under(&root.join("schedule")) {
        if path.starts_with(&performance_dir) {
            match load_performance_case(&path) {
                Ok(case) => loaded.set.performance.push((path, case)),
                Err(e) => loaded.errors.push(e),
            }
            continue;
        }
        match load_artifact::<CaseCore>(&path, case_schema) {
            Ok(case) => loaded.set.cases.push((path, case)),
            Err(e) => loaded.errors.push(e),
        }
    }
    for path in yaml_files_under(&root.join("bindings")) {
        match load_artifact::<OperationBinding>(&path, binding_schema) {
            Ok(binding) => loaded.set.bindings.push((path, binding)),
            Err(e) => loaded.errors.push(e),
        }
    }

    let mut singleton = |rel: &str, out: &mut dyn FnMut(PathBuf, &Path) -> Option<LoadError>| {
        let path = root.join(rel);
        if path.exists()
            && let Some(e) = out(path.clone(), &path)
        {
            loaded.errors.push(e);
        }
    };

    singleton(
        "vocab/outcomes.yaml",
        &mut |path, p| match load_artifact::<OutcomesVocab>(p, outcomes_schema) {
            Ok(v) => {
                loaded.set.outcomes = Some((path, v));
                None
            }
            Err(e) => Some(e),
        },
    );
    singleton(
        "vocab/selectors.yaml",
        &mut |path, p| match load_artifact::<SelectorsVocab>(p, selectors_schema) {
            Ok(v) => {
                loaded.set.selectors = Some((path, v));
                None
            }
            Err(e) => Some(e),
        },
    );
    singleton(
        "vocab/capability_matrix.yaml",
        &mut |path, p| match load_artifact::<CapabilityMatrix>(p, matrix_schema) {
            Ok(v) => {
                loaded.set.matrix = Some((path, v));
                None
            }
            Err(e) => Some(e),
        },
    );
    singleton(
        "vocab/journey_catalogue.yaml",
        &mut |path, p| match load_artifact::<crate::perf::JourneyCatalogue>(p, journeys_schema) {
            Ok(v) => {
                loaded.set.journeys = Some((path, v));
                None
            }
            Err(e) => Some(e),
        },
    );
    singleton(
        "corpus/MANIFEST.yaml",
        &mut |path, p| match load_artifact::<CorpusManifest>(p, corpus_schema) {
            Ok(v) => {
                loaded.set.corpus_dir = path.parent().map(Path::to_owned);
                loaded.set.corpus = Some((path, v));
                None
            }
            Err(e) => Some(e),
        },
    );
    singleton(
        "registers/ambiguities.yaml",
        &mut |path, p| match load_artifact::<AmbiguityRegister>(p, register_schema) {
            Ok(v) => {
                loaded.set.register = Some((path, v));
                None
            }
            Err(e) => Some(e),
        },
    );
    singleton(
        "vocab/wire_surface.yaml",
        &mut |path, p| match load_artifact::<WireSurface>(p, wire_surface_schema) {
            Ok(v) => {
                loaded.set.wire_surface = Some((path, v));
                None
            }
            Err(e) => Some(e),
        },
    );

    Ok(loaded)
}

#[cfg(test)]
#[expect(
    clippy::panic_in_result_fn,
    reason = "Result-returning tests in the Book ch11 shape, each asserting; \
              clippy offers no allow-in-tests knob for this lint"
)]
mod tests {
    use super::*;

    /// Writes `body` at `root/rel`, creating the directories it needs.
    fn put(root: &Path, rel: &str, body: &str) -> std::io::Result<()> {
        let path = root.join(rel);
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(path, body)
    }

    /// A defective artifact is one FINDING against its own file, and the load
    /// carries on: the tree's other files still land, so one broken file
    /// cannot hide the rest of the catalogue from every gate downstream.
    #[test]
    fn a_defective_file_is_one_error_against_itself_and_the_load_continues()
    -> Result<(), Box<dyn std::error::Error>> {
        let tmp = assert_fs::TempDir::new()?;
        let root = tmp.path().join("artifacts");
        std::fs::create_dir_all(&root)?;

        // One sound case, so the load is not empty when the defects land.
        put(
            &root,
            "schedule/ehr/I_EHR_SERVICE.create_ehr-main.yaml",
            "id: I_EHR_SERVICE.create_ehr-main\n\
             kind: functional\n\
             component: EHR\n\
             sm_operation: I_EHR_SERVICE.create_ehr\n\
             test_purpose: t\n\
             description: d\n\
             spec_refs: [\"ITS-REST master02 §EHR\"]\n\
             capabilities: [EhrOperations]\n\
             flow:\n  - { step: 1, call: create_ehr, expect: created }\n",
        )?;
        // A performance case whose `kind` is not `performance`: the typed
        // invariant, not the schema, is what refuses it.
        put(
            &root,
            "schedule/performance/PERF-broken.yaml",
            "id: PERF-broken\nkind: functional\ncomponent: PERFORMANCE\n\
             description: d\ntest_purpose: t\nspec_refs: []\nclass: POC\n\
             corpus: cnf.scale.10k\n\
             workload: { arrival_rate: 2/s, warmup: PT5M, duration: PT1H,\n\
             \x20            journeys: { chart_review: 100% } }\nthresholds: []\n",
        )?;
        // A singleton vocabulary file outside its schema.
        put(&root, "vocab/selectors.yaml", "body_selectors: 7\n")?;
        // A SUPPLIED declaration that is not JSON at all: the same law holds
        // for the document a submitter hands in.
        let declaration = tmp.path().join("submitted/statement.json");
        put(tmp.path(), "submitted/statement.json", "not json\n")?;

        let mut loaded = load_root(&root)?;
        loaded.review_declaration(&declaration)?;
        let failing: Vec<String> = loaded
            .errors
            .iter()
            .map(|e| e.path().display().to_string())
            .collect();
        for expected in [
            "schedule/performance/PERF-broken.yaml",
            "vocab/selectors.yaml",
            "submitted/statement.json",
        ] {
            assert!(
                failing.iter().any(|p| p.ends_with(expected)),
                "{expected} missing from {failing:?}"
            );
        }
        assert_eq!(loaded.set.cases.len(), 1, "{:?}", loaded.errors);
        assert!(loaded.set.performance.is_empty());
        assert!(loaded.set.selectors.is_none());
        assert!(
            loaded.set.parties.is_empty(),
            "a declaration that does not load never enters the set"
        );
        Ok(())
    }

    /// A tree with no declaration supplied carries none: nothing is swept from
    /// a sibling directory, so no gate can read a claim nobody submitted.
    #[test]
    fn no_declaration_is_swept_from_beside_the_root() -> Result<(), Box<dyn std::error::Error>> {
        let tmp = assert_fs::TempDir::new()?;
        let root = tmp.path().join("artifacts");
        std::fs::create_dir_all(&root)?;
        put(
            tmp.path(),
            "party/acme/statement.json",
            "{\"product\":{\"name\":\"x\",\"version\":\"1\",\"vendor\":\"v\",\
             \"identifier\":\"urn:x\"},\"schedule_release\":\"r\",\"claims\":{}}\n",
        )?;

        let loaded = load_root(&root)?;
        assert!(loaded.set.parties.is_empty(), "{:?}", loaded.set.parties);
        assert!(loaded.set.party_ixits.is_empty());
        assert!(loaded.errors.is_empty(), "{:?}", loaded.errors);
        Ok(())
    }

    /// A root with no artifacts at all loads clean and empty: an absent
    /// singleton is not an error, so a partial tree still reaches the gates
    /// that judge what it does carry.
    #[test]
    fn an_empty_root_loads_clean_and_empty() -> Result<(), Box<dyn std::error::Error>> {
        let tmp = assert_fs::TempDir::new()?;
        let loaded = load_root(tmp.path())?;
        assert!(loaded.errors.is_empty(), "{:?}", loaded.errors);
        assert!(loaded.set.cases.is_empty());
        assert!(loaded.set.bindings.is_empty());
        assert!(loaded.set.matrix.is_none());
        assert!(loaded.set.corpus_dir.is_none());
        Ok(())
    }
}
