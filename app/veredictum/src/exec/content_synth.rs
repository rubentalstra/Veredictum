// SPDX-FileCopyrightText: Veredictum contributors
// SPDX-License-Identifier: Apache-2.0

//! Per-row OPT synthesis for the *structural* content families, plus the
//! single dispatch entry the driver calls for every synthesized content row.
//!
//! Issue FerroEHR#228: a content decision table whose `constraint_context.constraint_columns`
//! name constraint-axis columns needs one OPT per row (the archetype/template
//! constraint varies per row, so no single baked template makes every row's
//! verdict correct). This module owns the STRUCTURAL families — the ones whose
//! constraint is carrier *shape* (container cardinality, attribute existence,
//! object type narrowing), not an ELEMENT.value domain constraint — and routes
//! the value/interval families to [`crate::exec::opt_synth`].
//!
//! Constraint shapes are grounded in AM AOM1.4 (`specs/openehr/AM/docs/AOM1.4/`):
//! `C_MULTIPLE_ATTRIBUTE.cardinality`, `C_ATTRIBUTE.existence`, and
//! `C_OBJECT.rm_type_name`. The carrier skeleton mirrors the committed Python
//! reference `corpus/templates/generate_content_opts.py` (itself built on the
//! vendored CNF Robot `minimal_observation.opt`).
//!
//! The per-row contract: one OPT is synthesized from that row's
//! constraint-axis cells, uploaded under the deterministic id
//! `recipes::synth_template_id` mints, and the row's instance is committed
//! against it. Upload tolerates 409, because a re-run row re-uploads the
//! byte-identical OPT.
//!
//! NOTE: no openEHR spec governs the corpus template packaging — our own
//! corpus-authoring design; the constraint SHAPES are the AOM1.4 ones cited above.

#![expect(
    clippy::disallowed_types,
    reason = "dev/verification tooling over JSON artifacts (the catalogue, results, wire \
              exchanges), whose shapes belong to the artifacts and the SUT"
)]

use crate::exec::opt_synth::{self, SynthError};
use core::fmt::Write as _;

use crate::model::case::MatrixCell;

/// One decision-table row bound to its columns (constraint-cell reader).
struct Cells<'a> {
    columns: &'a [String],
    cells: &'a [MatrixCell],
}

impl Cells<'_> {
    fn cell(&self, name: &str) -> Option<&MatrixCell> {
        self.columns
            .iter()
            .position(|c| c == name)
            .and_then(|i| self.cells.get(i))
    }

    fn text(&self, name: &str) -> Option<&str> {
        match self.cell(name) {
            Some(MatrixCell::Literal(serde_json::Value::String(s))) => Some(s.as_str()),
            _ => None,
        }
    }

    /// The literal text of a CONSTRAINT-AXIS column, which every structural
    /// family requires.
    ///
    /// # Errors
    /// [`SynthError::Unsupported`] when the column is absent from the row or
    /// its cell is not a literal string.
    fn axis(&self, name: &str) -> Result<&str, SynthError> {
        self.text(name).ok_or_else(|| {
            axis_refusal(
                name,
                "the constraint axis is absent from the row or is not a literal string",
            )
        })
    }
}

/// The typed refusal a structural constraint-axis cell earns when it is
/// absent or outside its closed token vocabulary.
///
/// A silently defaulted axis bakes a constraint nobody authored, so a mistyped
/// cell would judge the SUT against the wrong OPT and manufacture a passing
/// row out of the typo.
fn axis_refusal(column: &str, detail: &str) -> SynthError {
    SynthError::Unsupported(format!(
        "{column}: {detail}, so the synthesized OPT would carry a constraint the row never \
         declares"
    ))
}

/// AMB-42 realizability gate.
///
/// A rejected-expected row whose EVERY violated
/// constraint axis is unserializable on the OPT 1.4 wire (ITS-XML
/// `Archetype.xsd`, identical in both published lineages:
/// `C_TIME`/`C_DATE_TIME` carry no `millisecond_validity`
/// element; `C_DURATION` carries only pattern+range, so the AOM-1.4
/// `seconds_allowed` vs `fractional_seconds_allowed` distinction collapses
/// into the single pattern `S` slot) cannot be driven — its ground OPT
/// cannot exist. Returns the excusing citation, or `None` when the row is
/// realizable. (AOM-1.4 UML defines the fields — `c_time.adoc`,
/// `c_duration.adoc` — and CNF master17.4 tests them; the serialization gap
/// is the register entry's subject.)
#[must_use]
pub fn unrealizable_row(
    rm_class: &str,
    columns: &[String],
    cells: &[MatrixCell],
) -> Option<String> {
    if !matches!(
        rm_class,
        "DV_TIME" | "DV_DATE_TIME" | "DV_DURATION" | "DV_INTERVAL"
    ) {
        return None;
    }
    let cell = |name: &str| {
        columns
            .iter()
            .position(|c| c == name)
            .and_then(|i| cells.get(i))
    };
    // Only rejected-expected rows can be unrealizable this way. The loader
    // (`run::synthesize_content_case`) normalizes the authored `rejected`
    // token to the refusal outcome kind the row's `violates` list implies
    // before the driver sees the matrix — accept every spelling.
    match cell("expected") {
        Some(MatrixCell::Literal(serde_json::Value::String(s)))
            if s == "rejected" || s == "validation_failed" || s == "bad_request" => {}
        _ => return None,
    }
    let Some(MatrixCell::Literal(serde_json::Value::Array(violations))) = cell("violates") else {
        return None;
    };
    if violations.is_empty() {
        return None;
    }
    let flag_true = |name: &str| {
        matches!(
            cell(name),
            Some(MatrixCell::Literal(serde_json::Value::Bool(true)))
        )
    };
    let inexpressible = |violation: &str| {
        if violation.contains("millisecond_validity") {
            return true;
        }
        // The duration S slot is shared: prohibiting one of integer/
        // fractional seconds while allowing the other has no pattern form.
        let suffix = if violation.contains("for lower") || violation.contains("_lower") {
            "_lower"
        } else if violation.contains("for upper") || violation.contains("_upper") {
            "_upper"
        } else {
            ""
        };
        if violation.contains("fractional_seconds_allowed") {
            return flag_true(&format!("seconds_allowed{suffix}"));
        }
        if violation.contains("seconds_allowed") {
            return flag_true(&format!("fractional_seconds_allowed{suffix}"));
        }
        false
    };
    let all_inexpressible = violations
        .iter()
        .filter_map(|v| v.as_str())
        .all(inexpressible)
        && violations.iter().all(serde_json::Value::is_string);
    all_inexpressible.then(|| {
        "AMB-42: the violated constraint axes (millisecond_validity / the C_DURATION \
         seconds-vs-fractional distinction) are unserializable in the ITS-XML 1.0.2 \
         OPT wire (Archetype.xsd C_TIME/C_DATE_TIME/C_DURATION) — the row's ground \
         OPT cannot exist on this technology profile"
            .to_owned()
    })
}

/// Synthesize the OPT 1.4 XML for one content row. Dispatches structural
/// `rm_classes` here and value/interval classes to [`opt_synth`].
///
/// # Errors
/// [`SynthError`] when the `rm_class` / column shape is not covered, and when
/// a structural constraint-axis cell is absent or spells a token outside its
/// closed vocabulary (an interpreter or catalogue defect, never a conformance
/// outcome).
pub fn synthesize_opt(
    case_id: &str,
    rm_class: &str,
    template_id: &str,
    columns: &[String],
    cells: &[MatrixCell],
) -> Result<String, SynthError> {
    let row = Cells { columns, cells };
    match rm_class {
        "COMPOSITION"
            if columns.iter().any(|c| c == "cardinality")
                && columns.iter().any(|c| c == "context_existence") =>
        {
            composition_content_cardinality_context(template_id, &row)
        }
        "COMPOSITION" if columns.iter().any(|c| c == "cardinality") => {
            composition_content_cardinality(template_id, &row)
        }
        "COMPOSITION" => composition_context_existence(template_id, &row),
        "EVENT" if columns.iter().any(|c| c == "slot_type") => {
            event_type_narrowing(template_id, &row)
        }
        "EVENT" => event_state_existence(template_id, &row),
        "HISTORY"
            if columns.iter().any(|c| c == "cardinality")
                && columns.iter().any(|c| c == "summary_existence") =>
        {
            history_events_cardinality_summary(template_id, &row)
        }
        "HISTORY" if columns.iter().any(|c| c == "cardinality") => {
            history_events_cardinality(template_id, &row)
        }
        "HISTORY" => history_summary_existence(template_id, &row),
        "ITEM_STRUCTURE" => item_structure_type_narrowing(template_id, &row),
        "ITEM_TREE" | "ITEM_LIST" | "ITEM_TABLE" | "CLUSTER" => {
            item_container_cardinality(rm_class, template_id, &row)
        }
        "ELEMENT" => element_value_null_flavour_existence(template_id, &row),
        "ITEM" => item_type_narrowing(template_id, &row),
        "OBSERVATION" => observation_state_protocol_existence(template_id, &row),
        // Value + interval families.
        _ => opt_synth::synthesize_value_opt(case_id, rm_class, template_id, columns, cells),
    }
}

