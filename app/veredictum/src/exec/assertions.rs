// SPDX-FileCopyrightText: Veredictum contributors
// SPDX-License-Identifier: Apache-2.0

//! The pure assertion evaluators — verdict logic over canonical-JSON
//! values, shared by the live driver and the transcript player so any two
//! conformant runners compute identical verdicts.
//!
//! Wire-dependent assertion families (`version`, `signature`,
//! `instance_of`) need reads the driver performs; their FACT comparison
//! still happens here so the judgement stays pure.

#![expect(
    clippy::disallowed_types,
    reason = "dev/verification tooling over JSON artifacts (the catalogue, results, wire \
              exchanges), whose shapes belong to the artifacts and the SUT"
)]

use serde_json::Value;
use std::collections::BTreeMap;

use crate::exec::resultset;
use crate::exec::state::{Captured, VarStore};
use crate::model::assertion::{Assertion, IgnoreSpec};
use crate::refgrammar::{Segment, Template, ValueRef};
use crate::vocab::IgnoreSetName;

/// One assertion failure (stable, human-readable — lands in the outcome
/// record and the report).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AssertionFailure(pub String);

/// Which channel an assertion failure belongs to.
///
/// A finding against the server needs a value the server actually served. An
/// assertion the run could not judge at all proves nothing about the SUT, so
/// it takes the inconclusive channel beside a transport fault (ISO/IEC 9646
/// *inconclusive*; interpreter law (c), [`crate::exec`]).
///
/// The two are distinguished as TYPES rather than by reading the message: a
/// classification that branches on a substring changes the moment a message
/// is reworded.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AssertionOutcome {
    /// The SUT served a value, and it differs from the asserted one — a
    /// conformance finding, so the row FAILS (law b).
    Mismatch(String),
    /// The assertion cannot be judged on this ITS or on this run: the fact
    /// has no released read, the container the assertion names resolves to
    /// no single family, the authored pattern carries a token outside its
    /// closed vocabulary, or a prerequisite the assertion reads was never
    /// bound. The row is INCONCLUSIVE (law c), attributed to the runner or
    /// the catalogue, never to the server.
    Unjudgeable(String),
}

impl AssertionOutcome {
    /// The one-line reason, whichever channel this outcome carries.
    #[must_use]
    pub fn reason(&self) -> &str {
        match self {
            Self::Mismatch(reason) | Self::Unjudgeable(reason) => reason,
        }
    }
}

impl From<AssertionFailure> for AssertionOutcome {
    /// A pure judge compares a value the SUT SERVED against the authored one,
    /// so its failure is a conformance mismatch by construction. Only the
    /// wire-side resolution that precedes a judge can be unjudgeable, and
    /// those sites name the variant themselves.
    fn from(failure: AssertionFailure) -> Self {
        Self::Mismatch(failure.0)
    }
}

/// Resolve an RM path segment sequence (`context/setting`,
/// `content[0]/data/events[0]/...`) over a canonical-JSON value.
///
/// Supported addressing: object attributes and `[<index>]` list positions —
/// the subset the catalogue's field assertions use.
#[must_use]
pub fn resolve_path<'a>(root: &'a Value, path: &str) -> Option<&'a Value> {
    let mut current = root;
    for raw in path.split('/').filter(|s| !s.is_empty()) {
        let (attr, index) = match raw.split_once('[') {
            Some((attr, rest)) => {
                let index: usize = rest.strip_suffix(']')?.parse().ok()?;
                (attr, Some(index))
            }
            None => (raw, None),
        };
        if !attr.is_empty() {
            current = current.get(attr)?;
        }
        if let Some(i) = index {
            current = current.get(i)?;
        }
    }
    Some(current)
}

/// Render a template against the store (captures become their scalar
/// values; other reference kinds must have been pre-resolved by the driver).
///
/// # Errors
/// A message when a capture is unbound or non-scalar.
pub fn render_template(template: &Template, vars: &VarStore) -> Result<String, String> {
    let mut out = String::new();
    for segment in template.segments() {
        match segment {
            Segment::Lit(s) => out.push_str(s),
            Segment::Ref(ValueRef::Capture { name, .. }) => match vars.get(name) {
                Some(Captured::Scalar(s)) => out.push_str(s),
                Some(_) => return Err(format!("capture {name} is not scalar")),
                None => return Err(format!("capture {name} is not bound")),
            },
            Segment::Ref(other) => {
                return Err(format!("reference {other} must be resolved by the driver"));
            }
        }
    }
    Ok(out)
}

/// Strip the named ignore-sets and explicit paths from a value (top-level
/// path removal; nested server-assigned paths use `/`-separated forms).
///
/// A `**` segment matches zero or more intervening attribute steps, so
/// `**/uid` names one attribute wherever it occurs in a recursive RM
/// structure. Recursive containment is an RM shape, not a fixture depth:
/// `FOLDER.folders` is `List<FOLDER>` (RM common `folder.adoc`), so an
/// ignore-set that could only be written per depth would silently
/// under-cover a deeper tree.
#[must_use]
pub fn strip_ignored(value: &Value, ignored_paths: &[String]) -> Value {
    fn remove(value: &mut Value, segments: &[&str]) {
        let Some((head, rest)) = segments.split_first() else {
            return;
        };
        if *head == "**" {
            // Zero intervening steps: apply the remainder here …
            remove(value, rest);
            // … or one-or-more: descend and retry the same pattern.
            match value {
                Value::Object(map) => {
                    for child in map.values_mut() {
                        remove(child, segments);
                    }
                }
                Value::Array(items) => {
                    for item in items {
                        remove(item, segments);
                    }
                }
                _ => {}
            }
            return;
        }
        match value {
            Value::Object(map) => {
                if rest.is_empty() {
                    map.remove(*head);
                } else if let Some(next) = map.get_mut(*head) {
                    remove(next, rest);
                }
            }
            Value::Array(items) => {
                for item in items {
                    remove(item, segments);
                }
            }
            _ => {}
        }
    }
    let mut out = value.clone();
    for path in ignored_paths {
        let segments: Vec<&str> = path.split('/').filter(|s| !s.is_empty()).collect();
        remove(&mut out, &segments);
    }
    out
}

/// A FLAT body: a one-level object whose keys are path-formed strings.
fn is_flat_map(value: &Value) -> bool {
    match value {
        Value::Object(map) => {
            !map.is_empty()
                && map.keys().any(|k| k.contains('/'))
                && map.values().all(|v| !matches!(v, Value::Object(_)))
        }
        _ => false,
    }
}

/// Fold a committed FLAT document's `ctx/*` input-convenience keys onto the
/// RM-tree flat paths a read-back expresses them at, per ITS-REST
/// `simplified_formats` master06 §Context Information: `ctx/participation_*`
/// → `{root}/context/_participation:{i}|*`, `ctx/health_care_facility|*` →
/// `{root}/context/_health_care_facility|*`, and the declared
/// `ctx/id_namespace`/`ctx/id_scheme` defaults expand onto every folded
/// party that carries an `|id` ("default namespace/scheme for external
/// references" — master06 §ID Namespace and Scheme). Pure default-setters
/// whose targets the ignore-set covers (`ctx/time`, `ctx/setting`,
/// composer) pass through untouched for the ignore pass.
fn fold_flat_ctx(committed: &BTreeMap<String, Value>, root: &str) -> BTreeMap<String, Value> {
    let id_namespace = committed.get("ctx/id_namespace").cloned();
    let id_scheme = committed.get("ctx/id_scheme").cloned();
    let mut out: BTreeMap<String, Value> = BTreeMap::new();
    let mut folded_id_carriers: Vec<String> = Vec::new();
    for (key, value) in committed {
        if let Some(rest) = key.strip_prefix("ctx/participation_") {
            // participation_<field>[:i] — default index 0.
            let (field, index) = match rest.split_once(':') {
                Some((f, i)) => (f, i),
                None => (rest, "0"),
            };
            let target = format!("{root}/context/_participation:{index}|{field}");
            if field == "id" {
                folded_id_carriers.push(format!("{root}/context/_participation:{index}"));
            }
            out.insert(target, value.clone());
        } else if let Some(rest) = key.strip_prefix("ctx/health_care_facility|") {
            if rest == "id" {
                folded_id_carriers.push(format!("{root}/context/_health_care_facility"));
            }
            out.insert(
                format!("{root}/context/_health_care_facility|{rest}"),
                value.clone(),
            );
        } else if key == "ctx/id_namespace" || key == "ctx/id_scheme" {
            // consumed as qualifiers below
        } else {
            out.insert(key.clone(), value.clone());
        }
    }
    for carrier in folded_id_carriers {
        if let Some(ns) = &id_namespace {
            out.insert(format!("{carrier}|id_namespace"), ns.clone());
        }
        if let Some(scheme) = &id_scheme {
            out.insert(format!("{carrier}|id_scheme"), scheme.clone());
        }
    }
    out
}

