// SPDX-FileCopyrightText: Veredictum contributors
// SPDX-License-Identifier: Apache-2.0

//! Case-scoped execution state.
//!
//! Holds `requires` handles, captures (scalar and
//! list), and committed audit instants — plus the FIXED temporal resolution
//! rules so two runners query identical instants (interpreter law d:
//! before = t − 1 ms, after = t + 1 ms, between = the midpoint).

#![expect(
    clippy::disallowed_types,
    reason = "dev/verification tooling over JSON artifacts (the catalogue, results, wire \
              exchanges), whose shapes belong to the artifacts and the SUT"
)]

use std::collections::BTreeMap;

use crate::ids::CaptureName;
use crate::refgrammar::TimeExpr;

/// A captured value.
#[derive(Debug, Clone, PartialEq)]
pub enum Captured {
    /// A scalar capture (uids, ids, header values).
    Scalar(String),
    /// A list capture (`created.version_uids[]`).
    List(Vec<String>),
    /// A full response body (`ok.body`).
    Body(serde_json::Value),
    /// A committed audit instant, milliseconds since the Unix epoch
    /// (`created.commit_time` — the anchor for temporal at-time cases).
    /// Live commits are only known to an INTERVAL — `lo` = the request-send
    /// instant, `hi` = the response-receipt instant, and the true commit
    /// lies inside it — so `before` resolves from `lo` and `after` from
    /// `hi`, keeping both sound on the wire. A point instant (the
    /// transcript player's recorded ordinals) has `lo == hi`.
    InstantMs {
        /// Earliest millisecond the commit can have happened at.
        lo: i64,
        /// Latest millisecond the commit can have happened at.
        hi: i64,
    },
}

/// The per-row variable store. Reset around every row under
/// `reset_per_row` (law a); carried across rows under `single_pass`.
#[derive(Debug, Clone, Default)]
pub struct VarStore {
    values: BTreeMap<CaptureName, Captured>,
}

impl VarStore {
    /// Bind or overwrite a capture.
    pub fn set(&mut self, name: CaptureName, value: Captured) {
        self.values.insert(name, value);
    }

    /// Look up a capture.
    #[must_use]
    pub fn get(&self, name: &CaptureName) -> Option<&Captured> {
        self.values.get(name)
    }

    /// A scalar view of a capture (lists and bodies are not scalars).
    #[must_use]
    pub fn scalar(&self, name: &CaptureName) -> Option<&str> {
        match self.values.get(name) {
            Some(Captured::Scalar(s)) => Some(s),
            _ => None,
        }
    }

    /// The upper bound of the latest bound commit window, if any (the
    /// driver's temporal-separability pacing reads it).
    #[must_use]
    pub fn latest_instant_hi(&self) -> Option<i64> {
        self.values
            .values()
            .filter_map(|c| match c {
                Captured::InstantMs { hi, .. } => Some(*hi),
                _ => None,
            })
            .max()
    }

