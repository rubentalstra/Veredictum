// SPDX-FileCopyrightText: Veredictum contributors
// SPDX-License-Identifier: Apache-2.0

//! Serde helpers: ordered maps as `Vec<(K, V)>` (authored key order is part
//! of an artifact's meaning — flow `with:` payload order, capture order —
//! and `serde_json`'s `preserve_order` feature carries it through the YAML
//! front-end).

use std::fmt;
use std::marker::PhantomData;
use std::str::FromStr;

use serde::de::{DeserializeOwned, Error as DeError, MapAccess, Visitor};
use serde::{Deserialize, Deserializer};

/// Deserialize a mapping into `Vec<(K, V)>`, rejecting duplicate keys.
///
/// # Errors
/// Fails on a non-mapping value, an unparsable key, or a duplicate key.
pub(crate) fn ordered_map<'de, D, K, V>(deserializer: D) -> Result<Vec<(K, V)>, D::Error>
where
    D: Deserializer<'de>,
    K: FromStr + PartialEq + fmt::Display,
    K::Err: fmt::Display,
    V: DeserializeOwned,
{
    struct MapVisitor<K, V>(PhantomData<(K, V)>);

    impl<'de, K, V> Visitor<'de> for MapVisitor<K, V>
    where
        K: FromStr + PartialEq + fmt::Display,
        K::Err: fmt::Display,
        V: DeserializeOwned,
    {
        type Value = Vec<(K, V)>;

        fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
            f.write_str("a mapping")
        }

        fn visit_map<A: MapAccess<'de>>(self, mut access: A) -> Result<Self::Value, A::Error> {
            let mut entries: Vec<(K, V)> = Vec::new();
            while let Some((key, value)) = access.next_entry::<String, V>()? {
                let key = K::from_str(&key).map_err(A::Error::custom)?;
                if entries.iter().any(|(k, _)| *k == key) {
                    return Err(A::Error::custom(format!("duplicate key {key}")));
                }
                entries.push((key, value));
            }
            Ok(entries)
        }
    }

    deserializer.deserialize_map(MapVisitor(PhantomData))
}

/// [`ordered_map`] wrapped in `Option` for optional mapping fields.
///
/// # Errors
/// As [`ordered_map`].
pub(crate) fn optional_ordered_map<'de, D, K, V>(
    deserializer: D,
) -> Result<Option<Vec<(K, V)>>, D::Error>
where
    D: Deserializer<'de>,
    K: FromStr + PartialEq + fmt::Display,
    K::Err: fmt::Display,
    V: DeserializeOwned,
{
    struct Wrapper<K, V>(Vec<(K, V)>);
    impl<'de, K, V> Deserialize<'de> for Wrapper<K, V>
    where
        K: FromStr + PartialEq + fmt::Display,
        K::Err: fmt::Display,
        V: DeserializeOwned,
    {
        fn deserialize<D2: Deserializer<'de>>(deserializer: D2) -> Result<Self, D2::Error> {
            ordered_map(deserializer).map(Wrapper)
        }
    }
    Ok(Option::<Wrapper<K, V>>::deserialize(deserializer)?.map(|w| w.0))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A carrier over both helpers, keyed by plain strings.
    #[derive(Debug, Deserialize)]
    struct Carrier {
        #[serde(deserialize_with = "ordered_map")]
        required: Vec<(String, u32)>,
        #[serde(default, deserialize_with = "optional_ordered_map")]
        optional: Option<Vec<(String, u32)>>,
    }

    /// Authored key order is part of an artifact's meaning (a flow's `with:`
    /// payload order, a binding's capture order), so the mapping arrives in
    /// the order it was written rather than sorted.
    #[test]
    fn authored_key_order_survives_the_mapping() {
        let carrier: Carrier =
            serde_json::from_str(r#"{"required":{"zeta":1,"alpha":2},"optional":{"one":3}}"#)
                .unwrap();
        assert_eq!(
            carrier.required,
            vec![("zeta".to_owned(), 1), ("alpha".to_owned(), 2)]
        );
        assert_eq!(carrier.optional, Some(vec![("one".to_owned(), 3)]));
    }

    /// An absent optional mapping is `None`, distinct from an empty one: a key
    /// the artifact never wrote is not a key it wrote as empty.
    #[test]
    fn an_absent_optional_mapping_is_none_not_empty() {
        let absent: Carrier = serde_json::from_str(r#"{"required":{}}"#).unwrap();
        assert_eq!(absent.optional, None);
        let empty: Carrier = serde_json::from_str(r#"{"required":{},"optional":{}}"#).unwrap();
        assert_eq!(empty.optional, Some(Vec::new()));
    }

    /// A duplicate key is refused by name. Silently keeping the last value
    /// would let an artifact state one thing and mean another, with nothing
    /// to catch it.
    #[test]
    fn a_duplicate_key_is_refused_by_name() {
        let error = serde_json::from_str::<Carrier>(r#"{"required":{"same":1,"same":2}}"#)
            .expect_err("a duplicate key is refused");
        assert!(error.to_string().contains("duplicate key same"), "{error}");
    }

    /// A non-mapping value says what it expected, so an artifact that writes a
    /// list where a mapping belongs names the shape rather than the type.
    #[test]
    fn a_non_mapping_value_names_the_shape_it_expected() {
        let error = serde_json::from_str::<Carrier>(r#"{"required":[]}"#)
            .expect_err("a list is not a mapping");
        assert!(error.to_string().contains("a mapping"), "{error}");
    }
}
