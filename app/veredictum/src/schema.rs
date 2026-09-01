// SPDX-FileCopyrightText: Veredictum contributors
// SPDX-License-Identifier: Apache-2.0

//! Deterministic JSON-Schema emission for the five schedule-artifact
//! families (the set the upstream U1 proposal ships).
//!
//! The published schema files are the norm; this module is their single
//! source. Emission is byte-deterministic: fixed insertion order (via
//! `serde_json`'s `preserve_order`), two-space pretty printing, trailing
//! newline. Every closed vocabulary in a schema derives from the compiled
//! enums in [`crate::vocab`], so schema and reference implementation cannot
//! drift. Draft: JSON Schema 2020-12. `$id`s are versioned URNs pending the
//! upstream repository's canonical URLs.

#![expect(
    clippy::disallowed_types,
    reason = "dev/verification tooling over JSON artifacts (the catalogue, results, wire \
              exchanges), whose shapes belong to the artifacts and the SUT"
)]

use serde::Serialize;
use serde_json::{Value, json};

use crate::model::capability::Realization;
use crate::model::vocab_files::{BODY_SELECTOR_TOKENS, HEADER_MATCHER_FORMS};
use crate::model::wire_surface::SurfaceReason;
use crate::party::{OutcomeStatus, SelectionBasis, TechProfileSource, VerificationPackStatus};
use crate::registry::{
    ArtifactRole, DeploymentKind, EntryKind, READABLE_REGISTRY_SCHEMA_VERSIONS,
    READABLE_RULES_VERSIONS, Relationship, SignatureScheme, Tier as RegistryTier,
};
use crate::vocab::{
    CaseKind, CaseStatus, Component, CorpusFormat, Disposition, FormatName, HttpMethod, Iteration,
    ItsName, OutcomeKind, PlaceholderPolicy, ServerState, SpecComponent, Tier,
};

/// The schema version stamped into every `$id` (bumps with the schedule
/// release, independently of the product version).
pub const SCHEMA_VERSION: &str = "0.1.0";

const DRAFT: &str = "https://json-schema.org/draft/2020-12/schema";

/// Serde token of one enum variant.
fn token<T: Serialize>(v: &T) -> Value {
    match serde_json::to_value(v) {
        Ok(Value::String(s)) => Value::String(s),
        // Unreachable for the vocab enums (all serialize to strings); a
        // non-string token would be a defect in this module, surfaced by the
        // emission tests, never silently emitted.
        _ => Value::Null,
    }
}

/// Serde tokens of a whole vocabulary.
fn tokens<T: Serialize>(all: &[T]) -> Value {
    Value::Array(all.iter().map(token).collect())
}

/// The serde token of one enum variant, as an owned `String` (for building
/// property-name maps). Non-string tokens are a defect surfaced by the
/// emission tests, never silently emitted.
fn token_str<T: Serialize>(v: &T) -> String {
    match serde_json::to_value(v) {
        Ok(Value::String(s)) => s,
        _ => String::new(),
    }
}

fn urn(name: &str) -> String {
    format!("urn:openehr:cnf:schema:{name}:{SCHEMA_VERSION}")
}

const CASE_ID_PATTERN: &str = "^\\S+$";
const SM_OPERATION_PATTERN: &str = "^I_[A-Z0-9_]+\\.[a-z][a-z0-9_]*$";
const IDENT_PATTERN: &str = "^[A-Za-z_][A-Za-z0-9_]*$";
const CORPUS_KEY_PATTERN: &str = "^[a-z0-9_-]+(\\.[a-z0-9_-]+)*$";
const AMBIGUITY_ID_PATTERN: &str = "^AMB-[0-9]+$";
const OPTION_TAG_PATTERN: &str = "^[a-z0-9_-]+$";
const REGISTRY_ENTRY_ID_PATTERN: &str =
    "^[0-9]{4}-[0-9]{2}-[0-9]{2}-[a-z0-9]([a-z0-9-]*[a-z0-9])?$";
const REGISTRY_SYSTEM_PATTERN: &str = "^[a-z0-9]([a-z0-9-]*[a-z0-9])?$";
const REGISTRY_DIGEST_PATTERN: &str = "^[0-9a-f]{64}$";
const REGISTRY_UTC_TIMESTAMP_PATTERN: &str =
    "^[0-9]{4}-[0-9]{2}-[0-9]{2}T[0-9]{2}:[0-9]{2}:[0-9]{2}(\\.[0-9]+)?Z$";

fn string_array(item_pattern: Option<&str>) -> Value {
    match item_pattern {
        Some(p) => json!({ "type": "array", "items": { "type": "string", "pattern": p } }),
        None => json!({ "type": "array", "items": { "type": "string" } }),
    }
}

/// [`string_array`] with a `minItems: 1` floor, for a required list whose
/// empty form asserts nothing any reader can act on.
fn nonempty_string_array(item_pattern: &str) -> Value {
    json!({
        "type": "array", "minItems": 1,
        "items": { "type": "string", "pattern": item_pattern }
    })
}

/// The `applies` spec-version filter — ONE shape wherever a version floor is
/// declared: on a case core (the whole behaviour is release-dated), on an
/// operation binding (the wire itself is), and on a header expectation (one
/// released RESPONSE rule is, while the operation is not). Same components,
/// same Cargo/semver range grammar, same
/// [`crate::model::case::Applies::satisfied_by`] polarity at every site.
fn applies_def() -> Value {
    json!({
        "type": "object",
        "additionalProperties": false,
        "description": "Spec-version applicability ranges (Cargo/semver requirement syntax). Every declared range must be satisfied by the party's declared version; an undeclared component puts the gated item out of scope.",
        "properties": {
            "rm": { "type": "string" }, "base": { "type": "string" },
            "am": { "type": "string" }, "aql": { "type": "string" },
            "its_rest": { "type": "string" }, "term": { "type": "string" }
        }
    })
}

/// One `outcomes.*.headers` entry: a bare matcher string, or the mapping form
/// carrying the presence-strength and version-dating modifiers.
fn one_header_expectation_def() -> Value {
    json!({
        "oneOf": [
            { "type": "string",
              "description": "A matcher from the closed vocabulary (vocab/selectors.yaml header_matchers); `present?` is the shorthand for { match: present, optional: true }" },
            { "type": "object",
              "additionalProperties": false,
              "required": ["match"],
              "properties": {
                  "match": { "type": "string" },
                  "optional": { "type": "boolean",
                                "description": "Presence is SHOULD/MAY-strength: an absent or blank header satisfies the expectation, a present one is judged in full by `match` (ITS-REST overview §ETag and Last-Modified makes presence a SHOULD while §Deprecated headers makes the form a MUST)" },
                  "applies": applies_def()
              } }
        ]
    })
}

/// The `outcomes.*.headers` value for one header: a single expectation, or a
/// sequence of them judged conjunctively (used where the released text puts
/// rules of different strength or different dating on the same header).
fn header_expectation_def() -> Value {
    json!({
        "oneOf": [
            one_header_expectation_def(),
            { "type": "array",
              "minItems": 1,
              "items": one_header_expectation_def(),
              "description": "Several expectations on one header, all judged: each carries its own `optional` and `applies`, so an undated identity rule and a release-dated form rule keep their own grounds" }
        ]
    })
}

fn requires_def() -> Value {
    json!({
        "type": "object",
        "additionalProperties": false,
        "properties": {
            "server": { "enum": tokens(ServerState::ALL) },
            "templates": string_array(Some(CORPUS_KEY_PATTERN)),
            "ehr": { "oneOf": [
                { "const": "none" },
                { "type": "object", "additionalProperties": false,
                  "required": ["commits"],
                  "properties": { "commits": { "enum": ["none", "any"] } } }
            ] },
            "directory": { "anyOf": [
                { "const": "none" },
                { "type": "string", "pattern": CORPUS_KEY_PATTERN }
            ] },
            "party": { "anyOf": [
                { "const": "none" },
                { "type": "string", "pattern": CORPUS_KEY_PATTERN }
            ] },
            "party_relationship": { "anyOf": [
                { "const": "none" },
                { "type": "object",
                  "description": "A PARTY_RELATIONSHIP provisioned between two REAL parties (mints `${party_relationship_id}`, its VERSIONED_OBJECT uid). The two endpoint parties are created first and their container uids written into the relationship's source/target PARTY_REFs — RM demographic master02-demographic_package.adoc §Party Relationships: the references are \"OBJECT_REFs containing HIER_OBJECT_IDs to denote the Version container of a Party\". The relationship create has no released wire (register AMB-32, the `party-relationship` served_extensions family).",
                  "additionalProperties": false,
                  "required": ["source", "target", "relationship"],
                  "properties": {
                      "source": { "type": "string", "pattern": CORPUS_KEY_PATTERN },
                      "target": { "type": "string", "pattern": CORPUS_KEY_PATTERN },
                      "relationship": { "type": "string", "pattern": CORPUS_KEY_PATTERN }
                  } }
            ] },
            "import": { "anyOf": [
                { "const": "none" },
                { "type": "object",
                  "description": "An EHR-Extract received from another system before the flow, so a RELEASED read has an IMPORTED_VERSION to serve (RM common master06-change_control_package.adoc §Copying: \"An IMPORTED_VERSION instance is then created, its item set to the received ORIGINAL_VERSION\"). Mints `${imported_versioned_object_uid}` + `${imported_version_uid}` (+ `${imported_branch_version_uid}` when the named container carries a branch) from the extract's own identities, which the copy preserves; with no `requires.ehr` the import CLONES a whole EHR (§Copying Case 1) and mints `${ehr_id}` too, otherwise it lands in the provisioned one (Cases 2/3). The import itself has no released wire (register AMB-34, the `message-extract` served_extensions family), so the requirement is usable only on a party that serves it.",
                  "additionalProperties": false,
                  "required": ["extract", "container"],
                  "properties": {
                      "extract": { "type": "string", "pattern": CORPUS_KEY_PATTERN },
                      "container": { "description": "Which X_VERSIONED_* content item of the extract the minted handles name — an extract carries several at once.", "enum": tokens(crate::vocab::XVersionedClass::ALL) }
                  } }
            ] },
            "commit": string_array(Some(CORPUS_KEY_PATTERN)),
            "terminology": {
                "description": "The terminology deployment the case needs, matched against the addressed instance's ixit.terminology declaration at SELECTION time. Released ITS-REST 1.1.0 surfaces no terminology resource, so which terminology servers a deployment holds open, which namespaces they answer for, and what it does with a value set it cannot resolve are IXIT declarations; a case needing one the party does not declare is not-applicable with that citation.",
                "type": "object",
                "additionalProperties": false,
                "properties": {
                    "posture": { "description": "The unresolvable-value-set posture the case's expectation rests on (register AMB-172); omitted when the behaviour is posture-independent.", "enum": tokens(crate::ixit::TerminologyPosture::ALL) },
                    "served": string_array(None),
                    "unreachable": string_array(None),
                    "distinct_servers": { "description": "How many DISTINCT reachable servers the `served` namespaces must be spread across — the N>=2 simultaneous-servers requirement (BASE master12 §Overview).", "type": "integer", "minimum": 1 }
                }
            },
            "spec_profile": {
                "description": "The openEHR specification GENERATION SET the case's expectation rests on, matched against the addressed instance's ixit.spec_profile declaration at SELECTION time. No released operation discloses which set a deployment runs (the openEHR release strategy makes a minor release a compatible superset, so the sets differ only in accepted surface); a case needing one the party does not declare is not-applicable with that citation. A multi-instance case states a per-instance need under `instances`; this case-level form binds every addressed instance.",
                "enum": tokens(crate::ixit::SpecProfile::ALL)
            },
            "administrative": {
                "description": "The administrative-authorization posture the case's premise rests on, matched against the addressed instance's ixit.administrative declaration at SELECTION time (register AMB-228). SM master02-overview.adoc §Functional Style delegates access control to the implementation, so which roles a principal holds is an IXIT declaration and nothing on the wire discloses it; a case needing a posture the party does not declare — or declares oppositely — is not-applicable with that citation. A multi-instance case states a per-instance need under `instances`; this case-level form binds every addressed instance.",
                "type": "boolean"
            },
            "instances": { "type": "object",
                "propertyNames": { "pattern": IDENT_PATTERN },
                "additionalProperties": { "$ref": "#/$defs/requires" } }
        }
    })
}

fn parameters_def() -> Value {
    json!({
        "type": "object",
        "additionalProperties": false,
        "required": ["iteration"],
        "properties": {
            "iteration": { "enum": tokens(Iteration::ALL) },
            "matrix": {
                "type": "object",
                "additionalProperties": false,
                "required": ["columns"],
                "properties": {
                    "columns": { "type": "array", "minItems": 1, "items": { "type": "string" } },
                    "rows": { "type": "array", "items": { "type": "array" } },
                    "rows_from": { "type": "string" }
                }
            },
            "fixture_set": {
                "type": "array",
                "minItems": 1,
                "items": {
                    "type": "object",
                    "additionalProperties": false,
                    "required": ["data_set", "expected"],
                    "properties": {
                        "data_set": { "type": "string", "pattern": CORPUS_KEY_PATTERN },
                        "expected": { "enum": tokens(OutcomeKind::ALL) },
                        "defect": { "type": "string" },
                        "spec_ref": { "type": "string" }
                    }
                }
            }
        }
    })
}

fn flow_step_def() -> Value {
    json!({
        "type": "object",
        "additionalProperties": false,
        "required": ["step", "call", "expect"],
        "properties": {
            "step": { "type": "integer", "minimum": 1 },
            "call": { "type": "string", "minLength": 1 },
            "on": { "type": "string", "pattern": IDENT_PATTERN },
            "variant": { "type": "string" },
            "format": { "enum": tokens(FormatName::ALL) },
            "scopes": {
                "description": "The SMART `scope` claim this step's principal presents (ITS-REST docs/smart_app_launch/master08-scopes.adoc §Resource Scopes), space-joined into a minted RS256 access token by the addressed `bearer_mint` instance. Declaring the key at all — including as an empty list, the scope-less token the fail-closed deny branch needs — marks the step SMART-lane, so a party whose ixit declares no `smart` block records the case not-applicable.",
                "type": "array",
                "items": { "type": "string" }
            },
            "with": { "type": "object" },
            "expect": { "oneOf": [
                { "enum": tokens(OutcomeKind::ALL) },
                { "const": "${fixture.expected}" }
            ] },
            "capture": { "type": "object",
                "propertyNames": { "pattern": IDENT_PATTERN },
                "additionalProperties": { "type": "string" } },
            "assert": { "type": "array", "items": { "$ref": "#/$defs/assertion" } }
        }
    })
}

fn assertion_def() -> Value {
    // The schema pins the closed assertion-form vocabulary; per-form field
    // shapes are enforced by the typed model (richer than JSON Schema can
    // express — predicate exclusivity, reference grammar, aggregate rules).
    json!({
        "type": "object",
        "required": ["assert"],
        "properties": {
            "assert": { "enum": [
                "instance_of", "field", "equivalent", "version", "signature",
                "result_set", "unique", "returns", "xml_root", "message_exemplar",
                "state"
            ] }
        }
    })
}

