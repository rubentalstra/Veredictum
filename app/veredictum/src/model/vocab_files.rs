// SPDX-FileCopyrightText: Veredictum contributors
// SPDX-License-Identifier: Apache-2.0

//! The published vocabulary artifacts (`vocab/outcomes.yaml`,
//! `vocab/selectors.yaml`).
//!
//! The Rust enums in [`crate::vocab`] are the compiled form; these files are
//! the published normative form. The validator holds them equal in both
//! directions (a kind in the file the enum lacks, or vice versa, is an
//! error) so the artifact and the reference implementation cannot drift.

use serde::Deserialize;

use crate::vocab::{IgnoreSetName, OutcomeKind};

/// One published outcome-kind row.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct OutcomeRow {
    /// `success` | `error`.
    pub class: OutcomeClassToken,
    /// The schedule language for the kind.
    pub meaning: String,
}

/// The published class token.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum OutcomeClassToken {
    /// The outcome is a successful completion of the operation.
    Success,
    /// The outcome is a refusal or failure.
    Error,
}

/// `vocab/outcomes.yaml` — kind token → row.
#[derive(Debug, Clone)]
pub struct OutcomesVocab {
    entries: Vec<(String, OutcomeRow)>,
}

impl OutcomesVocab {
    /// All entries in authored order.
    #[must_use]
    pub fn entries(&self) -> &[(String, OutcomeRow)] {
        &self.entries
    }

    /// Two-way equality with the compiled enum, including class agreement.
    ///
    /// # Errors
    /// Returns every drift found.
    pub fn check_against_enum(&self) -> Result<(), Vec<String>> {
        let mut findings = Vec::new();
        for (token, row) in &self.entries {
            match OutcomeKind::from_token(token) {
                None => findings.push(format!("outcomes.yaml lists unknown kind {token:?}")),
                Some(kind) => {
                    let enum_class = matches!(kind.class(), crate::vocab::OutcomeClass::Success);
                    let file_class = row.class == OutcomeClassToken::Success;
                    if enum_class != file_class {
                        findings.push(format!(
                            "outcomes.yaml class for {token:?} disagrees with the compiled taxonomy"
                        ));
                    }
                }
            }
        }
        for kind in OutcomeKind::ALL {
            if !self.entries.iter().any(|(t, _)| t == kind.token()) {
                findings.push(format!("outcomes.yaml is missing kind {:?}", kind.token()));
            }
        }
        if findings.is_empty() {
            Ok(())
        } else {
            Err(findings)
        }
    }
}

impl<'de> Deserialize<'de> for OutcomesVocab {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let entries = crate::model::de::ordered_map(deserializer)?;
        Ok(Self { entries })
    }
}

/// A named ignore-set declaration in `vocab/selectors.yaml`.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct IgnoreSetDecl {
    /// Enumerated paths (the `ctx_defaults` set); absent for per-binding
    /// sets.
    #[serde(default)]
    pub paths: Vec<String>,
    /// True when membership is enumerated per operation binding
    /// (`server_assigned`).
    #[serde(default)]
    pub per_binding: bool,
    /// Citation for the set's normative source.
    pub source: String,
}

/// `vocab/selectors.yaml` — the published selector/matcher vocabularies +
/// the named ignore-set registry.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SelectorsVocab {
    /// The body-selector tokens (must equal the compiled enum).
    pub body_selectors: Vec<String>,
    /// The header-matcher forms (must equal the compiled vocabulary).
    pub header_matchers: Vec<String>,
    /// The named ignore-sets.
    #[serde(deserialize_with = "crate::model::de::ordered_map")]
    pub ignore_sets: Vec<(IgnoreSetKey, IgnoreSetDecl)>,
    /// Cross-cutting outcome kinds mapped once for the whole route table
    /// (the overview's global status-code rules), instead of per binding.
    #[serde(default, deserialize_with = "crate::model::de::optional_ordered_map")]
    pub universal_outcomes: Option<Vec<(String, UniversalOutcome)>>,
}

/// One universal outcome mapping (kind token -> wire status + citation).
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct UniversalOutcome {
    /// The wire status realizing the kind on every route.
    pub status: u16,
    /// The overview citation making the rule route-table-wide.
    pub source: String,
}

