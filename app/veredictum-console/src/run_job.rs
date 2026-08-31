// SPDX-FileCopyrightText: Veredictum contributors
// SPDX-License-Identifier: Apache-2.0

//! The run jobs (#66, #389): every run this process is driving, supervised
//! server-side and addressed by its own id.
//!
//! A run's identity is a [`RunId`], a UUID minted once and unique across
//! processes and restarts (#386): it names the run's own directory under the
//! mounted output tree, and the live URL carries it, so a run stays
//! addressable after the memory holding it is gone.
//!
//! Several people share one hosted instance, so the job memory is a MAP from
//! that id to the run's state, and nothing here asks "is there a job" — it
//! asks about a named run, or about a named submitter. The caps that keep the
//! instance usable are the constants block below, enforced while the map's
//! lock is held so two simultaneous starts cannot both pass them.
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
use std::collections::BTreeMap;
#[cfg(feature = "ssr")]
use std::path::PathBuf;
#[cfg(feature = "ssr")]
use std::sync::atomic::{AtomicU64, Ordering};
#[cfg(feature = "ssr")]
use std::sync::{Arc, Mutex};
#[cfg(feature = "ssr")]
use std::time::{Duration, Instant};

#[cfg(feature = "ssr")]
use crate::engine::{Canceller, Engine, Line, RunSpec};
#[cfg(feature = "ssr")]
use crate::submitter::Submitter;

/// How many tail lines the job keeps for the live screen.
#[cfg(feature = "ssr")]
const TAIL_CAP: usize = 200;

// ── The caps (#388, #389) ───────────────────────────────────────────────────
// Starting values to re-derive by MEASURING on the chosen host, never
// constants to defend. Each is driven by a test rather than asserted as a
// number, and [`Limits`] is how a test drives one whose real value is longer
// than a test may take.

/// How many runs may drive at once on one process.
///
/// The ceiling is memory: each run is an engine process that loads the whole
/// catalogue.
#[cfg(feature = "ssr")]
pub const MAX_CONCURRENT_RUNS: usize = 2;

/// How many runs one submitter may have in flight, counting queued ones.
///
/// One, so a single visitor cannot fill the instance.
#[cfg(feature = "ssr")]
pub const MAX_RUNS_PER_SUBMITTER: usize = 1;

/// How long a run may drive before the cap ends it.
///
/// At the deadline the run is cancelled with [`JobStatus::Expired`], which
/// says the cap ended it rather than the operator, and its partial record is
/// discarded.
#[cfg(feature = "ssr")]
pub const RUN_WALL_CLOCK: Duration = Duration::from_mins(30);

/// How many finished runs stay in the map before the oldest is evicted.
///
/// Eviction drops the memory only: the run's artifacts stay where the run
/// wrote them, so an evicted id still resolves through its own directory
/// (#386's recorded state).
#[cfg(feature = "ssr")]
pub const FINISHED_RUNS_KEPT: usize = 16;

/// How many connection drafts stay in memory before the oldest is evicted.
///
/// The drafts map is keyed by submitter and holds credential VALUES, so it is
/// bounded for the same reason the job map is.
#[cfg(feature = "ssr")]
pub const DRAFTS_KEPT: usize = 64;

/// How often the wall-clock watchdog wakes.
///
/// A hung run emits no output at all, so a deadline check inside the
/// output-streaming loop would never fire; the watchdog is its own timer.
#[cfg(feature = "ssr")]
pub const WATCHDOG_TICK: Duration = Duration::from_secs(10);

/// The caps one [`JobSlot`] enforces.
///
/// [`Limits::default`] is the constants block above, which is what the server
/// runs. A test injects a shortened wall clock or a smaller keep-count to
/// drive a cap whose real value is longer than a test may take.
#[cfg(feature = "ssr")]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Limits {
    /// [`MAX_CONCURRENT_RUNS`].
    pub max_concurrent: usize,
    /// [`MAX_RUNS_PER_SUBMITTER`].
    pub max_per_submitter: usize,
    /// [`RUN_WALL_CLOCK`].
    pub wall_clock: Duration,
    /// [`FINISHED_RUNS_KEPT`].
    pub finished_kept: usize,
    /// [`WATCHDOG_TICK`].
    pub watchdog_tick: Duration,
}

#[cfg(feature = "ssr")]
impl Default for Limits {
    fn default() -> Self {
        Self {
            max_concurrent: MAX_CONCURRENT_RUNS,
            max_per_submitter: MAX_RUNS_PER_SUBMITTER,
            wall_clock: RUN_WALL_CLOCK,
            finished_kept: FINISHED_RUNS_KEPT,
            watchdog_tick: WATCHDOG_TICK,
        }
    }
}

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

