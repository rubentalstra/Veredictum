// SPDX-FileCopyrightText: Veredictum contributors
// SPDX-License-Identifier: Apache-2.0

//! The `signature` assertion family against the fake SUT.
//!
//! RM common `master06-change_control_package.adoc` §Digital Signature makes
//! the signature a function of the version's own canonical content, so every
//! fact the family declares is judged against the served `ORIGINAL_VERSION`
//! envelope the case's own flow reads back. `present`, `equals` and
//! `distinct_from` are mode-agnostic; `verifiable` dispatches on the signing
//! posture of the INSTANCE the step ran on, which is a deployment fact the
//! ixit declares rather than anything the wire discloses.
//!
//! What these tests pin is that each fact BITES: a server serving no
//! signature, a stored value that is not the one supplied, two versions
//! carrying the same signature, and a signature that does not recompute all
//! fail the row and name what was served.

use base64::Engine as _;
use serde_json::{Value, json};
use sha2::{Digest as _, Sha256};
use veredictum::exec::driver::HttpDriver;
use veredictum::exec::outcome::Observation;
use veredictum::exec::signature::canonical_form;
use veredictum::exec::state::{Captured, VarStore};
use veredictum::exec::{StepDriver, StepObservation};
use veredictum::ids::CaptureName;
use veredictum::ixit::Ixit;
use veredictum::vocab::OutcomeKind;
use wiremock::matchers::{method, path};
use wiremock::{Mock, ResponseTemplate};

use crate::fake_sut::{FakeSut, artifact_set, case};

/// Anything a driver construction or a step can fail with, so a test body
/// propagates plumbing failures with `?`
/// (<https://doc.rust-lang.org/book/ch11-01-writing-tests.html>).
type Fallible = Result<(), Box<dyn std::error::Error>>;

/// The version uid the served envelope identifies itself by.
const UID: &str = "8849182c-82ad-4088-a07f-48ead4180515::sut.example::1";

/// The version-envelope read the signature family is judged against
/// (RM `VERSIONED_OBJECT.version_with_id`, realized by ITS-REST as the
/// `version` variant of the versioned-composition read).
fn envelope_binding() -> Value {
    json!({
        "sm_operation": "I_EHR_COMPOSITION.get_versioned_composition",
        "its": "its-rest",
        "variant": "version",
        "request": { "method": "GET", "path": "/version" },
        "outcomes": { "ok": { "status": 200 } }
    })
}

/// A one-step case whose read-back step declares the given signature facts.
fn envelope_case(facts: &Value) -> Value {
    json!({
        "id": "WIRE-signature", "kind": "functional", "component": "SECURITY",
        "sm_operation": "I_EHR_COMPOSITION.get_versioned_composition",
        "test_purpose": "t", "description": "d", "spec_refs": [],
        "flow": [{
            "step": 1, "call": "get_versioned_composition", "variant": "version",
            "expect": "ok",
            "assert": [facts.clone()]
        }]
    })
}

/// The served `ORIGINAL_VERSION` envelope, with or without a signature.
fn envelope(signature: Option<&str>) -> Value {
    let mut served = json!({
        "_type": "ORIGINAL_VERSION",
        "uid": { "value": UID },
        "lifecycle_state": { "value": "complete" },
        "data": { "_type": "COMPOSITION", "name": { "value": "signed" } }
    });
    if let Some(signature) = signature
        && let Some(map) = served.as_object_mut()
    {
        map.insert("signature".to_owned(), Value::String(signature.to_owned()));
    }
    served
}

/// The digest a `digest`-mode SUT would write back: base64 of the SHA-256 of
/// the agreed canonical form (RFC 8785 JCS of the version minus `signature`).
fn digest_of(served: &Value) -> Result<String, Box<dyn std::error::Error>> {
    let canonical = canonical_form(served)?;
    Ok(base64::engine::general_purpose::STANDARD.encode(Sha256::digest(canonical.as_bytes())))
}

/// The topology, with the party's declared signing posture when it declares
/// one. A posture is a deployment fact, so an absent block is what a party
/// running no signing looks like.
fn topology(base_url: &str, signing: Option<Value>) -> Result<Ixit, serde_json::Error> {
    let mut document = json!({
        "instances": { "sut": { "base_url": base_url, "auth": { "mode": "none" } } }
    });
    if let Some(signing) = signing
        && let Some(map) = document.as_object_mut()
    {
        map.insert("signing".to_owned(), signing);
    }
    serde_json::from_value(document)
}

