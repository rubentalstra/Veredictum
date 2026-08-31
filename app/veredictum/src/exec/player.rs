// SPDX-FileCopyrightText: Veredictum contributors
// SPDX-License-Identifier: Apache-2.0

//! The transcript player — the runner-verification pack's part 1: replay a
//! fixed transcript and reproduce the adjudicated verdicts.
//!
//! The transcript is itself a specified artifact (`transcript.schema.json`):
//! an ordered sequence per case × format × row of recorded exchanges with
//! adjudicated expected verdicts; the player answers the Nth matching request
//! with the Nth recorded response (matching = method + path suffix), so a
//! fixture file fully determines what any conformant runner must conclude.
//!
//! A replay judges every assertion a recorded exchange plus the catalogue's
//! own corpus decides ([`crate::exec::assertions::eval_from_exchange`]) and
//! REFUSES the entry for any family it cannot, on both assertion seams: no
//! verdict is ever reproduced over an assertion nobody evaluated.

#![expect(
    clippy::disallowed_types,
    reason = "dev/verification tooling over JSON artifacts (the catalogue, results, wire \
              exchanges), whose shapes belong to the artifacts and the SUT"
)]

use std::collections::BTreeMap;

use reqwest::StatusCode;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::exec::assertions::{CorpusGround, ExchangeFacts, ReplayJudgement, replay_judgement};
use crate::exec::outcome::StepJudgement;
use crate::exec::resolve::Resolver;
use crate::exec::state::{Captured, VarStore};
use crate::exec::{StepDriver, StepObservation, outcome};
use crate::ids::{CaptureName, CaseId, CorpusKey, SmOperationRef, ViewName};
use crate::model::assertion::{Assertion, PostconditionRole};
use crate::model::binding::{HeaderMatcher, WireExpectation, WireFrom};
use crate::model::case::{CaseCore, FlowStep};
use crate::vocab::{FormatName, OutcomeKind};

/// One recorded response in a transcript entry.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RecordedResponse {
    /// The HTTP status code the recorded server answered with.
    pub status: u16,
    /// The recorded response headers, lower-cased names.
    pub headers: BTreeMap<String, String>,
    /// The recorded response body, when it carried one.
    #[serde(default)]
    pub body: Option<Value>,
}

/// One recorded request key (matching = method + path suffix + step).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RecordedRequest {
    /// The HTTP method of the recorded request.
    pub method: String,
    /// The request path as recorded (matching is by suffix).
    pub path: String,
    /// Digest of the request body, so a replay can tell two shapes apart.
    #[serde(default)]
    pub body_digest: Option<String>,
    /// The `Accept` the step negotiated, when a binding's header expectation
    /// needs it.
    ///
    /// A `negotiated` matcher compares the served `Content-Type` against what
    /// the request asked for, and a recording that omits the ask cannot judge
    /// it. Optional so an entry whose expectation declares no such matcher
    /// carries nothing it does not need; an entry that DOES declare one and
    /// omits this is refused rather than passed, by the player's own
    /// ungrounded-header guard. Named in prose rather than linked: the guard is
    /// private and a public item may not link to it.
    #[serde(default)]
    pub accept: Option<String>,
}

/// The adjudicated expectation for one step exchange.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TranscriptStep {
    /// The flow step this exchange belongs to.
    pub step: u32,
    /// The request the step is expected to issue.
    pub request: RecordedRequest,
    /// The response the player answers it with.
    pub response: RecordedResponse,
}

/// One case×format×row sequence with its adjudicated verdict.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TranscriptEntry {
    /// The case this sequence belongs to.
    pub case: CaseId,
    /// The format axis the sequence was recorded on, when parameterized.
    #[serde(default)]
    pub format: Option<FormatName>,
    /// The 0-based parameter row the sequence belongs to.
    pub row: usize,
    /// The recorded exchanges, in step order.
    pub steps: Vec<TranscriptStep>,
    /// The adjudicated per-row verdict the runner MUST reproduce.
    pub expected_verdict: ExpectedVerdict,
    /// The adjudication citation (spec text, register entry).
    pub adjudication_ref: String,
}

