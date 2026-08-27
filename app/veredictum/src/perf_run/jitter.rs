// SPDX-FileCopyrightText: Veredictum contributors
// SPDX-License-Identifier: Apache-2.0

//! Constraint-aware value jitter for the composition pack's numeric leaves.
//!
//! A committed example skeleton is ONE payload. Committing it N times gives a
//! SUT a population whose every leaf value is identical, which lets structural
//! sharing, an index and a page cache behave in a way no real population would
//! produce. This module reads the leaf constraints the operational template
//! itself carries and redraws each numeric leaf inside its own declared range,
//! deterministically from the run seed.
//!
//! The constraint shapes read here are the released AM 1.4 XML ones:
//! `C_DV_QUANTITY` carries `list: C_QUANTITY_ITEM`, each with a `magnitude`
//! `IntervalOfReal` and its `units`
//! (`specs/its-xml-schemas/components/AM/Release-1.4/OpenehrProfile.xsd`
//! §`C_DV_QUANTITY`, §`C_QUANTITY_ITEM`), and a `DV_COUNT` node's `magnitude`
//! attribute carries a `C_PRIMITIVE_OBJECT` whose `item` is a `C_INTEGER` with
//! a `range: IntervalOfInteger`
//! (`specs/its-xml-schemas/components/AM/Release-1.4/Archetype.xsd`
//! §`C_PRIMITIVE_OBJECT`, §`C_INTEGER`).
//!
//! NOTE: no openEHR spec governs measured-performance payload variation — the
//! jitter policy below is our own design.

#![expect(
    clippy::disallowed_types,
    reason = "dev/verification tooling over JSON artifacts (the catalogue, results, wire \
              exchanges) — not the application (FerroEHR#1694)"
)]

use std::collections::BTreeMap;

use serde_json::{Number, Value};

/// The XML Schema instance namespace, whose `type` attribute discriminates
/// every constraint node of an operational template.
const XSI_NAMESPACE: &str = "http://www.w3.org/2001/XMLSchema-instance";

/// The payload-jitter draw stream, kept distinct from the arrival schedule's
/// seed so a leaf value never correlates with the instant its arrival fires.
const JITTER_SEED: u64 = 0x7665_7264_6a69_7474;

/// How many draw steps span a declared magnitude range. A power of ten keeps
/// the drawn fraction exactly representable at the decimal places the
/// committed value already carries.
const DRAW_STEPS: u32 = 1_000_000;

/// The most decimal places a redrawn magnitude keeps, so the step scale stays
/// exactly representable in binary64.
const MAX_DECIMALS: u32 = 6;

/// Why an operational template's leaf constraints could not be read.
#[derive(Debug, thiserror::Error)]
pub enum ConstraintReadError {
    /// The template is not a well-formed XML document.
    #[error("the operational template is not well-formed XML: {0}")]
    Malformed(String),
    /// A constraint bound is not a number the interval's type admits.
    #[error("the `{element}` bound of a constraint interval is not a number: {text:?}")]
    Bound {
        /// The bound element (`lower` or `upper`).
        element: String,
        /// The text the element carried.
        text: String,
    },
}

/// A real interval as an operational template declares it, `None` on a side
/// meaning that side is unbounded.
#[derive(Debug, Clone, Copy)]
struct RealInterval {
    lower: Option<f64>,
    lower_included: bool,
    upper: Option<f64>,
    upper_included: bool,
}

impl RealInterval {
    /// The interval that constrains nothing.
    const UNBOUNDED: Self = Self {
        lower: None,
        lower_included: true,
        upper: None,
        upper_included: true,
    };