/// Serve `served` from the envelope read and judge the case's declared
/// signature facts against it.
fn judge(
    served: &Value,
    facts: &Value,
    signing: Option<Value>,
    vars: &mut VarStore,
) -> Result<StepObservation, Box<dyn std::error::Error>> {
    let sut = FakeSut::start();
    sut.mount(
        Mock::given(method("GET"))
            .and(path("/version"))
            .respond_with(ResponseTemplate::new(200).set_body_json(served.clone())),
    );
    let set = artifact_set(&[envelope_binding()]);
    let ixit = topology(&sut.base_url(), signing)?;
    let core = case(envelope_case(facts));
    let step = core.flow.first().ok_or("the case declares no flow step")?;
    let mut driver = HttpDriver::new(&set, &ixit, None)?;
    Ok(driver.perform(&core, step, OutcomeKind::Ok, 0, vars)?)
}

/// A store binding the two comparands the `equals` and `distinct_from` facts
/// resolve against.
fn bound(signature: &str, other: &str) -> Result<VarStore, Box<dyn std::error::Error>> {
    let mut vars = VarStore::default();
    vars.set(
        CaptureName::parse("version_uid")?,
        Captured::Scalar(UID.to_owned()),
    );
    vars.set(
        CaptureName::parse("supplied_signature")?,
        Captured::Scalar(signature.to_owned()),
    );
    vars.set(
        CaptureName::parse("other_signature")?,
        Captured::Scalar(other.to_owned()),
    );
    Ok(vars)
}

/// The one failure reason a judged step produced.
fn only_failure(observed: &StepObservation) -> Result<String, Box<dyn std::error::Error>> {
    let first = observed
        .assertion_failures
        .first()
        .ok_or("the step raised no assertion failure")?;
    Ok(first.reason().to_owned())
}

/// `present` is mode-agnostic and bites on both absences the wire can serve:
/// no `signature` member at all, and an empty one.
#[test]
#[expect(
    clippy::panic_in_result_fn,
    reason = "the Book's Result-test shape: assertions panic, plumbing propagates with `?`"
)]
fn a_present_signature_fact_bites_on_both_absences() -> Fallible {
    let facts = json!({ "assert": "signature", "of": "${version_uid}", "present": true });
    let mut vars = bound("sig", "other")?;
    let observed = judge(&envelope(Some("sig-value")), &facts, None, &mut vars)?;
    assert_eq!(observed.observation, Observation::Kind(OutcomeKind::Ok));
    assert!(
        observed.assertion_failures.is_empty(),
        "a served signature satisfies present: {:?}",
        observed.assertion_failures
    );

    for served in [envelope(None), envelope(Some(""))] {
        let mut vars = bound("sig", "other")?;
        let observed = judge(&served, &facts, None, &mut vars)?;
        assert!(
            only_failure(&observed)?.contains("expected present"),
            "an absent signature must fail the row: {served}"
        );
    }
    Ok(())
}

/// A client-supplied signature is stored VERBATIM: `equals` compares the
/// served member against the value the case supplied, and names both sides
/// when they differ.
#[test]
#[expect(
    clippy::panic_in_result_fn,
    reason = "the Book's Result-test shape: assertions panic, plumbing propagates with `?`"
)]
fn an_equals_fact_compares_the_stored_signature_against_the_supplied_one() -> Fallible {
    let facts = json!({
        "assert": "signature", "of": "${version_uid}", "equals": "${supplied_signature}"
    });
    let mut vars = bound("client-supplied", "other")?;
    let observed = judge(&envelope(Some("client-supplied")), &facts, None, &mut vars)?;
    assert!(
        observed.assertion_failures.is_empty(),
        "a verbatim store satisfies equals: {:?}",
        observed.assertion_failures
    );

    let mut vars = bound("client-supplied", "other")?;
    let observed = judge(
        &envelope(Some("server-rewrote-it")),
        &facts,
        None,
        &mut vars,
    )?;
    let reason = only_failure(&observed)?;
    assert!(
        reason.contains("client-supplied") && reason.contains("server-rewrote-it"),
        "the failure names neither side: {reason}"
    );

    // An unresolvable comparand is a defect of the CASE, and it is reported
    // as one rather than as a server that stored the wrong value.
    let unbound = json!({
        "assert": "signature", "of": "${version_uid}", "equals": "${never_captured}"
    });
    let mut vars = bound("client-supplied", "other")?;
    let observed = judge(
        &envelope(Some("client-supplied")),
        &unbound,
        None,
        &mut vars,
    )?;
    assert!(
        only_failure(&observed)?.contains("never_captured"),
        "the unresolvable comparand must be named"
    );
    Ok(())
}

