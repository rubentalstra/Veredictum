// SPDX-FileCopyrightText: Veredictum contributors
// SPDX-License-Identifier: Apache-2.0

//! The data-driven executor: the flow interpreter under the five step-3
//! interpreter laws —
//!
//! **(a)** `reset_per_row` re-establishes the whole `requires` block around
//! every row; **(b)** a step whose observed outcome differs from `expect`
//! fails the row and aborts its remaining steps and row postconditions;
//! **(c)** transport faults, unmapped responses and assertions this ITS or
//! this run cannot judge ([`assertions::AssertionOutcome::Unjudgeable`]) →
//! `errored` (inconclusive), a mapped-but-unexpected outcome or a served
//! value that contradicts an assertion → `failed`; **(d)**
//! `${time:*}` resolution is fixed (±1 ms, midpoint — [`state::VarStore`]);
//! **(e)** aggregate assertions collect across rows and evaluate once after
//! the last row.
//!
//! The row engine is transport-agnostic: a [`StepDriver`] performs one step
//! call and reports what it observed, so the same laws run against the live
//! reqwest driver and the verification-pack transcript player.

#![allow(
    clippy::disallowed_types,
    reason = "dev/verification tooling over JSON artifacts (the catalogue, results, wire \
              exchanges), whose shapes belong to the artifacts and the SUT; the carriers \
              here are cfg(test)-only, so #[expect] would be unfulfilled in the non-test build"
)]

pub mod assertions;
pub mod bodies;
pub mod content_synth;
pub mod driver;
pub mod headers;
pub mod opt_synth;
pub mod outcome;
pub mod player;
pub mod recipes;
pub mod resolve;
pub mod resultset;
pub mod signature;
pub mod state;
pub mod transport;
pub mod versioned;

use crate::ids::CaseId;
use crate::model::case::{CaseCore, FlowStep};
use crate::vocab::{FormatName, OutcomeKind};

use assertions::AssertionOutcome;

use outcome::{Observation, StepJudgement};
use state::VarStore;

/// The per-row outcome record (ISO/IEC 9646 mapping: passed→pass,
/// failed→fail, errored→inconclusive; not-applicable/skipped are selection
/// records, each with a mandatory citation).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RowOutcome {
    /// Every step and assertion of the row held.
    Passed,
    /// An assertion or expectation did not hold.
    Failed {
        /// The flow step that failed.
        step: u32,
        /// What did not hold, in one line.
        reason: String,
    },
    /// The exchange itself broke, so the row proves nothing.
    Errored {
        /// The flow step that errored.
        step: u32,
        /// What went wrong, in one line.
        reason: String,
    },
    /// Selection excluded the row before it was driven.
    NotApplicable {
        /// The spec/register citation grounding the exclusion.
        citation: String,
    },
    /// The row was deliberately not driven.
    Skipped {
        /// The spec/register citation grounding the skip.
        citation: String,
    },
}

/// The record one case×format execution produces — the direct input to the
/// party-artifact emission (`results.json` outcomes[]).
#[derive(Debug, Clone)]
pub struct CaseRecord {
    /// The case this record is for.
    pub case: CaseId,
    /// The format axis the case was executed on, when parameterized.
    pub format: Option<FormatName>,
    /// One outcome per executed row, in row order.
    pub rows: Vec<RowOutcome>,
    /// rows driven / rows selected (the printed coverage bound).
    pub rows_driven: usize,
    /// How many rows selection admitted in total.
    pub rows_total: usize,
    /// Non-gating observations the rows produced, in execution order.
    ///
    /// A recorded observation is a SHOULD-strength sentence the SUT did not
    /// follow while satisfying every MUST the row gates on, so it changes no
    /// verdict and stays out of `results.json`, which carries only what a
    /// verdict is computed from. The run prints these beside the tally.
    pub advisories: Vec<String>,
}

