// SPDX-FileCopyrightText: Veredictum contributors
// SPDX-License-Identifier: Apache-2.0

//! The journey template pack.
//!
//! Carries the CKM OPTs + committed example skeletons the journey stages
//! commit against, and the deterministic payload stamping that turns a
//! committed skeleton into one arrival's body.
//!
//! Payload ground rules: the skeletons are committed artifacts (generated
//! once from the SUT's example endpoint and vendored byte-identical), so
//! every SUT receives the same starting bytes. Stamping then mutates the
//! event-context times, the composer name, and the numeric leaves whose
//! permitted range the operational template declares
//! ([`crate::perf_run::jitter`]), deterministically from the arrival's
//! planned instant and index — a stamped value is always inside the
//! constraint the same template imposes on the committed skeleton, so
//! variation never introduces a validation error a conformant server would
//! reject.

#![expect(
    clippy::disallowed_types,
    reason = "dev/verification tooling over JSON artifacts (the catalogue, results, wire \
              exchanges), whose shapes belong to the artifacts and the SUT"
)]

use std::path::Path;

use serde_json::Value;

use crate::model::corpus::CorpusManifest;
use crate::perf::JourneyCatalogue;
use crate::perf_run::jitter::LeafConstraints;

/// The rotating composer pool (staff are modelled as event rates; the
/// composer label is the only per-arrival identity).
const STAFF: [&str; 8] = [
    "Nurse Amara Okafor",
    "Dr. Ingrid Larsen",
    "Nurse Tomas Novak",
    "Dr. Priya Sharma",
    "Nurse Lucia Romero",
    "Dr. Sean Murphy",
    "Nurse Mei Chen",
    "Dr. Kwame Mensah",
];

/// One template of the pack: the OPT (the constraint carrier the seeder
/// uploads) and the committed example skeleton (the payload ground).
#[derive(Debug, Clone)]
pub struct PackTemplate {
    /// The corpus key (`cnf.ckm.vital_signs`).
    pub key: String,
    /// The OPT's `template_id` (the wire identifier).
    pub template_id: String,
    /// The vendored OPT 1.4 XML.
    pub opt_xml: String,
    /// The committed example composition skeleton.
    pub skeleton: Value,
    /// The numeric-leaf ranges read out of `opt_xml`, which bound the
    /// per-arrival jitter `stamped` applies to the skeleton.
    pub constraints: LeafConstraints,
}

/// The Simplified-FLAT payload one journey stage commits.
///
/// Carries the OPT that
/// constrains it, that OPT's `template_id` (the `openehr-template-id`
/// channel — ITS-REST overview `Requests_and_responses` §openehr-template-id)
/// and the committed FLAT body.
#[derive(Debug, Clone)]
pub struct FlatPayload {
    /// The `template_id` the commit names in `openehr-template-id`.
    pub template_id: String,
    /// The operational template XML the seeder uploads first.
    pub opt_xml: String,
    /// The FLAT body committed at each stage arrival.
    pub body: Value,
}

/// The TDD-import stage's payload: the document plus the operational
/// template it names, which the seeder uploads before any window opens.
#[derive(Debug, Clone)]
pub struct TddPayload {
    /// The operational template the document instantiates.
    pub opt_xml: String,
    /// The Template Data Document text the stage imports.
    pub document: String,
}

/// The committed payloads the journey stages that do NOT commit a CKM
/// COMPOSITION carry.
///
/// Every one is a corpus fixture the functional catalogue already adjudicates
/// — the load instrument invents no payload of its own. A field is `Some`
/// exactly when the catalogue names an operation that needs it (see
/// [`crate::perf::PerfOp::aux_payload`]).
#[derive(Debug, Clone, Default)]
pub struct AuxPayloads {
    /// The Simplified-FLAT commit payload, when a stage commits FLAT.
    pub flat: Option<FlatPayload>,
    /// The Template Data Document the TDD-import stage commits, with the
    /// operational template it instantiates.
    pub tdd: Option<TddPayload>,
    /// `PERSON`, first content state (the create body).
    pub person: Option<Value>,
    /// `PERSON`, amended content state (the versioned update body).
    pub person_amended: Option<Value>,
    /// `PARTY_RELATIONSHIP` body, when a stage creates one.
    pub party_relationship: Option<Value>,
}