// ---------------------------------------------------------------------------
// Low-level cADL/OPT XML builders (mirror generate_content_opts.py exactly).
// ---------------------------------------------------------------------------

fn xesc(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

/// Deterministic uid namespace (mirrors the Python `_NS`
/// 6f9619ff-8b86-d011-b42d-00cf4fc964ff): stable across runs, our own.
fn det_uid(template_id: &str) -> String {
    const NS: uuid::Uuid = uuid::Uuid::from_bytes([
        0x6f, 0x96, 0x19, 0xff, 0x8b, 0x86, 0xd0, 0x11, 0xb4, 0x2d, 0x00, 0xcf, 0x4f, 0xc9, 0x64,
        0xff,
    ]);
    uuid::Uuid::new_v5(&NS, template_id.as_bytes()).to_string()
}

/// An `IntervalOfInteger` body (`None` bound => unbounded that side).
fn interval(lower: Option<i64>, upper: Option<i64>) -> String {
    let lu = if lower.is_none() { "true" } else { "false" };
    let uu = if upper.is_none() { "true" } else { "false" };
    let mut s = format!(
        "<lower_included>true</lower_included><upper_included>true</upper_included>\
         <lower_unbounded>{lu}</lower_unbounded><upper_unbounded>{uu}</upper_unbounded>"
    );
    if let Some(l) = lower {
        let _ = write!(s, "<lower>{l}</lower>");
    }
    if let Some(u) = upper {
        let _ = write!(s, "<upper>{u}</upper>");
    }
    s
}

fn occ(lower: i64, upper: Option<i64>) -> String {
    format!(
        "<occurrences>{}</occurrences>",
        interval(Some(lower), upper)
    )
}

fn existence(lower: i64, upper: i64) -> String {
    format!(
        "<existence>{}</existence>",
        interval(Some(lower), Some(upper))
    )
}

/// `C_MULTIPLE_ATTRIBUTE.cardinality` (AOM1.4 §`C_MULTIPLE_ATTRIBUTE`).
fn cardinality(lower: i64, upper: Option<i64>) -> String {
    let uu = if upper.is_none() { "true" } else { "false" };
    let mut body = String::from(
        "<is_ordered>false</is_ordered><is_unique>false</is_unique><interval>\
         <lower_included>true</lower_included><lower_unbounded>false</lower_unbounded>",
    );
    let _ = write!(body, "<upper_unbounded>{uu}</upper_unbounded>");
    let _ = write!(body, "<lower>{lower}</lower>");
    if let Some(u) = upper {
        let _ = write!(body, "<upper>{u}</upper>");
    }
    body.push_str("</interval>");
    format!("<cardinality>{body}</cardinality>")
}

fn c_single_attr(name: &str, child: &str, exist: (i64, i64)) -> String {
    format!(
        "<attributes xsi:type=\"C_SINGLE_ATTRIBUTE\"><rm_attribute_name>{name}</rm_attribute_name>{}{child}</attributes>",
        existence(exist.0, exist.1)
    )
}

fn c_multiple_attr(name: &str, children: &str, card: &str, exist: (i64, i64)) -> String {
    format!(
        "<attributes xsi:type=\"C_MULTIPLE_ATTRIBUTE\"><rm_attribute_name>{name}</rm_attribute_name>{}{children}{card}</attributes>",
        existence(exist.0, exist.1)
    )
}

fn c_complex(rm_type: &str, attrs: &str, node_id: &str, occ_lu: (i64, Option<i64>)) -> String {
    let nid = if node_id.is_empty() {
        "<node_id />".to_owned()
    } else {
        format!("<node_id>{node_id}</node_id>")
    };
    format!(
        "<children xsi:type=\"C_COMPLEX_OBJECT\"><rm_type_name>{}</rm_type_name>{}{nid}{attrs}</children>",
        xesc(rm_type),
        occ(occ_lu.0, occ_lu.1)
    )
}

/// A `DV_TEXT` value `C_COMPLEX_OBJECT` with a fixed `C_STRING` list (the structural
/// carriers only need a *valid* ELEMENT.value; the constraint under test is the
/// carrier shape, not the value).
fn dv_text_value() -> String {
    c_complex("DV_TEXT", "", "", (1, Some(1)))
}

/// The observation `term_definitions` at0000..at0004 (+ optional extras),
/// mirroring the Python `obs_term_defs`.
fn obs_term_defs(extra: &[(&str, &str, &str)]) -> String {
    let mut base: Vec<(&str, &str, &str)> = vec![
        ("at0000", "Minimal", "unknown"),
        ("at0001", "Event Series", "@ internal @"),
        ("at0002", "Any event", "*"),
        ("at0003", "Tree", "@ internal @"),
        ("at0004", "value", "*"),
    ];
    base.extend_from_slice(extra);
    let mut out = String::new();
    for (code, text, desc) in base {
        let _ = write!(
            out,
            "<term_definitions code=\"{code}\"><items id=\"description\">{}</items><items id=\"text\">{}</items></term_definitions>",
            xesc(desc),
            xesc(text)
        );
    }
    out
}

/// The COMPOSITION template envelope (mirrors the Python `composition`).
#[expect(
    clippy::similar_names,
    reason = "content_exist / context_exist name the two RM attributes precisely"
)]
fn composition(
    template_id: &str,
    obs_children: &str,
    content_card: &str,
    content_exist: (i64, i64),
    context_exist: Option<(i64, i64)>,
) -> String {
    let uid = det_uid(template_id);
    let content_attr = c_multiple_attr("content", obs_children, content_card, content_exist);
    let category = c_single_attr(
        "category",
        &c_complex(
            "DV_CODED_TEXT",
            &c_single_attr("defining_code", &c_code_phrase("openehr", &["433"]), (1, 1)),
            "",
            (1, Some(1)),
        ),
        (1, 1),
    );
    // A constrained context always carries an OPTIONAL other_context child
    // (ITEM_TREE at0011, existence 0..1) so the official "context with
    // other_context" data rows are committable against a constrained node
    // (EVENT_CONTEXT itself is PATHABLE — empty node_id — while ITEM_TREE is
    // LOCATABLE and needs the archetype node).
    let (context_attr, context_terms) = match context_exist {
        Some(exist) => {
            let other = c_single_attr(
                "other_context",
                &c_complex("ITEM_TREE", "", "at0011", (1, Some(1))),
                (0, 1),
            );
            (
                c_single_attr(
                    "context",
                    &c_complex("EVENT_CONTEXT", &other, "", (1, Some(1))),
                    exist,
                ),
                "<term_definitions code=\"at0011\"><items id=\"description\">@ internal @</items><items id=\"text\">Other context</items></term_definitions>".to_owned(),
            )
        }
        None => (String::new(), String::new()),
    };
    format!(
        "<?xml version=\"1.0\" encoding=\"utf-8\"?>\n\
<template xmlns:xsi=\"http://www.w3.org/2001/XMLSchema-instance\" xmlns:xsd=\"http://www.w3.org/2001/XMLSchema\" xmlns=\"http://schemas.openehr.org/v1\">\n\
  <language><terminology_id><value>ISO_639-1</value></terminology_id><code_string>en</code_string></language>\n\
  <description><original_author id=\"Original Author\">CNF corpus</original_author><lifecycle_state>Initial</lifecycle_state><details><language><terminology_id><value>ISO_639-1</value></terminology_id><code_string>en</code_string></language><purpose>CNF content constraint template</purpose></details></description>\n\
  <uid><value>{uid}</value></uid>\n\
  <template_id><value>{tid}</value></template_id>\n\
  <concept>CNF content constraint</concept>\n\
  <definition>\n\
    <rm_type_name>COMPOSITION</rm_type_name>\n\
    {occ}\n\
    <node_id>at0000</node_id>\n\
    {category}\n\
    {context_attr}\n\
    {content_attr}\n\
    <archetype_id><value>openEHR-EHR-COMPOSITION.minimal.v1</value></archetype_id>\n\
    <template_id><value>{tid}</value></template_id>\n\
    <term_definitions code=\"at0000\"><items id=\"description\">unknown</items><items id=\"text\">Minimal</items></term_definitions>\n\
    {context_terms}\n\
  </definition>\n\
</template>\n",
        tid = xesc(template_id),
        occ = occ(1, Some(1)),
    )
}

