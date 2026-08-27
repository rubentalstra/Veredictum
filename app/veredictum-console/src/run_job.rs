// SPDX-FileCopyrightText: Veredictum contributors
// SPDX-License-Identifier: Apache-2.0

//! The run job (#66): one engine run at a time, supervised server-side.
//!
//! The job's memory is in-process, like everything else in the console: an
//! image restart legitimately forgets a finished job, and the artifacts it
//! wrote stay in the mounted output directory exactly as a terminal run
//! leaves them. The live screen polls [`JobView`]; the progress numbers come
//! from the engine's own `--progress` lines when the binary emits them
//! (#81), and degrade to elapsed-only when the pinned binary predates the
//! flag — never a fabricated counter.

use serde::{Deserialize, Serialize};

#[cfg(feature = "ssr")]
use std::sync::{Arc, Mutex};
#[cfg(feature = "ssr")]
use std::time::Instant;

#[cfg(feature = "ssr")]
use crate::engine::{Canceller, Engine, Line, RunSpec};

/// How many tail lines the job keeps for the live screen.
#[cfg(feature = "ssr")]
const TAIL_CAP: usize = 200;

/// One parsed `--progress` line: `progress: <k>/<n>` with an optional case id
/// (#81's documented grammar). Pure, unit-tested.
#[must_use]
pub fn parse_progress(line: &str) -> Option<(u64, u64, Option<String>)> {
    let rest = line.strip_prefix("progress: ")?;
    let (counts, case) = match rest.split_once(' ') {
        Some((counts, case)) => (counts, Some(case.to_owned())),
        None => (rest, None),
    };
    let (completed, total) = counts.split_once('/')?;
    Some((completed.parse().ok()?, total.parse().ok()?, case))
}

/// The estimate over observed per-case durations: the moving median times
/// the remaining count. Pure, unit-tested; the label "estimate" is the
/// screen's duty.
#[must_use]
pub fn eta_ms(durations_ms: &[u64], remaining: u64) -> Option<u64> {
    if durations_ms.is_empty() {
        return None;
    }
    let mut sorted = durations_ms.to_vec();
    sorted.sort_unstable();
    // The median index truncates by definition.
    #[expect(
        clippy::integer_division,
        reason = "the median index of a sorted list truncates by definition"
    )]
    let median = *sorted.get(sorted.len() / 2)?;
    median.checked_mul(remaining)
}

/// Where a job stands; the live screen's whole vocabulary.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum JobStatus {
    /// The engine is driving.
    Running,
    /// The engine exited and the record parsed.
    Finished,
    /// Cancel was requested and the process was killed.
    Cancelled,
    /// The engine exited without a parseable record; the field is verbatim.
    Failed(String),
}

/// The finished job's summary, from the typed record.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FinishedView {
    /// Passed / failed / errored / not-applicable, the engine's own tally.
    pub passed: u64,
    /// Failed case records.
    pub failed: u64,
    /// Errored case records.
    pub errored: u64,
    /// Not-applicable case records.
    pub not_applicable: u64,
    /// Where `results.json` landed, for the record surfaces (#67).
    pub results_path: String,
}

/// What the live screen polls.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct JobView {
    /// The job id (monotonic per console process).
    pub id: u64,
    /// Where the job stands.
    pub status: JobStatus,
    /// The SUT display name the run records.
    pub sut_name: String,
    /// Cases completed, from the engine's own progress stream.
    pub completed: u64,
    /// Cases selected, from the engine's own progress stream; `0` until the
    /// stream announces it (or forever, on a binary predating `--progress`).
    pub total: u64,
    /// The case currently driving, verbatim from the stream.
    pub current_case: Option<String>,
    /// Elapsed milliseconds.
    pub elapsed_ms: u64,
    /// The moving-median estimate of what remains; always labelled an
    /// estimate by the screen.
    pub eta_ms: Option<u64>,
    /// The engine's own output tail, newest last.
    pub tail: Vec<String>,
    /// The finished summary, once the record parsed.
    pub finished: Option<FinishedView>,
}