impl CaseRecord {
    /// A case passes iff every driven row passed (verdict rollup input).
    #[must_use]
    pub fn passed(&self) -> bool {
        !self.rows.is_empty()
            && self
                .rows
                .iter()
                .all(|r| matches!(r, RowOutcome::Passed | RowOutcome::NotApplicable { .. }))
    }
}

/// What a driver observed for one step, plus the captures it bound.
#[derive(Debug)]
pub struct StepObservation {
    /// What the driver saw on the wire, classified.
    pub observation: Observation,
    /// Post-step assertion failures (empty when all held), each carrying its
    /// own channel: a mismatch fails the row, an unjudgeable assertion errors
    /// it. Only meaningful when the observation matched the expectation.
    pub assertion_failures: Vec<AssertionOutcome>,
    /// Non-gating observations the step's assertions recorded: a SHOULD-strength
    /// divergence a passing assertion tolerated (see [`CaseRecord::advisories`]).
    pub advisories: Vec<String>,
}

impl StepObservation {
    /// A transport-class observation with no assertion results — the shape
    /// every driver-internal failure takes (see `StepDriver::perform`).
    #[must_use]
    pub fn transport(message: String) -> Self {
        Self {
            observation: Observation::Transport(message),
            assertion_failures: Vec::new(),
            advisories: Vec::new(),
        }
    }

    /// This step's recorded observations, each prefixed with the row and step
    /// it was made on.
    #[must_use]
    pub fn labelled_advisories(&self, row: usize, step: u32) -> Vec<String> {
        self.advisories
            .iter()
            .map(|advisory| format!("row {row} step {step}: {advisory}"))
            .collect()
    }
}

/// What a row's judged postconditions produced: the outcomes that decide the
/// row, and the non-gating divergences a passing assertion tolerated.
#[derive(Debug, Default)]
pub struct PostconditionOutcomes {
    /// Judged outcomes, in evaluation order, each carrying its own channel.
    pub failures: Vec<AssertionOutcome>,
    /// Recorded non-gating observations, in evaluation order.
    pub advisories: Vec<String>,
}

/// One step execution seam: the live HTTP driver and the transcript player
/// both implement this.
///
/// The driver resolves the step's operation against the bindings, performs
/// the call on the selected instance, classifies the response (law c), binds
/// the step's captures into `vars`, and evaluates the step's post-step
/// assertions.
pub trait StepDriver {
    /// Perform one flow step for the given expected kind.
    ///
    /// # Errors
    /// Driver-internal failures surface as `Observation::Transport`, never
    /// as `Err` — the `Err` channel is reserved for interpreter defects
    /// (unresolvable artifacts), which abort the run.
    fn perform(
        &mut self,
        case: &CaseCore,
        step: &FlowStep,
        expected: OutcomeKind,
        row: usize,
        vars: &mut VarStore,
    ) -> Result<StepObservation, String>;

    /// Re-establish the case's `requires` block (law a). Called before the
    /// first row and again before every row under `reset_per_row`. Returns
    /// [`Provisioned::RowNotApplicable`] when THIS row's ground cannot be
    /// realized on the technology profile (register-cited) — the row is
    /// recorded N/A and its steps are not driven.
    ///
    /// # Errors
    /// As [`StepDriver::perform`]: interpreter defects only.
    fn provision(
        &mut self,
        case: &CaseCore,
        row: usize,
        vars: &mut VarStore,
    ) -> Result<Provisioned, String>;

    /// Evaluate the case's per-row postconditions (non-aggregate).
    ///
    /// # Errors
    /// Interpreter defects only; judged outcomes return in the record, each
    /// carrying the channel [`run_case`] routes it by.
    fn postconditions(
        &mut self,
        case: &CaseCore,
        row: usize,
        vars: &mut VarStore,
    ) -> Result<PostconditionOutcomes, String>;

    /// Evaluate the aggregate assertions once after the last row (law e),
    /// over the values collected across all rows.
    ///
    /// # Errors
    /// Interpreter defects only.
    fn aggregates(&mut self, case: &CaseCore, all_rows: &[VarStore])
    -> Result<Vec<String>, String>;
}