/// `schedule/**` case cores (§ case-core contract).
#[must_use]
pub fn case_core_schema() -> Value {
    json!({
        "$schema": DRAFT,
        "$id": urn("case-core"),
        "title": "CNF 2.0 case core",
        "description": "One protocol-neutral test case (the Abstract Test Suite unit). Wire realization lives in the operation bindings; cases speak SM operations and outcome kinds only.",
        "type": "object",
        "additionalProperties": false,
        "required": ["id", "kind", "component", "test_purpose", "description", "spec_refs", "capabilities"],
        "properties": {
            "id": { "type": "string", "pattern": CASE_ID_PATTERN },
            "kind": { "enum": tokens(CaseKind::ALL) },
            "status": { "enum": tokens(CaseStatus::ALL), "default": "active" },
            "component": { "enum": tokens(Component::ALL) },
            "sm_operation": { "type": "string", "pattern": SM_OPERATION_PATTERN },
            "rm_class": { "type": "string" },
            "test_purpose": { "type": "string", "minLength": 1 },
            "description": { "type": "string", "minLength": 1 },
            "spec_refs": { "type": "array", "minItems": 1, "items": { "type": "string", "minLength": 1 } },
            "applies": applies_def(),
            "guards": {
                "description": "Spec-cited PROSE about the case, and nothing more: where the expectation was authored from, what the row does not claim, the adjudication it rests on, and the assumptions it makes. It participates in NO selection — the runner reads this property nowhere, at validate time or at drive time. APPLICABILITY IS DECIDED BY THE TYPED FIELDS ALONE: `status` (a case held back is `draft`, reported as its own exception and bearing no verdict), `applies` (spec-version windows), `capabilities` (a case gating only capabilities the ICS does not claim is excused), `option` (the undeclared arm of an `option_select` register branch is excused), the `requires` block (terminology, spec_profile, instances, party_relationship, server exclusivity), and the flow's own `on:` addressing plus its `${ixit:…}` reads. THE TWO BOUNDARIES, each with its validate gate: a guard may not RESTATE one of those typed rules, because a per-case copy is free to drift from the implemented rule with nothing to catch it (`guard-scope`); and a guard may not be PHRASED AS A CONDITION — 'applies only', 'not-applicable', 'guarded until', 'skip where', 'out of scope' — because the hold such a phrasing promises happens nowhere, and a reader who believes it concludes the case is held back while it drives and gates a verdict (`guard-condition`). Every guard carries its citation: an em-dash citation or a named master document.",
                "type": "array",
                "items": { "type": "string", "minLength": 1 }
            },
            "capabilities": nonempty_string_array(IDENT_PATTERN),
            "exercises": string_array(Some(IDENT_PATTERN)),
            "profiles": { "type": "array", "items": { "enum": tokens(Tier::ALL) } },
            "option": { "type": "string", "pattern": OPTION_TAG_PATTERN },
            "formats": { "type": "array", "items": { "enum": tokens(FormatName::ALL) } },
            "requires": { "$ref": "#/$defs/requires" },
            "parameters": { "$ref": "#/$defs/parameters" },
            "flow": { "type": "array", "minItems": 1, "items": { "$ref": "#/$defs/flowStep" } },
            "constraint_context": {
                "type": "object",
                "additionalProperties": false,
                "required": ["template", "path"],
                "properties": {
                    "template": { "type": "string", "pattern": CORPUS_KEY_PATTERN },
                    "path": { "type": "string", "minLength": 1 },
                    "constraint_columns": string_array(None)
                }
            },
            "decision_table": {
                "type": "object",
                "additionalProperties": false,
                "required": ["columns", "rows"],
                "properties": {
                    "columns": { "type": "array", "minItems": 1, "items": { "type": "string" } },
                    "rows": { "type": "array", "minItems": 1, "items": { "type": "array" } }
                }
            },
            "postconditions": { "type": "array", "items": { "$ref": "#/$defs/assertion" } },
            "verified_by": string_array(Some(CASE_ID_PATTERN)),
            "ambiguities": string_array(Some(AMBIGUITY_ID_PATTERN)),
            "data_sets": string_array(Some(CORPUS_KEY_PATTERN))
        },
        "allOf": [
            { "if": { "properties": { "kind": { "const": "functional" } } },
              "then": { "required": ["sm_operation", "flow"] } },
            { "if": { "properties": { "kind": { "const": "content" } } },
              "then": { "required": ["rm_class", "constraint_context", "decision_table"] } }
        ],
        "$defs": {
            "requires": requires_def(),
            "parameters": parameters_def(),
            "flowStep": flow_step_def(),
            "assertion": assertion_def()
        }
    })
}

/// `bindings/<its>/**` operation bindings (§ wire layer).
#[must_use]
pub fn operation_binding_schema() -> Value {
    json!({
        "$schema": DRAFT,
        "$id": urn("operation-binding"),
        "title": "CNF 2.0 operation binding",
        "description": "Per-ITS wire realization of one SM operation: request construction, outcome kind → wire expectation, logical capture → wire source. Every mapping cites its OAS source.",
        "type": "object",
        "additionalProperties": false,
        "required": ["sm_operation", "its"],
        "oneOf": [
            { "required": ["request", "outcomes"], "not": { "required": ["unrealized"] } },
            { "required": ["unrealized"],
              "not": { "anyOf": [ { "required": ["request"] }, { "required": ["outcomes"] }, { "required": ["extension"] } ] } }
        ],
        "properties": {
            "sm_operation": { "type": "string", "pattern": SM_OPERATION_PATTERN },
            "its": { "enum": ["its-rest"] },
            "variant": { "type": "string", "minLength": 1 },
            "applies": applies_def(),
            "unrealized": {
                "type": "object",
                "additionalProperties": false,
                "required": ["reason", "source", "ambiguity"],
                "properties": {
                    "reason": { "type": "string", "minLength": 1 },
                    "source": { "type": "string", "minLength": 1 },
                    "ambiguity": { "type": "string", "pattern": AMBIGUITY_ID_PATTERN }
                }
            },
            "extension": {
                "type": "object",
                "description": "The realization drives a route no openEHR specification governs (our own design/extension), declared as a served_extensions family in vocab/wire_surface.yaml. Mutually exclusive with `unrealized`; the capabilities such a binding's cases carry must be `realization: extension` in the capability matrix, which may never be `required` — so the row gates a CAPABILITY verdict only, never ITS-REST wire conformance.",
                "additionalProperties": false,
                "required": ["family", "reason", "source", "ambiguity"],
                "properties": {
                    "family": { "type": "string", "minLength": 1 },
                    "reason": { "type": "string", "minLength": 1 },
                    "source": { "type": "string", "minLength": 1 },
                    "ambiguity": { "type": "string", "pattern": AMBIGUITY_ID_PATTERN }
                }
            },
            "request": binding_request_def(),
            "formats": { "type": "array", "items": { "enum": tokens(FormatName::ALL) } },
            "format_headers": {
                "type": "object",
                "propertyNames": { "enum": tokens(FormatName::ALL) },
                "additionalProperties": {
                    "type": "object",
                    "additionalProperties": { "type": "string" }
                }
            },
            "outcomes": binding_outcomes_def(),
            "captures": {
                "type": "object",
                "propertyNames": { "pattern": IDENT_PATTERN },
                "additionalProperties": {
                    "type": "object",
                    "additionalProperties": false,
                    "required": ["from"],
                    "properties": {
                        "from": { "type": "string" },
                        "strip": { "enum": ["weak-quotes"] },
                        "transform": { "enum": tokens(crate::model::binding::TransformRule::ALL) },
                        "fallback": { "type": "string" }
                    }
                }
            },
            "server_assigned": { "type": "array", "items": { "type": "string", "minLength": 1 } }
        }
    })
}

/// The `request` member of an operation binding (§ wire layer) — request
/// construction: method, templated path, RFC 6570 query, body source,
/// header templates.
fn binding_request_def() -> Value {
    json!({
        "type": "object",
        "additionalProperties": false,
        "required": ["method", "path"],
        "properties": {
            "method": { "enum": tokens(HttpMethod::ALL) },
            "path": { "type": "string", "pattern": "^/" },
            "query": {
                "type": "object",
                "additionalProperties": { "oneOf": [
                    { "type": "string" },
                    { "type": "array", "minItems": 1, "items": { "type": "string" },
                      "description": "The repeated (RFC 6570 exploded, {?p*}) form: one name=value pair per member, unbound optional members absent" }
                ] }
            },
            "body": { "oneOf": [
                { "type": "string" },
                { "type": "object",
                  "additionalProperties": false,
                  "required": ["from_capture", "set"],
                  "properties": {
                      "from_capture": { "type": "string", "pattern": IDENT_PATTERN },
                      "set": { "type": "object", "minProperties": 1 }
                  } },
                { "type": "object", "not": { "required": ["from_capture"] } }
            ] },
            "headers": { "type": "object", "additionalProperties": { "type": "string" } }
        }
    })
}

/// The `outcomes` member of an operation binding — outcome kind → wire
/// expectation (status, permitted alternates, header matchers, body
/// selector).
fn binding_outcomes_def() -> Value {
    json!({
        "type": "object",
        "minProperties": 1,
        "propertyNames": { "enum": tokens(OutcomeKind::ALL) },
        "additionalProperties": {
            "type": "object",
            "additionalProperties": false,
            "required": ["status"],
            "properties": {
                "status": { "type": "integer", "minimum": 100, "maximum": 599 },
                "alt_status": {
                    "type": "array",
                    "minItems": 1,
                    "items": { "type": "integer", "minimum": 100, "maximum": 599 },
                    "description": "Overview-permitted additional non-conflicting status codes beyond the OAS enumeration (ITS-REST Requests_and_responses §HTTP status codes)"
                },
                "headers": { "type": "object", "additionalProperties": header_expectation_def() },
                "body": { "enum": BODY_SELECTOR_TOKENS }
            }
        }
    })
}

/// `vocab/outcomes.yaml` — all kinds required, none extra.
#[must_use]
pub fn outcomes_schema() -> Value {
    let required: Vec<Value> = OutcomeKind::ALL
        .iter()
        .map(|k| Value::String(k.token().to_owned()))
        .collect();
    json!({
        "$schema": DRAFT,
        "$id": urn("outcomes"),
        "title": "CNF 2.0 outcome-kind vocabulary",
        "description": "The closed outcome taxonomy. Cases speak ONLY these kinds; bindings map each kind to wire per operation. Extension only by schedule release.",
        "type": "object",
        "propertyNames": { "enum": tokens(OutcomeKind::ALL) },
        "required": required,
        "additionalProperties": {
            "type": "object",
            "additionalProperties": false,
            "required": ["class", "meaning"],
            "properties": {
                "class": { "enum": ["success", "error"] },
                "meaning": { "type": "string", "minLength": 1 }
            }
        }
    })
}

/// `vocab/selectors.yaml` — selector/matcher vocabularies + ignore-sets.
#[must_use]
pub fn selectors_schema() -> Value {
    json!({
        "$schema": DRAFT,
        "$id": urn("selectors"),
        "title": "CNF 2.0 selector vocabulary",
        "description": "The closed body-selector and header-matcher vocabularies, plus the named ignore-set registry the `equivalent` assertion resolves (normative, never runner judgment).",
        "type": "object",
        "additionalProperties": false,
        "required": ["body_selectors", "header_matchers", "ignore_sets"],
        "properties": {
            "body_selectors": { "const": BODY_SELECTOR_TOKENS },
            "header_matchers": { "const": HEADER_MATCHER_FORMS },
            "ignore_sets": {
                "type": "object",
                "additionalProperties": false,
                "required": ["server_assigned", "ctx_defaults"],
                "properties": {
                    "server_assigned": { "$ref": "#/$defs/ignoreSet" },
                    "ctx_defaults": { "$ref": "#/$defs/ignoreSet" }
                }
            },
            "universal_outcomes": {
                "type": "object",
                "propertyNames": { "enum": tokens(OutcomeKind::ALL) },
                "additionalProperties": {
                    "type": "object",
                    "additionalProperties": false,
                    "required": ["status", "source"],
                    "properties": {
                        "status": { "type": "integer", "minimum": 100, "maximum": 599 },
                        "source": { "type": "string", "minLength": 1 }
                    }
                }
            }
        },
        "$defs": {
            "ignoreSet": {
                "type": "object",
                "additionalProperties": false,
                "required": ["source"],
                "properties": {
                    "paths": { "type": "array", "items": { "type": "string", "minLength": 1 } },
                    "per_binding": { "type": "boolean" },
                    "source": { "type": "string", "minLength": 1 }
                }
            }
        }
    })
}

/// A capability row's register-linked adjudication block: the register entry
/// that decided the exception plus the reason the certificate renders.
fn register_adjudication() -> Value {
    json!({
        "type": "object",
        "additionalProperties": false,
        "required": ["register", "reason"],
        "properties": {
            "register": { "type": "string", "pattern": AMBIGUITY_ID_PATTERN },
            "reason": { "type": "string", "minLength": 1 }
        }
    })
}

/// The `vocab/capability_matrix.yaml` schema.
///
/// It maps capability → family/tier/required, with
/// family-scoped tiers enforced in-schema, the per-capability depth floor
/// (`min_cases`), the realization marker, and the two register-linked
/// adjudication blocks.
#[must_use]
pub fn capability_matrix_schema() -> Value {
    json!({
        "$schema": DRAFT,
        "$id": urn("capability-matrix"),
        "title": "CNF 2.0 capability matrix",
        "description": "The machine-readable capability→family→tier matrix — the Profiles book's capability×tier tables as data, the input the verdict machinery computes from.",
        "type": "object",
        "minProperties": 1,
        "propertyNames": { "pattern": IDENT_PATTERN },
        "additionalProperties": {
            "type": "object",
            "additionalProperties": false,
            "required": ["family", "tier", "required", "min_cases"],
            "properties": {
                "family": { "enum": ["Platform", "Enterprise", "Security"] },
                "tier": { "enum": tokens(Tier::ALL) },
                "required": { "type": "boolean" },
                "realization": { "enum": tokens(Realization::ALL) },
                "min_cases": { "type": "integer", "minimum": 0 },
                "evidence_exception": register_adjudication(),
                "workload_exclusion": register_adjudication(),
                "source": { "type": "string" }
            },
            "oneOf": [
                { "properties": { "family": { "const": "Platform" },
                                  "tier": { "enum": ["CORE", "STANDARD", "OPTIONS"] } } },
                { "properties": { "family": { "const": "Security" },
                                  "tier": { "enum": ["SEC-BASIC"] } } },
                { "properties": { "family": { "const": "Enterprise" },
                                  "tier": { "enum": ["D", "M", "X"] } } }
            ]
        }
    })
}

