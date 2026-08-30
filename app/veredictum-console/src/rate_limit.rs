// SPDX-FileCopyrightText: Veredictum contributors
// SPDX-License-Identifier: Apache-2.0

//! Per-submitter rate limits (#390) on the two things a visitor can make this
//! console spend: the reachability probe, and starting a run.
//!
//! The identity is the one [`crate::submitter`] already derives for the
//! concurrency caps (#389) — the peer address, or the address the header the
//! operator named carries. It is weak on purpose and it is never an
//! authentication claim; it bounds accidental and casual load.
//!
//! The caps in [`run_job`](crate::run_job) bound how much work exists AT ONCE.
//! These bound how OFTEN one visitor may ask for more, which is the half a
//! start-and-cancel loop walks straight through.
//!
//! Both limits apply in every posture, exactly as the concurrency caps do: a
//! limit that only a public instance enforces is a limit no gate ever drives.
//! The numbers are set so no operator and no browser journey reaches one.

use std::collections::BTreeMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use crate::submitter::Submitter;

// ── The rates (#390) ────────────────────────────────────────────────────────
// Starting values to re-derive by MEASURING on the chosen host, never
// constants to defend — the same shape the concurrency caps have. Each is
// driven by a test over an injected clock rather than by sleeping through a
// window.

/// The window each budget below is counted over.
pub const RATE_WINDOW: Duration = Duration::from_mins(1);

/// How many reachability probes one submitter may make per [`RATE_WINDOW`].
///
/// The probe is one GET to a server the visitor named, so its cost lands on
/// that server as much as on this one. Generous enough that composing a
/// connection, mistyping it and retrying never reaches it.
pub const MAX_PROBES_PER_WINDOW: u32 = 30;

/// How many run starts one submitter may make per [`RATE_WINDOW`].
///
/// A start spawns an engine process that loads the whole catalogue.
/// `MAX_RUNS_PER_SUBMITTER` already bounds how many run AT ONCE; this bounds
/// a start-then-cancel loop, which that cap alone does not.
pub const MAX_STARTS_PER_WINDOW: u32 = 10;

/// How many submitters the ledger tracks before the coldest are dropped.
///
/// The ledger is keyed by an address a visitor supplies, so it is bounded for
/// the same reason the job and draft maps are. A dropped entry only forgets a
/// partial window.
pub const TRACKED_SUBMITTERS: usize = 4_096;

/// What a visitor can make this console spend.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Metered {
    /// The reachability probe (`run_api::read::probe`).
    Probe,
    /// Starting a run, which spawns an engine process.
    Start,
}

impl Metered {
    /// What the refusal calls this action.
    #[must_use]
    pub fn label(self) -> &'static str {
        match self {
            Self::Probe => "connection probes",
            Self::Start => "run starts",
        }
    }
}

impl std::fmt::Display for Metered {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.label())
    }
}

/// The budgets one [`RateLimiter`] enforces.
///
/// [`Rates::default`] is the constants block above, which is what the server
/// runs. A test injects smaller numbers to drive a burst without making the
/// suite issue thirty requests.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Rates {
    /// [`MAX_PROBES_PER_WINDOW`].
    pub probes: u32,
    /// [`MAX_STARTS_PER_WINDOW`].
    pub starts: u32,
    /// [`RATE_WINDOW`].
    pub window: Duration,
    /// [`TRACKED_SUBMITTERS`].
    pub tracked: usize,
}

impl Default for Rates {
    fn default() -> Self {
        Self {
            probes: MAX_PROBES_PER_WINDOW,
            starts: MAX_STARTS_PER_WINDOW,
            window: RATE_WINDOW,
            tracked: TRACKED_SUBMITTERS,
        }
    }
}

impl Rates {
    /// The budget for one metered action.
    #[must_use]
    pub fn budget(self, what: Metered) -> u32 {
        match what {
            Metered::Probe => self.probes,
            Metered::Start => self.starts,
        }
    }
}

/// Where a [`RateLimiter`] reads time.
///
/// The server runs [`Clock::monotonic`]. A test runs [`Clock::manual`] and
/// moves the hand itself, so the window reopening is ASSERTED rather than
/// slept through — the same reason `run_job::Limits` is injectable.
#[derive(Debug, Clone)]
pub enum Clock {
    /// `Instant`, counted from the moment the clock was made.
    Monotonic(Instant),
    /// A hand a test moves, in milliseconds from an arbitrary origin.
    Manual(Arc<AtomicU64>),
}

impl Default for Clock {
    fn default() -> Self {
        Self::monotonic()
    }
}

impl Clock {
    /// The clock the server runs.
    #[must_use]
    pub fn monotonic() -> Self {
        Self::Monotonic(Instant::now())
    }

    /// A clock that moves only when [`Clock::advance`] is called.
    #[must_use]
    pub fn manual() -> Self {
        Self::Manual(Arc::new(AtomicU64::new(0)))
    }