/// The adjudicated verdict vocabulary of the pack.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ExpectedVerdict {
    /// Every assertion of the row must hold.
    Passed,
    /// The row must fail an assertion (ISO/IEC 9646 fail).
    Failed,
    /// The row must be inconclusive — the exchange itself broke.
    Errored,
    /// The row must be excluded by selection, with a citation.
    NotApplicable,
    /// The row must be skipped, with a citation.
    Skipped,
}

/// A whole transcript document.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Transcript {
    /// The schedule release the adjudications were made against.
    pub schedule_release: String,
    /// Every recorded sequence of the document.
    pub entries: Vec<TranscriptEntry>,
}

/// The catalogue's own corpus, as the replay's [`CorpusGround`].
///
/// A replay is driven over the same artifact root the live run drove, so a
/// `${ds:…}` comparand is read off disk through the run's own resolver. The
/// wrapper exists because [`CorpusGround`] is the narrow contract the pure
/// evaluators see: the corpus, and no row state, no provisioning, no ixit.
struct PlayerCorpus<'a> {
    resolver: Resolver<'a>,
    manifest: &'a crate::model::corpus::CorpusManifest,
}

impl CorpusGround for PlayerCorpus<'_> {
    fn data_set(&mut self, key: &CorpusKey, view: Option<&ViewName>) -> Result<Value, String> {
        match view {
            None => self.resolver.data_set(key).map_err(|e| e.to_string()),
            Some(view) => self.resolver.view(key, view).map_err(|e| e.to_string()),
        }
    }

    fn corpus_format(&self, key: &CorpusKey) -> Option<crate::vocab::CorpusFormat> {
        self.manifest.get(key).map(|entry| entry.format)
    }
}

/// The player: a [`StepDriver`] answering from recorded exchanges.
#[derive(Debug)]
pub struct TranscriptPlayer<'a> {
    set: &'a crate::artifacts::ArtifactSet,
    entry: &'a TranscriptEntry,
    cursor: usize,
}

impl<'a> TranscriptPlayer<'a> {
    /// A player over one transcript entry.
    #[must_use]
    pub fn new(set: &'a crate::artifacts::ArtifactSet, entry: &'a TranscriptEntry) -> Self {
        Self {
            set,
            entry,
            cursor: 0,
        }
    }

