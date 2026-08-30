// SPDX-FileCopyrightText: Veredictum contributors
// SPDX-License-Identifier: Apache-2.0

//! The normative AQL `RESULT_SET` equivalence comparator.
//!
//! The schedule's
//! `RESULT_SET` equivalence rules — each rule is either **\[spec\]**, cited to
//! the vendored QUERY/ITS-REST text, or **\[legislated\]**, a fixed proposed
//! default awaiting upstream ratification; both are implemented exactly as
//! specified.
//!
//! Rules implemented here:
//! 1. Comparison scope is `rows` only; `meta` is always excluded (\[spec\]:
//!    every `ResultSetMetadata` field is optional and implementation
//!    dependent — ITS-REST `schemas/query/ResultSetMetadata.yaml`).
//!    `columns` compare only when the case asserts them (\[spec\]:
//!    `ResultSet.yaml` requires only `rows`); column identity is the `AS`
//!    alias, else `#<0-based index>` (\[spec\]: `ResultSetColumn.yaml`).
//! 2. `ordered` = sequence equality (legal only under a totally-ordering
//!    ORDER BY — enforced at authoring); `set` = BAG (multiset) equality
//!    (\[spec\]: AQL is bag-semantics absent DISTINCT — QUERY
//!    `master03-syntax.adoc` §DISTINCT); `count` = row count; `contains` =
//!    every expected row appears bag-wise, extra rows permitted.
//! 3. Cell equality: an RM-object cell (carries `_type`) compares by
//!    canonical-JSON structural equality (\[spec\]: QUERY
//!    `master04-result_structure.adoc`); a scalar numeric cell compares by
//!    NUMERIC VALUE, not lexeme — `140` = `140.0` (\[legislated\]); a void
//!    cell is JSON `null` and equals only `null` (\[legislated\]); every
//!    other cell compares by exact lexeme unless the case declares
//!    [`crate::vocab::CellComparison::Instant`] (\[spec\]: ITS-REST
//!    `specifications/docs/overview/Resources.md` §Datetime format puts
//!    query-side date/time SPELLING at SHOULD-strength, while BASE
//!    `UML/classes/iso8601_timezone.adoc` §Description equates the two UTC
//!    spellings — "`Z` is a literal meaning UTC (modern replacement for
//!    GMT), i.e. timezone `+0000`").

#![expect(
    clippy::disallowed_types,
    reason = "dev/verification tooling over JSON artifacts (the catalogue, results, wire \
              exchanges), whose shapes belong to the artifacts and the SUT"
)]

use serde_json::Value;

use crate::vocab::CellComparison;

/// One comparison failure, human-readable and stable.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResultSetMismatch(pub String);

/// One tolerated lexical divergence recorded under
/// [`CellComparison::Instant`]: the served cell denotes the expected instant
/// and spells it differently.
///
/// The row still passes, because the sentence the spelling would be judged
/// against is SHOULD-strength (ITS-REST
/// `specifications/docs/overview/Resources.md` §Datetime format). The
/// divergence is carried out of the comparison so the run reports it instead
/// of swallowing it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InstantDivergence {
    /// The lexeme the case expects.
    pub expected: String,
    /// The lexeme the system under test served.
    pub served: String,
}

impl std::fmt::Display for InstantDivergence {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "date/time cell served as {:?} where the case spells the same instant {:?}",
            self.served, self.expected
        )
    }
}

/// Cell comparison under a declared [`CellComparison`], accumulating the
/// divergences that mode tolerates.
#[derive(Debug, Default)]
pub struct CellComparator {
    mode: CellComparison,
    divergences: Vec<InstantDivergence>,
}

impl CellComparator {
    /// Creates a comparator for the given mode.
    #[must_use]
    pub fn new(mode: CellComparison) -> Self {
        Self {
            mode,
            divergences: Vec::new(),
        }
    }

    /// The tolerated divergences recorded so far, in comparison order.
    #[must_use]
    pub fn divergences(&self) -> &[InstantDivergence] {
        &self.divergences
    }