    /// Moves a manual clock forward.
    ///
    /// A monotonic clock moves by itself and this does nothing to one, which
    /// is why a test always builds its limiter with [`Clock::manual`].
    pub fn advance(&self, by: Duration) {
        if let Self::Manual(hand) = self {
            hand.fetch_add(millis(by), Ordering::Relaxed);
        }
    }

    /// The reading, in milliseconds from this clock's own origin.
    fn now_ms(&self) -> u64 {
        match self {
            Self::Monotonic(origin) => millis(origin.elapsed()),
            Self::Manual(hand) => hand.load(Ordering::Relaxed),
        }
    }
}

/// A duration in whole milliseconds, saturating rather than wrapping.
fn millis(of: Duration) -> u64 {
    u64::try_from(of.as_millis()).unwrap_or(u64::MAX)
}

/// One submitter's window for one metered action.
#[derive(Debug, Clone, Copy)]
struct Window {
    /// When the window opened, on the limiter's own clock.
    opened_ms: u64,
    /// How much of the budget it has spent.
    used: u32,
}

/// Why a metered request was not admitted.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum RateRefusal {
    /// The submitter has spent this window's budget.
    #[error(
        "too many {what} from this address: this instance allows {allowed} every {window_secs} seconds. Try again in {retry_in_secs} seconds."
    )]
    TooMany {
        /// Which metered action was refused.
        what: Metered,
        /// The budget for that action.
        allowed: u32,
        /// The window the budget is counted over, in seconds.
        window_secs: u64,
        /// How long until the window reopens, in seconds.
        retry_in_secs: u64,
    },
    /// The ledger's lock was poisoned by a panicking thread.
    #[error("the rate ledger is poisoned ({0}); restart the console")]
    Poisoned(String),
}

/// The per-submitter ledger the probe and the start seams ask before working.
#[derive(Debug, Clone)]
pub struct RateLimiter {
    windows: Arc<Mutex<BTreeMap<(Submitter, Metered), Window>>>,
    rates: Rates,
    clock: Clock,
}

impl Default for RateLimiter {
    fn default() -> Self {
        Self::with(Rates::default(), Clock::monotonic())
    }
}

impl RateLimiter {
    /// A ledger enforcing the given budgets off the given clock.
    #[must_use]
    pub fn with(rates: Rates, clock: Clock) -> Self {
        Self {
            windows: Arc::new(Mutex::new(BTreeMap::new())),
            rates,
            clock,
        }
    }

    /// The budgets this ledger enforces.
    #[must_use]
    pub fn rates(&self) -> Rates {
        self.rates
    }

    /// The clock this ledger reads, so a test can move it.
    #[must_use]
    pub fn clock(&self) -> &Clock {
        &self.clock
    }

    /// Spends one unit of this submitter's budget, or refuses with the retry.
    ///
    /// The decision and the spend happen under one lock, so two simultaneous
    /// requests from one submitter cannot both pass the last unit.
    ///
    /// # Errors
    /// [`RateRefusal::TooMany`] naming the action, the budget and when the
    /// window reopens, or [`RateRefusal::Poisoned`].
    pub fn admit(&self, who: Submitter, what: Metered) -> Result<(), RateRefusal> {
        let budget = self.rates.budget(what);
        let window_ms = millis(self.rates.window);
        let now = self.clock.now_ms();
        let mut ledger = self
            .windows
            .lock()
            .map_err(|poison| RateRefusal::Poisoned(poison.to_string()))?;
        let entry = ledger.entry((who, what)).or_insert(Window {
            opened_ms: now,
            used: 0,
        });
        if now.saturating_sub(entry.opened_ms) >= window_ms {
            entry.opened_ms = now;
            entry.used = 0;
        }
        if entry.used >= budget {
            let elapsed = now.saturating_sub(entry.opened_ms);
            let retry_ms = window_ms.saturating_sub(elapsed);
            return Err(RateRefusal::TooMany {
                what,
                allowed: budget,
                window_secs: window_ms.div_ceil(1_000),
                retry_in_secs: retry_ms.div_ceil(1_000),
            });
        }
        entry.used = entry.used.saturating_add(1);
        prune(&mut ledger, now, window_ms, self.rates.tracked);
        Ok(())
    }
}

/// Drops closed windows once the ledger is over its cap, coldest first.
///
/// Only a window that has fully expired is dropped, so pruning never forgives
/// a burst that is still in progress.
fn prune(
    ledger: &mut BTreeMap<(Submitter, Metered), Window>,
    now: u64,
    window_ms: u64,
    tracked: usize,
) {
    if ledger.len() <= tracked {
        return;
    }
    ledger.retain(|_, window| now.saturating_sub(window.opened_ms) < window_ms);
    let excess = ledger.len().saturating_sub(tracked);
    if excess == 0 {
        return;
    }
    let mut ages: Vec<(u64, (Submitter, Metered))> = ledger
        .iter()
        .map(|(key, window)| (window.opened_ms, *key))
        .collect();
    ages.sort_unstable();
    for (_, key) in ages.into_iter().take(excess) {
        ledger.remove(&key);
    }
}