    /// The tightest interval contained in both.
    fn intersect(self, other: Self) -> Self {
        let (lower, lower_included) = match (self.lower, other.lower) {
            (None, None) => (None, true),
            (Some(a), None) => (Some(a), self.lower_included),
            (None, Some(b)) => (Some(b), other.lower_included),
            (Some(a), Some(b)) if a > b => (Some(a), self.lower_included),
            (Some(a), Some(b)) if b > a => (Some(b), other.lower_included),
            (Some(a), Some(_)) => (Some(a), self.lower_included && other.lower_included),
        };
        let (upper, upper_included) = match (self.upper, other.upper) {
            (None, None) => (None, true),
            (Some(a), None) => (Some(a), self.upper_included),
            (None, Some(b)) => (Some(b), other.upper_included),
            (Some(a), Some(b)) if a < b => (Some(a), self.upper_included),
            (Some(a), Some(b)) if b < a => (Some(b), other.upper_included),
            (Some(a), Some(_)) => (Some(a), self.upper_included && other.upper_included),
        };
        Self {
            lower,
            lower_included,
            upper,
            upper_included,
        }
    }

    /// Whether a value satisfies the lower bound.
    fn admits_lower(self, value: f64) -> bool {
        self.lower.is_none_or(|lower| {
            if self.lower_included {
                value >= lower
            } else {
                value > lower
            }
        })
    }

    /// Whether a value satisfies the upper bound.
    fn admits_upper(self, value: f64) -> bool {
        self.upper.is_none_or(|upper| {
            if self.upper_included {
                value <= upper
            } else {
                value < upper
            }
        })
    }
}

/// An integer interval as an operational template declares it.
#[derive(Debug, Clone, Copy)]
struct IntInterval {
    lower: Option<i64>,
    lower_included: bool,
    upper: Option<i64>,
    upper_included: bool,
}

impl IntInterval {
    /// The tightest interval contained in both.
    fn intersect(self, other: Self) -> Self {
        let (lower, lower_included) = match (self.lower, other.lower) {
            (None, None) => (None, true),
            (Some(a), None) => (Some(a), self.lower_included),
            (None, Some(b)) => (Some(b), other.lower_included),
            (Some(a), Some(b)) => match a.cmp(&b) {
                std::cmp::Ordering::Greater => (Some(a), self.lower_included),
                std::cmp::Ordering::Less => (Some(b), other.lower_included),
                std::cmp::Ordering::Equal => (Some(a), self.lower_included && other.lower_included),
            },
        };
        let (upper, upper_included) = match (self.upper, other.upper) {
            (None, None) => (None, true),
            (Some(a), None) => (Some(a), self.upper_included),
            (None, Some(b)) => (Some(b), other.upper_included),
            (Some(a), Some(b)) => match a.cmp(&b) {
                std::cmp::Ordering::Less => (Some(a), self.upper_included),
                std::cmp::Ordering::Greater => (Some(b), other.upper_included),
                std::cmp::Ordering::Equal => (Some(a), self.upper_included && other.upper_included),
            },
        };
        Self {
            lower,
            lower_included,
            upper,
            upper_included,
        }
    }

    /// The closed integer span, when both sides are bounded and non-empty.
    fn closed(self) -> Option<(i64, i64)> {
        let lower = if self.lower_included {
            self.lower?
        } else {
            self.lower?.checked_add(1)?
        };
        let upper = if self.upper_included {
            self.upper?
        } else {
            self.upper?.checked_sub(1)?
        };
        (lower <= upper).then_some((lower, upper))
    }
}

/// An interval under construction, filled element by element as the reader
/// walks one `IntervalOfReal` or `IntervalOfInteger`.
#[derive(Debug, Default)]
struct IntervalAccumulator {
    lower: Option<String>,
    upper: Option<String>,
    lower_included: Option<bool>,
    upper_included: Option<bool>,
    lower_unbounded: bool,
    upper_unbounded: bool,
}

impl IntervalAccumulator {
    /// Whether any bound element of the interval was seen.
    fn is_filled(&self) -> bool {
        self.lower.is_some()
            || self.upper.is_some()
            || self.lower_unbounded
            || self.upper_unbounded
            || self.lower_included.is_some()
            || self.upper_included.is_some()
    }

    /// Records one bound element of the interval currently being read.
    fn record(&mut self, element: &str, text: &str) {
        let flag = text.trim() == "true";
        match element {
            "lower" => self.lower = Some(text.trim().to_owned()),
            "upper" => self.upper = Some(text.trim().to_owned()),
            "lower_included" => self.lower_included = Some(flag),
            "upper_included" => self.upper_included = Some(flag),
            "lower_unbounded" => self.lower_unbounded = flag,
            "upper_unbounded" => self.upper_unbounded = flag,
            _ => {}
        }
    }