/// The estimated wait for a queued run, from the running runs' own estimates.
///
/// A run at position `p` waits for the `p`-th slot to free, so the answer is
/// the `p`-th smallest estimate among the runs currently driving. An unknown
/// estimate anywhere ahead, or a position past the number of slots, gives
/// `None` — no estimate at all is honest, and a fabricated one is not.
#[must_use]
pub fn queue_wait_ms(running_eta_ms: &[Option<u64>], position: u32) -> Option<u64> {
    if position == 0 {
        return None;
    }
    let mut known: Vec<u64> = Vec::with_capacity(running_eta_ms.len());
    for estimate in running_eta_ms {
        known.push((*estimate)?);
    }
    known.sort_unstable();
    let index = usize::try_from(position).ok()?.checked_sub(1)?;
    known.get(index).copied()
}

/// Where a job stands; the live screen's whole vocabulary.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum JobStatus {
    /// The instance is full, so the run is accepted and waiting: it has its
    /// id and its address already, and its place in the queue is stated
    /// rather than hidden behind a spinner.
    Queued {
        /// This run's place in the queue, 1 for the next to start.
        ///
        /// A `u32` and never a `usize`: the value crosses the server-fn wire
        /// to a 32-bit WASM client.
        position: u32,
    },
    /// The engine is driving.
    Running,
    /// The engine exited and the record parsed.
    Finished,
    /// Cancel was requested by the operator and the process was killed.
    Cancelled,
    /// The wall-clock cap ended the run, not the operator, and its partial
    /// record was discarded.
    Expired,
    /// The engine exited without a parseable record; the field is verbatim.
    Failed(String),
}

impl JobStatus {
    /// Whether the run has stopped for good.
    ///
    /// A terminal run holds no slot, blocks no submitter, and is what
    /// eviction and the finished-run readers select on.
    #[must_use]
    pub fn is_terminal(&self) -> bool {
        match self {
            Self::Queued { .. } | Self::Running => false,
            Self::Finished | Self::Cancelled | Self::Expired | Self::Failed(_) => true,
        }
    }
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
    /// Elapsed milliseconds: since the run started driving, or — while it is
    /// queued — since it was accepted.
    pub elapsed_ms: u64,
    /// The moving-median estimate of what remains for a driving run, and the
    /// estimated wait until a slot frees for a queued one. Always labelled an
    /// estimate by the screen.
    pub eta_ms: Option<u64>,
    /// The engine's own output tail, newest last.
    pub tail: Vec<String>,
    /// The finished summary, once the record parsed.
    pub finished: Option<FinishedView>,
}

/// Which of a submitter's runs a reader is asking for.
///
/// The closed vocabulary of the ONE per-submitter lookup, so the record and
/// export seams share a reader instead of spelling the same claim twice
/// (#134).
#[cfg(feature = "ssr")]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Latest {
    /// The most recent run, whatever it is doing.
    Any,
    /// The most recent run that finished with a parsed record.
    Finished,
}

/// Why a run stopped early, which is what tells the operator's cancel from
/// the wall-clock cap's.
#[cfg(feature = "ssr")]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum CancelCause {
    /// Someone pressed cancel.
    Operator,
    /// The wall-clock cap ran out.
    WallClock,
}

/// A queued run's unspent start: everything the spawn will need.
#[cfg(feature = "ssr")]
#[derive(Debug)]
struct Pending {
    engine: Engine,
    spec: RunSpec,
}

/// The supervisor's per-run state.
#[cfg(feature = "ssr")]
#[derive(Debug)]
struct JobState {
    id: RunId,
    submitter: Submitter,
    seq: u64,
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
    finished_at: Option<Instant>,
    canceller: Option<Canceller>,
    cancel: Option<CancelCause>,
    out_dir: PathBuf,
    pending: Option<Pending>,
}

/// Every run this process holds, keyed by its id.
///
/// A `BTreeMap` and never a `HashMap`: this map is iterated for the queue and
/// for eviction, and undefined iteration order is denied workspace-wide.
#[cfg(feature = "ssr")]
type Jobs = BTreeMap<RunId, JobState>;

/// The arrival counter: FIFO ordering the map's key cannot give, because a
/// UUID sorts by its bytes rather than by when it was minted.
#[cfg(feature = "ssr")]
static NEXT_SEQ: AtomicU64 = AtomicU64::new(0);

/// The directory one job's artifacts land in, under the mounted output tree.
///
/// The ONE derivation of that path (#134): the run seam creates it before the
/// spawn, and the export seam reads the sealed bundle back out of it. Two
/// spellings of the same claim drift the moment either side changes.
#[cfg(feature = "ssr")]
#[must_use]
pub fn job_dir(out: &std::path::Path, id: RunId) -> PathBuf {
    out.join(format!("console-job-{id}"))
}

/// How long a finished run's artifacts stay on disk.
///
/// A separate lifetime from [`FINISHED_RUNS_KEPT`], which bounds MEMORY. The
/// disk needs its own because the instrument now runs on a host that does not
/// restart: what a disposable filesystem discarded every few hours would
/// otherwise grow until the disk is gone, and a full disk fails in a way that
/// looks like nothing at all. A day means a submitter who walks away and comes
/// back the same day still finds their record; beyond that the run answers
/// through #386's honest "this console knows nothing about that run".
#[cfg(feature = "ssr")]
pub const ARTIFACTS_KEPT: Duration = Duration::from_hours(24);