/// The outcome of law-a provisioning for one row.
#[derive(Debug, Clone, PartialEq)]
pub enum Provisioned {
    /// The ground is established; drive the row's steps.
    Ready,
    /// The row's ground is unrealizable on this technology profile — the
    /// row is N/A with the given register citation.
    RowNotApplicable {
        /// The excusing register citation (e.g. an `AMB-nn` entry).
        citation: String,
    },
    /// The SUT REFUSED a provisioning exchange (e.g. a template upload
    /// answered outside 2xx/409), so the case's required ground does not
    /// exist — the row is inconclusive, never a SUT failure of the
    /// behaviour under test (`.claude` triage law: an unestablished
    /// `requires` precondition is a step-resolution failure). The reason
    /// names the provisioning exchange so the red row localizes to it.
    RowErrored {
        /// The refused provisioning exchange (operation, status, body head).
        reason: String,
    },
}

/// Resolve the expected kind for a step in a given row (the per-fixture
/// `${fixture.expected}` override).
fn expected_kind(case: &CaseCore, step: &FlowStep, row: usize) -> Option<OutcomeKind> {
    if let Some(matrix) = case.parameters.as_ref().and_then(|p| p.matrix.as_ref())
        && let Some(col) = matrix.columns.iter().position(|c| c == "expected")
        && let Some(crate::model::case::MatrixCell::Literal(serde_json::Value::String(s))) =
            matrix.rows.get(row).and_then(|cells| cells.get(col))
        && let Some(kind) = OutcomeKind::from_token(s)
    {
        return Some(kind);
    }
    match step.expect {
        crate::model::case::ExpectSpec::Kind(kind) => Some(kind),
        crate::model::case::ExpectSpec::FixtureExpected => case
            .parameters
            .as_ref()
            .and_then(|p| p.fixture_set.as_ref())
            .and_then(|fixtures| fixtures.get(row))
            .map(|f| f.expected),
    }
}

/// Row count of a case (matrix rows, fixture rows, or one).
#[must_use]
pub fn row_count(case: &CaseCore) -> usize {
    case.parameters
        .as_ref()
        .map_or(1, |p| {
            p.matrix
                .as_ref()
                .map(|m| m.rows.len())
                .or_else(|| p.fixture_set.as_ref().map(Vec::len))
                .unwrap_or(1)
        })
        .max(1)
}

/// The assertion outcome a row is recorded against: the first MISMATCH when
/// the list holds one, else the first entry.
///
/// A mismatch is a finding the run actually proved, so it outranks an
/// unjudgeable sibling assertion, which proved nothing. Without the
/// preference an unjudgeable assertion authored ahead of a real mismatch
/// would hide a genuine conformance finding behind an inconclusive row.
fn judged_first(failures: &[AssertionOutcome]) -> Option<&AssertionOutcome> {
    failures
        .iter()
        .find(|failure| matches!(**failure, AssertionOutcome::Mismatch(_)))
        .or_else(|| failures.first())
}

/// Route one assertion outcome to its row outcome: a mismatch is a
/// conformance finding against the SUT (law b), an unjudgeable assertion is
/// inconclusive beside a transport fault (law c).
fn row_from_assertion(step: u32, failure: &AssertionOutcome) -> RowOutcome {
    match failure {
        AssertionOutcome::Mismatch(reason) => RowOutcome::Failed {
            step,
            reason: reason.clone(),
        },
        AssertionOutcome::Unjudgeable(reason) => RowOutcome::Errored {
            step,
            reason: reason.clone(),
        },
    }
}