/// The corpus keys the auxiliary payloads come from.
///
/// Fixed, because they are the payloads the functional batteries already
/// adjudicate; the `journey-envelope` validate gate checks the manifest
/// carries them whenever the catalogue names an operation that needs one.
pub const FLAT_OPT_KEY: &str = "cnf.opt.minimal_action";
/// The FLAT body the FLAT-commit stage posts.
pub const FLAT_BODY_KEY: &str = "cnf.flat.vitals.minimal_ctx";
/// The `PERSON` create body the demographic stage posts.
pub const PERSON_KEY: &str = "cnf.demographic.person.v1";
/// The amended `PERSON` body the demographic update stage puts.
pub const PERSON_AMENDED_KEY: &str = "cnf.demographic.person.v2";
/// The `PARTY_RELATIONSHIP` body the demographic stage creates.
pub const PARTY_RELATIONSHIP_KEY: &str = "cnf.demographic.party_relationship.v1";
/// The TDD stage's operational template.
///
/// `nested.en.v1` is category `433|event|`, so a sustained arrival commits a
/// fresh COMPOSITION each time; a `431|persistent|` template would hold
/// exactly one per EHR (RM ehr master04 §COMPOSITION category) and every
/// arrival after the first would be a conflict the instrument manufactured.
pub const TDD_OPT_KEY: &str = "cnf.opt.nested";
/// The Template Data Document the TDD-import stage sends.
pub const TDD_BODY_KEY: &str = "cnf.messaging.tdd.nested";

/// The loaded pack: every template the journey catalogue names, plus the
/// auxiliary payloads its non-COMPOSITION stages carry.
#[derive(Debug, Clone)]
pub struct JourneyPack {
    /// Every operational template the journey catalogue names.
    pub templates: Vec<PackTemplate>,
    /// The payloads the non-COMPOSITION stages carry.
    pub aux: AuxPayloads,
}

impl JourneyPack {
    /// Load every template the catalogue's stages name: the manifest's OPT
    /// entry (`<key>`) and example entry (`<key>.example`), both resolved
    /// against the corpus directory.
    ///
    /// # Errors
    /// A message naming the missing manifest entry, file, or field.
    pub fn load(
        corpus_dir: &Path,
        manifest: &CorpusManifest,
        catalogue: &JourneyCatalogue,
    ) -> Result<Self, String> {
        let mut keys: Vec<String> = Vec::new();
        for (_, journey) in &catalogue.0 {
            for stage in &journey.stages {
                if let Some(template) = &stage.template
                    && !keys.contains(template)
                {
                    keys.push(template.clone());
                }
            }
        }
        keys.sort();
        let entry = |k: &str| {
            crate::ids::CorpusKey::parse(k)
                .ok()
                .and_then(|parsed| manifest.get(&parsed).cloned())
                .ok_or_else(|| format!("corpus manifest has no entry {k}"))
        };
        let read = |source: Option<&String>, what: &str| {
            let source = source.ok_or_else(|| format!("{what} entry has no source"))?;
            std::fs::read_to_string(corpus_dir.join(source))
                .map_err(|e| format!("cannot read {source}: {e}"))
        };
        let read_json = |key: &str| -> Result<Value, String> {
            let e = entry(key)?;
            serde_json::from_str(&read(e.source.as_ref(), key)?)
                .map_err(|error| format!("corpus fixture {key}: {error}"))
        };
        let mut templates = Vec::with_capacity(keys.len());
        for key in keys {
            let opt_entry = entry(&key)?;
            let example_entry = entry(&format!("{key}.example"))?;
            let template_id = opt_entry
                .template_id
                .clone()
                .ok_or_else(|| format!("manifest entry {key} carries no template_id"))?;
            let opt_xml = read(opt_entry.source.as_ref(), &key)?;
            let skeleton: Value = serde_json::from_str(&read(example_entry.source.as_ref(), &key)?)
                .map_err(|e| format!("example skeleton {key}: {e}"))?;
            let constraints = LeafConstraints::from_opt(&opt_xml)
                .map_err(|e| format!("operational template {key}: {e}"))?;
            templates.push(PackTemplate {
                key,
                template_id,
                opt_xml,
                skeleton,
                constraints,
            });
        }
        if templates.is_empty() {
            return Err("the journey catalogue names no templates".to_owned());
        }

        // The auxiliary payloads: loaded exactly when the catalogue names an
        // operation that carries one, so a party's catalogue never pays for
        // fixtures its journeys do not touch.
        let mut needed: Vec<crate::perf::AuxPayloadKind> = Vec::new();
        for (_, journey) in &catalogue.0 {
            for stage in &journey.stages {
                if let Some(kind) = crate::perf::PerfOp::parse(&stage.op)
                    .ok()
                    .and_then(crate::perf::PerfOp::aux_payload)
                    && !needed.contains(&kind)
                {
                    needed.push(kind);
                }
            }
        }
        let mut aux = AuxPayloads::default();
        for kind in needed {
            match kind {
                crate::perf::AuxPayloadKind::Flat => {
                    let opt_entry = entry(FLAT_OPT_KEY)?;
                    aux.flat = Some(FlatPayload {
                        template_id: opt_entry.template_id.clone().ok_or_else(|| {
                            format!("manifest entry {FLAT_OPT_KEY} carries no template_id")
                        })?,
                        opt_xml: read(opt_entry.source.as_ref(), FLAT_OPT_KEY)?,
                        body: read_json(FLAT_BODY_KEY)?,
                    });
                }
                crate::perf::AuxPayloadKind::Person => {
                    aux.person = Some(read_json(PERSON_KEY)?);
                    aux.person_amended = Some(read_json(PERSON_AMENDED_KEY)?);
                }
                crate::perf::AuxPayloadKind::PartyRelationship => {
                    aux.party_relationship = Some(read_json(PARTY_RELATIONSHIP_KEY)?);
                }
                crate::perf::AuxPayloadKind::Tdd => {
                    let opt_entry = entry(TDD_OPT_KEY)?;
                    let body_entry = entry(TDD_BODY_KEY)?;
                    aux.tdd = Some(TddPayload {
                        opt_xml: read(opt_entry.source.as_ref(), TDD_OPT_KEY)?,
                        document: read(body_entry.source.as_ref(), TDD_BODY_KEY)?,
                    });
                }
            }
        }
        Ok(Self { templates, aux })
    }