    /// The real interval read, or a typed error naming the unreadable bound.
    fn real(&self) -> Result<RealInterval, ConstraintReadError> {
        // The Bound variant names the element and the text it carried, which
        // is the whole diagnosis; a `ParseFloatError` adds nothing to it.
        let parse = |element: &str, text: Option<&String>| -> Result<Option<f64>, _> {
            match text.map(|raw| (raw, raw.parse::<f64>())) {
                None => Ok(None),
                Some((_, Ok(value))) => Ok(Some(value)),
                Some((raw, Err(_))) => Err(ConstraintReadError::Bound {
                    element: element.to_owned(),
                    text: raw.clone(),
                }),
            }
        };
        Ok(RealInterval {
            lower: if self.lower_unbounded {
                None
            } else {
                parse("lower", self.lower.as_ref())?
            },
            lower_included: self.lower_included.unwrap_or(true),
            upper: if self.upper_unbounded {
                None
            } else {
                parse("upper", self.upper.as_ref())?
            },
            upper_included: self.upper_included.unwrap_or(true),
        })
    }

    /// The integer interval read, or a typed error naming the unreadable bound.
    fn integer(&self) -> Result<IntInterval, ConstraintReadError> {
        let parse = |element: &str, text: Option<&String>| -> Result<Option<i64>, _> {
            match text.map(|raw| (raw, raw.parse::<i64>())) {
                None => Ok(None),
                Some((_, Ok(value))) => Ok(Some(value)),
                Some((raw, Err(_))) => Err(ConstraintReadError::Bound {
                    element: element.to_owned(),
                    text: raw.clone(),
                }),
            }
        };
        Ok(IntInterval {
            lower: if self.lower_unbounded {
                None
            } else {
                parse("lower", self.lower.as_ref())?
            },
            lower_included: self.lower_included.unwrap_or(true),
            upper: if self.upper_unbounded {
                None
            } else {
                parse("upper", self.upper.as_ref())?
            },
            upper_included: self.upper_included.unwrap_or(true),
        })
    }
}

/// One element of the reader's open-element stack.
#[derive(Debug)]
struct Frame {
    name: String,
    xsi_type: Option<String>,
    rm_type: Option<String>,
    rm_attribute: Option<String>,
}

/// Which interval the reader is currently filling.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Filling {
    /// Nothing: the reader is outside every interval it reads.
    Nothing,
    /// The `magnitude` of a `C_QUANTITY_ITEM`.
    QuantityMagnitude,
    /// The `range` of a `C_INTEGER` constraining a `DV_COUNT` magnitude.
    CountRange,
}

/// Every leaf constraint the pack can read out of one operational template.
///
/// Two leaf families jitter, because a template declares their permitted range
/// in a form that attaches to the instance leaf with no archetype-path
/// resolution:
///
/// - `DV_QUANTITY.magnitude`, keyed by the leaf's own `units`. A valid leaf's
///   node constrains a `C_DV_QUANTITY` whose `list` holds an entry for those
///   units, so the INTERSECTION of every `magnitude` interval the template
///   declares for those units is contained in that entry's own interval, and a
///   value drawn from the intersection is admissible wherever those units are.
/// - `DV_COUNT.magnitude`, from the intersection of every `C_INTEGER` `range`
///   the template declares under a `DV_COUNT` `magnitude` attribute. The same
///   containment argument holds with the template as the key.
///
/// The families that deliberately do NOT jitter, each because the constraint
/// does not carry what a valid instance would have to say:
///
/// - `DV_CODED_TEXT` and `DV_ORDINAL`. A `C_DV_ORDINAL` `list` entry carries an
///   empty `symbol/value`, and a `C_CODE_PHRASE` carries codes only, so the
///   rubric the instance must present alongside a redrawn code is not in the
///   constraint at all.
/// - `DV_TEXT`. Its `value` is constrained by a `C_STRING` that only an
///   archetype-path resolution could attach to one leaf, and the same shape
///   also carries every `LOCATABLE.name`.
/// - `DV_DATE_TIME`. `EVENT.time` is read relative to `HISTORY.origin`
///   (RM `data_structures/master06-history_package.adoc` §The History Class),
///   so event times carry a relationship across nodes that a per-leaf redraw
///   would not preserve.
#[derive(Debug, Clone, Default)]
pub struct LeafConstraints {
    /// Per-units intersected `DV_QUANTITY.magnitude` interval.
    quantity: BTreeMap<String, RealInterval>,
    /// The template-wide intersected `DV_COUNT.magnitude` interval.
    count: Option<IntInterval>,
}