/// How often the artifact sweeper wakes.
///
/// Time-driven rather than event-driven on purpose: a deploy may be followed by
/// no run at all, and the directories left by the deploy before it still have
/// to go.
#[cfg(feature = "ssr")]
pub const SWEEP_INTERVAL: Duration = Duration::from_hours(1);

/// What one sweep did.
#[cfg(feature = "ssr")]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Swept {
    /// Directories removed.
    pub removed: usize,
    /// Directories kept because a run in the map still names them.
    pub live: usize,
    /// Directories kept because they are younger than [`ARTIFACTS_KEPT`].
    pub young: usize,
    /// Directories that could not be read or removed, which is a fact about
    /// the host rather than about a run.
    pub refused: usize,
}

/// Removes the artifact directories of runs this process no longer holds and
/// that are older than `keep`.
///
/// `live` is every id still in the map, and one of those is never swept
/// whatever its age: the live screen reads that directory. Nothing outside the
/// `console-job-<uuid>` shape is touched, so an operator's own files under the
/// output mount are left alone.
///
/// An unreadable directory is counted and stepped over rather than returned as
/// a failure: a sweep that stops at the first surprise leaves the disk filling.
#[cfg(feature = "ssr")]
#[must_use]
pub fn sweep_artifacts(
    out: &std::path::Path,
    keep: Duration,
    live: &std::collections::BTreeSet<RunId>,
) -> Swept {
    let mut swept = Swept::default();
    let Ok(entries) = std::fs::read_dir(out) else {
        return swept;
    };
    for entry in entries.flatten() {
        let name = entry.file_name();
        let Some(name) = name.to_str() else {
            continue;
        };
        let Some(id) = name
            .strip_prefix("console-job-")
            .and_then(|rest| rest.parse::<RunId>().ok())
        else {
            continue;
        };
        if live.contains(&id) {
            swept.live += 1;
            continue;
        }
        let age = entry
            .metadata()
            .and_then(|meta| meta.modified())
            .map(|at| at.elapsed().unwrap_or_default());
        match age {
            Ok(age) if age >= keep => {
                if std::fs::remove_dir_all(entry.path()).is_ok() {
                    swept.removed += 1;
                } else {
                    swept.refused += 1;
                }
            }
            Ok(_) => swept.young += 1,
            Err(_) => swept.refused += 1,
        }
    }
    swept
}

/// The run map: every campaign this process is driving, queued or remembered.
#[cfg(feature = "ssr")]
#[derive(Debug, Clone)]
pub struct JobSlot {
    state: Arc<Mutex<Jobs>>,
    limits: Limits,
}

#[cfg(feature = "ssr")]
impl Default for JobSlot {
    fn default() -> Self {
        Self {
            state: Arc::new(Mutex::new(Jobs::new())),
            limits: Limits::default(),
        }
    }
}

/// Everything the supervisor can refuse.
#[cfg(feature = "ssr")]
#[derive(Debug, thiserror::Error)]
pub enum JobError {
    /// This submitter already has a run in flight, queued or driving. The
    /// field is the run they already have, so the screen can link to it.
    #[error(
        "you already have a run in flight (run {0}); watch it or cancel it before starting another"
    )]
    Busy(RunId),
    /// The map's lock was poisoned by a panicking thread; the field is the
    /// poison's own display.
    #[error("the job state is poisoned ({0}); restart the console")]
    Poisoned(String),
    /// There is no such run to act on here.
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
/// the killed subprocess leaves no results document by design. Which cancel
/// it was decides which word the screen reads — the operator's, or the
/// wall-clock cap's.
#[cfg(feature = "ssr")]
fn finish_status(
    outcome: Result<crate::engine::Finished, crate::engine::Error>,
    cancel: Option<CancelCause>,
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
        Err(_) if cancel == Some(CancelCause::WallClock) => (JobStatus::Expired, None),
        Err(_) if cancel == Some(CancelCause::Operator) => (JobStatus::Cancelled, None),
        Err(e) => (JobStatus::Failed(e.to_string()), None),
    }
}

/// One job's live view, read off the state the supervising thread writes.
///
/// The queued case needs its neighbours: its estimated wait is derived from
/// the runs currently driving, which is why this reads the whole map.
#[cfg(feature = "ssr")]
fn snapshot(jobs: &Jobs, job: &JobState) -> JobView {
    let elapsed = u64::try_from(job.started.elapsed().as_millis()).unwrap_or(u64::MAX);
    let remaining = job.total.saturating_sub(job.completed);
    let eta = match job.status {
        JobStatus::Running => eta_ms(&job.durations_ms, remaining),
        JobStatus::Queued { position } => {
            let ahead: Vec<Option<u64>> = jobs
                .values()
                .filter(|other| other.status == JobStatus::Running)
                .map(|other| {
                    eta_ms(
                        &other.durations_ms,
                        other.total.saturating_sub(other.completed),
                    )
                })
                .collect();
            queue_wait_ms(&ahead, position)
        }
        _ => None,
    };
    JobView {
        id: job.id,
        status: job.status.clone(),
        sut_name: job.sut_name.clone(),
        completed: job.completed,
        total: job.total,
        current_case: job.current_case.clone(),
        elapsed_ms: elapsed,
        eta_ms: eta,
        tail: job.tail.iter().cloned().collect(),
        finished: job.finished.clone(),
    }
}