    /// Look up a template by corpus key.
    #[must_use]
    pub fn index_of(&self, key: &str) -> Option<usize> {
        self.templates.iter().position(|t| t.key == key)
    }

    /// The template at a schedule-resolved index.
    #[must_use]
    pub fn get(&self, index: usize) -> Option<&PackTemplate> {
        self.templates.get(index)
    }
}

/// The deterministic simulated clock: planned offsets map onto a fixed
/// base date, so renders are byte-identical across SUTs and runs.
#[expect(
    clippy::integer_division,
    reason = "whole hours/minutes/days of the simulated clock: exact integer split, \
              which is what makes the rendered timestamp byte-identical across runs"
)]
pub(crate) fn sim_time(offset_s: u64) -> String {
    let day_s = offset_s % 86_400;
    let (h, m, s) = (day_s / 3600, (day_s % 3600) / 60, day_s % 60);
    let day = 1 + (offset_s / 86_400) % 27;
    format!("2024-06-{day:02}T{h:02}:{m:02}:{s:02}Z")
}

fn staff(arrival: u64) -> &'static str {
    let index = usize::try_from(arrival % 8).unwrap_or(0);
    STAFF.get(index).copied().unwrap_or(STAFF[0])
}

/// Stamp a skeleton for one arrival: event-context times to the planned
/// instant's simulated clock, composer name from the rotating staff pool, and
/// every numeric leaf the operational template gives a readable range redrawn
/// inside it, so a population varies leaf by leaf instead of committing one
/// payload N times.
fn stamped(template: &PackTemplate, offset_s: u64, arrival: u64) -> Value {
    let mut body = template.skeleton.clone();
    template
        .constraints
        .apply(&mut body, &template.key, arrival);
    let time = sim_time(offset_s);
    if let Some(context) = body.get_mut("context") {
        for field in ["start_time", "end_time"] {
            if let Some(Value::String(value)) =
                context.get_mut(field).and_then(|t| t.get_mut("value"))
            {
                value.clone_from(&time);
            }
        }
    }
    if let Some(Value::String(name)) = body.get_mut("composer").and_then(|c| c.get_mut("name")) {
        staff(arrival).clone_into(name);
    }
    body
}

/// A stamped composition body (canonical JSON bytes).
///
/// # Errors
/// A serialization failure message.
pub(crate) fn composition_body(
    template: &PackTemplate,
    offset_s: u64,
    arrival: u64,
) -> Result<Vec<u8>, String> {
    serde_json::to_vec(&stamped(template, offset_s, arrival)).map_err(|e| e.to_string())
}