    /// The corpus ground for this replay, or [`None`] when the artifact set
    /// carries no corpus at all.
    fn corpus(&self) -> Option<PlayerCorpus<'a>> {
        let (_, manifest) = self.set.corpus.as_ref()?;
        let corpus_dir = self.set.corpus_dir.as_deref()?;
        Some(PlayerCorpus {
            resolver: Resolver::new(manifest, corpus_dir, None),
            manifest,
        })
    }

    /// Selects the operation binding under the same variant discipline as the
    /// live driver (`HttpDriver::binding_for_variant`): a step's `variant`
    /// selects the binding declaring it, and a variant-less step — or a variant
    /// with no dedicated binding — resolves the variant-less binding.
    fn binding_for(
        &self,
        case: &CaseCore,
        call: &str,
        variant: Option<&str>,
    ) -> Option<&'a crate::model::binding::OperationBinding> {
        let op = if call.contains('.') {
            SmOperationRef::parse(call).ok()?
        } else {
            case.sm_operation.as_ref()?.sibling(call)
        };
        let mut bindings = self.set.bindings.iter().map(|(_, b)| b);
        if let Some(v) = variant
            && let Some(exact) = bindings
                .clone()
                .find(|b| b.sm_operation == op && b.variant.as_deref() == Some(v))
        {
            return Some(exact);
        }
        bindings.find(|b| b.sm_operation == op && b.variant.is_none())
    }

    /// Refuses the entry when the outcome's expectation declares a header
    /// matcher this recording cannot ground.
    ///
    /// `negotiated` compares the served `Content-Type` against the `Accept` the
    /// request carried. A recording that omits the ask makes that comparison
    /// unsound, and the evaluator answers "no failure" for an absent ask, so
    /// evaluating anyway would let a wrong media type reproduce a pass. The
    /// entry is refused by name instead, the same discipline
    /// [`Self::refuse_unrecorded`] applies to an assertion family.
    fn refuse_ungrounded_headers(
        case: &CaseCore,
        step: &FlowStep,
        expectation: &WireExpectation,
        recorded: &TranscriptStep,
    ) -> Result<(), String> {
        if recorded.request.accept.is_some() {
            return Ok(());
        }
        let ungrounded: Vec<&str> = expectation
            .headers
            .as_deref()
            .unwrap_or_default()
            .iter()
            .filter(|(_, declarations)| {
                declarations
                    .all()
                    .iter()
                    .any(|h| matches!(h.matcher, HeaderMatcher::Negotiated))
            })
            .map(|(name, _)| name.as_str())
            .collect();
        if ungrounded.is_empty() {
            return Ok(());
        }
        Err(format!(
            "case {} step {} declares a negotiated matcher on {}, and the recorded request \
             carries no `accept`; a pack entry may not claim a verdict over a header the replay \
             cannot judge",
            case.id,
            step.step,
            ungrounded.join(", ")
        ))
    }

    /// Refuses the entry when `step` asserts a family the replay cannot judge.
    ///
    /// A transcript records the response side of the flow's own exchanges and
    /// nothing else: no payload committed earlier in the row, no versioned
    /// read, no instance posture. A [`ReplayJudgement::Unrecorded`] assertion
    /// is unevaluable from that plus the catalogue's corpus, so the entry is
    /// refused by name rather than claiming a verdict over an assertion nobody
    /// ran.
    fn refuse_unrecorded(case: &CaseCore, step: &FlowStep) -> Result<(), String> {
        let unjudgeable: Vec<&str> = step
            .assertions
            .iter()
            .filter(|assertion| replay_judgement(assertion) == ReplayJudgement::Unrecorded)
            .map(Assertion::family)
            .collect();
        if unjudgeable.is_empty() {
            return Ok(());
        }
        Err(format!(
            "case {} step {} asserts families the transcript replay cannot judge ({}); a pack \
             entry may not claim a verdict over assertions the replay never evaluated",
            case.id,
            step.step,
            unjudgeable.join(", ")
        ))
    }

    /// Binds `step`'s captures from the recorded response, mirroring the live
    /// driver's closed capture grammar.
    ///
    /// Only the captures declared for the classified outcome `kind` bind. A
    /// `commit_time` capture answers from the cursor, since a transcript
    /// carries no clock.
    fn bind_recorded_captures(
        &self,
        step: &FlowStep,
        binding: &crate::model::binding::OperationBinding,
        recorded: &TranscriptStep,
        kind: OutcomeKind,
        vars: &mut VarStore,
    ) {
        for (name, source) in step.captures() {
            if source.outcome != kind {
                continue;
            }
            match &source.field {
                crate::refgrammar::CaptureField::Body => {
                    if let Some(b) = &recorded.response.body {
                        vars.set(name.clone(), Captured::Body(b.clone()));
                    }
                }
                crate::refgrammar::CaptureField::CommitTime => {
                    #[expect(
                        clippy::expect_used,
                        reason = "the cursor indexes the transcript's in-memory step vector, so \
                                  it is bounded orders of magnitude below i64::MAX / 1000; a \
                                  failed widening or multiplication is logically impossible and \
                                  should fail loud, never substitute an instant"
                    )]
                    let ms = i64::try_from(self.cursor)
                        .ok()
                        .and_then(|seconds| seconds.checked_mul(1_000))
                        .expect("the transcript cursor should fit an i64 millisecond instant");
                    vars.set(name.clone(), Captured::InstantMs { lo: ms, hi: ms });
                }
                crate::refgrammar::CaptureField::Field { name: field, list } => {
                    let Some(spec) = binding
                        .captures
                        .as_deref()
                        .unwrap_or_default()
                        .iter()
                        .find(|(n, _)| n == field)
                        .map(|(_, s)| s)
                    else {
                        continue;
                    };
                    if *list {
                        if let (Some(body), WireFrom::Body { path }) =
                            (&recorded.response.body, &spec.from)
                        {
                            let items = extract_list(body, path);
                            vars.set(name.clone(), Captured::List(items));
                        }
                    } else if let Some(value) = extract_scalar(&recorded.response, spec, vars) {
                        vars.set(name.clone(), Captured::Scalar(value));
                    }
                }
            }
        }
    }
}