/// How many runs are driving right now.
#[cfg(feature = "ssr")]
fn running_count(jobs: &Jobs) -> usize {
    jobs.values()
        .filter(|job| job.status == JobStatus::Running)
        .count()
}

/// The run this submitter already has in flight, when the per-submitter cap
/// is already met.
///
/// The oldest such run, because that is the one the screen sends them back
/// to.
#[cfg(feature = "ssr")]
fn in_flight_of(jobs: &Jobs, submitter: Submitter, limits: &Limits) -> Option<RunId> {
    let mut mine: Vec<(u64, RunId)> = jobs
        .iter()
        .filter(|(_, job)| job.submitter == submitter && !job.status.is_terminal())
        .map(|(id, job)| (job.seq, *id))
        .collect();
    if mine.len() < limits.max_per_submitter {
        return None;
    }
    mine.sort_unstable();
    mine.first().map(|(_, id)| *id)
}

/// The oldest queued run, which is the next to start.
#[cfg(feature = "ssr")]
fn next_queued(jobs: &Jobs) -> Option<RunId> {
    jobs.iter()
        .filter(|(_, job)| matches!(job.status, JobStatus::Queued { .. }))
        .min_by_key(|(_, job)| job.seq)
        .map(|(id, _)| *id)
}

/// Restates every queued run's place, 1 for the next to start.
///
/// Called after any change to queue membership, so a stored position is never
/// stale — which is the only way a position and the queue can disagree.
#[cfg(feature = "ssr")]
fn renumber(jobs: &mut Jobs) {
    let mut queued: Vec<(u64, RunId)> = jobs
        .iter()
        .filter(|(_, job)| matches!(job.status, JobStatus::Queued { .. }))
        .map(|(id, job)| (job.seq, *id))
        .collect();
    queued.sort_unstable();
    for (index, (_, id)) in queued.into_iter().enumerate() {
        if let Some(job) = jobs.get_mut(&id) {
            let position = u32::try_from(index.saturating_add(1)).unwrap_or(u32::MAX);
            job.status = JobStatus::Queued { position };
        }
    }
}

/// Drops the oldest terminal runs past the keep-count.
///
/// The memory goes; the artifacts stay. An evicted id still resolves through
/// its own directory under the mounted output tree, which is exactly what
/// #386's recorded state exists for.
#[cfg(feature = "ssr")]
fn evict(jobs: &mut Jobs, limits: &Limits) {
    let mut terminal: Vec<(Option<Instant>, u64, RunId)> = jobs
        .iter()
        .filter(|(_, job)| job.status.is_terminal())
        .map(|(id, job)| (job.finished_at, job.seq, *id))
        .collect();
    let excess = terminal.len().saturating_sub(limits.finished_kept);
    if excess == 0 {
        return;
    }
    terminal.sort_unstable_by(|a, b| a.0.cmp(&b.0).then(a.1.cmp(&b.1)));
    for (_, _, id) in terminal.into_iter().take(excess) {
        jobs.remove(&id);
    }
}

/// Spawns the named run's engine and the two threads that watch it.
///
/// The pending start is CONSUMED: a spawn that fails cannot be retried from
/// this entry, and the caller decides whether that is a refusal to the
/// operator (at start) or a failed run (at promotion).
///
/// # Errors
/// The engine's own spawn refusal.
#[cfg(feature = "ssr")]
fn begin(
    state: &Arc<Mutex<Jobs>>,
    jobs: &mut Jobs,
    id: RunId,
    limits: Limits,
) -> Result<(), crate::engine::Error> {
    let Some(job) = jobs.get_mut(&id) else {
        return Ok(());
    };
    let Some(pending) = job.pending.take() else {
        return Ok(());
    };
    let running = pending.engine.spawn(&pending.spec)?;
    let now = Instant::now();
    job.canceller = Some(running.canceller());
    job.started = now;
    job.last_case_at = now;
    job.status = JobStatus::Running;
    let out_dir = job.out_dir.clone();

    // The supervising thread owns the stream; the server fns only ever take
    // the lock briefly. A thread rather than a tokio task because the engine
    // stream is blocking I/O end to end.
    let supervised = Arc::clone(state);
    std::thread::spawn(move || supervise(&supervised, id, running, &out_dir, limits));
    let watched = Arc::clone(state);
    std::thread::spawn(move || watchdog(&watched, id, limits));
    Ok(())
}