/// `corpus/MANIFEST.yaml` — governed corpus entries.
#[must_use]
pub fn corpus_manifest_schema() -> Value {
    json!({
        "$schema": DRAFT,
        "$id": urn("corpus-manifest"),
        "title": "CNF 2.0 corpus manifest",
        "description": "Every fixture and generated set is a manifest entry: adjudicated verdict + defect live here (never only in a filename); generated sets are committed seeded deterministic recipes.",
        "type": "object",
        "minProperties": 1,
        "propertyNames": { "pattern": CORPUS_KEY_PATTERN },
        "additionalProperties": {
            "type": "object",
            "additionalProperties": false,
            "required": ["format", "validity", "provenance"],
            "oneOf": [
                { "required": ["source"], "not": { "required": ["generated_by"] } },
                { "required": ["generated_by"], "not": { "required": ["source"] } }
            ],
            "properties": {
                "source": { "type": "string", "minLength": 1 },
                "generated_by": {
                    "type": "object",
                    "additionalProperties": false,
                    "required": ["recipe", "digest"],
                    "properties": {
                        "recipe": { "type": "string", "pattern": IDENT_PATTERN },
                        "digest": { "type": "string", "minLength": 1 }
                    }
                },
                "format": { "enum": tokens(CorpusFormat::ALL) },
                "template_id": { "type": "string", "minLength": 1 },
                "rm_versions": { "type": "array", "items": { "type": "string" } },
                "validity": {
                    "type": "object",
                    "additionalProperties": false,
                    "required": ["verdict"],
                    "properties": {
                        "verdict": { "enum": ["valid", "invalid"] },
                        "defect": { "type": "string", "minLength": 1 },
                        "spec_ref": { "type": "string", "minLength": 1 }
                    },
                    "if": { "properties": { "verdict": { "const": "invalid" } } },
                    "then": { "required": ["verdict", "defect", "spec_ref"] }
                },
                "placeholders": {
                    "type": "object",
                    "additionalProperties": { "enum": tokens(PlaceholderPolicy::ALL) }
                },
                "provenance": { "type": "string", "minLength": 1 },
                "views": {
                    "type": "object",
                    "propertyNames": { "pattern": IDENT_PATTERN },
                    "additionalProperties": {
                        "type": "object",
                        "additionalProperties": false,
                        "required": ["select"],
                        "properties": {
                            "select": { "type": "string", "minLength": 1 },
                            "where": { "type": "string" },
                            "order_by": { "type": "string" }
                        }
                    }
                },
                "recipes": {
                    "type": "object",
                    "propertyNames": { "pattern": IDENT_PATTERN },
                    "additionalProperties": {
                        "type": "object",
                        "additionalProperties": false,
                        "required": ["digest"],
                        "properties": { "digest": { "type": "string", "minLength": 1 } }
                    }
                }
            }
        }
    })
}

/// `registers/ambiguities.yaml` — the ambiguity register.
#[must_use]
pub fn ambiguity_register_schema() -> Value {
    json!({
        "$schema": DRAFT,
        "$id": urn("ambiguity-register"),
        "title": "CNF 2.0 ambiguity register",
        "description": "Every entry a real, verified spec divergence or silence with the normative handling a runner must apply. A runner that resolves an ambiguity privately is non-conformant to the schedule.",
        "type": "object",
        "minProperties": 1,
        "propertyNames": { "pattern": AMBIGUITY_ID_PATTERN },
        "additionalProperties": {
            "type": "object",
            "additionalProperties": false,
            "required": ["ambiguity", "source", "handling", "disposition"],
            "properties": {
                "ambiguity": { "type": "string", "minLength": 1 },
                "source": { "type": "string", "minLength": 1, "description": "Where the silence/divergence was verified. Convention (machine-gated by the validate spec-ref check, issue FerroEHR#2545): the field splits into `;`/` + ` fragments; every fragment opening with a spec component token (RM, BASE, AM, QUERY, TERM, LANG, SM, CNF, ITS-REST, ITS-XML, ITS-JSON) is a citation clause and must machine-resolve against the vendored trees (document + § sections; {a,b} brace shorthands expand and every variant must resolve); any other fragment is adjudication prose and passes. A source with no citation clause at all fails — a silence claim must ground on at least one resolvable citation." },
                "handling": { "type": "string", "minLength": 1 },
                "disposition": { "enum": tokens(Disposition::ALL), "description": "The machine-readable handling class. The fixed_handling/editorial boundary (issue FerroEHR#2546) is one question — did the entry CHOOSE anything? editorial = a wording/typography defect with zero behavioural latitude (the corrected reading is forced by the surrounding released text); fixed_handling = real latitude existed and the entry pins this catalogue's choice. Neither changes gating; report_only and editorial additionally require an upstream_issue." },
                "options": { "type": "object",
                             "description": "For `option_select` only: the option FAMILIES the entry branches into, family name to that family's mutually exclusive arms. One entry adjudicates one ambiguity and may leave several independent choices open at once (AMB-167 leaves ten, one per REST resource family), so the arms are grouped rather than pooled: a pooled list cannot say which arms are alternatives to which, and a declaration answering one choice then looks like it answered them all. A declaration answers EVERY family whose sibling cases its claim reaches, with exactly one arm each.",
                             "propertyNames": { "pattern": OPTION_TAG_PATTERN },
                             "additionalProperties": { "type": "array", "minItems": 2, "uniqueItems": true,
                                                       "items": { "type": "string", "pattern": OPTION_TAG_PATTERN } } },
                "upstream_issue": { "type": "integer", "minimum": 1 }
            },
            "allOf": [
                { "if": { "properties": { "disposition": { "const": "option_select" } } },
                  "then": { "required": ["ambiguity", "source", "handling", "disposition", "options"],
                            "properties": { "options": { "minProperties": 1 } } },
                  "else": { "properties": { "options": { "maxProperties": 0 } } } },
                { "if": { "properties": { "disposition": { "enum": ["report_only", "editorial"] } }, "required": ["disposition"] },
                  "then": { "required": ["upstream_issue"] } }
            ]
        }
    })
}

/// A `{ its, formats }` technology-profile object schema, as a party DECLARES
/// one.
fn tech_profile_def() -> Value {
    json!({
        "type": "object",
        "additionalProperties": false,
        "required": ["its"],
        "properties": {
            "its": { "enum": tokens(ItsName::ALL) },
            "formats": { "type": "array", "items": { "enum": tokens(FormatName::ALL) } }
        }
    })
}

/// The technology profile a results document RECORDS: the declared shape plus
/// the provenance of the recorded format list.
fn recorded_tech_profile_def() -> Value {
    json!({
        "type": "object",
        "additionalProperties": false,
        "required": ["its"],
        "properties": {
            "its": { "enum": tokens(ItsName::ALL) },
            "formats": { "type": "array", "items": { "enum": tokens(FormatName::ALL) } },
            "source": {
                "enum": tokens(TechProfileSource::ALL),
                "description": "Where the recorded format list came from: `declared` when the party statement's tech_profiles entry for this ITS named it, `defaulted` when no declaration named this ITS and every format the instrument speaks was recorded instead. The recorded list is what the verdict pipeline selects gating records with, so the two carry different claims: a defaulted list keeps a red row in an unlisted format from vanishing behind a pass, and it is not a party's claim about the formats it serves. Absent only in a document written before the member existed, where absence is unknown and never either source."
            }
        }
    })
}

/// The declared spec-version object: fixed component keys, each a string.
fn spec_versions_def() -> Value {
    let mut props = serde_json::Map::new();
    for component in SpecComponent::ALL {
        props.insert(token_str(component), json!({ "type": "string" }));
    }
    json!({
        "type": "object",
        "additionalProperties": false,
        "properties": Value::Object(props)
    })
}

/// `statement.json` — the party statement (ICS + `SDoC`).
#[must_use]
pub fn statement_schema() -> Value {
    json!({
        "$schema": DRAFT,
        "$id": urn("statement"),
        "title": "CNF 2.0 party statement (ICS + SDoC)",
        "description": "The supplier's Implementation Conformance Statement (the verdict-bearing claims) plus the SDoC self-declaration. The canonical JSON interchange artifact the verdict machinery consumes; verdicts are computed from it, never asserted here.",
        "type": "object",
        "additionalProperties": false,
        "required": ["product", "schedule_release", "claims"],
        "properties": {
            "product": {
                "type": "object",
                "additionalProperties": false,
                "required": ["name", "version", "vendor", "identifier"],
                "properties": {
                    "name": { "type": "string", "minLength": 1 },
                    "version": { "type": "string", "minLength": 1 },
                    "vendor": { "type": "string", "minLength": 1 },
                    "identifier": { "type": "string", "minLength": 1 }
                }
            },
            "schedule_release": { "type": "string", "minLength": 1 },
            "spec_versions": spec_versions_def(),
            "claims": {
                "type": "object",
                "additionalProperties": false,
                "properties": {
                    "capabilities": string_array(Some(IDENT_PATTERN)),
                    "profiles": { "type": "array", "items": { "enum": tokens(Tier::ALL) } }
                }
            },
            "tech_profiles": { "type": "array", "items": tech_profile_def() },
            "options": string_array(Some(OPTION_TAG_PATTERN)),
            "served_extensions": {
                "description": "The route families THIS party declares it serves beyond the openEHR resource set, by their vocab/wire_surface.yaml served_extensions family name. A declaration, never a claim: no verdict reads it, and the statement renders only what the party itself declares — the catalogue axis is one product's outward surface and is never published as another vendor's. An unresolvable family name is a validation finding; an absent or empty list renders as an explicit declaration of none.",
                "type": "array",
                "items": { "type": "string", "minLength": 1 }
            },
            "performance": {
                "type": "object",
                "additionalProperties": false,
                "required": ["class", "environment_ref"],
                "properties": {
                    "class": { "enum": perf_class_tokens() },
                    "environment_ref": { "type": "string", "minLength": 1 }
                }
            },
            "non_functional": { "type": "object" },
            "evidence": {
                "type": "array",
                "items": {
                    "type": "object",
                    "additionalProperties": false,
                    "required": ["results_path", "sha256"],
                    "properties": {
                        "results_path": { "type": "string", "minLength": 1 },
                        "sha256": { "type": "string", "minLength": 1 }
                    }
                }
            },
            "attestation": {
                "type": "object",
                "additionalProperties": false,
                "required": ["signatory", "role", "date", "statement"],
                "properties": {
                    "signatory": { "type": "string", "minLength": 1 },
                    "role": { "type": "string", "minLength": 1 },
                    "date": { "type": "string", "minLength": 1 },
                    "statement": { "type": "string", "minLength": 1 }
                }
            }
        }
    })
}

/// One case×format outcome record, as the results document and the evidence
/// bundle both carry it.
fn outcome_record_def() -> Value {
    json!({
        "type": "object",
        "additionalProperties": false,
        "required": ["case", "status", "rows_driven", "rows_total"],
        "properties": {
            "case": { "type": "string", "pattern": CASE_ID_PATTERN },
            "format": { "enum": tokens(FormatName::ALL) },
            "status": { "enum": tokens(OutcomeStatus::ALL) },
            "rows_driven": { "type": "integer", "minimum": 0 },
            "rows_total": { "type": "integer", "minimum": 0 },
            "failing_step": { "type": "integer", "minimum": 0 },
            "reason": { "type": "string" },
            "citation": { "type": "string" },
            "failed_rows": {
                "type": "array",
                "items": {
                    "type": "object",
                    "additionalProperties": false,
                    "required": ["row", "step", "reason"],
                    "properties": {
                        "row": { "type": "integer", "minimum": 0 },
                        "step": { "type": "integer", "minimum": 0 },
                        "reason": { "type": "string", "minLength": 1 }
                    }
                }
            }
        },
        "allOf": [
            { "if": { "properties": { "status": { "enum": ["skipped", "not_applicable"] } },
                      "required": ["status"] },
              "then": { "required": ["case", "status", "rows_driven", "rows_total", "citation"],
                        "properties": { "citation": { "type": "string", "minLength": 1 } } } }
        ]
    })
}

/// `results.json` — the party results (the campaign outcomes).
#[must_use]
pub fn results_schema() -> Value {
    json!({
        "$schema": DRAFT,
        "$id": urn("results"),
        "title": "CNF 2.0 party results",
        "description": "The campaign outcomes for one technology profile: per-case×format outcome records (with mandatory citations on skipped/not_applicable), the ambiguity dispositions applied, and provenance (SUT/runner/ixit digest).",
        "type": "object",
        "additionalProperties": false,
        "required": ["sut", "runner", "schedule_release", "tech_profile", "ixit_digest"],
        "properties": {
            "sut": {
                "type": "object",
                "additionalProperties": false,
                "required": ["name", "version"],
                "properties": {
                    "name": { "type": "string", "minLength": 1 },
                    "version": { "type": "string", "minLength": 1 }
                }
            },
            "runner": {
                "type": "object",
                "additionalProperties": false,
                "required": ["name", "version", "verification_pack_status"],
                "properties": {
                    "name": { "type": "string", "minLength": 1 },
                    "version": { "type": "string", "minLength": 1 },
                    "verification_pack_status": { "enum": tokens(VerificationPackStatus::ALL) }
                }
            },
            "schedule_release": { "type": "string", "minLength": 1 },
            "tech_profile": recorded_tech_profile_def(),
            "ixit_digest": {
                "type": "string",
                "pattern": "^[0-9a-f]{16}$",
                "description": "The leading 8 bytes of the SHA-256 over the ixit declaration this campaign was driven under, lowercase hex, computed over the document's bytes with nothing canonicalized first. A reader holding that declaration re-derives it with `sha256sum ixit.json | cut -c1-16`."
            },
            "statement_digest": {
                "type": "string",
                "pattern": "^[0-9a-f]{16}$",
                "description": "The leading 8 bytes of the SHA-256 over the party statement this campaign was selected under, lowercase hex, computed over the document's bytes with nothing canonicalized first. A reader holding that statement re-derives it with `sha256sum statement.json | cut -c1-16`, which is what tells one claim from another claim declaring the same its-rest formats. Absent for a campaign no statement selected (selection_basis statement_blind), and absent in a document written before the member existed, where the identity is unknown and never a match."
            },
            "selection_basis": {
                "enum": tokens(SelectionBasis::ALL),
                "description": "What ISO/IEC 9646 test selection had to select this campaign with: `statement` when a party statement (ICS) was supplied, `statement_blind` when none was. A blind campaign is a sweep of the whole catalogue rather than a party-scoped record — nothing selected the party's option arms, extension routes, claimed capabilities or release floors, and the cases whose failure that absence would manufacture (an unselected arm of a mutually exclusive option_select branch, an extension route no released text governs) are recorded not_applicable with their citation. Absent only in a document written before the member existed, where absence is unknown and never either basis."
            },
            "restapi_specs_version": {
                "type": "string",
                "minLength": 1,
                "description": "The restapi_specs_version member the SUT's System OPTIONS manifest served during the campaign, when that exchange was driven (released OAS system.openapi.yaml Options — every member optional, so absence is normal). An independent confirmation of the statement's declared spec_versions.its_rest, never a source of truth: divergence from the declaration is a static-review finding, not a re-declaration."
            },
            "outcomes": {
                "type": "array",
                "items": outcome_record_def()
            },
            "measurements": {
                "type": "array",
                "items": {
                    "type": "object",
                    "additionalProperties": false,
                    "required": ["case", "class", "environment", "offered_load_sustained",
                                  "warmup_s", "duration_s", "operations", "verdict"],
                    "properties": {
                        "case": { "type": "string", "pattern": CASE_ID_PATTERN },
                        "class": { "enum": perf_class_tokens() },
                        "environment": environment_def(),
                        "offered_load_sustained": { "type": "number", "minimum": 0.0 },
                        "warmup_s": { "type": "integer", "minimum": 0 },
                        "duration_s": { "type": "integer", "minimum": 1 },
                        "operations": {
                            "type": "array",
                            "minItems": 1,
                            "items": operation_measurement_def()
                        },
                        "verdict": { "enum": ["earned", "not-earned"] },
                        "violations": { "type": "array", "items": { "type": "string", "minLength": 1 } },
                        "resources": resources_def()
                    }
                }
            },
            "ambiguity_dispositions": {
                "description": "The ambiguity dispositions the campaign applied: one record per option_select register arm the party's ICS declared, in the register's authored order. The arms of one option family are mutually exclusive, so which arm a deployment serves is the ICS's answer and the record says which answer it drove under. Empty for a campaign no statement selected, which declares no arm at all.",
                "type": "array",
                "items": {
                    "type": "object",
                    "additionalProperties": false,
                    "required": ["ambiguity"],
                    "properties": {
                        "ambiguity": { "type": "string", "pattern": AMBIGUITY_ID_PATTERN },
                        "option": { "type": "string", "pattern": OPTION_TAG_PATTERN }
                    }
                }
            }
        }
    })
}