/// Whether a FLAT key falls under an ignore path: the `ctx/*` default-setter
/// spellings map to their master06 targets (`ctx/time` →
/// `context/start_time`, `ctx/setting` → `context/setting`, `ctx/composer_*`
/// → `composer`); any other key matches when its post-root path starts with
/// the ignore path (`uid` also matches the flat `_uid` spelling).
fn flat_key_ignored(key: &str, ignored_paths: &[String]) -> bool {
    let effective: &str = match key {
        "ctx/time" => "context/start_time",
        "ctx/end_time" => "context/end_time",
        "ctx/setting" => "context/setting",
        k if k.starts_with("ctx/composer") => "composer",
        k => k.split_once('/').map_or(k, |(_, rest)| rest),
    };
    let effective = effective.replace("/_uid", "/uid");
    let effective = effective.strip_prefix('_').unwrap_or(&effective);
    ignored_paths.iter().any(|p| {
        effective == p.as_str()
            || effective.starts_with(&format!("{p}/"))
            || effective.starts_with(&format!("{p}|"))
    })
}

/// Canonicalize a FLAT key by eliding every `:0` first-element index
/// (`a:0/b`, `a:0|x`, trailing `a:0`) — the index is optional on the wire
/// (`simplified_formats` master04 §Field Identifiers), so both spellings
/// name the same datum.
fn dezero(key: &str) -> String {
    let inner = key.replace(":0/", "/").replace(":0|", "|");
    inner
        .strip_suffix(":0")
        .map_or(inner.clone(), ToOwned::to_owned)
}

/// Flatten a STRUCTURED body into its FLAT key form per the
/// `simplified_formats` master04 STRUCTURED->FLAT algorithm: object keys
/// join with `/`, array elements append `:{i}` to their segment,
/// `|`-prefixed attribute keys append without a separator, and the
/// empty-string key is the element's main value.
fn flatten_structured(value: &Value, prefix: &str, out: &mut BTreeMap<String, Value>) {
    match value {
        Value::Object(map) => {
            for (key, child) in map {
                let next = if key.is_empty() {
                    prefix.to_owned()
                } else if let Some(attr) = key.strip_prefix('|') {
                    format!("{prefix}|{attr}")
                } else if prefix.is_empty() {
                    key.clone()
                } else {
                    format!("{prefix}/{key}")
                };
                flatten_structured(child, &next, out);
            }
        }
        Value::Array(items) => {
            for (i, item) in items.iter().enumerate() {
                flatten_structured(item, &format!("{prefix}:{i}"), out);
            }
        }
        leaf => {
            out.insert(prefix.to_owned(), leaf.clone());
        }
    }
}

/// STRUCTURED is recognized by its attribute-key shape: `|`-prefixed or
/// empty-string keys somewhere in the tree (master04 §Structured format) —
/// canonical JSON never carries either, so a canonical body is never
/// misread as simplified.
fn has_simplified_leaf_keys(value: &Value) -> bool {
    match value {
        Value::Object(map) => map
            .iter()
            .any(|(k, v)| k.is_empty() || k.starts_with('|') || has_simplified_leaf_keys(v)),
        Value::Array(items) => items.iter().any(has_simplified_leaf_keys),
        _ => false,
    }
}

/// A simplified body in either wire form, as a FLAT key map: a FLAT body
/// verbatim, a STRUCTURED body via the master04 flattening. `None` for
/// canonical/other bodies (a canonical COMPOSITION carries `_type`).
fn simplified_as_flat(value: &Value) -> Option<BTreeMap<String, Value>> {
    let Value::Object(map) = value else {
        return None;
    };
    if is_flat_map(value) {
        return Some(map.iter().map(|(k, v)| (k.clone(), v.clone())).collect());
    }
    if !map.is_empty()
        && map.keys().all(|k| !k.contains('/'))
        && !map.contains_key("_type")
        && (map.contains_key("ctx") || has_simplified_leaf_keys(value))
    {
        let mut out = BTreeMap::new();
        flatten_structured(value, "", &mut out);
        return Some(out);
    }
    None
}

/// FLAT round-trip equivalence: fold the committed side's ctx keys onto the
/// read-back's RM-path forms (master06), drop ignore-set keys from both
/// sides, then require every committed entry to appear in the read-back
/// with an equal value (keys compared in their `:0`-elided canonical form,
/// master04 §Field Identifiers). Read-back surplus (template-derived
/// defaults such as `category|*`, terminology qualifiers, server-assigned
/// ids) is tolerated: the export is the full RM projection of the committed
/// data (`simplified_formats` master04), so the round-trip guarantee is
/// that no committed datum is lost or altered. (No openEHR spec defines a
/// round-trip comparator — our own design over the master04/master06
/// semantics.)
fn flat_equivalent(
    actual: &BTreeMap<String, Value>,
    committed: &BTreeMap<String, Value>,
    ignored_paths: &[String],
) -> bool {
    let root = actual
        .keys()
        .find(|k| !k.starts_with("ctx/"))
        .and_then(|k| k.split(['/', ':']).next())
        .unwrap_or_default()
        .to_owned();
    let normalized: BTreeMap<String, &Value> = actual.iter().map(|(k, v)| (dezero(k), v)).collect();
    let folded = fold_flat_ctx(committed, &root);
    folded
        .iter()
        .filter(|(k, _)| !flat_key_ignored(k, ignored_paths))
        .all(|(k, want)| {
            normalized
                .get(&dezero(k))
                .is_some_and(|got| resultset::cells_equal(got, want))
        })
}

/// The `equivalent` comparison.
///
/// Structural equality after stripping the
/// resolved ignore paths from BOTH sides (numeric leaves by value, via the
/// result-set cell rule — canonical JSON carries RM numbers), with canonical
/// `_type` self-tag PRESENCE normalized (see this module's `rm_cells_equal`). FLAT bodies
/// take the master06-aware round-trip rule (`flat_equivalent`).
#[must_use]
pub fn equivalent(actual: &Value, expected: &Value, ignored_paths: &[String]) -> bool {
    if let (Some(a), Some(e)) = (simplified_as_flat(actual), simplified_as_flat(expected)) {
        return flat_equivalent(&a, &e, ignored_paths);
    }
    let a = strip_ignored(actual, ignored_paths);
    let b = strip_ignored(expected, ignored_paths);
    rm_cells_equal(&a, &b)
}

/// Canonical-RM structural equality for `equivalent`: the result-set cell
/// rule everywhere, EXCEPT that a `_type` self-tag present on only one side
/// of an object is not a content difference. ITS-REST overview Resources.md
/// §JSON Format makes `_type` presence conditional ("should be used to
/// specify the RM type whenever polymorphism is involved, or when the
/// underlying definition in RM type is abstract") while the MUST governs its
/// VALUE — so a decode→re-encode path that self-tags every object (the
/// canonical codec) and a sparsely-tagged committed twin describe the same
/// RM content. Present on BOTH sides, the tags must be equal (a genuine
/// polymorphic-type substitution stays detectable).
fn rm_cells_equal(a: &Value, b: &Value) -> bool {
    match (a, b) {
        (Value::Object(x), Value::Object(y)) => {
            let keys: std::collections::BTreeSet<&str> = x
                .keys()
                .chain(y.keys())
                .map(String::as_str)
                .filter(|k| *k != "_type")
                .collect();
            let type_tags_agree = match (x.get("_type"), y.get("_type")) {
                (Some(ta), Some(tb)) => ta == tb,
                _ => true,
            };
            type_tags_agree
                && keys.iter().all(|k| match (x.get(*k), y.get(*k)) {
                    (Some(va), Some(vb)) => rm_cells_equal(va, vb),
                    _ => false,
                })
        }
        (Value::Array(x), Value::Array(y)) => {
            x.len() == y.len() && x.iter().zip(y).all(|(va, vb)| rm_cells_equal(va, vb))
        }
        _ => resultset::cells_equal(a, b),
    }
}

/// Resolve the `ignoring:` list into concrete paths: named sets come from
/// the binding (`server_assigned`) and the selectors vocabulary
/// (`ctx_defaults`); explicit paths pass through.
#[must_use]
pub fn resolve_ignore_sets(
    specs: &[IgnoreSpec],
    server_assigned: &[String],
    ctx_defaults: &[String],
) -> Vec<String> {
    let mut paths = Vec::new();
    for spec in specs {
        match spec {
            IgnoreSpec::Named(IgnoreSetName::ServerAssigned) => {
                paths.extend(server_assigned.iter().cloned());
            }
            IgnoreSpec::Named(IgnoreSetName::CtxDefaults) => {
                paths.extend(ctx_defaults.iter().cloned());
            }
            IgnoreSpec::Path(p) => paths.push(p.clone()),
        }
    }
    paths
}

/// Evaluate a `field` assertion over a response body.
///
/// # Errors
/// [`AssertionFailure`] describing the violated predicate.
#[expect(
    clippy::too_many_arguments,
    reason = "mirrors the assertion's field set"
)]
pub fn eval_field(
    body: &Value,
    path: &str,
    equals: Option<&Value>,
    not_equals: Option<&Value>,
    exists: Option<bool>,
    absent: Option<bool>,
    matches: Option<&str>,
    absent_or_matches: Option<&str>,
) -> Result<(), AssertionFailure> {
    let found = resolve_path(body, path);
    if let Some(true) = exists {
        return found
            .map(|_| ())
            .ok_or_else(|| AssertionFailure(format!("{path}: expected present, is absent")));
    }
    if let Some(true) = absent {
        return match found {
            None => Ok(()),
            Some(v) => Err(AssertionFailure(format!(
                "{path}: expected absent, found {v}"
            ))),
        };
    }
    if let Some(pattern) = absent_or_matches {
        return match found {
            None => Ok(()),
            Some(actual) => match_serialized(path, actual, pattern),
        };
    }
    let Some(actual) = found else {
        return Err(AssertionFailure(format!(
            "{path}: path resolves to nothing"
        )));
    };
    if let Some(want) = equals {
        if resultset::cells_equal(actual, want) {
            return Ok(());
        }
        return Err(AssertionFailure(format!(
            "{path}: {actual} != expected {want}"
        )));
    }
    if let Some(reject) = not_equals {
        if resultset::cells_equal(actual, reject) {
            return Err(AssertionFailure(format!(
                "{path}: equals the client-supplied value {reject} (must be server-set)"
            )));
        }
        return Ok(());
    }
    if let Some(pattern) = matches {
        return match_serialized(path, actual, pattern);
    }
    Ok(())
}

