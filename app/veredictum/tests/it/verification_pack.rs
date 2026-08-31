// SPDX-FileCopyrightText: Veredictum contributors
// SPDX-License-Identifier: Apache-2.0

//! The runner-verification pack, part 1 (verdict conformance): replaying
//! the committed transcript reproduces every adjudicated verdict — and a
//! deliberately-broken runner (tampered adjudications) is REJECTED, which
//! is the pack's own falsifiability requirement.

#![expect(
    clippy::expect_used,
    reason = "test-support helpers (not `#[test]` fns, so the clippy.toml in-tests scoping does not reach them) are panic-idiomatic: a broken fixture must abort the test loudly, Book ch11"
)]

use veredictum::artifacts::load_root;
use veredictum::exec::RowOutcome;
use veredictum::exec::player::{ExpectedVerdict, Transcript, replay_entry, verdict_matches};

fn load() -> (veredictum::artifacts::ArtifactSet, Transcript) {
    let crate_dir = std::path::Path::new(concat!(env!("CARGO_MANIFEST_DIR"), "/../.."));
    let loaded = load_root(&crate_dir.join("artifacts")).expect("schema compilation");
    assert!(loaded.errors.is_empty(), "artifact tree must load cleanly");
    let text = std::fs::read_to_string(crate_dir.join("verification-pack/transcript.json"))
        .expect("committed transcript");
    let transcript: Transcript = serde_json::from_str(&text).expect("transcript parses");
    (loaded.set, transcript)
}

#[test]
fn every_adjudicated_verdict_is_reproduced() {
    let (set, transcript) = load();
    assert!(!transcript.entries.is_empty());
    for entry in &transcript.entries {
        let (expected, produced) =
            replay_entry(&set, entry).unwrap_or_else(|e| panic!("{}: {e}", entry.case));
        assert!(
            verdict_matches(expected, &produced),
            "{} row {}: adjudicated {:?}, runner produced {:?} ({})",
            entry.case,
            entry.row,
            expected,
            produced,
            entry.adjudication_ref
        );
    }
}

#[test]
fn the_pack_rejects_a_broken_runner() {
    // Tampering with the adjudication (the moral equivalent of a runner
    // that mis-classifies) MUST be detected: flip the errored entry to
    // `passed` and require the reproduction check to fail.
    let (set, mut transcript) = load();
    let entry = transcript
        .entries
        .iter_mut()
        .find(|e| matches!(e.expected_verdict, ExpectedVerdict::Errored))
        .expect("an errored adjudication exists");
    entry.expected_verdict = ExpectedVerdict::Passed;
    let (expected, produced) = replay_entry(&set, entry).expect("replay");
    assert!(
        !verdict_matches(expected, &produced),
        "a mis-adjudicated verdict must NOT be reproduced silently"
    );
}

/// A replay never claims a verdict over an assertion it did not evaluate
/// (#239). The postcondition seam refuses every judged family by role: the
/// player runs the postconditions after the flow, with no exchange of their
/// own to read, so an empty answer would have reproduced the adjudicated
/// verdict while silently skipping the assertion.
#[test]
fn a_pack_entry_carrying_a_judged_postcondition_is_refused() {
    let (mut set, transcript) = load();
    let entry = transcript
        .entries
        .first()
        .expect("the pack carries entries");
    let case = set
        .cases
        .iter_mut()
        .map(|(_, c)| c)
        .find(|c| c.id == entry.case)
        .expect("the entry's case is in the catalogue");
    let judged: veredictum::model::assertion::Assertion = serde_json::from_value(
        serde_json::json!({ "assert": "field", "path": "is_queryable", "exists": true }),
    )
    .expect("a field assertion parses");
    case.postconditions.push(judged);

    let refusal = replay_entry(&set, entry).expect_err("the replay must refuse, never answer");
    assert!(
        refusal.contains("field") && refusal.contains(entry.case.as_str()),
        "the refusal names neither the family nor the case: {refusal}"
    );
}

/// The families a replay does not have to judge leave it running: `unique`
/// is aggregate (law e, evaluated over the collected rows) and `state` is
/// informative, so the committed pack — whose passing case carries a `state`
/// postcondition — still reproduces its verdicts above.
#[test]
fn the_aggregate_and_informative_families_do_not_block_a_replay() {
    let (mut set, transcript) = load();
    let entry = transcript
        .entries
        .first()
        .expect("the pack carries entries");
    let case = set
        .cases
        .iter_mut()
        .map(|(_, c)| c)
        .find(|c| c.id == entry.case)
        .expect("the entry's case is in the catalogue");
    let informative: veredictum::model::assertion::Assertion = serde_json::from_value(
        serde_json::json!({ "assert": "message_exemplar", "text": "recorded for readers" }),
    )
    .expect("a message_exemplar assertion parses");
    case.postconditions.push(informative);

    let (expected, produced) = replay_entry(&set, entry).expect("replay");
    assert!(verdict_matches(expected, &produced));
}