impl LeafConstraints {
    /// Reads every jitterable leaf constraint out of an operational template.
    ///
    /// # Errors
    /// [`ConstraintReadError`] when the template is not well-formed XML, or
    /// when a constraint interval carries a bound that is not a number.
    pub fn from_opt(opt_xml: &str) -> Result<Self, ConstraintReadError> {
        let mut reader = quick_xml::NsReader::from_str(opt_xml);
        let mut stack: Vec<Frame> = Vec::new();
        let mut quantity: BTreeMap<String, RealInterval> = BTreeMap::new();
        let mut count: Option<IntInterval> = None;
        let mut filling = Filling::Nothing;
        let mut accumulator = IntervalAccumulator::default();
        let mut units: Option<String> = None;
        let mut list_open = false;
        loop {
            let event = reader
                .read_event()
                .map_err(|e| ConstraintReadError::Malformed(e.to_string()))?;
            match event {
                // Element balance is tracked here rather than left to the
                // reader: quick-xml reports end-of-input on a document whose
                // elements are still open, and a truncated operational
                // template is not one whose constraints may be trusted.
                quick_xml::events::Event::Eof if stack.is_empty() => break,
                quick_xml::events::Event::Eof => {
                    return Err(ConstraintReadError::Malformed(format!(
                        "the document ends with {} element(s) still open",
                        stack.len()
                    )));
                }
                quick_xml::events::Event::Start(start) => {
                    let frame = frame_of(&mut reader, &start)?;
                    match frame.name.as_str() {
                        "list" if parent_is(&stack, "C_DV_QUANTITY") => {
                            list_open = true;
                            units = None;
                            accumulator = IntervalAccumulator::default();
                            filling = Filling::Nothing;
                        }
                        "magnitude" if list_open && filling == Filling::Nothing => {
                            filling = Filling::QuantityMagnitude;
                            accumulator = IntervalAccumulator::default();
                        }
                        "range" if parent_is(&stack, "C_INTEGER") && in_count_magnitude(&stack) => {
                            filling = Filling::CountRange;
                            accumulator = IntervalAccumulator::default();
                        }
                        _ => {}
                    }
                    stack.push(frame);
                }
                quick_xml::events::Event::Text(text) => {
                    let decoded = text
                        .decode()
                        .map_err(|e| ConstraintReadError::Malformed(e.to_string()))?;
                    let trimmed = decoded.trim();
                    if !trimmed.is_empty() {
                        record_text(&mut stack, &mut accumulator, &mut units, filling, trimmed);
                    }
                }
                quick_xml::events::Event::End(_) => {
                    let closed = stack.pop();
                    let name = closed.as_ref().map_or("", |frame| frame.name.as_str());
                    match name {
                        "magnitude" if filling == Filling::QuantityMagnitude => {
                            // The accumulator is held until `</list>`: `units`
                            // is the LAST child of a C_QUANTITY_ITEM, so the
                            // key it files under is not known yet.
                            filling = Filling::Nothing;
                        }
                        "range" if filling == Filling::CountRange => {
                            let read = accumulator.integer()?;
                            count = Some(count.map_or(read, |held| held.intersect(read)));
                            filling = Filling::Nothing;
                            accumulator = IntervalAccumulator::default();
                        }
                        "list" if list_open => {
                            if let Some(key) = units.take() {
                                let read = if accumulator.is_filled() {
                                    accumulator.real()?
                                } else {
                                    RealInterval::UNBOUNDED
                                };
                                let held = quantity
                                    .get(&key)
                                    .copied()
                                    .map_or(read, |held| held.intersect(read));
                                quantity.insert(key, held);
                            }
                            list_open = false;
                            accumulator = IntervalAccumulator::default();
                        }
                        _ => {}
                    }
                }
                _ => {}
            }
        }
        Ok(Self { quantity, count })
    }