/// The version-signing posture block — declared party-wide (the default) and
/// optionally per instance (the deployment that runs a different mode). One
/// definition, two placements, so the two can never drift apart.
fn signing_def(description: &str) -> Value {
    json!({
        "description": description,
        "type": "object",
        "required": ["mode"],
        "oneOf": [
            { "additionalProperties": false, "required": ["mode", "algorithm", "encoding"],
              "properties": { "mode": { "const": "digest" },
                              "algorithm": { "type": "string", "minLength": 1 },
                              "encoding": { "type": "string", "minLength": 1 },
                              "prefix": { "type": "string" } } },
            { "additionalProperties": false, "required": ["mode", "public_key"],
              "properties": { "mode": { "const": "pgp" },
                              "public_key": { "type": "string", "minLength": 1 } } }
        ]
    })
}

/// The openEHR specification generation set — declared party-wide (the
/// default) and optionally per instance (the deployment running the other
/// set). One definition, two placements, exactly like [`signing_def`], so the
/// two can never drift apart.
fn spec_profile_def(description: &str) -> Value {
    json!({
        "description": description,
        "enum": tokens(crate::ixit::SpecProfile::ALL)
    })
}

/// The terminology posture block — declared party-wide (the default) and
/// optionally per instance (the deployment that runs the other
/// unresolvable-value-set posture). One definition, two placements, exactly
/// like [`signing_def`], so the two can never drift apart.
fn terminology_def(description: &str) -> Value {
    json!({
        "description": description,
        "type": "object",
        "additionalProperties": false,
        "required": ["posture", "servers"],
        "properties": {
            "posture": {
                "description": "What the deployment does with a bound value set it cannot resolve. BASE architecture_overview master12 §Binding Terminology Value-sets to Archetypes puts the query in an external terminology query server and never says what happens when it cannot be reached, so the branch is a deployment fact, not a spec rule (register AMB-172).",
                "enum": tokens(crate::ixit::TerminologyPosture::ALL)
            },
            "servers": {
                "description": "The terminology servers this deployment is wired to. A namespace is whatever key a case names for a terminology (code-system URI, value-set URL, terminology id); a DECLARED-unreachable server is how the terminology-server-down branch is exercised for the whole run, never by a mid-run reconfiguration.",
                "type": "array",
                "minItems": 1,
                "items": {
                    "type": "object",
                    "additionalProperties": false,
                    "required": ["name", "namespaces"],
                    "properties": {
                        "name": { "type": "string", "minLength": 1 },
                        "reachable": { "type": "boolean" },
                        "namespaces": { "type": "array", "minItems": 1, "items": { "type": "string", "minLength": 1 } }
                    }
                }
            }
        }
    })
}

/// `ixit.json` — the SUT topology the runner drives (ISO/IEC 9646 IXIT).
/// The schema validates exactly what [`crate::ixit::Ixit`] parses.
#[must_use]
pub fn ixit_schema() -> Value {
    json!({
        "$schema": DRAFT,
        "$id": urn("ixit"),
        "title": "CNF 2.0 IXIT (implementation extra information for testing)",
        "description": "The SUT topology a runner drives: named instances (base URL + auth mode, credentials by env-var reference, never inline) plus the environment block. The default instance `sut` is required for every run.",
        "type": "object",
        "additionalProperties": false,
        "required": ["instances"],
        "properties": {
            "instances": ixit_instances_def(),
            "environment": environment_def(),
            "containers": {
                "type": "object",
                "additionalProperties": false,
                "required": ["sut", "db"],
                "properties": {
                    "sut": { "type": "string", "minLength": 1 },
                    "db": { "type": "string", "minLength": 1 }
                }
            },
            "system_id": {
                "description": "The SUT's own configured system identifier — the value it stamps into AUDIT_DETAILS.system_id when the client supplies none (ITS-REST Requests_and_responses §openehr-version and openehr-audit-details) and into every OBJECT_VERSION_ID.creating_system_id it mints. Declared here because no released operation discloses it; absent => the cases reading ${ixit:system_id} are not-applicable with that citation.",
                "type": "string",
                "minLength": 1
            },
            "dump_location": {
                "description": "A location on the SUT's OWN file system the admin dump/load operations may write an archive to and read one back from (SM i_admin_dump_load.adoc export_ehrs(file_sys_loc)/load_ehrs(file_sys_loc), whose only declared error is file_not_writable). Declared here because which paths a deployment can write is a property of its image and mounts that no operation discloses; absent => the cases reading ${ixit:dump_location} are not-applicable with that citation.",
                "type": "string",
                "minLength": 1
            },
            "signing": signing_def(
                "The party's DEFAULT version-signing posture (RM common master06 §Digital Signature). Present => the Signing capability is claimed and this block declares the mode every instance runs unless it declares its own. digest: self-describing plain digest (algorithm/encoding/prefix); pgp: openPGP verified against the public key."
            ),
            "terminology": terminology_def(
                "The party's DEFAULT terminology posture: the terminology query servers this deployment is wired to (BASE architecture_overview master12 §Binding Terminology Value-sets to Archetypes — the bound value set is resolved by a server outside the CDR), which namespaces each answers for, and the unresolvable-value-set branch it realizes. Declared here because released ITS-REST 1.1.0 surfaces no terminology resource, so nothing on the wire discloses any of it; absent => every terminology-dependent case is not-applicable with that citation."
            ),
            "spec_profile": spec_profile_def(
                "The party's DEFAULT openEHR specification generation set: `stable` = the latest RELEASED generations, `development` = the development generations, which admit surface no release defines yet. Declared here because no released operation discloses which set a deployment runs (the openEHR release strategy makes a minor release a compatible superset, so the sets differ only in accepted surface); absent => every case whose expectation rests on a generation set is not-applicable with that citation. An instance may override it."
            ),
            "smart": ixit_smart_def()
        }
    })
}

/// The `instances` member of the ixit — the named SUT deployments/principals
/// (base URL + auth mode with credentials by env-var reference, never
/// inline), each optionally carrying its own `signing`/`terminology` posture
/// (instance-first resolution, the party default as fallback).
fn ixit_instances_def() -> Value {
    json!({
        "type": "object",
        "minProperties": 1,
        "propertyNames": { "pattern": IDENT_PATTERN },
        "additionalProperties": {
            "type": "object",
            "additionalProperties": false,
            "required": ["base_url", "auth"],
            "properties": {
                "base_url": { "type": "string", "minLength": 1 },
                "auth": {
                    "type": "object",
                    "required": ["mode"],
                    "oneOf": [
                        { "additionalProperties": false, "required": ["mode"],
                          "properties": { "mode": { "const": "none" } } },
                        { "additionalProperties": false,
                          "required": ["mode", "user_env", "password_env"],
                          "properties": { "mode": { "const": "basic" },
                                          "user_env": { "type": "string", "minLength": 1 },
                                          "password_env": { "type": "string", "minLength": 1 } } },
                        { "additionalProperties": false, "required": ["mode", "token_env"],
                          "properties": { "mode": { "const": "bearer" },
                                          "token_env": { "type": "string", "minLength": 1 } } },
                        { "additionalProperties": false, "required": ["mode"],
                          "properties": { "mode": { "const": "bearer_mint" },
                                          "subject": { "type": "string", "minLength": 1 },
                                          "roles": { "type": "array", "items": { "type": "string", "minLength": 1 } },
                                          "default_scopes": { "type": "array", "items": { "type": "string" } } } }
                    ]
                },
                "headers": { "type": "object", "additionalProperties": { "type": "string" } },
                "signing": signing_def(
                    "THIS instance's version-signing posture, when it differs from the party default (RM common master06 §Digital Signature: the mode is a deployment fact, and a deployment runs one). A party claiming both modes declares two deployments as two instances, each with its own block; every signature check resolves instance-first, party default second. Absent => the top-level `signing` applies."
                ),
                "terminology": terminology_def(
                    "THIS instance's terminology posture, when it differs from the party default. The unresolvable-value-set branch is one switch per running deployment, so a party exercising both runs two deployments and declares each one's posture on its own instance — the same law the `signing` block follows. Absent => the top-level `terminology` applies."
                ),
                "spec_profile": spec_profile_def(
                    "THIS instance's openEHR specification generation set, when it differs from the party default. One running deployment implements exactly one set, so a party claiming both runs two deployments and declares each one's set on its own instance — the same law the `signing` and `terminology` blocks follow. Absent => the top-level `spec_profile` applies."
                ),
                "administrative": {
                    "description": "Whether this instance's principal holds administrative authorization (register AMB-228). SM master02-overview.adoc §Functional Style delegates access control to the implementation, so the posture is an IXIT declaration; a role-boundary case states the posture it needs in requires, and an undeclared or opposite declaration records it not-applicable with that citation. Absent => undeclared, never a default.",
                    "type": "boolean"
                }
            }
        }
    })
}

/// The `smart` member of the ixit — the party's SMART App Launch lane
/// (ITS-REST `docs/smart_app_launch`): the Platform instance + the static
/// test issuer the runner mints per-step scoped tokens against.
fn ixit_smart_def() -> Value {
    json!({
        "description": "The party's SMART App Launch lane (ITS-REST docs/smart_app_launch). Present => the deployment runs the CDR's SMART resource-server role and trusts the declared static test issuer, so the runner may mint per-step scoped access tokens (the CDR never issues them — master06 §Supported Authentication Flows makes that the Authorization Server's duty, and the conformance stack runs none). Absent => every SMART case is not-applicable with that citation.",
        "type": "object",
        "additionalProperties": false,
        "required": ["platform_instance", "mint"],
        "properties": {
            "platform_instance": {
                "description": "The instance whose base_url is the SMART Platform base URL — master04 §Service Discovery serves /.well-known/smart-configuration relative to it, not to the openEHR REST base the other instances address.",
                "type": "string",
                "pattern": IDENT_PATTERN
            },
            "mint": {
                "description": "The static test issuer the `bearer_mint` instances sign RS256 access tokens with. Committed test material, never production key material.",
                "type": "object",
                "additionalProperties": false,
                "required": ["issuer", "subject", "key_file", "kid", "ttl_seconds"],
                "properties": {
                    "issuer": { "type": "string", "minLength": 1 },
                    "audience": { "type": "string", "minLength": 1 },
                    "subject": { "type": "string", "minLength": 1 },
                    "roles": { "type": "array", "items": { "type": "string", "minLength": 1 } },
                    "key_file": { "type": "string", "minLength": 1 },
                    "kid": { "type": "string", "minLength": 1 },
                    "ttl_seconds": { "type": "integer", "minimum": 1 }
                }
            }
        }
    })
}

/// The resource-telemetry block of one measurement record or stress step
/// — measured CONTEXT, never verdict-bearing: per-container CPU/RSS/I/O
/// series on a fixed cadence (run-clock offsets, phase-stamped) plus, on
/// measured class runs only, the database volume's four disk anchors
/// (stress steps stay anchor-free — exploration stays light). Optional:
/// absent when the ixit declares no `containers` block or the container
/// runtime was unreachable.
fn resources_def() -> Value {
    let byte_counter = json!({ "type": "integer", "minimum": 0 });
    json!({
        "type": "object",
        "additionalProperties": false,
        "required": ["sample_interval_s", "containers"],
        "properties": {
            "sample_interval_s": { "type": "integer", "minimum": 1 },
            "containers": {
                "type": "array",
                "items": {
                    "type": "object",
                    "additionalProperties": false,
                    "required": ["role", "name", "samples"],
                    "properties": {
                        "role": { "enum": tokens(crate::perf::ContainerRole::ALL) },
                        "name": { "type": "string", "minLength": 1 },
                        "samples": {
                            "type": "array",
                            "items": {
                                "type": "object",
                                "additionalProperties": false,
                                "required": ["offset_s", "phase", "cpu_pct", "rss_bytes",
                                              "blk_read_bytes", "blk_write_bytes",
                                              "net_rx_bytes", "net_tx_bytes"],
                                "properties": {
                                    "offset_s": { "type": "integer", "minimum": 0 },
                                    "phase": { "enum": tokens(crate::perf::ResourcePhase::ALL) },
                                    "cpu_pct": { "type": "number", "minimum": 0.0 },
                                    "rss_bytes": byte_counter.clone(),
                                    "blk_read_bytes": byte_counter.clone(),
                                    "blk_write_bytes": byte_counter.clone(),
                                    "net_rx_bytes": byte_counter.clone(),
                                    "net_tx_bytes": byte_counter.clone()
                                }
                            }
                        }
                    }
                }
            },
            "disk": {
                "type": "object",
                "additionalProperties": false,
                "properties": {
                    "before_scale_seed_bytes": byte_counter.clone(),
                    "after_scale_seed_bytes": byte_counter.clone(),
                    "after_ward_seed_bytes": byte_counter.clone(),
                    "after_window_bytes": byte_counter.clone(),
                    "seed_compositions": { "type": "integer", "minimum": 1 }
                }
            }
        }
    })
}

/// The ixit environment block (shared by the ixit schema and every
/// measurement record — an earned class is reported WITH its environment).
fn environment_def() -> Value {
    json!({
        "type": "object",
        "additionalProperties": false,
        "required": ["hardware_class", "cores", "memory_gb", "storage_class", "topology"],
        "properties": {
            "exclusive_server": { "type": "boolean" },
            "hardware_class": { "type": "string", "minLength": 1 },
            "cores": { "type": "integer", "minimum": 0 },
            "memory_gb": { "type": "integer", "minimum": 0 },
            "storage_class": { "type": "string", "minLength": 1 },
            "topology": { "type": "string", "minLength": 1 }
        }
    })
}

/// One per-operation measurement record (shared by the results.json
/// `measurements` block and the stress report's steps): counts, summary
/// percentiles, and the re-checkable base64 HDR V2 histogram.
fn operation_measurement_def() -> Value {
    json!({
        "type": "object",
        "additionalProperties": false,
        "required": ["operation", "requests", "errors",
                      "latency_ms_p50", "latency_ms_p90",
                      "latency_ms_p99", "hdr_v2_base64"],
        "properties": {
            "operation": { "type": "string", "minLength": 1 },
            "requests": { "type": "integer", "minimum": 0 },
            "errors": { "type": "integer", "minimum": 0 },
            "latency_ms_p50": { "type": "number", "minimum": 0.0 },
            "latency_ms_p90": { "type": "number", "minimum": 0.0 },
            "latency_ms_p99": { "type": "number", "minimum": 0.0 },
            "hdr_v2_base64": { "type": "string", "minLength": 1 }
        }
    })
}