    /// Cell equality under rule 3 and this comparator's mode.
    pub fn equal(&mut self, a: &Value, b: &Value) -> bool {
        match (a, b) {
            // numeric-value equality, not lexeme (\[legislated\])
            (Value::Number(x), Value::Number(y)) => match (x.as_f64(), y.as_f64()) {
                (Some(x), Some(y)) => (x - y).abs() == 0.0,
                _ => x == y,
            },
            // void equals only void (\[legislated\])
            (Value::Null, Value::Null) => true,
            // structural equality, numeric leaves by value, recursively
            (Value::Array(x), Value::Array(y)) => {
                if x.len() != y.len() {
                    return false;
                }
                for (a, b) in x.iter().zip(y) {
                    if !self.equal(a, b) {
                        return false;
                    }
                }
                true
            }
            (Value::Object(x), Value::Object(y)) => {
                if x.len() != y.len() {
                    return false;
                }
                for (k, va) in x {
                    let Some(vb) = y.get(k) else {
                        return false;
                    };
                    if !self.equal(va, vb) {
                        return false;
                    }
                }
                true
            }
            (Value::String(x), Value::String(y)) if x != y => self.same_instant(x, y),
            _ => a == b,
        }
    }

    /// Whether two differing date/time lexemes denote the same instant under
    /// [`CellComparison::Instant`], recording the divergence when they do.
    fn same_instant(&mut self, served: &str, expected: &str) -> bool {
        if self.mode != CellComparison::Instant {
            return false;
        }
        // `Timestamp` refuses an offset-less string, which is the case
        // ITS-REST Resources.md §Datetime format leaves to "the local
        // timezone" and no comparison here can resolve.
        let (Ok(left), Ok(right)) = (
            served.parse::<jiff::Timestamp>(),
            expected.parse::<jiff::Timestamp>(),
        ) else {
            return false;
        };
        if left != right {
            return false;
        }
        self.divergences.push(InstantDivergence {
            expected: expected.to_owned(),
            served: served.to_owned(),
        });
        true
    }

    /// The divergence count at this point, for [`CellComparator::rollback`].
    fn mark(&self) -> usize {
        self.divergences.len()
    }

    /// Discards every divergence recorded since `mark` — a bag probe that did
    /// not match must leave nothing behind.
    fn rollback(&mut self, mark: usize) {
        self.divergences.truncate(mark);
    }
}

/// Cell equality under rule 3, with exact lexeme comparison
/// ([`CellComparison::Lexeme`]).
#[must_use]
pub fn cells_equal(a: &Value, b: &Value) -> bool {
    CellComparator::default().equal(a, b)
}

fn rows_of(result_set: &Value) -> Result<&Vec<Value>, ResultSetMismatch> {
    result_set
        .get("rows")
        .and_then(Value::as_array)
        .ok_or_else(|| {
            ResultSetMismatch(
                "response carries no rows array (ResultSet.yaml requires rows)".to_owned(),
            )
        })
}

fn row_equal(cmp: &mut CellComparator, a: &Value, b: &Value) -> bool {
    match (a.as_array(), b.as_array()) {
        (Some(a), Some(b)) => {
            if a.len() != b.len() {
                return false;
            }
            for (x, y) in a.iter().zip(b) {
                if !cmp.equal(x, y) {
                    return false;
                }
            }
            true
        }
        _ => cmp.equal(a, b),
    }
}

/// Bag containment: every expected row appears in `actual` (multiplicity
/// respected). Returns the leftover-unmatched expected row on failure.
fn bag_contains(
    cmp: &mut CellComparator,
    actual: &[Value],
    expected: &[Value],
) -> Result<(), ResultSetMismatch> {
    let mut used = vec![false; actual.len()];
    for (i, want) in expected.iter().enumerate() {
        let mut found = None;
        for (j, got) in actual.iter().enumerate() {
            if used.get(j) != Some(&false) {
                continue;
            }
            // A probe that does NOT match must leave no divergence behind:
            // only the row actually paired with `want` speaks about the SUT.
            let mark = cmp.mark();
            if row_equal(cmp, got, want) {
                found = Some(j);
                break;
            }
            cmp.rollback(mark);
        }
        match found {
            Some(j) => {
                if let Some(slot) = used.get_mut(j) {
                    *slot = true;
                }
            }
            None => {
                return Err(ResultSetMismatch(format!(
                    "expected row {i} has no bag-wise match: {want}"
                )));
            }
        }
    }
    Ok(())
}

/// `match: ordered` — sequence equality over rows.
///
/// # Errors
/// The first mismatch, positional.
pub fn compare_ordered(
    result_set: &Value,
    expected: &[Value],
    cmp: &mut CellComparator,
) -> Result<(), ResultSetMismatch> {
    let rows = rows_of(result_set)?;
    if rows.len() != expected.len() {
        return Err(ResultSetMismatch(format!(
            "row count {} != expected {}",
            rows.len(),
            expected.len()
        )));
    }
    for (i, (got, want)) in rows.iter().zip(expected).enumerate() {
        if !row_equal(cmp, got, want) {
            return Err(ResultSetMismatch(format!(
                "row {i}: {got} != expected {want}"
            )));
        }
    }
    Ok(())
}

