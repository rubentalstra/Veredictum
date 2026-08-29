// SPDX-FileCopyrightText: Veredictum contributors
// SPDX-License-Identifier: Apache-2.0

//! Reference resolution.
//!
//! Turns the closed `${…}` grammar into concrete
//! values at execution time: corpus data sets (via the manifest), named
//! views, registered recipes, matrix/fixture row bindings, captures, and
//! the fixed temporal expressions.

#![expect(
    clippy::disallowed_types,
    reason = "dev/verification tooling over JSON artifacts (the catalogue, results, wire \
              exchanges), whose shapes belong to the artifacts and the SUT"
)]

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use serde_json::Value;

use crate::exec::recipes::{self, BoundRow};
use crate::exec::state::{Captured, VarStore};
use crate::ids::{CorpusKey, RecipeName, ViewName};
use crate::model::case::{CaseCore, FixtureEntry, MatrixCell};
use crate::model::corpus::CorpusManifest;
use crate::model::value::TemplatedValue;
use crate::refgrammar::{FixtureField, Segment, Template, ValueRef};

/// Resolution error — an interpreter defect (the artifacts referenced
/// something unresolvable), never a conformance outcome.
#[derive(Debug, thiserror::Error)]
pub enum ResolveError {
    /// A referenced corpus key is absent from the manifest.
    #[error("corpus key {0} is not in the manifest")]
    UnknownCorpusKey(CorpusKey),
    /// A manifest-listed corpus payload could not be loaded or parsed.
    #[error("corpus key {key}: {message}")]
    Corpus {
        /// The corpus key that failed.
        key: CorpusKey,
        /// What went wrong.
        message: String,
    },
    /// A named projection over a corpus data set failed.
    #[error("view {view} on {key}: {message}")]
    View {
        /// The corpus key the view projects over.
        key: CorpusKey,
        /// The view that failed.
        view: ViewName,
        /// What went wrong.
        message: String,
    },
    /// A referenced corpus recipe is not registered in the runner.
    #[error("recipe {0} is not registered")]
    UnknownRecipe(RecipeName),
    /// A registered recipe failed while generating its rows.
    #[error("{0}")]
    Recipe(#[from] recipes::RecipeError),
    /// A `${row.…}` reference addressed a column the current row lacks.
    #[error("row reference {0} outside the current row")]
    Row(String),
    /// A `${ixit:…}` fact the party's ixit does not declare. Never guessed:
    /// the run records the case not-applicable with this citation.
    #[error("the ixit declares no {0}")]
    Ixit(&'static str),
    /// A `${…}` variable reference could not be resolved from the case state.
    #[error("{0}")]
    Vars(String),
}

/// The resolution context for one case×row.
pub struct Resolver<'a> {
    manifest: &'a CorpusManifest,
    corpus_dir: &'a Path,
    /// The party's declared SUT facts — the only source of a `${ixit:…}`
    /// reference. `None` in the pure-artifact contexts that resolve no
    /// party facts.
    ixit: Option<&'a crate::ixit::Ixit>,
    /// Cache of loaded corpus payloads.
    cache: BTreeMap<CorpusKey, Value>,
    /// The current matrix row (when the case has one).
    row: Option<(Vec<String>, Vec<MatrixCell>)>,
    /// The current fixture entry (when the case iterates fixtures).
    fixture: Option<FixtureEntry>,
    /// The case's `rm_class` (content cases: the generated instance type).
    rm_class: Option<String>,
    /// The bound case id (scopes the deterministic-id recipes).
    case_id: String,
    /// The bound case's constraint template id (content carrier stamping):
    /// the manifest `template_id` of the first `requires.templates` key.
    content_template_id: Option<String>,
    row_index: usize,
}

impl std::fmt::Debug for Resolver<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Resolver")
            .field("row_index", &self.row_index)
            .finish_non_exhaustive()
    }
}

impl<'a> Resolver<'a> {
    /// A resolver rooted at the corpus manifest + its directory, and the
    /// party ixit whose declared facts `${ixit:…}` reads.
    #[must_use]
    pub fn new(
        manifest: &'a CorpusManifest,
        corpus_dir: &'a Path,
        ixit: Option<&'a crate::ixit::Ixit>,
    ) -> Self {
        Self {
            manifest,
            corpus_dir,
            ixit,
            cache: BTreeMap::new(),
            row: None,
            fixture: None,
            rm_class: None,
            case_id: String::new(),
            content_template_id: None,
            row_index: 0,
        }
    }

    /// Bind the resolver to a case row (matrix or fixture iteration).
    pub fn bind_row(&mut self, case: &CaseCore, row: usize) {
        self.case_id = case.id.to_string();
        // A content case whose constraint_context declares constraint-axis
        // columns is committed against a PER-ROW synthesized OPT (issue FerroEHR#228):
        // the carrier stamps the deterministic per-row template id, matching the
        // OPT the driver synthesizes+uploads for this row. Otherwise the carrier
        // uses the single baked template from `requires.templates`.
        self.content_template_id = if case
            .constraint_context
            .as_ref()
            .is_some_and(|ctx| !ctx.constraint_columns.is_empty())
        {
            Some(recipes::synth_template_id(
                &self.case_id,
                row,
                case.parameters
                    .as_ref()
                    .and_then(|p| p.matrix.as_ref())
                    .and_then(|m| m.rows.get(row))
                    .map_or(&[][..], |cells| cells.as_slice()),
            ))
        } else {
            case.requires.templates.first().map(|key| {
                self.manifest
                    .get(key)
                    .and_then(|entry| entry.template_id.clone())
                    .unwrap_or_else(|| key.to_string())
            })
        };
        self.row_index = row;
        self.row = case
            .parameters
            .as_ref()
            .and_then(|p| p.matrix.as_ref())
            .and_then(|m| {
                m.rows
                    .get(row)
                    .map(|cells| (m.columns.clone(), cells.clone()))
            });
        self.fixture = case
            .parameters
            .as_ref()
            .and_then(|p| p.fixture_set.as_ref())
            .and_then(|fs| fs.get(row).cloned());
        self.rm_class.clone_from(&case.rm_class);
    }

    /// The declared corpus format of a manifest key (upload routing).
    #[must_use]
    pub fn corpus_format(&self, key: &CorpusKey) -> Option<crate::vocab::CorpusFormat> {
        self.manifest.get(key).map(|entry| entry.format)
    }

    /// The current row's index.
    #[must_use]
    pub fn row_index(&self) -> usize {
        self.row_index
    }

    /// The current row's cell under a named column (sentinels visible).
    #[must_use]
    pub fn row_cell(&self, column: &str) -> Option<&MatrixCell> {
        let (columns, cells) = self.row.as_ref()?;
        columns
            .iter()
            .position(|c| c == column)
            .and_then(|i| cells.get(i))
    }