    /// Resolve a temporal expression against captured instants (law d).
    ///
    /// # Errors
    /// Returns a message when a referenced capture is missing or not an
    /// instant, or the arithmetic overflows.
    pub fn resolve_time(&self, expr: &TimeExpr) -> Result<i64, String> {
        let instant = |name: &CaptureName| -> Result<(i64, i64), String> {
            match self.values.get(name) {
                Some(Captured::InstantMs { lo, hi }) => Ok((*lo, *hi)),
                Some(_) => Err(format!("capture {name} is not a commit instant")),
                None => Err(format!("capture {name} is not bound")),
            }
        };
        match expr {
            // strictly before the earliest the commit can have happened
            TimeExpr::Before(t) => instant(t)?
                .0
                .checked_sub(1)
                .ok_or_else(|| "instant arithmetic underflow".to_owned()),
            // strictly after the latest the commit can have happened
            TimeExpr::After(t) => instant(t)?
                .1
                .checked_add(1)
                .ok_or_else(|| "instant arithmetic overflow".to_owned()),
            TimeExpr::Between(t1, t2) => {
                // midpoint of the gap between the two commit windows
                let (a, b) = (instant(t1)?.1, instant(t2)?.0);
                #[expect(
                    clippy::integer_division,
                    reason = "midpoint of a millisecond gap: the truncated half is \
                              deliberate, an instant is a whole millisecond"
                )]
                b.checked_sub(a)
                    .and_then(|d| a.checked_add(d / 2))
                    .ok_or_else(|| "instant arithmetic overflow".to_owned())
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn temporal_law_d_is_fixed() {
        let mut store = VarStore::default();
        let t1 = CaptureName::parse("t1").unwrap();
        let t2 = CaptureName::parse("t2").unwrap();
        store.set(
            t1.clone(),
            Captured::InstantMs {
                lo: 1_000,
                hi: 1_000,
            },
        );
        store.set(
            t2.clone(),
            Captured::InstantMs {
                lo: 2_001,
                hi: 2_001,
            },
        );

        assert_eq!(
            store.resolve_time(&TimeExpr::Before(t1.clone())).unwrap(),
            999
        );
        assert_eq!(
            store.resolve_time(&TimeExpr::After(t1.clone())).unwrap(),
            1_001
        );
        assert_eq!(
            store
                .resolve_time(&TimeExpr::Between(t1.clone(), t2))
                .unwrap(),
            1_500 // midpoint, integer division
        );
        assert!(
            store
                .resolve_time(&TimeExpr::Before(CaptureName::parse("ghost").unwrap()))
                .is_err()
        );
    }

    /// A LIVE commit is known only to an interval, so `before` resolves from
    /// the earliest the commit can have happened and `after` from the latest:
    /// both stay sound against a server whose clock sits anywhere inside the
    /// window the runner observed.
    #[test]
    fn a_commit_window_resolves_from_the_sound_end_of_the_interval() {
        let mut store = VarStore::default();
        let t = CaptureName::parse("t").unwrap();
        store.set(
            t.clone(),
            Captured::InstantMs {
                lo: 1_000,
                hi: 1_400,
            },
        );

        assert_eq!(
            store.resolve_time(&TimeExpr::Before(t.clone())).unwrap(),
            999,
            "before is strictly before the earliest possible commit"
        );
        assert_eq!(
            store.resolve_time(&TimeExpr::After(t.clone())).unwrap(),
            1_401,
            "after is strictly after the latest possible commit"
        );

        // The pacing channel reads the newest upper bound across every window.
        assert_eq!(store.latest_instant_hi(), Some(1_400));
        let u = CaptureName::parse("u").unwrap();
        store.set(
            u,
            Captured::InstantMs {
                lo: 9_000,
                hi: 9_100,
            },
        );
        assert_eq!(store.latest_instant_hi(), Some(9_100));
    }

    /// A capture bound to something that is not a commit instant is a typed
    /// failure naming the capture, never a silent zero: a temporal case
    /// resolved against a uid would query an arbitrary instant and pass.
    #[test]
    fn a_non_instant_capture_is_refused_by_name() {
        let mut store = VarStore::default();
        let uid = CaptureName::parse("uid").unwrap();
        store.set(uid.clone(), Captured::Scalar("abc::sys::1".to_owned()));
        let body = CaptureName::parse("body").unwrap();
        store.set(body.clone(), Captured::Body(serde_json::json!({ "a": 1 })));

        let message = store
            .resolve_time(&TimeExpr::After(uid.clone()))
            .expect_err("a scalar capture is not an instant");
        assert_eq!(message, "capture uid is not a commit instant");
        assert!(
            store
                .resolve_time(&TimeExpr::Between(body, uid.clone()))
                .is_err()
        );

        // A store holding no instant at all has no pacing bound to report.
        assert_eq!(store.latest_instant_hi(), None);
        // The scalar view refuses every non-scalar capture, so a list or a
        // body can never be substituted where a scalar is required.
        assert_eq!(store.scalar(&uid), Some("abc::sys::1"));
        let list = CaptureName::parse("uids").unwrap();
        store.set(list.clone(), Captured::List(vec!["a".to_owned()]));
        assert_eq!(store.scalar(&list), None);
        assert_eq!(
            store.get(&list),
            Some(&Captured::List(vec!["a".to_owned()]))
        );
    }

    /// The arithmetic fails loud at the representable edges rather than
    /// wrapping into an instant on the wrong side of the commit.
    #[test]
    fn instant_arithmetic_reports_its_own_overflow() {
        let mut store = VarStore::default();
        let low = CaptureName::parse("low").unwrap();
        store.set(
            low.clone(),
            Captured::InstantMs {
                lo: i64::MIN,
                hi: i64::MIN,
            },
        );
        let high = CaptureName::parse("high").unwrap();
        store.set(
            high.clone(),
            Captured::InstantMs {
                lo: i64::MAX,
                hi: i64::MAX,
            },
        );

        assert_eq!(
            store.resolve_time(&TimeExpr::Before(low.clone())),
            Err("instant arithmetic underflow".to_owned())
        );
        assert_eq!(
            store.resolve_time(&TimeExpr::After(high.clone())),
            Err("instant arithmetic overflow".to_owned())
        );
        assert_eq!(
            store.resolve_time(&TimeExpr::Between(low, high)),
            Err("instant arithmetic overflow".to_owned())
        );
    }
}