/// Key newtype for the ignore-set map.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct IgnoreSetKey(pub IgnoreSetName);

impl std::str::FromStr for IgnoreSetKey {
    type Err = String;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "server_assigned" => Ok(Self(IgnoreSetName::ServerAssigned)),
            "ctx_defaults" => Ok(Self(IgnoreSetName::CtxDefaults)),
            _ => Err(format!("{s:?} is not a named ignore-set")),
        }
    }
}

impl std::fmt::Display for IgnoreSetKey {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self.0 {
            IgnoreSetName::ServerAssigned => "server_assigned",
            IgnoreSetName::CtxDefaults => "ctx_defaults",
        })
    }
}

/// The compiled body-selector tokens, in vocabulary order.
pub const BODY_SELECTOR_TOKENS: &[&str] = &[
    "prefer_conditional",
    "error_loose",
    "result_set_body",
    "negotiated",
    "present",
    "absent",
];

/// The compiled header-matcher forms, in vocabulary order.
pub const HEADER_MATCHER_FORMS: &[&str] = &[
    "present",
    "present?",
    "absent",
    "negotiated",
    "latest-version-uid",
    "pattern:<regex>",
    "<literal>",
];

impl SelectorsVocab {
    /// Two-way equality with the compiled vocabularies, plus ignore-set
    /// shape (per-binding sets carry no paths; enumerated sets carry some).
    ///
    /// # Errors
    /// Returns every drift found.
    pub fn check_against_enum(&self) -> Result<(), Vec<String>> {
        let mut findings = Vec::new();
        if self.body_selectors != BODY_SELECTOR_TOKENS {
            findings.push(format!(
                "selectors.yaml body_selectors {:?} != the compiled vocabulary {BODY_SELECTOR_TOKENS:?}",
                self.body_selectors
            ));
        }
        if self.header_matchers != HEADER_MATCHER_FORMS {
            findings.push(format!(
                "selectors.yaml header_matchers {:?} != the compiled vocabulary {HEADER_MATCHER_FORMS:?}",
                self.header_matchers
            ));
        }
        for (key, decl) in &self.ignore_sets {
            match key.0 {
                IgnoreSetName::ServerAssigned => {
                    if !decl.per_binding || !decl.paths.is_empty() {
                        findings.push(
                            "server_assigned must be per_binding with no enumerated paths"
                                .to_owned(),
                        );
                    }
                }
                IgnoreSetName::CtxDefaults => {
                    if decl.per_binding || decl.paths.is_empty() {
                        findings.push(
                            "ctx_defaults must enumerate its paths (not per_binding)".to_owned(),
                        );
                    }
                }
            }
        }
        for name in [IgnoreSetName::ServerAssigned, IgnoreSetName::CtxDefaults] {
            if !self.ignore_sets.iter().any(|(k, _)| k.0 == name) {
                findings.push(format!("selectors.yaml is missing ignore-set {name:?}"));
            }
        }
        if findings.is_empty() {
            Ok(())
        } else {
            Err(findings)
        }
    }
}

#[cfg(test)]
#[expect(
    clippy::disallowed_types,
    reason = "the selector fixtures are independently authored JSON, which is how a drifted \
              vocabulary reaches the comparator the way an artifact would"
)]
mod tests {
    use super::*;

    #[test]
    fn outcomes_vocab_drift_bites() {
        use std::fmt::Write;
        let mut yaml = String::new();
        for kind in OutcomeKind::ALL {
            let class = match kind.class() {
                crate::vocab::OutcomeClass::Success => "success",
                crate::vocab::OutcomeClass::Error => "error",
            };
            let _ = writeln!(
                yaml,
                "{}: {{ class: {class}, meaning: \"m\" }}",
                kind.token()
            );
        }
        let vocab: OutcomesVocab = serde_saphyr::from_str(&yaml).unwrap();
        assert!(vocab.check_against_enum().is_ok());

        let short: OutcomesVocab =
            serde_saphyr::from_str("created: { class: success, meaning: m }\n").unwrap();
        assert!(short.check_against_enum().is_err());
    }