    /// Load a corpus payload (JSON parsed for JSON-family formats; text
    /// wrapped as a JSON string for opt-xml/adl2-text/aql-text sources;
    /// generated sets synthesize element 0 unless indexed via views).
    ///
    /// # Errors
    /// [`ResolveError`] when the key is unknown or the source unreadable.
    pub fn data_set(&mut self, key: &CorpusKey) -> Result<Value, ResolveError> {
        if let Some(cached) = self.cache.get(key) {
            return Ok(cached.clone());
        }
        let entry = self
            .manifest
            .get(key)
            .ok_or_else(|| ResolveError::UnknownCorpusKey(key.clone()))?;
        let value = if let Some(source) = &entry.source {
            let path: PathBuf = self.corpus_dir.join(source);
            let text = std::fs::read_to_string(&path).map_err(|e| ResolveError::Corpus {
                key: key.clone(),
                message: format!("{}: {e}", path.display()),
            })?;
            match entry.format {
                crate::vocab::CorpusFormat::CanonicalJson
                | crate::vocab::CorpusFormat::WtFlat
                | crate::vocab::CorpusFormat::WtStructured => {
                    serde_json::from_str(&text).map_err(|e| ResolveError::Corpus {
                        key: key.clone(),
                        message: format!("JSON parse: {e}"),
                    })?
                }
                // NOTE: `raw-json` joins the text formats deliberately — a
                // `Value::String` body is the carrier `crate::exec::driver`
                // writes verbatim, so the bytes reach the SUT unrepaired.
                crate::vocab::CorpusFormat::CanonicalXml
                | crate::vocab::CorpusFormat::OptXml
                | crate::vocab::CorpusFormat::Adl2Text
                | crate::vocab::CorpusFormat::Adl14Text
                | crate::vocab::CorpusFormat::AqlText
                | crate::vocab::CorpusFormat::RawJson => Value::String(text),
            }
        } else if let Some(generated) = &entry.generated_by {
            // Generated sets: the whole set as a JSON array via the recipe.
            let name = generated.recipe.as_str();
            let series: Result<Vec<Value>, recipes::RecipeError> = match name {
                "bp_series" => (0..10).map(recipes::bp_series).collect(),
                "query_bp" => (0..10).map(recipes::query_bp).collect(),
                other => {
                    return Err(ResolveError::Corpus {
                        key: key.clone(),
                        message: format!("generated_by recipe {other} has no registered generator"),
                    });
                }
            };
            Value::Array(series?)
        } else {
            return Err(ResolveError::Corpus {
                key: key.clone(),
                message: "entry has neither source nor generated_by".to_owned(),
            });
        };
        self.cache.insert(key.clone(), value.clone());
        Ok(value)
    }