    /// Whether the template declares no readable numeric-leaf constraint.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.quantity.is_empty() && self.count.is_none()
    }

    /// Redraws every readable numeric leaf of one composition body, in place.
    ///
    /// The draw is a pure function of `template_key` and `arrival`, so the same
    /// arrival of the same template reproduces byte for byte.
    pub(crate) fn apply(&self, body: &mut Value, template_key: &str, arrival: u64) {
        if self.is_empty() {
            return;
        }
        let stream = template_key.bytes().fold(
            crate::perf_run::fnv1a(JITTER_SEED, &[arrival]),
            |seed, byte| crate::perf_run::fnv1a(seed, &[u64::from(byte)]),
        );
        let mut ordinal: u64 = 0;
        self.redraw(body, stream, &mut ordinal);
    }

    /// The pre-order walk: every numeric leaf takes the next draw of the
    /// stream, whether or not the template lets it move.
    fn redraw(&self, value: &mut Value, stream: u64, ordinal: &mut u64) {
        match value {
            Value::Object(map) => {
                let leaf = map.get("_type").and_then(Value::as_str).map(str::to_owned);
                if leaf.as_deref() == Some("DV_QUANTITY") || leaf.as_deref() == Some("DV_COUNT") {
                    let draw = crate::perf_run::fnv1a(stream, &[*ordinal]);
                    *ordinal = ordinal.wrapping_add(1);
                    if leaf.as_deref() == Some("DV_QUANTITY") {
                        self.redraw_quantity(map, draw);
                    } else {
                        self.redraw_count(map, draw);
                    }
                }
                for (_, child) in map.iter_mut() {
                    self.redraw(child, stream, ordinal);
                }
            }
            Value::Array(items) => {
                for item in items.iter_mut() {
                    self.redraw(item, stream, ordinal);
                }
            }
            _ => {}
        }
    }

    /// Redraws one `DV_QUANTITY` magnitude, leaving the leaf untouched when the
    /// template declares nothing readable for its units.
    fn redraw_quantity(&self, map: &mut serde_json::Map<String, Value>, draw: u64) {
        let Some(interval) = map
            .get("units")
            .and_then(Value::as_str)
            .and_then(|units| self.quantity.get(units))
            .copied()
        else {
            return;
        };
        let Some(decimals) = map.get("magnitude").and_then(decimals_of) else {
            return;
        };
        let Some(redrawn) = redraw_real(interval, decimals, draw).and_then(Number::from_f64) else {
            return;
        };
        map.insert("magnitude".to_owned(), Value::Number(redrawn));
    }

    /// Redraws one `DV_COUNT` magnitude, leaving the leaf untouched when the
    /// template declares no readable integer range.
    fn redraw_count(&self, map: &mut serde_json::Map<String, Value>, draw: u64) {
        let Some((lower, upper)) = self.count.and_then(IntInterval::closed) else {
            return;
        };
        if !map.contains_key("magnitude") {
            return;
        }
        let Some(redrawn) = redraw_integer(lower, upper, draw) else {
            return;
        };
        map.insert("magnitude".to_owned(), Value::Number(Number::from(redrawn)));
    }
}