/// `stress.json` — the step-load stress report (exploration only: the
/// maximum sustainable throughput, never a conformance record).
#[must_use]
pub fn stress_schema() -> Value {
    json!({
        "$schema": DRAFT,
        "$id": urn("stress"),
        "title": "CNF 2.0 stress report",
        "description": "The step-load stress instrument's exploration artifact: geometric load steps (each with re-checkable HDR V2 records) to the maximum sustainable throughput — where the system breaks — run on a class-scale corpus, environment-bound; never a conformance record, and floor-free by design.",
        "type": "object",
        "additionalProperties": false,
        "required": ["corpus", "environment", "step_warmup_s", "step_hold_s",
                      "p99_budget_ms", "error_budget", "steps",
                      "max_sustainable_throughput_per_s", "ladder_capped",
                      "generator_bound", "remark"],
        "properties": {
            "corpus": { "type": "string", "pattern": CORPUS_KEY_PATTERN },
            "environment": environment_def(),
            "step_warmup_s": { "type": "integer", "minimum": 0 },
            "step_hold_s": { "type": "integer", "minimum": 1 },
            "p99_budget_ms": { "type": "number", "exclusiveMinimum": 0.0 },
            "error_budget": { "type": "number", "minimum": 0.0 },
            "steps": {
                "type": "array",
                "minItems": 1,
                "items": {
                    "type": "object",
                    "additionalProperties": false,
                    "required": ["rate", "offered_load_sustained", "operations",
                                  "stable", "generator_bound"],
                    "properties": {
                        "rate": { "type": "number", "exclusiveMinimum": 0.0 },
                        "offered_load_sustained": { "type": "number", "minimum": 0.0 },
                        "operations": {
                            "type": "array",
                            "minItems": 1,
                            "items": operation_measurement_def()
                        },
                        "stable": { "type": "boolean" },
                        "breaches": { "type": "array", "items": { "type": "string", "minLength": 1 } },
                        "generator_bound": { "type": "boolean" },
                        "resources": resources_def()
                    }
                }
            },
            "max_sustainable_throughput_per_s": { "type": "number", "minimum": 0.0 },
            "ladder_capped": { "type": "boolean" },
            "generator_bound": { "type": "boolean" },
            "remark": { "type": "string", "minLength": 1 }
        }
    })
}

/// `aql-probe.json` — the AQL optimization probe report (exploration
/// evidence: wire percentiles + per-statement DB attribution over the
/// seeded corpus; never a conformance record).
#[must_use]
pub fn aql_probe_schema() -> Value {
    json!({
        "$schema": DRAFT,
        "$id": urn("aql-probe"),
        "title": "CNF 2.0 AQL probe report",
        "description": "The seeded-corpus AQL optimization probe: per-probe wire-latency percentiles and pg_stat_statements attribution through the container runtime, environment-bound. Exploration evidence for the optimization loop — never a conformance record; results.json is never touched.",
        "type": "object",
        "additionalProperties": false,
        "required": ["corpus", "environment", "requests_per_probe", "maintenance_settled",
                      "attribution", "probes", "remark"],
        "properties": {
            "corpus": { "type": "string", "pattern": CORPUS_KEY_PATTERN },
            "environment": environment_def(),
            "requests_per_probe": { "type": "integer", "minimum": 1 },
            "maintenance_settled": { "type": "boolean" },
            "attribution": { "type": "string", "minLength": 1 },
            "probes": {
                "type": "array",
                "minItems": 1,
                "items": {
                    "type": "object",
                    "additionalProperties": false,
                    "required": ["name", "aql", "failures", "wire_ms"],
                    "properties": {
                        "name": { "type": "string", "minLength": 1 },
                        "aql": { "type": "string", "minLength": 1 },
                        "failures": { "type": "integer", "minimum": 0 },
                        "wire_ms": {
                            "type": "object",
                            "additionalProperties": false,
                            "required": ["min_ms", "p50_ms", "p95_ms", "max_ms"],
                            "properties": {
                                "min_ms": { "type": "number", "minimum": 0.0 },
                                "p50_ms": { "type": "number", "minimum": 0.0 },
                                "p95_ms": { "type": "number", "minimum": 0.0 },
                                "max_ms": { "type": "number", "minimum": 0.0 }
                            }
                        },
                        "statements": {
                            "type": "array",
                            "items": {
                                "type": "object",
                                "additionalProperties": false,
                                "required": ["sql", "calls", "mean_ms", "total_ms",
                                              "shared_blks_hit", "shared_blks_read"],
                                "properties": {
                                    "sql": { "type": "string", "minLength": 1 },
                                    "calls": { "type": "integer", "minimum": 0 },
                                    "mean_ms": { "type": "number", "minimum": 0.0 },
                                    "total_ms": { "type": "number", "minimum": 0.0 },
                                    "shared_blks_hit": { "type": "integer", "minimum": 0 },
                                    "shared_blks_read": { "type": "integer", "minimum": 0 }
                                }
                            }
                        }
                    }
                }
            },
            "remark": { "type": "string", "minLength": 1 }
        }
    })
}

/// The published performance-class tokens, ladder order.
fn perf_class_tokens() -> Vec<&'static str> {
    crate::perf::PerfClass::ALL
        .iter()
        .map(|c| c.token())
        .collect()
}

/// `schedule/performance/**` performance cases (conformance-by-measurement).
#[must_use]
pub fn performance_case_schema() -> Value {
    json!({
        "$schema": DRAFT,
        "$id": urn("performance-case"),
        "title": "CNF 2.0 performance case",
        "description": "A kind: performance case — open-loop offered load against class thresholds; verdicts are measured (earned | not-earned), bound to the ixit environment block.",
        "type": "object",
        "additionalProperties": false,
        "required": ["id", "kind", "component", "description", "test_purpose", "spec_refs", "class", "corpus", "workload", "thresholds"],
        "properties": {
            "id": { "type": "string", "pattern": CASE_ID_PATTERN },
            "kind": { "const": "performance" },
            "component": { "const": "PERFORMANCE" },
            "description": { "type": "string", "minLength": 1 },
            "test_purpose": { "type": "string", "minLength": 1 },
            "spec_refs": { "type": "array", "minItems": 1, "items": { "type": "string", "minLength": 1 } },
            "class": { "enum": ["POC", "S", "L", "R"] },
            "corpus": { "type": "string", "pattern": CORPUS_KEY_PATTERN },
            "workload": {
                "type": "object",
                "additionalProperties": false,
                "required": ["arrival_rate", "warmup", "duration", "journeys"],
                "properties": {
                    "arrival_rate": { "type": "string", "pattern": "^[0-9.]+/s$" },
                    "warmup": { "type": "string", "pattern": "^PT([0-9]+H)?([0-9]+M)?([0-9]+S)?$" },
                    "duration": { "type": "string", "pattern": "^PT([0-9]+H)?([0-9]+M)?([0-9]+S)?$" },
                    "arrival_curve": { "enum": ["uniform", "diurnal"] },
                    "journeys": { "type": "object", "minProperties": 1,
                              "propertyNames": { "pattern": IDENT_PATTERN },
                              "additionalProperties": { "type": "string", "pattern": "^[0-9.]+%$" } }
                }
            },
            "thresholds": {
                "type": "array",
                "minItems": 1,
                "items": {
                    "type": "object",
                    "additionalProperties": false,
                    "required": ["metric"],
                    "properties": {
                        "metric": { "enum": ["latency_p50", "latency_p90", "latency_p99", "error_rate", "offered_load_sustained"] },
                        "operation": { "type": "string" },
                        "max": { "type": "number" },
                        "min": { "type": "number" }
                    }
                }
            }
        }
    })
}

/// The published journey-operation tokens, vocabulary order.
fn perf_op_tokens() -> Vec<&'static str> {
    crate::perf::PerfOp::ALL
        .iter()
        .map(|op| op.as_str())
        .collect()
}

/// `vocab/journey_catalogue.yaml` — the hospital-simulation journey
/// vocabulary the performance workloads decompose into.
#[must_use]
pub fn journey_catalogue_schema() -> Value {
    let duration_pattern = "^PT([0-9]+H)?([0-9]+M)?([0-9]+S)?$";
    let offset = json!({
        "oneOf": [
            { "type": "string", "pattern": duration_pattern },
            { "type": "object", "additionalProperties": false,
              "required": ["uniform"],
              "properties": { "uniform": {
                  "type": "array", "minItems": 2, "maxItems": 2,
                  "items": { "type": "string", "pattern": duration_pattern } } } },
            { "type": "object", "additionalProperties": false,
              "required": ["periodic"],
              "properties": { "periodic": {
                  "type": "object", "additionalProperties": false,
                  "required": ["interval", "count"],
                  "properties": {
                      "interval": { "type": "string", "pattern": duration_pattern },
                      "count": { "type": "integer", "minimum": 1 } } } } }
        ]
    });
    json!({
        "$schema": DRAFT,
        "$id": urn("journey-catalogue"),
        "title": "CNF 2.0 journey catalogue",
        "description": "The hospital-simulation vocabulary: clinical journeys as ordered, time-offset operation sequences over the closed operation vocabulary; commit/update stages name their corpus template; each journey cites its activity-statistics derivation. No openEHR spec governs measured performance (CNF guide master03-overview.adoc §Product Scope) — our own design/extension.",
        "type": "object",
        "minProperties": 1,
        "propertyNames": { "pattern": IDENT_PATTERN },
        "additionalProperties": {
            "type": "object",
            "additionalProperties": false,
            "required": ["description", "derivation", "stages"],
            "properties": {
                "description": { "type": "string", "minLength": 1 },
                "derivation": { "type": "string", "minLength": 1 },
                "stages": {
                    "type": "array",
                    "minItems": 1,
                    "items": {
                        "type": "object",
                        "additionalProperties": false,
                        "required": ["op", "at"],
                        "properties": {
                            "op": { "enum": perf_op_tokens() },
                            "template": { "type": "string", "pattern": CORPUS_KEY_PATTERN },
                            "at": offset
                        }
                    }
                }
            }
        }
    })
}

/// `vocab/wire_surface.yaml` — the wire-surface coverage register (the
/// `surface-coverage` gate's authored, spec-cited exceptions + cross-cutting
/// elements).
///
/// Every `source` is a released-spec / ITS-REST-docs citation, never the
/// vendored OAS (owner ruling 2026-07-24).
#[must_use]
pub fn wire_surface_schema() -> Value {
    let reason = json!({ "enum": tokens(SurfaceReason::ALL) });
    json!({
        "$schema": DRAFT,
        "$id": urn("wire-surface"),
        "title": "CNF 2.0 wire-surface coverage register",
        "description": "The authored, spec-cited record of the wire surface the catalogue is measured against for TOTAL coverage (issue FerroEHR#271): Axis-1 SM operations with no its-rest binding, Axis-2 per-binding outcome/format branches no case exercises, Axis-3 cross-cutting wire behaviours mapped to cases or an adjudicated exception, and Axis-4 the outward declaration of the route families the SUT serves beyond the openEHR resource set (a declaration, never an obligation — no coverage requirement is derived from it). Every source is a released spec component / ITS-REST docs text, never the vendored OAS.",
        "type": "object",
        "additionalProperties": false,
        "properties": {
            "sm_operations": {
                "type": "array",
                "items": {
                    "type": "object",
                    "additionalProperties": false,
                    "required": ["operation", "reason", "source"],
                    "properties": {
                        "operation": { "type": "string", "pattern": SM_OPERATION_PATTERN },
                        "reason": reason.clone(),
                        "source": { "type": "string", "minLength": 1 },
                        "note": { "type": "string", "minLength": 1 }
                    }
                }
            },
            "branches": {
                "type": "array",
                "items": {
                    "type": "object",
                    "additionalProperties": false,
                    "required": ["binding", "reason", "source"],
                    "oneOf": [
                        { "required": ["outcome"], "not": { "required": ["format"] } },
                        { "required": ["format"], "not": { "required": ["outcome"] } }
                    ],
                    "properties": {
                        "binding": { "type": "string", "pattern": SM_OPERATION_PATTERN },
                        "variant": { "type": "string", "minLength": 1 },
                        "outcome": { "enum": tokens(OutcomeKind::ALL) },
                        "format": { "enum": tokens(FormatName::ALL) },
                        "reason": reason.clone(),
                        "source": { "type": "string", "minLength": 1 },
                        "note": { "type": "string", "minLength": 1 }
                    }
                }
            },
            "elements": {
                "type": "array",
                "items": {
                    "type": "object",
                    "additionalProperties": false,
                    "required": ["id", "description", "source"],
                    "oneOf": [
                        { "required": ["covered_by"], "not": { "required": ["exception"] },
                          "properties": { "covered_by": { "minItems": 1 } } },
                        { "required": ["exception"], "not": { "required": ["covered_by"] } }
                    ],
                    "properties": {
                        "id": { "type": "string", "pattern": OPTION_TAG_PATTERN },
                        "description": { "type": "string", "minLength": 1 },
                        "source": { "type": "string", "minLength": 1 },
                        "covered_by": string_array(Some(CASE_ID_PATTERN)),
                        "exception": {
                            "type": "object",
                            "additionalProperties": false,
                            "required": ["reason"],
                            "properties": {
                                "reason": reason.clone(),
                                "register": { "type": "string", "pattern": AMBIGUITY_ID_PATTERN },
                                "note": { "type": "string", "minLength": 1 }
                            }
                        }
                    }
                }
            },
            "served_extensions": {
                "type": "array",
                "items": {
                    "type": "object",
                    "additionalProperties": false,
                    "required": ["family", "routes", "config_gate", "spec_silence", "never_gates"],
                    "properties": {
                        "family": { "type": "string", "minLength": 1 },
                        "routes": {
                            "type": "array",
                            "minItems": 1,
                            "items": { "type": "string", "pattern": route_pattern() }
                        },
                        "config_gate": { "type": "string", "minLength": 1 },
                        "spec_silence": { "type": "string", "minLength": 1 },
                        "never_gates": { "const": true }
                    }
                }
            }
        }
    })
}

/// The Axis-4 route grammar (`"<METHOD> /<path>"`), with the method alternation
/// derived from the closed HTTP-method vocabulary so schema and reference
/// implementation cannot drift.
fn route_pattern() -> String {
    let methods: Vec<String> = HttpMethod::ALL.iter().map(token_str).collect();
    format!("^({}) /\\S*$", methods.join("|"))
}

/// The runner-verification transcript (pack part 1).
#[must_use]
pub fn transcript_schema() -> Value {
    json!({
        "$schema": DRAFT,
        "$id": urn("transcript"),
        "title": "CNF 2.0 runner-verification transcript",
        "description": "An ordered sequence per case × format × row of recorded exchanges with adjudicated verdicts; replayed by sequence so a fixture file fully determines what any conformant runner must conclude.",
        "type": "object",
        "additionalProperties": false,
        "required": ["schedule_release", "entries"],
        "properties": {
            "schedule_release": { "type": "string", "minLength": 1 },
            "entries": {
                "type": "array",
                "minItems": 1,
                "items": {
                    "type": "object",
                    "additionalProperties": false,
                    "required": ["case", "row", "steps", "expected_verdict", "adjudication_ref"],
                    "properties": {
                        "case": { "type": "string", "pattern": CASE_ID_PATTERN },
                        "format": { "enum": tokens(FormatName::ALL) },
                        "row": { "type": "integer", "minimum": 0 },
                        "steps": {
                            "type": "array",
                            "minItems": 1,
                            "items": {
                                "type": "object",
                                "additionalProperties": false,
                                "required": ["step", "request", "response"],
                                "properties": {
                                    "step": { "type": "integer", "minimum": 1 },
                                    "request": {
                                        "type": "object",
                                        "additionalProperties": false,
                                        "required": ["method", "path"],
                                        "properties": {
                                            "method": { "enum": tokens(HttpMethod::ALL) },
                                            "path": { "type": "string", "minLength": 1 },
                                            "body_digest": { "type": "string" }
                                        }
                                    },
                                    "response": {
                                        "type": "object",
                                        "additionalProperties": false,
                                        "required": ["status"],
                                        "properties": {
                                            "status": { "type": "integer", "minimum": 100, "maximum": 599 },
                                            "headers": { "type": "object", "additionalProperties": { "type": "string" } },
                                            "body": {}
                                        }
                                    }
                                }
                            }
                        },
                        "expected_verdict": { "enum": ["passed", "failed", "errored", "not_applicable", "skipped"] },
                        "adjudication_ref": { "type": "string", "minLength": 1 }
                    }
                }
            }
        }
    })
}