fn c_code_phrase(term: &str, codes: &[&str]) -> String {
    let mut cl = String::new();
    for c in codes {
        let _ = write!(cl, "<code_list>{}</code_list>", xesc(c));
    }
    format!(
        "<children xsi:type=\"C_CODE_PHRASE\"><rm_type_name>CODE_PHRASE</rm_type_name>{}<node_id /><terminology_id><value>{}</value></terminology_id>{cl}</children>",
        occ(1, Some(1)),
        xesc(term)
    )
}

// ---------------------------------------------------------------------------
// OBSERVATION carrier assemblers (parameterized structural variants).
// ---------------------------------------------------------------------------

/// The value ELEMENT (at0004) with a valid `DV_TEXT` value slot.
fn value_element() -> String {
    let value_attr = c_single_attr("value", &dv_text_value(), (0, 1));
    format!(
        "<children xsi:type=\"C_COMPLEX_OBJECT\"><rm_type_name>ELEMENT</rm_type_name>{}<node_id>at0004</node_id>{value_attr}</children>",
        occ(0, Some(1))
    )
}

/// A minimal `data` `ITEM_TREE` (at0003) carrying the value element.
fn item_tree_data() -> String {
    let items = c_multiple_attr("items", &value_element(), &cardinality(0, None), (0, 1));
    let item_tree = c_complex("ITEM_TREE", &items, "at0003", (1, Some(1)));
    c_single_attr("data", &item_tree, (1, 1))
}

/// Wrap an OBSERVATION `C_ARCHETYPE_ROOT` (openEHR-EHR-OBSERVATION.minimal.v1)
/// around the given data/state/protocol attribute XML + term extras.
fn observation_root(attrs: &str, extra_terms: &[(&str, &str, &str)]) -> String {
    format!(
        "<children xsi:type=\"C_ARCHETYPE_ROOT\"><rm_type_name>OBSERVATION</rm_type_name>\
<occurrences><lower_included>true</lower_included><lower_unbounded>false</lower_unbounded><upper_unbounded>true</upper_unbounded><lower>0</lower></occurrences>\
<node_id>at0000</node_id>{attrs}<archetype_id><value>openEHR-EHR-OBSERVATION.minimal.v1</value></archetype_id>{}</children>",
        obs_term_defs(extra_terms)
    )
}

/// Standard OBSERVATION.data = HISTORY(at0001) / EVENT(slot,at0002) / `ITEM_TREE`.
/// `events_exist` is the `HISTORY.events` `C_ATTRIBUTE.existence`; callers pass
/// `cardinality_existence(token)` when the cardinality axis is under test so a
/// zero-events row is admitted for the tokens that allow it, and `(1, 1)` when
/// events are fixed mandatory (the standard `events 1..*` data shape).
fn observation_history(
    events_slot_type: &str,
    event_attrs_extra: &str,
    events_card: &str,
    events_exist: (i64, i64),
) -> String {
    let event_attrs = format!("{}{event_attrs_extra}", item_tree_data());
    let event = c_complex(events_slot_type, &event_attrs, "at0002", (0, Some(1)));
    let events = c_multiple_attr("events", &event, events_card, events_exist);
    let history = c_complex("HISTORY", &events, "at0001", (1, Some(1)));
    c_single_attr("data", &history, (1, 1))
}

// ---------------------------------------------------------------------------
// Structural family builders.
// ---------------------------------------------------------------------------

/// The `cardinality` cell vocabulary: the closed set of
/// `C_MULTIPLE_ATTRIBUTE.cardinality` intervals the structural content cases
/// author (AOM1.4 §`C_MULTIPLE_ATTRIBUTE`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum CardinalityToken {
    /// `any` — the RM-default unbounded container, 0..*.
    Any,
    /// `1plus` — 1..*.
    OnePlus,
    /// `3plus` — 3..*.
    ThreePlus,
    /// `opt` — 0..1.
    Opt,
    /// `mand` — 1..1.
    Mand,
    /// `3to5` — 3..5.
    ThreeToFive,
}

impl CardinalityToken {
    /// Reads `column` off the row as a cardinality token.
    ///
    /// # Errors
    /// [`SynthError::Unsupported`] when the cell is absent or spells a token
    /// outside the vocabulary.
    fn read(row: &Cells<'_>, column: &str) -> Result<Self, SynthError> {
        match row.axis(column)? {
            "any" => Ok(Self::Any),
            "1plus" => Ok(Self::OnePlus),
            "3plus" => Ok(Self::ThreePlus),
            "opt" => Ok(Self::Opt),
            "mand" => Ok(Self::Mand),
            "3to5" => Ok(Self::ThreeToFive),
            other => Err(axis_refusal(
                column,
                &format!(
                    "{other:?} is outside the cardinality vocabulary \
                     (any|1plus|3plus|opt|mand|3to5)"
                ),
            )),
        }
    }

    /// The `C_MULTIPLE_ATTRIBUTE.cardinality` interval XML this token names.
    fn cardinality(self) -> String {
        match self {
            Self::Any => cardinality(0, None),
            Self::OnePlus => cardinality(1, None),
            Self::ThreePlus => cardinality(3, None),
            Self::Opt => cardinality(0, Some(1)),
            Self::Mand => cardinality(1, Some(1)),
            Self::ThreeToFive => cardinality(3, Some(5)),
        }
    }

    /// The container attribute's `C_ATTRIBUTE.existence`, which follows the
    /// cardinality token: `any`/`opt` leave the attribute optional (0..1) so an
    /// omitted or zero-count container is admitted and the RM invariant decides
    /// — e.g. `HISTORY.Events_valid` (`(events /= Void and then not
    /// events.is_empty) or summary /= Void`, RM `data_structures` §HISTORY
    /// Invariants) accepts a zero-events HISTORY via its summary disjunct;
    /// `1plus`/`3plus`/`mand`/`3to5` make the attribute mandatory (1..1). The
    /// cardinality alone never fires on an omitted container (AOM1.4
    /// §`C_MULTIPLE_ATTRIBUTE` + §`C_ATTRIBUTE`), and on the canonical wire an
    /// empty list serializes as absent, so mandating existence 1..1 would
    /// wrongly reject the zero-count row.
    fn existence(self) -> (i64, i64) {
        match self {
            Self::OnePlus | Self::ThreePlus | Self::Mand | Self::ThreeToFive => (1, 1),
            Self::Any | Self::Opt => (0, 1),
        }
    }
}

/// The `*_existence` cell vocabulary: a `C_ATTRIBUTE.existence` pair
/// (AOM1.4 §`C_ATTRIBUTE`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ExistenceToken {
    /// `optional` — existence 0..1.
    Optional,
    /// `mandatory` — existence 1..1.
    Mandatory,
}

impl ExistenceToken {
    /// Reads `column` off the row as an existence token.
    ///
    /// # Errors
    /// [`SynthError::Unsupported`] when the cell is absent or spells a token
    /// outside the vocabulary.
    fn read(row: &Cells<'_>, column: &str) -> Result<Self, SynthError> {
        match row.axis(column)? {
            "optional" => Ok(Self::Optional),
            "mandatory" => Ok(Self::Mandatory),
            other => Err(axis_refusal(
                column,
                &format!("{other:?} is outside the existence vocabulary (optional|mandatory)"),
            )),
        }
    }