/// Run one case×format through the interpreter laws.
///
/// # Errors
/// Interpreter defects only (unresolvable artifacts); conformance outcomes
/// — incl. errored rows — return in the record.
pub fn run_case<D: StepDriver>(
    case: &CaseCore,
    format: Option<FormatName>,
    driver: &mut D,
) -> Result<CaseRecord, String> {
    let total = row_count(case);
    let reset_per_row = case
        .parameters
        .as_ref()
        .is_none_or(|p| matches!(p.iteration, crate::vocab::Iteration::ResetPerRow));

    let mut rows = Vec::with_capacity(total);
    let mut row_states: Vec<VarStore> = Vec::with_capacity(total);
    let mut advisories: Vec<String> = Vec::new();
    let mut vars = VarStore::default();

    for row in 0..total {
        if reset_per_row || row == 0 {
            vars = VarStore::default();
            // Law a. Step 0 is the precondition, before any flow step drove.
            match driver.provision(case, row, &mut vars)? {
                Provisioned::Ready => {}
                Provisioned::RowNotApplicable { citation } => {
                    row_states.push(vars.clone());
                    rows.push(RowOutcome::NotApplicable { citation });
                    continue;
                }
                Provisioned::RowErrored { reason } => {
                    row_states.push(vars.clone());
                    rows.push(RowOutcome::Errored { step: 0, reason });
                    continue;
                }
            }
        }

        let mut row_outcome = RowOutcome::Passed;
        'steps: for step in &case.flow {
            let Some(expected) = expected_kind(case, step, row) else {
                row_outcome = RowOutcome::Errored {
                    step: step.step,
                    reason: "no expected kind resolvable for this row".to_owned(),
                };
                break 'steps;
            };
            let observed = driver.perform(case, step, expected, row, &mut vars)?;
            advisories.extend(observed.labelled_advisories(row, step.step));
            match outcome::judge(expected, &observed.observation) {
                StepJudgement::Continue => {
                    if let Some(failure) = judged_first(&observed.assertion_failures) {
                        row_outcome = row_from_assertion(step.step, failure);
                        break 'steps; // law b for a mismatch, law c otherwise
                    }
                }
                StepJudgement::Failed { expected, observed } => {
                    row_outcome = RowOutcome::Failed {
                        step: step.step,
                        reason: format!(
                            "expected `{}`, observed `{}`",
                            expected.token(),
                            observed.token()
                        ),
                    };
                    break 'steps; // law b: abort remaining steps
                }
                StepJudgement::Errored(reason) => {
                    row_outcome = RowOutcome::Errored {
                        step: step.step,
                        reason,
                    };
                    break 'steps;
                }
            }
        }

        // Law b: row postconditions run only when every step held.
        if matches!(row_outcome, RowOutcome::Passed) {
            let postconditions = driver.postconditions(case, row, &mut vars)?;
            advisories.extend(
                postconditions
                    .advisories
                    .iter()
                    .map(|line| format!("row {row} postconditions: {line}")),
            );
            if let Some(failure) = judged_first(&postconditions.failures) {
                row_outcome = row_from_assertion(0, failure);
            }
        }

        row_states.push(vars.clone());
        rows.push(row_outcome);
    }

    // Law e: aggregates once after the last row.
    if rows.iter().all(|r| matches!(r, RowOutcome::Passed))
        && let Some(failure) = driver.aggregates(case, &row_states)?.into_iter().next()
        && let Some(last) = rows.last_mut()
    {
        {
            *last = RowOutcome::Failed {
                step: 0,
                reason: format!("aggregate: {failure}"),
            };
        }
    }

    Ok(CaseRecord {
        case: case.id.clone(),
        format,
        rows_driven: rows.len(),
        rows_total: total,
        rows,
        advisories,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::vocab::OutcomeKind;

    /// A scripted driver: each (row, step) yields a fixed observation.
    struct Scripted {
        provisioned: usize,
        script: Vec<Observation>,
        aggregate_failure: Option<String>,
        cursor: usize,
    }

    impl StepDriver for Scripted {
        fn perform(
            &mut self,
            _case: &CaseCore,
            _step: &FlowStep,
            _expected: OutcomeKind,
            _row: usize,
            _vars: &mut VarStore,
        ) -> Result<StepObservation, String> {
            let observation = self.script.get(self.cursor).cloned().unwrap();
            self.cursor += 1;
            Ok(StepObservation {
                observation,
                assertion_failures: Vec::new(),
                advisories: Vec::new(),
            })
        }
        fn provision(
            &mut self,
            _c: &CaseCore,
            _r: usize,
            _v: &mut VarStore,
        ) -> Result<Provisioned, String> {
            self.provisioned += 1;
            Ok(Provisioned::Ready)
        }
        fn postconditions(
            &mut self,
            _c: &CaseCore,
            _r: usize,
            _v: &mut VarStore,
        ) -> Result<PostconditionOutcomes, String> {
            Ok(PostconditionOutcomes::default())
        }
        fn aggregates(&mut self, _c: &CaseCore, _rows: &[VarStore]) -> Result<Vec<String>, String> {
            Ok(self.aggregate_failure.clone().into_iter().collect())
        }
    }

    fn two_row_case() -> CaseCore {
        serde_json::from_value(serde_json::json!({
            "id": "I_EHR_SERVICE.create_ehr-law_test",
            "kind": "functional",
            "component": "EHR",
            "sm_operation": "I_EHR_SERVICE.create_ehr",
            "capabilities": ["EhrOperations"],
            "profiles": ["CORE"],
            "test_purpose": "laws",
            "description": "laws",
            "spec_refs": ["CNF platform_test_schedule master06 §create_ehr data sets"],
            "requires": { "server": "empty" },
            "parameters": { "iteration": "reset_per_row",
                             "matrix": { "columns": ["ehr_id"], "rows": [["absent"], ["provided"]] } },
            "flow": [
                { "step": 1, "call": "create_ehr", "expect": "created" },
                { "step": 2, "call": "create_ehr", "expect": "already_exists" }
            ]
        }))
        .unwrap()
    }

    #[test]
    fn law_a_reprovisions_per_row_and_law_b_aborts() {
        let case = two_row_case();
        let mut driver = Scripted {
            provisioned: 0,
            script: vec![
                Observation::Kind(OutcomeKind::Created),
                Observation::Kind(OutcomeKind::Created), // mismatch
                Observation::Kind(OutcomeKind::Created),
                Observation::Kind(OutcomeKind::AlreadyExists),
            ],
            aggregate_failure: None,
            cursor: 0,
        };
        let record = run_case(&case, None, &mut driver).unwrap();
        assert_eq!(driver.provisioned, 2); // law a: one provision per row
        assert!(matches!(record.rows[0], RowOutcome::Failed { step: 2, .. }));
        assert!(matches!(record.rows[1], RowOutcome::Passed));
        assert!(!record.passed());
    }

    #[test]
    fn law_c_errored_is_not_failed_and_law_e_runs_last() {
        let case = two_row_case();
        let mut driver = Scripted {
            provisioned: 0,
            script: vec![
                Observation::Kind(OutcomeKind::Created),
                Observation::Kind(OutcomeKind::AlreadyExists),
                Observation::Kind(OutcomeKind::Created),
                Observation::Kind(OutcomeKind::AlreadyExists),
            ],
            aggregate_failure: Some("ehr_id values are not pairwise distinct".to_owned()),
            cursor: 0,
        };
        let record = run_case(&case, None, &mut driver).unwrap();
        // Law e: the aggregate failure lands on the LAST row.
        assert!(matches!(record.rows[1], RowOutcome::Failed { step: 0, .. }));

        let mut errored = Scripted {
            provisioned: 0,
            script: vec![
                Observation::Transport("connection refused".into()),
                Observation::Kind(OutcomeKind::Created),
                Observation::Kind(OutcomeKind::AlreadyExists),
            ],
            aggregate_failure: None,
            cursor: 0,
        };
        let record = run_case(&case, None, &mut errored).unwrap();
        assert!(matches!(
            record.rows[0],
            RowOutcome::Errored { step: 1, .. }
        ));
        assert!(matches!(record.rows[1], RowOutcome::Passed));
    }

    /// A driver whose provisioning outcome, assertion results and
    /// postconditions are all scripted per row, so law a's two non-Ready
    /// verdicts and law b's assertion channel are exercised on their own.
    struct Provisioning {
        grounds: Vec<Provisioned>,
        assertion_failures: Vec<Vec<String>>,
        postconditions: Vec<String>,
        performed: usize,
        cursor: usize,
    }

    impl Provisioning {
        fn ready(assertion_failures: Vec<Vec<String>>) -> Self {
            Self {
                grounds: vec![Provisioned::Ready; assertion_failures.len().max(1)],
                assertion_failures,
                postconditions: Vec::new(),
                performed: 0,
                cursor: 0,
            }
        }
    }

    impl StepDriver for Provisioning {
        fn perform(
            &mut self,
            _case: &CaseCore,
            _step: &FlowStep,
            expected: OutcomeKind,
            _row: usize,
            _vars: &mut VarStore,
        ) -> Result<StepObservation, String> {
            let assertion_failures = self
                .assertion_failures
                .get(self.performed)
                .cloned()
                .unwrap_or_default()
                .into_iter()
                .map(AssertionOutcome::Mismatch)
                .collect();
            self.performed += 1;
            Ok(StepObservation {
                observation: Observation::Kind(expected),
                assertion_failures,
                advisories: Vec::new(),
            })
        }
        fn provision(
            &mut self,
            _c: &CaseCore,
            _r: usize,
            _v: &mut VarStore,
        ) -> Result<Provisioned, String> {
            let outcome = self
                .grounds
                .get(self.cursor)
                .cloned()
                .unwrap_or(Provisioned::Ready);
            self.cursor += 1;
            Ok(outcome)
        }
        fn postconditions(
            &mut self,
            _c: &CaseCore,
            _r: usize,
            _v: &mut VarStore,
        ) -> Result<PostconditionOutcomes, String> {
            Ok(PostconditionOutcomes {
                failures: self
                    .postconditions
                    .iter()
                    .cloned()
                    .map(AssertionOutcome::Mismatch)
                    .collect(),
                advisories: Vec::new(),
            })
        }
        fn aggregates(&mut self, _c: &CaseCore, _rows: &[VarStore]) -> Result<Vec<String>, String> {
            Ok(Vec::new())
        }
    }

    /// Law a's two non-Ready verdicts mean opposite things about the server:
    /// an unrealizable ground records the row not-applicable with its register
    /// citation, a refused provisioning exchange records it inconclusive at
    /// step 0, and neither is a SUT failure of the behaviour under test.
    #[test]
    fn an_unprovisionable_row_is_excused_or_inconclusive_and_never_driven() {
        let case = two_row_case();
        let mut driver = Provisioning {
            grounds: vec![
                Provisioned::RowNotApplicable {
                    citation: "AMB-99: no such ground on this profile".to_owned(),
                },
                Provisioned::RowErrored {
                    reason: "template upload answered 500".to_owned(),
                },
            ],
            assertion_failures: Vec::new(),
            postconditions: Vec::new(),
            performed: 0,
            cursor: 0,
        };
        let record = run_case(&case, None, &mut driver).unwrap();

        assert_eq!(
            record.rows[0],
            RowOutcome::NotApplicable {
                citation: "AMB-99: no such ground on this profile".to_owned()
            }
        );
        assert_eq!(
            record.rows[1],
            RowOutcome::Errored {
                step: 0,
                reason: "template upload answered 500".to_owned()
            }
        );
        assert_eq!(
            driver.performed, 0,
            "neither row's steps are driven once provisioning did not succeed"
        );
        assert!(
            !record.passed(),
            "an inconclusive row is not a passing case"
        );
    }

    /// A case whose every row is excused still rolls up as passed: an N/A row
    /// carries a citation rather than evidence against the server, so it never
    /// turns a case red on its own.
    #[test]
    fn a_wholly_excused_case_rolls_up_as_passed() {
        let case = two_row_case();
        let mut driver = Provisioning {
            grounds: vec![
                Provisioned::RowNotApplicable {
                    citation: "AMB-99".to_owned(),
                };
                2
            ],
            assertion_failures: Vec::new(),
            postconditions: Vec::new(),
            performed: 0,
            cursor: 0,
        };
        let record = run_case(&case, None, &mut driver).unwrap();
        assert!(record.passed());
        assert_eq!(record.rows_driven, 2);
        assert_eq!(record.rows_total, 2);
    }

    /// Law b through the ASSERTION channel: a step whose observation matched
    /// the expectation but whose post-step assertion failed fails the row at
    /// that step and aborts the rest of it.
    #[test]
    fn a_failed_step_assertion_fails_the_row_at_its_own_step() {
        let case = two_row_case();
        // Row 0's step 2 never runs, so the drive count is 1 + 2 = 3.
        let mut driver = Provisioning::ready(vec![
            vec!["body/uid did not match".to_owned()],
            Vec::new(),
            Vec::new(),
        ]);
        let record = run_case(&case, None, &mut driver).unwrap();
        assert_eq!(
            record.rows[0],
            RowOutcome::Failed {
                step: 1,
                reason: "body/uid did not match".to_owned()
            }
        );
        assert!(matches!(record.rows[1], RowOutcome::Passed));
        assert_eq!(driver.performed, 3, "row 0 aborted after its first step");

        let mut clean = Provisioning::ready(vec![Vec::new(); 4]);
        let record = run_case(&case, None, &mut clean).unwrap();
        assert!(record.passed());
        assert_eq!(clean.performed, 4, "both steps of both rows drove");
    }

    /// Law b again, on the row POSTCONDITIONS: they run only when every step
    /// held, and a failure lands on the row at step 0 (the row, not a step).
    #[test]
    fn row_postconditions_fail_the_row_at_step_zero() {
        let case = two_row_case();
        let mut driver = Provisioning::ready(vec![Vec::new(); 4]);
        driver.postconditions = vec!["the read-back is not equivalent".to_owned()];
        let record = run_case(&case, None, &mut driver).unwrap();
        for row in &record.rows {
            assert_eq!(
                row,
                &RowOutcome::Failed {
                    step: 0,
                    reason: "the read-back is not equivalent".to_owned()
                }
            );
        }
    }

    /// The reserved matrix `expected` column is the NORMATIVE per-row
    /// override: a row naming its own outcome kind is judged against that,
    /// while the flow's own `expect` is the inherited default.
    #[test]
    fn the_reserved_expected_column_overrides_the_flow_expectation() {
        let case: CaseCore = serde_json::from_value(serde_json::json!({
            "id": "I_EHR_SERVICE.create_ehr-per_row_expectation",
            "kind": "functional", "component": "EHR",
            "sm_operation": "I_EHR_SERVICE.create_ehr",
            "capabilities": ["EhrOperations"], "profiles": ["CORE"],
            "test_purpose": "per-row expectation", "description": "per-row expectation",
            "spec_refs": ["CNF platform_test_schedule master06 §create_ehr data sets"],
            "parameters": { "iteration": "reset_per_row", "matrix": {
                "columns": ["ehr_id", "expected"],
                "rows": [["absent", "created"], ["provided", "already_exists"]]
            } },
            "flow": [{ "step": 1, "call": "create_ehr", "expect": "created" }]
        }))
        .unwrap();

        assert_eq!(
            expected_kind(&case, &case.flow[0], 0),
            Some(OutcomeKind::Created)
        );
        assert_eq!(
            expected_kind(&case, &case.flow[0], 1),
            Some(OutcomeKind::AlreadyExists),
            "row 1's own column, not the flow's `created`"
        );

        assert_eq!(
            expected_kind(&case, &case.flow[0], 9),
            Some(OutcomeKind::Created),
            "a row past the matrix inherits the flow expectation"
        );
    }

    /// `${fixture.expected}` resolves per fixture-set row, and a row the set
    /// does not carry resolves to NOTHING — which the interpreter records as
    /// an errored row rather than guessing an expectation.
    #[test]
    fn a_fixture_row_carries_its_own_expectation_and_a_missing_one_errors() {
        let case: CaseCore = serde_json::from_value(serde_json::json!({
            "id": "I_EHR_COMPOSITION.create_composition-fixtures",
            "kind": "functional", "component": "EHR_COMPOSITION",
            "sm_operation": "I_EHR_COMPOSITION.create_composition",
            "capabilities": ["CompositionOperations"], "profiles": ["CORE"],
            "test_purpose": "fixture expectations", "description": "fixture expectations",
            "spec_refs": ["CNF platform_test_schedule master06 §create_ehr data sets"],
            "parameters": { "iteration": "reset_per_row", "fixture_set": [
                { "data_set": "cnf.valid.one", "expected": "created" },
                { "data_set": "cnf.invalid.one", "expected": "validation_failed",
                  "defect": "empty 1..* list", "spec_ref": "RM data_structures §ITEM_LIST" }
            ] },
            "flow": [{ "step": 1, "call": "create_composition", "expect": "${fixture.expected}" }]
        }))
        .unwrap();

        assert_eq!(row_count(&case), 2, "the fixture set is the row axis");
        assert_eq!(
            expected_kind(&case, &case.flow[0], 0),
            Some(OutcomeKind::Created)
        );
        assert_eq!(
            expected_kind(&case, &case.flow[0], 1),
            Some(OutcomeKind::ValidationFailed)
        );
        assert_eq!(
            expected_kind(&case, &case.flow[0], 2),
            None,
            "a row the fixture set does not carry resolves to no expectation"
        );

        // The interpreter never substitutes a default outcome for a row it
        // cannot resolve, so driving it is inconclusive.
        let mut truncated = case.clone();
        if let Some(parameters) = &mut truncated.parameters
            && let Some(fixtures) = &mut parameters.fixture_set
        {
            fixtures.clear();
        }
        let mut driver = Provisioning::ready(Vec::new());
        let record = run_case(&truncated, None, &mut driver).unwrap();
        assert_eq!(record.rows_total, 1, "an empty axis still drives one row");
        assert_eq!(
            record.rows[0],
            RowOutcome::Errored {
                step: 1,
                reason: "no expected kind resolvable for this row".to_owned()
            }
        );
        assert_eq!(driver.performed, 0);
    }

    /// A case with no `parameters` at all is one row, and a record with no
    /// rows at all never counts as passed.
    #[test]
    fn the_row_axis_is_never_empty_and_an_empty_record_never_passes() {
        let case: CaseCore = serde_json::from_value(serde_json::json!({
            "id": "I_EHR_SERVICE.create_ehr-single",
            "kind": "functional", "component": "EHR",
            "sm_operation": "I_EHR_SERVICE.create_ehr",
            "capabilities": ["EhrOperations"], "profiles": ["CORE"],
            "test_purpose": "single row", "description": "single row",
            "spec_refs": ["CNF platform_test_schedule master06 §create_ehr data sets"],
            "flow": [{ "step": 1, "call": "create_ehr", "expect": "created" }]
        }))
        .unwrap();
        assert_eq!(row_count(&case), 1);

        let empty = CaseRecord {
            case: case.id.clone(),
            format: None,
            rows: Vec::new(),
            rows_driven: 0,
            rows_total: 1,
            advisories: Vec::new(),
        };
        assert!(
            !empty.passed(),
            "a case that produced no row proves nothing"
        );
    }

    /// A driver-internal failure reaches the interpreter as a TRANSPORT
    /// observation with no assertion results, which law c classifies as
    /// inconclusive — the `Err` channel stays reserved for interpreter defects.
    #[test]
    fn a_driver_internal_failure_is_a_transport_observation() {
        let observed = StepObservation::transport("connection refused".to_owned());
        assert_eq!(
            observed.observation,
            Observation::Transport("connection refused".to_owned())
        );
        assert!(observed.assertion_failures.is_empty());
    }
}