    /// The equality runs in both directions: a kind the compiled taxonomy
    /// does not know is drift, and so is a class the file spells differently
    /// from the taxonomy. Both are reported by row, not merged into one
    /// "files differ" line.
    #[test]
    fn an_unknown_kind_and_a_disagreeing_class_are_both_drift() {
        let vocab: OutcomesVocab = serde_saphyr::from_str(
            "created: { class: error, meaning: m }\nteapot: { class: error, meaning: m }\n",
        )
        .unwrap();
        assert_eq!(vocab.entries().len(), 2);
        let findings = vocab
            .check_against_enum()
            .expect_err("a wrong class and an unknown kind");
        assert!(
            findings.contains(
                &"outcomes.yaml class for \"created\" disagrees with the compiled taxonomy"
                    .to_owned()
            ),
            "{findings:?}"
        );
        assert!(
            findings.contains(&"outcomes.yaml lists unknown kind \"teapot\"".to_owned()),
            "{findings:?}"
        );
    }

    fn selectors(
        body: &[&str],
        header: &[&str],
        ignore_sets: &serde_json::Value,
    ) -> SelectorsVocab {
        serde_json::from_value(serde_json::json!({
            "body_selectors": body,
            "header_matchers": header,
            "ignore_sets": ignore_sets
        }))
        .unwrap()
    }

    fn sound_ignore_sets() -> serde_json::Value {
        serde_json::json!({
            "server_assigned": { "per_binding": true, "source": "s" },
            "ctx_defaults": { "paths": ["/context/health_care_facility"], "source": "s" }
        })
    }

    /// The published selector file and the compiled vocabularies are held
    /// equal, and each named ignore-set keeps the shape its membership rule
    /// implies: `server_assigned` is enumerated per binding, `ctx_defaults`
    /// enumerates its own paths.
    #[test]
    fn selector_drift_and_ignore_set_shape_are_both_reported() {
        let sound = selectors(
            BODY_SELECTOR_TOKENS,
            HEADER_MATCHER_FORMS,
            &sound_ignore_sets(),
        );
        assert!(sound.check_against_enum().is_ok());

        let drifted = selectors(
            &["present"],
            &["absent"],
            &serde_json::json!({
                "server_assigned": { "paths": ["/uid"], "per_binding": false, "source": "s" },
                "ctx_defaults": { "per_binding": true, "source": "s" }
            }),
        );
        let findings = drifted
            .check_against_enum()
            .expect_err("both vocabularies and both ignore-set shapes drifted");
        assert_eq!(findings.len(), 4, "{findings:?}");
        assert!(
            findings
                .iter()
                .any(|f| f.starts_with("selectors.yaml body_selectors")),
            "{findings:?}"
        );
        assert!(
            findings
                .iter()
                .any(|f| f.starts_with("selectors.yaml header_matchers")),
            "{findings:?}"
        );
        assert!(
            findings.contains(
                &"server_assigned must be per_binding with no enumerated paths".to_owned()
            ),
            "{findings:?}"
        );
        assert!(
            findings
                .contains(&"ctx_defaults must enumerate its paths (not per_binding)".to_owned()),
            "{findings:?}"
        );
    }

    /// An ignore-set the file omits is drift too: the comparator resolves the
    /// set by name, so a missing declaration silently ignores nothing.
    #[test]
    fn a_missing_ignore_set_is_drift() {
        let partial = selectors(
            BODY_SELECTOR_TOKENS,
            HEADER_MATCHER_FORMS,
            &serde_json::json!({
                "server_assigned": { "per_binding": true, "source": "s" }
            }),
        );
        assert_eq!(
            partial.check_against_enum(),
            Err(vec![
                "selectors.yaml is missing ignore-set CtxDefaults".to_owned()
            ])
        );
    }

    /// The ignore-set key renders back to its published token, and a name
    /// outside the registry is refused rather than keyed to a default.
    #[test]
    fn an_ignore_set_key_round_trips_its_published_token() {
        for token in ["server_assigned", "ctx_defaults"] {
            let key: IgnoreSetKey = token.parse().unwrap();
            assert_eq!(key.to_string(), token);
        }
        assert_eq!(
            "audit_defaults".parse::<IgnoreSetKey>(),
            Err("\"audit_defaults\" is not a named ignore-set".to_owned())
        );
    }
}