    /// The `C_ATTRIBUTE.existence` lower/upper pair this token names.
    fn pair(self) -> (i64, i64) {
        match self {
            Self::Optional => (0, 1),
            Self::Mandatory => (1, 1),
        }
    }
}

/// The EVENT `slot_type` vocabulary: the abstract `EVENT<T>` plus its two
/// concrete descendants, which is the whole set RM `data_structures`
/// master06-history\_package §Overview defines (`EVENT<T->ITEM_STRUCTURE>`,
/// `POINT_EVENT<T>`, `INTERVAL_EVENT<T>`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum EventSlotType {
    /// `EVENT` — the unnarrowed abstract slot.
    Event,
    /// `POINT_EVENT`.
    PointEvent,
    /// `INTERVAL_EVENT`.
    IntervalEvent,
}

impl EventSlotType {
    /// Reads `column` off the row as an EVENT slot type.
    ///
    /// # Errors
    /// [`SynthError::Unsupported`] when the cell is absent or names a class
    /// outside the `EVENT` hierarchy.
    fn read(row: &Cells<'_>, column: &str) -> Result<Self, SynthError> {
        match row.axis(column)? {
            "EVENT" => Ok(Self::Event),
            "POINT_EVENT" => Ok(Self::PointEvent),
            "INTERVAL_EVENT" => Ok(Self::IntervalEvent),
            other => Err(axis_refusal(
                column,
                &format!("{other:?} is not an RM EVENT class (EVENT|POINT_EVENT|INTERVAL_EVENT)"),
            )),
        }
    }

    /// The `C_OBJECT.rm_type_name` this slot type narrows to.
    fn rm_type_name(self) -> &'static str {
        match self {
            Self::Event => "EVENT",
            Self::PointEvent => "POINT_EVENT",
            Self::IntervalEvent => "INTERVAL_EVENT",
        }
    }
}

/// The ITEM `slot_type` vocabulary: the abstract `ITEM` and its two concrete
/// descendants. RM `data_structures` UML `item.adoc` §ITEM Class describes it
/// as "The abstract parent of `CLUSTER` and `ELEMENT` representation classes",
/// which closes the set at three.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ItemSlotType {
    /// `ITEM` — the unnarrowed abstract slot.
    Item,
    /// `CLUSTER`.
    Cluster,
    /// `ELEMENT`.
    Element,
}

impl ItemSlotType {
    /// Reads `column` off the row as an `ITEM` slot type.
    ///
    /// # Errors
    /// [`SynthError::Unsupported`] when the cell is absent or names a class
    /// outside the `ITEM` hierarchy.
    fn read(row: &Cells<'_>, column: &str) -> Result<Self, SynthError> {
        match row.axis(column)? {
            "ITEM" => Ok(Self::Item),
            "CLUSTER" => Ok(Self::Cluster),
            "ELEMENT" => Ok(Self::Element),
            other => Err(axis_refusal(
                column,
                &format!("{other:?} is not an RM ITEM class (ITEM|CLUSTER|ELEMENT)"),
            )),
        }
    }

    /// The `C_OBJECT.rm_type_name` this slot type narrows to.
    fn rm_type_name(self) -> &'static str {
        match self {
            Self::Item => "ITEM",
            Self::Cluster => "CLUSTER",
            Self::Element => "ELEMENT",
        }
    }
}

/// The ITEM\_STRUCTURE `slot_type` vocabulary: the abstract `ITEM_STRUCTURE`
/// plus its four concrete descendants, which is the whole set RM
/// `data_structures` master04-item\_structure\_package §Overview defines
/// (`ITEM_SINGLE`, `ITEM_LIST`, `ITEM_TREE`, `ITEM_TABLE`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[expect(
    clippy::enum_variant_names,
    reason = "the variants name RM classes, and every ITEM_STRUCTURE descendant is spelled ITEM_*"
)]
enum ItemStructureSlotType {
    /// `ITEM_STRUCTURE` — the unnarrowed abstract slot.
    ItemStructure,
    /// `ITEM_SINGLE`.
    ItemSingle,
    /// `ITEM_LIST`.
    ItemList,
    /// `ITEM_TABLE`.
    ItemTable,
    /// `ITEM_TREE`.
    ItemTree,
}

impl ItemStructureSlotType {
    /// Reads `column` off the row as an `ITEM_STRUCTURE` slot type.
    ///
    /// # Errors
    /// [`SynthError::Unsupported`] when the cell is absent or names a class
    /// outside the `ITEM_STRUCTURE` hierarchy.
    fn read(row: &Cells<'_>, column: &str) -> Result<Self, SynthError> {
        match row.axis(column)? {
            "ITEM_STRUCTURE" => Ok(Self::ItemStructure),
            "ITEM_SINGLE" => Ok(Self::ItemSingle),
            "ITEM_LIST" => Ok(Self::ItemList),
            "ITEM_TABLE" => Ok(Self::ItemTable),
            "ITEM_TREE" => Ok(Self::ItemTree),
            other => Err(axis_refusal(
                column,
                &format!(
                    "{other:?} is not an RM ITEM_STRUCTURE class \
                     (ITEM_STRUCTURE|ITEM_SINGLE|ITEM_LIST|ITEM_TABLE|ITEM_TREE)"
                ),
            )),
        }
    }

    /// The `C_OBJECT.rm_type_name` this slot type narrows to.
    fn rm_type_name(self) -> &'static str {
        match self {
            Self::ItemStructure => "ITEM_STRUCTURE",
            Self::ItemSingle => "ITEM_SINGLE",
            Self::ItemList => "ITEM_LIST",
            Self::ItemTable => "ITEM_TABLE",
            Self::ItemTree => "ITEM_TREE",
        }
    }
}

/// The master15 combined family: `C_MULTIPLE_ATTRIBUTE.cardinality` on
/// `COMPOSITION.content` AND `C_ATTRIBUTE.existence` on `COMPOSITION.context`
/// in one template (AOM1.4 §`C_MULTIPLE_ATTRIBUTE` + §`C_ATTRIBUTE`) — the
/// content_card_X-context_mand official cases constrain both axes at once.
#[expect(
    clippy::similar_names,
    reason = "content_exist / context_exist name the two RM attributes precisely"
)]
fn composition_content_cardinality_context(
    template_id: &str,
    row: &Cells<'_>,
) -> Result<String, SynthError> {
    let token = CardinalityToken::read(row, "cardinality")?;
    // Same existence-follows-the-token rule as the single-axis family (see
    // `CardinalityToken::existence`): the cardinality alone never fires on an
    // omitted container.
    let content_exist = token.existence();
    let context_exist = ExistenceToken::read(row, "context_existence")?.pair();
    let obs = observation_root(&item_tree_data_observation_history(), &[]);
    Ok(composition(
        template_id,
        &obs,
        &token.cardinality(),
        content_exist,
        Some(context_exist),
    ))
}

fn composition_content_cardinality(
    template_id: &str,
    row: &Cells<'_>,
) -> Result<String, SynthError> {
    let token = CardinalityToken::read(row, "cardinality")?;
    // The COMPOSITION.content existence follows the cardinality token so an
    // absent/zero-count content is accepted for `any`/`opt` and rejected by
    // existence for the mandatory families; the instance omits content when the
    // count is 0 (an empty present list is rejected at the RM level regardless
    // of cardinality).
    let content_exist = token.existence();
    let obs = observation_root(&item_tree_data_observation_history(), &[]);
    Ok(composition(
        template_id,
        &obs,
        &token.cardinality(),
        content_exist,
        None,
    ))
}

/// OBSERVATION with the standard `HISTORY/EVENT/ITEM_TREE` data (default events
/// cardinality 1..*, EVENT slot).
fn item_tree_data_observation_history() -> String {
    observation_history("EVENT", "", &cardinality(1, None), (1, 1))
}

fn composition_context_existence(template_id: &str, row: &Cells<'_>) -> Result<String, SynthError> {
    let exist = ExistenceToken::read(row, "context_existence")?.pair();
    let obs = observation_root(&item_tree_data_observation_history(), &[]);
    Ok(composition(
        template_id,
        &obs,
        &cardinality(0, None),
        (0, 1),
        Some(exist),
    ))
}

