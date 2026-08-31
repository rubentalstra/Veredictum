// SPDX-FileCopyrightText: Veredictum contributors
// SPDX-License-Identifier: Apache-2.0

//! The ambiguity register (`registers/ambiguities.yaml`) — every entry a
//! real, verified spec divergence or silence with the normative handling a
//! runner must apply.
//!
//! The register is normative: a runner that "resolves" an ambiguity privately
//! is non-conformant to the schedule.

#![allow(
    clippy::disallowed_types,
    reason = "dev/verification tooling over JSON artifacts (the catalogue, results, wire \
              exchanges), whose shapes belong to the artifacts and the SUT; the carriers \
              here are cfg(test)-only, so #[expect] would be unfulfilled in the non-test build"
)]

use serde::Deserialize;

use crate::ids::{AmbiguityId, OptionFamilyName, OptionTag};
use crate::vocab::Disposition;

/// The option FAMILIES an `option_select` entry branches into: one family per
/// independent choice, each enumerating that choice's mutually exclusive arms,
/// in authored order.
///
/// One entry adjudicates one ambiguity and may leave several independent
/// choices open at once (AMB-167 leaves ten, one per REST resource family), so
/// the arms are grouped rather than pooled. A pooled list cannot say which
/// arms are alternatives to which, and a declaration answering one choice then
/// looks like it answered them all.
#[derive(Debug, Clone, Default)]
pub struct OptionFamilies {
    families: Vec<(OptionFamilyName, Vec<OptionTag>)>,
}

impl OptionFamilies {
    /// Every family with its arms, in authored order.
    #[must_use]
    pub fn families(&self) -> &[(OptionFamilyName, Vec<OptionTag>)] {
        &self.families
    }

    /// Whether the entry declares no family at all.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.families.is_empty()
    }

    /// Every arm of every family, in authored order.
    pub fn tags(&self) -> impl Iterator<Item = &OptionTag> {
        self.families.iter().flat_map(|(_, arms)| arms.iter())
    }

    /// Whether any family declares `tag` as one of its arms.
    #[must_use]
    pub fn contains(&self, tag: &OptionTag) -> bool {
        self.tags().any(|arm| arm == tag)
    }

    /// The family `tag` is an arm of, with its arms.
    #[must_use]
    pub fn family_of(&self, tag: &OptionTag) -> Option<(&OptionFamilyName, &[OptionTag])> {
        self.families
            .iter()
            .find(|(_, arms)| arms.contains(tag))
            .map(|(name, arms)| (name, arms.as_slice()))
    }
}

impl<'de> Deserialize<'de> for OptionFamilies {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let families = crate::model::de::ordered_map(deserializer)?;
        Ok(Self { families })
    }
}

/// One register entry.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AmbiguityEntry {
    /// The divergence/silence, with its source citation.
    pub ambiguity: String,
    /// Where it was verified (spec file/section).
    ///
    /// The field splits into `;` / ` + ` fragments. A fragment opening with a
    /// component token (`RM`, `BASE`, `AM`, `QUERY`, `TERM`, `LANG`, `SM`,
    /// `CNF`, `ITS-REST`, `ITS-XML`, `ITS-JSON`) is a citation clause and must
    /// machine-resolve (document + `§` sections, `{a,b}` shorthands expanded);
    /// any other fragment is adjudication prose. At least one citation clause
    /// is required, so a silence claim always grounds on resolvable text.
    pub source: String,
    /// The normative handling a runner must apply.
    pub handling: String,
    /// The machine-readable branch the pipeline takes.
    pub disposition: Disposition,
    /// For `option_select` entries: the option families the sibling cases
    /// carry, family name to its mutually exclusive arms (the ICS `options`
    /// declaration answers each family with exactly one arm).
    #[serde(default)]
    pub options: OptionFamilies,
    /// The tracker issue carrying the outbound `upstream-report`.
    ///
    /// Required for `report_only` and `editorial` entries, so a divergence the
    /// framework carries is always reported back rather than absorbed;
    /// optional for the dispositions that only flag an upstream candidate.
    #[serde(default)]
    pub upstream_issue: Option<u64>,
}