/// Streams one run to its end, then frees its slot.
#[cfg(feature = "ssr")]
fn supervise(
    state: &Arc<Mutex<Jobs>>,
    id: RunId,
    running: crate::engine::RunningEngine,
    out_dir: &std::path::Path,
    limits: Limits,
) {
    let on_line = |line: Line| {
        let Ok(mut jobs) = state.lock() else { return };
        let Some(job) = jobs.get_mut(&id) else { return };
        record_line(job, line);
    };
    let outcome = running.stream(on_line);
    let mut expired = false;
    if let Ok(mut jobs) = state.lock() {
        if let Some(job) = jobs.get_mut(&id) {
            job.canceller = None;
            let (status, finished) = finish_status(outcome, job.cancel);
            expired = status == JobStatus::Expired;
            if let Some(view) = finished {
                job.finished = Some(view);
            }
            job.status = status;
            job.finished_at = Some(Instant::now());
        }
        promote(state, &mut jobs, limits);
        renumber(&mut jobs);
        evict(&mut jobs, &limits);
    }
    if expired {
        // NOTE: no openEHR spec governs this — our own design; a run the cap
        // ended graded nothing, so its half-written directory is discarded
        // rather than presented as a record.
        drop(std::fs::remove_dir_all(out_dir));
    }
}

/// Ends the named run when it outlives the wall-clock cap.
///
/// Its own timer, because a hung run emits no output and a check inside the
/// streaming loop would never run.
#[cfg(feature = "ssr")]
fn watchdog(state: &Arc<Mutex<Jobs>>, id: RunId, limits: Limits) {
    loop {
        std::thread::sleep(limits.watchdog_tick);
        let canceller = {
            let Ok(mut jobs) = state.lock() else { return };
            let Some(job) = jobs.get_mut(&id) else { return };
            if job.status != JobStatus::Running {
                return;
            }
            if job.started.elapsed() < limits.wall_clock {
                continue;
            }
            job.cancel = Some(CancelCause::WallClock);
            job.canceller.clone()
        };
        if let Some(canceller) = canceller {
            drop(canceller.cancel());
        }
        return;
    }
}

/// Starts queued runs into every free slot, oldest first.
#[cfg(feature = "ssr")]
fn promote(state: &Arc<Mutex<Jobs>>, jobs: &mut Jobs, limits: Limits) {
    while running_count(jobs) < limits.max_concurrent {
        let Some(id) = next_queued(jobs) else { return };
        if let Err(e) = begin(state, jobs, id, limits) {
            // Nobody is waiting on this promotion's answer, so the refusal
            // becomes the run's own recorded failure.
            if let Some(job) = jobs.get_mut(&id) {
                job.status = JobStatus::Failed(e.to_string());
                job.finished_at = Some(Instant::now());
            }
        }
    }
}

#[cfg(feature = "ssr")]
impl JobSlot {
    /// A run map enforcing the given caps.
    ///
    /// The server always builds [`Limits::default`]; this exists so a test
    /// can drive a cap whose real value is longer than a test may take.
    #[must_use]
    pub fn with_limits(limits: Limits) -> Self {
        Self {
            state: Arc::new(Mutex::new(Jobs::new())),
            limits,
        }
    }

    /// The caps this map enforces.
    #[must_use]
    pub fn limits(&self) -> Limits {
        self.limits
    }