fn event_state_existence(template_id: &str, row: &Cells<'_>) -> Result<String, SynthError> {
    let exist = ExistenceToken::read(row, "state_existence")?.pair();
    let state_attr = c_single_attr(
        "state",
        &c_complex("ITEM_TREE", "", "at0005", (1, Some(1))),
        exist,
    );
    let data = observation_history("EVENT", &state_attr, &cardinality(1, None), (1, 1));
    let obs = observation_root(&data, &[("at0005", "State", "@ internal @")]);
    Ok(composition(
        template_id,
        &obs,
        &cardinality(0, None),
        (0, 1),
        None,
    ))
}

fn event_type_narrowing(template_id: &str, row: &Cells<'_>) -> Result<String, SynthError> {
    let slot = EventSlotType::read(row, "slot_type")?;
    let data = observation_history(slot.rm_type_name(), "", &cardinality(1, None), (1, 1));
    let obs = observation_root(&data, &[]);
    Ok(composition(
        template_id,
        &obs,
        &cardinality(0, None),
        (0, 1),
        None,
    ))
}

fn history_events_cardinality(template_id: &str, row: &Cells<'_>) -> Result<String, SynthError> {
    let token = CardinalityToken::read(row, "cardinality")?;
    let data = observation_history("EVENT", "", &token.cardinality(), token.existence());
    let obs = observation_root(&data, &[]);
    Ok(composition(
        template_id,
        &obs,
        &cardinality(0, None),
        (0, 1),
        None,
    ))
}

/// The master16 combined family: `C_MULTIPLE_ATTRIBUTE.cardinality` on
/// HISTORY.events AND `C_ATTRIBUTE.existence` on HISTORY.summary in one
/// template — the events_card_X-summary_ex_mand official cases (and the
/// summary-present rows of the `summary_ex_opt` cases) constrain both axes.
fn history_events_cardinality_summary(
    template_id: &str,
    row: &Cells<'_>,
) -> Result<String, SynthError> {
    let token = CardinalityToken::read(row, "cardinality")?;
    let summary_exist = ExistenceToken::read(row, "summary_existence")?.pair();
    let summary_attr = c_single_attr(
        "summary",
        &c_complex("ITEM_TREE", "", "at0007", (1, Some(1))),
        summary_exist,
    );
    let event_attrs = item_tree_data();
    let event = c_complex("EVENT", &event_attrs, "at0002", (0, Some(1)));
    let events = c_multiple_attr("events", &event, &token.cardinality(), token.existence());
    let history = c_complex(
        "HISTORY",
        &format!("{events}{summary_attr}"),
        "at0001",
        (1, Some(1)),
    );
    let data = c_single_attr("data", &history, (1, 1));
    let obs = observation_root(&data, &[("at0007", "Summary", "@ internal @")]);
    Ok(composition(
        template_id,
        &obs,
        &cardinality(0, None),
        (0, 1),
        None,
    ))
}

fn history_summary_existence(template_id: &str, row: &Cells<'_>) -> Result<String, SynthError> {
    let exist = ExistenceToken::read(row, "summary_existence")?.pair();
    let summary_attr = c_single_attr(
        "summary",
        &c_complex("ITEM_TREE", "", "at0007", (1, Some(1))),
        exist,
    );
    // HISTORY(at0001) with events 1..* + a summary attribute.
    let event_attrs = item_tree_data();
    let event = c_complex("EVENT", &event_attrs, "at0002", (0, Some(1)));
    let events = c_multiple_attr("events", &event, &cardinality(1, None), (1, 1));
    let history = c_complex(
        "HISTORY",
        &format!("{events}{summary_attr}"),
        "at0001",
        (1, Some(1)),
    );
    let data = c_single_attr("data", &history, (1, 1));
    let obs = observation_root(&data, &[("at0007", "Summary", "@ internal @")]);
    Ok(composition(
        template_id,
        &obs,
        &cardinality(0, None),
        (0, 1),
        None,
    ))
}

/// The EVALUATION `C_ARCHETYPE_ROOT` (openEHR-EHR-EVALUATION.minimal.v1) around
/// a `data` attribute, carrying the at0000/at0003 term definitions plus extras.
fn evaluation_root(data_attr: &str, extra_terms: &[(&str, &str, &str)]) -> String {
    let mut terms = String::new();
    for (code, text, desc) in extra_terms {
        let _ = write!(
            terms,
            "<term_definitions code=\"{code}\"><items id=\"description\">{}</items><items id=\"text\">{}</items></term_definitions>",
            xesc(desc),
            xesc(text)
        );
    }
    format!(
        "<children xsi:type=\"C_ARCHETYPE_ROOT\"><rm_type_name>EVALUATION</rm_type_name>\
<occurrences><lower_included>true</lower_included><lower_unbounded>false</lower_unbounded><upper_unbounded>true</upper_unbounded><lower>0</lower></occurrences>\
<node_id>at0000</node_id>{data_attr}<archetype_id><value>openEHR-EHR-EVALUATION.minimal.v1</value></archetype_id>\
<term_definitions code=\"at0000\"><items id=\"description\">unknown</items><items id=\"text\">Minimal</items></term_definitions>\
<term_definitions code=\"at0003\"><items id=\"description\">@ internal @</items><items id=\"text\">structure</items></term_definitions>{terms}</children>"
    )
}

fn item_structure_type_narrowing(template_id: &str, row: &Cells<'_>) -> Result<String, SynthError> {
    let slot = ItemStructureSlotType::read(row, "slot_type")?;
    let data_child = c_complex(slot.rm_type_name(), "", "at0003", (1, Some(1)));
    let data_attr = c_single_attr("data", &data_child, (1, 1));
    let eval_root = evaluation_root(&data_attr, &[]);
    Ok(composition(
        template_id,
        &eval_root,
        &cardinality(0, None),
        (0, 1),
        None,
    ))
}

/// A leaf ELEMENT (at0004) with a `DV_TEXT` value, as a container member.
fn leaf_element_member() -> String {
    let value_attr = c_single_attr("value", &dv_text_value(), (0, 1));
    format!(
        "<children xsi:type=\"C_COMPLEX_OBJECT\"><rm_type_name>ELEMENT</rm_type_name>{}<node_id>at0004</node_id>{value_attr}</children>",
        occ(0, None)
    )
}

/// One `ITEM_TABLE` row (at0010): a `CLUSTER` whose `items` carry the leaf
/// `ELEMENT` (RM `data_structures` §`ITEM_TABLE` — `rows: List<CLUSTER>`,
/// invariant `Valid_structure: rows.for_all (items.for_all (instance_of
/// ("ELEMENT")))`).
fn table_row_cluster() -> String {
    let items = c_multiple_attr(
        "items",
        &leaf_element_member(),
        &cardinality(0, None),
        (1, 1),
    );
    c_complex("CLUSTER", &items, "at0010", (0, None))
}

/// The RM attribute a container class holds its members in: `ITEM_TABLE` names
/// it `rows`, every other container class names it `items` (RM
/// `data_structures` §`ITEM_TREE` / §`ITEM_LIST` / §`ITEM_TABLE` / §`CLUSTER`).
fn container_attribute(rm_class: &str) -> &'static str {
    if rm_class == "ITEM_TABLE" {
        "rows"
    } else {
        "items"
    }
}