/// A one-version CONTRIBUTION envelope around a stamped composition (the
/// ITS contribution schema: `ORIGINAL_VERSION` members carrying `data` +
/// `commit_audit` + `lifecycle_state`; RM common §`change_control`).
///
/// # Errors
/// A serialization failure message.
pub(crate) fn contribution_body(
    template: &PackTemplate,
    offset_s: u64,
    arrival: u64,
) -> Result<Vec<u8>, String> {
    let audit = |change: &str, code: &str| {
        serde_json::json!({
            "_type": "AUDIT_DETAILS",
            "system_id": "veredictum",
            "committer": { "_type": "PARTY_IDENTIFIED", "name": staff(arrival) },
            "change_type": { "_type": "DV_CODED_TEXT", "value": change,
                "defining_code": { "_type": "CODE_PHRASE",
                    "terminology_id": { "_type": "TERMINOLOGY_ID", "value": "openehr" },
                    "code_string": code } }
        })
    };
    let envelope = serde_json::json!({
        "_type": "CONTRIBUTION",
        "versions": [{
            "_type": "ORIGINAL_VERSION",
            "lifecycle_state": {
                "_type": "DV_CODED_TEXT",
                "value": "complete",
                "defining_code": { "_type": "CODE_PHRASE",
                    "terminology_id": { "_type": "TERMINOLOGY_ID", "value": "openehr" },
                    "code_string": "532" }
            },
            "commit_audit": audit("creation", "249"),
            "data": stamped(template, offset_s, arrival)
        }],
        "audit": audit("creation", "249")
    });
    serde_json::to_vec(&envelope).map_err(|e| e.to_string())
}

/// A replacement `EHR_STATUS` body: queryable + modifiable, anonymous
/// subject, an `other_details` note stamped with the simulated clock (the
/// admission/discharge status touch).
pub(crate) fn ehr_status_body(offset_s: u64) -> Vec<u8> {
    // An EHR_STATUS is an archetype root, so the ARCHETYPED block is
    // mandatory and the root archetype_node_id equals its archetype_id (RM
    // common locatable.adoc §Invariants Archetyped_valid +
    // §archetype_node_id) — the 2026-07-29 POC window sent a root-less body
    // and the server rightly 422'd every update arrival.
    let body = serde_json::json!({
        "_type": "EHR_STATUS",
        "name": { "_type": "DV_TEXT", "value": "EHR Status" },
        "archetype_node_id": "openEHR-EHR-EHR_STATUS.generic.v1",
        "archetype_details": {
            "_type": "ARCHETYPED",
            "archetype_id": { "_type": "ARCHETYPE_ID",
                               "value": "openEHR-EHR-EHR_STATUS.generic.v1" },
            "rm_version": "1.1.0"
        },
        "subject": { "_type": "PARTY_SELF" },
        "is_queryable": true,
        "is_modifiable": true,
        "other_details": {
            "_type": "ITEM_TREE",
            "name": { "_type": "DV_TEXT", "value": "status" },
            "archetype_node_id": "at0001",
            "items": [{
                "_type": "ELEMENT",
                "name": { "_type": "DV_TEXT", "value": "last ADT touch" },
                "archetype_node_id": "at0002",
                "value": { "_type": "DV_DATE_TIME", "value": sim_time(offset_s) }
            }]
        }
    });
    serde_json::to_vec(&body).unwrap_or_default()
}

/// A directory FOLDER body: the per-episode tree (`episodes` open on
/// admission; a `closed` marker folder appended by the discharge
/// close-out). `archetype_node_id` on EVERY node — RM common
/// `LOCATABLE.Archetype_node_id_valid` (mandatory on each LOCATABLE, the
/// subfolders included).
pub(crate) fn folder_body(closed: bool) -> Vec<u8> {
    let mut folders = vec![serde_json::json!({
        "_type": "FOLDER",
        "archetype_node_id": "openEHR-EHR-FOLDER.generic.v1",
        "name": { "_type": "DV_TEXT", "value": "episodes" }
    })];
    if closed {
        folders.push(serde_json::json!({
            "_type": "FOLDER",
            "archetype_node_id": "openEHR-EHR-FOLDER.generic.v1",
            "name": { "_type": "DV_TEXT", "value": "closed" }
        }));
    }
    let body = serde_json::json!({
        "_type": "FOLDER",
        "name": { "_type": "DV_TEXT", "value": "root" },
        "archetype_node_id": "openEHR-EHR-FOLDER.generic.v1",
        "folders": folders
    });
    serde_json::to_vec(&body).unwrap_or_default()
}

