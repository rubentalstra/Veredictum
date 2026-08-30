// SPDX-FileCopyrightText: Veredictum contributors
// SPDX-License-Identifier: Apache-2.0

//! The pure judges behind the `version` assertion family.
//!
//! Every fact the family declares lives on one of three served
//! representations, so each judge here takes the representation and the
//! authored expectation and returns a failure that names both sides:
//!
//! - the version identity the row resolved — the `ETag` or the commit body's
//!   uid, judged against a `uid_pattern` (BASE `base_types` master05
//!   §Syntaxes);
//! - the served `VERSION` envelope — `commit_audit.change_type` and
//!   `lifecycle_state`, the latter effected from `item` on an
//!   `IMPORTED_VERSION` (RM common `UML/classes/version.adoc` §Attributes,
//!   `original_version.adoc` §Attributes, `imported_version.adoc` §Functions,
//!   `audit_details.adoc` §Attributes);
//! - the `REVISION_HISTORY` — one `REVISION_HISTORY_ITEM` per version, which
//!   is the only released wire surface disclosing how many versions a
//!   container holds (RM common `UML/classes/revision_history_item.adoc`
//!   §Description: "An entry in a revision history, corresponding to a version
//!   from a versioned container"). `VERSIONED_OBJECT.version_count` is a
//!   FUNCTION (`versioned_object.adoc` §Functions), and released ITS-JSON
//!   `components/RM/Release-1.1.0/Common/VERSIONED_OBJECT.json` closes the
//!   served object to `uid`/`owner_id`/`time_created` with
//!   `additionalProperties: false`, so the container read cannot carry it.
//!
//! The driver ([`crate::exec::driver`]) performs the reads; nothing here
//! touches the wire.

use serde_json::Value;

use crate::exec::assertions::AssertionFailure;
use crate::exec::headers::{UID, VERSION_TREE_ID};
use crate::vocab::ChangeType;

/// The `uid.value` an `ORIGINAL_VERSION` envelope carries, if it carries one.
///
/// `VERSION.uid` is an `OBJECT_VERSION_ID` (RM common
/// `UML/classes/version.adoc` §Functions), served as the canonical
/// `{ "value": … }` object id. An `IMPORTED_VERSION` carries none of its own —
/// `uid ()` is effected with `Result = item.uid` (`imported_version.adoc`
/// §Functions) — so this reads `None` there and a caller matching an in-hand
/// body against a resolved uid falls through to the envelope read.
#[must_use]
pub fn envelope_uid(envelope: &Value) -> Option<&str> {
    envelope.get("uid")?.get("value")?.as_str()
}

/// Whether a served body IS a `VERSION`, by the `_type` discriminator the
/// released ITS-JSON binds it to.
///
/// `components/RM/Release-1.1.0/Common/ORIGINAL_VERSION.json` and
/// `IMPORTED_VERSION.json` each fix `_type` to a `const` naming their own
/// class, and those two are the concrete `VERSION` descendants the release
/// carries. A versioned item served on its own — a `COMPOSITION` under
/// `Prefer: return=representation`, say — repeats the version's
/// `OBJECT_VERSION_ID` at `uid.value` (RM common
/// `UML/classes/version.adoc` §Functions) while carrying no `commit_audit`,
/// so the uid alone cannot tell the envelope from its content.
#[must_use]
pub fn is_version_envelope(body: &Value) -> bool {
    version_class(body).is_some()
}

/// The concrete `VERSION` descendant a served body names in its `_type`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum VersionClass {
    /// `ORIGINAL_VERSION`, which carries `lifecycle_state` itself (RM common
    /// `UML/classes/original_version.adoc` §Attributes).
    Original,
    /// `IMPORTED_VERSION`, whose `lifecycle_state` is effected from the
    /// wrapped `ORIGINAL_VERSION` (RM common
    /// `UML/classes/imported_version.adoc` §Functions).
    Imported,
}