/// Whether any flow `with`/`scope` template or any assertion of `case`
/// references `handle` as a capture.
fn case_reads_capture(case: &CaseCore, handle: &CaptureName) -> bool {
    let matches_handle = |reference: &crate::refgrammar::ValueRef| matches!(reference, crate::refgrammar::ValueRef::Capture { name, .. } if name == handle);
    case.flow.iter().any(|step| {
        step.with_entries()
            .iter()
            .any(|(_, value)| value.refs().iter().any(|r| matches_handle(r)))
            || step
                .scope_templates()
                .iter()
                .any(|template| template.refs().iter().any(|r| matches_handle(r)))
            || step.assertions.iter().any(|a| {
                crate::model::assertion::assertion_refs(a)
                    .iter()
                    .any(matches_handle)
            })
    }) || case.postconditions.iter().any(|a| {
        crate::model::assertion::assertion_refs(a)
            .iter()
            .any(matches_handle)
    })
}

impl StepDriver for TranscriptPlayer<'_> {
    fn perform(
        &mut self,
        case: &CaseCore,
        step: &FlowStep,
        expected: OutcomeKind,
        _row: usize,
        vars: &mut VarStore,
    ) -> Result<StepObservation, String> {
        let Some(recorded) = self.entry.steps.get(self.cursor) else {
            return Ok(StepObservation::transport(
                "transcript exhausted before the flow ended".to_owned(),
            ));
        };
        self.cursor += 1;

        let Some(binding) = self.binding_for(case, &step.call, step.variant.as_deref()) else {
            return Err(format!("no binding declares operation {}", step.call));
        };
        let selectors = self.set.selectors.as_ref().map(|(_, s)| s);
        let observation =
            outcome::classify_status(binding, selectors, recorded.response.status, expected);

        if let outcome::Observation::Kind(kind) = observation {
            self.bind_recorded_captures(step, binding, recorded, kind, vars);
        }
        // Law b aborts the row on a mismatch and `run_case` never reads the
        // assertion list then, so the replay must be able to judge the
        // assertions exactly when the observation met the expectation.
        if !matches!(
            outcome::judge(expected, &observation),
            StepJudgement::Continue
        ) {
            return Ok(StepObservation {
                observation,
                assertion_failures: Vec::new(),
                advisories: Vec::new(),
            });
        }
        Self::refuse_unrecorded(case, step)?;
        let Ok(status) = StatusCode::from_u16(recorded.response.status) else {
            return Ok(StepObservation::transport(format!(
                "recorded status {} is not an HTTP status code",
                recorded.response.status
            )));
        };
        let body = recorded.response.body.as_ref().unwrap_or(&Value::Null);
        let facts = ExchangeFacts {
            status,
            body,
            media_type: recorded
                .response
                .headers
                .iter()
                .find(|(name, _)| name.eq_ignore_ascii_case("content-type"))
                .map(|(_, value)| value.as_str()),
        };
        let mut no_corpus = crate::exec::assertions::NoCorpus;
        let mut corpus = self.corpus();
        let ground: &mut dyn CorpusGround = match corpus.as_mut() {
            Some(loaded) => loaded,
            None => &mut no_corpus,
        };
        // Binding header matchers are EXECUTED expectations, not documentation
        // (#473): before this the player judged status, captures and assertions
        // and never these, so an entry could reproduce a pass over a recording
        // whose headers violated its own binding.
        let mut assertion_failures = Vec::new();
        if let outcome::Observation::Kind(kind) = observation
            && let Some(expectation) = binding.outcome(kind)
        {
            Self::refuse_ungrounded_headers(case, step, expectation, recorded)?;
            // `last_version_uid` and `spec_versions` are None because a pack
            // recording tracks neither: the uid matcher then asserts presence
            // only and a dated rule is out of scope, which is what both
            // evaluators already do for a party that declares nothing.
            let header_ctx = crate::exec::headers::RequestContext {
                accept: recorded.request.accept.as_deref(),
                last_version_uid: None,
                spec_versions: None,
            };
            assertion_failures.extend(
                crate::exec::headers::evaluate(
                    expectation,
                    &recorded.response.headers,
                    recorded.response.body.as_ref(),
                    &header_ctx,
                    vars,
                )
                .into_iter()
                .map(crate::exec::assertions::AssertionOutcome::Mismatch),
            );
        }
        let mut advisories = Vec::new();
        for assertion in &step.assertions {
            match crate::exec::assertions::eval_from_exchange(assertion, facts, ground) {
                Ok(recorded_advisories) => advisories.extend(recorded_advisories),
                Err(outcome) => assertion_failures.push(outcome),
            }
        }
        Ok(StepObservation {
            observation,
            assertion_failures,
            advisories,
        })
    }

    fn provision(
        &mut self,
        case: &CaseCore,
        _row: usize,
        _vars: &mut VarStore,
    ) -> Result<crate::exec::Provisioned, String> {
        // The transcript records no provisioned handles, so a case that READS
        // one would resolve against a value the exchanges never used.
        let minted = case.requires.minted_handles();
        if minted.is_empty() {
            return Ok(crate::exec::Provisioned::Ready);
        }
        let read: Vec<String> = minted
            .iter()
            .filter(|handle| case_reads_capture(case, handle))
            .map(ToString::to_string)
            .collect();
        if read.is_empty() {
            return Ok(crate::exec::Provisioned::Ready);
        }
        Err(format!(
            "case {} reads provisioned handle(s) {} the transcript does not record; a pack \
             entry may not resolve a requires handle against a value the recorded exchanges \
             never used",
            case.id,
            read.join(", ")
        ))
    }

    /// Refuses any judged postcondition instead of reproducing a verdict it
    /// never checked.
    ///
    /// A postcondition runs after the flow, with no exchange of its own to
    /// read, so every [`PostconditionRole::Judged`] family is unevaluable here
    /// and the entry is refused by name. The aggregate family is law e
    /// ([`TranscriptPlayer::aggregates`]) and the informative families are
    /// never pass/fail, so neither blocks a replay.
    fn postconditions(
        &mut self,
        case: &CaseCore,
        _row: usize,
        _vars: &mut VarStore,
    ) -> Result<crate::exec::PostconditionOutcomes, String> {
        let unjudgeable: Vec<&str> = case
            .postconditions
            .iter()
            .filter(|a| matches!(a.postcondition_role(), PostconditionRole::Judged))
            .map(Assertion::family)
            .collect();
        if unjudgeable.is_empty() {
            return Ok(crate::exec::PostconditionOutcomes::default());
        }
        Err(format!(
            "case {} carries postconditions the transcript replay cannot judge ({}); a pack entry \
             may not claim a verdict over assertions the replay never evaluated",
            case.id,
            unjudgeable.join(", ")
        ))
    }

    fn aggregates(
        &mut self,
        case: &CaseCore,
        all_rows: &[VarStore],
    ) -> Result<Vec<String>, String> {
        let mut failures = Vec::new();
        for assertion in &case.postconditions {
            if let Assertion::Unique { over, .. } = assertion
                && let crate::refgrammar::ValueRef::Capture { name, .. } = &over.0
                && let Err(crate::exec::assertions::AssertionFailure(m)) =
                    crate::exec::assertions::eval_unique(name, all_rows)
            {
                failures.push(m);
            }
        }
        Ok(failures)
    }
}