/// A recorded exchange that CONTRADICTS its own step assertion must stop
/// reproducing a passing verdict (#255). The pack's passing case asserts
/// `instance_of: EHR` on its step-3 read-back, so a doctored transcript
/// serving another RM type there has to fail the row at that step. The
/// committed pack is untouched: the doctoring happens on the parsed copy.
#[test]
fn a_recorded_exchange_contradicting_its_step_assertion_stops_passing() {
    let (set, mut transcript) = load();
    let entry = transcript
        .entries
        .iter_mut()
        .find(|e| matches!(e.expected_verdict, ExpectedVerdict::Passed))
        .expect("a passing adjudication exists");
    let read_back = entry
        .steps
        .iter_mut()
        .find(|s| s.step == 3)
        .expect("the passing entry records the step-3 read-back");
    read_back.response.body = Some(serde_json::json!({
        "_type": "FOLDER",
        "ehr_id": { "value": "7d44b88c-4199-4bad-97dc-d78268e01391" }
    }));

    let (expected, produced) = replay_entry(&set, entry).expect("replay");
    assert!(
        matches!(produced, RowOutcome::Failed { step: 3, .. }),
        "the contradicted assertion must fail the row at its own step, produced {produced:?}"
    );
    assert!(
        !verdict_matches(expected, &produced),
        "a step assertion the exchange contradicts must not reproduce `passed`"
    );
}

/// The step seam refuses what it cannot judge, exactly as the postcondition
/// seam does: a `version` assertion is judged against a versioned-object read
/// the transcript never recorded, so an entry carrying one on a step whose
/// observation met its expectation is REFUSED by name.
#[test]
fn a_step_assertion_the_replay_cannot_judge_refuses_the_entry() {
    let (mut set, transcript) = load();
    let entry = transcript
        .entries
        .iter()
        .find(|e| matches!(e.expected_verdict, ExpectedVerdict::Passed))
        .expect("a passing adjudication exists");
    let case = set
        .cases
        .iter_mut()
        .map(|(_, c)| c)
        .find(|c| c.id == entry.case)
        .expect("the entry's case is in the catalogue");
    let unrecorded: veredictum::model::assertion::Assertion =
        serde_json::from_value(serde_json::json!({ "assert": "version", "count": 1 }))
            .expect("a version assertion parses");
    case.flow
        .iter_mut()
        .find(|s| s.step == 3)
        .expect("the case carries a step 3")
        .assertions
        .push(unrecorded);

    let refusal = replay_entry(&set, entry).expect_err("the replay must refuse, never answer");
    assert!(
        refusal.contains("version") && refusal.contains(entry.case.as_str()),
        "the refusal names neither the family nor the case: {refusal}"
    );
}

/// A pack case that READS a provisioned `requires` handle is refused: the
/// transcript records the flow's own exchanges and no provisioned handles,
/// so any value the replay bound would be one the recorded exchanges never
/// used.
#[test]
fn a_pack_case_reading_a_provisioned_handle_is_refused() {
    let (mut set, transcript) = load();
    let entry = transcript
        .entries
        .first()
        .expect("the pack carries entries");
    let case = set
        .cases
        .iter_mut()
        .map(|(_, c)| c)
        .find(|c| c.id == entry.case)
        .expect("the entry's case is in the catalogue");
    case.requires.ehr = Some(veredictum::model::case::EhrRequirement::Exists {
        commits: veredictum::model::case::CommitState::None,
    });
    let reads_handle: veredictum::model::assertion::Assertion = serde_json::from_value(
        serde_json::json!({ "assert": "field", "path": "ehr_id/value", "equals": "${ehr_id}" }),
    )
    .expect("a field assertion parses");
    case.flow
        .first_mut()
        .expect("the case carries a flow")
        .assertions
        .push(reads_handle);

    let refusal = replay_entry(&set, entry).expect_err("the replay must refuse, never answer");
    assert!(
        refusal.contains("ehr_id") && refusal.contains(entry.case.as_str()),
        "the refusal names neither the handle nor the case: {refusal}"
    );
}

/// The pack's `equivalent` entry JUDGES the document it records (#469): an
/// ADL2 source served with one changed term stops reproducing `passed`.
///
/// Before this the family was classified as carrying no recorded ground at
/// all, so the pack could hold no entry over it and proved nothing about the
/// comparator the live driver uses. The committed pack is untouched: the
/// doctoring happens on the parsed copy.
#[test]
fn a_recorded_document_diverging_from_its_fixture_stops_passing() {
    let (set, mut transcript) = load();
    let entry = transcript
        .entries
        .iter_mut()
        .find(|e| e.case.as_str() == "I_DEFINITION_ADL2.get_artefact-retrieve")
        .expect("the pack carries the ADL2 retrieval entry");
    let served = entry
        .steps
        .first_mut()
        .expect("the entry records its one step");
    let document = match served.response.body.take() {
        Some(serde_json::Value::String(text)) => text,
        other => panic!("the recorded ADL2 body is document text, got {other:?}"),
    };
    served.response.body = Some(serde_json::Value::String(
        document.replace("CNF minimal encounter", "something else entirely"),
    ));

    let (expected, produced) = replay_entry(&set, entry).expect("replay");
    assert!(
        matches!(produced, RowOutcome::Failed { step: 1, .. }),
        "a served document that differs from the fixture must fail the row at its own step, produced {produced:?}"
    );
    assert!(
        !verdict_matches(expected, &produced),
        "a diverging document must not reproduce `passed`"
    );
}