/// `match: set` — bag (multiset) equality: duplicates are significant.
///
/// # Errors
/// The unmatched row (either direction).
pub fn compare_bag(
    result_set: &Value,
    expected: &[Value],
    cmp: &mut CellComparator,
) -> Result<(), ResultSetMismatch> {
    let rows = rows_of(result_set)?;
    if rows.len() != expected.len() {
        return Err(ResultSetMismatch(format!(
            "row count {} != expected {} (bag equality)",
            rows.len(),
            expected.len()
        )));
    }
    bag_contains(cmp, rows, expected)
}

/// `match: contains` — every expected row appears bag-wise; extras allowed.
///
/// # Errors
/// The unmatched expected row.
pub fn compare_contains(
    result_set: &Value,
    expected: &[Value],
    cmp: &mut CellComparator,
) -> Result<(), ResultSetMismatch> {
    bag_contains(cmp, rows_of(result_set)?, expected)
}

/// `match: count` — row count only.
///
/// # Errors
/// The count mismatch.
pub fn compare_count(result_set: &Value, expected: u64) -> Result<(), ResultSetMismatch> {
    #[expect(
        clippy::as_conversions,
        reason = "row count widens exactly: usize is at most 64 bits on every supported target"
    )]
    let n = rows_of(result_set)?.len() as u64;
    if n == expected {
        Ok(())
    } else {
        Err(ResultSetMismatch(format!(
            "row count {n} != expected {expected}"
        )))
    }
}