fn extract_scalar(
    response: &RecordedResponse,
    spec: &crate::model::binding::WireCapture,
    vars: &VarStore,
) -> Option<String> {
    let from_source = |source: &WireFrom| -> Option<String> {
        match source {
            WireFrom::Header { name, last_segment } => {
                let value = response
                    .headers
                    .iter()
                    .find(|(k, _)| k.eq_ignore_ascii_case(name))
                    .map(|(_, v)| v.clone())?;
                if *last_segment {
                    value.rsplit('/').next().map(ToOwned::to_owned)
                } else {
                    Some(value)
                }
            }
            WireFrom::Body { path } => {
                let mut current = response.body.as_ref()?;
                for seg in path.split('.') {
                    current = current.get(seg)?;
                }
                match current {
                    Value::String(s) => Some(s.clone()),
                    other => Some(other.to_string()),
                }
            }
            WireFrom::Capture(name) => vars.scalar(name).map(ToOwned::to_owned),
        }
    };
    let mut value =
        from_source(&spec.from).or_else(|| spec.fallback.as_ref().and_then(from_source))?;
    if matches!(
        spec.strip,
        Some(crate::model::binding::StripRule::WeakQuotes)
    ) {
        value = value.trim_start_matches("W/").trim_matches('"').to_owned();
    }
    // One implementation of the closed transform grammar, so a replay cannot
    // judge a capture differently from the live run that recorded it.
    if let Some(transform) = spec.transform {
        value = transform.apply(&value)?;
    }
    Some(value)
}