    /// The map's guard, or the poison's own words.
    fn lock(&self) -> Result<std::sync::MutexGuard<'_, Jobs>, JobError> {
        self.state
            .lock()
            .map_err(|poison| JobError::Poisoned(poison.to_string()))
    }

    /// Allocates this run's id — the caller derives the output directory from
    /// it BEFORE starting, so the artifacts' home carries the id.
    ///
    /// Infallible: minting a UUID takes no lock and reads no shared state.
    #[expect(
        clippy::unused_self,
        reason = "run identity belongs to the map that will hold the run, and a caller must not mint one against some other map"
    )]
    #[must_use]
    pub fn allocate_id(&self) -> RunId {
        RunId::mint()
    }

    /// Accepts a run under a previously allocated id.
    ///
    /// The submitter's own cap is checked, then the concurrency cap: within
    /// the free slots the run spawns immediately, and beyond them it is
    /// QUEUED — accepted, addressable at once, with its place stated. Both
    /// caps are read and acted on under one lock, so two simultaneous starts
    /// cannot both pass them.
    ///
    /// # Errors
    /// [`JobError::Busy`] naming the run this submitter already has,
    /// [`JobError::Poisoned`], and the engine's own spawn refusals.
    pub fn start(
        &self,
        id: RunId,
        submitter: Submitter,
        engine: &Engine,
        spec: RunSpec,
        sut_name: String,
    ) -> Result<RunId, JobError> {
        let mut jobs = self.lock()?;
        if let Some(existing) = in_flight_of(&jobs, submitter, &self.limits) {
            return Err(JobError::Busy(existing));
        }
        let now = Instant::now();
        let out_dir = spec.out_dir.clone();
        jobs.insert(
            id,
            JobState {
                id,
                submitter,
                seq: NEXT_SEQ.fetch_add(1, Ordering::Relaxed),
                status: JobStatus::Queued { position: 0 },
                sut_name,
                completed: 0,
                total: 0,
                current_case: None,
                started: now,
                last_case_at: now,
                durations_ms: Vec::new(),
                tail: std::collections::VecDeque::new(),
                finished: None,
                finished_at: None,
                canceller: None,
                cancel: None,
                out_dir,
                pending: Some(Pending {
                    engine: engine.clone(),
                    spec,
                }),
            },
        );
        if running_count(&jobs) < self.limits.max_concurrent
            && let Err(e) = begin(&self.state, &mut jobs, id, self.limits)
        {
            jobs.remove(&id);
            return Err(JobError::Engine(e));
        }
        renumber(&mut jobs);
        evict(&mut jobs, &self.limits);
        Ok(id)
    }

    /// Cancels the NAMED run.
    ///
    /// A queued run is removed outright, so it never spawns a process at all;
    /// a driving one is killed and the supervising thread records the state.
    /// A run this map does not hold, or one that already stopped, is
    /// [`JobError::Idle`]: cancel addresses one run, never whatever happens
    /// to be in flight.
    ///
    /// # Errors
    /// [`JobError::Idle`] when the named run is not in flight here,
    /// [`JobError::Poisoned`], and the kill's own failure.
    pub fn cancel(&self, id: RunId) -> Result<(), JobError> {
        let canceller = {
            let mut jobs = self.lock()?;
            let job = jobs.get_mut(&id).ok_or(JobError::Idle)?;
            match job.status {
                JobStatus::Queued { .. } => {
                    jobs.remove(&id);
                    renumber(&mut jobs);
                    return Ok(());
                }
                JobStatus::Running => {}
                _ => return Err(JobError::Idle),
            }
            job.cancel = Some(CancelCause::Operator);
            job.canceller.clone().ok_or(JobError::Idle)?
        };
        canceller.cancel().map_err(JobError::Engine)
    }

    /// The live view of the NAMED run: `Some` only when this map holds it.
    ///
    /// The call the live screen makes, so the screen can tell "this process
    /// is driving the run you asked about" from "this process never heard of
    /// it".
    ///
    /// # Errors
    /// [`JobError::Poisoned`] only.
    pub fn view_of(&self, id: RunId) -> Result<Option<JobView>, JobError> {
        let jobs = self.lock()?;
        Ok(jobs.get(&id).map(|job| snapshot(&jobs, job)))
    }

    /// The ONE per-submitter lookup: this submitter's most recent run of the
    /// asked-for kind.
    ///
    /// Every seam that once asked "what is the current run" asks this
    /// instead, so the record, export and live screens read one claim through
    /// one path (#134).
    ///
    /// # Errors
    /// [`JobError::Poisoned`] only.
    pub fn latest_of(&self, submitter: Submitter, want: Latest) -> Result<Option<RunId>, JobError> {
        let jobs = self.lock()?;
        Ok(jobs
            .values()
            .filter(|job| job.submitter == submitter)
            .filter(|job| match want {
                Latest::Any => true,
                Latest::Finished => job.finished.is_some(),
            })
            .max_by_key(|job| job.seq)
            .map(|job| job.id))
    }

    /// The run this submitter already has in flight, when their cap is met.
    ///
    /// The same `in_flight_of` decision [`JobSlot::start`] enforces under the
    /// lock, offered as a pre-flight so a caller writes nothing for a run
    /// that will be refused. It is never the guarantee: only the check inside
    /// the lock is.
    ///
    /// # Errors
    /// [`JobError::Poisoned`] only.
    pub fn in_flight_of(&self, submitter: Submitter) -> Result<Option<RunId>, JobError> {
        let jobs = self.lock()?;
        Ok(in_flight_of(&jobs, submitter, &self.limits))
    }

    /// How many runs are driving right now, for the caps' own tests.
    ///
    /// # Errors
    /// [`JobError::Poisoned`] only.
    pub fn running(&self) -> Result<usize, JobError> {
        let jobs = self.lock()?;
        Ok(running_count(&jobs))
    }

    /// Every run id the map still holds, live or remembered.
    ///
    /// The sweeper's exclusion set: a directory named here is read by the live
    /// screen and is never removed, whatever its age.
    ///
    /// # Errors
    /// [`JobError::Poisoned`] only.
    pub fn live_ids(&self) -> Result<std::collections::BTreeSet<RunId>, JobError> {
        let jobs = self.lock()?;
        Ok(jobs.keys().copied().collect())
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
    use super::{RunId, RunIdError, eta_ms, parse_progress, queue_wait_ms};

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

    /// The queued run's wait is the slot it is actually waiting for, and it
    /// is `None` rather than a guess whenever anything ahead is unknown.
    #[test]
    fn the_queue_wait_is_the_nth_slot_to_free() {
        assert_eq!(queue_wait_ms(&[Some(9_000), Some(4_000)], 1), Some(4_000));
        assert_eq!(queue_wait_ms(&[Some(9_000), Some(4_000)], 2), Some(9_000));
        assert_eq!(queue_wait_ms(&[Some(9_000), Some(4_000)], 3), None);
        assert_eq!(queue_wait_ms(&[Some(9_000), None], 1), None);
        assert_eq!(queue_wait_ms(&[], 1), None);
        assert_eq!(queue_wait_ms(&[Some(1)], 0), None);
    }
}