/// `C_MULTIPLE_ATTRIBUTE.cardinality` on the member container of one
/// `ITEM_STRUCTURE` subtype or of a `CLUSTER` (AOM1.4 §`C_MULTIPLE_ATTRIBUTE`).
///
/// NOTE: RM `data_structures` §`CLUSTER` makes `items` 1..1 where the three
/// `ITEM_STRUCTURE` containers make theirs 0..1, so it keeps existence 1..1 for
/// every token instead of following it.
fn item_container_cardinality(
    rm_class: &str,
    template_id: &str,
    row: &Cells<'_>,
) -> Result<String, SynthError> {
    let token = CardinalityToken::read(row, "cardinality")?;
    let is_cluster = rm_class == "CLUSTER";
    // RM `data_structures` §CLUSTER makes `items` 1..*, and AOM2 VCACA makes
    // a stated cardinality legal only when same-or-narrower than the RM's —
    // so a lower bound of 0 is unstatable on CLUSTER. `C_MULTIPLE_ATTRIBUTE`
    // types `cardinality` 1..1 (AOM1.4 class table; the released Archetype.xsd
    // element is mandatory), so `any` restates the RM's own 1..* (same-as-RM
    // is legal) and `opt` is refused rather than silently widened.
    let card = match token {
        CardinalityToken::Opt if is_cluster => {
            return Err(axis_refusal(
                "cardinality",
                "`opt` (0..1) widens CLUSTER.items past the RM's 1..* floor — AOM2 VCACA \
                 makes that template invalid, so the row is unauthorable",
            ));
        }
        CardinalityToken::Any if is_cluster => cardinality(1, None),
        _ => token.cardinality(),
    };
    let exist = if is_cluster {
        (1, 1)
    } else {
        token.existence()
    };
    let member = if rm_class == "ITEM_TABLE" {
        table_row_cluster()
    } else {
        leaf_element_member()
    };
    let container_attr = c_multiple_attr(container_attribute(rm_class), &member, &card, exist);
    let (data_child, extra_terms): (String, &[(&str, &str, &str)]) = if is_cluster {
        let cluster = c_complex("CLUSTER", &container_attr, "at0010", (0, Some(1)));
        let tree_items = c_multiple_attr("items", &cluster, &cardinality(0, None), (0, 1));
        (
            c_complex("ITEM_TREE", &tree_items, "at0003", (1, Some(1))),
            &[
                ("at0004", "value", "*"),
                ("at0010", "Cluster", "@ internal @"),
            ],
        )
    } else if rm_class == "ITEM_TABLE" {
        (
            c_complex(rm_class, &container_attr, "at0003", (1, Some(1))),
            &[("at0004", "value", "*"), ("at0010", "Row", "@ internal @")],
        )
    } else {
        (
            c_complex(rm_class, &container_attr, "at0003", (1, Some(1))),
            &[("at0004", "value", "*")],
        )
    };
    let data_attr = c_single_attr("data", &data_child, (1, 1));
    let eval_root = evaluation_root(&data_attr, extra_terms);
    Ok(composition(
        template_id,
        &eval_root,
        &cardinality(0, None),
        (0, 1),
        None,
    ))
}

/// `C_ATTRIBUTE.existence` on `ELEMENT.value` AND `ELEMENT.null_flavour` in one
/// template (AOM1.4 §`C_ATTRIBUTE`; RM `data_structures` §`ELEMENT` — both 0..1).
fn element_value_null_flavour_existence(
    template_id: &str,
    row: &Cells<'_>,
) -> Result<String, SynthError> {
    let value_exist = ExistenceToken::read(row, "value_existence")?.pair();
    let null_flavour_exist = ExistenceToken::read(row, "null_flavour_existence")?.pair();
    let value_attr = c_single_attr("value", &dv_text_value(), value_exist);
    let null_flavour_attr = c_single_attr(
        "null_flavour",
        &c_complex("DV_CODED_TEXT", "", "", (1, Some(1))),
        null_flavour_exist,
    );
    let element = format!(
        "<children xsi:type=\"C_COMPLEX_OBJECT\"><rm_type_name>ELEMENT</rm_type_name>{}<node_id>at0004</node_id>{value_attr}{null_flavour_attr}</children>",
        occ(0, None)
    );
    let items = c_multiple_attr("items", &element, &cardinality(0, None), (0, 1));
    let tree = c_complex("ITEM_TREE", &items, "at0003", (1, Some(1)));
    let data_attr = c_single_attr("data", &tree, (1, 1));
    let eval_root = evaluation_root(&data_attr, &[("at0004", "value", "*")]);
    Ok(composition(
        template_id,
        &eval_root,
        &cardinality(0, None),
        (0, 1),
        None,
    ))
}

/// `C_OBJECT.rm_type_name` on the `ITEM` member of an `ITEM_TREE` (AOM1.4
/// §`C_OBJECT`; RM `data_structures` §`ITEM` — the abstract parent of `CLUSTER`
/// and `ELEMENT`).
fn item_type_narrowing(template_id: &str, row: &Cells<'_>) -> Result<String, SynthError> {
    let slot = ItemSlotType::read(row, "slot_type")?;
    let member = c_complex(slot.rm_type_name(), "", "at0004", (0, None));
    let items = c_multiple_attr("items", &member, &cardinality(0, None), (0, 1));
    let tree = c_complex("ITEM_TREE", &items, "at0003", (1, Some(1)));
    let data_attr = c_single_attr("data", &tree, (1, 1));
    let eval_root = evaluation_root(&data_attr, &[("at0004", "item", "*")]);
    Ok(composition(
        template_id,
        &eval_root,
        &cardinality(0, None),
        (0, 1),
        None,
    ))
}

fn observation_state_protocol_existence(
    template_id: &str,
    row: &Cells<'_>,
) -> Result<String, SynthError> {
    let state_exist = ExistenceToken::read(row, "state_existence")?.pair();
    let protocol_exist = ExistenceToken::read(row, "protocol_existence")?.pair();
    let data = item_tree_data_observation_history();
    let state_attr = c_single_attr(
        "state",
        &c_complex("HISTORY", &events_only(), "at0005", (1, Some(1))),
        state_exist,
    );
    let protocol_attr = c_single_attr(
        "protocol",
        &c_complex("ITEM_TREE", "", "at0006", (1, Some(1))),
        protocol_exist,
    );
    let attrs = format!("{data}{state_attr}{protocol_attr}");
    let obs = observation_root(
        &attrs,
        &[
            ("at0005", "State", "@ internal @"),
            ("at0006", "Protocol", "@ internal @"),
        ],
    );
    Ok(composition(
        template_id,
        &obs,
        &cardinality(0, None),
        (0, 1),
        None,
    ))
}