fn extract_list(body: &Value, path: &str) -> Vec<String> {
    let mut current = vec![body];
    for seg in path.split('.') {
        let (attr, star) = match seg.strip_suffix("[*]") {
            Some(attr) => (attr, true),
            None => (seg, false),
        };
        let mut next = Vec::new();
        for v in current {
            let v = if attr.is_empty() {
                Some(v)
            } else {
                v.get(attr)
            };
            if let Some(v) = v {
                if star {
                    if let Some(items) = v.as_array() {
                        next.extend(items.iter());
                    }
                } else {
                    next.push(v);
                }
            }
        }
        current = next;
    }
    current
        .into_iter()
        .filter_map(|v| match v {
            Value::String(s) => Some(s.clone()),
            other => other.as_str().map(ToOwned::to_owned),
        })
        .collect()
}

/// Replay one transcript entry against its case and judge whether the
/// runner reproduces the adjudicated verdict.
///
/// # Errors
/// Interpreter defects only (unknown case, unresolvable binding).
pub fn replay_entry(
    set: &crate::artifacts::ArtifactSet,
    entry: &TranscriptEntry,
) -> Result<(ExpectedVerdict, crate::exec::RowOutcome), String> {
    let case = set
        .cases
        .iter()
        .map(|(_, c)| c)
        .find(|c| c.id == entry.case)
        .ok_or_else(|| format!("transcript case {} is not in the catalogue", entry.case))?;
    // A transcript entry records ONE case×format×row sequence: slice the
    // case to that row so the recorded steps line up 1:1 with the flow.
    let mut sliced = case.clone();
    if let Some(parameters) = &mut sliced.parameters {
        if let Some(matrix) = &mut parameters.matrix {
            let row =
                matrix.rows.get(entry.row).cloned().ok_or_else(|| {
                    format!("case {} has no matrix row {}", entry.case, entry.row)
                })?;
            matrix.rows = vec![row];
        }
        if let Some(fixtures) = &mut parameters.fixture_set {
            let fixture = fixtures
                .get(entry.row)
                .cloned()
                .ok_or_else(|| format!("case {} has no fixture row {}", entry.case, entry.row))?;
            *fixtures = vec![fixture];
        }
    }
    let mut player = TranscriptPlayer::new(set, entry);
    let record = crate::exec::run_case(&sliced, entry.format, &mut player)?;
    let row = record
        .rows
        .first()
        .cloned()
        .ok_or_else(|| format!("case {} produced no row", entry.case))?;
    Ok((entry.expected_verdict, row))
}

/// Whether a produced row outcome reproduces the adjudicated verdict.
#[must_use]
pub fn verdict_matches(expected: ExpectedVerdict, produced: &crate::exec::RowOutcome) -> bool {
    matches!(
        (expected, produced),
        (ExpectedVerdict::Passed, crate::exec::RowOutcome::Passed)
            | (
                ExpectedVerdict::Failed,
                crate::exec::RowOutcome::Failed { .. }
            )
            | (
                ExpectedVerdict::Errored,
                crate::exec::RowOutcome::Errored { .. }
            )
            | (
                ExpectedVerdict::NotApplicable,
                crate::exec::RowOutcome::NotApplicable { .. }
            )
            | (
                ExpectedVerdict::Skipped,
                crate::exec::RowOutcome::Skipped { .. }
            )
    )
}