#[cfg(test)]
#[expect(
    clippy::panic_in_result_fn,
    reason = "the Book ch11 Result-returning test shape: assertions panic, plumbing propagates with ? (https://doc.rust-lang.org/book/ch11-01-writing-tests.html)"
)]
mod tests {
    use super::{Clock, Metered, RateLimiter, RateRefusal, Rates};
    use crate::submitter::Submitter;
    use std::net::{IpAddr, Ipv4Addr};
    use std::time::Duration;

    /// Two distinct visitors, so a test asserts a per-submitter decision and
    /// never one address's budget leaking into another's.
    const ALICE: Submitter = Submitter::Peer(IpAddr::V4(Ipv4Addr::new(198, 51, 100, 7)));
    const BOB: Submitter = Submitter::Peer(IpAddr::V4(Ipv4Addr::new(203, 0, 113, 9)));

    /// A ledger with a tiny budget over a hand a test moves itself.
    fn ledger(probes: u32, starts: u32) -> RateLimiter {
        RateLimiter::with(
            Rates {
                probes,
                starts,
                window: Duration::from_mins(1),
                ..Rates::default()
            },
            Clock::manual(),
        )
    }

    /// A burst is refused once the budget is spent, and the refusal states
    /// when the visitor may try again.
    #[test]
    fn a_burst_is_refused_with_a_stated_retry() -> Result<(), RateRefusal> {
        let limiter = ledger(3, 1);
        for _ in 0..3 {
            limiter.admit(ALICE, Metered::Probe)?;
        }
        let refusal = limiter
            .admit(ALICE, Metered::Probe)
            .expect_err("the fourth probe must refuse");
        let RateRefusal::TooMany {
            what,
            allowed,
            window_secs,
            retry_in_secs,
        } = refusal
        else {
            panic!("expected a budget refusal, got {refusal:?}");
        };
        assert_eq!(what, Metered::Probe);
        assert_eq!(allowed, 3);
        assert_eq!(window_secs, 60);
        assert_eq!(retry_in_secs, 60, "no time has passed on the manual clock");
        assert!(refusal.to_string().contains("connection probes"));
        Ok(())
    }

    /// The window reopens, and it is ASSERTED by moving the clock rather than
    /// by sleeping a minute inside the suite.
    #[test]
    fn the_window_reopens_on_the_clock() -> Result<(), RateRefusal> {
        let limiter = ledger(2, 1);
        limiter.admit(ALICE, Metered::Probe)?;
        limiter.admit(ALICE, Metered::Probe)?;
        assert!(limiter.admit(ALICE, Metered::Probe).is_err());

        // Halfway through, the window is still the same one, and the stated
        // retry has come down with it.
        limiter.clock().advance(Duration::from_secs(30));
        let refusal = limiter
            .admit(ALICE, Metered::Probe)
            .expect_err("the window has not closed yet");
        assert_eq!(
            refusal,
            RateRefusal::TooMany {
                what: Metered::Probe,
                allowed: 2,
                window_secs: 60,
                retry_in_secs: 30,
            }
        );

        limiter.clock().advance(Duration::from_secs(30));
        limiter.admit(ALICE, Metered::Probe)?;
        Ok(())
    }

    /// One visitor's spent budget never refuses another's request, and the
    /// probe and start budgets are counted apart.
    #[test]
    fn budgets_are_per_submitter_and_per_action() -> Result<(), RateRefusal> {
        let limiter = ledger(1, 1);
        limiter.admit(ALICE, Metered::Probe)?;
        assert!(limiter.admit(ALICE, Metered::Probe).is_err());
        limiter.admit(BOB, Metered::Probe)?;
        limiter.admit(ALICE, Metered::Start)?;
        assert!(limiter.admit(ALICE, Metered::Start).is_err());
        limiter.admit(Submitter::Unknown, Metered::Start)?;
        Ok(())
    }

    /// The ledger is bounded: a flood of one-shot addresses does not grow it
    /// without limit once their windows have closed.
    #[test]
    fn the_ledger_is_bounded() -> Result<(), RateRefusal> {
        let limiter = RateLimiter::with(
            Rates {
                probes: 4,
                starts: 4,
                window: Duration::from_mins(1),
                tracked: 8,
            },
            Clock::manual(),
        );
        for octet in 0..64_u8 {
            let who = Submitter::Peer(IpAddr::V4(Ipv4Addr::new(198, 51, 100, octet)));
            limiter.admit(who, Metered::Probe)?;
            limiter.clock().advance(Duration::from_secs(61));
        }
        limiter.admit(ALICE, Metered::Probe)?;
        let held = limiter
            .windows
            .lock()
            .map_err(|poison| RateRefusal::Poisoned(poison.to_string()))?
            .len();
        assert!(held <= 8, "the ledger grew to {held} entries");
        Ok(())
    }
}