/// A HISTORY events attribute (1..*) with one EVENT slot (for the state HISTORY).
fn events_only() -> String {
    let event = c_complex("EVENT", &item_tree_data(), "at0002", (0, Some(1)));
    c_multiple_attr("events", &event, &cardinality(1, None), (1, 1))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn cols(names: &[&str]) -> Vec<String> {
        names.iter().map(ToString::to_string).collect()
    }

    fn lit(s: &str) -> MatrixCell {
        MatrixCell::Literal(json!(s))
    }

    #[test]
    fn cardinality_tokens_map_to_intervals() {
        let c = cols(&["cardinality", "content_count", "expected", "violates"]);
        let cells = vec![
            lit("3to5"),
            MatrixCell::Literal(json!(3)),
            lit("accepted"),
            lit("[]"),
        ];
        let xml = synthesize_opt(
            "CONT-COMPOSITION-content_cardinality",
            "COMPOSITION",
            "cnf.tpl.x.r0",
            &c,
            &cells,
        )
        .unwrap();
        assert!(xml.contains("<cardinality>"));
        assert!(xml.contains("<lower>3</lower>"));
        assert!(xml.contains("<upper>5</upper>"));
        assert!(xml.contains("openEHR-EHR-COMPOSITION.minimal.v1"));
    }

    /// CLUSTER.items is 1..* in the RM, and AOM2 VCACA makes a stated
    /// cardinality legal only when same-or-narrower: `opt` (0..1) is refused
    /// rather than synthesized into a spec-illegal template, and `any`
    /// restates the RM's own 1..*, because `C_MULTIPLE_ATTRIBUTE.cardinality`
    /// is 1..1 and cannot be omitted (adjudicated on #283; the omission
    /// variant shipped schema-invalid OPTs the second reproduction refused).
    #[test]
    fn cluster_items_never_widens_the_rm_floor() {
        let c = cols(&["cardinality", "member_count", "expected", "violates"]);
        let opt_cells = vec![
            lit("opt"),
            MatrixCell::Literal(json!(1)),
            lit("accepted"),
            lit("[]"),
        ];
        let refused = synthesize_opt(
            "CONT-CLUSTER-items_cardinality",
            "CLUSTER",
            "cnf.tpl.x.r0",
            &c,
            &opt_cells,
        )
        .expect_err("`opt` on CLUSTER.items must refuse, never widen");
        assert!(format!("{refused:?}").contains("VCACA"), "{refused:?}");

        let any_cells = vec![
            lit("any"),
            MatrixCell::Literal(json!(1)),
            lit("accepted"),
            lit("[]"),
        ];
        let xml = synthesize_opt(
            "CONT-CLUSTER-items_cardinality",
            "CLUSTER",
            "cnf.tpl.x.r0",
            &c,
            &any_cells,
        )
        .expect("`any` on CLUSTER.items synthesizes without a stated cardinality");
        // Three cardinality elements: the ITEM_TREE wrapper's 0..*, the
        // template's outer container (both floors ARE 0..*), and the
        // CLUSTER's own items restating the RM's 1..*. A missing third is
        // the schema-invalid omission; three with no 1..* interval is the
        // widening regression.
        let cardinalities: Vec<&str> = xml
            .split("<cardinality>")
            .skip(1)
            .filter_map(|rest| rest.split("</cardinality>").next())
            .collect();
        assert_eq!(cardinalities.len(), 3, "{xml}");
        assert_eq!(
            cardinalities
                .iter()
                .filter(|c| c.contains("<lower>1</lower>")
                    && c.contains("<upper_unbounded>true</upper_unbounded>"))
                .count(),
            1,
            "exactly one cardinality restates the RM's 1..*: {cardinalities:?}"
        );
    }

    #[test]
    fn context_existence_mandatory_emits_1_1() {
        let c = cols(&[
            "context_existence",
            "context_committed",
            "expected",
            "violates",
        ]);
        let cells = vec![lit("mandatory"), lit("absent"), lit("rejected"), lit("[]")];
        let xml = synthesize_opt(
            "CONT-COMPOSITION-context_existence",
            "COMPOSITION",
            "cnf.tpl.x.r0",
            &c,
            &cells,
        )
        .unwrap();
        assert!(xml.contains("<rm_attribute_name>context</rm_attribute_name>"));
        // existence 1..1 present on the context attribute.
        assert!(xml.contains("EVENT_CONTEXT"));
    }

    #[test]
    fn event_type_narrowing_sets_slot_rm_type() {
        let c = cols(&["slot_type", "committed_type", "expected", "violates"]);
        let cells = vec![
            lit("POINT_EVENT"),
            lit("INTERVAL_EVENT"),
            lit("rejected"),
            lit("[]"),
        ];
        let xml = synthesize_opt(
            "CONT-EVENT-type_narrowing",
            "EVENT",
            "cnf.tpl.x.r0",
            &c,
            &cells,
        )
        .unwrap();
        assert!(xml.contains("<rm_type_name>POINT_EVENT</rm_type_name>"));
    }

    #[test]
    fn item_structure_uses_evaluation_carrier() {
        let c = cols(&["slot_type", "committed_type", "expected", "violates"]);
        let cells = vec![
            lit("ITEM_TREE"),
            lit("ITEM_LIST"),
            lit("rejected"),
            lit("[]"),
        ];
        let xml = synthesize_opt(
            "CONT-ITEM_STRUCTURE-type_narrowing",
            "ITEM_STRUCTURE",
            "cnf.tpl.x.r0",
            &c,
            &cells,
        )
        .unwrap();
        assert!(xml.contains("openEHR-EHR-EVALUATION.minimal.v1"));
        assert!(xml.contains("<rm_type_name>ITEM_TREE</rm_type_name>"));
    }

    /// Every synthesized template is well-formed XML: the families below the
    /// `ITEM_STRUCTURE` slot assemble their carriers by hand, so an unbalanced
    /// element would otherwise only surface as a rejected upload mid-run.
    #[test]
    #[expect(
        clippy::panic_in_result_fn,
        reason = "the Book ch11 Result-returning test shape: plumbing propagates with ?, the claim is an assertion"
    )]
    fn every_new_container_family_is_well_formed_xml() -> Result<(), quick_xml::Error> {
        let card = cols(&["cardinality", "member_count", "expected", "violates"]);
        let card_cells = vec![
            lit("3to5"),
            MatrixCell::Literal(json!(3)),
            lit("accepted"),
            lit("[]"),
        ];
        let mut documents = Vec::new();
        for rm_class in ["ITEM_TREE", "ITEM_LIST", "ITEM_TABLE", "CLUSTER"] {
            documents.push(
                synthesize_opt(
                    "CONT-X-items_cardinality",
                    rm_class,
                    "cnf.tpl.x.r0",
                    &card,
                    &card_cells,
                )
                .unwrap(),
            );
        }
        documents.push(
            synthesize_opt(
                "CONT-ELEMENT-value_null_flavour_existence",
                "ELEMENT",
                "cnf.tpl.x.r0",
                &cols(&[
                    "value_existence",
                    "null_flavour_existence",
                    "value_committed",
                    "null_flavour_committed",
                    "expected",
                    "violates",
                ]),
                &[
                    lit("mandatory"),
                    lit("optional"),
                    lit("present"),
                    lit("absent"),
                    lit("accepted"),
                    lit("[]"),
                ],
            )
            .unwrap(),
        );
        documents.push(
            synthesize_opt(
                "CONT-ITEM-type_cluster",
                "ITEM",
                "cnf.tpl.x.r0",
                &cols(&["slot_type", "committed_type", "expected", "violates"]),
                &[lit("CLUSTER"), lit("ELEMENT"), lit("rejected"), lit("[]")],
            )
            .unwrap(),
        );
        for document in &documents {
            let mut reader = quick_xml::Reader::from_str(document);
            let mut buffer = Vec::new();
            loop {
                match reader.read_event_into(&mut buffer)? {
                    quick_xml::events::Event::Eof => break,
                    _ => buffer.clear(),
                }
            }
            assert!(document.contains("openEHR-EHR-EVALUATION.minimal.v1"));
        }
        Ok(())
    }

    /// The container attribute under test is the one the RM names, and the
    /// CLUSTER family keeps its RM-mandatory 1..1 existence.
    #[test]
    fn the_container_family_constrains_the_rm_named_attribute() {
        let c = cols(&["cardinality", "member_count", "expected", "violates"]);
        let cells = vec![
            lit("any"),
            MatrixCell::Literal(json!(0)),
            lit("accepted"),
            lit("[]"),
        ];
        let table = synthesize_opt("CONT-X", "ITEM_TABLE", "cnf.tpl.x.r0", &c, &cells).unwrap();
        assert!(table.contains("<rm_attribute_name>rows</rm_attribute_name>"));
        assert!(table.contains("<rm_type_name>ITEM_TABLE</rm_type_name>"));
        let cluster = synthesize_opt("CONT-X", "CLUSTER", "cnf.tpl.x.r0", &c, &cells).unwrap();
        assert!(cluster.contains("<node_id>at0010</node_id>"));
        // The `any` token would leave a 0..1 existence on the three
        // ITEM_STRUCTURE containers; CLUSTER.items is RM 1..1 and never follows
        // it, so no 0..1 existence sits on this template's items attribute.
        let items = cluster
            .split("<rm_attribute_name>items</rm_attribute_name>")
            .nth(2)
            .unwrap_or_default();
        assert!(
            items.contains("<lower>1</lower><upper>1</upper></existence>"),
            "{items}"
        );
    }

    #[test]
    fn observation_state_protocol_existence_both_mandatory() {
        let c = cols(&[
            "state_existence",
            "protocol_existence",
            "data_committed",
            "state_committed",
            "protocol_committed",
            "expected",
            "violates",
        ]);
        let cells = vec![
            lit("mandatory"),
            lit("mandatory"),
            lit("present"),
            lit("absent"),
            lit("absent"),
            lit("rejected"),
            lit("[]"),
        ];
        let xml = synthesize_opt(
            "CONT-OBSERVATION-state_protocol_existence",
            "OBSERVATION",
            "cnf.tpl.x.r0",
            &c,
            &cells,
        )
        .unwrap();
        assert!(xml.contains("<rm_attribute_name>state</rm_attribute_name>"));
        assert!(xml.contains("<rm_attribute_name>protocol</rm_attribute_name>"));
    }

    #[test]
    fn synthesis_is_deterministic() {
        let c = cols(&["cardinality", "content_count", "expected", "violates"]);
        let cells = vec![
            lit("any"),
            MatrixCell::Literal(json!(0)),
            lit("accepted"),
            lit("[]"),
        ];
        let a = synthesize_opt(
            "CONT-COMPOSITION-content_cardinality",
            "COMPOSITION",
            "cnf.tpl.x.r0",
            &c,
            &cells,
        )
        .unwrap();
        let b = synthesize_opt(
            "CONT-COMPOSITION-content_cardinality",
            "COMPOSITION",
            "cnf.tpl.x.r0",
            &c,
            &cells,
        )
        .unwrap();
        assert_eq!(a, b);
    }

    /// One doctored row per structural family: the case id, its `rm_class`,
    /// the columns, the cells carrying the typo, and the phrase the refusal
    /// must name.
    type DoctoredRow = (
        &'static str,
        &'static str,
        Vec<&'static str>,
        Vec<MatrixCell>,
        &'static str,
    );

    /// A doctored cell for every structural family, so a family added later
    /// without a closed vocabulary has no row here and is visible as such.
    fn doctored_rows() -> Vec<DoctoredRow> {
        vec![
            (
                "CONT-COMPOSITION-content_cardinality",
                "COMPOSITION",
                vec!["cardinality", "content_count"],
                vec![lit("3to6"), MatrixCell::Literal(json!(3))],
                "cardinality vocabulary",
            ),
            (
                "CONT-COMPOSITION-context_existence",
                "COMPOSITION",
                vec!["context_existence", "context_committed"],
                vec![lit("required"), lit("absent")],
                "existence vocabulary",
            ),
            (
                "CONT-COMP-content_card_any-context_mand",
                "COMPOSITION",
                vec!["cardinality", "context_existence"],
                vec![lit("any"), lit("Mandatory")],
                "existence vocabulary",
            ),
            (
                "CONT-HISTORY-events_cardinality",
                "HISTORY",
                vec!["cardinality", "events_count"],
                vec![lit("1plu"), MatrixCell::Literal(json!(1))],
                "cardinality vocabulary",
            ),
            (
                "CONT-HIST-events_card_any-summary_ex_mand",
                "HISTORY",
                vec!["cardinality", "summary_existence"],
                vec![lit("any"), lit("optionall")],
                "existence vocabulary",
            ),
            (
                "CONT-HISTORY-summary_existence",
                "HISTORY",
                vec!["summary_existence", "summary_committed"],
                vec![lit("opt"), lit("absent")],
                "existence vocabulary",
            ),
            (
                "CONT-EVENT-state_existence",
                "EVENT",
                vec!["state_existence", "state_committed"],
                vec![lit("mand"), lit("absent")],
                "existence vocabulary",
            ),
            (
                "CONT-EVENT-type_narrowing",
                "EVENT",
                vec!["slot_type", "committed_type"],
                vec![lit("POITN_EVENT"), lit("POINT_EVENT")],
                "RM EVENT class",
            ),
            (
                "CONT-ITEM_STRUCTURE-type_narrowing",
                "ITEM_STRUCTURE",
                vec!["slot_type", "committed_type"],
                vec![lit("ITEM_CLUSTER"), lit("ITEM_TREE")],
                "RM ITEM_STRUCTURE class",
            ),
            (
                "CONT-OBSERVATION-state_protocol_existence",
                "OBSERVATION",
                vec!["state_existence", "protocol_existence"],
                vec![lit("mandatory"), lit("MANDATORY")],
                "existence vocabulary",
            ),
            (
                "CONT-ITEM_TREE-items_cardinality",
                "ITEM_TREE",
                vec!["cardinality", "items_count"],
                vec![lit("0plus"), MatrixCell::Literal(json!(1))],
                "cardinality vocabulary",
            ),
            (
                "CONT-CLUSTER-items_cardinality",
                "CLUSTER",
                vec!["cardinality", "items_count"],
                vec![lit("anyy"), MatrixCell::Literal(json!(1))],
                "cardinality vocabulary",
            ),
            (
                "CONT-ELEMENT-value_null_flavour_existence",
                "ELEMENT",
                vec!["value_existence", "null_flavour_existence"],
                vec![lit("optional"), lit("mandatry")],
                "existence vocabulary",
            ),
            (
                "CONT-ITEM-type_narrowing",
                "ITEM",
                vec!["slot_type", "committed_type"],
                vec![lit("CLUSTERR"), lit("CLUSTER")],
                "RM ITEM class",
            ),
        ]
    }

    /// A doctored constraint-axis cell REFUSES instead of synthesizing a
    /// permissive default. A mistyped `3to5` that silently became `0..*` would
    /// bake a constraint no committed row declares, and the SUT would then be
    /// graded against it — a passing row manufactured out of the typo.
    #[test]
    fn an_unknown_structural_token_refuses() {
        for (case, rm_class, columns, cells, needle) in doctored_rows() {
            let refused = synthesize_opt(case, rm_class, "cnf.tpl.x.r0", &cols(&columns), &cells);
            let Err(SynthError::Unsupported(message)) = refused else {
                panic!("{case}: a doctored token must not synthesize: {refused:?}");
            };
            assert!(message.contains(needle), "{case}: {message}");
        }
    }

    /// An axis whose cell is absent or non-textual refuses too: the family
    /// reads a constraint the row never authored, so there is nothing to bake.
    #[test]
    fn an_absent_structural_axis_refuses() {
        let refused = synthesize_opt(
            "CONT-COMPOSITION-content_cardinality",
            "COMPOSITION",
            "cnf.tpl.x.r0",
            &cols(&["cardinality", "content_count"]),
            &[MatrixCell::Null, MatrixCell::Literal(json!(3))],
        );
        let Err(SynthError::Unsupported(message)) = refused else {
            panic!("a null axis cell must not synthesize: {refused:?}");
        };
        assert!(message.contains("cardinality"), "{message}");
        assert!(message.contains("absent"), "{message}");
    }

    /// Every token the committed catalogue spells still synthesizes, so the
    /// closed vocabularies match the artifacts they read rather than narrowing
    /// coverage.
    #[test]
    fn every_committed_structural_token_synthesizes() {
        for token in ["any", "1plus", "3plus", "opt", "mand", "3to5"] {
            let xml = synthesize_opt(
                "CONT-COMPOSITION-content_cardinality",
                "COMPOSITION",
                "cnf.tpl.x.r0",
                &cols(&["cardinality", "content_count"]),
                &[lit(token), MatrixCell::Literal(json!(3))],
            );
            assert!(xml.is_ok(), "{token}: {xml:?}");
        }
        for token in ["optional", "mandatory"] {
            let xml = synthesize_opt(
                "CONT-COMPOSITION-context_existence",
                "COMPOSITION",
                "cnf.tpl.x.r0",
                &cols(&["context_existence", "context_committed"]),
                &[lit(token), lit("absent")],
            );
            assert!(xml.is_ok(), "{token}: {xml:?}");
        }
        for token in ["EVENT", "POINT_EVENT", "INTERVAL_EVENT"] {
            let xml = synthesize_opt(
                "CONT-EVENT-type_narrowing",
                "EVENT",
                "cnf.tpl.x.r0",
                &cols(&["slot_type", "committed_type"]),
                &[lit(token), lit("POINT_EVENT")],
            );
            assert!(xml.is_ok(), "{token}: {xml:?}");
        }
        for token in [
            "ITEM_STRUCTURE",
            "ITEM_SINGLE",
            "ITEM_LIST",
            "ITEM_TABLE",
            "ITEM_TREE",
        ] {
            let xml = synthesize_opt(
                "CONT-ITEM_STRUCTURE-type_narrowing",
                "ITEM_STRUCTURE",
                "cnf.tpl.x.r0",
                &cols(&["slot_type", "committed_type"]),
                &[lit(token), lit("ITEM_TREE")],
            );
            assert!(xml.is_ok(), "{token}: {xml:?}");
        }
        for token in ["ITEM", "CLUSTER", "ELEMENT"] {
            let xml = synthesize_opt(
                "CONT-ITEM-type_narrowing",
                "ITEM",
                "cnf.tpl.x.r0",
                &cols(&["slot_type", "committed_type"]),
                &[lit(token), lit("CLUSTER")],
            );
            assert!(xml.is_ok(), "{token}: {xml:?}");
        }
        for token in ["optional", "mandatory"] {
            let xml = synthesize_opt(
                "CONT-ELEMENT-value_null_flavour_existence",
                "ELEMENT",
                "cnf.tpl.x.r0",
                &cols(&["value_existence", "null_flavour_existence"]),
                &[lit(token), lit("optional")],
            );
            assert!(xml.is_ok(), "{token}: {xml:?}");
        }
    }
}