/// Routes one text node into the frame or interval it belongs to.
fn record_text(
    stack: &mut [Frame],
    accumulator: &mut IntervalAccumulator,
    units: &mut Option<String>,
    filling: Filling,
    text: &str,
) {
    let depth = stack.len();
    let name = match stack.last() {
        Some(frame) => frame.name.clone(),
        None => return,
    };
    match name.as_str() {
        "rm_type_name" | "rm_attribute_name" => {
            let Some(parent) = depth.checked_sub(2).and_then(|i| stack.get_mut(i)) else {
                return;
            };
            if name == "rm_type_name" {
                parent.rm_type = Some(text.to_owned());
            } else {
                parent.rm_attribute = Some(text.to_owned());
            }
        }
        "units" => {
            let is_quantity_item = depth
                .checked_sub(2)
                .and_then(|i| stack.get(i))
                .is_some_and(|parent| parent.name == "list");
            if is_quantity_item {
                *units = Some(text.to_owned());
            }
        }
        _ if filling != Filling::Nothing => accumulator.record(&name, text),
        _ => {}
    }
}

/// The frame for one opening element, carrying its resolved `xsi:type`.
fn frame_of(
    reader: &mut quick_xml::NsReader<&[u8]>,
    start: &quick_xml::events::BytesStart<'_>,
) -> Result<Frame, ConstraintReadError> {
    let name = String::from_utf8_lossy(start.local_name().as_ref()).into_owned();
    let mut xsi_type = None;
    for attribute in start.attributes() {
        let attribute = attribute.map_err(|e| ConstraintReadError::Malformed(e.to_string()))?;
        let (namespace, local) = reader.resolver_mut().resolve_attribute(attribute.key);
        let is_xsi_type = local.as_ref() == b"type"
            && matches!(
                namespace,
                quick_xml::name::ResolveResult::Bound(ns) if ns.as_ref() == XSI_NAMESPACE.as_bytes()
            );
        if is_xsi_type {
            let value = String::from_utf8_lossy(attribute.value.as_ref()).into_owned();
            xsi_type = value.rsplit(':').next().map(str::to_owned);
        }
    }
    Ok(Frame {
        name,
        xsi_type,
        rm_type: None,
        rm_attribute: None,
    })
}

/// Whether the innermost open element carries this `xsi:type`.
fn parent_is(stack: &[Frame], xsi_type: &str) -> bool {
    stack
        .last()
        .is_some_and(|frame| frame.xsi_type.as_deref() == Some(xsi_type))
}

/// Whether the innermost enclosing constrained ATTRIBUTE is the `magnitude` of
/// a `DV_COUNT` node.
fn in_count_magnitude(stack: &[Frame]) -> bool {
    for index in (0..stack.len()).rev() {
        let Some(frame) = stack.get(index) else {
            return false;
        };
        let Some(attribute) = frame.rm_attribute.as_deref() else {
            continue;
        };
        let owner = index
            .checked_sub(1)
            .and_then(|i| stack.get(i))
            .and_then(|parent| parent.rm_type.as_deref());
        return attribute == "magnitude" && owner == Some("DV_COUNT");
    }
    false
}

/// The decimal places a committed magnitude carries, so a redrawn one keeps the
/// same shape rather than inventing precision the example never had.
fn decimals_of(value: &Value) -> Option<u32> {
    let Value::Number(number) = value else {
        return None;
    };
    let text = number.to_string();
    if text.contains(['e', 'E']) {
        return None;
    }
    let fraction = text.split_once('.').map_or("", |(_, fraction)| fraction);
    u32::try_from(fraction.len())
        .ok()
        .map(|d| d.min(MAX_DECIMALS))
}

/// A magnitude drawn inside a declared real interval, at a fixed number of
/// decimal places, or `None` when no such value exists.
fn redraw_real(interval: RealInterval, decimals: u32, draw: u64) -> Option<f64> {
    let (lower, upper) = (interval.lower?, interval.upper?);
    if !lower.is_finite() || !upper.is_finite() || upper <= lower {
        return None;
    }
    let scale = 10_f64.powi(i32::try_from(decimals).ok()?);
    let step = 1.0 / scale;
    let index = u32::try_from(draw % u64::from(DRAW_STEPS)).ok()?;
    let fraction = f64::from(index) / f64::from(DRAW_STEPS);
    let mut value = ((lower + fraction * (upper - lower)) * scale).round() / scale;
    if !interval.admits_lower(value) {
        value = (lower * scale).ceil() / scale;
        if !interval.admits_lower(value) {
            value += step;
        }
    }
    if !interval.admits_upper(value) {
        value = (upper * scale).floor() / scale;
        if !interval.admits_upper(value) {
            value -= step;
        }
    }
    let inside = interval.admits_lower(value) && interval.admits_upper(value);
    (inside && value.is_finite()).then_some(value)
}