/// The class a served body declares, or `None` when the body is no served
/// `VERSION` at all.
///
/// The vocabulary is CLOSED to the two classes the released ITS-JSON binds a
/// `_type` `const` to, and an unrecognized token names none of them rather
/// than falling back to one.
fn version_class(body: &Value) -> Option<VersionClass> {
    match body.get("_type").and_then(Value::as_str)? {
        "ORIGINAL_VERSION" => Some(VersionClass::Original),
        "IMPORTED_VERSION" => Some(VersionClass::Imported),
        _ => None,
    }
}

/// The regex fragment a `<name>` token of a `uid_pattern` denotes.
///
/// The vocabulary is CLOSED, and an unknown token is refused by
/// [`expand_uid_literal`] rather than widened into a wildcard: an
/// `OBJECT_VERSION_ID` pattern that degrades to a near-tautology asserts
/// nothing. Each token names a segment whose lexical form BASE `base_types`
/// master05 §Syntaxes defines — `object_version_id = object_id, '::',
/// creating_system_id, '::', version_tree_id`, with `object_id` and
/// `creating_system_id` both `uid`.
#[must_use]
pub fn uid_pattern_token(name: &str) -> Option<&'static str> {
    match name {
        "uuid" | "system" => Some(UID),
        "n" => Some(VERSION_TREE_ID),
        _ => None,
    }
}

/// Turn one literal run of a `uid_pattern` into a regex fragment: `<name>`
/// tokens become their released grammar, everything else is escaped.
///
/// # Errors
/// The name of the first `<…>` token outside [`uid_pattern_token`]'s closed
/// vocabulary.
pub fn expand_uid_literal(literal: &str) -> Result<String, String> {
    let mut out = String::new();
    let mut rest = literal;
    while let Some(start) = rest.find('<') {
        let (head, tail) = rest.split_at(start);
        out.push_str(&regex::escape(head));
        let Some(end) = tail.find('>') else {
            out.push_str(&regex::escape(tail));
            return Ok(out);
        };
        let name = tail.get(1..end).unwrap_or_default();
        let grammar = uid_pattern_token(name).ok_or_else(|| name.to_owned())?;
        out.push_str(grammar);
        rest = tail.get(end.saturating_add(1)..).unwrap_or_default();
    }
    out.push_str(&regex::escape(rest));
    Ok(out)
}

/// Judge the version identity the row RESOLVED against a compiled
/// `uid_pattern`.
///
/// The value judged is the one the row already holds — the served `ETag` or
/// the uid of a commit body — never a re-read envelope addressed BY that same
/// uid, which a conformant server echoes back unchanged.
///
/// # Errors
/// The row resolved an empty uid, or the value does not match.
pub fn eval_uid_pattern(
    uid: &str,
    pattern: &regex::Regex,
    authored: &str,
) -> Result<(), AssertionFailure> {
    if uid.is_empty() {
        return Err(AssertionFailure(
            "version uid_pattern: the row resolved an empty uid (BASE base_types master05 §Syntaxes: object_version_id = object_id, '::', creating_system_id, '::', version_tree_id)"
                .into(),
        ));
    }
    if pattern.is_match(uid) {
        Ok(())
    } else {
        Err(AssertionFailure(format!(
            "version uid_pattern: served uid {uid:?} does not match {authored:?}"
        )))
    }
}

/// The coarse RM change class an `audit change type` code denotes.
///
/// RM common `master06-change_control_package.adoc` §Change Control assigns
/// the codes: an addition sets `249|creation|`, a deletion sets
/// `523|deleted|`, and a modification of an existing item sets `250|amendment|`
/// when the change is logically a correction and `251|modification|` when it is
/// a change or addition to the content. The catalogue asserts that three-way
/// class, so MODIFY covers both codes of the modification bullet — which of
/// the two a server picks turns on the committing application's intent, a fact
/// no wire read discloses.
#[must_use]
pub fn change_class(code: &str) -> Option<ChangeType> {
    match code {
        "249" => Some(ChangeType::Create),
        "250" | "251" => Some(ChangeType::Modify),
        "523" => Some(ChangeType::Deleted),
        _ => None,
    }
}