impl AmbiguityEntry {
    /// Checks the disposition-shape invariants.
    ///
    /// `option_select` entries enumerate at least one option family, every
    /// family enumerates at least two mutually exclusive arms, no arm is
    /// shared between two families of the entry, and other dispositions carry
    /// no families at all; `report_only` and `editorial` entries carry an
    /// `upstream_issue`.
    ///
    /// # Errors
    /// Returns a message naming the violated invariant.
    pub fn check_invariants(&self) -> Result<(), String> {
        if matches!(
            self.disposition,
            Disposition::ReportOnly | Disposition::Editorial
        ) && self.upstream_issue.is_none()
        {
            return Err(format!(
                "disposition {:?} must carry an upstream_issue (the GitHub issue number of the outbound upstream-report issue)",
                self.disposition
            ));
        }
        match self.disposition {
            Disposition::OptionSelect if self.options.is_empty() => {
                Err("option_select entry must enumerate at least one option family".to_owned())
            }
            Disposition::OptionSelect => self.check_families(),
            _ if !self.options.is_empty() => Err(format!(
                "disposition {:?} carries option families (only option_select may)",
                self.disposition
            )),
            _ => Ok(()),
        }
    }

    /// Every family is a real choice, and no arm belongs to two of them.
    fn check_families(&self) -> Result<(), String> {
        let mut seen: Vec<&OptionTag> = Vec::new();
        for (name, arms) in self.options.families() {
            if arms.len() < 2 {
                return Err(format!(
                    "option family {name} must enumerate at least two option tags"
                ));
            }
            for arm in arms {
                if seen.contains(&arm) {
                    return Err(format!(
                        "option tag {arm} is an arm of more than one family of this entry"
                    ));
                }
                seen.push(arm);
            }
        }
        Ok(())
    }
}

/// The whole register, keyed by `AMB-<n>`.
#[derive(Debug, Clone)]
pub struct AmbiguityRegister {
    entries: Vec<(AmbiguityId, AmbiguityEntry)>,
}

impl AmbiguityRegister {
    /// Look up an entry.
    #[must_use]
    pub fn get(&self, id: &AmbiguityId) -> Option<&AmbiguityEntry> {
        self.entries.iter().find(|(k, _)| k == id).map(|(_, e)| e)
    }

    /// All entries in authored order.
    #[must_use]
    pub fn entries(&self) -> &[(AmbiguityId, AmbiguityEntry)] {
        &self.entries
    }

    /// Whether any entry declares the option tag as an arm of one of its
    /// families.
    #[must_use]
    pub fn declares_option(&self, tag: &OptionTag) -> bool {
        self.entries.iter().any(|(_, e)| e.options.contains(tag))
    }

    /// The entry and family `tag` is an arm of: the id, the family name, and
    /// the family's mutually exclusive arms.
    #[must_use]
    pub fn option_family_of(
        &self,
        tag: &OptionTag,
    ) -> Option<(&AmbiguityId, &OptionFamilyName, &[OptionTag])> {
        self.entries.iter().find_map(|(id, entry)| {
            entry
                .options
                .family_of(tag)
                .map(|(name, arms)| (id, name, arms))
        })
    }
}