/// A demographic `PERSON` body for one arrival: the committed corpus
/// fixture with its legal-identity name stamped from the arrival index, so
/// successive registrations are distinct records and the render stays
/// deterministic. Nothing structural is touched — the payload's RM validity
/// is the fixture's (RM demographic §PARTY `Identities_valid`).
///
/// # Errors
/// A serialization failure message.
pub(crate) fn person_body(person: &Value, arrival: u64) -> Result<Vec<u8>, String> {
    let mut body = person.clone();
    if let Some(Value::String(name)) = body
        .get_mut("identities")
        .and_then(|i| i.get_mut(0))
        .and_then(|identity| identity.get_mut("details"))
        .and_then(|details| details.get_mut("items"))
        .and_then(|items| items.get_mut(0))
        .and_then(|item| item.get_mut("value"))
        .and_then(|value| value.get_mut("value"))
    {
        *name = format!("{} (registration {arrival})", staff(arrival));
    }
    serde_json::to_vec(&body).map_err(|e| e.to_string())
}

/// A `PARTY_RELATIONSHIP` body: the committed corpus fixture with its
/// `source` pointed at the party this journey instance just registered (RM
/// demographic master02 §Party Relationships — the relationship names the
/// parties it relates).
///
/// # Errors
/// A serialization failure message.
pub(crate) fn party_relationship_body(
    relationship: &Value,
    source_uid: &str,
) -> Result<Vec<u8>, String> {
    let mut body = relationship.clone();
    if let Some(Value::String(id)) = body
        .get_mut("source")
        .and_then(|source| source.get_mut("id"))
        .and_then(|id| id.get_mut("value"))
    {
        source_uid.clone_into(id);
    }
    serde_json::to_vec(&body).map_err(|e| e.to_string())
}

/// The Simplified-FLAT composition body (the committed fixture verbatim —
/// FLAT paths are template-derived, so nothing in it may be stamped).
///
/// # Errors
/// A serialization failure message.
pub(crate) fn flat_body(payload: &FlatPayload) -> Result<Vec<u8>, String> {
    serde_json::to_vec(&payload.body).map_err(|e| e.to_string())
}