/// The supervisor's shared state.
#[cfg(feature = "ssr")]
#[derive(Debug)]
struct JobState {
    id: u64,
    status: JobStatus,
    sut_name: String,
    completed: u64,
    total: u64,
    current_case: Option<String>,
    started: Instant,
    last_case_at: Instant,
    durations_ms: Vec<u64>,
    tail: std::collections::VecDeque<String>,
    finished: Option<FinishedView>,
    canceller: Option<Canceller>,
    cancel_requested: bool,
}

/// The one job slot: the console runs at most one campaign at a time.
#[cfg(feature = "ssr")]
#[derive(Debug, Clone, Default)]
pub struct JobSlot {
    state: Arc<Mutex<Option<JobState>>>,
    next_id: Arc<Mutex<u64>>,
}

/// Everything the supervisor can refuse.
#[cfg(feature = "ssr")]
#[derive(Debug, thiserror::Error)]
pub enum JobError {
    /// A run is already driving.
    #[error("a run is already in flight (job {0}); cancel it first or wait")]
    Busy(u64),
    /// The slot's lock was poisoned by a panicking thread; the field is the
    /// poison's own display.
    #[error("the job state is poisoned ({0}); restart the console")]
    Poisoned(String),
    /// There is no job to act on.
    #[error("no run is in flight")]
    Idle,
    /// The engine refused the spawn.
    #[error(transparent)]
    Engine(#[from] crate::engine::Error),
}

#[cfg(feature = "ssr")]
impl JobSlot {
    /// Allocates the next job id — the caller derives the output directory
    /// from it BEFORE starting, so the artifacts' home carries the id.
    ///
    /// # Errors
    /// [`JobError::Poisoned`] only.
    pub fn allocate_id(&self) -> Result<u64, JobError> {
        let mut next = self
            .next_id
            .lock()
            .map_err(|poison| JobError::Poisoned(poison.to_string()))?;
        *next += 1;
        Ok(*next)
    }

    /// Starts a run under a previously allocated id: spawns the engine and a
    /// supervising thread that streams it into the slot. Refuses while a run
    /// is already in flight.
    ///
    /// # Errors
    /// [`JobError::Busy`] with the in-flight id, [`JobError::Poisoned`], and
    /// the engine's own spawn refusals.
    pub fn start(
        &self,
        id: u64,
        engine: &Engine,
        spec: &RunSpec,
        sut_name: String,
    ) -> Result<u64, JobError> {
        let mut guard = self
            .state
            .lock()
            .map_err(|poison| JobError::Poisoned(poison.to_string()))?;
        if let Some(existing) = guard.as_ref()
            && existing.status == JobStatus::Running
        {
            return Err(JobError::Busy(existing.id));
        }
        let running = engine.spawn(spec)?;
        let canceller = running.canceller();
        let now = Instant::now();
        *guard = Some(JobState {
            id,
            status: JobStatus::Running,
            sut_name,
            completed: 0,
            total: 0,
            current_case: None,
            started: now,
            last_case_at: now,
            durations_ms: Vec::new(),
            tail: std::collections::VecDeque::new(),
            finished: None,
            canceller: Some(canceller),
            cancel_requested: false,
        });
        drop(guard);

        let slot = Arc::clone(&self.state);
        // The supervising thread owns the stream; the server fns only ever
        // take the lock briefly. A thread rather than a tokio task because
        // the engine stream is blocking I/O end to end.
        std::thread::spawn(move || {
            let on_line = |line: Line| {
                let Ok(mut guard) = slot.lock() else { return };
                let Some(job) = guard.as_mut() else { return };
                let text = match line {
                    Line::Out(text) => {
                        if let Some((completed, total, case)) = parse_progress(&text) {
                            let now = Instant::now();
                            if completed > job.completed && job.completed > 0 {
                                let elapsed = now.duration_since(job.last_case_at).as_millis();
                                job.durations_ms
                                    .push(u64::try_from(elapsed).unwrap_or(u64::MAX));
                            }
                            job.last_case_at = now;
                            job.completed = completed;
                            job.total = total;
                            job.current_case = case;
                            return;
                        }
                        text
                    }
                    Line::Err(text) => text,
                };
                if job.tail.len() == TAIL_CAP {
                    job.tail.pop_front();
                }
                job.tail.push_back(text);
            };
            let outcome = running.stream(on_line);
            if let Ok(mut guard) = slot.lock()
                && let Some(job) = guard.as_mut()
            {
                job.canceller = None;
                job.status = match outcome {
                    Ok(finished) => {
                        let counts = tally(&finished.results);
                        job.finished = Some(FinishedView {
                            passed: counts.0,
                            failed: counts.1,
                            errored: counts.2,
                            not_applicable: counts.3,
                            results_path: finished.results_path.display().to_string(),
                        });
                        JobStatus::Finished
                    }
                    Err(_) if job.cancel_requested => JobStatus::Cancelled,
                    Err(e) => JobStatus::Failed(e.to_string()),
                };
            }
        });
        Ok(id)
    }