    /// Evaluate a named view over a data set. The committed views are
    /// declarative; the registered evaluators here implement the exact
    /// projections the manifest declares (each is part of the recipe
    /// contract and listed as a registered exception).
    ///
    /// # Errors
    /// [`ResolveError`] when the view is undeclared or its evaluator is
    /// not registered.
    /// Every view name the evaluator match in [`Self::view`] can answer — the
    /// single list the `corpus-integrity` validate gate cross-checks the
    /// manifest's DECLARED views against, so a declared view without an
    /// evaluator fails at validate time instead of at run time (FerroEHR#971).
    pub const REGISTERED_VIEWS: &'static [&'static str] = &[
        "current_state_code",
        "signature",
        "magnitude_ge_140_by_uid",
        "magnitude_ge_140",
        "systolic_ge_140_uids_asc",
        "all_uids_asc",
        "top3_systolic_desc_uids",
        "folder_composition_pairs",
        "f2_scoped_uids",
        "referenced_uids",
    ];

    /// Resolve a declared corpus view to its selection-spec value.
    ///
    /// # Errors
    /// [`ResolveError`] when the view is undeclared or its evaluator is
    /// not registered.
    pub fn view(&mut self, key: &CorpusKey, view: &ViewName) -> Result<Value, ResolveError> {
        let entry = self
            .manifest
            .get(key)
            .ok_or_else(|| ResolveError::UnknownCorpusKey(key.clone()))?;
        if entry.view(view).is_none() {
            return Err(ResolveError::View {
                key: key.clone(),
                view: view.clone(),
                message: "view not declared in the manifest".to_owned(),
            });
        }
        let data = self.data_set(key)?;
        match view.as_str() {
            // cnf.flat.vitals.minimal_ctx#current_state_code (the official
            // minimal_action FLAT instance's ACTION state leaf)
            "current_state_code" => data
                .get("minimal/minimal:0/ism_transition/current_state|code")
                .cloned()
                .ok_or_else(|| ResolveError::View {
                    key: key.clone(),
                    view: view.clone(),
                    message: "flat key missing".to_owned(),
                }),
            // cnf.security.signed_version#signature
            "signature" => data
                .get("signature")
                .cloned()
                .ok_or_else(|| ResolveError::View {
                    key: key.clone(),
                    view: view.clone(),
                    message: "signature attribute missing".to_owned(),
                }),
            // cnf.set.bp-10#magnitude_ge_140_by_uid and friends resolve at
            // query time against committed uids — the driver substitutes the
            // captured uid list; here we return the SELECTION SPEC the
            // driver evaluates.
            "magnitude_ge_140_by_uid" | "magnitude_ge_140" | "systolic_ge_140_uids_asc" => {
                Ok(serde_json::json!({ "systolic_min": 140, "order": "uid" }))
            }
            // the whole committed set, uid-ascending (bag/order anchors)
            "all_uids_asc" => Ok(serde_json::json!({ "systolic_min": 0, "order": "uid" })),
            // ORDER BY systolic DESC LIMIT 3 — the top of the 100+10k ladder
            "top3_systolic_desc_uids" => {
                Ok(serde_json::json!({ "systolic_min": 0, "order": "systolic_desc", "limit": 3 }))
            }
            // cnf.directory.folder_containment_tree#… — the folder-containment
            // selection specs over the committed set. The (folder, index)
            // topology is the fixture's provenance contract; the driver maps
            // each index to the committed uid it captured (AMB-218/AMB-219).
            "folder_composition_pairs" => Ok(serde_json::json!({
                "select": "pairs",
                "pairs": [
                    ["f11", 3],
                    ["f1", 0], ["f1", 1], ["f1", 2], ["f1", 3],
                    ["f2", 2],
                    ["root", 0], ["root", 1], ["root", 2], ["root", 3],
                ],
            })),
            "f2_scoped_uids" => Ok(serde_json::json!({ "select": "uids", "indices": [2] })),
            "referenced_uids" => {
                Ok(serde_json::json!({ "select": "uids", "indices": [0, 1, 2, 3] }))
            }
            other => Err(ResolveError::View {
                key: key.clone(),
                view: view.clone(),
                message: format!("view {other} has no registered evaluator"),
            }),
        }
    }

    /// Resolve one reference to a JSON value.
    ///
    /// # Errors
    /// [`ResolveError`] on unresolvable references.
    pub fn resolve_ref(&mut self, r: &ValueRef, vars: &VarStore) -> Result<Value, ResolveError> {
        match r {
            ValueRef::Row(column) => {
                let (columns, cells) = self.row.as_ref().ok_or_else(|| {
                    ResolveError::Row(format!("${{row.{column}}} without a bound row"))
                })?;
                let i = columns.iter().position(|c| c == column).ok_or_else(|| {
                    ResolveError::Row(format!("column {column} not in the matrix"))
                })?;
                match cells.get(i) {
                    Some(MatrixCell::Literal(v)) => Ok(v.clone()),
                    // `null` is a FIRST-CLASS cell value (stays in the payload);
                    // `absent` means omit — distinguished by the Map resolver
                    // via `row_cell`, which sees the sentinel itself.
                    Some(MatrixCell::Null | MatrixCell::Absent) | None => Ok(Value::Null),
                    Some(MatrixCell::Provided) => {
                        // `provided` in an id column: deterministic synthesis.
                        Ok(Value::String(recipes::deterministic_ehr_id(
                            &self.case_id,
                            self.row_index,
                        )))
                    }
                }
            }
            ValueRef::Fixture(field) => {
                let fixture = self.fixture.as_ref().ok_or_else(|| {
                    ResolveError::Row("${fixture.*} without a fixture row".to_owned())
                })?;
                Ok(match field {
                    FixtureField::DataSet => Value::String(fixture.data_set.to_string()),
                    FixtureField::Expected => Value::String(fixture.expected.token().to_owned()),
                    FixtureField::Defect => {
                        fixture.defect.clone().map_or(Value::Null, Value::String)
                    }
                })
            }
            ValueRef::FixtureDataSet => {
                let key = self
                    .fixture
                    .as_ref()
                    .map(|f| f.data_set.clone())
                    .ok_or_else(|| {
                        ResolveError::Row("${ds:fixture} without a fixture row".to_owned())
                    })?;
                self.data_set(&key)
            }
            ValueRef::Capture { name, optional } => match vars.get(name) {
                Some(Captured::Scalar(s)) => Ok(Value::String(s.clone())),
                Some(Captured::List(items)) => Ok(Value::Array(
                    items.iter().cloned().map(Value::String).collect(),
                )),
                Some(Captured::Body(v)) => Ok(v.clone()),
                Some(Captured::InstantMs { hi, .. }) => Ok(Value::Number((*hi).into())),
                None if *optional => Ok(Value::Null),
                None => Err(ResolveError::Vars(format!("capture {name} is not bound"))),
            },
            ValueRef::DataSet { key, view: None } => self.data_set(key),
            ValueRef::DataSet {
                key,
                view: Some(view),
            } => self.view(key, view),
            ValueRef::Recipe(name) => match name.as_str() {
                "content_instance" => {
                    let (columns, cells) = self.row.as_ref().ok_or_else(|| {
                        ResolveError::Row(
                            "${recipe:content_instance(row)} without a bound row".to_owned(),
                        )
                    })?;
                    let rm_class = self.rm_class.clone().ok_or_else(|| {
                        ResolveError::Row("content_instance without an rm_class".to_owned())
                    })?;
                    let template_id = self
                        .content_template_id
                        .clone()
                        .unwrap_or_else(|| "cnf.minimal_event".to_owned());
                    Ok(recipes::content_instance(
                        &rm_class,
                        &template_id,
                        columns,
                        cells,
                    ))
                }
                "ehr_status" => {
                    let (columns, cells) = self.row.as_ref().ok_or_else(|| {
                        ResolveError::Row(
                            "${recipe:ehr_status(row)} without a bound row".to_owned(),
                        )
                    })?;
                    let bound = BoundRow { columns, cells };
                    Ok(recipes::ehr_status(&self.case_id, &bound, self.row_index)?
                        .unwrap_or(Value::Null))
                }
                _ => Err(ResolveError::UnknownRecipe(name.clone())),
            },
            ValueRef::Time(expr) => {
                let ms = vars.resolve_time(expr).map_err(ResolveError::Vars)?;
                Ok(Value::String(format_instant_ms(ms)))
            }
            ValueRef::Ixit(field) => self.ixit_fact(*field),
        }
    }

    /// Resolve one `${ixit:<field>}` deployment fact from the PARTY
    /// declaration and nothing else: an undeclared fact is an error here, and
    /// `crate::run` turns it into not-applicable-with-citation at selection
    /// time, so it costs coverage rather than correctness.
    ///
    /// # Errors
    /// [`ResolveError::Ixit`] naming the undeclared field.
    fn ixit_fact(&self, field: crate::refgrammar::IxitField) -> Result<Value, ResolveError> {
        let declared = match field {
            crate::refgrammar::IxitField::SystemId => {
                self.ixit.and_then(|ixit| ixit.system_id.clone())
            }
            crate::refgrammar::IxitField::DumpLocation => {
                self.ixit.and_then(|ixit| ixit.dump_location.clone())
            }
        };
        declared
            .map(Value::String)
            .ok_or(ResolveError::Ixit(field.token()))
    }

    /// Resolve a templated string: a single-reference template yields the
    /// referenced value verbatim; a mixed template renders to a string.
    ///
    /// # Errors
    /// [`ResolveError`] on unresolvable references.
    pub fn resolve_template(
        &mut self,
        template: &Template,
        vars: &VarStore,
    ) -> Result<Value, ResolveError> {
        if let Some(single) = template.as_single_ref() {
            return self.resolve_ref(single, vars);
        }
        let mut out = String::new();
        for segment in template.segments() {
            match segment {
                Segment::Lit(s) => out.push_str(s),
                Segment::Ref(r) => match self.resolve_ref(r, vars)? {
                    Value::String(s) => out.push_str(&s),
                    other => out.push_str(&other.to_string()),
                },
            }
        }
        Ok(Value::String(out))
    }

    /// Whether a payload field is OMITTED for the current row: the value is
    /// a single reference whose resolution is the absent sentinel (row cell
    /// `absent`, an `ehr_status: absent` recipe product, or an unresolved
    /// optional capture).
    fn omits_field(&mut self, value: &TemplatedValue, vars: &VarStore) -> bool {
        let TemplatedValue::Text(t) = value else {
            return false;
        };
        let Some(single) = t.as_single_ref() else {
            return false;
        };
        match single {
            ValueRef::Row(column) => {
                matches!(self.row_cell(column), Some(MatrixCell::Absent) | None)
            }
            ValueRef::Capture {
                name,
                optional: true,
            } => vars.get(name).is_none(),
            ValueRef::Recipe(name) if name.as_str() == "ehr_status" => {
                matches!(self.row_cell("ehr_status"), Some(MatrixCell::Absent))
            }
            ValueRef::Fixture(FixtureField::Defect) => {
                self.fixture.as_ref().is_none_or(|f| f.defect.is_none())
            }
            _ => false,
        }
    }

    /// Resolve a whole `with:` payload tree. `absent`-sentinel row cells and
    /// unresolved-optional references resolve to `Null` and are DROPPED from
    /// object payloads (the normative absent-means-omit rule).
    ///
    /// # Errors
    /// [`ResolveError`] on unresolvable references.
    pub fn resolve_value(
        &mut self,
        value: &TemplatedValue,
        vars: &VarStore,
    ) -> Result<Value, ResolveError> {
        Ok(match value {
            TemplatedValue::Null => Value::Null,
            TemplatedValue::Bool(b) => Value::Bool(*b),
            TemplatedValue::Number(n) => Value::Number(n.clone()),
            TemplatedValue::Text(t) => self.resolve_template(t, vars)?,
            TemplatedValue::Seq(items) => Value::Array(
                items
                    .iter()
                    .map(|v| self.resolve_value(v, vars))
                    .collect::<Result<_, _>>()?,
            ),
            TemplatedValue::Map(entries) => {
                let mut map = serde_json::Map::new();
                for (k, v) in entries {
                    // absent-means-omit: an `absent` row sentinel, an absent
                    // recipe product, or an unresolved optional drops the
                    // field entirely; a `null` cell stays a literal null.
                    if self.omits_field(v, vars) {
                        continue;
                    }
                    let resolved = self.resolve_value(v, vars)?;
                    map.insert(k.clone(), resolved);
                }
                Value::Object(map)
            }
        })
    }
}