#[cfg(all(test, feature = "ssr"))]
mod map_tests {
    //! The caps and the map's own bookkeeping, driven over the map rather
    //! than asserted as numbers. No engine is spawned: what these exercise is
    //! the admission, queue and eviction decisions themselves, which is where
    //! every cap is actually enforced.

    use super::{
        FinishedView, JobState, JobStatus, Jobs, Limits, RunId, evict, in_flight_of, next_queued,
        renumber, running_count,
    };
    use crate::submitter::Submitter;
    use std::net::{IpAddr, Ipv4Addr};
    use std::time::Instant;

    /// One submitter, by its last octet.
    fn who(octet: u8) -> Submitter {
        Submitter::Peer(IpAddr::V4(Ipv4Addr::new(198, 51, 100, octet)))
    }

    /// One run in the map, with no engine behind it.
    fn park(jobs: &mut Jobs, seq: u64, submitter: Submitter, status: JobStatus) -> RunId {
        let id = RunId::mint();
        let now = Instant::now();
        let terminal = status.is_terminal();
        jobs.insert(
            id,
            JobState {
                id,
                submitter,
                seq,
                status,
                sut_name: String::from("parked"),
                completed: 0,
                total: 0,
                current_case: None,
                started: now,
                last_case_at: now,
                durations_ms: Vec::new(),
                tail: std::collections::VecDeque::new(),
                finished: None,
                finished_at: terminal.then_some(now),
                canceller: None,
                cancel: None,
                out_dir: std::path::PathBuf::from("/nonexistent"),
                pending: None,
            },
        );
        id
    }

    /// The concurrency cap: with the ceiling reached, the next start is
    /// QUEUED rather than refused, and the one after it queues behind that.
    #[test]
    fn the_concurrency_cap_queues_rather_than_refusing() {
        let limits = Limits::default();
        let mut jobs = Jobs::new();
        for seq in 0..u64::try_from(limits.max_concurrent).unwrap_or(0) {
            park(&mut jobs, seq, who(1), JobStatus::Running);
        }
        assert_eq!(
            running_count(&jobs),
            limits.max_concurrent,
            "the ceiling is reached, so the next start cannot spawn"
        );
        let first = park(&mut jobs, 90, who(2), JobStatus::Queued { position: 0 });
        let second = park(&mut jobs, 91, who(3), JobStatus::Queued { position: 0 });
        renumber(&mut jobs);
        assert_eq!(
            jobs.get(&first).map(|job| job.status.clone()),
            Some(JobStatus::Queued { position: 1 })
        );
        assert_eq!(
            jobs.get(&second).map(|job| job.status.clone()),
            Some(JobStatus::Queued { position: 2 })
        );
        assert_eq!(next_queued(&jobs), Some(first), "the queue is first-in");
    }

    /// The per-submitter cap: a second start from the same address is refused
    /// NAMING the run they already have, whether it is driving or queued, and
    /// another visitor is unaffected.
    #[test]
    fn the_per_submitter_cap_names_the_run_they_already_have() {
        let limits = Limits::default();
        let mut jobs = Jobs::new();
        assert_eq!(in_flight_of(&jobs, who(1), &limits), None);

        let mine = park(&mut jobs, 0, who(1), JobStatus::Running);
        assert_eq!(in_flight_of(&jobs, who(1), &limits), Some(mine));
        assert_eq!(in_flight_of(&jobs, who(2), &limits), None);

        let queued = park(&mut jobs, 1, who(2), JobStatus::Queued { position: 1 });
        assert_eq!(
            in_flight_of(&jobs, who(2), &limits),
            Some(queued),
            "a queued run counts as in flight"
        );
    }

    /// A run that stopped holds nothing: the submitter may start again and
    /// the slot is free.
    #[test]
    fn a_terminal_run_holds_no_slot_and_no_submitter() {
        let limits = Limits::default();
        let mut jobs = Jobs::new();
        for status in [
            JobStatus::Finished,
            JobStatus::Cancelled,
            JobStatus::Expired,
            JobStatus::Failed(String::from("boom")),
        ] {
            let mut one = Jobs::new();
            park(&mut one, 0, who(1), status.clone());
            assert_eq!(in_flight_of(&one, who(1), &limits), None, "{status:?}");
            assert_eq!(running_count(&one), 0, "{status:?}");
        }
        park(&mut jobs, 0, who(1), JobStatus::Finished);
        assert_eq!(next_queued(&jobs), None);
    }

