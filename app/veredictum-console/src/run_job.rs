// SPDX-FileCopyrightText: Veredictum contributors
// SPDX-License-Identifier: Apache-2.0

//! The run job (#66): one engine run at a time, supervised server-side.
//!
//! A run's identity is a [`RunId`], a UUID minted once and unique across
//! processes and restarts (#386): it names the run's own directory under the
//! mounted output tree, and the live URL carries it, so a run stays
//! addressable after the memory holding it is gone.
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

/// A run's identity: a UUID, minted once when the run is allocated.
///
/// Unique across processes and restarts, which is what makes it addressable:
/// `/run/live/{run_id}` names one run, and `job_dir` gives that run its own
/// directory under the mounted output tree. Both compilation targets carry
/// the type — the id crosses the server-fn wire and the browser reads it out
/// of the URL — so the reference to that server-only function is plain text
/// rather than a link the featureless doc build cannot resolve.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(transparent)]
pub struct RunId(uuid::Uuid);

impl RunId {
    /// The nil run id: all zeros, which no minted id can be.
    ///
    /// Documentation capture mode shows it in place of a minted id
    /// ([`crate::capture::PINNED_RUN_ID`]), so a capture pass over an
    /// unchanged console photographs the same address.
    pub const NIL: Self = Self(uuid::Uuid::nil());

    /// Mints a fresh run id.
    ///
    /// Server-side only: a v4 mint needs a random source, and the console's
    /// WASM bundle claims no getrandom backend.
    #[cfg(feature = "ssr")]
    #[must_use]
    pub fn mint() -> Self {
        Self(uuid::Uuid::new_v4())
    }
}

impl std::fmt::Display for RunId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        std::fmt::Display::fmt(&self.0.as_hyphenated(), f)
    }
}

/// Why a run id read from a URL could not be understood.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("`{text}` is not a run id: {reason}")]
pub struct RunIdError {
    /// The text the URL carried, verbatim.
    pub text: String,
    /// The UUID parser's own reason.
    pub reason: String,
}

impl std::str::FromStr for RunId {
    type Err = RunIdError;

    fn from_str(text: &str) -> Result<Self, Self::Err> {
        uuid::Uuid::parse_str(text)
            .map(Self)
            .map_err(|e| RunIdError {
                text: text.to_owned(),
                reason: e.to_string(),
            })
    }
}

/// One parsed `--progress` line: `progress: <k>/<n>` with an optional case id
/// (#81's documented grammar).
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
/// the remaining count. Labelling it an estimate is the screen's duty.
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
    /// The run's identity, the address `/run/live/{run_id}` carries.
    pub id: RunId,
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
    id: RunId,
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

/// The directory one job's artifacts land in, under the mounted output tree.
///
/// The ONE derivation of that path (#134): the run seam creates it before the
/// spawn, and the export seam reads the sealed bundle back out of it. Two
/// spellings of the same claim drift the moment either side changes.
#[cfg(feature = "ssr")]
#[must_use]
pub fn job_dir(out: &std::path::Path, id: RunId) -> std::path::PathBuf {
    out.join(format!("console-job-{id}"))
}

/// The one job slot: the console runs at most one campaign at a time.
#[cfg(feature = "ssr")]
#[derive(Debug, Clone, Default)]
pub struct JobSlot {
    state: Arc<Mutex<Option<JobState>>>,
}