/// `distinct_from` is what makes per-version signing observable: the canonical
/// form includes `uid`, so two versions cannot carry one signature. Both
/// degenerate comparisons — no served signature, and an empty comparand —
/// fail rather than pass vacuously.
#[test]
#[expect(
    clippy::panic_in_result_fn,
    reason = "the Book's Result-test shape: assertions panic, plumbing propagates with `?`"
)]
fn a_distinct_from_fact_refuses_a_repeated_or_a_vacuous_comparison() -> Fallible {
    let facts = json!({
        "assert": "signature", "of": "${version_uid}", "distinct_from": "${other_signature}"
    });
    let mut vars = bound("sig", "signature-of-version-1")?;
    let observed = judge(
        &envelope(Some("signature-of-version-2")),
        &facts,
        None,
        &mut vars,
    )?;
    assert!(
        observed.assertion_failures.is_empty(),
        "two distinct signatures satisfy the fact: {:?}",
        observed.assertion_failures
    );

    let mut vars = bound("sig", "signature-of-version-1")?;
    let observed = judge(
        &envelope(Some("signature-of-version-1")),
        &facts,
        None,
        &mut vars,
    )?;
    assert!(
        only_failure(&observed)?.contains("identical"),
        "one signature over two versions must fail the row"
    );

    let mut vars = bound("sig", "signature-of-version-1")?;
    let observed = judge(&envelope(None), &facts, None, &mut vars)?;
    assert!(
        only_failure(&observed)?.contains("carries no signature"),
        "an absent signature satisfies nothing"
    );

    // An empty comparand means the earlier capture failed — asserting
    // "distinct from nothing" would pass every server.
    let mut vars = bound("sig", "")?;
    let observed = judge(
        &envelope(Some("signature-of-version-2")),
        &facts,
        None,
        &mut vars,
    )?;
    assert!(
        only_failure(&observed)?.contains("comparand is empty"),
        "an empty comparand must be loud"
    );
    Ok(())
}

/// `verifiable` recomputes the agreed canonical form under the posture the
/// ixit declares for the instance the step ran on. A signature that does not
/// recompute is a conformance finding; a posture nobody declared, or a
/// signature that is not there at all, is a fact the run cannot judge and
/// says so.
#[test]
#[expect(
    clippy::panic_in_result_fn,
    reason = "the Book's Result-test shape: assertions panic, plumbing propagates with `?`"
)]
fn a_verifiable_fact_recomputes_the_canonical_form_under_the_declared_posture() -> Fallible {
    let facts = json!({ "assert": "signature", "of": "${version_uid}", "verifiable": true });
    let digest = json!({ "mode": "digest", "algorithm": "sha256", "encoding": "base64" });
    let signed = digest_of(&envelope(None))?;

    let mut vars = bound("sig", "other")?;
    let observed = judge(
        &envelope(Some(&signed)),
        &facts,
        Some(digest.clone()),
        &mut vars,
    )?;
    assert!(
        observed.assertion_failures.is_empty(),
        "the recomputed digest verifies: {:?}",
        observed.assertion_failures
    );

    let mut vars = bound("sig", "other")?;
    let observed = judge(
        &envelope(Some("not-the-digest")),
        &facts,
        Some(digest.clone()),
        &mut vars,
    )?;
    assert!(
        only_failure(&observed)?.contains("does not verify"),
        "a signature over other bytes must fail the row"
    );

    let mut vars = bound("sig", "other")?;
    let observed = judge(&envelope(None), &facts, Some(digest), &mut vars)?;
    assert!(
        only_failure(&observed)?.contains("carries no signature"),
        "nothing served is nothing to verify"
    );

    // No declared posture: the run has no key material to verify against,
    // which is a boundary of the topology rather than a server defect.
    let mut vars = bound("sig", "other")?;
    let observed = judge(&envelope(Some(&signed)), &facts, None, &mut vars)?;
    assert!(
        only_failure(&observed)?.contains("no `signing` posture"),
        "an undeclared posture must be named"
    );

    // A posture naming an algorithm this instrument does not implement is an
    // instrument-side error, reported as the verification it could not run.
    let unknown = json!({ "mode": "digest", "algorithm": "sha512", "encoding": "base64" });
    let mut vars = bound("sig", "other")?;
    let observed = judge(&envelope(Some(&signed)), &facts, Some(unknown), &mut vars)?;
    let reason = only_failure(&observed)?;
    assert!(
        reason.contains("signature verify:") && reason.contains("sha512"),
        "{reason}"
    );
    Ok(())
}