    /// Cancels the in-flight run; the supervising thread records the state.
    ///
    /// # Errors
    /// [`JobError::Idle`] with nothing in flight, [`JobError::Poisoned`], and
    /// the kill's own failure.
    pub fn cancel(&self) -> Result<(), JobError> {
        let canceller = {
            let mut guard = self
                .state
                .lock()
                .map_err(|poison| JobError::Poisoned(poison.to_string()))?;
            let job = guard.as_mut().ok_or(JobError::Idle)?;
            if job.status != JobStatus::Running {
                return Err(JobError::Idle);
            }
            job.cancel_requested = true;
            job.canceller.clone().ok_or(JobError::Idle)?
        };
        canceller.cancel().map_err(JobError::Engine)
    }

    /// The live view, when a job exists.
    ///
    /// # Errors
    /// [`JobError::Poisoned`] only.
    pub fn view(&self) -> Result<Option<JobView>, JobError> {
        let guard = self
            .state
            .lock()
            .map_err(|poison| JobError::Poisoned(poison.to_string()))?;
        Ok(guard.as_ref().map(|job| {
            let elapsed = u64::try_from(job.started.elapsed().as_millis()).unwrap_or(u64::MAX);
            let remaining = job.total.saturating_sub(job.completed);
            JobView {
                id: job.id,
                status: job.status.clone(),
                sut_name: job.sut_name.clone(),
                completed: job.completed,
                total: job.total,
                current_case: job.current_case.clone(),
                elapsed_ms: elapsed,
                eta_ms: if job.status == JobStatus::Running {
                    eta_ms(&job.durations_ms, remaining)
                } else {
                    None
                },
                tail: job.tail.iter().cloned().collect(),
                finished: job.finished.clone(),
            }
        }))
    }
}

/// The engine's own tally over the typed record: passed / failed / errored /
/// not-applicable, by outcome status token.
#[cfg(feature = "ssr")]
fn tally(results: &veredictum::party::Results) -> (u64, u64, u64, u64) {
    let mut counts = (0_u64, 0_u64, 0_u64, 0_u64);
    for outcome in &results.outcomes {
        let token = serde_json::to_string(&outcome.status).unwrap_or_default();
        match token.trim_matches('"') {
            "passed" => counts.0 += 1,
            "failed" => counts.1 += 1,
            "errored" => counts.2 += 1,
            _ => counts.3 += 1,
        }
    }
    counts
}

#[cfg(test)]
mod tests {
    use super::{eta_ms, parse_progress};

    #[test]
    fn the_progress_grammar_parses_both_shapes() {
        assert_eq!(parse_progress("progress: 0/14"), Some((0, 14, None)));
        assert_eq!(
            parse_progress("progress: 3/14 I_EHR_SERVICE.create_ehr-main"),
            Some((3, 14, Some(String::from("I_EHR_SERVICE.create_ehr-main"))))
        );
        assert_eq!(parse_progress("14 case-records: …"), None);
        assert_eq!(parse_progress("progress: nonsense"), None);
    }

    #[test]
    fn the_estimate_is_the_median_times_the_remainder() {
        assert_eq!(eta_ms(&[], 5), None);
        assert_eq!(eta_ms(&[100], 5), Some(500));
        assert_eq!(eta_ms(&[50, 1_000, 100], 4), Some(400));
    }
}
