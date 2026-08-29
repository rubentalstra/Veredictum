// SPDX-FileCopyrightText: Veredictum contributors
// SPDX-License-Identifier: Apache-2.0

//! Templated values — the payload shapes of `flow[].with` and assertion
//! operands.
//!
//! A value tree is JSON-shaped; every string in it is parsed as a
//! [`Template`], so an illegal `${…}` reference anywhere in a case is a
//! load-time error (the reference/sentinel grammar check of the schedule's
//! CI gate list).

#![expect(
    clippy::disallowed_types,
    reason = "dev/verification tooling over JSON artifacts (the catalogue, results, wire \
              exchanges), whose shapes belong to the artifacts and the SUT"
)]

use serde::de::Error as DeError;
use serde::{Deserialize, Deserializer};

use crate::refgrammar::{RefError, Template, ValueRef};

/// A JSON-shaped value whose strings are reference templates.
#[derive(Debug, Clone, PartialEq)]
pub enum TemplatedValue {
    /// JSON `null`.
    Null,
    /// A JSON boolean.
    Bool(bool),
    /// A JSON number, kept in its authored lexical form.
    Number(serde_json::Number),
    /// A string carrying `${…}` references, resolved per row.
    Text(Template),
    /// A JSON array of templated values.
    Seq(Vec<TemplatedValue>),
    /// Key order preserved as authored.
    Map(Vec<(String, TemplatedValue)>),
}

impl TemplatedValue {
    /// Convert from a parsed JSON value, validating every embedded string.
    ///
    /// # Errors
    /// Returns [`RefError`] for any illegal `${…}` reference.
    pub fn from_value(value: &serde_json::Value) -> Result<Self, RefError> {
        Ok(match value {
            serde_json::Value::Null => Self::Null,
            serde_json::Value::Bool(b) => Self::Bool(*b),
            serde_json::Value::Number(n) => Self::Number(n.clone()),
            serde_json::Value::String(s) => Self::Text(Template::parse(s)?),
            serde_json::Value::Array(items) => Self::Seq(
                items
                    .iter()
                    .map(Self::from_value)
                    .collect::<Result<_, _>>()?,
            ),
            serde_json::Value::Object(map) => Self::Map(
                map.iter()
                    .map(|(k, v)| Ok((k.clone(), Self::from_value(v)?)))
                    .collect::<Result<_, RefError>>()?,
            ),
        })
    }

    /// This value as plain JSON, when it carries no `${…}` reference anywhere.
    ///
    /// A reference-free tree needs no resolver: every string is its own
    /// literal, so the answer is byte-identical to what
    /// `Resolver::resolve_value` would produce for it. `None` says the tree
    /// carries a reference, and the caller must resolve it (or refuse).
    #[must_use]
    pub fn literal(&self) -> Option<serde_json::Value> {
        Some(match self {
            Self::Null => serde_json::Value::Null,
            Self::Bool(b) => serde_json::Value::Bool(*b),
            Self::Number(n) => serde_json::Value::Number(n.clone()),
            Self::Text(t) => {
                if t.refs().next().is_some() {
                    return None;
                }
                serde_json::Value::String(t.raw().to_owned())
            }
            Self::Seq(items) => serde_json::Value::Array(
                items
                    .iter()
                    .map(Self::literal)
                    .collect::<Option<Vec<_>>>()?,
            ),
            Self::Map(entries) => {
                let mut map = serde_json::Map::new();
                for (key, value) in entries {
                    map.insert(key.clone(), value.literal()?);
                }
                serde_json::Value::Object(map)
            }
        })
    }

    /// Every reference anywhere in the tree.
    #[must_use]
    pub fn refs(&self) -> Vec<&ValueRef> {
        let mut out = Vec::new();
        self.collect_refs(&mut out);
        out
    }

    fn collect_refs<'a>(&'a self, out: &mut Vec<&'a ValueRef>) {
        match self {
            Self::Text(t) => out.extend(t.refs()),
            Self::Seq(items) => {
                for item in items {
                    item.collect_refs(out);
                }
            }
            Self::Map(entries) => {
                for (_, v) in entries {
                    v.collect_refs(out);
                }
            }
            Self::Null | Self::Bool(_) | Self::Number(_) => {}
        }
    }
}

impl<'de> Deserialize<'de> for TemplatedValue {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let value = serde_json::Value::deserialize(deserializer)?;
        Self::from_value(&value).map_err(D::Error::custom)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strings_validate_everywhere() {
        let ok = serde_json::json!({
            "versions": [
                { "data": "${ds:cnf.composition.minimal_event.v1}", "change_type": "creation" },
                { "data": "${ds:cnf.composition.minimal_event.v2}", "preceding_version_uid": "${v1}" }
            ]
        });
        let v = TemplatedValue::from_value(&ok).unwrap();
        assert_eq!(v.refs().len(), 3);

        let bad = serde_json::json!({ "nested": [{ "x": "${step2.body}" }] });
        assert!(TemplatedValue::from_value(&bad).is_err());
    }

    /// Every JSON scalar shape survives the conversion, and none of them
    /// carries a reference — so a `null`, a boolean or a number in a `with:`
    /// block resolves to itself rather than to a missing binding.
    #[test]
    fn scalar_values_convert_and_carry_no_references() {
        let value = TemplatedValue::from_value(&serde_json::json!({
            "absent": serde_json::Value::Null,
            "flag": true,
            "count": 3
        }))
        .unwrap();
        assert!(value.refs().is_empty());
        let TemplatedValue::Map(entries) = &value else {
            panic!("expected a map")
        };
        assert_eq!(
            entries.iter().map(|(k, _)| k.as_str()).collect::<Vec<_>>(),
            ["absent", "flag", "count"]
        );
        assert_eq!(entries.first().map(|(_, v)| v), Some(&TemplatedValue::Null));
    }

    /// A reference-free tree reads as its own JSON literal; one reference
    /// anywhere in it, however deep, withholds the literal so the caller has
    /// to resolve it or refuse.
    #[test]
    fn only_a_reference_free_tree_reads_as_a_literal() {
        let literal = TemplatedValue::from_value(&serde_json::json!({
            "value": "other care",
            "count": 3,
            "flags": [true, serde_json::Value::Null]
        }))
        .unwrap();
        assert_eq!(
            literal.literal(),
            Some(serde_json::json!({
                "value": "other care",
                "count": 3,
                "flags": [true, serde_json::Value::Null]
            }))
        );

        let referencing =
            TemplatedValue::from_value(&serde_json::json!({ "uid": "${first_ehr_id}" })).unwrap();
        assert_eq!(referencing.literal(), None);

        let nested =
            TemplatedValue::from_value(&serde_json::json!({ "a": [{ "b": "x${v1}y" }] })).unwrap();
        assert_eq!(nested.literal(), None);
    }
}