/// Match one resolved field value's serialized form against a regex, shared by
/// the `matches` and `absent_or_matches` predicates.
fn match_serialized(path: &str, actual: &Value, pattern: &str) -> Result<(), AssertionFailure> {
    let re = regex::Regex::new(pattern)
        .map_err(|e| AssertionFailure(format!("{path}: pattern does not compile: {e}")))?;
    let text = match actual {
        Value::String(s) => s.clone(),
        other => other.to_string(),
    };
    if re.is_match(&text) {
        return Ok(());
    }
    Err(AssertionFailure(format!(
        "{path}: {text:?} does not match {pattern:?}"
    )))
}

/// Evaluate the aggregate `unique` assertion over the per-row stores
/// (law e: collected across all rows, evaluated once).
///
/// # Errors
/// [`AssertionFailure`] naming the duplicate value.
pub fn eval_unique(
    over: &crate::ids::CaptureName,
    all_rows: &[VarStore],
) -> Result<(), AssertionFailure> {
    let mut seen: Vec<&str> = Vec::new();
    for (row, store) in all_rows.iter().enumerate() {
        let Some(value) = store.scalar(over) else {
            continue; // rows that never bound the capture do not participate
        };
        if seen.contains(&value) {
            return Err(AssertionFailure(format!(
                "unique over ${{{over}}}: value {value:?} repeats at row {row}"
            )));
        }
        seen.push(value);
    }
    Ok(())
}

/// Evaluate a `returns` assertion against a (scalar-shaped) body.
///
/// # Errors
/// [`AssertionFailure`] describing the mismatch.
pub fn eval_returns(
    body: &Value,
    equals: Option<&Value>,
    matches: Option<&str>,
    omits: Option<&str>,
) -> Result<(), AssertionFailure> {
    if let Some(want) = equals {
        if resultset::cells_equal(body, want) {
            return Ok(());
        }
        return Err(AssertionFailure(format!(
            "returns: {body} != expected {want}"
        )));
    }
    let text = match body {
        Value::String(s) => s.clone(),
        other => other.to_string(),
    };
    if let Some(pattern) = matches {
        let re = regex::Regex::new(pattern)
            .map_err(|e| AssertionFailure(format!("returns pattern does not compile: {e}")))?;
        if !re.is_match(&text) {
            return Err(AssertionFailure(format!(
                "returns: {text:?} does not match {pattern:?}"
            )));
        }
    }
    if let Some(pattern) = omits {
        let re = regex::Regex::new(pattern).map_err(|e| {
            AssertionFailure(format!("returns omits pattern does not compile: {e}"))
        })?;
        if re.is_match(&text) {
            return Err(AssertionFailure(format!(
                "returns: {text:?} matches {pattern:?} but must omit it"
            )));
        }
    }
    Ok(())
}

/// The XML Schema instance namespace, whose `type` attribute selects the
/// concrete type of an element declared with an abstract one
/// (<https://www.w3.org/TR/xmlschema-1/#xsi_type>).
const XSI_NAMESPACE: &str = "http://www.w3.org/2001/XMLSchema-instance";

/// The judged facts of an XML document entity's ROOT element.
#[derive(Debug, Clone, PartialEq, Eq)]
struct XmlRootElement {
    /// The root's LOCAL name.
    local: String,
    /// The namespace URI the root resolves to, when the document binds one.
    namespace: Option<String>,
    /// The root's `xsi:type`, when present: the `QName`'s LOCAL part and the
    /// namespace URI its prefix resolves to (absent when the `QName` is
    /// unprefixed and the document binds no default namespace).
    xsi_type: Option<(String, Option<String>)>,
}

/// The `xsi:type` an element carries, resolved: the `QName`'s LOCAL part and the
/// namespace URI its prefix resolves to.
///
/// The attribute is identified by its resolved NAME (the XML Schema instance
/// namespace + local `type`), never by the literal prefix `xsi`, which a
/// document is free to spell any way it binds it. Its VALUE is a `QName` and
/// resolves by the `QName`-in-content rule — an unprefixed `QName` takes the
/// document's DEFAULT namespace — which `resolve_element` implements.
///
/// # Errors
/// A message when an attribute is malformed or its value is not valid UTF-8,
/// or when the `xsi:type` `QName` carries a prefix the document never bound.
fn root_xsi_type(
    reader: &mut quick_xml::NsReader<&[u8]>,
    start: &quick_xml::events::BytesStart<'_>,
) -> Result<Option<(String, Option<String>)>, AssertionFailure> {
    for attribute in start.attributes() {
        let attribute = attribute
            .map_err(|e| AssertionFailure(format!("xml_root: body is not well-formed XML: {e}")))?;
        let (attribute_ns, attribute_local) =
            reader.resolver_mut().resolve_attribute(attribute.key);
        let is_xsi_type = attribute_local.as_ref() == b"type"
            && matches!(
                attribute_ns,
                quick_xml::name::ResolveResult::Bound(ns) if ns.as_ref() == XSI_NAMESPACE.as_bytes()
            );
        if !is_xsi_type {
            continue;
        }
        // Attribute-value normalization per XML 1.0 §3.3.3 (the version every
        // published ITS-XML schema and canonical openEHR document is written
        // in — `<?xml version="1.0"?>`): entity references resolved, tab/CR/LF
        // folded to spaces, before the value is read as a QName.
        let value = attribute
            .normalized_value(quick_xml::XmlVersion::Implicit1_0)
            .map_err(|e| AssertionFailure(format!("xml_root: xsi:type is not readable: {e}")))?;
        let (type_ns, type_local) = reader
            .resolver_mut()
            .resolve_element(quick_xml::name::QName(value.as_bytes()));
        let namespace = match type_ns {
            quick_xml::name::ResolveResult::Bound(ns) => {
                Some(String::from_utf8_lossy(ns.as_ref()).into_owned())
            }
            quick_xml::name::ResolveResult::Unbound => None,
            quick_xml::name::ResolveResult::Unknown(prefix) => {
                return Err(AssertionFailure(format!(
                    "xml_root: the xsi:type QName's prefix `{}` is not bound to any namespace",
                    String::from_utf8_lossy(&prefix)
                )));
            }
        };
        return Ok(Some((
            String::from_utf8_lossy(type_local.as_ref()).into_owned(),
            namespace,
        )));
    }
    Ok(None)
}

/// The root element of an XML document entity: its LOCAL name, the namespace
/// URI it resolves to when the document binds one, and its `xsi:type` when it
/// carries one.
///
/// Namespace resolution is delegated to `quick_xml::NsReader` rather than
/// pattern-matched out of the text: a conforming document may bind the
/// namespace with any prefix (or as the default `xmlns`), and only a real
/// resolver relates the root's prefix to the URI in scope for it. The
/// `xsi:type` VALUE is a `QName` too and resolves by the `QName`-in-content rule —
/// an unprefixed `QName` takes the DEFAULT namespace — which is what
/// `resolve_element` implements.
///
/// The whole document is read to end-of-input, not just its first tag: a
/// payload that is not well-formed cannot be valid against any schema either,
/// so the same §"XML Format" MUST that fixes the root also rules it out.
///
/// # Errors
/// A message when the payload is not a well-formed XML document entity.
fn xml_root_element(text: &str) -> Result<XmlRootElement, AssertionFailure> {
    let mut reader = quick_xml::NsReader::from_str(text);
    let mut root: Option<XmlRootElement> = None;
    // Element balance, tracked here rather than left to the reader's
    // configuration: at end of input an unclosed element is a truncated
    // document, which no schema can validate.
    let mut depth: i64 = 0;
    loop {
        let (resolved, event) = reader
            .read_resolved_event()
            .map_err(|e| AssertionFailure(format!("xml_root: body is not well-formed XML: {e}")))?;
        if matches!(event, quick_xml::events::Event::Start(_)) {
            depth += 1;
        } else if matches!(event, quick_xml::events::Event::End(_)) {
            depth -= 1;
        }
        match event {
            quick_xml::events::Event::Eof => break,
            quick_xml::events::Event::Start(e) | quick_xml::events::Event::Empty(e)
                if root.is_none() =>
            {
                let local = String::from_utf8_lossy(e.local_name().as_ref()).into_owned();
                let namespace = match resolved {
                    quick_xml::name::ResolveResult::Bound(ns) => {
                        Some(String::from_utf8_lossy(ns.as_ref()).into_owned())
                    }
                    quick_xml::name::ResolveResult::Unbound => None,
                    quick_xml::name::ResolveResult::Unknown(prefix) => {
                        return Err(AssertionFailure(format!(
                            "xml_root: the root element's prefix `{}` is not bound to any namespace",
                            String::from_utf8_lossy(&prefix)
                        )));
                    }
                };
                let xsi_type = root_xsi_type(&mut reader, &e)?;
                root = Some(XmlRootElement {
                    local,
                    namespace,
                    xsi_type,
                });
            }
            // The prolog (declaration, doctype, comments, whitespace) before
            // the root, and the whole content after it: read through, so an
            // ill-formed document fails rather than passing on its first tag.
            _ => {}
        }
    }
    if depth != 0 {
        return Err(AssertionFailure(
            "xml_root: body is not well-formed XML: the document ends with unclosed elements"
                .to_owned(),
        ));
    }
    root.ok_or_else(|| AssertionFailure("xml_root: body carries no XML element at all".to_owned()))
}