/// A binding's header matchers are EXECUTED expectations, and the pack player
/// ran none of them before #473: an entry could reproduce `passed` over a
/// recording whose served media type contradicted the type its own request
/// asked for.
#[test]
fn a_served_type_contradicting_the_recorded_ask_stops_passing() {
    let (set, mut transcript) = load();
    let entry = transcript
        .entries
        .iter_mut()
        .find(|e| e.case.as_str() == "I_DEFINITION_ADL2.get_artefact-retrieve")
        .expect("the pack carries the ADL2 retrieval entry");
    let served = entry
        .steps
        .first_mut()
        .expect("the entry records its one step");
    assert_eq!(
        served.request.accept.as_deref(),
        Some("text/plain"),
        "the recorded ask is what makes the negotiated matcher judgeable"
    );
    let name = served
        .response
        .headers
        .keys()
        .find(|k| k.eq_ignore_ascii_case("content-type"))
        .cloned()
        .expect("the recording serves a content type");
    let _replaced = served
        .response
        .headers
        .insert(name, "application/json".to_owned());

    let (expected, produced) = replay_entry(&set, entry).expect("replay");
    assert!(
        matches!(produced, RowOutcome::Failed { step: 1, .. }),
        "a served type contradicting the ask must fail the row at its own step, produced {produced:?}"
    );
    assert!(
        !verdict_matches(expected, &produced),
        "a violated header matcher must not reproduce `passed`"
    );
}

/// A pack entry whose expectation declares a `negotiated` matcher and whose
/// recording carries no ask is REFUSED, never passed: the evaluator answers "no
/// failure" for an absent ask, so evaluating anyway would let a wrong media
/// type through (#473).
#[test]
fn an_entry_declaring_a_negotiated_matcher_without_an_ask_is_refused() {
    let (set, mut transcript) = load();
    let entry = transcript
        .entries
        .iter_mut()
        .find(|e| e.case.as_str() == "I_DEFINITION_ADL2.get_artefact-retrieve")
        .expect("the pack carries the ADL2 retrieval entry");
    entry
        .steps
        .first_mut()
        .expect("the entry records its one step")
        .request
        .accept = None;

    let refusal = replay_entry(&set, entry).expect_err("the entry must be refused");
    assert!(
        refusal.contains("negotiated matcher") && refusal.contains("Content-Type"),
        "the refusal must name the matcher and the header: {refusal}"
    );
}

/// A `signature` assertion whose only fact is `present` is judged from the
/// recorded envelope (#469): the member is in the recording, so the replay
/// reads it rather than refusing the entry.
#[test]
fn a_present_only_signature_is_judged_from_the_recorded_envelope() {
    let (mut set, transcript) = load();
    let entry = transcript
        .entries
        .iter()
        .find(|e| matches!(e.expected_verdict, ExpectedVerdict::Passed))
        .expect("a passing adjudication exists");
    let case = set
        .cases
        .iter_mut()
        .map(|(_, c)| c)
        .find(|c| c.id == entry.case)
        .expect("the entry's case is in the catalogue");
    let present: veredictum::model::assertion::Assertion = serde_json::from_value(
        serde_json::json!({ "assert": "signature", "of": "${v1}", "present": true }),
    )
    .expect("a signature assertion parses");
    case.flow
        .iter_mut()
        .find(|s| s.step == 3)
        .expect("the case carries a step 3")
        .assertions
        .push(present);

    // The step-3 read-back carries no `signature`, so the fact is JUDGED and
    // found absent — the row fails, which is the proof it was evaluated.
    let (_, produced) = replay_entry(&set, entry).expect("the replay must judge, never refuse");
    assert!(
        matches!(produced, RowOutcome::Failed { step: 3, .. }),
        "a present-only signature must be judged off the recorded envelope, produced {produced:?}"
    );
}

#[test]
fn transcript_verdict_vocabulary_is_exercised() {
    let (_, transcript) = load();
    let has = |v: ExpectedVerdict| {
        transcript
            .entries
            .iter()
            .any(|e| std::mem::discriminant(&e.expected_verdict) == std::mem::discriminant(&v))
    };
    assert!(
        has(ExpectedVerdict::Passed),
        "pack must exercise a passing row"
    );
    assert!(
        has(ExpectedVerdict::Failed),
        "pack must exercise a failing row"
    );
    assert!(
        has(ExpectedVerdict::Errored),
        "pack must exercise an inconclusive row"
    );
}