/// The `ITEM_TAG` set the tagging journey replaces (ITS-REST TAGS API).
pub(crate) fn tags_body(offset_s: u64) -> Vec<u8> {
    let body = serde_json::json!([
        { "key": "cnf.workflow", "value": "ward-round" },
        { "key": "cnf.touched", "value": sim_time(offset_s) }
    ]);
    serde_json::to_vec(&body).unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// An operational template declaring one temperature range, in the AM 1.4
    /// XML shape the CKM exports use
    /// (`specs/its-xml-schemas/components/AM/Release-1.4/OpenehrProfile.xsd`
    /// §`C_DV_QUANTITY`).
    const CELSIUS_OPT: &str = "<template xmlns=\"http://schemas.openehr.org/v1\" \
         xmlns:xsi=\"http://www.w3.org/2001/XMLSchema-instance\">\
         <children xsi:type=\"C_DV_QUANTITY\"><rm_type_name>DV_QUANTITY</rm_type_name>\
         <list><magnitude><lower_included>true</lower_included>\
         <upper_included>false</upper_included><lower_unbounded>false</lower_unbounded>\
         <upper_unbounded>false</upper_unbounded><lower>0</lower><upper>100</upper>\
         </magnitude><units>Cel</units></list></children></template>";

    fn template() -> PackTemplate {
        PackTemplate {
            key: "cnf.ckm.vital_signs".to_owned(),
            template_id: "Vital signs".to_owned(),
            opt_xml: "<template/>".to_owned(),
            skeleton: serde_json::json!({
                "_type": "COMPOSITION",
                "context": {
                    "_type": "EVENT_CONTEXT",
                    "start_time": { "_type": "DV_DATE_TIME", "value": "2020-01-01T00:00:00Z" }
                },
                "composer": { "_type": "PARTY_IDENTIFIED", "name": "original" }
            }),
            constraints: LeafConstraints::default(),
        }
    }

    /// The same template with one constrained temperature leaf and the ranges
    /// [`CELSIUS_OPT`] declares for it.
    fn measuring_template() -> PackTemplate {
        let mut template = template();
        template.opt_xml = CELSIUS_OPT.to_owned();
        template.constraints = LeafConstraints::from_opt(CELSIUS_OPT).unwrap();
        template.skeleton = serde_json::json!({
            "_type": "COMPOSITION",
            "context": {
                "_type": "EVENT_CONTEXT",
                "start_time": { "_type": "DV_DATE_TIME", "value": "2020-01-01T00:00:00Z" }
            },
            "composer": { "_type": "PARTY_IDENTIFIED", "name": "original" },
            "content": [{
                "_type": "ELEMENT",
                "name": { "_type": "DV_TEXT", "value": "Temperature" },
                "value": { "_type": "DV_QUANTITY", "magnitude": 49.5, "units": "Cel" }
            }]
        });
        template
    }

    #[test]
    fn stamping_is_deterministic_and_touches_only_time_and_composer() {
        let t = template();
        let a = composition_body(&t, 3661, 5).unwrap();
        let b = composition_body(&t, 3661, 5).unwrap();
        assert_eq!(a, b);
        let value: Value = serde_json::from_slice(&a).unwrap();
        assert_eq!(
            value["context"]["start_time"]["value"],
            "2024-06-01T01:01:01Z"
        );
        assert_eq!(value["composer"]["name"], STAFF[5]);
        // A different planned instant yields a different render.
        let c = composition_body(&t, 7200, 5).unwrap();
        assert_ne!(a, c);
    }

    /// One population: the bodies a run commits for arrivals `0..count`.
    fn population(template: &PackTemplate, count: u64) -> Vec<Vec<u8>> {
        (0..count)
            .map(|arrival| composition_body(template, arrival * 60, arrival).unwrap())
            .collect()
    }

    #[test]
    fn the_same_seed_renders_the_same_population_byte_for_byte() {
        let t = measuring_template();
        assert_eq!(population(&t, 64), population(&t, 64));
    }

    #[test]
    fn a_population_no_longer_carries_one_leaf_value_in_every_composition() {
        let t = measuring_template();
        let magnitudes: std::collections::BTreeSet<String> = population(&t, 64)
            .iter()
            .map(|body| {
                let value: Value = serde_json::from_slice(body).unwrap();
                value["content"][0]["value"]["magnitude"].to_string()
            })
            .collect();
        assert!(
            magnitudes.len() > 32,
            "64 compositions carried only {} distinct temperatures",
            magnitudes.len()
        );
    }

    #[test]
    fn every_redrawn_leaf_stays_inside_the_range_the_template_declares() {
        // The bounds are read first-hand out of CELSIUS_OPT above: magnitude
        // in [0, 100), the shape the vendored CKM exports use.
        let t = measuring_template();
        for body in population(&t, 512) {
            let value: Value = serde_json::from_slice(&body).unwrap();
            let magnitude = value["content"][0]["value"]["magnitude"].as_f64().unwrap();
            assert!(
                (0.0..100.0).contains(&magnitude),
                "redrawn magnitude {magnitude} is outside the declared [0, 100)"
            );
            assert_eq!(value["content"][0]["value"]["units"], "Cel");
        }
    }

    #[test]
    fn the_jitter_touches_the_numeric_leaf_and_nothing_else_structural() {
        let t = measuring_template();
        let stamped_body = composition_body(&t, 0, 3).unwrap();
        let value: Value = serde_json::from_slice(&stamped_body).unwrap();
        assert_eq!(value["_type"], "COMPOSITION");
        assert_eq!(value["content"][0]["_type"], "ELEMENT");
        assert_eq!(value["content"][0]["name"]["value"], "Temperature");
        assert_eq!(value["content"][0]["value"]["_type"], "DV_QUANTITY");
        let keys: Vec<&String> = value["content"][0]["value"]
            .as_object()
            .unwrap()
            .keys()
            .collect();
        assert_eq!(keys, vec!["_type", "magnitude", "units"]);
    }

    #[test]
    fn the_contribution_envelope_wraps_one_original_version() {
        let t = template();
        let bytes = contribution_body(&t, 60, 1).unwrap();
        let value: Value = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(value["_type"], "CONTRIBUTION");
        let versions = value["versions"].as_array().unwrap();
        assert_eq!(versions.len(), 1);
        assert_eq!(versions[0]["_type"], "ORIGINAL_VERSION");
        assert_eq!(
            versions[0]["lifecycle_state"]["defining_code"]["code_string"],
            "532"
        );
        assert_eq!(versions[0]["data"]["_type"], "COMPOSITION");
    }

    /// The vendored CKM template pack, the payload ground a measured run
    /// commits.
    fn ckm_dir() -> std::path::PathBuf {
        Path::new(concat!(env!("CARGO_MANIFEST_DIR"), "/../.."))
            .join("artifacts/corpus/templates/ckm")
    }

    /// One vendored template loaded exactly as [`JourneyPack::load`] loads it.
    fn vendored(stem: &str, key: &str, template_id: &str) -> PackTemplate {
        let dir = ckm_dir();
        let opt_xml = std::fs::read_to_string(dir.join(format!("{stem}.opt"))).unwrap();
        let skeleton: Value = serde_json::from_str(
            &std::fs::read_to_string(dir.join(format!("{stem}.example.json"))).unwrap(),
        )
        .unwrap();
        let constraints = LeafConstraints::from_opt(&opt_xml).unwrap();
        PackTemplate {
            key: key.to_owned(),
            template_id: template_id.to_owned(),
            opt_xml,
            skeleton,
            constraints,
        }
    }

    /// Every scalar leaf of a body, keyed by its JSON pointer.
    fn flatten(value: &Value, at: &str, into: &mut std::collections::BTreeMap<String, String>) {
        match value {
            Value::Object(map) => {
                for (key, child) in map {
                    flatten(child, &format!("{at}/{key}"), into);
                }
            }
            Value::Array(items) => {
                for (index, item) in items.iter().enumerate() {
                    flatten(item, &format!("{at}/{index}"), into);
                }
            }
            other => {
                into.insert(at.to_owned(), other.to_string());
            }
        }
    }

    /// Every `DV_QUANTITY` leaf of a body as `(units, magnitude)`.
    fn quantities(value: &Value, into: &mut Vec<(String, f64)>) {
        match value {
            Value::Object(map) => {
                if map.get("_type").and_then(Value::as_str) == Some("DV_QUANTITY")
                    && let (Some(units), Some(magnitude)) = (
                        map.get("units").and_then(Value::as_str),
                        map.get("magnitude").and_then(Value::as_f64),
                    )
                {
                    into.push((units.to_owned(), magnitude));
                }
                for (_, child) in map {
                    quantities(child, into);
                }
            }
            Value::Array(items) => {
                for item in items {
                    quantities(item, into);
                }
            }
            _ => {}
        }
    }

    /// The magnitude ranges `vital-signs.opt` declares, read first-hand out of
    /// its `C_DV_QUANTITY` list entries: `(units, lower, upper,
    /// upper_included)`, each the INTERSECTION where the template declares the
    /// units more than once (`/min` at `[0, 1000)` and `[0, 200]` intersects to
    /// `[0, 200]`).
    const VITAL_SIGNS_RANGES: [(&str, f64, f64, bool); 7] = [
        ("Cel", 0.0, 100.0, false),
        ("mm[Hg]", 0.0, 1000.0, false),
        ("kg/m2", 0.0, 1000.0, false),
        ("cm", 0.0, 1000.0, true),
        ("kg", 0.0, 1000.0, true),
        ("g", 0.0, 1_000_000.0, true),
        ("/min", 0.0, 200.0, true),
    ];

    #[test]
    fn the_vendored_vital_signs_population_reproduces_byte_for_byte() {
        let t = vendored("vital-signs", "cnf.ckm.vital_signs", "Vital signs");
        assert_eq!(population(&t, 48), population(&t, 48));
    }

    #[test]
    fn two_compositions_of_one_vendored_population_differ_in_their_leaves() {
        let t = vendored("vital-signs", "cnf.ckm.vital_signs", "Vital signs");
        let of = |arrival: u64| {
            let body = composition_body(&t, arrival * 60, arrival).unwrap();
            let value: Value = serde_json::from_slice(&body).unwrap();
            let mut found = Vec::new();
            quantities(&value, &mut found);
            found
        };
        let (first, second) = (of(0), of(1));
        assert_eq!(
            first.len(),
            8,
            "the committed skeleton carries 8 quantities"
        );
        assert_ne!(
            first, second,
            "two arrivals of one population carried identical quantity leaves"
        );
    }

    #[test]
    fn every_redrawn_vital_sign_is_inside_the_range_the_vendored_opt_declares() {
        let t = vendored("vital-signs", "cnf.ckm.vital_signs", "Vital signs");
        for arrival in 0..256_u64 {
            let body = composition_body(&t, arrival * 60, arrival).unwrap();
            let value: Value = serde_json::from_slice(&body).unwrap();
            let mut found = Vec::new();
            quantities(&value, &mut found);
            for (units, magnitude) in found {
                let declared = VITAL_SIGNS_RANGES
                    .iter()
                    .find(|(declared_units, _, _, _)| *declared_units == units);
                let Some(&(_, lower, upper, upper_included)) = declared else {
                    panic!("arrival {arrival} carried an undeclared unit {units}");
                };
                let inside = magnitude >= lower
                    && (if upper_included {
                        magnitude <= upper
                    } else {
                        magnitude < upper
                    });
                assert!(
                    inside,
                    "arrival {arrival}: {magnitude} {units} is outside the declared range"
                );
            }
        }
    }

    #[test]
    fn the_jitter_moves_magnitudes_and_leaves_every_other_leaf_alone() {
        let t = vendored("vital-signs", "cnf.ckm.vital_signs", "Vital signs");
        let mut before = std::collections::BTreeMap::new();
        flatten(&t.skeleton, "", &mut before);
        let body = composition_body(&t, 600, 7).unwrap();
        let stamped_value: Value = serde_json::from_slice(&body).unwrap();
        let mut after = std::collections::BTreeMap::new();
        flatten(&stamped_value, "", &mut after);
        assert_eq!(
            before.keys().collect::<Vec<_>>(),
            after.keys().collect::<Vec<_>>(),
            "stamping added or removed a leaf"
        );
        let moved: Vec<&String> = before
            .iter()
            .filter(|(pointer, value)| after.get(*pointer) != Some(*value))
            .map(|(pointer, _)| pointer)
            .collect();
        assert!(!moved.is_empty(), "stamping changed nothing at all");
        for pointer in moved {
            let expected = pointer.ends_with("/magnitude")
                || pointer == "/context/start_time/value"
                || pointer == "/context/end_time/value"
                || pointer == "/composer/name";
            assert!(
                expected,
                "stamping moved {pointer}, which it must not touch"
            );
        }
    }

    #[test]
    fn a_template_that_declares_no_readable_range_keeps_its_committed_leaf() {
        // `generic-lab-test-result.opt` declares `mg/L` with an UNBOUNDED
        // upper, and its other units carry no magnitude interval at all, so
        // the honest answer is to leave that leaf where the example put it.
        let t = vendored(
            "generic-lab-test-result",
            "cnf.ckm.lab_result",
            "Generic lab test result example simple",
        );
        for arrival in 0..16_u64 {
            let body = composition_body(&t, arrival * 60, arrival).unwrap();
            let value: Value = serde_json::from_slice(&body).unwrap();
            let mut found = Vec::new();
            quantities(&value, &mut found);
            assert_eq!(found, vec![("mg/L".to_owned(), 10.0)]);
        }
    }

    #[test]
    fn every_vendored_pack_template_reads_its_constraints() {
        let mut read = 0_usize;
        for entry in std::fs::read_dir(ckm_dir()).unwrap() {
            let path = entry.unwrap().path();
            if path.extension().is_none_or(|e| e != "opt") {
                continue;
            }
            let opt_xml = std::fs::read_to_string(&path).unwrap();
            assert!(
                LeafConstraints::from_opt(&opt_xml).is_ok(),
                "{} is not readable as an operational template",
                path.display()
            );
            read += 1;
        }
        assert!(read >= 16, "the pack shrank to {read} templates");
    }

    #[test]
    fn constructed_bodies_parse_and_carry_their_shape() {
        let status: Value = serde_json::from_slice(&ehr_status_body(0)).unwrap();
        assert_eq!(status["_type"], "EHR_STATUS");
        assert_eq!(status["subject"]["_type"], "PARTY_SELF");
        let folder: Value = serde_json::from_slice(&folder_body(true)).unwrap();
        assert_eq!(folder["folders"].as_array().unwrap().len(), 2);
        // RM common LOCATABLE.Archetype_node_id_valid on every node.
        assert!(folder["archetype_node_id"].is_string());
        for sub in folder["folders"].as_array().unwrap() {
            assert!(
                sub["archetype_node_id"].is_string(),
                "subfolder without archetype_node_id"
            );
        }
        let tags: Value = serde_json::from_slice(&tags_body(0)).unwrap();
        assert_eq!(tags.as_array().unwrap().len(), 2);
        assert!(sim_time(90_061).starts_with("2024-06-02T01:01:01"));
    }
}