/// Judge an envelope's `commit_audit.change_type` against the asserted class.
///
/// `commit_audit` is read at the top level for both served classes.
/// `IMPORTED_VERSION` inherits it from `VERSION`, "providing imported versions
/// with their own audit trail and Contribution, distinct from those of the
/// imported `ORIGINAL_VERSION`" (RM common
/// `UML/classes/imported_version.adoc` §Description), and released ITS-JSON
/// `components/RM/Release-1.1.0/Common/IMPORTED_VERSION.json` requires it on
/// the wrapper, so the trail judged is the import's own.
///
/// # Errors
/// The envelope carries no coded `commit_audit.change_type`, the code is
/// outside the three classes the schedule asserts, or it names another class.
pub fn eval_change_type(envelope: &Value, want: ChangeType) -> Result<(), AssertionFailure> {
    let coded = envelope
        .get("commit_audit")
        .and_then(|a| a.get("change_type"));
    let Some(code) = coded
        .and_then(|c| c.get("defining_code"))
        .and_then(|c| c.get("code_string"))
        .and_then(Value::as_str)
    else {
        return Err(AssertionFailure(
            "version change_type: the served envelope carries no commit_audit.change_type.defining_code.code_string (RM common audit_details.adoc: change_type is a DV_CODED_TEXT from the openEHR audit change type group)"
                .into(),
        ));
    };
    match change_class(code) {
        Some(observed) if observed == want => Ok(()),
        Some(observed) => Err(AssertionFailure(format!(
            "version change_type: served code {code} is {observed:?}, expected {want:?}"
        ))),
        None => Err(AssertionFailure(format!(
            "version change_type: served code {code} is outside the CREATE/MODIFY/DELETED classes the schedule asserts (RM common master06 §Change Control)"
        ))),
    }
}

/// Render a served `DV_CODED_TEXT` in the `terminology::code|rubric|` form the
/// schedule authors `lifecycle_state` in.
#[must_use]
pub fn coded_text_term(coded: &Value) -> Option<String> {
    let code = coded.get("defining_code")?;
    let code_string = code.get("code_string")?.as_str()?;
    let terminology = code
        .get("terminology_id")
        .and_then(|t| t.get("value"))
        .and_then(Value::as_str)?;
    let rubric = coded
        .get("value")
        .and_then(Value::as_str)
        .unwrap_or_default();
    Some(format!("{terminology}::{code_string}|{rubric}|"))
}

/// The `DV_CODED_TEXT` an envelope's `lifecycle_state` resolves to, following
/// the effected function where the served class declares one.
///
/// An `ORIGINAL_VERSION` carries the attribute itself. An `IMPORTED_VERSION`
/// cannot: released ITS-JSON
/// `components/RM/Release-1.1.0/Common/IMPORTED_VERSION.json` gives it
/// `contribution`, `commit_audit`, `signature` and `item` under
/// `additionalProperties: false`, and RM common
/// `UML/classes/imported_version.adoc` §Functions types `lifecycle_state ()`
/// as effected, "Lifecycle state of the content item in wrapped
/// `ORIGINAL_VERSION`, derived as `_item.lifecycle_state_`". A body naming
/// neither class keeps the top-level read, which is where a server that omits
/// `_type` puts the attribute.
fn envelope_lifecycle_state(envelope: &Value) -> Option<&Value> {
    match version_class(envelope) {
        Some(VersionClass::Imported) => envelope.get("item")?.get("lifecycle_state"),
        Some(VersionClass::Original) | None => envelope.get("lifecycle_state"),
    }
}