impl<'de> Deserialize<'de> for AmbiguityRegister {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let entries = crate::model::de::ordered_map(deserializer)?;
        Ok(Self { entries })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn disposition_shapes() {
        let r: AmbiguityRegister = serde_json::from_value(serde_json::json!({
            "AMB-4": {
                "ambiguity": "a duplicate ADL 1.4 template_id upload may be refused as a conflict or replace the stored template",
                "source": "SM i_definition_adl14.adoc §upload_opt (silent on duplicates) vs ITS-REST 409_template_already_exists.yaml",
                "handling": "sibling cases carry option tags; the ICS options declaration selects",
                "disposition": "option_select",
                "options": { "adl14-duplicate": ["adl14-duplicate-conflict", "adl14-duplicate-replace"] }
            }
        }))
        .unwrap();
        let (_, entry) = &r.entries()[0];
        assert!(entry.check_invariants().is_ok());
        assert!(r.declares_option(&OptionTag::parse("adl14-duplicate-conflict").unwrap()));

        let e: AmbiguityEntry = serde_json::from_value(serde_json::json!({
            "ambiguity": "x", "source": "s", "handling": "h",
            "disposition": "report_only", "options": { "stray": ["a", "b"] }
        }))
        .unwrap();
        assert!(e.check_invariants().is_err());
    }

    fn entry(disposition: &str, options: &serde_json::Value) -> AmbiguityEntry {
        serde_json::from_value(serde_json::json!({
            "ambiguity": "x", "source": "s", "handling": "h",
            "disposition": disposition, "options": options
        }))
        .unwrap()
    }

    /// `option_select` is a CHOICE the ICS makes, so an entry offering fewer
    /// than two branches leaves nothing to select between.
    #[test]
    fn an_option_select_entry_enumerates_at_least_two_branches() {
        let message = entry(
            "option_select",
            &serde_json::json!({ "solo": ["only-one"] }),
        )
        .check_invariants()
        .expect_err("one branch is not a choice");
        assert_eq!(
            message,
            "option family solo must enumerate at least two option tags"
        );
        assert!(
            entry(
                "option_select",
                &serde_json::json!({ "pair": ["one", "two"] })
            )
            .check_invariants()
            .is_ok()
        );
    }

    /// An entry with no family at all declares no choice, so nothing selects
    /// between its sibling cases.
    #[test]
    fn an_option_select_entry_enumerates_at_least_one_family() {
        let message = entry("option_select", &serde_json::json!({}))
            .check_invariants()
            .expect_err("no family is no choice");
        assert_eq!(
            message,
            "option_select entry must enumerate at least one option family"
        );
    }

    /// An arm shared between two families of one entry makes the families
    /// unresolvable: declaring it would answer both at once.
    #[test]
    fn an_arm_belongs_to_exactly_one_family() {
        let message = entry(
            "option_select",
            &serde_json::json!({ "a": ["one", "two"], "b": ["two", "three"] }),
        )
        .check_invariants()
        .expect_err("a shared arm answers two families at once");
        assert_eq!(
            message,
            "option tag two is an arm of more than one family of this entry"
        );
    }

    /// Every arm resolves back to the family it belongs to, which is what a
    /// declaration is checked against.
    #[test]
    fn an_arm_names_its_own_family() {
        let e = entry(
            "option_select",
            &serde_json::json!({
                "ehr-xml": ["ehr-xml-supported", "ehr-xml-unsupported"],
                "ehr-xml-write": ["ehr-xml-write-accepted", "ehr-xml-write-refused"]
            }),
        );
        let tag = OptionTag::parse("ehr-xml-write-refused").unwrap();
        let (name, arms) = e.options.family_of(&tag).expect("the arm names a family");
        assert_eq!(name.as_str(), "ehr-xml-write");
        assert_eq!(arms.len(), 2);
        assert!(
            e.options
                .family_of(&OptionTag::parse("nobody-declares-this").unwrap())
                .is_none()
        );
    }

    /// Only `option_select` branches, so tags on any other disposition state
    /// a choice the pipeline never makes.
    #[test]
    fn only_option_select_may_carry_option_tags() {
        let message = entry(
            "fixed_handling",
            &serde_json::json!({ "stray": ["a", "b"] }),
        )
        .check_invariants()
        .expect_err("a fixed handling has no branches");
        assert_eq!(
            message,
            "disposition FixedHandling carries option families (only option_select may)"
        );
        assert!(
            entry("fixed_handling", &serde_json::json!({}))
                .check_invariants()
                .is_ok()
        );
    }
}