/// One recorded wire exchange, as the run transcript and the evidence bundle
/// both carry it.
fn recorded_exchange_def() -> Value {
    json!({
        "type": "object",
        "additionalProperties": false,
        "required": ["seq", "row", "request", "response"],
        "properties": {
            "seq": {
                "type": "integer",
                "minimum": 1,
                "description": "The exchange's ordinal within its case, in send order — provisioning exchanges included."
            },
            "row": { "type": "integer", "minimum": 0 },
            "request": {
                "type": "object",
                "additionalProperties": false,
                "required": ["method", "url", "headers"],
                "properties": {
                    "method": { "enum": tokens(HttpMethod::ALL) },
                    "url": { "type": "string", "minLength": 1 },
                    "headers": { "type": "object", "additionalProperties": { "type": "string" } },
                    "body": {}
                }
            },
            "response": {
                "type": "object",
                "additionalProperties": false,
                "required": ["status", "headers"],
                "properties": {
                    "status": { "type": "integer", "minimum": 100, "maximum": 599 },
                    "headers": { "type": "object", "additionalProperties": { "type": "string" } },
                    "body": {}
                }
            }
        }
    })
}

/// The run wire transcript (`transcript.json` beside a run's `results.json`).
///
/// A different family from [`transcript_schema`], which is the verification
/// pack's REPLAY fixture: that one requires an adjudicated `expected_verdict`
/// and an `adjudication_ref` per entry, and carries no request headers or
/// request body at all. A run transcript adjudicates nothing and records both
/// sides of the wire, so it is its own document.
#[must_use]
pub fn run_transcript_schema() -> Value {
    json!({
        "$schema": DRAFT,
        "$id": urn("run-transcript"),
        "title": "CNF 2.0 run wire transcript",
        "description": "The exchanges one run drove, request and response verbatim, written beside its results.json when `run --record-exchanges` is set. Ordered by case id then send sequence, so a re-run of the same campaign emits the same bytes. The `authorization` request header's value is withheld; response bodies are recorded as they arrived and can carry real patient data.",
        "type": "object",
        "additionalProperties": false,
        "required": ["sut", "schedule_release", "cases"],
        "properties": {
            "sut": {
                "type": "object",
                "additionalProperties": false,
                "required": ["name", "version"],
                "properties": {
                    "name": { "type": "string", "minLength": 1 },
                    "version": { "type": "string", "minLength": 1 }
                }
            },
            "schedule_release": { "type": "string", "minLength": 1 },
            "cases": {
                "type": "array",
                "items": {
                    "type": "object",
                    "additionalProperties": false,
                    "required": ["case", "exchanges"],
                    "properties": {
                        "case": { "type": "string", "pattern": CASE_ID_PATTERN },
                        "format": { "enum": tokens(FormatName::ALL) },
                        "exchanges": {
                            "type": "array",
                            "minItems": 1,
                            "items": recorded_exchange_def()
                        }
                    }
                }
            }
        }
    })
}

/// The evidence bundle (`evidence.json`, written by `veredictum evidence`).
///
/// A SELECTION out of a run transcript, carrying the question it answers and
/// the outcome row each selected case recorded. It is never empty: the export
/// refuses rather than write a document of the right shape with no content in
/// it, so `cases` has a floor of one and every entry has at least one
/// exchange.
#[must_use]
pub fn evidence_bundle_schema() -> Value {
    json!({
        "$schema": DRAFT,
        "$id": urn("evidence-bundle"),
        "title": "CNF 2.0 evidence bundle",
        "description": "The recorded exchanges of a named set of a finished run's cases, carved out of its transcript.json by `veredictum evidence` for a red-run triage to read. Carries the selection it answers, every selected case the recording had nothing for, and the outcome row each exported case recorded. The `authorization` header's value is withheld; response bodies are the wire's own and can carry real patient data.",
        "type": "object",
        "additionalProperties": false,
        "required": ["sut", "schedule_release", "selection", "cases"],
        "properties": {
            "sut": {
                "type": "object",
                "additionalProperties": false,
                "required": ["name", "version"],
                "properties": {
                    "name": { "type": "string", "minLength": 1 },
                    "version": { "type": "string", "minLength": 1 }
                }
            },
            "schedule_release": { "type": "string", "minLength": 1 },
            "selection": {
                "type": "object",
                "additionalProperties": false,
                "required": ["only", "statuses"],
                "description": "What the export was asked for. The three selectors union: a case is exported when any of them names it.",
                "properties": {
                    "only": {
                        "type": "array",
                        "items": { "type": "string", "pattern": CASE_ID_PATTERN }
                    },
                    "filter": { "type": "string", "minLength": 1 },
                    "statuses": {
                        "type": "array",
                        "items": { "enum": tokens(OutcomeStatus::ALL) }
                    }
                }
            },
            "without_exchanges": {
                "type": "array",
                "items": { "type": "string", "pattern": CASE_ID_PATTERN },
                "description": "Selected case ids the transcript carries no exchange for, sorted. A half-matched selection says so here rather than looking complete."
            },
            "cases": {
                "type": "array",
                "minItems": 1,
                "items": {
                    "type": "object",
                    "additionalProperties": false,
                    "required": ["case", "exchanges"],
                    "properties": {
                        "case": { "type": "string", "pattern": CASE_ID_PATTERN },
                        "format": { "enum": tokens(FormatName::ALL) },
                        "outcome": outcome_record_def(),
                        "exchanges": {
                            "type": "array",
                            "minItems": 1,
                            "items": recorded_exchange_def()
                        }
                    }
                }
            }
        }
    })
}