/// Judge an envelope's `lifecycle_state` against the asserted coded term.
///
/// `ORIGINAL_VERSION.lifecycle_state` is a `DV_CODED_TEXT` "coded by openEHR
/// vocabulary `version lifecycle state`" (RM common
/// `UML/classes/original_version.adoc` §Attributes), so the comparison is over
/// the full coded term, never the rubric alone. An `IMPORTED_VERSION` carries
/// no `lifecycle_state` of its own and is judged on the state of the version
/// it wraps, which `imported_version.adoc` §Functions effects as
/// `_item.lifecycle_state_`.
///
/// # Errors
/// The envelope carries no coded `lifecycle_state`, or it names another term.
pub fn eval_lifecycle_state(envelope: &Value, want: &str) -> Result<(), AssertionFailure> {
    let Some(observed) = envelope_lifecycle_state(envelope).and_then(coded_text_term) else {
        return Err(AssertionFailure(
            "version lifecycle_state: the served envelope carries no coded lifecycle_state (RM common original_version.adoc: a DV_CODED_TEXT from the version lifecycle state group; imported_version.adoc §Functions effects it from item.lifecycle_state)"
                .into(),
        ));
    };
    if observed == want {
        Ok(())
    } else {
        Err(AssertionFailure(format!(
            "version lifecycle_state: served {observed:?}, expected {want:?}"
        )))
    }
}

/// The number of versions a served `REVISION_HISTORY` accounts for.
///
/// # Errors
/// The representation carries no `items` list.
pub fn version_count(history: &Value) -> Result<u64, AssertionFailure> {
    let Some(items) = history.get("items").and_then(Value::as_array) else {
        return Err(AssertionFailure(
            "version count: the served REVISION_HISTORY carries no items list (RM common revision_history.adoc: items is 1..1)"
                .into(),
        ));
    };
    u64::try_from(items.len()).map_err(|e| {
        AssertionFailure(format!(
            "version count: the items list is unmeasurable ({e})"
        ))
    })
}

/// Judge a served `REVISION_HISTORY` against an asserted version count.
///
/// # Errors
/// The representation carries no `items` list, or the count differs.
pub fn eval_count(history: &Value, want: u64) -> Result<(), AssertionFailure> {
    let observed = version_count(history)?;
    if observed == want {
        Ok(())
    } else {
        Err(AssertionFailure(format!(
            "version count: the container holds {observed} version(s), expected {want}"
        )))
    }
}

#[cfg(test)]
#[expect(
    clippy::panic_in_result_fn,
    reason = "the Book ch11 test shape: assertions panic, plumbing propagates with `?`"
)]
mod tests {
    use super::*;
    use serde_json::json;

    fn envelope(change_code: &str, lifecycle_code: &str, uid: &str) -> Value {
        json!({
            "_type": "ORIGINAL_VERSION",
            "uid": { "value": uid },
            "lifecycle_state": {
                "value": "complete",
                "defining_code": {
                    "terminology_id": { "value": "openehr" },
                    "code_string": lifecycle_code
                }
            },
            "commit_audit": {
                "change_type": {
                    "value": "creation",
                    "defining_code": {
                        "terminology_id": { "value": "openehr" },
                        "code_string": change_code
                    }
                }
            }
        })
    }

    #[test]
    fn the_three_asserted_classes_map_from_the_terminology_codes() {
        assert_eq!(change_class("249"), Some(ChangeType::Create));
        assert_eq!(change_class("250"), Some(ChangeType::Modify));
        assert_eq!(change_class("251"), Some(ChangeType::Modify));
        assert_eq!(change_class("523"), Some(ChangeType::Deleted));
        // 666|attestation| commits no version, so it is no class the family
        // asserts: a loud failure, never a silent Create.
        assert_eq!(change_class("666"), None);
        assert_eq!(change_class(""), None);
    }

    #[test]
    fn a_change_type_mismatch_names_both_sides() {
        let served = envelope("523", "523", "a::s::2");
        let e = eval_change_type(&served, ChangeType::Create).unwrap_err();
        assert!(
            e.0.contains("523") && e.0.contains("Create"),
            "message names neither side: {}",
            e.0
        );
        assert_eq!(eval_change_type(&served, ChangeType::Deleted), Ok(()));
    }