/// Milliseconds since the Unix epoch → an ISO 8601 UTC instant with
/// millisecond precision (pure integer arithmetic — no clock, no locale).
#[must_use]
#[expect(
    clippy::integer_division,
    reason = "time decomposition + Hinnant's civil-from-days: both are DEFINED in \
              exact integer (floor) division; a float step would break the identity"
)]
pub fn format_instant_ms(ms: i64) -> String {
    let (secs, millis) = (ms.div_euclid(1000), ms.rem_euclid(1000));
    let days = secs.div_euclid(86_400);
    let tod = secs.rem_euclid(86_400);
    let (hh, mm, ss) = (tod / 3600, (tod % 3600) / 60, tod % 60);
    // civil-from-days (Howard Hinnant's algorithm, integer-only)
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = if m <= 2 { y + 1 } else { y };
    format!("{y:04}-{m:02}-{d:02}T{hh:02}:{mm:02}:{ss:02}.{millis:03}Z")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn manifest() -> CorpusManifest {
        serde_saphyr::from_str(
            "cnf.set.bp-10:\n  generated_by: { recipe: bp_series, digest: \"sha256:x\" }\n  format: canonical-json\n  validity: { verdict: valid }\n  provenance: p\n  views:\n    magnitude_ge_140_by_uid: { select: s }\n",
        )
        .unwrap()
    }

    #[test]
    fn generated_sets_and_instants() {
        let m = manifest();
        let dir = PathBuf::from(".");
        let mut r = Resolver::new(&m, &dir, None);
        let set = r
            .data_set(&CorpusKey::parse("cnf.set.bp-10").unwrap())
            .unwrap();
        assert_eq!(set.as_array().unwrap().len(), 10);

        assert_eq!(format_instant_ms(0), "1970-01-01T00:00:00.000Z");
        assert_eq!(
            format_instant_ms(1_767_225_600_000),
            "2026-01-01T00:00:00.000Z"
        );
        assert_eq!(format_instant_ms(999), "1970-01-01T00:00:00.999Z");
    }

    /// A `raw-json` data set reaches the driver as the file's BYTES (issue
    /// FerroEHR#1725): `canonical-json` round-trips through `serde_json::Value` and
    /// silently repairs the very defects a byte-level negative case exists to
    /// deliver — a repeated member, member ordering, an exotic number lexeme.
    /// The `Value::String` carrier is what makes `driver::send` write the
    /// body verbatim (`body_is_json == false`).
    #[test]
    fn a_raw_json_data_set_is_delivered_byte_for_byte() {
        let defective = "{\n  \"_type\": \"COMPOSITION\",\n  \"name\": {\"value\": \"a\"},\n  \
                         \"name\": {\"value\": \"b\"},\n  \"magnitude\": 1.500\n}\n";
        let dir = assert_fs::TempDir::new().unwrap();
        assert_fs::prelude::FileWriteStr::write_str(
            &assert_fs::prelude::PathChild::child(&dir, "dup.json"),
            defective,
        )
        .unwrap();
        let m: CorpusManifest = serde_saphyr::from_str(
            "cnf.raw.dup_member:\n  source: dup.json\n  format: raw-json\n  \
             validity: { verdict: invalid, defect: \"JSON: repeated member\", \
             spec_ref: \"ITS-REST Resources.md §JSON Format\" }\n  provenance: p\n",
        )
        .unwrap();
        let mut r = Resolver::new(&m, dir.path(), None);
        let resolved = r
            .data_set(&CorpusKey::parse("cnf.raw.dup_member").unwrap())
            .unwrap();
        assert_eq!(resolved, Value::String(defective.to_owned()));

        // The control: the SAME bytes declared `canonical-json` lose the
        // repeated member and the `1.500` lexeme before they can be sent.
        let m: CorpusManifest = serde_saphyr::from_str(
            "cnf.raw.dup_member:\n  source: dup.json\n  format: canonical-json\n  \
             validity: { verdict: invalid, defect: \"JSON: repeated member\", \
             spec_ref: \"ITS-REST Resources.md §JSON Format\" }\n  provenance: p\n",
        )
        .unwrap();
        let mut r = Resolver::new(&m, dir.path(), None);
        let normalised = r
            .data_set(&CorpusKey::parse("cnf.raw.dup_member").unwrap())
            .unwrap();
        assert!(normalised.is_object());
        assert_ne!(normalised.to_string(), defective);
    }

    /// `${ixit:system_id}` reads the PARTY declaration and nothing else: a
    /// party that declares none gets a typed error the run turns into a
    /// not-applicable record, never a guessed identifier.
    #[test]
    fn ixit_system_id_resolves_only_from_the_declaration() {
        let m = manifest();
        let dir = PathBuf::from(".");
        let vars = VarStore::default();
        let reference = ValueRef::parse("ixit:system_id").unwrap();

        let declared: crate::ixit::Ixit = serde_json::from_value(serde_json::json!({
            "instances": { "sut": { "base_url": "http://x", "auth": { "mode": "none" } } },
            "system_id": "ferroehr.local"
        }))
        .unwrap();
        let mut r = Resolver::new(&m, &dir, Some(&declared));
        assert_eq!(
            r.resolve_ref(&reference, &vars).unwrap(),
            Value::String("ferroehr.local".to_owned())
        );

        let bare: crate::ixit::Ixit = serde_json::from_value(serde_json::json!({
            "instances": { "sut": { "base_url": "http://x", "auth": { "mode": "none" } } }
        }))
        .unwrap();
        let mut r = Resolver::new(&m, &dir, Some(&bare));
        assert!(matches!(
            r.resolve_ref(&reference, &vars),
            Err(ResolveError::Ixit("system_id"))
        ));

        let mut r = Resolver::new(&m, &dir, None);
        assert!(matches!(
            r.resolve_ref(&reference, &vars),
            Err(ResolveError::Ixit("system_id"))
        ));
    }

    /// A manifest declaring EVERY registered view over one generated set, so
    /// a view name can be evaluated without a fixture on disk.
    fn every_view_manifest() -> CorpusManifest {
        let mut declared = String::new();
        for view in Resolver::REGISTERED_VIEWS {
            declared.push_str("    ");
            declared.push_str(view);
            declared.push_str(": { select: s }\n");
        }
        serde_saphyr::from_str(&format!(
            "cnf.set.bp-10:\n  generated_by: {{ recipe: bp_series, digest: \"sha256:x\" }}\n  \
             format: canonical-json\n  validity: {{ verdict: valid }}\n  provenance: p\n  \
             views:\n{declared}"
        ))
        .unwrap()
    }

    /// `REGISTERED_VIEWS` is the list the `corpus-integrity` validate gate
    /// checks a manifest's declared views against, so a name on it with no
    /// evaluator arm passes validate and fails at run time instead.
    #[test]
    fn every_registered_view_name_has_an_evaluator() {
        let m = every_view_manifest();
        let dir = PathBuf::from(".");
        let key = CorpusKey::parse("cnf.set.bp-10").unwrap();
        for name in Resolver::REGISTERED_VIEWS {
            let mut r = Resolver::new(&m, &dir, None);
            let view = ViewName::parse(name).unwrap();
            if let Err(e) = r.view(&key, &view) {
                assert!(
                    !e.to_string().contains("no registered evaluator"),
                    "{name} is registered but has no evaluator arm"
                );
            }
        }
    }

    /// The folder-containment views are index-addressed SELECTION SPECS: the
    /// driver maps each index to the uid it captured from the case's own
    /// commit set, so a composition classified by several folders is expected
    /// once per containing folder.
    #[test]
    fn folder_containment_views_are_index_addressed_selection_specs() {
        let m = every_view_manifest();
        let dir = PathBuf::from(".");
        let key = CorpusKey::parse("cnf.set.bp-10").unwrap();
        let mut r = Resolver::new(&m, &dir, None);

        let pairs = r
            .view(&key, &ViewName::parse("folder_composition_pairs").unwrap())
            .unwrap();
        assert_eq!(pairs.get("select").unwrap(), "pairs");
        let rows = pairs.get("pairs").unwrap().as_array().unwrap();
        assert_eq!(rows.len(), 10, "the authored containment pair set");
        let classifying: Vec<&str> = rows
            .iter()
            .filter(|pair| pair.get(1) == Some(&serde_json::json!(2)))
            .filter_map(|pair| pair.get(0).and_then(Value::as_str))
            .collect();
        assert_eq!(
            classifying,
            vec!["f1", "f2", "root"],
            "composition 2 is multiply classified"
        );

        assert_eq!(
            r.view(&key, &ViewName::parse("f2_scoped_uids").unwrap())
                .unwrap(),
            serde_json::json!({ "select": "uids", "indices": [2] })
        );
        assert_eq!(
            r.view(&key, &ViewName::parse("referenced_uids").unwrap())
                .unwrap(),
            serde_json::json!({ "select": "uids", "indices": [0, 1, 2, 3] })
        );
    }

    #[test]
    fn absent_cells_drop_from_payloads() {
        let m = manifest();
        let dir = PathBuf::from(".");
        let mut r = Resolver::new(&m, &dir, None);
        let case: CaseCore = serde_json::from_value(serde_json::json!({
            "id": "I_EHR_SERVICE.create_ehr-x", "kind": "functional", "component": "EHR",
            "sm_operation": "I_EHR_SERVICE.create_ehr",
            "capabilities": ["EhrOperations"], "profiles": ["CORE"],
            "test_purpose": "t", "description": "d",
            "spec_refs": ["CNF platform_test_schedule master06 §create_ehr data sets"],
            "parameters": { "iteration": "reset_per_row",
                "matrix": { "columns": ["ehr_id"], "rows": [["absent"], ["provided"]] } },
            "flow": [ { "step": 1, "call": "create_ehr", "expect": "created" } ]
        }))
        .unwrap();
        let payload: TemplatedValue =
            serde_json::from_value(serde_json::json!({ "ehr_id": "${row.ehr_id}" })).unwrap();
        let vars = VarStore::default();

        r.bind_row(&case, 0); // absent
        let v = r.resolve_value(&payload, &vars).unwrap();
        assert!(v.get("ehr_id").is_none());

        r.bind_row(&case, 1); // provided -> deterministic id
        let v = r.resolve_value(&payload, &vars).unwrap();
        assert!(v.get("ehr_id").unwrap().as_str().unwrap().contains('-'));
    }

    /// A case core carrying one matrix row and one fixture row, so the row-,
    /// fixture- and recipe-bound references all have ground to resolve against.
    fn bound_case() -> CaseCore {
        serde_json::from_value(serde_json::json!({
            "id": "I_EHR_SERVICE.create_ehr-bound", "kind": "functional", "component": "EHR",
            "sm_operation": "I_EHR_SERVICE.create_ehr",
            "capabilities": ["EhrOperations"], "profiles": ["CORE"],
            "test_purpose": "t", "description": "d",
            "spec_refs": ["CNF platform_test_schedule master06 §create_ehr data sets"],
            "requires": { "templates": ["cnf.tpl.baked"] },
            "parameters": { "iteration": "reset_per_row",
                "matrix": {
                    "columns": ["ehr_status", "is_queryable", "is_modifiable"],
                    "rows": [["provided", true, false], ["absent", true, true]]
                } },
            "flow": [ { "step": 1, "call": "create_ehr", "expect": "created" } ]
        }))
        .unwrap()
    }

    /// A manifest carrying the baked template key the bound case requires,
    /// plus the generated set the other tests read.
    fn manifest_with_template() -> CorpusManifest {
        serde_saphyr::from_str(
            "cnf.set.bp-10:\n  generated_by: { recipe: bp_series, digest: \"sha256:x\" }\n  \
             format: canonical-json\n  validity: { verdict: valid }\n  provenance: p\n\
             cnf.tpl.baked:\n  source: baked.opt\n  format: opt-xml\n  \
             template_id: cnf.minimal_event\n  validity: { verdict: valid }\n  provenance: p\n",
        )
        .unwrap()
    }

    /// The corpus format a manifest key declares is what routes an upload, and
    /// a key the manifest does not carry declares nothing.
    #[test]
    fn the_declared_corpus_format_and_row_index_are_readable() {
        let m = manifest_with_template();
        let dir = PathBuf::from(".");
        let mut r = Resolver::new(&m, &dir, None);
        assert_eq!(
            r.corpus_format(&CorpusKey::parse("cnf.tpl.baked").unwrap()),
            Some(crate::vocab::CorpusFormat::OptXml)
        );
        assert_eq!(
            r.corpus_format(&CorpusKey::parse("cnf.absent.key").unwrap()),
            None
        );

        assert_eq!(r.row_index(), 0, "an unbound resolver sits at row 0");
        let case = bound_case();
        r.bind_row(&case, 1);
        assert_eq!(r.row_index(), 1);
        assert_eq!(r.row_cell("ehr_status"), Some(&MatrixCell::Absent));
        assert_eq!(r.row_cell("no_such_column"), None);
        // The debug rendering names the bound row and hides the rest.
        assert!(format!("{r:?}").contains("row_index: 1"), "{r:?}");
    }

    /// A constant-constraint case stamps the manifest `template_id` of its one
    /// baked template; a varying-constraint case (constraint columns declared)
    /// stamps the deterministic PER-ROW synthesized id instead, so the carrier
    /// and the OPT the driver uploads for that row name the same template.
    #[test]
    fn the_carrier_template_id_follows_the_constraint_model() {
        let m = manifest_with_template();
        let dir = PathBuf::from(".");
        let vars = VarStore::default();
        let payload: TemplatedValue =
            serde_json::from_value(serde_json::json!("${recipe:content_instance(row)}")).unwrap();

        let mut baked: CaseCore = serde_json::from_value(serde_json::json!({
            "id": "CONT-DV_TEXT-baked", "kind": "content", "component": "CONTENT",
            "rm_class": "DV_TEXT", "test_purpose": "t", "description": "d", "spec_refs": [],
            "requires": { "templates": ["cnf.tpl.baked"] },
            "parameters": { "iteration": "reset_per_row",
                "matrix": { "columns": ["value"], "rows": [["a"], ["b"]] } },
            "flow": [ { "step": 1, "call": "create_composition", "expect": "created" } ]
        }))
        .unwrap();
        let mut r = Resolver::new(&m, &dir, None);
        r.bind_row(&baked, 0);
        let carrier = r.resolve_value(&payload, &vars).unwrap();
        assert_eq!(
            carrier["archetype_details"]["template_id"]["value"],
            Value::String("cnf.minimal_event".to_owned()),
            "the manifest's own template_id, not the corpus key"
        );

        // A required template the manifest does not carry falls back to the
        // key itself rather than dropping the stamp.
        baked.requires.templates = vec![CorpusKey::parse("cnf.tpl.unlisted").unwrap()];
        let mut r = Resolver::new(&m, &dir, None);
        r.bind_row(&baked, 0);
        let carrier = r.resolve_value(&payload, &vars).unwrap();
        assert_eq!(
            carrier["archetype_details"]["template_id"]["value"],
            Value::String("cnf.tpl.unlisted".to_owned())
        );

        let varying: CaseCore = serde_json::from_value(serde_json::json!({
            "id": "CONT-DV_TEXT-varying", "kind": "content", "component": "CONTENT",
            "rm_class": "DV_TEXT", "test_purpose": "t", "description": "d", "spec_refs": [],
            "constraint_context": {
                "template": "cnf.tpl.baked", "path": "/content[0]",
                "constraint_columns": ["C_STRING.pattern"]
            },
            "parameters": { "iteration": "reset_per_row",
                "matrix": { "columns": ["value", "C_STRING.pattern"],
                            "rows": [["a", "^a$"], ["b", "^b$"]] } },
            "flow": [ { "step": 1, "call": "create_composition", "expect": "created" } ]
        }))
        .unwrap();
        let mut r = Resolver::new(&m, &dir, None);
        r.bind_row(&varying, 1);
        let carrier = r.resolve_value(&payload, &vars).unwrap();
        let stamped = carrier["archetype_details"]["template_id"]["value"]
            .as_str()
            .expect("the carrier stamps a template id")
            .to_owned();
        assert_eq!(
            stamped,
            recipes::synth_template_id(
                "CONT-DV_TEXT-varying",
                1,
                &[
                    MatrixCell::Literal(serde_json::json!("b")),
                    MatrixCell::Literal(serde_json::json!("^b$")),
                ]
            ),
            "the per-row synthesized id, matching the OPT the driver uploads"
        );
    }

    /// Every way a corpus entry can fail to yield a data set is a TYPED
    /// resolution error naming the key: an unknown key, an unreadable source,
    /// a source that is not the JSON its format declares, a generated set with
    /// no registered generator, and an entry declaring neither source nor
    /// recipe. None of them may resolve to an empty payload.
    #[test]
    fn every_unresolvable_corpus_entry_is_a_typed_error() {
        let dir = assert_fs::TempDir::new().unwrap();
        assert_fs::prelude::FileWriteStr::write_str(
            &assert_fs::prelude::PathChild::child(&dir, "broken.json"),
            "{ not json",
        )
        .unwrap();
        let m: CorpusManifest = serde_saphyr::from_str(
            "cnf.missing.file:\n  source: nowhere.json\n  format: canonical-json\n  \
             validity: { verdict: valid }\n  provenance: p\n\
             cnf.broken.json:\n  source: broken.json\n  format: canonical-json\n  \
             validity: { verdict: valid }\n  provenance: p\n\
             cnf.no.generator:\n  generated_by: { recipe: no_such_recipe, digest: \"sha256:x\" }\n  \
             format: canonical-json\n  validity: { verdict: valid }\n  provenance: p\n\
             cnf.empty.entry:\n  format: canonical-json\n  validity: { verdict: valid }\n  \
             provenance: p\n",
        )
        .unwrap();
        let mut r = Resolver::new(&m, dir.path(), None);

        let unknown = CorpusKey::parse("cnf.not.in.manifest").unwrap();
        assert!(matches!(
            r.data_set(&unknown),
            Err(ResolveError::UnknownCorpusKey(key)) if key == unknown
        ));

        for (key, needle) in [
            ("cnf.missing.file", "nowhere.json"),
            ("cnf.broken.json", "JSON parse"),
            ("cnf.no.generator", "has no registered generator"),
            ("cnf.empty.entry", "neither source nor generated_by"),
        ] {
            let failure = r
                .data_set(&CorpusKey::parse(key).unwrap())
                .expect_err("the entry cannot yield a data set");
            let message = failure.to_string();
            assert!(message.contains(key), "{message}");
            assert!(message.contains(needle), "{message}");
        }
    }

    /// A text-format source reaches the driver as a JSON string carrier, and a
    /// loaded set is cached: the second read of the same key never touches the
    /// filesystem again.
    #[test]
    fn a_text_source_is_carried_as_a_string_and_cached() {
        let dir = assert_fs::TempDir::new().unwrap();
        let opt = "<template xmlns=\"http://schemas.openehr.org/v1\"/>";
        assert_fs::prelude::FileWriteStr::write_str(
            &assert_fs::prelude::PathChild::child(&dir, "baked.opt"),
            opt,
        )
        .unwrap();
        let m: CorpusManifest = serde_saphyr::from_str(
            "cnf.tpl.baked:\n  source: baked.opt\n  format: opt-xml\n  \
             validity: { verdict: valid }\n  provenance: p\n",
        )
        .unwrap();
        let mut r = Resolver::new(&m, dir.path(), None);
        let key = CorpusKey::parse("cnf.tpl.baked").unwrap();
        assert_eq!(r.data_set(&key).unwrap(), Value::String(opt.to_owned()));

        // Cached: the same key still resolves after its file is gone.
        std::fs::remove_file(dir.path().join("baked.opt")).unwrap();
        assert_eq!(r.data_set(&key).unwrap(), Value::String(opt.to_owned()));
    }

    /// The AQL-chapter generated set is its own manifest key with its own
    /// digest pin, and a view the manifest does not DECLARE is refused before
    /// any evaluator runs — a case cannot reach a projection the corpus never
    /// promised.
    #[test]
    fn a_generated_set_resolves_and_an_undeclared_view_is_refused() {
        let m: CorpusManifest = serde_saphyr::from_str(
            "cnf.query.bp:\n  generated_by: { recipe: query_bp, digest: \"sha256:x\" }\n  \
             format: canonical-json\n  validity: { verdict: valid }\n  provenance: p\n  \
             views:\n    all_uids_asc: { select: s }\n",
        )
        .unwrap();
        let dir = PathBuf::from(".");
        let mut r = Resolver::new(&m, &dir, None);
        let key = CorpusKey::parse("cnf.query.bp").unwrap();
        let set = r.data_set(&key).unwrap();
        assert_eq!(set.as_array().map(Vec::len), Some(10));

        let declared = ViewName::parse("all_uids_asc").unwrap();
        assert_eq!(
            r.view(&key, &declared).unwrap(),
            serde_json::json!({ "systolic_min": 0, "order": "uid" })
        );

        let undeclared = ViewName::parse("top3_systolic_desc_uids").unwrap();
        let failure = r
            .view(&key, &undeclared)
            .expect_err("the manifest declares no such view");
        assert!(
            failure
                .to_string()
                .contains("view not declared in the manifest"),
            "{failure}"
        );
        assert!(matches!(
            r.view(&CorpusKey::parse("cnf.absent").unwrap(), &declared),
            Err(ResolveError::UnknownCorpusKey(_))
        ));

        // A whole data set reaches a step through `${ds:<key>}` with no view.
        let vars = VarStore::default();
        let whole = r
            .resolve_ref(&ValueRef::parse("ds:cnf.query.bp").unwrap(), &vars)
            .unwrap();
        assert_eq!(whole.as_array().map(Vec::len), Some(10));
    }

    /// A manifest may DECLARE a view the runner has no evaluator for — the
    /// `corpus-integrity` gate catches that at validate time, and at run time
    /// it is a typed failure naming the view rather than an empty projection.
    #[test]
    fn a_declared_view_with_no_evaluator_fails_by_name() {
        let m: CorpusManifest = serde_saphyr::from_str(
            "cnf.set.bp-10:\n  generated_by: { recipe: bp_series, digest: \"sha256:x\" }\n  \
             format: canonical-json\n  validity: { verdict: valid }\n  provenance: p\n  \
             views:\n    invented_projection: { select: s }\n",
        )
        .unwrap();
        let dir = PathBuf::from(".");
        let mut r = Resolver::new(&m, &dir, None);
        let failure = r
            .view(
                &CorpusKey::parse("cnf.set.bp-10").unwrap(),
                &ViewName::parse("invented_projection").unwrap(),
            )
            .expect_err("the runner registers no such evaluator");
        assert!(
            failure
                .to_string()
                .contains("view invented_projection has no registered evaluator"),
            "{failure}"
        );
        assert!(
            !Resolver::REGISTERED_VIEWS.contains(&"invented_projection"),
            "the registered list is what the validate gate checks against"
        );
    }

    /// The two attribute-addressed views read their attribute off the loaded
    /// data set, and a set that does not carry it is a typed view failure —
    /// never a silently absent expectation.
    #[test]
    fn attribute_views_report_the_attribute_they_could_not_find() {
        let dir = assert_fs::TempDir::new().unwrap();
        assert_fs::prelude::FileWriteStr::write_str(
            &assert_fs::prelude::PathChild::child(&dir, "signed.json"),
            r#"{ "signature": "sha256:abc",
                 "minimal/minimal:0/ism_transition/current_state|code": "532" }"#,
        )
        .unwrap();
        assert_fs::prelude::FileWriteStr::write_str(
            &assert_fs::prelude::PathChild::child(&dir, "bare.json"),
            "{}",
        )
        .unwrap();
        let views =
            "  views:\n    signature: { select: s }\n    current_state_code: { select: s }\n";
        let m: CorpusManifest = serde_saphyr::from_str(&format!(
            "cnf.security.signed_version:\n  source: signed.json\n  format: canonical-json\n  \
             validity: {{ verdict: valid }}\n  provenance: p\n{views}\
             cnf.bare:\n  source: bare.json\n  format: canonical-json\n  \
             validity: {{ verdict: valid }}\n  provenance: p\n{views}"
        ))
        .unwrap();
        let mut r = Resolver::new(&m, dir.path(), None);

        let signed = CorpusKey::parse("cnf.security.signed_version").unwrap();
        assert_eq!(
            r.view(&signed, &ViewName::parse("signature").unwrap())
                .unwrap(),
            Value::String("sha256:abc".to_owned())
        );
        assert_eq!(
            r.view(&signed, &ViewName::parse("current_state_code").unwrap())
                .unwrap(),
            Value::String("532".to_owned())
        );

        let bare = CorpusKey::parse("cnf.bare").unwrap();
        for (view, needle) in [
            ("signature", "signature attribute missing"),
            ("current_state_code", "flat key missing"),
        ] {
            let failure = r
                .view(&bare, &ViewName::parse(view).unwrap())
                .expect_err("the set carries no such attribute");
            assert!(failure.to_string().contains(needle), "{failure}");
        }
    }

    /// `${row.…}` is bound to the CURRENT row and nothing else: an unbound
    /// resolver and a column the matrix does not carry are both typed errors,
    /// while `null` stays a first-class literal null in the payload.
    #[test]
    fn row_references_resolve_only_against_the_bound_row() {
        let m = manifest();
        let dir = PathBuf::from(".");
        let vars = VarStore::default();
        let reference = ValueRef::parse("row.ehr_status").unwrap();

        let mut unbound = Resolver::new(&m, &dir, None);
        let failure = unbound
            .resolve_ref(&reference, &vars)
            .expect_err("no row is bound");
        assert!(
            failure.to_string().contains("without a bound row"),
            "{failure}"
        );

        let case = bound_case();
        let mut r = Resolver::new(&m, &dir, None);
        r.bind_row(&case, 0);
        let absent_column = ValueRef::parse("row.no_such_column").unwrap();
        let failure = r
            .resolve_ref(&absent_column, &vars)
            .expect_err("the matrix carries no such column");
        assert!(
            failure
                .to_string()
                .contains("column no_such_column not in the matrix"),
            "{failure}"
        );
        assert_eq!(
            r.resolve_ref(&ValueRef::parse("row.is_modifiable").unwrap(), &vars)
                .unwrap(),
            Value::Bool(false)
        );

        // A `null` CELL is a literal null in the payload, distinct from the
        // `absent` sentinel the map resolver drops.
        let nulled: CaseCore = serde_json::from_value(serde_json::json!({
            "id": "I_EHR_SERVICE.create_ehr-nulled", "kind": "functional", "component": "EHR",
            "sm_operation": "I_EHR_SERVICE.create_ehr",
            "capabilities": ["EhrOperations"], "profiles": ["CORE"],
            "test_purpose": "t", "description": "d",
            "spec_refs": ["CNF platform_test_schedule master06 §create_ehr data sets"],
            "parameters": { "iteration": "reset_per_row",
                "matrix": { "columns": ["subject"], "rows": [[null], ["absent"]] } },
            "flow": [ { "step": 1, "call": "create_ehr", "expect": "created" } ]
        }))
        .unwrap();
        let payload: TemplatedValue =
            serde_json::from_value(serde_json::json!({ "subject": "${row.subject}" })).unwrap();
        r.bind_row(&nulled, 0);
        assert_eq!(
            r.resolve_value(&payload, &vars).unwrap(),
            serde_json::json!({ "subject": null }),
            "a null cell stays a literal null"
        );
        r.bind_row(&nulled, 1);
        assert_eq!(
            r.resolve_value(&payload, &vars).unwrap(),
            serde_json::json!({}),
            "an absent cell drops the field"
        );
    }

    /// A fixture row exposes its three declared fields and its data set; an
    /// unbound fixture is a typed error, and a row declaring no defect
    /// resolves that field to null and drops it from an object payload.
    #[test]
    fn fixture_references_expose_the_row_and_refuse_an_unbound_one() {
        let dir = assert_fs::TempDir::new().unwrap();
        assert_fs::prelude::FileWriteStr::write_str(
            &assert_fs::prelude::PathChild::child(&dir, "invalid.json"),
            r#"{ "_type": "COMPOSITION" }"#,
        )
        .unwrap();
        let m: CorpusManifest = serde_saphyr::from_str(
            "cnf.invalid.one:\n  source: invalid.json\n  format: canonical-json\n  \
             validity: { verdict: invalid, defect: \"missing mandatory language\", \
             spec_ref: \"RM ehr composition.adoc §Attributes\" }\n  provenance: p\n",
        )
        .unwrap();
        let case: CaseCore = serde_json::from_value(serde_json::json!({
            "id": "I_EHR_COMPOSITION.create_composition-fixtures", "kind": "functional",
            "component": "EHR_COMPOSITION",
            "sm_operation": "I_EHR_COMPOSITION.create_composition",
            "capabilities": ["CompositionOperations"], "profiles": ["CORE"],
            "test_purpose": "t", "description": "d", "spec_refs": [],
            "parameters": { "iteration": "reset_per_row", "fixture_set": [
                { "data_set": "cnf.invalid.one", "expected": "validation_failed",
                  "defect": "missing mandatory language" },
                { "data_set": "cnf.invalid.one", "expected": "created" }
            ] },
            "flow": [ { "step": 1, "call": "create_composition", "expect": "${fixture.expected}" } ]
        }))
        .unwrap();
        let vars = VarStore::default();

        let mut unbound = Resolver::new(&m, dir.path(), None);
        for body in ["fixture.data_set", "ds:fixture"] {
            let failure = unbound
                .resolve_ref(&ValueRef::parse(body).unwrap(), &vars)
                .expect_err("no fixture row is bound");
            assert!(
                failure.to_string().contains("without a fixture row"),
                "{failure}"
            );
        }

        let mut r = Resolver::new(&m, dir.path(), None);
        r.bind_row(&case, 0);
        assert_eq!(
            r.resolve_ref(&ValueRef::parse("fixture.data_set").unwrap(), &vars)
                .unwrap(),
            Value::String("cnf.invalid.one".to_owned())
        );
        assert_eq!(
            r.resolve_ref(&ValueRef::parse("fixture.expected").unwrap(), &vars)
                .unwrap(),
            Value::String("validation_failed".to_owned())
        );
        assert_eq!(
            r.resolve_ref(&ValueRef::parse("fixture.defect").unwrap(), &vars)
                .unwrap(),
            Value::String("missing mandatory language".to_owned())
        );
        assert_eq!(
            r.resolve_ref(&ValueRef::parse("ds:fixture").unwrap(), &vars)
                .unwrap(),
            serde_json::json!({ "_type": "COMPOSITION" })
        );

        // The valid twin declares no defect: the field resolves to null and is
        // dropped from an object payload rather than committed as `null`.
        let payload: TemplatedValue =
            serde_json::from_value(serde_json::json!({ "defect": "${fixture.defect}" })).unwrap();
        r.bind_row(&case, 1);
        assert_eq!(
            r.resolve_ref(&ValueRef::parse("fixture.defect").unwrap(), &vars)
                .unwrap(),
            Value::Null
        );
        assert_eq!(
            r.resolve_value(&payload, &vars).unwrap(),
            serde_json::json!({})
        );
    }

    /// Every captured shape resolves to its own JSON form, an OPTIONAL capture
    /// that never bound resolves to null (and drops from an object payload),
    /// and a REQUIRED capture that never bound is a typed error — never an
    /// empty string sent to the server.
    #[test]
    fn captures_resolve_by_shape_and_an_unbound_required_one_is_refused() {
        let m = manifest();
        let dir = PathBuf::from(".");
        let mut r = Resolver::new(&m, &dir, None);
        let mut vars = VarStore::default();
        vars.set(
            crate::ids::CaptureName::parse("uid").unwrap(),
            Captured::Scalar("abc::sys::1".to_owned()),
        );
        vars.set(
            crate::ids::CaptureName::parse("uids").unwrap(),
            Captured::List(vec!["a".to_owned(), "b".to_owned()]),
        );
        vars.set(
            crate::ids::CaptureName::parse("body").unwrap(),
            Captured::Body(serde_json::json!({ "_type": "EHR" })),
        );
        vars.set(
            crate::ids::CaptureName::parse("t1").unwrap(),
            Captured::InstantMs {
                lo: 1_767_225_600_000,
                hi: 1_767_225_600_000,
            },
        );

        let mut resolve = |body: &str| r.resolve_ref(&ValueRef::parse(body).unwrap(), &vars);
        assert_eq!(
            resolve("uid").unwrap(),
            Value::String("abc::sys::1".to_owned())
        );
        assert_eq!(resolve("uids").unwrap(), serde_json::json!(["a", "b"]));
        assert_eq!(
            resolve("body").unwrap(),
            serde_json::json!({ "_type": "EHR" })
        );
        assert_eq!(
            resolve("t1").unwrap(),
            serde_json::json!(1_767_225_600_000_i64),
            "an instant capture resolves to the upper bound of its window"
        );
        assert_eq!(resolve("ghost?").unwrap(), Value::Null);
        let failure = resolve("ghost").expect_err("a required capture never bound");
        assert!(
            failure.to_string().contains("capture ghost is not bound"),
            "{failure}"
        );

        // The fixed temporal rules render as ISO 8601 instants.
        assert_eq!(
            resolve("time:before(t1)").unwrap(),
            Value::String("2025-12-31T23:59:59.999Z".to_owned())
        );
        let unresolvable =
            resolve("time:after(uid)").expect_err("a scalar capture is not a commit instant");
        assert!(matches!(unresolvable, ResolveError::Vars(_)));

        // An unresolved optional drops the field entirely.
        let payload: TemplatedValue =
            serde_json::from_value(serde_json::json!({ "uid": "${ghost?}" })).unwrap();
        assert_eq!(
            r.resolve_value(&payload, &vars).unwrap(),
            serde_json::json!({})
        );
    }

    /// The two registered recipes resolve against the bound row, an
    /// `ehr_status: absent` row resolves to null and drops from the payload,
    /// and a recipe name the runner does not register is a typed error.
    #[test]
    fn recipe_references_resolve_only_the_registered_names() {
        let m = manifest();
        let dir = PathBuf::from(".");
        let vars = VarStore::default();
        let case = bound_case();

        let mut unbound = Resolver::new(&m, &dir, None);
        for body in ["recipe:ehr_status(row)", "recipe:content_instance(row)"] {
            let failure = unbound
                .resolve_ref(&ValueRef::parse(body).unwrap(), &vars)
                .expect_err("no row is bound");
            assert!(
                failure.to_string().contains("without a bound row"),
                "{failure}"
            );
        }

        let mut r = Resolver::new(&m, &dir, None);
        r.bind_row(&case, 0);
        let status = r
            .resolve_ref(&ValueRef::parse("recipe:ehr_status(row)").unwrap(), &vars)
            .unwrap();
        assert_eq!(status["_type"], Value::String("EHR_STATUS".to_owned()));
        assert_eq!(status["is_modifiable"], Value::Bool(false));

        // Row 1 declares `ehr_status: absent`: the recipe product is null, and
        // an object payload drops the member rather than sending `null`.
        r.bind_row(&case, 1);
        assert_eq!(
            r.resolve_ref(&ValueRef::parse("recipe:ehr_status(row)").unwrap(), &vars)
                .unwrap(),
            Value::Null
        );
        let payload: TemplatedValue = serde_json::from_value(serde_json::json!({
            "ehr_status": "${recipe:ehr_status(row)}"
        }))
        .unwrap();
        assert_eq!(
            r.resolve_value(&payload, &vars).unwrap(),
            serde_json::json!({})
        );

        // A content case with no rm_class cannot build an instance.
        let failure = r
            .resolve_ref(
                &ValueRef::parse("recipe:content_instance(row)").unwrap(),
                &vars,
            )
            .expect_err("the bound case declares no rm_class");
        assert!(
            failure.to_string().contains("without an rm_class"),
            "{failure}"
        );

        let unregistered = r
            .resolve_ref(&ValueRef::parse("recipe:no_such(row)").unwrap(), &vars)
            .expect_err("the runner registers no such recipe");
        assert!(matches!(unregistered, ResolveError::UnknownRecipe(_)));
    }

    /// `${ixit:dump_location}` reads the party declaration exactly as
    /// `system_id` does: declared it resolves, undeclared it is the typed
    /// error the run turns into a cited not-applicable record.
    #[test]
    fn the_dump_location_fact_resolves_only_from_the_declaration() {
        let m = manifest();
        let dir = PathBuf::from(".");
        let vars = VarStore::default();
        let reference = ValueRef::parse("ixit:dump_location").unwrap();

        let declared: crate::ixit::Ixit = serde_json::from_value(serde_json::json!({
            "instances": { "sut": { "base_url": "http://x", "auth": { "mode": "none" } } },
            "dump_location": "/var/lib/ehr/dump"
        }))
        .unwrap();
        let mut r = Resolver::new(&m, &dir, Some(&declared));
        assert_eq!(
            r.resolve_ref(&reference, &vars).unwrap(),
            Value::String("/var/lib/ehr/dump".to_owned())
        );

        let mut bare = Resolver::new(&m, &dir, None);
        assert!(matches!(
            bare.resolve_ref(&reference, &vars),
            Err(ResolveError::Ixit("dump_location"))
        ));
    }

    /// A template mixing literals and references RENDERS to a string, while a
    /// single-reference template yields the referenced value verbatim — the
    /// difference between a URL segment and a whole committed payload.
    #[test]
    fn a_mixed_template_renders_and_a_single_reference_stays_typed() {
        let m = manifest();
        let dir = PathBuf::from(".");
        let mut r = Resolver::new(&m, &dir, None);
        let mut vars = VarStore::default();
        vars.set(
            crate::ids::CaptureName::parse("ehr_id").unwrap(),
            Captured::Scalar("e-1".to_owned()),
        );
        vars.set(
            crate::ids::CaptureName::parse("uids").unwrap(),
            Captured::List(vec!["a".to_owned()]),
        );

        let mixed = Template::parse("/ehr/${ehr_id}/composition").unwrap();
        assert_eq!(
            r.resolve_template(&mixed, &vars).unwrap(),
            Value::String("/ehr/e-1/composition".to_owned())
        );
        // A non-string reference inside a mixed template renders as its JSON.
        let with_list = Template::parse("uids=${uids}").unwrap();
        assert_eq!(
            r.resolve_template(&with_list, &vars).unwrap(),
            Value::String(r#"uids=["a"]"#.to_owned())
        );

        let single = Template::parse("${uids}").unwrap();
        assert_eq!(
            r.resolve_template(&single, &vars).unwrap(),
            serde_json::json!(["a"]),
            "a single reference keeps the referenced value's own type"
        );
    }

    /// Every `TemplatedValue` arm resolves to its own JSON, so a payload tree
    /// carrying scalars and lists reaches the wire unchanged.
    #[test]
    fn the_whole_payload_tree_resolves_arm_by_arm() {
        let m = manifest();
        let dir = PathBuf::from(".");
        let mut r = Resolver::new(&m, &dir, None);
        let vars = VarStore::default();
        let payload: TemplatedValue = serde_json::from_value(serde_json::json!({
            "null": null,
            "bool": true,
            "number": 42,
            "text": "plain",
            "list": [1, "two", false, null]
        }))
        .unwrap();
        assert_eq!(
            r.resolve_value(&payload, &vars).unwrap(),
            serde_json::json!({
                "null": null, "bool": true, "number": 42, "text": "plain",
                "list": [1, "two", false, null]
            })
        );
    }
}