/// The tokens of a closed bench vocabulary, as a JSON array.
fn bench_tokens(tokens: &[&'static str]) -> Value {
    Value::Array(tokens.iter().map(|token| json!(token)).collect())
}

/// One operation's per-repetition statistics inside a bench result.
fn bench_operation_stats_def() -> Value {
    json!({
        "type": "object",
        "additionalProperties": false,
        "required": ["count", "errors", "errors_by_class", "throughput_ops_s",
                      "p50_us", "p75_us", "p90_us", "p99_us", "p999_us", "max_us",
                      "hdr_v2_base64"],
        "properties": {
            "count": { "type": "integer", "minimum": 0 },
            "errors": { "type": "integer", "minimum": 0 },
            "errors_by_class": {
                "type": "object",
                "propertyNames": {
                    "enum": bench_tokens(&crate::bench::result::ErrorClass::ALL
                        .iter().map(|class| class.as_str()).collect::<Vec<_>>())
                },
                "additionalProperties": { "type": "integer", "minimum": 0 }
            },
            "throughput_ops_s": { "type": "number", "minimum": 0.0 },
            "p50_us": { "type": "integer", "minimum": 0 },
            "p75_us": { "type": "integer", "minimum": 0 },
            "p90_us": { "type": "integer", "minimum": 0 },
            "p99_us": { "type": "integer", "minimum": 0 },
            "p999_us": { "type": "integer", "minimum": 0 },
            "max_us": { "type": "integer", "minimum": 0 },
            "hdr_v2_base64": { "type": "string", "minLength": 1 }
        }
    })
}

/// A cross-repetition median plus inter-quartile range.
fn bench_cross_stat_def() -> Value {
    json!({
        "type": "object",
        "additionalProperties": false,
        "required": ["median", "iqr"],
        "properties": {
            "median": { "type": "number" },
            "iqr": { "type": "number", "minimum": 0.0 }
        }
    })
}

/// The bench operation vocabulary, as the property names every per-operation
/// map is keyed by.
fn bench_operation_names() -> Value {
    json!({
        "enum": bench_tokens(&crate::bench::pack::BenchOp::ALL
            .iter().map(|op| op.as_str()).collect::<Vec<_>>())
    })
}

/// One phase's cross-repetition summary, reused by the target's `cross` block
/// and by every baseline's.
fn bench_cross_phase_def(regimes: &Value) -> Value {
    json!({
        "type": "object",
        "additionalProperties": {
            "type": "object",
            "additionalProperties": false,
            "required": ["regime", "operations"],
            "properties": {
                "regime": { "enum": regimes },
                "operations": {
                    "type": "object",
                    "propertyNames": bench_operation_names(),
                    "additionalProperties": {
                        "type": "object",
                        "additionalProperties": false,
                        "required": ["repetitions", "p50_us", "p75_us", "p90_us",
                                      "p99_us", "p999_us", "throughput_ops_s"],
                        "properties": {
                            "repetitions": { "type": "integer", "minimum": 1 },
                            "p50_us": bench_cross_stat_def(),
                            "p75_us": bench_cross_stat_def(),
                            "p90_us": bench_cross_stat_def(),
                            "p99_us": bench_cross_stat_def(),
                            "p999_us": bench_cross_stat_def(),
                            "throughput_ops_s": bench_cross_stat_def()
                        }
                    }
                }
            }
        }
    })
}

/// One same-machine baseline block: a full per-operation summary beside the
/// provenance a submitter must disclose for it to be re-composable.
fn bench_baseline_def(regimes: &Value, repetitions: &Value, seed_phases: &Value) -> Value {
    json!({
        "type": "object",
        "additionalProperties": false,
        "required": ["cdr", "display_name", "images", "recipe", "resources", "base_url",
                      "started_at", "finished_at", "seed_phases", "repetitions", "cross",
                      "posture"],
        "properties": {
            "cdr": {
                "enum": bench_tokens(&crate::bench::baselines::ReferenceCdr::ALL
                    .iter().map(|cdr| cdr.as_str()).collect::<Vec<_>>())
            },
            "display_name": { "type": "string", "minLength": 1 },
            "images": {
                "type": "object",
                "minProperties": 1,
                "additionalProperties": {
                    "type": "string",
                    "pattern": "^[^@]+@sha256:[0-9a-f]{64}$"
                }
            },
            "recipe": {
                "type": "object",
                "additionalProperties": false,
                "required": ["repository", "git_ref", "file"],
                "properties": {
                    "repository": { "type": "string", "minLength": 1 },
                    "git_ref": { "type": "string", "minLength": 1 },
                    "file": { "type": "string", "minLength": 1 }
                }
            },
            "resources": {
                "type": "object",
                "additionalProperties": false,
                "required": ["server_cpus", "server_memory", "database_cpus",
                              "database_memory", "database_shm_size"],
                "properties": {
                    "server_cpus": { "type": "string", "minLength": 1 },
                    "server_memory": { "type": "string", "minLength": 1 },
                    "database_cpus": { "type": "string", "minLength": 1 },
                    "database_memory": { "type": "string", "minLength": 1 },
                    "database_shm_size": { "type": "string", "minLength": 1 }
                }
            },
            "base_url": { "type": "string", "minLength": 1 },
            "sut_version": { "type": "string", "minLength": 1 },
            "started_at": { "type": "string", "minLength": 1 },
            "finished_at": { "type": "string", "minLength": 1 },
            "seed_phases": seed_phases,
            "repetitions": repetitions,
            "cross": bench_cross_phase_def(regimes),
            "posture": bench_posture_def()
        }
    })
}

/// The target measured against one baseline: the dimensionless ratio, its two
/// inputs, and every place no ratio could be formed.
fn bench_relative_def(regimes: &Value) -> Value {
    json!({
        "type": "object",
        "additionalProperties": false,
        "required": ["baseline", "display_name", "derivation", "phases", "gaps"],
        "properties": {
            "baseline": {
                "enum": bench_tokens(&crate::bench::baselines::ReferenceCdr::ALL
                    .iter().map(|cdr| cdr.as_str()).collect::<Vec<_>>())
            },
            "display_name": { "type": "string", "minLength": 1 },
            "derivation": { "const": crate::bench::relative::RELATIVE_DERIVATION },
            "phases": {
                "type": "object",
                "additionalProperties": {
                    "type": "object",
                    "additionalProperties": false,
                    "required": ["regime", "operations"],
                    "properties": {
                        "regime": { "enum": regimes },
                        "operations": {
                            "type": "object",
                            "propertyNames": bench_operation_names(),
                            "additionalProperties": {
                                "type": "object",
                                "additionalProperties": false,
                                "required": ["metrics"],
                                "properties": {
                                    "metrics": {
                                        "type": "object",
                                        "propertyNames": {
                                            "enum": bench_tokens(&crate::bench::compare::Metric::ALL
                                                .iter().map(|metric| metric.as_str())
                                                .collect::<Vec<_>>())
                                        },
                                        "additionalProperties": {
                                            "type": "object",
                                            "additionalProperties": false,
                                            "required": ["target_median", "baseline_median", "index"],
                                            "properties": {
                                                "target_median": { "type": "number" },
                                                "baseline_median": { "type": "number", "exclusiveMinimum": 0.0 },
                                                "index": { "type": "number", "minimum": 0.0 }
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            },
            "gaps": {
                "type": "array",
                "items": {
                    "type": "object",
                    "additionalProperties": false,
                    "required": ["phase", "operation", "reason"],
                    "properties": {
                        "phase": { "type": "string", "minLength": 1 },
                        "operation": { "type": "string", "minLength": 1 },
                        "metric": {
                            "enum": bench_tokens(&crate::bench::compare::Metric::ALL
                                .iter().map(|metric| metric.as_str()).collect::<Vec<_>>())
                        },
                        "reason": {
                            "enum": bench_tokens(&crate::bench::relative::GapReason::ALL
                                .iter().map(|reason| reason.as_str()).collect::<Vec<_>>())
                        }
                    }
                }
            }
        }
    })
}

/// `bench-result.json` — one universal-benchmark run's record.
///
/// A comparative SPEED record, and nothing else: [`crate::bench::BOUNDARY_STATEMENT`]
/// is a schema-required constant, so a document that drops it is invalid.
#[must_use]
#[expect(clippy::too_many_lines, reason = "one literal JSON-Schema document")]
pub fn bench_result_schema() -> Value {
    let regimes = bench_tokens(
        &crate::bench::result::LoopRegime::ALL
            .iter()
            .map(|regime| regime.as_str())
            .collect::<Vec<_>>(),
    );
    let seed_phases = json!({
        "type": "array",
        "items": {
            "type": "object",
            "additionalProperties": false,
            "required": ["name", "regime", "ehrs", "compositions_per_ehr", "workers",
                          "elapsed_s", "bulk_load_writes_per_s",
                          "whole_loop_ms_per_composition"],
            "properties": {
                "name": { "type": "string", "minLength": 1 },
                "regime": { "enum": regimes },
                "ehrs": { "type": "integer", "minimum": 0 },
                "compositions_per_ehr": { "type": "integer", "minimum": 0 },
                "workers": { "type": "integer", "minimum": 1 },
                "elapsed_s": { "type": "number", "minimum": 0.0 },
                "bulk_load_writes_per_s": { "type": "number", "minimum": 0.0 },
                "whole_loop_ms_per_composition": { "type": "number", "minimum": 0.0 }
            }
        }
    });
    let repetitions = json!({
        "type": "array",
        "minItems": 1,
        "items": {
            "type": "object",
            "additionalProperties": false,
            "required": ["repetition", "phases"],
            "properties": {
                "repetition": { "type": "integer", "minimum": 1 },
                "phases": {
                    "type": "object",
                    "additionalProperties": {
                        "type": "object",
                        "additionalProperties": false,
                        "required": ["regime", "rate_per_s", "warmup_s", "duration_s",
                                      "planned_measured_arrivals",
                                      "dispatched_measured_arrivals", "warmup_arrivals",
                                      "offered_load_sustained_per_s", "generator_bound",
                                      "operations"],
                        "properties": {
                            "regime": { "enum": regimes },
                            "rate_per_s": { "type": "number", "minimum": 0.0 },
                            "warmup_s": { "type": "integer", "minimum": 0 },
                            "duration_s": { "type": "integer", "minimum": 0 },
                            "planned_measured_arrivals": { "type": "integer", "minimum": 0 },
                            "dispatched_measured_arrivals": { "type": "integer", "minimum": 0 },
                            "warmup_arrivals": { "type": "integer", "minimum": 0 },
                            "offered_load_sustained_per_s": { "type": "number", "minimum": 0.0 },
                            "generator_bound": { "type": "boolean" },
                            "operations": {
                                "type": "object",
                                "propertyNames": bench_operation_names(),
                                "additionalProperties": bench_operation_stats_def()
                            }
                        }
                    }
                },
                "sweeps": {
                    "type": "object",
                    "additionalProperties": {
                        "type": "object",
                        "additionalProperties": false,
                        "required": ["name", "regime", "workers", "compositions",
                                      "requests_per_composition", "requests", "elapsed_s",
                                      "whole_loop_us_per_request", "operations"],
                        "properties": {
                            "name": { "type": "string", "minLength": 1 },
                            "regime": { "enum": regimes },
                            "workers": { "type": "integer", "minimum": 1 },
                            "compositions": { "type": "integer", "minimum": 0 },
                            "requests_per_composition": { "type": "integer", "minimum": 0 },
                            "requests": { "type": "integer", "minimum": 0 },
                            "elapsed_s": { "type": "number", "minimum": 0.0 },
                            "whole_loop_us_per_request": { "type": "number", "minimum": 0.0 },
                            "operations": {
                                "type": "object",
                                "propertyNames": bench_operation_names(),
                                "additionalProperties": bench_operation_stats_def()
                            }
                        }
                    }
                }
            }
        }
    });
    json!({
        "$schema": DRAFT,
        "$id": urn("bench-result"),
        "title": "Veredictum bench result",
        "description": "One universal-benchmark run: an embedded pack with pinned fixtures, driven against a base URL, seeded once and measured N times open-loop with latency taken from the planned arrival instant. Comparative speed only — never a conformance record, a certificate, or a performance-class rating.",
        "type": "object",
        "additionalProperties": false,
        "required": ["schema_version", "boundary_statement", "pack", "target", "environment",
                      "started_at", "finished_at", "scale", "seed_phases", "repetitions", "cross",
                      "methodology", "submittable", "posture"],
        "properties": {
            "schema_version": { "type": "string", "minLength": 1 },
            "boundary_statement": { "const": crate::bench::BOUNDARY_STATEMENT },
            "label": { "type": "string", "minLength": 1 },
            "pack": {
                "type": "object",
                "additionalProperties": false,
                "required": ["id", "version", "description", "max_failed_share", "seed",
                              "fixtures"],
                "properties": {
                    "id": { "type": "string", "pattern": OPTION_TAG_PATTERN },
                    "version": { "type": "string", "minLength": 1 },
                    "description": { "type": "string", "minLength": 1 },
                    "max_failed_share": { "type": "number", "minimum": 0.0, "maximum": 1.0 },
                    "seed": { "type": "integer", "minimum": 0 },
                    "fixtures": {
                        "type": "object",
                        "additionalProperties": { "type": "string", "pattern": "^[0-9a-f]{64}$" }
                    }
                }
            },
            "target": {
                "type": "object",
                "additionalProperties": false,
                "required": ["base_url"],
                "properties": {
                    "base_url": { "type": "string", "minLength": 1 },
                    "sut_version": { "type": "string", "minLength": 1 }
                }
            },
            "environment": {
                "type": "object",
                "additionalProperties": false,
                "required": ["arch", "os"],
                "properties": {
                    "arch": { "type": "string", "minLength": 1 },
                    "os": { "type": "string", "minLength": 1 },
                    "available_parallelism": { "type": "integer", "minimum": 1 },
                    "cpu_model": { "type": "string", "minLength": 1 },
                    "total_memory_bytes": { "type": "integer", "minimum": 1 }
                }
            },
            "started_at": { "type": "string", "minLength": 1 },
            "finished_at": { "type": "string", "minLength": 1 },
            "scale": {
                "type": "object",
                "additionalProperties": false,
                "required": ["factor", "declared_workers", "reference_configuration"],
                "properties": {
                    "factor": { "type": "number", "exclusiveMinimum": 0.0 },
                    "declared_workers": { "type": "boolean" },
                    "reference_configuration": { "type": "boolean" }
                }
            },
            "version_at_time": { "type": "string", "minLength": 1 },
            "seed_phases": seed_phases,
            "repetitions": repetitions,
            "cross": bench_cross_phase_def(&regimes),
            "baselines": {
                "type": "array",
                "items": bench_baseline_def(&regimes, &repetitions, &seed_phases)
            },
            "relative": {
                "type": "array",
                "items": bench_relative_def(&regimes)
            },
            "methodology": {
                "type": "object",
                "additionalProperties": false,
                "required": ["statement", "open_loop", "coordinated_omission_free",
                              "seed_once_measure_n", "repetitions"],
                "properties": {
                    "statement": { "const": crate::bench::METHODOLOGY },
                    "open_loop": { "const": true },
                    "coordinated_omission_free": { "const": true },
                    "seed_once_measure_n": { "const": true },
                    "repetitions": { "type": "integer", "minimum": 1 }
                }
            },
            "submittable": { "type": "boolean" },
            "submittable_unmet": {
                "type": "array",
                "uniqueItems": true,
                "items": {
                    "enum": bench_tokens(&crate::bench::result::SubmissionRequirement::ALL
                        .iter().map(|requirement| requirement.as_str()).collect::<Vec<_>>())
                }
            },
            "posture": bench_posture_def()
        }
    })
}

/// The posture block: the profile a run declared, every disclosed item's
/// declared value, and the bracketing canary evidence behind its
/// verified-or-declared-only label.
#[must_use]
fn bench_posture_def() -> Value {
    use crate::bench::posture::{
        Assurance, AuditSink, AuthnMode, Bracket, CanaryOutcome, CompressionMode, PostureItem,
        SigningScheme, Tenancy, TlsMode, ValidationDepth,
    };
    let tokens = |values: Vec<&'static str>| bench_tokens(&values);
    // The seven item vocabularies share tokens (`none` and `off` each occur in
    // three), and a JSON Schema `enum` holds a SET, so their union is sorted
    // and deduplicated rather than concatenated.
    let union: std::collections::BTreeSet<&'static str> = [
        AuditSink::ALL
            .iter()
            .map(|value| value.as_str())
            .collect::<Vec<_>>(),
        SigningScheme::ALL
            .iter()
            .map(|value| value.as_str())
            .collect(),
        ValidationDepth::ALL
            .iter()
            .map(|value| value.as_str())
            .collect(),
        AuthnMode::ALL.iter().map(|value| value.as_str()).collect(),
        TlsMode::ALL.iter().map(|value| value.as_str()).collect(),
        CompressionMode::ALL
            .iter()
            .map(|value| value.as_str())
            .collect(),
        Tenancy::ALL.iter().map(|value| value.as_str()).collect(),
    ]
    .concat()
    .into_iter()
    .collect();
    let declared = tokens(union.into_iter().collect());
    json!({
        "type": "object",
        "additionalProperties": false,
        "description": "What was switched on behind these numbers. A run declares exactly one named posture profile out of the set its pack defines, and black-box canaries bracket the measured window: every item is labelled `verified` when both brackets observed the declared value first-hand, and `declared-only` when nothing on the wire discloses it. A canary that contradicted the declaration, or a pair of brackets that disagreed, refused the run, so no such record exists.",
        "required": ["profile", "summary", "items"],
        "properties": {
            "profile": { "type": "string", "minLength": 1 },
            "summary": { "type": "string", "minLength": 1 },
            "comparability": {
                "description": "Every item on which the measured deployment's own configuration departs from the profile named above. A same-machine baseline is composed from somebody else's pinned recipe, so where that recipe configures an item differently the run declares, and the canaries check, what the deployment actually does; this block is how the record says so. Absent for a run that declared its profile as the pack defines it.",
                "type": "array",
                "minItems": 1,
                "items": {
                    "type": "object",
                    "additionalProperties": false,
                    "required": ["item", "profile_declares", "deployment_configures", "source"],
                    "properties": {
                        "item": {
                            "enum": tokens(PostureItem::ALL.iter().map(|item| item.as_str()).collect())
                        },
                        "profile_declares": { "enum": declared.clone() },
                        "deployment_configures": { "enum": declared.clone() },
                        "source": { "type": "string", "minLength": 1 }
                    }
                }
            },
            "items": {
                "type": "array",
                "minItems": 1,
                "items": {
                    "type": "object",
                    "additionalProperties": false,
                    "required": ["item", "declared", "assurance", "readings"],
                    "properties": {
                        "item": {
                            "enum": tokens(PostureItem::ALL.iter().map(|item| item.as_str()).collect())
                        },
                        "declared": { "enum": declared },
                        "assurance": {
                            "enum": tokens(Assurance::ALL.iter().map(|value| value.as_str()).collect())
                        },
                        "readings": {
                            "type": "array",
                            "minItems": 1,
                            "items": {
                                "type": "object",
                                "additionalProperties": false,
                                "required": ["bracket", "outcome", "observed", "evidence"],
                                "properties": {
                                    "bracket": {
                                        "enum": tokens(Bracket::ALL.iter().map(|value| value.as_str()).collect())
                                    },
                                    "outcome": {
                                        "enum": tokens(CanaryOutcome::ALL.iter().map(|value| value.as_str()).collect())
                                    },
                                    "observed": { "type": "string", "minLength": 1 },
                                    "evidence": { "type": "string", "minLength": 1 }
                                }
                            }
                        }
                    }
                }
            }
        }
    })
}

/// `bench-packs.json` — the embedded benchmark packs, as the binary describes
/// them.
///
/// The document a rendered legend is generated from: every phase with its
/// discipline, every mix entry with what it probes, every fixture pin with its
/// provenance, and the requirements a record meets before it may be ranked.
#[must_use]
#[expect(clippy::too_many_lines, reason = "one literal JSON-Schema document")]
pub fn bench_packs_schema() -> Value {
    let fixtures = json!({
        "type": "array",
        "minItems": 1,
        "items": {
            "type": "object",
            "additionalProperties": false,
            "required": ["key", "kind", "media_type", "bytes", "sha256", "provenance"],
            "properties": {
                "key": { "type": "string", "minLength": 1 },
                "kind": {
                    "enum": bench_tokens(&crate::bench::pack::FixtureKind::ALL
                        .iter().map(|kind| kind.as_str()).collect::<Vec<_>>())
                },
                "media_type": { "type": "string", "minLength": 1 },
                "bytes": { "type": "integer", "minimum": 1 },
                "sha256": { "type": "string", "pattern": "^[0-9a-f]{64}$" },
                "provenance": { "type": "string", "minLength": 1 }
            }
        }
    });
    let seed_phase = json!({
        "type": "object",
        "additionalProperties": false,
        "required": ["kind", "name", "discipline", "ehrs", "compositions_per_ehr",
                      "compositions", "workers", "fixtures"],
        "properties": {
            "kind": { "const": "seed" },
            "name": { "type": "string", "minLength": 1 },
            "discipline": { "const": "closed-loop" },
            "ehrs": { "type": "integer", "minimum": 0 },
            "compositions_per_ehr": { "type": "integer", "minimum": 0 },
            "compositions": { "type": "integer", "minimum": 0 },
            "workers": { "type": "integer", "minimum": 1 },
            "fixtures": {
                "type": "array",
                "minItems": 1,
                "items": { "type": "string", "minLength": 1 }
            }
        }
    });
    let sweep_phase = json!({
        "type": "object",
        "additionalProperties": false,
        "required": ["kind", "name", "discipline", "workers", "requests_per_composition",
                      "operations"],
        "properties": {
            "kind": { "const": "sweep" },
            "name": { "type": "string", "minLength": 1 },
            "discipline": { "const": "closed-loop" },
            "workers": { "type": "integer", "minimum": 1 },
            "requests_per_composition": { "type": "integer", "minimum": 1 },
            "operations": {
                "type": "array",
                "minItems": 1,
                "items": bench_operation_names()
            }
        }
    });
    let measure_phase = json!({
        "type": "object",
        "additionalProperties": false,
        "required": ["kind", "name", "discipline", "rate_per_s", "warmup_s", "duration_s",
                      "planned_arrivals", "planned_measured_arrivals", "mix"],
        "properties": {
            "kind": { "const": "measure" },
            "name": { "type": "string", "minLength": 1 },
            "discipline": { "const": "open-loop" },
            "rate_per_s": { "type": "number", "exclusiveMinimum": 0.0 },
            "warmup_s": { "type": "integer", "minimum": 0 },
            "duration_s": { "type": "integer", "minimum": 1 },
            "planned_arrivals": { "type": "integer", "minimum": 1 },
            "planned_measured_arrivals": { "type": "integer", "minimum": 1 },
            "mix": {
                "type": "array",
                "minItems": 1,
                "items": {
                    "type": "object",
                    "additionalProperties": false,
                    "required": ["op", "share", "rate_per_s", "rationale"],
                    "properties": {
                        "op": bench_operation_names(),
                        "share": { "type": "integer", "minimum": 1 },
                        "rate_per_s": { "type": "number", "minimum": 0.0 },
                        "rationale": { "type": "string", "minLength": 1 }
                    }
                }
            }
        }
    });
    json!({
        "$schema": DRAFT,
        "$id": urn("bench-packs"),
        "title": "Veredictum benchmark pack manifest",
        "description": "Every benchmark pack this binary embeds: the phases with their load discipline, the operation mix with what each entry probes, the pinned fixtures with their provenance, the seed the arrival streams draw from, and the requirements a record meets before it may be ranked. Emitted by `veredictum bench-packs`, and the only source a published description of a pack may be generated from.",
        "type": "object",
        "additionalProperties": false,
        "required": ["schema_version", "boundary_statement", "methodology", "relative_index",
                      "seed_disclosure", "posture_disclosure", "submission_requirements",
                      "operations", "packs"],
        "properties": {
            "schema_version": { "type": "string", "minLength": 1 },
            "boundary_statement": { "const": crate::bench::BOUNDARY_STATEMENT },
            "methodology": { "const": crate::bench::METHODOLOGY },
            "relative_index": { "const": crate::bench::relative::RELATIVE_DERIVATION },
            "seed_disclosure": { "const": crate::bench::manifest::SEED_DISCLOSURE },
            "posture_disclosure": { "const": crate::bench::manifest::POSTURE_DISCLOSURE },
            "submission_requirements": {
                "type": "array",
                "minItems": 1,
                "items": {
                    "type": "object",
                    "additionalProperties": false,
                    "required": ["token", "statement"],
                    "properties": {
                        "token": {
                            "enum": bench_tokens(&crate::bench::result::SubmissionRequirement::ALL
                                .iter().map(|requirement| requirement.as_str()).collect::<Vec<_>>())
                        },
                        "statement": { "type": "string", "minLength": 1 }
                    }
                }
            },
            "operations": {
                "type": "array",
                "minItems": 1,
                "items": {
                    "type": "object",
                    "additionalProperties": false,
                    "required": ["token", "wire"],
                    "properties": {
                        "token": bench_operation_names(),
                        "wire": { "type": "string", "pattern": "^(GET|POST) /" }
                    }
                }
            },
            "packs": {
                "type": "array",
                "minItems": 1,
                "items": {
                    "type": "object",
                    "additionalProperties": false,
                    "required": ["id", "version", "description", "max_failed_share", "seed",
                                  "fixtures", "phases", "profiles", "probe_rationales"],
                    "properties": {
                        "id": { "type": "string", "pattern": OPTION_TAG_PATTERN },
                        "version": { "type": "string", "minLength": 1 },
                        "description": { "type": "string", "minLength": 1 },
                        "max_failed_share": { "type": "number", "minimum": 0.0, "maximum": 1.0 },
                        "seed": { "type": "integer", "minimum": 0 },
                        "fixtures": fixtures,
                        "phases": {
                            "type": "array",
                            "minItems": 1,
                            "items": { "oneOf": [seed_phase, sweep_phase, measure_phase] }
                        },
                        "profiles": {
                            "type": "array",
                            "minItems": 1,
                            "items": {
                                "type": "object",
                                "additionalProperties": false,
                                "required": ["name", "summary", "default", "declares"],
                                "properties": {
                                    "name": { "type": "string", "pattern": OPTION_TAG_PATTERN },
                                    "summary": { "type": "string", "minLength": 1 },
                                    "default": { "type": "boolean" },
                                    "declares": {
                                        "type": "object",
                                        "minProperties": 1,
                                        "propertyNames": {
                                            "enum": bench_tokens(&crate::bench::posture::PostureItem::ALL
                                                .iter().map(|item| item.as_str()).collect::<Vec<_>>())
                                        },
                                        "additionalProperties": { "type": "string", "minLength": 1 }
                                    }
                                }
                            }
                        },
                        "probe_rationales": {
                            "type": "object",
                            "propertyNames": bench_operation_names(),
                            "additionalProperties": { "type": "string", "minLength": 1 }
                        }
                    }
                }
            }
        }
    })
}

/// Every token of one registry vocabulary, in its declared order.
fn registry_tokens<T: Copy>(all: &'static [T], render: fn(T) -> &'static str) -> Value {
    Value::Array(
        all.iter()
            .copied()
            .map(|item| Value::String(render(item).to_owned()))
            .collect(),
    )
}

/// The disclosure block every registry entry carries, whatever it measured.
fn registry_disclosure_schema() -> Value {
    json!({
        "type": "object",
        "additionalProperties": false,
        "required": ["instrument_version", "run_started_at", "environment", "sut_configuration", "conflict_of_interest"],
        "properties": {
            "instrument_version": { "type": "string", "minLength": 1 },
            "run_started_at": { "type": "string", "pattern": REGISTRY_UTC_TIMESTAMP_PATTERN },
            "environment": {
                "type": "object",
                "additionalProperties": false,
                "required": ["os", "arch", "host_class"],
                "properties": {
                    "os": { "type": "string", "minLength": 1 },
                    "arch": { "type": "string", "minLength": 1 },
                    "host_class": { "type": "string", "minLength": 1 },
                    "cpu_model": { "type": "string", "minLength": 1 },
                    "cores": { "type": "integer", "minimum": 1 },
                    "memory_bytes": { "type": "integer", "minimum": 1 }
                }
            },
            "sut_configuration": { "type": "string", "minLength": 1 },
            "conflict_of_interest": { "type": "string", "minLength": 1 }
        }
    })
}

/// The two result blocks, each internally tagged by the board it belongs on.
fn registry_result_schema() -> Value {
    json!({
        "oneOf": [
            {
                "type": "object",
                "additionalProperties": false,
                "required": ["kind", "catalogue_revision", "statement"],
                "properties": {
                    "kind": { "const": EntryKind::Conformance.as_str() },
                    "catalogue_revision": { "type": "string", "minLength": 1 },
                    "statement": { "type": "string", "minLength": 1 }
                }
            },
            {
                "type": "object",
                "additionalProperties": false,
                "required": ["kind", "pack_id", "pack_version", "repetitions", "posture_profile"],
                "properties": {
                    "kind": { "const": EntryKind::Bench.as_str() },
                    "pack_id": { "type": "string", "minLength": 1 },
                    "pack_version": { "type": "string", "minLength": 1 },
                    "repetitions": { "type": "integer", "minimum": 1 },
                    "posture_profile": { "type": "string", "minLength": 1 }
                }
            }
        ]
    })
}

/// The three provenance blocks, each internally tagged by the tier it
/// establishes.
fn registry_provenance_schema() -> Value {
    json!({
        "oneOf": [
            {
                "type": "object",
                "additionalProperties": false,
                "required": ["tier", "workflow_ref", "run_id", "run_attempt", "predicate_type", "verify_command"],
                "properties": {
                    "tier": { "const": RegistryTier::Reproduced.as_str() },
                    "workflow_ref": { "type": "string", "minLength": 1 },
                    "run_id": { "type": "string", "pattern": "^[0-9]+$" },
                    "run_attempt": { "type": "integer", "minimum": 1 },
                    "predicate_type": { "type": "string", "minLength": 1 },
                    "verify_command": { "type": "string", "minLength": 1 }
                }
            },
            {
                "type": "object",
                "additionalProperties": false,
                "required": ["tier", "instrument_origin", "console_run_id", "workflow_ref", "run_id", "run_attempt", "scheme", "signature", "signs", "identity", "verify_command"],
                "properties": {
                    "tier": { "const": RegistryTier::Console.as_str() },
                    "instrument_origin": { "type": "string", "minLength": 1 },
                    "console_run_id": { "type": "string", "minLength": 1 },
                    "workflow_ref": { "type": "string", "minLength": 1 },
                    "run_id": { "type": "string", "pattern": "^[0-9]+$" },
                    "run_attempt": { "type": "integer", "minimum": 1 },
                    "scheme": { "enum": registry_tokens(SignatureScheme::ALL, SignatureScheme::as_str) },
                    "signature": { "type": "string", "minLength": 1 },
                    "signs": { "type": "string", "minLength": 1 },
                    "identity": { "type": "string", "minLength": 1 },
                    "verify_command": { "type": "string", "minLength": 1 }
                }
            },
            {
                "type": "object",
                "additionalProperties": false,
                "required": ["tier", "scheme", "signature", "signs", "identity", "verify_command"],
                "properties": {
                    "tier": { "const": RegistryTier::SelfReported.as_str() },
                    "scheme": { "enum": registry_tokens(SignatureScheme::ALL, SignatureScheme::as_str) },
                    "signature": { "type": "string", "minLength": 1 },
                    "signs": { "type": "string", "minLength": 1 },
                    "identity": { "type": "string", "minLength": 1 },
                    "verify_command": { "type": "string", "minLength": 1 }
                }
            }
        ]
    })
}

/// `registry/entries/<kind>/<system>/<entry-id>.json` — one published result,
/// its mandatory disclosure, the artifacts it stands on, and the tier anybody
/// here can honestly claim for it.
#[must_use]
pub fn registry_entry_schema() -> Value {
    json!({
        "$schema": DRAFT,
        "$id": urn("registry-entry"),
        "title": "Veredictum public results registry entry",
        "description": "One append-only registry entry. Every figure a board prints comes out of the artifacts this entry pins by digest, never out of a number restated here. The tier is the discriminant of `provenance`: `reproduced` carries the identity of the workflow that performed the run, `console` carries the workflow that re-derived the verdicts from the submitted transcript plus the signature CI then made, and `self-reported` carries the submitter's own signature. No signing key exists on the hosted instrument or in this repository. An entry is a REPORT, never a certificate.",
        "type": "object",
        "additionalProperties": false,
        "required": ["registry_schema_version", "entry_id", "rules_version", "submitter", "subject", "disclosure", "result", "artifacts", "provenance"],
        "properties": {
            "registry_schema_version": { "enum": READABLE_REGISTRY_SCHEMA_VERSIONS },
            "entry_id": { "type": "string", "pattern": REGISTRY_ENTRY_ID_PATTERN },
            "rules_version": { "enum": READABLE_RULES_VERSIONS },
            "submitter": {
                "type": "object",
                "additionalProperties": false,
                "required": ["name", "contact", "relationship"],
                "properties": {
                    "name": { "type": "string", "minLength": 1 },
                    "contact": { "type": "string", "minLength": 1 },
                    "relationship": { "enum": registry_tokens(Relationship::ALL, Relationship::as_str) }
                }
            },
            "subject": {
                "type": "object",
                "additionalProperties": false,
                "required": ["system", "display_name", "version", "deployment"],
                "properties": {
                    "system": { "type": "string", "pattern": REGISTRY_SYSTEM_PATTERN },
                    "display_name": { "type": "string", "minLength": 1 },
                    "version": { "type": "string", "minLength": 1 },
                    "deployment": {
                        "type": "object",
                        "additionalProperties": false,
                        "required": ["kind", "reproduction_authorized"],
                        "properties": {
                            "kind": { "enum": registry_tokens(DeploymentKind::ALL, DeploymentKind::as_str) },
                            "topology": { "type": "string", "pattern": REGISTRY_SYSTEM_PATTERN },
                            "images": {
                                "type": "object",
                                "additionalProperties": { "type": "string", "minLength": 1 }
                            },
                            "endpoint": { "type": "string", "minLength": 1 },
                            "reproduction_authorized": { "type": "boolean" }
                        }
                    }
                }
            },
            "disclosure": registry_disclosure_schema(),
            "result": registry_result_schema(),
            "artifacts": {
                "type": "array",
                "minItems": 1,
                "items": {
                    "type": "object",
                    "additionalProperties": false,
                    "required": ["role", "path", "sha256"],
                    "properties": {
                        "role": { "enum": registry_tokens(ArtifactRole::ALL, ArtifactRole::as_str) },
                        "path": { "type": "string", "minLength": 1 },
                        "sha256": { "type": "string", "pattern": REGISTRY_DIGEST_PATTERN }
                    }
                }
            },
            "provenance": registry_provenance_schema(),
            "supersedes": {
                "type": "array",
                "items": { "type": "string", "pattern": REGISTRY_ENTRY_ID_PATTERN }
            },
            "supersede_reason": { "type": "string", "minLength": 1 },
            "notes": { "type": "string", "minLength": 1 }
        }
    })
}

/// `registry/topologies/<id>.json` — a deployment this repository composes
/// itself, which is the only thing the reproduction lane will drive.
#[must_use]
pub fn registry_topology_schema() -> Value {
    json!({
        "$schema": DRAFT,
        "$id": urn("registry-topology"),
        "title": "Veredictum reproducible topology",
        "description": "A deployment recipe this repository controls end to end, so a reproduction run executes nothing a submitter wrote. The reproduction lane composes `compose_file` (or the upstream document `compose_from` names), waits for `ready_url`, exports `credentials` into the environment the ixit's auth modes reference, and drives the catalogue over `ixit`. The ixit is this repository's to write, because it composes the deployment and therefore knows its addresses; the claim is not, so `statement` may only cite a declaration the supplier published.",
        "type": "object",
        "additionalProperties": false,
        "required": ["id", "display_name", "ixit", "base_url", "ready_url", "credentials"],
        "properties": {
            "id": { "type": "string", "pattern": REGISTRY_SYSTEM_PATTERN },
            "display_name": { "type": "string", "minLength": 1 },
            "compose_file": { "type": "string", "minLength": 1 },
            "compose_from": {
                "type": "object",
                "additionalProperties": false,
                "required": ["repository", "path"],
                "properties": {
                    "repository": { "type": "string", "minLength": 1 },
                    "path": { "type": "string", "minLength": 1 }
                }
            },
            "compose_env": {
                "type": "object",
                "additionalProperties": { "type": "string" }
            },
            "ixit": { "type": "string", "minLength": 1 },
            "statement": {
                "type": "string",
                "minLength": 1,
                "description": "A declaration the SUPPLIER of this deployment published, committed here as a citation with its source recorded in `provenance`. ISO/IEC 9646-7 assigns an ICS proforma's support and supported-values columns to the supplier of the implementation, so this repository never authors one on a vendor's behalf. Absent, the reproduction asserts no claim and produces no conformance verdict."
            },
            "base_url": { "type": "string", "minLength": 1 },
            "ready_url": { "type": "string", "minLength": 1 },
            "credentials": {
                "type": "object",
                "additionalProperties": { "type": "string" }
            },
            "provenance": { "type": "string", "minLength": 1 }
        },
        "oneOf": [
            { "required": ["compose_file"] },
            { "required": ["compose_from"] }
        ]
    })
}

/// The full published set: (file name, schema document).
#[must_use]
pub fn emit_all() -> Vec<(&'static str, Value)> {
    vec![
        ("case-core.schema.json", case_core_schema()),
        ("operation-binding.schema.json", operation_binding_schema()),
        ("outcomes.schema.json", outcomes_schema()),
        ("selectors.schema.json", selectors_schema()),
        ("capability-matrix.schema.json", capability_matrix_schema()),
        ("corpus-manifest.schema.json", corpus_manifest_schema()),
        (
            "ambiguity-register.schema.json",
            ambiguity_register_schema(),
        ),
        ("statement.schema.json", statement_schema()),
        ("results.schema.json", results_schema()),
        ("ixit.schema.json", ixit_schema()),
        ("performance-case.schema.json", performance_case_schema()),
        ("journey-catalogue.schema.json", journey_catalogue_schema()),
        ("wire-surface.schema.json", wire_surface_schema()),
        ("stress.schema.json", stress_schema()),
        ("aql-probe.schema.json", aql_probe_schema()),
        ("transcript.schema.json", transcript_schema()),
        ("run-transcript.schema.json", run_transcript_schema()),
        ("evidence-bundle.schema.json", evidence_bundle_schema()),
        ("bench-result.schema.json", bench_result_schema()),
        ("bench-packs.schema.json", bench_packs_schema()),
        ("registry-entry.schema.json", registry_entry_schema()),
        ("registry-topology.schema.json", registry_topology_schema()),
    ]
}

/// Render a schema document to its canonical published text
/// (two-space pretty print + trailing newline).
#[must_use]
pub fn render(schema: &Value) -> String {
    let mut text = serde_json::to_string_pretty(schema).unwrap_or_default();
    text.push('\n');
    text
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_schema_compiles_under_2020_12() {
        for (name, schema) in emit_all() {
            jsonschema::validator_for(&schema)
                .unwrap_or_else(|e| panic!("{name} does not compile: {e}"));
        }
    }

    #[test]
    fn rendering_is_deterministic() {
        let a = render(&case_core_schema());
        let b = render(&case_core_schema());
        assert_eq!(a, b);
        assert!(a.ends_with('\n'));
    }

    #[test]
    fn no_null_tokens_leak() {
        for (name, schema) in emit_all() {
            let text = render(&schema);
            assert!(
                !text.contains(": null,"),
                "{name} leaked a null vocabulary token"
            );
        }
    }
}