/// Column identity check (rule 1): asserted names against the `AS` alias,
/// else `#<index>`.
///
/// # Errors
/// The first name mismatch, or a missing columns array when asserted.
pub fn compare_columns(result_set: &Value, expected: &[String]) -> Result<(), ResultSetMismatch> {
    let columns = result_set
        .get("columns")
        .and_then(Value::as_array)
        .ok_or_else(|| {
            ResultSetMismatch("columns asserted but the response carries none".to_owned())
        })?;
    if columns.len() != expected.len() {
        return Err(ResultSetMismatch(format!(
            "column count {} != expected {}",
            columns.len(),
            expected.len()
        )));
    }
    for (i, (col, want)) in columns.iter().zip(expected).enumerate() {
        let name = col
            .get("name")
            .and_then(Value::as_str)
            .map_or_else(|| format!("#{i}"), ToOwned::to_owned);
        if &name != want {
            return Err(ResultSetMismatch(format!(
                "column {i}: {name:?} != expected {want:?}"
            )));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn numeric_value_not_lexeme() {
        assert!(cells_equal(&json!(140), &json!(140.0)));
        assert!(!cells_equal(&json!(140), &json!(140.5)));
        assert!(cells_equal(&json!(null), &json!(null)));
        assert!(!cells_equal(&json!(null), &json!(0)));
    }

    #[test]
    fn rm_object_cells_compare_structurally_with_numeric_leaves() {
        let a = json!({"_type": "DV_QUANTITY", "magnitude": 140, "units": "mm[Hg]"});
        let b = json!({"_type": "DV_QUANTITY", "magnitude": 140.0, "units": "mm[Hg]"});
        assert!(cells_equal(&a, &b));
    }

    fn lexeme() -> CellComparator {
        CellComparator::default()
    }

    fn instant() -> CellComparator {
        CellComparator::new(CellComparison::Instant)
    }

    #[test]
    fn bag_semantics_respect_duplicates() {
        let rs = json!({"rows": [["a"], ["a"], ["b"]], "meta": {"_href": "ignored"}});
        assert!(
            compare_bag(
                &rs,
                &[json!(["a"]), json!(["b"]), json!(["a"])],
                &mut lexeme()
            )
            .is_ok()
        );
        // duplicate significance: two a's expected, one present
        let rs2 = json!({"rows": [["a"], ["b"], ["b"]]});
        assert!(
            compare_bag(
                &rs2,
                &[json!(["a"]), json!(["a"]), json!(["b"])],
                &mut lexeme()
            )
            .is_err()
        );
    }

    #[test]
    fn ordered_contains_count_and_columns() {
        let rs = json!({
            "columns": [{"name": "uid"}],
            "rows": [["u1"], ["u2"], ["u3"]]
        });
        assert!(
            compare_ordered(
                &rs,
                &[json!(["u1"]), json!(["u2"]), json!(["u3"])],
                &mut lexeme()
            )
            .is_ok()
        );
        assert!(
            compare_ordered(
                &rs,
                &[json!(["u2"]), json!(["u1"]), json!(["u3"])],
                &mut lexeme()
            )
            .is_err()
        );
        assert!(compare_contains(&rs, &[json!(["u3"]), json!(["u1"])], &mut lexeme()).is_ok());
        assert!(compare_count(&rs, 3).is_ok());
        assert!(compare_columns(&rs, &["uid".to_owned()]).is_ok());
        assert!(compare_columns(&rs, &["id".to_owned()]).is_err());
    }

    /// Rule 1: comparison scope is `rows`, and `ResultSet.yaml` requires it —
    /// a response carrying none is a mismatch every comparator reports the
    /// same way, never an empty set that silently satisfies a `count: 0`.
    #[test]
    fn a_response_without_rows_fails_every_comparator() {
        let no_rows = json!({ "meta": { "_href": "x" }, "columns": [] });
        for mismatch in [
            compare_ordered(&no_rows, &[], &mut lexeme()).expect_err("ordered needs rows"),
            compare_bag(&no_rows, &[], &mut lexeme()).expect_err("bag needs rows"),
            compare_contains(&no_rows, &[], &mut lexeme()).expect_err("contains needs rows"),
            compare_count(&no_rows, 0).expect_err("count needs rows"),
        ] {
            assert!(
                mismatch.0.contains("ResultSet.yaml requires rows"),
                "{mismatch:?}"
            );
        }
        assert!(compare_count(&json!({ "rows": 3 }), 0).is_err());
    }

    /// Each comparator names its own count mismatch, so a red row says which
    /// rule it violated rather than only that something differed.
    #[test]
    fn each_comparator_reports_its_own_count_mismatch() {
        let rs = json!({ "rows": [["a"], ["b"]] });

        let ordered =
            compare_ordered(&rs, &[json!(["a"])], &mut lexeme()).expect_err("2 rows are not 1");
        assert_eq!(ordered.0, "row count 2 != expected 1");

        let bag = compare_bag(&rs, &[json!(["a"])], &mut lexeme()).expect_err("2 rows are not 1");
        assert_eq!(bag.0, "row count 2 != expected 1 (bag equality)");

        let count = compare_count(&rs, 5).expect_err("2 rows are not 5");
        assert_eq!(count.0, "row count 2 != expected 5");

        // `contains` permits extras by construction, so the same input holds.
        assert!(compare_contains(&rs, &[json!(["a"])], &mut lexeme()).is_ok());
        let unmatched = compare_contains(&rs, &[json!(["z"])], &mut lexeme())
            .expect_err("z is in no row of the set");
        assert!(
            unmatched
                .0
                .starts_with("expected row 0 has no bag-wise match")
        );
    }

    /// Column identity is the `AS` alias, else `#<0-based index>`
    /// (`ResultSetColumn.yaml`), and columns compare only when the case asserts
    /// them — so an assertion against a response carrying none is a mismatch.
    #[test]
    fn column_identity_falls_back_to_the_positional_name() {
        let unaliased = json!({ "columns": [{ "path": "/uid" }, { "name": "sys" }], "rows": [] });
        assert!(
            compare_columns(&unaliased, &["#0".to_owned(), "sys".to_owned()]).is_ok(),
            "an unaliased column is identified by its 0-based index"
        );

        let count =
            compare_columns(&unaliased, &["#0".to_owned()]).expect_err("2 columns are not 1");
        assert_eq!(count.0, "column count 2 != expected 1");

        let missing = compare_columns(&json!({ "rows": [] }), &["uid".to_owned()])
            .expect_err("columns were asserted");
        assert_eq!(missing.0, "columns asserted but the response carries none");
    }

    /// Rule 3 applies recursively: a nested list of RM objects compares
    /// element-wise with numeric leaves by value, and a length difference at
    /// any depth is a difference.
    #[test]
    fn nested_list_cells_compare_element_wise() {
        let a = json!([{ "_type": "DV_COUNT", "magnitude": 1 }, { "_type": "DV_COUNT", "magnitude": 2 }]);
        let b = json!([{ "_type": "DV_COUNT", "magnitude": 1.0 }, { "_type": "DV_COUNT", "magnitude": 2.0 }]);
        assert!(cells_equal(&a, &b));

        let shorter = json!([{ "_type": "DV_COUNT", "magnitude": 1 }]);
        assert!(!cells_equal(&a, &shorter));

        assert!(!cells_equal(
            &json!({ "a": 1, "b": 2 }),
            &json!({ "a": 1, "c": 2 })
        ));

        // A scalar row (not an array) still compares as one cell.
        let rs = json!({ "rows": ["u1", "u2"] });
        assert!(compare_ordered(&rs, &[json!("u1"), json!("u2")], &mut lexeme()).is_ok());
        let positional = compare_ordered(&rs, &[json!("u1"), json!("u9")], &mut lexeme())
            .expect_err("row 1 differs");
        assert!(positional.0.starts_with("row 1:"), "{positional:?}");
    }
    /// The default stays exact-lexeme, which is the only comparison that can
    /// test the write-path preservation sentence (ITS-REST
    /// `docs/overview/Resources.md` §Datetime format: a body value "will be
    /// preserved as it was sent by the client").
    #[test]
    fn the_default_mode_refuses_a_respelled_instant() {
        let rs = json!({"rows": [["2026-01-01T00:00:00+00:00"]]});
        assert!(compare_ordered(&rs, &[json!(["2026-01-01T00:00:00Z"])], &mut lexeme()).is_err());
    }

    /// `Z` and `+00:00` denote one instant (BASE
    /// `UML/classes/iso8601_timezone.adoc` §Description: "`Z` is a literal
    /// meaning UTC …, i.e. timezone `+0000`"), so the instant mode passes the
    /// row AND records the divergence.
    #[test]
    fn instant_mode_passes_a_respelled_offset_and_records_it() {
        let rs = json!({"rows": [["2026-01-01T00:00:00+00:00"], ["2026-01-01T09:00:00.000Z"]]});
        let mut cmp = instant();
        assert!(
            compare_ordered(
                &rs,
                &[
                    json!(["2026-01-01T00:00:00Z"]),
                    json!(["2026-01-01T09:00:00Z"])
                ],
                &mut cmp
            )
            .is_ok()
        );
        assert_eq!(cmp.divergences().len(), 2);
        assert_eq!(
            cmp.divergences().first().map(|d| d.served.as_str()),
            Some("2026-01-01T00:00:00+00:00")
        );
        assert!(cmp.divergences().iter().all(|d| !d.to_string().is_empty()));
    }

    /// The mode never widens beyond the instant: a different instant, a
    /// non-date/time string, and a lexeme with no timezone at all (which
    /// Resources.md §Datetime format leaves to "the local timezone") all keep
    /// failing.
    #[test]
    fn instant_mode_never_widens_past_the_instant() {
        let mut cmp = instant();
        let other_instant = json!({"rows": [["2026-01-01T01:00:00Z"]]});
        assert!(
            compare_ordered(&other_instant, &[json!(["2026-01-01T00:00:00Z"])], &mut cmp).is_err()
        );
        let text = json!({"rows": [["blood pressure 1"]]});
        assert!(compare_ordered(&text, &[json!(["blood pressure 0"])], &mut cmp).is_err());
        let no_zone = json!({"rows": [["2026-01-01T00:00:00"]]});
        assert!(compare_ordered(&no_zone, &[json!(["2026-01-01T00:00:00Z"])], &mut cmp).is_err());
        assert!(cmp.divergences().is_empty());
    }

    /// The mode reaches a date/time LEAF of an RM-object cell, and a bag probe
    /// that does not match leaves no divergence behind.
    #[test]
    fn instant_mode_reaches_rm_leaves_and_rolls_back_failed_probes() {
        let rs = json!({"rows": [
            [{"_type": "DV_DATE_TIME", "value": "2026-01-01T09:00:00+00:00"}],
            [{"_type": "DV_DATE_TIME", "value": "2026-01-01T00:00:00Z"}]
        ]});
        let mut cmp = instant();
        assert!(
            compare_bag(
                &rs,
                &[
                    json!([{"_type": "DV_DATE_TIME", "value": "2026-01-01T00:00:00Z"}]),
                    json!([{"_type": "DV_DATE_TIME", "value": "2026-01-01T09:00:00Z"}])
                ],
                &mut cmp
            )
            .is_ok()
        );
        // The probe against the already-matching row is rolled back, so only
        // the one lexically diverging pairing is counted.
        assert_eq!(cmp.divergences().len(), 1, "{:?}", cmp.divergences());
        assert_eq!(
            cmp.divergences().first().map(|d| d.expected.as_str()),
            Some("2026-01-01T09:00:00Z")
        );
    }
}