/// Everything the supervisor can refuse.
#[cfg(feature = "ssr")]
#[derive(Debug, thiserror::Error)]
pub enum JobError {
    /// A run is already driving.
    #[error("a run is already in flight (run {0}); cancel it first or wait")]
    Busy(RunId),
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

/// Records one streamed engine line on `job`.
///
/// A `--progress` line (#81's grammar) moves the counters and the per-case
/// duration sample instead of landing in the tail; everything else is tail
/// text, capped at [`TAIL_CAP`] lines.
#[cfg(feature = "ssr")]
fn record_line(job: &mut JobState, line: Line) {
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
}

/// The status the stream's `outcome` lands the job in, with the summary a
/// finished run carries.
///
/// A failure after a requested cancel is a cancellation, never a run failure:
/// the killed subprocess leaves no results document by design.
#[cfg(feature = "ssr")]
fn finish_status(
    outcome: Result<crate::engine::Finished, crate::engine::Error>,
    cancel_requested: bool,
) -> (JobStatus, Option<FinishedView>) {
    match outcome {
        Ok(finished) => {
            let counts = tally(&finished.results);
            (
                JobStatus::Finished,
                Some(FinishedView {
                    passed: counts.0,
                    failed: counts.1,
                    errored: counts.2,
                    not_applicable: counts.3,
                    results_path: finished.results_path.display().to_string(),
                }),
            )
        }
        Err(_) if cancel_requested => (JobStatus::Cancelled, None),
        Err(e) => (JobStatus::Failed(e.to_string()), None),
    }
}

/// One job's live view, read off the state the supervising thread writes.
#[cfg(feature = "ssr")]
fn snapshot(job: &JobState) -> JobView {
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
}

#[cfg(feature = "ssr")]
impl JobSlot {
    /// Allocates this run's id — the caller derives the output directory from
    /// it BEFORE starting, so the artifacts' home carries the id.
    ///
    /// Infallible: minting a UUID takes no lock and reads no shared state.
    #[expect(
        clippy::unused_self,
        reason = "the slot owns run identity, and #389 turns this into an insertion that needs it"
    )]
    #[must_use]
    pub fn allocate_id(&self) -> RunId {
        RunId::mint()
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
        id: RunId,
        engine: &Engine,
        spec: &RunSpec,
        sut_name: String,
    ) -> Result<RunId, JobError> {
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
                record_line(job, line);
            };
            let outcome = running.stream(on_line);
            if let Ok(mut guard) = slot.lock()
                && let Some(job) = guard.as_mut()
            {
                job.canceller = None;
                let (status, finished) = finish_status(outcome, job.cancel_requested);
                if let Some(view) = finished {
                    job.finished = Some(view);
                }
                job.status = status;
            }
        });
        Ok(id)
    }

    /// Cancels the NAMED run; the supervising thread records the state.
    ///
    /// A run this slot does not hold, or one that already left `Running`, is
    /// [`JobError::Idle`]: cancel addresses one run, never whatever happens
    /// to be in flight.
    ///
    /// # Errors
    /// [`JobError::Idle`] when the named run is not driving here,
    /// [`JobError::Poisoned`], and the kill's own failure.
    pub fn cancel(&self, id: RunId) -> Result<(), JobError> {
        let canceller = {
            let mut guard = self
                .state
                .lock()
                .map_err(|poison| JobError::Poisoned(poison.to_string()))?;
            let job = guard.as_mut().ok_or(JobError::Idle)?;
            if job.id != id || job.status != JobStatus::Running {
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
        Ok(guard.as_ref().map(snapshot))
    }

    /// The live view of the NAMED run: `Some` only when this slot holds it.
    ///
    /// The call the live screen makes, so the screen can tell "this process
    /// is driving the run you asked about" from "some other run is in this
    /// slot". #389 turns it into a map lookup.
    ///
    /// # Errors
    /// [`JobError::Poisoned`] only.
    pub fn view_of(&self, id: RunId) -> Result<Option<JobView>, JobError> {
        let guard = self
            .state
            .lock()
            .map_err(|poison| JobError::Poisoned(poison.to_string()))?;
        Ok(guard.as_ref().filter(|job| job.id == id).map(snapshot))
    }

    /// The run this process holds, running or finished.
    ///
    /// The seam #389 replaces: one slot has exactly one current run, and a
    /// map of concurrent runs has none. Everything else here addresses a run
    /// by id.
    ///
    /// # Errors
    /// [`JobError::Poisoned`] only.
    pub fn current(&self) -> Result<Option<RunId>, JobError> {
        let guard = self
            .state
            .lock()
            .map_err(|poison| JobError::Poisoned(poison.to_string()))?;
        Ok(guard.as_ref().map(|job| job.id))
    }
}

/// The engine's own tally over the typed record: passed / failed / errored /
/// excused, counted on the lib's status enum itself.
///
/// The match is exhaustive on purpose: a status the engine adds later breaks
/// this build instead of being counted as excused by a catch-all.
#[cfg(feature = "ssr")]
pub(crate) fn tally(results: &veredictum::party::Results) -> (u64, u64, u64, u64) {
    use veredictum::party::OutcomeStatus;

    let mut counts = (0_u64, 0_u64, 0_u64, 0_u64);
    for outcome in &results.outcomes {
        match outcome.status {
            OutcomeStatus::Passed => counts.0 += 1,
            OutcomeStatus::Failed => counts.1 += 1,
            OutcomeStatus::Errored => counts.2 += 1,
            OutcomeStatus::Skipped | OutcomeStatus::NotApplicable => counts.3 += 1,
        }
    }
    counts
}

#[cfg(test)]
mod tests {
    use super::{RunId, RunIdError, eta_ms, parse_progress};

    /// One fixed id, so the derivations below assert a path and never a
    /// fresh mint.
    const FIXED: &str = "3f2504e0-4f89-41d3-9a0c-0305e82c3301";

    #[cfg(feature = "ssr")]
    #[expect(
        clippy::panic_in_result_fn,
        reason = "the Book ch11 Result-returning test shape: assertions panic, plumbing propagates with ?"
    )]
    #[test]
    fn the_job_directory_is_the_output_root_plus_the_id() -> Result<(), RunIdError> {
        let id: RunId = FIXED.parse()?;
        assert_eq!(
            super::job_dir(std::path::Path::new("/work/out"), id),
            std::path::PathBuf::from(format!("/work/out/console-job-{FIXED}"))
        );
        Ok(())
    }

    /// The URL trip: the id renders hyphenated and reads back as itself.
    #[expect(
        clippy::panic_in_result_fn,
        reason = "the Book ch11 Result-returning test shape: assertions panic, plumbing propagates with ?"
    )]
    #[test]
    fn a_run_id_round_trips_through_the_url() -> Result<(), RunIdError> {
        let id: RunId = FIXED.parse()?;
        assert_eq!(id.to_string(), FIXED, "the URL spelling is hyphenated");
        assert_eq!(FIXED.parse::<RunId>()?, id);
        assert_eq!(
            RunId::NIL.to_string(),
            "00000000-0000-0000-0000-000000000000"
        );
        Ok(())
    }

    /// The server-fn trip: the id serializes as itself, so a URL segment and
    /// a wire value are the same text.
    #[cfg(feature = "ssr")]
    #[expect(
        clippy::panic_in_result_fn,
        reason = "the Book ch11 Result-returning test shape: assertions panic, plumbing propagates with ?"
    )]
    #[test]
    fn a_run_id_round_trips_across_the_server_fn_wire() -> Result<(), Box<dyn std::error::Error>> {
        let id: RunId = FIXED.parse()?;
        let wire = serde_json::to_string(&id)?;
        assert_eq!(wire, format!("\"{FIXED}\""));
        assert_eq!(serde_json::from_str::<RunId>(&wire)?, id);
        Ok(())
    }

    /// A path segment that is not a run id refuses with the text it read, so
    /// the screen can say what it was handed.
    #[test]
    fn a_malformed_run_id_refuses_by_name() {
        let refusal = "../../etc/passwd"
            .parse::<RunId>()
            .expect_err("a path is not a run id");
        assert_eq!(refusal.text, "../../etc/passwd");
        assert!(!refusal.reason.is_empty(), "{refusal:?}");
        assert!(refusal.to_string().contains("is not a run id"), "{refusal}");
    }

    /// Every status the engine records lands in exactly one column, and the
    /// two citation-bearing selection records count as excused rather than as
    /// failures.
    #[cfg(feature = "ssr")]
    #[expect(
        clippy::panic_in_result_fn,
        reason = "the Book ch11 Result-returning test shape: assertions panic, plumbing propagates with ?"
    )]
    #[test]
    fn the_tally_counts_every_recorded_status() -> Result<(), serde_json::Error> {
        // Authored as bytes, the way the engine writes the document the
        // console reads back, so a codec change fails here.
        const RECORD: &str = r#"{
            "sut": { "name": "sut", "version": "1" },
            "runner": {
                "name": "veredictum",
                "version": "0",
                "verification_pack_status": "passed"
            },
            "schedule_release": "0",
            "tech_profile": { "its": "its-rest", "formats": [] },
            "ixit_digest": "0",
            "outcomes": [
                { "case": "A-a", "status": "passed", "rows_driven": 1, "rows_total": 1 },
                { "case": "A-b", "status": "passed", "rows_driven": 1, "rows_total": 1 },
                { "case": "A-c", "status": "failed", "rows_driven": 1, "rows_total": 1 },
                { "case": "A-d", "status": "errored", "rows_driven": 1, "rows_total": 1 },
                {
                    "case": "A-e",
                    "status": "skipped",
                    "rows_driven": 0,
                    "rows_total": 1,
                    "citation": "ITS-REST"
                },
                {
                    "case": "A-f",
                    "status": "not_applicable",
                    "rows_driven": 0,
                    "rows_total": 1,
                    "citation": "ITS-REST"
                }
            ]
        }"#;

        let results: veredictum::party::Results = serde_json::from_str(RECORD)?;
        assert_eq!(super::tally(&results), (2, 1, 1, 2));
        Ok(())
    }

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