/// Evaluate an `xml_root` assertion over a canonical-XML response body.
///
/// # Errors
/// [`AssertionFailure`] when the body is not an XML document entity, when the
/// root's local name differs, when its namespace is not the expected published
/// openEHR ITS-XML target namespace, or when an expected `xsi_type` is absent
/// or names another type.
pub fn eval_xml_root(
    body: &Value,
    name: &str,
    namespace: Option<crate::vocab::XmlNamespace>,
    xsi_type: Option<&str>,
) -> Result<(), AssertionFailure> {
    let Value::String(text) = body else {
        return Err(AssertionFailure(format!(
            "xml_root: expected a canonical-XML document body, got {}",
            match body {
                Value::Null => "no body".to_owned(),
                other => other.to_string().chars().take(80).collect::<String>(),
            }
        )));
    };
    let root = xml_root_element(text)?;
    let local = root.local;
    if local != name {
        return Err(AssertionFailure(format!(
            "xml_root: document root is `{local}`, expected the published document element `{name}`"
        )));
    }
    if let Some(expected) = namespace {
        match root.namespace.as_deref() {
            Some(uri) if expected.accepts(uri) => {}
            Some(uri) => {
                return Err(AssertionFailure(format!(
                    "xml_root: root `{local}` is in namespace {uri:?}, expected {}",
                    expected.token()
                )));
            }
            None => {
                return Err(AssertionFailure(format!(
                    "xml_root: root `{local}` is in NO namespace, expected {} — every published \
                     ITS-XML schema declares elementFormDefault=\"qualified\" over its \
                     targetNamespace, so a conforming document's root is namespace-qualified",
                    expected.token()
                )));
            }
        }
    }
    let Some(expected_type) = xsi_type else {
        return Ok(());
    };
    // An element whose XSD-declared type is abstract MUST name a non-abstract
    // derived type with `xsi:type` (XML Schema Part 1 §2.6.1 + §3.4.6,
    // <https://www.w3.org/TR/xmlschema-1/#xsi_type>).
    let Some((type_local, type_uri)) = root.xsi_type else {
        return Err(AssertionFailure(format!(
            "xml_root: root `{local}` carries no xsi:type, expected `{expected_type}` — the \
             published element's declared type is abstract, and an instance may not use an \
             abstract type directly"
        )));
    };
    if type_local != expected_type {
        return Err(AssertionFailure(format!(
            "xml_root: root `{local}` names concrete type `{type_local}`, expected \
             `{expected_type}`"
        )));
    }
    // The ITS-XML complexTypes are declared in each schema's own
    // `targetNamespace`, so a type QName in another namespace names another
    // schema's type — judged by the same expectation as the root.
    if let Some(expected) = namespace {
        match type_uri.as_deref() {
            Some(uri) if expected.accepts(uri) => {}
            Some(uri) => {
                return Err(AssertionFailure(format!(
                    "xml_root: xsi:type `{type_local}` is in namespace {uri:?}, expected {}",
                    expected.token()
                )));
            }
            None => {
                return Err(AssertionFailure(format!(
                    "xml_root: xsi:type `{type_local}` resolves to NO namespace, expected {} — \
                     the ITS-XML complexTypes are declared in each schema's targetNamespace",
                    expected.token()
                )));
            }
        }
    }
    Ok(())
}

/// Whether an assertion is wire-dependent (its facts need a driver read:
/// versioned-object reads for `version`/`signature`, schema validation for
/// `instance_of`). The pure evaluators above handle the rest.
#[must_use]
pub fn is_wire_dependent(assertion: &Assertion) -> bool {
    matches!(
        assertion,
        Assertion::Version { .. } | Assertion::Signature { .. } | Assertion::InstanceOf { .. }
    )
}

/// When the driver judges an assertion's declared facts.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Judgement {
    /// Judged where it is authored: on its flow step, or after the flow for a
    /// postcondition.
    PerStep,
    /// Judged once per case, after the last row (interpreter law e).
    Aggregate,
    /// Carries no pass/fail criterion of its own. Exactly two members, both
    /// adjudicated: `message_exemplar` (register AMB-1 — the schedule's error
    /// prose is never a criterion) and `state`, whose machine verification is
    /// the case its `verified_by` names.
    Informative,
}