/// A count drawn inside a closed integer span.
fn redraw_integer(lower: i64, upper: i64, draw: u64) -> Option<i64> {
    let span = u64::try_from(upper.checked_sub(lower)?)
        .ok()?
        .checked_add(1)?;
    let offset = i64::try_from(draw % span).ok()?;
    lower.checked_add(offset)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A minimal operational template carrying one `C_DV_QUANTITY` per units
    /// and one `DV_COUNT` magnitude range, in the AM 1.4 XML shape the CKM
    /// exports use.
    fn opt(body: &str) -> String {
        format!(
            "<template xmlns=\"http://schemas.openehr.org/v1\" \
             xmlns:xsi=\"http://www.w3.org/2001/XMLSchema-instance\">{body}</template>"
        )
    }

    fn quantity_item(units: &str, lower: &str, upper: &str, upper_included: bool) -> String {
        format!(
            "<children xsi:type=\"C_DV_QUANTITY\"><rm_type_name>DV_QUANTITY</rm_type_name>\
             <list><magnitude><lower_included>true</lower_included>\
             <upper_included>{upper_included}</upper_included>\
             <lower_unbounded>false</lower_unbounded><upper_unbounded>false</upper_unbounded>\
             <lower>{lower}</lower><upper>{upper}</upper></magnitude>\
             <precision><lower>1</lower><upper>1</upper></precision>\
             <units>{units}</units></list></children>"
        )
    }

    fn count_range(lower: &str, upper: &str) -> String {
        format!(
            "<children xsi:type=\"C_COMPLEX_OBJECT\"><rm_type_name>DV_COUNT</rm_type_name>\
             <attributes xsi:type=\"C_SINGLE_ATTRIBUTE\">\
             <rm_attribute_name>magnitude</rm_attribute_name>\
             <children xsi:type=\"C_PRIMITIVE_OBJECT\"><rm_type_name>INTEGER</rm_type_name>\
             <item xsi:type=\"C_INTEGER\"><range><lower_included>true</lower_included>\
             <upper_included>true</upper_included><lower_unbounded>false</lower_unbounded>\
             <upper_unbounded>false</upper_unbounded><lower>{lower}</lower><upper>{upper}</upper>\
             </range></item></children></attributes></children>"
        )
    }

    #[test]
    fn a_units_range_is_the_intersection_of_every_declaration_for_it() {
        let xml = opt(&format!(
            "{}{}",
            quantity_item("/min", "0", "1000", false),
            quantity_item("/min", "0", "200", false)
        ));
        let read = LeafConstraints::from_opt(&xml).unwrap();
        let interval = read.quantity.get("/min").copied().unwrap();
        assert_eq!(interval.lower, Some(0.0));
        assert_eq!(interval.upper, Some(200.0));
        assert!(!interval.upper_included);
    }

    #[test]
    fn an_unconstrained_declaration_never_widens_a_units_range() {
        let xml = opt(&format!(
            "{}<children xsi:type=\"C_DV_QUANTITY\"><list><units>Cel</units></list></children>",
            quantity_item("Cel", "0", "100", false)
        ));
        let read = LeafConstraints::from_opt(&xml).unwrap();
        let interval = read.quantity.get("Cel").copied().unwrap();
        assert_eq!(interval.upper, Some(100.0));
    }

    #[test]
    fn the_count_range_is_the_intersection_of_every_dv_count_declaration() {
        let xml = opt(&format!(
            "{}{}",
            count_range("0", "100"),
            count_range("1", "31")
        ));
        let read = LeafConstraints::from_opt(&xml).unwrap();
        assert_eq!(read.count.and_then(IntInterval::closed), Some((1, 31)));
    }

    #[test]
    fn an_occurrences_interval_is_never_mistaken_for_a_magnitude() {
        let xml = opt(
            "<children xsi:type=\"C_DV_QUANTITY\"><occurrences><lower>1</lower>\
             <upper>1</upper></occurrences><list><units>Cel</units></list></children>",
        );
        let read = LeafConstraints::from_opt(&xml).unwrap();
        let interval = read.quantity.get("Cel").copied().unwrap();
        assert_eq!(interval.lower, None);
        assert_eq!(interval.upper, None);
    }

    #[test]
    fn a_redrawn_magnitude_stays_inside_its_declared_range_for_every_draw() {
        let interval = RealInterval {
            lower: Some(0.0),
            lower_included: true,
            upper: Some(100.0),
            upper_included: false,
        };
        for draw in 0..5_000_u64 {
            let value = redraw_real(interval, 1, draw.wrapping_mul(2_654_435_761)).unwrap();
            assert!(
                (0.0..100.0).contains(&value),
                "draw {draw} produced {value}, outside [0, 100)"
            );
        }
    }

    #[test]
    fn a_redrawn_count_stays_inside_its_declared_range_for_every_draw() {
        for draw in 0..5_000_u64 {
            let value = redraw_integer(1, 31, draw.wrapping_mul(2_654_435_761)).unwrap();
            assert!((1..=31).contains(&value), "draw {draw} produced {value}");
        }
    }

    #[test]
    fn an_empty_or_inverted_range_redraws_nothing() {
        let inverted = RealInterval {
            lower: Some(10.0),
            lower_included: true,
            upper: Some(1.0),
            upper_included: true,
        };
        assert!(redraw_real(inverted, 1, 7).is_none());
        assert!(redraw_integer(5, 4, 7).is_none());
    }

    #[test]
    fn a_leaf_the_template_says_nothing_about_keeps_its_committed_value() {
        let read =
            LeafConstraints::from_opt(&opt(&quantity_item("Cel", "0", "100", false))).unwrap();
        let mut body = serde_json::json!({
            "_type": "ELEMENT",
            "value": { "_type": "DV_QUANTITY", "magnitude": 7.5, "units": "mm[Hg]" },
            "other": { "_type": "DV_COUNT", "magnitude": 3 }
        });
        let before = body.clone();
        read.apply(&mut body, "cnf.ckm.vital_signs", 4);
        assert_eq!(body, before);
    }

    #[test]
    fn the_same_arrival_of_the_same_template_redraws_the_same_bytes() {
        let read = LeafConstraints::from_opt(&opt(&format!(
            "{}{}",
            quantity_item("Cel", "0", "100", false),
            count_range("1", "31")
        )))
        .unwrap();
        let skeleton = serde_json::json!({
            "_type": "COMPOSITION",
            "content": [
                { "_type": "ELEMENT",
                  "value": { "_type": "DV_QUANTITY", "magnitude": 49.5, "units": "Cel" } },
                { "_type": "ELEMENT",
                  "value": { "_type": "DV_COUNT", "magnitude": 3 } }
            ]
        });
        let render = |arrival: u64| {
            let mut body = skeleton.clone();
            read.apply(&mut body, "cnf.ckm.vital_signs", arrival);
            serde_json::to_vec(&body).unwrap()
        };
        assert_eq!(render(11), render(11));
        assert_ne!(render(11), render(12));
        // The redraw moved BOTH leaf families off the committed value.
        let first: Value = serde_json::from_slice(&render(11)).unwrap();
        assert_ne!(first["content"][0]["value"]["magnitude"], 49.5);
        assert_ne!(first["content"][1]["value"]["magnitude"], 3);
    }

    #[test]
    fn a_malformed_template_is_a_typed_error_not_a_silent_empty_pack() {
        let error = LeafConstraints::from_opt("<template><unclosed>").unwrap_err();
        assert!(matches!(error, ConstraintReadError::Malformed(_)));
    }

    #[test]
    fn a_non_numeric_bound_is_a_typed_error() {
        let xml = opt(&quantity_item("Cel", "zero", "100", true));
        let error = LeafConstraints::from_opt(&xml).unwrap_err();
        assert!(matches!(error, ConstraintReadError::Bound { .. }));
    }
}