    /// Eviction keeps the most recent terminal runs and drops the oldest,
    /// and it never touches a run that is still queued or driving.
    /// The sweeper's four answers, over a real output tree: a live run is kept
    /// whatever its age, an old one goes, a young one stays, and something that
    /// is not a job directory at all is never touched.
    #[cfg(feature = "ssr")]
    #[expect(
        clippy::panic_in_result_fn,
        reason = "the Book ch11 Result-returning test shape: assertions panic, plumbing propagates with ?"
    )]
    #[test]
    fn the_sweeper_keeps_what_is_live_and_what_is_young() -> Result<(), std::io::Error> {
        use std::collections::BTreeSet;
        use std::time::Duration;

        let out = std::env::temp_dir().join(format!("veredictum-sweep-{}", std::process::id()));
        std::fs::create_dir_all(&out)?;

        let live_id = RunId::mint();
        let old_id = RunId::mint();
        let young_id = RunId::mint();
        for id in [live_id, old_id, young_id] {
            std::fs::create_dir_all(super::job_dir(&out, id))?;
        }
        // An operator's own file under the output mount, which the sweeper must
        // leave alone because it is not a job directory.
        let theirs = out.join("notes.txt");
        std::fs::write(&theirs, b"mine")?;

        let live: BTreeSet<RunId> = [live_id].into_iter().collect();

        // A zero keep-window makes every directory "old", so the live set is
        // the only thing that can save one — which is the property under test.
        let swept = super::sweep_artifacts(&out, Duration::ZERO, &live);
        assert_eq!(swept.live, 1, "the live run's directory is kept: {swept:?}");
        assert_eq!(swept.removed, 2, "the other two go: {swept:?}");
        assert!(super::job_dir(&out, live_id).is_dir());
        assert!(!super::job_dir(&out, old_id).exists());
        assert!(
            theirs.is_file(),
            "a file that is not a job directory is untouched"
        );

        // And with a window nothing has outlived, a directory the map has
        // forgotten still stays.
        std::fs::create_dir_all(super::job_dir(&out, young_id))?;
        let swept = super::sweep_artifacts(&out, Duration::from_hours(1), &BTreeSet::new());
        assert_eq!(swept.removed, 0, "{swept:?}");
        assert_eq!(swept.young, 2, "{swept:?}");

        std::fs::remove_dir_all(&out)?;
        Ok(())
    }

    #[test]
    fn eviction_drops_the_oldest_finished_runs_first() {
        let limits = Limits {
            finished_kept: 2,
            ..Limits::default()
        };
        let mut jobs = Jobs::new();
        let running = park(&mut jobs, 0, who(1), JobStatus::Running);
        let queued = park(&mut jobs, 1, who(2), JobStatus::Queued { position: 1 });
        let oldest = park(&mut jobs, 2, who(3), JobStatus::Finished);
        let middle = park(&mut jobs, 3, who(4), JobStatus::Finished);
        let newest = park(&mut jobs, 4, who(5), JobStatus::Finished);
        // The parked runs share an instant, so the arrival counter is what
        // orders them — which is the tiebreak the real map relies on too.
        evict(&mut jobs, &limits);
        assert!(!jobs.contains_key(&oldest), "the oldest terminal run goes");
        for kept in [running, queued, middle, newest] {
            assert!(jobs.contains_key(&kept), "{kept} was evicted");
        }
    }

    /// An evicted run is forgotten by the map alone: its artifacts stay where
    /// the run wrote them, which is why an evicted id still resolves through
    /// its own directory.
    #[expect(
        clippy::panic_in_result_fn,
        reason = "the Book ch11 Result-returning test shape: assertions panic, plumbing propagates with ?"
    )]
    #[test]
    fn eviction_forgets_memory_and_never_an_artifact() -> Result<(), Box<dyn std::error::Error>> {
        let scratch = assert_fs::TempDir::new()?;
        let dir = super::job_dir(scratch.path(), RunId::mint());
        std::fs::create_dir_all(&dir)?;
        std::fs::write(dir.join("results.json"), b"{}")?;

        let limits = Limits {
            finished_kept: 0,
            ..Limits::default()
        };
        let mut jobs = Jobs::new();
        let id = park(&mut jobs, 0, who(1), JobStatus::Finished);
        if let Some(job) = jobs.get_mut(&id) {
            job.finished = Some(FinishedView {
                passed: 1,
                failed: 0,
                errored: 0,
                not_applicable: 0,
                results_path: dir.join("results.json").display().to_string(),
            });
            job.out_dir = dir.clone();
        }
        evict(&mut jobs, &limits);
        assert!(jobs.is_empty(), "the memory goes");
        assert!(
            dir.join("results.json").is_file(),
            "the artifacts stay: {}",
            dir.display()
        );
        Ok(())
    }
}