    #[test]
    fn an_envelope_without_a_coded_change_type_fails_loudly() {
        let served = json!({ "uid": { "value": "a::s::1" }, "commit_audit": {} });
        assert!(eval_change_type(&served, ChangeType::Create).is_err());
    }

    #[test]
    fn lifecycle_state_compares_the_whole_coded_term() {
        let served = envelope("523", "523", "a::s::2");
        assert_eq!(
            eval_lifecycle_state(&served, "openehr::523|complete|"),
            Ok(())
        );
        assert!(eval_lifecycle_state(&served, "openehr::532|complete|").is_err());
        assert!(eval_lifecycle_state(&served, "complete").is_err());
    }

    /// An `IMPORTED_VERSION` is judged on the lifecycle state of the version it
    /// wraps: `lifecycle_state ()` is effected, "derived as
    /// `_item.lifecycle_state_`" (RM common `UML/classes/imported_version.adoc`
    /// §Functions). A wrong term still fails, so the resolution judges.
    #[test]
    fn an_imported_versions_lifecycle_state_resolves_through_its_item() {
        let imported = json!({
            "_type": "IMPORTED_VERSION",
            "commit_audit": { "change_type": {
                "value": "creation",
                "defining_code": {
                    "terminology_id": { "value": "openehr" },
                    "code_string": "249"
                }
            } },
            "item": envelope("250", "532", "a::s::2")
        });
        assert_eq!(
            eval_lifecycle_state(&imported, "openehr::532|complete|"),
            Ok(())
        );
        let wrong = eval_lifecycle_state(&imported, "openehr::523|deleted|").unwrap_err();
        assert!(wrong.0.contains("532"), "{}", wrong.0);
        // The wrapper's own audit trail is the one judged, never the item's.
        assert_eq!(eval_change_type(&imported, ChangeType::Create), Ok(()));
        // A wrapper serving no item has no lifecycle state to resolve.
        let itemless = json!({ "_type": "IMPORTED_VERSION" });
        assert!(eval_lifecycle_state(&itemless, "openehr::532|complete|").is_err());
        // A body naming no class keeps the top-level read, which is where a
        // server that omits `_type` puts the attribute.
        let untyped = json!({ "lifecycle_state": {
            "value": "complete",
            "defining_code": {
                "terminology_id": { "value": "openehr" },
                "code_string": "532"
            }
        } });
        assert_eq!(
            eval_lifecycle_state(&untyped, "openehr::532|complete|"),
            Ok(())
        );
    }

    /// Only the two concrete `VERSION` classes the released ITS-JSON binds a
    /// `_type` const to ARE the envelope; a versioned item repeating the same
    /// `OBJECT_VERSION_ID` is not, and neither is a body naming no class.
    #[test]
    fn a_version_envelope_is_recognized_by_its_released_type_token() {
        let uid = "a::s::2";
        assert!(is_version_envelope(&envelope("249", "532", uid)));
        assert!(is_version_envelope(&json!({ "_type": "IMPORTED_VERSION" })));
        // An IMPORTED_VERSION carries no uid of its own: the released ITS-JSON
        // closes it to contribution/commit_audit/signature/item, and `uid ()`
        // is effected from `item.uid` (imported_version.adoc §Functions).
        let imported = json!({ "_type": "IMPORTED_VERSION", "item": { "uid": { "value": uid } } });
        assert!(is_version_envelope(&imported));
        assert_eq!(envelope_uid(&imported), None);
        assert!(!is_version_envelope(&json!({
            "_type": "COMPOSITION", "uid": { "value": uid }
        })));
        assert!(!is_version_envelope(&json!({ "uid": { "value": uid } })));
        assert!(!is_version_envelope(&json!({ "_type": "PERSON" })));
        assert!(!is_version_envelope(&Value::Null));
    }