/// The judgement the driver gives an assertion.
///
/// The match is exhaustive on purpose: a new assertion variant cannot be added
/// without classifying it here, and a variant classified [`Judgement::PerStep`]
/// that no evaluator reaches would be an assertion authored in the catalogue
/// and never judged — the silent-pass class this instrument refuses.
#[must_use]
pub fn judgement_of(assertion: &Assertion) -> Judgement {
    match assertion {
        Assertion::Field { .. }
        | Assertion::Equivalent { .. }
        | Assertion::Returns { .. }
        | Assertion::ResultSet { .. }
        | Assertion::XmlRoot { .. }
        | Assertion::InstanceOf { .. }
        | Assertion::Signature { .. }
        | Assertion::Version { .. } => Judgement::PerStep,
        Assertion::Unique { .. } => Judgement::Aggregate,
        Assertion::MessageExemplar { .. } | Assertion::State { .. } => Judgement::Informative,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    /// Every assertion variant's judgement, pinned by name.
    ///
    /// Reclassifying a judged family as informative is how a whole catalogue
    /// chapter goes back to passing on an arm that evaluates nothing, so the
    /// classification is a test, not a convention.
    #[test]
    fn every_assertion_variant_declares_when_it_is_judged() {
        let cases: &[(Value, Judgement)] = &[
            (
                json!({ "assert": "field", "path": "uid/value", "exists": true }),
                Judgement::PerStep,
            ),
            (
                json!({ "assert": "equivalent", "to": "committed" }),
                Judgement::PerStep,
            ),
            (
                json!({ "assert": "returns", "equals": true }),
                Judgement::PerStep,
            ),
            (
                json!({ "assert": "result_set", "match": "count", "count": 1 }),
                Judgement::PerStep,
            ),
            (
                json!({ "assert": "xml_root", "name": "composition" }),
                Judgement::PerStep,
            ),
            (
                json!({ "assert": "instance_of", "rm_type": "COMPOSITION" }),
                Judgement::PerStep,
            ),
            (
                json!({ "assert": "signature", "of": "${v1}", "present": true }),
                Judgement::PerStep,
            ),
            (
                json!({ "assert": "version", "count": 1 }),
                Judgement::PerStep,
            ),
            (
                json!({ "assert": "unique", "over": "${new_ehr_id}", "aggregate": true }),
                Judgement::Aggregate,
            ),
            (
                json!({ "assert": "message_exemplar", "text": "EHR not found" }),
                Judgement::Informative,
            ),
            (
                json!({ "assert": "state", "text": "the EHR exists" }),
                Judgement::Informative,
            ),
        ];
        for (document, expected) in cases {
            let assertion: Assertion = serde_json::from_value(document.clone())
                .unwrap_or_else(|e| panic!("{document} does not parse: {e}"));
            assert_eq!(
                judgement_of(&assertion),
                *expected,
                "{document} changed judgement"
            );
        }
    }

    #[test]
    fn path_resolution_addresses_objects_and_lists() {
        let body = json!({
            "context": { "setting": { "value": "other care" } },
            "content": [ { "data": { "events": [ { "time": "t0" } ] } } ]
        });
        assert_eq!(
            resolve_path(&body, "context/setting/value").unwrap(),
            &json!("other care")
        );
        assert_eq!(
            resolve_path(&body, "content[0]/data/events[0]/time").unwrap(),
            &json!("t0")
        );
        assert!(resolve_path(&body, "content[1]").is_none());
    }

    #[test]
    fn recursive_ignore_segment_strips_every_depth() {
        // A FOLDER tree (RM common `folder.adoc`: `folders: List<FOLDER>`),
        // with a uid on the root and on each nested node.
        let tree = json!({
            "_type": "FOLDER",
            "uid": { "_type": "OBJECT_VERSION_ID", "value": "r::s::1" },
            "folders": [
                {
                    "_type": "FOLDER",
                    "uid": { "_type": "HIER_OBJECT_ID", "value": "a" },
                    "name": { "value": "emergency" },
                    "folders": [
                        { "_type": "FOLDER", "uid": { "value": "b" }, "name": { "value": "episode" } }
                    ]
                }
            ]
        });
        // A depth-anchored path reaches only the root.
        let shallow = strip_ignored(&tree, &["uid".to_owned()]);
        assert!(shallow.get("uid").is_none());
        assert!(shallow["folders"][0].get("uid").is_some());
        // `**/uid` reaches the root and every nested node.
        let deep = strip_ignored(&tree, &["**/uid".to_owned()]);
        assert!(deep.get("uid").is_none());
        assert!(deep["folders"][0].get("uid").is_none());
        assert!(deep["folders"][0]["folders"][0].get("uid").is_none());
        // Nothing else is touched.
        assert_eq!(deep["folders"][0]["name"]["value"], json!("emergency"));
        assert_eq!(
            deep["folders"][0]["folders"][0]["name"]["value"],
            json!("episode")
        );
    }

    #[test]
    fn field_predicates() {
        let body =
            json!({ "is_queryable": true, "audit": { "time_committed": "2026-07-21T10:00:00Z" } });
        assert!(
            eval_field(
                &body,
                "is_queryable",
                Some(&json!(true)),
                None,
                None,
                None,
                None,
                None
            )
            .is_ok()
        );
        assert!(
            eval_field(
                &body,
                "is_queryable",
                None,
                None,
                Some(true),
                None,
                None,
                None
            )
            .is_ok()
        );
        assert!(eval_field(&body, "subject", None, None, None, Some(true), None, None).is_ok());
        // the server-set predicate: stored time must differ from the client value
        assert!(
            eval_field(
                &body,
                "audit/time_committed",
                None,
                Some(&json!("1990-01-01T00:00:00Z")),
                None,
                None,
                None,
                None
            )
            .is_ok()
        );
        assert!(
            eval_field(
                &body,
                "audit/time_committed",
                None,
                Some(&json!("2026-07-21T10:00:00Z")),
                None,
                None,
                None,
                None
            )
            .is_err()
        );
    }

    #[test]
    fn equivalence_strips_normative_ignore_sets_only() {
        let committed = json!({ "name": { "value": "v1" }, "content": [{"x": 1}] });
        let served = json!({
            "uid": { "value": "generated::sut::1" },
            "name": { "value": "v1" },
            "content": [{"x": 1}]
        });
        assert!(equivalent(&served, &committed, &["uid".to_owned()]));
        assert!(!equivalent(&served, &committed, &[])); // nothing stripped -> uid differs
    }

    #[test]
    fn xml_root_judges_the_published_element_and_its_namespace() {
        use crate::vocab::XmlNamespace;

        let v1 = Value::String(
            r#"<?xml version="1.0" encoding="UTF-8"?>
               <composition xmlns="http://schemas.openehr.org/v1"><name/></composition>"#
                .to_owned(),
        );
        assert!(eval_xml_root(&v1, "composition", Some(XmlNamespace::Published), None).is_ok());
        assert!(eval_xml_root(&v1, "composition", Some(XmlNamespace::V1), None).is_ok());
        assert!(eval_xml_root(&v1, "composition", Some(XmlNamespace::V2), None).is_err());

        // A prefix binding is equally conforming — only the URI is asserted.
        let prefixed = Value::String(
            r#"<oe:composition xmlns:oe="http://schemas.openehr.org/v2"/>"#.to_owned(),
        );
        assert!(
            eval_xml_root(
                &prefixed,
                "composition",
                Some(XmlNamespace::Published),
                None
            )
            .is_ok()
        );

        // The defect this assertion exists for: a root in NO namespace, against
        // schemas that are elementFormDefault="qualified" over a targetNamespace.
        let unqualified = Value::String(r#"<composition archetype_node_id="x"/>"#.to_owned());
        let failure = eval_xml_root(
            &unqualified,
            "composition",
            Some(XmlNamespace::Published),
            None,
        )
        .expect_err("an unqualified root must fail");
        assert!(failure.0.contains("NO namespace"), "{failure:?}");
        // …and the name-only row still passes it, which is why the namespace
        // fact needs its own assertion rather than a regex over the body.
        assert!(eval_xml_root(&unqualified, "composition", None, None).is_ok());

        let wrong_name =
            Value::String(r#"<folder xmlns="http://schemas.openehr.org/v1"/>"#.to_owned());
        assert!(eval_xml_root(&wrong_name, "composition", None, None).is_err());

        // A JSON body is not an XML document entity.
        assert!(
            eval_xml_root(
                &json!({ "_type": "COMPOSITION" }),
                "composition",
                None,
                None
            )
            .is_err()
        );
        assert!(eval_xml_root(&Value::Null, "composition", None, None).is_err());
        // Malformed XML fails loudly rather than silently passing.
        let malformed = Value::String("<composition>".to_owned());
        assert!(eval_xml_root(&malformed, "composition", None, None).is_err());
    }

    /// The abstract-root half: `ALL/Version.xsd` publishes
    /// `<xs:element name="version" type="VERSION"/>` over
    /// `<xs:complexType name="VERSION" abstract="true">`, and XML Schema Part 1
    /// §2.6.1 + §3.4.6 (<https://www.w3.org/TR/xmlschema-1/#xsi_type>) forbid an
    /// instance from using an abstract type directly — it must select a
    /// non-abstract derived type with `xsi:type`. So on that root the concrete
    /// class is a judged fact, and it is what tells an `ORIGINAL_VERSION`
    /// response apart from an `IMPORTED_VERSION` one.
    #[test]
    fn xml_root_judges_the_concrete_type_of_an_abstract_root() {
        use crate::vocab::XmlNamespace;

        let original = Value::String(
            r#"<version xmlns="http://schemas.openehr.org/v1"
                        xmlns:xsi="http://www.w3.org/2001/XMLSchema-instance"
                        xsi:type="ORIGINAL_VERSION"><uid/></version>"#
                .to_owned(),
        );
        assert!(
            eval_xml_root(
                &original,
                "version",
                Some(XmlNamespace::Published),
                Some("ORIGINAL_VERSION")
            )
            .is_ok()
        );
        // The discrimination the attribute exists for.
        let failure = eval_xml_root(
            &original,
            "version",
            Some(XmlNamespace::Published),
            Some("IMPORTED_VERSION"),
        )
        .expect_err("a different concrete type must fail");
        assert!(failure.0.contains("ORIGINAL_VERSION"), "{failure:?}");

        // The prefix is the document's own choice, on the attribute NAME and
        // inside the QName VALUE alike; both resolve, neither is matched.
        let prefixed = Value::String(
            r#"<oe:version xmlns:oe="http://schemas.openehr.org/v1"
                           xmlns:i="http://www.w3.org/2001/XMLSchema-instance"
                           i:type="oe:IMPORTED_VERSION"/>"#
                .to_owned(),
        );
        assert!(
            eval_xml_root(
                &prefixed,
                "version",
                Some(XmlNamespace::Published),
                Some("IMPORTED_VERSION")
            )
            .is_ok()
        );

        // An abstract root with no xsi:type at all is invalid against the
        // published schema, and says so.
        let bare = Value::String(r#"<version xmlns="http://schemas.openehr.org/v1"/>"#.to_owned());
        let failure = eval_xml_root(
            &bare,
            "version",
            Some(XmlNamespace::Published),
            Some("ORIGINAL_VERSION"),
        )
        .expect_err("an abstract root must name its concrete type");
        assert!(failure.0.contains("no xsi:type"), "{failure:?}");
        // …and a row that does not assert the type still passes it, which is
        // why the concrete class needs its own field.
        assert!(eval_xml_root(&bare, "version", Some(XmlNamespace::Published), None).is_ok());

        // A type QName from a foreign namespace names another schema's type.
        let foreign = Value::String(
            r#"<version xmlns="http://schemas.openehr.org/v1"
                        xmlns:xsi="http://www.w3.org/2001/XMLSchema-instance"
                        xmlns:x="http://example.org/other"
                        xsi:type="x:ORIGINAL_VERSION"/>"#
                .to_owned(),
        );
        let failure = eval_xml_root(
            &foreign,
            "version",
            Some(XmlNamespace::Published),
            Some("ORIGINAL_VERSION"),
        )
        .expect_err("a foreign type namespace must fail");
        assert!(failure.0.contains("example.org"), "{failure:?}");

        // An unbound prefix on the QName is a defect, not a silent local name.
        let unbound = Value::String(
            r#"<version xmlns="http://schemas.openehr.org/v1"
                        xmlns:xsi="http://www.w3.org/2001/XMLSchema-instance"
                        xsi:type="nope:ORIGINAL_VERSION"/>"#
                .to_owned(),
        );
        assert!(
            eval_xml_root(
                &unbound,
                "version",
                Some(XmlNamespace::Published),
                Some("ORIGINAL_VERSION")
            )
            .is_err()
        );
    }

    #[test]
    fn unique_is_aggregate_across_rows() {
        let name = crate::ids::CaptureName::parse("new_ehr_id").unwrap();
        let mut a = VarStore::default();
        a.set(name.clone(), Captured::Scalar("id-1".into()));
        let mut b = VarStore::default();
        b.set(name.clone(), Captured::Scalar("id-2".into()));
        assert!(eval_unique(&name, &[a.clone(), b]).is_ok());
        let mut c = VarStore::default();
        c.set(name.clone(), Captured::Scalar("id-1".into()));
        assert!(eval_unique(&name, &[a, c]).is_err());
    }

    /// A FLAT body is "key-value pairs at a single level in JSON where …
    /// keys are full WT paths" and "context fields MUST use `ctx/` prefix"
    /// (ITS-REST `docs/simplified_formats/master04-basic_concepts.adoc`
    /// §Format variants → Flat format), which is exactly the shape this
    /// predicate reads: path-formed keys, no nested object under any of them.
    #[test]
    fn a_flat_body_is_recognized_by_its_single_level_of_path_keys() {
        let flat = json!({
            "ctx/language": "en",
            "vital_signs/body_temperature:0/any_event:0/temperature|magnitude": 37.5,
            "vital_signs/body_temperature:0/any_event:0/temperature|unit": "°C"
        });
        assert!(is_flat_map(&flat));
        // A canonical body nests, and carries no path-formed key.
        assert!(!is_flat_map(&json!({
            "_type": "COMPOSITION",
            "name": { "value": "Vital signs" }
        })));
        // One path-formed key is not enough if a value still nests.
        assert!(!is_flat_map(&json!({
            "vital_signs/x|magnitude": 1,
            "nested": { "a": 1 }
        })));
        assert!(!is_flat_map(&json!({})), "an empty object is not a body");
        assert!(!is_flat_map(&json!([])), "an array is not a FLAT map");
    }

    /// The STRUCTURED→FLAT algorithm: "build path by concatenating property
    /// names with forward slash", "for properties with a pipe prefix, append
    /// to a parent path with pipe", "unwrap arrays" and "preserve instance
    /// indices" (ITS-REST
    /// `docs/simplified_formats/master04-basic_concepts.adoc` §Conversion
    /// Between Formats → Structured to Flat). The empty-string key is the
    /// element's own main value, so it flattens onto the parent path itself.
    #[test]
    fn a_structured_body_flattens_onto_its_flat_key_form() {
        let structured = json!({
            "vital_signs": [ {
                "body_temperature": [ {
                    "any_event": [ {
                        "temperature": [ { "|magnitude": 37.5, "|unit": "°C" } ],
                        "time": [ { "": "2026-07-21T10:00:00Z" } ]
                    } ]
                } ]
            } ]
        });
        let mut flat = BTreeMap::new();
        flatten_structured(&structured, "", &mut flat);
        let keys: Vec<&str> = flat.keys().map(String::as_str).collect();
        assert_eq!(
            keys,
            vec![
                "vital_signs:0/body_temperature:0/any_event:0/temperature:0|magnitude",
                "vital_signs:0/body_temperature:0/any_event:0/temperature:0|unit",
                "vital_signs:0/body_temperature:0/any_event:0/time:0",
            ]
        );
        assert_eq!(
            flat.get("vital_signs:0/body_temperature:0/any_event:0/temperature:0|magnitude"),
            Some(&json!(37.5))
        );
    }

    /// The attribute-key shape is what tells a simplified body from a
    /// canonical one: `|`-prefixed and empty-string keys are STRUCTURED
    /// spellings (master04 §Format variants), and canonical JSON carries
    /// neither — it carries `_type`.
    #[test]
    fn a_canonical_body_is_never_read_as_a_simplified_one() {
        let canonical = json!({
            "_type": "COMPOSITION",
            "name": { "_type": "DV_TEXT", "value": "Vital signs" }
        });
        assert!(!has_simplified_leaf_keys(&canonical));
        assert!(
            simplified_as_flat(&canonical).is_none(),
            "a canonical body has no FLAT reading"
        );
        // A STRUCTURED body does, and it comes back flattened.
        let structured = json!({ "vital_signs": [ { "temperature": [ { "|magnitude": 1 } ] } ] });
        assert!(has_simplified_leaf_keys(&structured));
        let flat = simplified_as_flat(&structured).expect("a STRUCTURED body reads as FLAT");
        assert_eq!(
            flat.keys().map(String::as_str).collect::<Vec<_>>(),
            vec!["vital_signs:0/temperature:0|magnitude"]
        );
        // A FLAT body reads as itself, verbatim.
        let already_flat = json!({ "ctx/language": "en", "vitals/temp|magnitude": 37.5 });
        assert_eq!(
            simplified_as_flat(&already_flat).map(|m| m.len()),
            Some(2),
            "a FLAT body is its own key map"
        );
        assert!(simplified_as_flat(&json!("text")).is_none());
    }

    /// The committed side's `ctx/*` input keys name the same data the
    /// read-back expresses at RM paths: `ctx/participation_<field>:<i>` and
    /// `ctx/health_care_facility|<attr>` (ITS-REST
    /// `docs/simplified_formats/master06-context_information.adoc`
    /// §Participation, §`health_care_facility`), with `ctx/id_namespace` and
    /// `ctx/id_scheme` as the declared defaults for external references
    /// (§ID Namespace and Scheme) rather than data of their own.
    #[test]
    fn the_ctx_input_keys_fold_onto_the_paths_a_read_back_uses() {
        let committed: BTreeMap<String, Value> = [
            ("ctx/participation_name:1".to_owned(), json!("Lara Markham")),
            ("ctx/participation_id:1".to_owned(), json!("198")),
            ("ctx/participation_function".to_owned(), json!("performer")),
            ("ctx/health_care_facility|id".to_owned(), json!("9091")),
            ("ctx/id_namespace".to_owned(), json!("HOSPITAL-NS")),
            ("ctx/id_scheme".to_owned(), json!("HOSPITAL-NS")),
            ("ctx/language".to_owned(), json!("en")),
            ("vitals/temperature|magnitude".to_owned(), json!(37.5)),
        ]
        .into_iter()
        .collect();

        let folded = fold_flat_ctx(&committed, "vitals");
        assert_eq!(
            folded.get("vitals/context/_participation:1|name"),
            Some(&json!("Lara Markham"))
        );
        assert_eq!(
            folded.get("vitals/context/_participation:0|function"),
            Some(&json!("performer")),
            "an index-free participation key is the first participation"
        );
        assert_eq!(
            folded.get("vitals/context/_health_care_facility|id"),
            Some(&json!("9091"))
        );
        // The declared defaults expand onto every folded party carrying an id,
        // and never survive as keys of their own.
        assert_eq!(
            folded.get("vitals/context/_participation:1|id_namespace"),
            Some(&json!("HOSPITAL-NS"))
        );
        assert_eq!(
            folded.get("vitals/context/_health_care_facility|id_scheme"),
            Some(&json!("HOSPITAL-NS"))
        );
        assert!(!folded.contains_key("ctx/id_namespace"));
        assert!(!folded.contains_key("ctx/id_scheme"));
        // Everything else passes through untouched, ctx defaults included:
        // the ignore pass is what excuses them.
        assert_eq!(folded.get("ctx/language"), Some(&json!("en")));
        assert_eq!(
            folded.get("vitals/temperature|magnitude"),
            Some(&json!(37.5))
        );
    }

    /// The ignore pass reads a FLAT key at the RM path it names: the
    /// `ctx/*` default-setter spellings map onto their master06 targets
    /// (`ctx/time` → `context/start_time`, `ctx/setting` → `context/setting`,
    /// `ctx/composer_*` → `composer`), and any other key matches when its
    /// post-root path falls under the ignored path.
    #[test]
    fn a_flat_key_is_ignored_at_the_rm_path_it_names() {
        let ignored = [
            "context/start_time".to_owned(),
            "context/setting".to_owned(),
            "composer".to_owned(),
            "uid".to_owned(),
        ];
        assert!(flat_key_ignored("ctx/time", &ignored));
        assert!(flat_key_ignored("ctx/setting", &ignored));
        assert!(flat_key_ignored("ctx/composer_name", &ignored));
        assert!(flat_key_ignored("ctx/composer_id", &ignored));
        // The post-root path is what is compared, so the root segment of a
        // read-back key never has to be enumerated.
        assert!(flat_key_ignored("vitals/context/start_time", &ignored));
        assert!(flat_key_ignored("vitals/uid|value", &ignored));
        // The flat `_uid` spelling of the same RM attribute is the same datum.
        assert!(flat_key_ignored("vitals/_uid", &ignored));
        // A datum nobody ignored stays compared.
        assert!(!flat_key_ignored("vitals/temperature|magnitude", &ignored));
        assert!(!flat_key_ignored("ctx/end_time", &ignored));
        assert!(flat_key_ignored(
            "ctx/end_time",
            &["context/end_time".to_owned()]
        ));
    }

    /// Keys are compared in one canonical spelling, so a first-element index
    /// written out and one left off name the same datum.
    #[test]
    fn the_first_element_index_is_elided_before_keys_are_compared() {
        assert_eq!(
            dezero("vitals:0/temperature:0|magnitude"),
            "vitals/temperature|magnitude"
        );
        assert_eq!(dezero("vitals/events:0"), "vitals/events");
        assert_eq!(
            dezero("vitals:1/temperature:2|magnitude"),
            "vitals:1/temperature:2|magnitude",
            "only the FIRST element's index is elidable"
        );
        assert_eq!(
            dezero("vitals/temperature|magnitude"),
            "vitals/temperature|magnitude"
        );
    }

    /// The FLAT round-trip rule: every committed datum must come back with an
    /// equal value, read-back surplus is tolerated (the export is the full RM
    /// projection of the committed data, master04 §Format variants), and an
    /// ignored path is compared on neither side.
    #[test]
    fn a_flat_round_trip_loses_no_committed_datum_and_tolerates_surplus() {
        let committed = json!({
            "ctx/language": "en",
            "ctx/time": "2026-07-21T10:00:00Z",
            "vitals/temperature|magnitude": 37.5,
            "vitals/temperature|unit": "°C"
        });
        let read_back = json!({
            "vitals/temperature:0|magnitude": 37.5,
            "vitals/temperature:0|unit": "°C",
            "vitals/context/start_time": "2026-07-21T10:00:04Z",
            "vitals/category|code_string": "433",
            "vitals/_uid": "8849182c-82ad-4088-a07f-48ead4180515::sut::1",
            "ctx/language": "en"
        });
        let ignored = ["context/start_time".to_owned(), "uid".to_owned()];
        assert!(
            equivalent(&read_back, &committed, &ignored),
            "the read-back carries every committed datum"
        );

        // A changed datum is a failure, surplus or not.
        let altered = json!({
            "vitals/temperature:0|magnitude": 38.5,
            "vitals/temperature:0|unit": "°C",
            "ctx/language": "en"
        });
        assert!(!equivalent(&altered, &committed, &ignored));

        // A dropped datum is a failure too.
        let lossy = json!({
            "vitals/temperature:0|magnitude": 37.5,
            "ctx/language": "en"
        });
        assert!(!equivalent(&lossy, &committed, &ignored));

        // Without the ignore set the server-set start_time is compared, and
        // the committed `ctx/time` no longer matches it.
        assert!(!equivalent(&read_back, &committed, &[]));
    }

    /// ITS-REST overview `Resources.md` §JSON Format makes the `_type`
    /// self-tag CONDITIONAL while the requirement governs its VALUE, so a
    /// fully self-tagging codec and a sparsely tagged committed twin describe
    /// the same RM content — but two DIFFERENT tags are a real polymorphic
    /// substitution and stay detectable.
    #[test]
    fn a_type_self_tag_present_on_one_side_only_is_not_a_content_difference() {
        let served = json!({
            "_type": "COMPOSITION",
            "name": { "_type": "DV_TEXT", "value": "Vital signs" }
        });
        let committed = json!({ "name": { "value": "Vital signs" } });
        assert!(equivalent(&served, &committed, &[]));

        let substituted = json!({
            "_type": "COMPOSITION",
            "name": { "_type": "DV_CODED_TEXT", "value": "Vital signs" }
        });
        let tagged_committed = json!({
            "name": { "_type": "DV_TEXT", "value": "Vital signs" }
        });
        assert!(
            !equivalent(&substituted, &tagged_committed, &[]),
            "two different concrete types are not the same content"
        );
        // Array length is content, never padding.
        assert!(!equivalent(
            &json!({ "content": [1, 2] }),
            &json!({ "content": [1] }),
            &[]
        ));
    }

    /// The named ignore-sets come from the two artifacts that define them —
    /// the binding's `server_assigned` list and the selectors vocabulary's
    /// `ctx_defaults` — and an explicit path passes through as written.
    #[test]
    fn named_ignore_sets_expand_from_their_own_artifacts() {
        use crate::model::assertion::IgnoreSpec;

        let specs = vec![
            IgnoreSpec::Named(IgnoreSetName::ServerAssigned),
            IgnoreSpec::Named(IgnoreSetName::CtxDefaults),
            IgnoreSpec::Path("content[0]/uid".to_owned()),
        ];
        let resolved = resolve_ignore_sets(
            &specs,
            &["uid".to_owned(), "**/uid".to_owned()],
            &["context/start_time".to_owned()],
        );
        assert_eq!(
            resolved,
            vec![
                "uid".to_owned(),
                "**/uid".to_owned(),
                "context/start_time".to_owned(),
                "content[0]/uid".to_owned(),
            ],
            "the sets expand in the order the row declares them"
        );
        assert!(
            resolve_ignore_sets(&[], &["uid".to_owned()], &[]).is_empty(),
            "a row that ignores nothing strips nothing"
        );
    }

    /// The `returns` predicates over a scalar-shaped body: an equal value, a
    /// pattern that must match, and a pattern that must NOT appear.
    #[test]
    fn returns_predicates_judge_the_whole_body() {
        assert!(eval_returns(&json!(3), Some(&json!(3)), None, None).is_ok());
        let failure = eval_returns(&json!(3), Some(&json!(4)), None, None).expect_err("3 is not 4");
        assert!(failure.0.contains("!= expected"), "{failure:?}");

        assert!(eval_returns(&json!("v1.2.3"), None, Some(r"^v\d+\.\d+"), None).is_ok());
        assert!(eval_returns(&json!("draft"), None, Some(r"^v\d+"), None).is_err());

        // `omits` is the negative direction: the body must not carry it.
        assert!(eval_returns(&json!("public data"), None, None, Some("secret")).is_ok());
        let leaked = eval_returns(&json!("carries a secret"), None, None, Some("secret"))
            .expect_err("a body that must omit the pattern carries it");
        assert!(leaked.0.contains("must omit"), "{leaked:?}");

        // A pattern that does not compile is reported as such, never as a
        // silently passing row.
        let broken = eval_returns(&json!("x"), None, Some("("), None)
            .expect_err("an uncompilable pattern is a failure");
        assert!(broken.0.contains("does not compile"), "{broken:?}");
        let broken = eval_returns(&json!("x"), None, None, Some("("))
            .expect_err("an uncompilable omits pattern is a failure");
        assert!(broken.0.contains("does not compile"), "{broken:?}");
    }

    /// The `field` predicates that the existing battery leaves open: a
    /// pattern over a resolved leaf, a path that resolves to nothing, and an
    /// `absent` expectation the body contradicts.
    #[test]
    fn field_predicates_report_the_predicate_they_violated() {
        let body = json!({
            "system_id": "sut.example.org",
            "versions": [ { "uid": { "value": "a::b::1" } } ]
        });
        assert!(
            eval_field(
                &body,
                "system_id",
                None,
                None,
                None,
                None,
                Some(r"\.org$"),
                None
            )
            .is_ok()
        );
        let failure = eval_field(
            &body,
            "system_id",
            None,
            None,
            None,
            None,
            Some(r"^\d+$"),
            None,
        )
        .expect_err("an identifier is not digits");
        assert!(failure.0.contains("does not match"), "{failure:?}");

        let failure = eval_field(
            &body,
            "missing/leaf",
            Some(&json!(1)),
            None,
            None,
            None,
            None,
            None,
        )
        .expect_err("an unresolvable path cannot be compared");
        assert!(failure.0.contains("resolves to nothing"), "{failure:?}");

        let failure = eval_field(&body, "system_id", None, None, None, Some(true), None, None)
            .expect_err("a present attribute is not absent");
        assert!(failure.0.contains("expected absent"), "{failure:?}");

        let failure = eval_field(&body, "audit", None, None, Some(true), None, None, None)
            .expect_err("an absent attribute is not present");
        assert!(failure.0.contains("expected present"), "{failure:?}");

        // An uncompilable pattern is a failure, not a pass.
        assert!(eval_field(&body, "system_id", None, None, None, None, Some("("), None).is_err());
        // A row with no predicate at all asserts only that the path resolves.
        assert!(
            eval_field(
                &body,
                "versions[0]/uid/value",
                None,
                None,
                None,
                None,
                None,
                None
            )
            .is_ok()
        );
    }

    /// The optional-member predicate stays silent on an absent member and
    /// judges a present one, which is what an OPTIONAL member carrying a
    /// declared shape needs (ITS-REST `docs/query/Response.md` §Metadata).
    #[test]
    fn absent_or_matches_passes_on_absence_and_judges_on_presence() {
        let body = json!({ "meta": { "_created": "2026-07-21T10:00:00Z" } });
        // Absent: nothing to judge, so the row passes.
        assert!(
            eval_field(
                &body,
                "meta/_generator",
                None,
                None,
                None,
                None,
                None,
                Some("^x")
            )
            .is_ok()
        );
        assert!(
            eval_field(
                &body,
                "meta/_created",
                None,
                None,
                None,
                None,
                None,
                Some(r"^\d{4}-\d{2}-\d{2}T")
            )
            .is_ok()
        );
        let failure = eval_field(
            &body,
            "meta/_created",
            None,
            None,
            None,
            None,
            None,
            Some(r"^\d+$"),
        )
        .expect_err("an extended ISO 8601 date-time is not a run of digits");
        assert!(failure.0.contains("does not match"), "{failure:?}");
    }

    /// A malformed index step addresses nothing rather than panicking or
    /// silently dropping the step: an assertion over a mis-authored path
    /// reports "resolves to nothing" instead of passing over the whole body.
    #[test]
    fn a_malformed_index_step_resolves_to_nothing() {
        let body = json!({ "versions": [{ "uid": "a" }, { "uid": "b" }] });
        assert_eq!(
            resolve_path(&body, "versions[1]/uid"),
            Some(&json!("b")),
            "the well-formed index addresses its element"
        );
        // A non-numeric index, an unterminated one, and an index past the end.
        for path in ["versions[x]/uid", "versions[0/uid", "versions[9]/uid"] {
            assert_eq!(resolve_path(&body, path), None, "{path}");
        }
        // A bare index step with no attribute indexes the CURRENT value.
        assert_eq!(resolve_path(&body, "versions/[0]/uid"), Some(&json!("a")));
        // A leading and a doubled separator are empty steps, which are skipped.
        assert_eq!(resolve_path(&body, "//versions[0]//uid"), Some(&json!("a")));
    }

    /// A template the driver was supposed to pre-resolve is a LOUD failure:
    /// only captures render here, so a `${row.…}` or an unbound or non-scalar
    /// capture never becomes an empty segment in a compared value.
    #[test]
    fn only_scalar_captures_render_and_everything_else_is_refused() {
        use crate::refgrammar::Template;

        let mut vars = VarStore::default();
        vars.set(
            crate::ids::CaptureName::parse("ehr_id").unwrap(),
            Captured::Scalar("e-1".to_owned()),
        );
        vars.set(
            crate::ids::CaptureName::parse("uids").unwrap(),
            Captured::List(vec!["a".to_owned()]),
        );

        let bound = Template::parse("/ehr/${ehr_id}").unwrap();
        assert_eq!(render_template(&bound, &vars).unwrap(), "/ehr/e-1");

        let non_scalar = Template::parse("${uids}").unwrap();
        assert_eq!(
            render_template(&non_scalar, &vars),
            Err("capture uids is not scalar".to_owned())
        );

        let unbound = Template::parse("${ghost}").unwrap();
        assert_eq!(
            render_template(&unbound, &vars),
            Err("capture ghost is not bound".to_owned())
        );

        let driver_side = Template::parse("${row.ehr_id}").unwrap();
        let failure = render_template(&driver_side, &vars)
            .expect_err("a row reference is the driver's to resolve");
        assert!(
            failure.contains("must be resolved by the driver"),
            "{failure}"
        );
    }

    /// An ignore path descends through objects and lists, and an empty path
    /// strips nothing — an ignore set can never quietly blank a whole body.
    #[test]
    fn an_ignore_path_descends_and_an_empty_one_strips_nothing() {
        let body = json!({
            "context": { "start_time": "t", "setting": "s" },
            "versions": [
                { "uid": "a", "commit_audit": { "time_committed": "t1" } },
                { "uid": "b", "commit_audit": { "time_committed": "t2" } }
            ]
        });

        let nested = strip_ignored(&body, &["context/start_time".to_owned()]);
        assert_eq!(nested["context"], json!({ "setting": "s" }));
        assert_eq!(nested["versions"], body["versions"], "untouched elsewhere");

        // Through a list: every element loses the named leaf.
        let through_list =
            strip_ignored(&body, &["versions/commit_audit/time_committed".to_owned()]);
        for version in through_list["versions"].as_array().unwrap() {
            assert_eq!(version["commit_audit"], json!({}));
            assert!(version["uid"].is_string(), "siblings survive");
        }

        // An empty path names nothing, so the body comes back whole.
        assert_eq!(strip_ignored(&body, &[String::new()]), body);
        assert_eq!(strip_ignored(&body, &["/".to_owned()]), body);
        // A path through a scalar leaf strips nothing rather than failing.
        assert_eq!(
            strip_ignored(&body, &["context/setting/deeper".to_owned()]),
            body
        );
    }

    /// A non-string leaf is compared as its JSON text, so a numeric or object
    /// value still faces a `matches:` predicate rather than passing untested.
    #[test]
    fn a_non_string_leaf_is_matched_as_its_json_text() {
        let body = json!({ "magnitude": 140, "uid": { "value": "a::b::1" } });
        assert!(
            eval_field(
                &body,
                "magnitude",
                None,
                None,
                None,
                None,
                Some(r"^\d+$"),
                None
            )
            .is_ok()
        );
        let failure = eval_field(&body, "magnitude", None, None, None, None, Some("^x"), None)
            .expect_err("140 does not start with x");
        assert!(failure.0.contains("\"140\""), "{failure:?}");
        assert!(eval_field(&body, "uid", None, None, None, None, Some("a::b::1"), None).is_ok());

        // `not_equals` is the server-set predicate: equal to the client's own
        // value is the failure, anything else passes.
        assert!(
            eval_field(
                &body,
                "magnitude",
                None,
                Some(&json!(1)),
                None,
                None,
                None,
                None
            )
            .is_ok()
        );
        let failure = eval_field(
            &body,
            "magnitude",
            None,
            Some(&json!(140)),
            None,
            None,
            None,
            None,
        )
        .expect_err("the value is the client-supplied one");
        assert!(failure.0.contains("must be server-set"), "{failure:?}");
    }

    /// The aggregate `unique` assertion (law e) ignores rows that never bound
    /// the capture: a row excused before it committed anything must not count
    /// as a duplicate of another excused row.
    #[test]
    fn unique_ignores_rows_that_bound_nothing() {
        let name = crate::ids::CaptureName::parse("ehr_id").unwrap();
        let mut bound = VarStore::default();
        bound.set(name.clone(), Captured::Scalar("e-1".to_owned()));
        let mut same = VarStore::default();
        same.set(name.clone(), Captured::Scalar("e-1".to_owned()));

        assert!(
            eval_unique(&name, &[VarStore::default(), VarStore::default()]).is_ok(),
            "two rows that bound nothing are not two duplicates"
        );
        assert!(eval_unique(&name, &[bound.clone(), VarStore::default()]).is_ok());
        let failure = eval_unique(&name, &[bound, VarStore::default(), same])
            .expect_err("the same id at two rows is a duplicate");
        assert!(failure.0.contains("repeats at row 2"), "{failure:?}");
    }

    /// A `returns` predicate reads a non-string body as its JSON text, so a
    /// numeric or object response is judged rather than passing untested.
    #[test]
    fn returns_predicates_read_a_non_string_body_as_its_json() {
        let count = json!(7);
        assert!(eval_returns(&count, None, Some(r"^\d$"), None).is_ok());
        let failure =
            eval_returns(&count, None, Some("^x"), None).expect_err("7 does not start with x");
        assert!(failure.0.contains("does not match"), "{failure:?}");
        assert!(eval_returns(&count, None, None, Some("nowhere")).is_ok());
        assert!(eval_returns(&count, None, None, Some("7")).is_err());

        // An uncompilable pattern is a failure on both predicate channels.
        assert!(eval_returns(&count, None, Some("("), None).is_err());
    }

    /// A namespace-qualified `xsi:type` `QName` is judged against the SAME
    /// expectation as the root, so a type from another schema's target
    /// namespace is refused even when its local name matches.
    #[test]
    fn an_xsi_type_in_the_wrong_namespace_is_refused() {
        use crate::vocab::XmlNamespace;

        // The root binds the published namespace with a PREFIX and leaves no
        // default, so the unprefixed xsi:type QName resolves to NO namespace.
        let unqualified_type = Value::String(
            r#"<oe:version xmlns:oe="http://schemas.openehr.org/v1"
                           xmlns:xsi="http://www.w3.org/2001/XMLSchema-instance"
                           xsi:type="ORIGINAL_VERSION"/>"#
                .to_owned(),
        );
        let failure = eval_xml_root(
            &unqualified_type,
            "version",
            Some(XmlNamespace::Published),
            Some("ORIGINAL_VERSION"),
        )
        .expect_err("the type QName resolves to no namespace");
        assert!(
            failure.0.contains("resolves to NO namespace"),
            "{failure:?}"
        );
        // The same document passes when only the root's namespace is asserted.
        assert!(
            eval_xml_root(
                &unqualified_type,
                "version",
                Some(XmlNamespace::Published),
                None
            )
            .is_ok()
        );

        // A type QName bound to another namespace names another schema's type.
        let foreign_type = Value::String(
            r#"<version xmlns="http://schemas.openehr.org/v1"
                        xmlns:other="http://example.invalid/other"
                        xmlns:xsi="http://www.w3.org/2001/XMLSchema-instance"
                        xsi:type="other:ORIGINAL_VERSION"/>"#
                .to_owned(),
        );
        let failure = eval_xml_root(
            &foreign_type,
            "version",
            Some(XmlNamespace::Published),
            Some("ORIGINAL_VERSION"),
        )
        .expect_err("the type is another schema's");
        assert!(failure.0.contains("is in namespace"), "{failure:?}");

        // A prefix the document never bound is refused on both QNames.
        let unbound_type_prefix = Value::String(
            r#"<version xmlns="http://schemas.openehr.org/v1"
                        xmlns:xsi="http://www.w3.org/2001/XMLSchema-instance"
                        xsi:type="zz:ORIGINAL_VERSION"/>"#
                .to_owned(),
        );
        let failure = eval_xml_root(&unbound_type_prefix, "version", None, Some("x"))
            .expect_err("the xsi:type prefix is unbound");
        assert!(
            failure.0.contains("is not bound to any namespace"),
            "{failure:?}"
        );

        let unbound_root_prefix = Value::String("<zz:version/>".to_owned());
        let failure = eval_xml_root(&unbound_root_prefix, "version", None, None)
            .expect_err("the root's prefix is unbound");
        assert!(
            failure.0.contains("is not bound to any namespace"),
            "{failure:?}"
        );
    }

    /// The wire-dependent set is exactly the three assertions whose facts a
    /// pure evaluator cannot produce: the pure evaluators here handle the
    /// rest, so a new assertion silently joining the driver-only set would
    /// stop being evaluated at all.
    #[test]
    fn only_the_driver_read_assertions_are_wire_dependent() {
        let assertion = |document: Value| -> Assertion {
            serde_json::from_value(document).expect("an authored assertion")
        };

        for wire in [
            json!({ "assert": "version", "of": "${uid}", "lifecycle_state": "532" }),
            json!({ "assert": "signature", "of": "${uid}", "present": true }),
            json!({ "assert": "instance_of", "rm_type": "COMPOSITION" }),
        ] {
            assert!(is_wire_dependent(&assertion(wire.clone())), "{wire}");
        }
        for pure in [
            json!({ "assert": "field", "path": "uid/value", "exists": true }),
            json!({ "assert": "unique", "over": "${ehr_id}", "aggregate": true }),
            json!({ "assert": "returns", "equals": 1 }),
        ] {
            assert!(!is_wire_dependent(&assertion(pure.clone())), "{pure}");
        }
    }
}