    #[test]
    fn the_uid_pattern_vocabulary_is_closed() -> Result<(), String> {
        assert_eq!(expand_uid_literal("::<system>::1")?, format!("::{UID}::1"));
        assert_eq!(expand_uid_literal("<unknown>::x").unwrap_err(), "unknown");
        // Each token names the segment BASE `base_types` master05 §Syntaxes
        // defines: `uuid` and `system` are both `uid`, `n` is the version
        // tree.
        assert_eq!(uid_pattern_token("uuid"), Some(UID));
        assert_eq!(uid_pattern_token("system"), Some(UID));
        assert_eq!(uid_pattern_token("n"), Some(VERSION_TREE_ID));
        assert_eq!(uid_pattern_token("anything"), None);
        // An unterminated `<` is a literal run, escaped like any other.
        assert_eq!(expand_uid_literal("a.b<n")?, regex::escape("a.b<n"));
        assert_eq!(expand_uid_literal("a.b")?, regex::escape("a.b"));
        Ok(())
    }

    /// Every judge fails LOUDLY on a representation that carries nothing to
    /// judge, and names what the released text says should have been there:
    /// a silent pass would let a server earn the row by serving less.
    #[test]
    fn a_representation_that_carries_nothing_fails_rather_than_passes() -> Result<(), String> {
        let bare = json!({ "_type": "ORIGINAL_VERSION" });
        assert_eq!(envelope_uid(&bare), None);
        // An empty resolved uid matches even a permissive pattern, so it is
        // refused before the match rather than passing on nothing.
        let pattern = regex::Regex::new("^.*$").map_err(|e| e.to_string())?;
        let uid = eval_uid_pattern("", &pattern, "<uuid>").unwrap_err();
        assert!(uid.0.contains("empty uid"), "{}", uid.0);

        let lifecycle = eval_lifecycle_state(&bare, "openehr::532|complete|").unwrap_err();
        assert!(
            lifecycle.0.contains("carries no coded lifecycle_state"),
            "{}",
            lifecycle.0
        );
        assert_eq!(coded_text_term(&json!({ "defining_code": {} })), None);
        assert_eq!(
            coded_text_term(&json!({
                "defining_code": { "code_string": "532", "terminology_id": {} }
            })),
            None
        );
        assert_eq!(coded_text_term(&json!({})), None);
        // The rubric is optional: an absent `value` renders as the empty
        // rubric rather than dropping the term.
        assert_eq!(
            coded_text_term(&json!({
                "defining_code": {
                    "code_string": "532",
                    "terminology_id": { "value": "openehr" }
                }
            })),
            Some("openehr::532||".to_owned())
        );

        // A code outside the three asserted classes is named as outside them,
        // never mapped into the nearest one.
        let outside = json!({
            "commit_audit": { "change_type": {
                "defining_code": { "code_string": "816" }
            } }
        });
        let change = eval_change_type(&outside, ChangeType::Create).unwrap_err();
        assert!(
            change.0.contains("816") && change.0.contains("outside"),
            "{}",
            change.0
        );

        assert_eq!(version_count(&json!({ "items": [] })), Ok(0));
        Ok(())
    }

    #[test]
    fn a_uid_pattern_judges_the_resolved_uid() -> Result<(), Box<dyn std::error::Error>> {
        let literal = expand_uid_literal("<uuid>::<system>::2")?;
        let pattern = regex::Regex::new(&format!("^{literal}$"))?;
        let resolved = "8849182c-82ad-4088-a07f-48ead4180515::s.example::2";
        assert_eq!(
            eval_uid_pattern(resolved, &pattern, "<uuid>::<system>::2"),
            Ok(())
        );
        let wrong = "8849182c-82ad-4088-a07f-48ead4180515::s.example::3";
        let failure = eval_uid_pattern(wrong, &pattern, "<uuid>::<system>::2").unwrap_err();
        assert!(failure.0.contains("::3"), "{}", failure.0);
        Ok(())
    }

    #[test]
    fn the_count_comes_from_the_revision_history_items() {
        let history = json!({ "items": [ { "version_id": { "value": "a::s::1" } } ] });
        assert_eq!(eval_count(&history, 1), Ok(()));
        assert!(eval_count(&history, 2).is_err());
        assert!(eval_count(&json!({}), 0).is_err());
    }
}
