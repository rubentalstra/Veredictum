// SPDX-FileCopyrightText: Veredictum contributors
// SPDX-License-Identifier: Apache-2.0

//! The engine: preflight, seed once, measure N times.
//!
//! The preflight proves the whole write-then-read path before any clock
//! starts: an authenticated read of the template list, the pack's template
//! upload, then one scratch EHR, one composition committed into it, and that
//! composition read back. A failure at any of those refuses the run with the
//! exchange named, so a half-measured document never exists.
//!
//! The measured dispatcher fires every arrival at its planned instant and
//! measures latency from that instant, so a stalled system shows the stall in
//! every arrival queued behind it. Warmup arrivals are dispatched and then
//! discarded, and the schedule is a pure function of the pack's seed, so two
//! repetitions offer the same work in the same order.
//!
//! The measured window is bracketed by the posture canaries
//! ([`crate::bench::posture`]): the declared profile is checked against the
//! running system after the seed phases and again after the last repetition,
//! and a reading that contradicts the declaration, or a pair that disagree,
//! refuses the run before any record exists.

#![expect(
    clippy::disallowed_types,
    reason = "the wire bodies this module offers a SUT are JSON documents; the composition fixture is stamped as a document and posted as bytes"
)]

use std::collections::BTreeMap;
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use std::sync::{LazyLock, Mutex, mpsc};
use std::time::{Duration, Instant};

use hdrhistogram::Histogram;
use reqwest::{Method, StatusCode};
use serde_json::{Value, json};

use crate::bench::client::{AuthKind, BenchClient, PreferReturn, created_identifier, query_value};
use crate::bench::compare::summarize;
use crate::bench::fingerprint::EnvironmentFingerprint;
use crate::bench::pack::{
    BenchOp, BenchPack, BenchPhase, Fixture, FixtureKind, MeasurePhase, SeedPhase, SweepPhase,
};
use crate::bench::posture::{AuthnMode, Bracket, CanaryTarget, PostureProfile, VersionSample};
use crate::bench::result::{
    BenchResult, ErrorClass, LoopRegime, MeasuredPhaseRecord, Methodology, OperationStats,
    PackRecord, RepetitionRecord, ScaleRecord, SeedPhaseRecord, SweepPhaseRecord, TargetRecord,
};
use crate::bench::{BOUNDARY_STATEMENT, BenchError, METHODOLOGY};

/// Latency histograms record microseconds in `1 us ..= 10 min` at 3
/// significant figures, the same bounds the measured-class instrument uses,
/// so a bench histogram and a class histogram are read the same way.
const HDR_MAX_US: u64 = 600_000_000;

/// The EHR-scoped projection [`BenchOp::AdhocQueryUid`] offers, bound through
/// `query_parameters` (openEHR QUERY `AQL` §Parameters, and ITS-REST
/// `query_execute_ad_hoc_query`).
const ADHOC_UID_AQL: &str = "SELECT c/uid/value FROM EHR e CONTAINS COMPOSITION c \
     WHERE e/ehr_id/value = $ehr_id LIMIT 10";

/// The point lookup [`BenchOp::AdhocQueryPointLookup`] offers: one composition
/// addressed by the identifier its commit disclosed, inside one EHR.
///
/// `COMPOSITION.uid.value` is the `/uid/value` path (openEHR QUERY `AQL`
/// §"openEHR path syntax"), and the standard predicate on the `EHR` class
/// expression is the spec's own scoping form (§Parameters, §FROM).
// NOTE: openEHR QUERY `AQL` §"openEHR path syntax" maps COMPOSITION.uid.value
// to /uid/value and settles nothing about which identifier a repository
// projects there, so a server projecting the versioned object answers no row.
const AQL_POINT_LOOKUP: &str = "SELECT c/uid/value FROM EHR e[ehr_id/value=$ehr_id] \
     CONTAINS COMPOSITION c WHERE c/uid/value = $uid";

/// The EHR-scoped scan [`BenchOp::AdhocQueryEhrScan`] offers: every
/// composition in one EHR, projected by uid and bounded by nothing.
const AQL_EHR_SCAN: &str =
    "SELECT c/uid/value FROM EHR e[ehr_id/value=$ehr_id] CONTAINS COMPOSITION c";

/// The ordered page [`BenchOp::AdhocQueryOrderedPage`] offers.
///
/// openEHR QUERY `AQL` §"ORDER BY" states that without the clause the result
/// has no ordering this specification defines, which is why a paged read
/// carries one.
const AQL_ORDERED_PAGE: &str = "SELECT c/uid/value, c/context/start_time/value \
     FROM EHR e CONTAINS COMPOSITION c ORDER BY c/context/start_time/value DESC";

/// The systolic magnitude leaf every value-reading class predicates on.
///
/// openEHR QUERY `AQL` §"openEHR path syntax" gives exactly this path for the
/// Systolic `DV_QUANTITY` of `openEHR-EHR-OBSERVATION.blood_pressure.v2`,
/// which is the observation the seeded Vital signs composition carries.
const SYSTOLIC_MAGNITUDE: &str =
    "o/data[at0001]/events[at0006]/data[at0003]/items[at0004]/value/magnitude";

/// The blood-pressure containment every value-reading class shares
/// (openEHR QUERY `AQL` §Containment).
const BLOOD_PRESSURE: &str = "CONTAINS OBSERVATION o[openEHR-EHR-OBSERVATION.blood_pressure.v2]";

/// The EHR-scoped magnitude predicate [`BenchOp::AdhocQueryFiltered`] offers.
static AQL_FILTERED: LazyLock<String> = LazyLock::new(|| {
    format!(
        "SELECT c/uid/value FROM EHR e[ehr_id/value=$ehr_id] CONTAINS COMPOSITION c \
         {BLOOD_PRESSURE} WHERE {SYSTOLIC_MAGNITUDE} >= $systolic"
    )
});

/// The same predicate with no EHR scope, which
/// [`BenchOp::AdhocQueryPopulation`] offers under a fetch bound.
static AQL_POPULATION: LazyLock<String> = LazyLock::new(|| {
    format!(
        "SELECT c/uid/value FROM EHR e CONTAINS COMPOSITION c {BLOOD_PRESSURE} \
         WHERE {SYSTOLIC_MAGNITUDE} >= $systolic"
    )
});

/// The aggregate over that population, which
/// [`BenchOp::AdhocQueryAggregate`] offers (openEHR QUERY `AQL` §COUNT:
/// "returns the number of values of given expression argument").
static AQL_AGGREGATE: LazyLock<String> = LazyLock::new(|| {
    format!(
        "SELECT COUNT(c/uid/value) FROM EHR e CONTAINS COMPOSITION c {BLOOD_PRESSURE} \
         WHERE {SYSTOLIC_MAGNITUDE} >= $systolic"
    )
});

/// The lowest systolic threshold a filtered arrival draws, in `mm[Hg]`.
const SYSTOLIC_FLOOR: u64 = 90;

/// How wide the drawn threshold band is, in `mm[Hg]`.
const SYSTOLIC_SPAN: u64 = 60;

/// Rows the unscoped population query asks for, through the `fetch` member of
/// the ad-hoc request body (ITS-REST
/// `specifications/operations/query_execute_adhoc_query_body.yaml`: "fetching
/// `fetch` numbers of rows from `offset`").
const POPULATION_FETCH: u64 = 50;

/// Rows one page of the ordered read asks for.
const PAGE_FETCH: u64 = 20;

/// How many distinct pages an ordered-page arrival draws from.
const ORDERED_PAGES: u64 = 10;

/// How many stamped composition variants a measured phase pre-renders. Every
/// write arrival draws one, so payload bytes vary without the hot path
/// paying for a serialization per arrival.
const PAYLOAD_VARIANTS: u64 = 16;

/// The systolic magnitude leaf a stamped variant redraws, as a JSON pointer
/// into the embedded composition (RFC 6901).
const SYSTOLIC: &str = "/content/0/data/events/0/data/items/0/value/magnitude";

/// The diastolic magnitude leaf, likewise.
const DIASTOLIC: &str = "/content/0/data/events/0/data/items/1/value/magnitude";

/// Stream separators, so the operation draw, the EHR draw, the composition
/// draw and the payload draw never correlate with one another.
const STREAM_OP: u64 = 0x6f70_6572_6174_696f;
const STREAM_EHR: u64 = 0x6568_725f_7461_7267;
const STREAM_COMPOSITION: u64 = 0x636f_6d70_5f74_6172;
const STREAM_PAYLOAD: u64 = 0x7061_796c_6f61_645f;
const STREAM_QUERY: u64 = 0x7175_6572_795f_7061;

/// What the caller asked the engine to do.
#[derive(Debug)]
pub struct BenchRun<'a> {
    /// The pack to drive.
    pub pack: &'a BenchPack,
    /// The system's base URL.
    pub base_url: &'a str,
    /// The posture profile this run declares. Exactly one, and the canaries
    /// check it against the running system before and after the measured
    /// window.
    pub profile: &'a PostureProfile,
    /// How the client presents itself.
    pub auth: AuthKind,
    /// The user `--auth basic` needs.
    pub user: Option<&'a str>,
    /// A secret supplied in process, which replaces the environment lookup
    /// the auth mode otherwise does. The baseline orchestration sets it to
    /// the credential of the stack it composed; a target run leaves it
    /// `None`.
    pub credential: Option<&'a str>,
    /// How many times to repeat the measured phases.
    pub repetitions: u32,
    /// The operator's label for this run.
    pub label: Option<&'a str>,
    /// Multiplies every seed phase's EHR count. `1.0` is the pack's pinned
    /// population, and any other value takes the run off the pack's reference
    /// configuration.
    pub scale: f64,
    /// Overrides every seed phase's declared worker count. `None` keeps what
    /// the pack declares.
    pub seed_workers: Option<usize>,
}

/// One composition the seed phase committed, and everything a read needs to
/// address it.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
struct SeededComposition {
    /// Which seeded EHR it was committed into.
    ehr_index: usize,
    /// The versioned-object uid, which the latest-version reads address.
    object_uid: String,
    /// The `OBJECT_VERSION_ID` the commit answered with, which the
    /// version-by-id read addresses.
    version_uid: String,
}

/// The corpus one seed phase left behind.
#[derive(Debug, Default)]
struct BenchCorpus {
    /// Every seeded EHR id, in creation order.
    ehr_ids: Vec<String>,
    /// Every committed composition, in creation order.
    compositions: Vec<SeededComposition>,
    /// The instant every `version_at_time` read addresses, captured once
    /// after the seed phases finished. Every seeded version predates it.
    version_at_time: String,
}

/// How many committed versions the signing canary is handed to sample.
const POSTURE_SAMPLES: usize = 3;

impl BenchCorpus {
    /// Versions the signing canary reads back, spread across the population.
    ///
    /// The samples come from the run's OWN seed traffic rather than from a
    /// commit made for the probe, so a signing scheme switched on around a
    /// dedicated write would not reach them.
    fn version_samples(&self) -> Vec<VersionSample> {
        let total = self.compositions.len();
        if total == 0 {
            return Vec::new();
        }
        let wanted = POSTURE_SAMPLES.min(total);
        let mut samples = Vec::with_capacity(wanted);
        for slot in 0..wanted {
            #[expect(
                clippy::integer_division,
                reason = "an even spread across the population: exact integer bucketing"
            )]
            let index = slot.saturating_mul(total) / wanted;
            let Some(composition) = self.compositions.get(index) else {
                continue;
            };
            let Some(ehr_id) = self.ehr_ids.get(composition.ehr_index) else {
                continue;
            };
            samples.push(VersionSample {
                ehr_id: ehr_id.clone(),
                object_uid: composition.object_uid.clone(),
                version_uid: composition.version_uid.clone(),
            });
        }
        samples
    }
}

/// The disclosure mode the run's own `--auth` choice declares.
const fn authn_of(auth: AuthKind) -> AuthnMode {
    match auth {
        AuthKind::None => AuthnMode::None,
        AuthKind::Basic => AuthnMode::Basic,
        AuthKind::Bearer => AuthnMode::Bearer,
    }
}

/// Whether a provisioning write landed in the created family.
///
/// With `Prefer: return=minimal` some systems answer `201 Created` and others
/// `204 No Content` with the identifying headers, and the bench engine
/// measures speed rather than adjudicating that choice. The functional
/// catalogue is where an exact status is pinned.
fn created(status: StatusCode) -> bool {
    status == StatusCode::CREATED || status == StatusCode::NO_CONTENT
}

/// The EHR count a scale factor asks a seed phase for.
///
/// The factor multiplies the EHR count only, never the compositions committed
/// into each one, so a scaled run keeps the per-EHR depth the pack pins and
/// shrinks the population instead.
///
/// # Errors
/// [`BenchError::Seed`] for a factor that is not a positive finite number.
fn scaled_ehrs(declared: usize, factor: f64) -> Result<usize, BenchError> {
    if !factor.is_finite() || factor <= 0.0 {
        return Err(BenchError::Seed {
            phase: "(scale)".to_owned(),
            detail: format!("--scale must be a positive finite number (got {factor})"),
        });
    }
    #[expect(
        clippy::as_conversions,
        clippy::cast_precision_loss,
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        reason = "an operator-scale EHR count times an operator-scale factor, rounded, far below 2^52"
    )]
    let scaled = (declared as f64 * factor).round() as usize;
    Ok(scaled.max(1))
}

/// Whether any phase in the pack drives a read parameterized by an instant,
/// which is what makes the captured instant worth recording.
fn reads_a_version_at_time(pack: &BenchPack) -> bool {
    let at_time = |op: &BenchOp| {
        matches!(
            op,
            BenchOp::GetCompositionAtTime | BenchOp::GetVersionedCompositionVersionAtTime
        )
    };
    pack.sweep_phases()
        .iter()
        .any(|sweep| sweep.per_composition.iter().any(at_time))
        || pack
            .measure_phases()
            .iter()
            .any(|phase| phase.mix.iter().any(|entry| at_time(&entry.op)))
}

/// The versioned-object part of an `OBJECT_VERSION_ID` (`uid::system::1`
/// becomes `uid`), which is what the latest-version read addresses.
fn object_uid_of(version_uid: &str) -> String {
    version_uid
        .split("::")
        .next()
        .unwrap_or(version_uid)
        .to_owned()
}

/// Executes one whole bench run and returns its record.
///
/// # Errors
/// [`BenchError::Repetitions`] for a repetition count below one,
/// [`BenchError::FixturePin`] when an embedded fixture moved,
/// [`BenchError::Preflight`] when the target does not answer the write-then-read
/// path, [`BenchError::Seed`] when the bulk load could not complete,
/// [`BenchError::Measure`] when a measured phase could not be aggregated, and
/// [`BenchError::PostureContradiction`] or [`BenchError::PostureFlip`] when a
/// canary disagrees with the declared profile or with its own other bracket.
pub fn execute(
    run: &BenchRun<'_>,
    progress: &(dyn Fn(String) + Sync),
) -> Result<BenchResult, BenchError> {
    if run.repetitions == 0 {
        return Err(BenchError::Repetitions(run.repetitions));
    }
    run.pack.verify_pins()?;
    if let Some(warning) = plain_http_credential_warning(run.base_url, run.auth) {
        progress(warning);
    }
    let client = BenchClient::with_credential(run.base_url, run.auth, run.user, run.credential)?;
    let started_at = jiff::Timestamp::now().to_string();

    progress("preflight: proving the write-then-read path".to_owned());
    preflight(&client, run.pack)?;
    let sut_version = probe_sut_version(&client);

    let mut corpus = BenchCorpus::default();
    let (seed_phases, declared_workers) = seed_all(&client, run, &mut corpus, progress)?;
    corpus.version_at_time = jiff::Timestamp::now().to_string();

    let raw = client.without_decompression()?;
    let anonymous = client.without_credential()?;
    let samples = corpus.version_samples();
    let canary = CanaryTarget {
        client: &client,
        raw: &raw,
        anonymous: &anonymous,
        profile: run.profile,
        authn: authn_of(run.auth),
        tls: crate::bench::posture::tls_of(&client.recorded_base_url()),
        invalid_twin: run.pack.invalid_twin(),
        samples: &samples,
    };
    progress(format!(
        "posture canaries: reading the `{}` declaration before the measured window",
        run.profile.name
    ));
    let before = crate::bench::posture::bracket(&canary, Bracket::Before);

    let measure_phases = run.pack.measure_phases();
    let sweep_phases = run.pack.sweep_phases();
    let mut repetitions = Vec::with_capacity(usize::try_from(run.repetitions).unwrap_or(1));
    for repetition in 1..=run.repetitions {
        repetitions.push(one_repetition(
            &client,
            run,
            repetition,
            &sweep_phases,
            &measure_phases,
            &corpus,
            progress,
        )?);
    }

    progress("posture canaries: re-reading the declaration after the measured window".to_owned());
    let after = crate::bench::posture::bracket(&canary, Bracket::After);
    let posture =
        crate::bench::posture::settle(run.profile, canary.authn, canary.tls, &before, &after)?;

    let cross = summarize(&repetitions);
    let mut result = BenchResult {
        schema_version: crate::schema::SCHEMA_VERSION.to_owned(),
        boundary_statement: BOUNDARY_STATEMENT.to_owned(),
        label: run.label.map(str::to_owned),
        pack: PackRecord::of(run.pack),
        target: TargetRecord {
            base_url: client.recorded_base_url(),
            sut_version,
        },
        environment: EnvironmentFingerprint::detect(),
        started_at,
        finished_at: jiff::Timestamp::now().to_string(),
        scale: ScaleRecord::new(run.scale, declared_workers),
        version_at_time: reads_a_version_at_time(run.pack).then(|| corpus.version_at_time.clone()),
        seed_phases,
        repetitions,
        cross,
        baselines: Vec::new(),
        relative: Vec::new(),
        methodology: Methodology {
            statement: METHODOLOGY.to_owned(),
            open_loop: true,
            coordinated_omission_free: true,
            seed_once_measure_n: true,
            repetitions: run.repetitions,
        },
        submittable: false,
        submittable_unmet: Vec::new(),
        posture,
    };
    result.settle_submittability();
    Ok(result)
}

/// Runs every seed phase the pack declares, extending the corpus in place.
///
/// Returns the phase records and whether every phase ran at the worker count
/// its pack declares, which is half of what puts a run on the pack's reference
/// configuration.
fn seed_all(
    client: &BenchClient,
    run: &BenchRun<'_>,
    corpus: &mut BenchCorpus,
    progress: &(dyn Fn(String) + Sync),
) -> Result<(Vec<SeedPhaseRecord>, bool), BenchError> {
    let mut seed_phases = Vec::new();
    let mut declared_workers = true;
    for phase in &run.pack.phases {
        let BenchPhase::Seed(seed) = phase else {
            continue;
        };
        let ehrs = scaled_ehrs(seed.ehrs, run.scale)?;
        let workers = run.seed_workers.unwrap_or(seed.workers).max(1);
        if workers != seed.workers {
            declared_workers = false;
        }
        progress(format!(
            "seed phase {}: {} EHRs x {} compositions on {} worker(s)",
            seed.name, ehrs, seed.compositions_per_ehr, workers
        ));
        let record = seed_phase(client, seed, ehrs, workers, corpus, progress)?;
        seed_phases.push(record);
    }
    if corpus.ehr_ids.is_empty() {
        return Err(BenchError::Seed {
            phase: "(none)".to_owned(),
            detail: "the pack seeded no EHR, so no measured phase has a target".to_owned(),
        });
    }
    Ok((seed_phases, declared_workers))
}

/// Executes one repetition: every closed-loop sweep, then every open-loop
/// measured phase, over the corpus the seed phases left behind.
fn one_repetition(
    client: &BenchClient,
    run: &BenchRun<'_>,
    repetition: u32,
    sweep_phases: &[&SweepPhase],
    measure_phases: &[&MeasurePhase],
    corpus: &BenchCorpus,
    progress: &(dyn Fn(String) + Sync),
) -> Result<RepetitionRecord, BenchError> {
    let mut sweeps = BTreeMap::new();
    for phase in sweep_phases {
        progress(format!(
            "repetition {repetition}/{}: sweep {} over {} composition(s) x {} request(s) on {} worker(s)",
            run.repetitions,
            phase.name,
            corpus.compositions.len(),
            phase.per_composition.len(),
            phase.workers
        ));
        let record = sweep_phase(client, phase, corpus, progress)?;
        let _replaced = sweeps.insert(phase.name.clone(), record);
    }
    let mut phases = BTreeMap::new();
    for (index, phase) in measure_phases.iter().enumerate() {
        progress(format!(
            "repetition {repetition}/{}: phase {} at {}/s for {}s warmup + {}s measured",
            run.repetitions, phase.name, phase.rate_per_s, phase.warmup_s, phase.duration_s
        ));
        let record = measure_phase(
            client,
            run.pack,
            phase,
            u64::try_from(index).unwrap_or(0),
            corpus,
            progress,
        )?;
        let _replaced = phases.insert(phase.name.clone(), record);
    }
    Ok(RepetitionRecord {
        repetition,
        phases,
        sweeps,
    })
}

/// The one-line warning for credentials over plain HTTP to a non-loopback
/// host, or `None` where the transport is the operator's own machine or
/// carries no credential.
///
/// The base URL is operator-supplied and a local quickstart is legitimately
/// `http://localhost`, so this never refuses the run — it says out loud what
/// the transport does with the credential (#296).
fn plain_http_credential_warning(base_url: &str, auth: AuthKind) -> Option<String> {
    if matches!(auth, AuthKind::None) {
        return None;
    }
    let rest = base_url.strip_prefix("http://")?;
    let authority = rest.split(['/', '?']).next().unwrap_or_default();
    // A bracketed IPv6 authority keeps its colons; otherwise the first colon
    // starts the port (RFC 3986 §3.2.2 host grammar).
    let host = authority
        .strip_prefix('[')
        .and_then(|inside| inside.split(']').next())
        .unwrap_or_else(|| authority.split(':').next().unwrap_or_default());
    let loopback = host == "localhost" || host.starts_with("127.") || host == "::1";
    if loopback {
        return None;
    }
    Some(format!(
        "warning: the credential rides plain http to {host} — every request sends it \
         unencrypted across the network; prefer an https base URL for any target that \
         is not this machine"
    ))
}

/// Refuses the run unless the whole write-then-read path answers.
///
/// # Errors
/// [`BenchError::Preflight`] naming the exchange that failed, or
/// [`BenchError::Transport`] when one never reached a response.
pub fn preflight(client: &BenchClient, pack: &BenchPack) -> Result<(), BenchError> {
    let templates = client.send(
        "template list",
        Method::GET,
        "/definition/template/adl1.4",
        None,
        PreferReturn::Unstated,
    )?;
    if !templates.status.is_success() {
        return Err(refused(
            "template list",
            format!(
                "GET /definition/template/adl1.4 answered {}",
                templates.status
            ),
        ));
    }

    let fixtures = pack.fixtures();
    for fixture in fixtures
        .iter()
        .filter(|fixture| fixture.kind == FixtureKind::OperationalTemplate)
    {
        upload_template(client, "template upload", fixture)
            .map_err(|detail| refused("template upload", detail))?;
    }

    let Some(composition) = fixtures
        .iter()
        .find(|fixture| fixture.kind == FixtureKind::Composition)
    else {
        return Ok(());
    };
    preflight_round_trip(client, composition)
}

/// A preflight refusal, named by the exchange it stopped at.
fn refused(exchange: &str, detail: String) -> BenchError {
    BenchError::Preflight {
        exchange: exchange.to_owned(),
        detail,
    }
}

/// Offers one operational template, tolerating an already-provisioned one.
///
/// A conflict means the template is already there, which is the ordinary
/// state of a system a bench run has touched before.
fn upload_template(
    client: &BenchClient,
    exchange: &'static str,
    fixture: &Fixture,
) -> Result<(), String> {
    let upload = client
        .send(
            exchange,
            Method::POST,
            "/definition/template/adl1.4",
            Some((fixture.kind.media_type(), fixture.bytes.as_bytes().to_vec())),
            PreferReturn::Unstated,
        )
        .map_err(|error| error.to_string())?;
    if created(upload.status) || upload.status == StatusCode::CONFLICT {
        return Ok(());
    }
    Err(format!(
        "POST /definition/template/adl1.4 for {} answered {} (201, 204 or 409 expected)",
        fixture.key, upload.status
    ))
}

/// Proves one scratch EHR, one commit into it, and the read back.
fn preflight_round_trip(client: &BenchClient, composition: &Fixture) -> Result<(), BenchError> {
    let ehr = client.send(
        "scratch ehr create",
        Method::POST,
        "/ehr",
        None,
        PreferReturn::Identifier,
    )?;
    if ehr.status != StatusCode::CREATED {
        return Err(refused(
            "scratch ehr create",
            format!("POST /ehr answered {} (201 expected)", ehr.status),
        ));
    }
    let ehr_id = created_identifier(&ehr).ok_or_else(|| {
        refused(
            "scratch ehr create",
            "the create disclosed no ehr_id: no uid body, no ETag and no Location".to_owned(),
        )
    })?;
    let commit = client.send(
        "scratch composition commit",
        Method::POST,
        &format!("/ehr/{ehr_id}/composition"),
        Some((
            composition.kind.media_type(),
            composition.bytes.as_bytes().to_vec(),
        )),
        PreferReturn::Identifier,
    )?;
    if !created(commit.status) {
        return Err(refused(
            "scratch composition commit",
            format!(
                "POST /ehr/{ehr_id}/composition answered {} (201 or 204 expected)",
                commit.status
            ),
        ));
    }
    let uid = created_identifier(&commit)
        .map(|version| object_uid_of(&version))
        .ok_or_else(|| {
            refused(
                "scratch composition commit",
                "the commit disclosed no version uid: no uid body, no ETag and no Location"
                    .to_owned(),
            )
        })?;
    let read = client.send(
        "scratch composition read",
        Method::GET,
        &format!("/ehr/{ehr_id}/composition/{uid}"),
        None,
        PreferReturn::Unstated,
    )?;
    if read.status != StatusCode::OK {
        return Err(refused(
            "scratch composition read",
            format!(
                "GET /ehr/{ehr_id}/composition/{uid} answered {} (200 expected)",
                read.status
            ),
        ));
    }
    Ok(())
}

/// The version the system discloses about itself, where it discloses one.
///
/// No openEHR specification defines a version-disclosure endpoint, so this is
/// our own best-effort probe over two shapes deployments commonly serve. A
/// system that answers neither is legitimately silent, which is `None` rather
/// than a run failure.
#[must_use]
pub fn probe_sut_version(client: &BenchClient) -> Option<String> {
    for path in ["/../system/info", "/"] {
        let Ok(reply) = client.send(
            "version probe",
            Method::GET,
            path,
            None,
            PreferReturn::Unstated,
        ) else {
            continue;
        };
        if !reply.status.is_success() {
            continue;
        }
        let Ok(document) = serde_json::from_slice::<Value>(&reply.body) else {
            continue;
        };
        for pointer in ["/solution_version", "/version", "/info/version"] {
            if let Some(version) = document.pointer(pointer).and_then(Value::as_str) {
                return Some(version.to_owned());
            }
        }
    }
    None
}

/// Runs one closed-loop bulk load, extending the corpus in place.
///
/// `ehrs` and `workers` are the EFFECTIVE values, after the run's scale factor
/// and any worker override, so what the record reports is what was executed.
fn seed_phase(
    client: &BenchClient,
    phase: &SeedPhase,
    ehrs: usize,
    workers: usize,
    corpus: &mut BenchCorpus,
    progress: &(dyn Fn(String) + Sync),
) -> Result<SeedPhaseRecord, BenchError> {
    let fail = |detail: String| BenchError::Seed {
        phase: phase.name.clone(),
        detail,
    };
    for fixture in &phase.fixtures {
        if fixture.kind != FixtureKind::OperationalTemplate {
            continue;
        }
        upload_template(client, "seed template upload", fixture).map_err(fail)?;
    }
    let composition = phase
        .fixtures
        .iter()
        .find(|fixture| fixture.kind == FixtureKind::Composition)
        .ok_or_else(|| fail("the phase declares no composition fixture to commit".to_owned()))?;

    let started = Instant::now();
    let workers = workers.max(1);
    let base = corpus.ehr_ids.len();
    let created_ehrs = seed_ehrs(client, ehrs, workers).map_err(fail)?;
    corpus.ehr_ids.extend(created_ehrs);
    progress(format!("seed phase {}: {ehrs} EHRs created", phase.name));

    let total = ehrs
        .checked_mul(phase.compositions_per_ehr)
        .ok_or_else(|| fail("the seed volume overflows".to_owned()))?;
    let mut compositions =
        seed_compositions(client, phase, composition, corpus, base, workers, total)
            .map_err(fail)?;
    if compositions.len() != total {
        return Err(fail(format!(
            "committed {} of {total} compositions",
            compositions.len()
        )));
    }
    compositions.sort();
    corpus.compositions.append(&mut compositions);

    let elapsed_s = started.elapsed().as_secs_f64();
    let writes = ehrs.saturating_add(total);
    #[expect(
        clippy::as_conversions,
        clippy::cast_precision_loss,
        reason = "seed volumes are far below 2^52"
    )]
    let (bulk_load_writes_per_s, whole_loop_ms_per_composition) = {
        let throughput = if elapsed_s > 0.0 {
            writes as f64 / elapsed_s
        } else {
            0.0
        };
        let per_composition = if total > 0 {
            elapsed_s * 1000.0 / total as f64
        } else {
            0.0
        };
        (throughput, per_composition)
    };
    Ok(SeedPhaseRecord {
        name: phase.name.clone(),
        regime: LoopRegime::ClosedLoop,
        ehrs: u64::try_from(ehrs).unwrap_or(u64::MAX),
        compositions_per_ehr: u64::try_from(phase.compositions_per_ehr).unwrap_or(u64::MAX),
        workers: u64::try_from(workers).unwrap_or(u64::MAX),
        elapsed_s,
        bulk_load_writes_per_s,
        whole_loop_ms_per_composition,
    })
}

/// Creates `count` EHRs across a closed worker pool, in creation order.
///
/// A worker that meets a failure stops rather than filling the log with the
/// same fault once per remaining slot; the first recorded fault is what the
/// caller reports.
fn seed_ehrs(client: &BenchClient, count: usize, workers: usize) -> Result<Vec<String>, String> {
    let slots: Vec<Mutex<Option<String>>> = (0..count).map(|_| Mutex::new(None)).collect();
    let failures: Mutex<Vec<String>> = Mutex::new(Vec::new());
    let next = AtomicUsize::new(0);
    std::thread::scope(|scope| {
        for _ in 0..workers {
            let _handle = scope.spawn(|| {
                loop {
                    let index = next.fetch_add(1, Ordering::Relaxed);
                    if index >= count {
                        break;
                    }
                    let outcome = client
                        .send(
                            "seed ehr create",
                            Method::POST,
                            "/ehr",
                            None,
                            PreferReturn::Identifier,
                        )
                        .map_err(|error| error.to_string())
                        .and_then(|reply| {
                            if reply.status != StatusCode::CREATED {
                                return Err(format!("create ehr answered {}", reply.status));
                            }
                            created_identifier(&reply)
                                .ok_or_else(|| "create ehr disclosed no ehr_id".to_owned())
                        });
                    match outcome {
                        Ok(id) => {
                            if let Some(Ok(mut slot)) = slots.get(index).map(Mutex::lock) {
                                *slot = Some(id);
                            }
                        }
                        Err(detail) => {
                            if let Ok(mut recorded) = failures.lock() {
                                recorded.push(detail);
                            }
                            break;
                        }
                    }
                }
            });
        }
    });
    if let Ok(recorded) = failures.lock()
        && let Some(first) = recorded.first()
    {
        return Err(format!("seeding EHRs: {first}"));
    }
    let mut ids = Vec::with_capacity(count);
    for slot in &slots {
        let id = slot
            .lock()
            .ok()
            .and_then(|guard| guard.clone())
            .ok_or_else(|| "seeding EHRs left a gap".to_owned())?;
        ids.push(id);
    }
    Ok(ids)
}

/// Commits the phase's compositions into the EHRs it just created.
///
/// `total` is the effective volume the caller computed from the scaled EHR
/// count, so this walk and the record agree about how much work was offered.
fn seed_compositions(
    client: &BenchClient,
    phase: &SeedPhase,
    composition: &Fixture,
    corpus: &BenchCorpus,
    base: usize,
    workers: usize,
    total: usize,
) -> Result<Vec<SeededComposition>, String> {
    let body = composition.bytes.as_bytes().to_vec();
    let committed: Mutex<Vec<SeededComposition>> = Mutex::new(Vec::with_capacity(total));
    let failures: Mutex<Vec<String>> = Mutex::new(Vec::new());
    let next = AtomicUsize::new(0);
    std::thread::scope(|scope| {
        for _ in 0..workers {
            let _handle = scope.spawn(|| {
                let mut local = Vec::new();
                loop {
                    let slot = next.fetch_add(1, Ordering::Relaxed);
                    if slot >= total {
                        break;
                    }
                    #[expect(
                        clippy::integer_division,
                        reason = "which EHR the slot-th commit belongs to: exact integer bucketing"
                    )]
                    let local_index = slot / phase.compositions_per_ehr.max(1);
                    let ehr_index = base.saturating_add(local_index);
                    let Some(ehr_id) = corpus.ehr_ids.get(ehr_index) else {
                        break;
                    };
                    let outcome = client
                        .send(
                            "seed composition commit",
                            Method::POST,
                            &format!("/ehr/{ehr_id}/composition"),
                            Some((composition.kind.media_type(), body.clone())),
                            PreferReturn::Identifier,
                        )
                        .map_err(|error| error.to_string())
                        .and_then(|reply| {
                            if !created(reply.status) {
                                return Err(format!("commit answered {}", reply.status));
                            }
                            created_identifier(&reply)
                                .ok_or_else(|| "commit disclosed no version uid".to_owned())
                        });
                    match outcome {
                        Ok(version) => local.push(SeededComposition {
                            ehr_index,
                            object_uid: object_uid_of(&version),
                            version_uid: version,
                        }),
                        Err(detail) => {
                            if let Ok(mut recorded) = failures.lock() {
                                recorded.push(detail);
                            }
                            break;
                        }
                    }
                }
                if let Ok(mut all) = committed.lock() {
                    all.append(&mut local);
                }
            });
        }
    });
    if let Ok(recorded) = failures.lock()
        && let Some(first) = recorded.first()
    {
        return Err(format!("seeding compositions: {first}"));
    }
    committed
        .into_inner()
        .map_err(|error| format!("seeding lock poisoned: {error}"))
}

/// One planned arrival on the open-loop schedule.
#[derive(Debug, Clone, Copy)]
struct PlannedArrival {
    /// Offset from the phase start.
    at: Duration,
    /// The operation this arrival offers.
    op: BenchOp,
    /// Whether this arrival lands inside the measured span.
    recorded: bool,
    /// The arrival's ordinal, which seeds its target draws.
    index: u64,
}

/// One completed arrival, as the collector sees it.
#[derive(Debug)]
struct Completion {
    op: BenchOp,
    latency_us: u64,
    class: Option<ErrorClass>,
    recorded: bool,
}

/// Builds one phase's arrival schedule. Deterministic in the pack seed and
/// the phase index, so every repetition offers the same work in the same
/// order.
fn build_schedule(pack: &BenchPack, phase: &MeasurePhase, phase_index: u64) -> Vec<PlannedArrival> {
    let total = phase.planned_arrivals();
    if total == 0 {
        return Vec::new();
    }
    let mut arrivals = Vec::with_capacity(usize::try_from(total).unwrap_or(0));
    for index in 0..total {
        #[expect(
            clippy::as_conversions,
            clippy::cast_precision_loss,
            reason = "the arrival ordinal is far below 2^52"
        )]
        let offset_s = index as f64 / phase.rate_per_s;
        let draw = crate::perf_run::fnv1a(pack.seed ^ STREAM_OP, &[phase_index, index]);
        let Some(op) = phase.op_for_draw(draw) else {
            continue;
        };
        arrivals.push(PlannedArrival {
            at: Duration::from_secs_f64(offset_s),
            op,
            recorded: phase.is_measured(index),
            index,
        });
    }
    arrivals
}

/// Pre-renders the stamped composition variants a write arrival draws from.
fn payload_variants(pack: &BenchPack, fixture: &Fixture) -> Result<Vec<Vec<u8>>, BenchError> {
    let template: Value =
        serde_json::from_str(fixture.bytes).map_err(|source| BenchError::Serialize {
            context: "composition fixture",
            source,
        })?;
    let mut variants = Vec::with_capacity(usize::try_from(PAYLOAD_VARIANTS).unwrap_or(1));
    for variant in 0..PAYLOAD_VARIANTS {
        let mut document = template.clone();
        let draw = crate::perf_run::fnv1a(pack.seed ^ STREAM_PAYLOAD, &[variant]);
        if let Some(slot) = document.pointer_mut(SYSTOLIC) {
            *slot = json!(90_u64.saturating_add(draw % 60));
        }
        if let Some(slot) = document.pointer_mut(DIASTOLIC) {
            *slot = json!(50_u64.saturating_add((draw >> 8) % 40));
        }
        variants.push(
            serde_json::to_vec(&document).map_err(|source| BenchError::Serialize {
                context: "stamped composition",
                source,
            })?,
        );
    }
    Ok(variants)
}

/// Everything one offered request needs to address the seeded population.
#[derive(Debug, Clone, Copy)]
struct ArrivalTarget<'a> {
    /// The EHR the request is scoped to.
    ehr_id: &'a str,
    /// The versioned-object uid a composition read addresses.
    object_uid: &'a str,
    /// The `OBJECT_VERSION_ID` a version-by-id read addresses.
    version_uid: &'a str,
    /// The instant a `version_at_time` read addresses, already
    /// percent-encoded for a query parameter.
    version_at_time: &'a str,
    /// The composition bytes a write arrival offers.
    payload: &'a [u8],
    /// The seeded draw every query parameter of this arrival derives from.
    query_draw: u64,
}

/// The `POST /query/aql` body one query class offers, or `None` for an
/// operation that is not an ad-hoc query.
///
/// The request members are the ones the ad-hoc execute operation defines
/// (ITS-REST `specifications/schemas/query/AdhocQueryExecute.yaml`: `q`,
/// `offset`, `fetch`, `query_parameters`), and every value substituted into a
/// parameter comes from the arrival's own seeded draw, so a class never
/// repeats the previous arrival's result set and the whole draw is
/// reproducible from the pack seed.
fn query_request(op: BenchOp, target: ArrivalTarget<'_>) -> Option<Value> {
    let draw = target.query_draw;
    let systolic = SYSTOLIC_FLOOR.saturating_add(draw % SYSTOLIC_SPAN);
    let offset = ((draw >> 8) % ORDERED_PAGES).saturating_mul(PAGE_FETCH);
    let body = match op {
        BenchOp::AdhocQueryUid => {
            json!({ "q": ADHOC_UID_AQL, "query_parameters": { "ehr_id": target.ehr_id } })
        }
        BenchOp::AdhocQueryPointLookup => json!({
            "q": AQL_POINT_LOOKUP,
            "query_parameters": { "ehr_id": target.ehr_id, "uid": target.version_uid },
        }),
        BenchOp::AdhocQueryEhrScan => {
            json!({ "q": AQL_EHR_SCAN, "query_parameters": { "ehr_id": target.ehr_id } })
        }
        BenchOp::AdhocQueryFiltered => json!({
            "q": AQL_FILTERED.as_str(),
            "query_parameters": { "ehr_id": target.ehr_id, "systolic": systolic },
        }),
        BenchOp::AdhocQueryPopulation => json!({
            "q": AQL_POPULATION.as_str(),
            "fetch": POPULATION_FETCH,
            "query_parameters": { "systolic": systolic },
        }),
        BenchOp::AdhocQueryAggregate => json!({
            "q": AQL_AGGREGATE.as_str(),
            "query_parameters": { "systolic": systolic },
        }),
        BenchOp::AdhocQueryOrderedPage => json!({
            "q": AQL_ORDERED_PAGE,
            "fetch": PAGE_FETCH,
            "offset": offset,
        }),
        BenchOp::CreateComposition
        | BenchOp::GetCompositionAtTime
        | BenchOp::GetCompositionLatest
        | BenchOp::GetEhr
        | BenchOp::GetEhrStatus
        | BenchOp::GetVersionedComposition
        | BenchOp::GetVersionedCompositionRevisionHistory
        | BenchOp::GetVersionedCompositionVersionAtTime
        | BenchOp::GetVersionedCompositionVersionById
        | BenchOp::GetVersionedCompositionVersionLatest => return None,
    };
    Some(body)
}

/// Offers one arrival and reports whether it landed as the operation
/// requires, plus the class of any failure.
fn offer(
    client: &BenchClient,
    op: BenchOp,
    target: ArrivalTarget<'_>,
) -> (bool, Option<ErrorClass>) {
    // One path per arrival, built from the operation's own published wire
    // template, so what the manifest states and what goes out are one string.
    let path = op.path(
        target.ehr_id,
        target.object_uid,
        &query_value(target.version_uid),
        target.version_at_time,
    );
    let reply = match op {
        BenchOp::CreateComposition => client.send(
            op.as_str(),
            Method::POST,
            &path,
            Some(("application/json", target.payload.to_vec())),
            PreferReturn::Identifier,
        ),
        BenchOp::GetCompositionLatest
        | BenchOp::GetCompositionAtTime
        | BenchOp::GetVersionedComposition
        | BenchOp::GetVersionedCompositionVersionLatest
        | BenchOp::GetVersionedCompositionVersionAtTime
        | BenchOp::GetVersionedCompositionVersionById
        | BenchOp::GetVersionedCompositionRevisionHistory
        | BenchOp::GetEhr
        | BenchOp::GetEhrStatus => client.send(
            op.as_str(),
            Method::GET,
            &path,
            None,
            PreferReturn::Unstated,
        ),
        BenchOp::AdhocQueryUid
        | BenchOp::AdhocQueryAggregate
        | BenchOp::AdhocQueryEhrScan
        | BenchOp::AdhocQueryFiltered
        | BenchOp::AdhocQueryOrderedPage
        | BenchOp::AdhocQueryPointLookup
        | BenchOp::AdhocQueryPopulation => {
            // `query_request` answers every query variant, so `None` here
            // would mean the two matches disagree about the vocabulary.
            let Some(body) = query_request(op, target) else {
                return (false, Some(ErrorClass::Transport));
            };
            match serde_json::to_vec(&body) {
                Ok(bytes) => client.send(
                    op.as_str(),
                    Method::POST,
                    &path,
                    Some(("application/json", bytes)),
                    PreferReturn::Unstated,
                ),
                Err(_unserializable) => return (false, Some(ErrorClass::Transport)),
            }
        }
    };
    match reply {
        Err(BenchError::Transport { source, .. }) => {
            let class = if source.is_timeout() {
                ErrorClass::Timeout
            } else {
                ErrorClass::Transport
            };
            (false, Some(class))
        }
        Err(_other) => (false, Some(ErrorClass::Transport)),
        Ok(reply) => {
            let accepted = match op {
                BenchOp::CreateComposition => created(reply.status),
                BenchOp::GetCompositionAtTime
                | BenchOp::GetCompositionLatest
                | BenchOp::GetEhr
                | BenchOp::GetEhrStatus
                | BenchOp::GetVersionedComposition
                | BenchOp::GetVersionedCompositionRevisionHistory
                | BenchOp::GetVersionedCompositionVersionAtTime
                | BenchOp::GetVersionedCompositionVersionById
                | BenchOp::GetVersionedCompositionVersionLatest
                | BenchOp::AdhocQueryUid
                | BenchOp::AdhocQueryAggregate
                | BenchOp::AdhocQueryEhrScan
                | BenchOp::AdhocQueryFiltered
                | BenchOp::AdhocQueryOrderedPage
                | BenchOp::AdhocQueryPointLookup
                | BenchOp::AdhocQueryPopulation => reply.status == StatusCode::OK,
            };
            if accepted {
                (true, None)
            } else {
                (false, Some(ErrorClass::of_status(reply.status)))
            }
        }
    }
}

/// Folds a sweep's observations into one histogram and one failure tally per
/// operation.
///
/// # Errors
/// [`BenchError::Histogram`] when a histogram cannot be created or encoded.
fn aggregate(
    observations: &[(BenchOp, u64, Option<ErrorClass>)],
    elapsed_s: f64,
) -> Result<BTreeMap<String, OperationStats>, BenchError> {
    let mut recorders: BTreeMap<&'static str, (Histogram<u64>, BTreeMap<String, u64>)> =
        BTreeMap::new();
    for (op, latency_us, class) in observations {
        let entry = match recorders.entry(op.as_str()) {
            std::collections::btree_map::Entry::Occupied(entry) => entry.into_mut(),
            std::collections::btree_map::Entry::Vacant(entry) => {
                let histogram = Histogram::new_with_bounds(1, HDR_MAX_US, 3)
                    .map_err(|error| BenchError::Histogram(error.to_string()))?;
                entry.insert((histogram, BTreeMap::new()))
            }
        };
        let _saturated = entry.0.record(latency_us.clamp(&1, &HDR_MAX_US).to_owned());
        if let Some(class) = class {
            let counter = entry.1.entry(class.as_str().to_owned()).or_insert(0);
            *counter = counter.saturating_add(1);
        }
    }
    let mut operations = BTreeMap::new();
    for (op, (histogram, classes)) in recorders {
        let _replaced = operations.insert(
            op.to_owned(),
            OperationStats::from_histogram(&histogram, classes, elapsed_s)?,
        );
    }
    Ok(operations)
}

/// One sweep worker's walk: it claims composition slots off the shared cursor
/// until the population is exhausted, offering the phase's requests against
/// every slot it claims and returning what each one cost.
fn sweep_worker(
    client: &BenchClient,
    phase: &SweepPhase,
    corpus: &BenchCorpus,
    at_time: &str,
    cursor: &AtomicUsize,
) -> Vec<(BenchOp, u64, Option<ErrorClass>)> {
    let mut local = Vec::new();
    loop {
        let slot = cursor.fetch_add(1, Ordering::Relaxed);
        let Some(seeded) = corpus.compositions.get(slot) else {
            break;
        };
        let ehr_id = corpus
            .ehr_ids
            .get(seeded.ehr_index)
            .map_or("", String::as_str);
        for op in &phase.per_composition {
            let issued = Instant::now();
            // A sweep walks the population in creation order, so its targets
            // are the walk's own cursor and no parameter is drawn.
            let (ok, class) = offer(
                client,
                *op,
                ArrivalTarget {
                    ehr_id,
                    object_uid: seeded.object_uid.as_str(),
                    version_uid: seeded.version_uid.as_str(),
                    version_at_time: at_time,
                    payload: &[],
                    query_draw: u64::try_from(slot).unwrap_or(0),
                },
            );
            let latency_us =
                u64::try_from(issued.elapsed().as_micros().min(u128::from(HDR_MAX_US)))
                    .unwrap_or(HDR_MAX_US);
            local.push((*op, latency_us, if ok { None } else { class }));
        }
    }
    local
}

/// Executes one closed-loop sweep: every seeded composition, in creation
/// order, offered the phase's requests one after another.
///
/// Creation order is the reproduction: a single-client harness walks the
/// population it wrote, in the order it wrote it, and the locality that gives
/// a system is part of what the published figure measures. The open-loop phase
/// beside it draws its targets from the seeded stream instead.
///
/// Latency here is the request's own duration, which is what a closed-loop
/// client observes and the only honest thing to record when the next request
/// waits for this one. The headline figure is the whole-loop average.
fn sweep_phase(
    client: &BenchClient,
    phase: &SweepPhase,
    corpus: &BenchCorpus,
    progress: &(dyn Fn(String) + Sync),
) -> Result<SweepPhaseRecord, BenchError> {
    let fail = |detail: String| BenchError::Measure {
        phase: phase.name.clone(),
        detail,
    };
    let compositions = corpus.compositions.len();
    let per_composition = phase.per_composition.len();
    if compositions == 0 || per_composition == 0 {
        return Err(fail(
            "the sweep has no composition to walk or no request to offer".to_owned(),
        ));
    }
    let encoded_at_time = query_value(&corpus.version_at_time);
    let workers = phase.workers.max(1);
    let next = AtomicUsize::new(0);
    let collected: Mutex<Vec<(BenchOp, u64, Option<ErrorClass>)>> =
        Mutex::new(Vec::with_capacity(phase.requests(compositions)));

    let started = Instant::now();
    {
        let cursor = &next;
        let sink = &collected;
        let at_time = encoded_at_time.as_str();
        std::thread::scope(|scope| {
            for _ in 0..workers {
                let _handle = scope.spawn(move || {
                    let mut local = sweep_worker(client, phase, corpus, at_time, cursor);
                    if let Ok(mut all) = sink.lock() {
                        all.append(&mut local);
                    }
                });
            }
        });
    }
    let elapsed_s = started.elapsed().as_secs_f64();

    let observations = collected
        .into_inner()
        .map_err(|error| fail(format!("sweep lock poisoned: {error}")))?;
    let requests = observations.len();
    let operations = aggregate(&observations, elapsed_s)?;

    #[expect(
        clippy::as_conversions,
        clippy::cast_precision_loss,
        reason = "request counts are far below 2^52"
    )]
    let whole_loop_us_per_request = if requests > 0 {
        elapsed_s * 1_000_000.0 / requests as f64
    } else {
        0.0
    };
    progress(format!(
        "sweep {} finished: {requests} request(s) in {elapsed_s:.1}s, {whole_loop_us_per_request:.1} us/request whole-loop",
        phase.name
    ));
    Ok(SweepPhaseRecord {
        name: phase.name.clone(),
        regime: LoopRegime::ClosedLoop,
        workers: u64::try_from(workers).unwrap_or(u64::MAX),
        compositions: u64::try_from(compositions).unwrap_or(u64::MAX),
        requests_per_composition: u64::try_from(per_composition).unwrap_or(u64::MAX),
        requests: u64::try_from(requests).unwrap_or(u64::MAX),
        elapsed_s,
        whole_loop_us_per_request,
        operations,
    })
}

/// Executes one open-loop measured phase.
#[expect(
    clippy::too_many_lines,
    reason = "one measured-window procedure: schedule, dispatch, collect, aggregate"
)]
fn measure_phase(
    client: &BenchClient,
    pack: &BenchPack,
    phase: &MeasurePhase,
    phase_index: u64,
    corpus: &BenchCorpus,
    progress: &(dyn Fn(String) + Sync),
) -> Result<MeasuredPhaseRecord, BenchError> {
    let fail = |detail: String| BenchError::Measure {
        phase: phase.name.clone(),
        detail,
    };
    let composition_fixture = pack
        .fixtures()
        .into_iter()
        .find(|fixture| fixture.kind == FixtureKind::Composition)
        .ok_or_else(|| fail("the pack embeds no composition to write".to_owned()))?;
    let payloads = payload_variants(pack, &composition_fixture)?;
    let encoded_at_time = query_value(&corpus.version_at_time);
    let schedule = build_schedule(pack, phase, phase_index);
    let planned_measured = schedule.iter().filter(|arrival| arrival.recorded).count();

    let (tx, rx) = mpsc::channel::<Completion>();
    let collector = std::thread::spawn(move || {
        let mut recorders: BTreeMap<&'static str, (Histogram<u64>, BTreeMap<String, u64>)> =
            BTreeMap::new();
        let mut warmup: u64 = 0;
        let mut broken: u64 = 0;
        for done in rx {
            if !done.recorded {
                warmup = warmup.saturating_add(1);
                continue;
            }
            let entry = match recorders.entry(done.op.as_str()) {
                std::collections::btree_map::Entry::Occupied(entry) => entry.into_mut(),
                std::collections::btree_map::Entry::Vacant(entry) => {
                    let Ok(histogram) = Histogram::new_with_bounds(1, HDR_MAX_US, 3) else {
                        broken = broken.saturating_add(1);
                        continue;
                    };
                    entry.insert((histogram, BTreeMap::new()))
                }
            };
            let _saturated = entry.0.record(done.latency_us.clamp(1, HDR_MAX_US));
            if let Some(class) = done.class {
                let counter = entry.1.entry(class.as_str().to_owned()).or_insert(0);
                *counter = counter.saturating_add(1);
            }
        }
        (recorders, warmup, broken)
    });

    let start = Instant::now();
    let dispatched_measured = AtomicU64::new(0);
    let dispatch_span = std::thread::scope(|scope| {
        for arrival in &schedule {
            let planned = start + arrival.at;
            let now = Instant::now();
            if planned > now {
                std::thread::sleep(planned - now);
            }
            if arrival.recorded {
                let _previous = dispatched_measured.fetch_add(1, Ordering::Relaxed);
            }
            let tx = tx.clone();
            let payloads = &payloads;
            let encoded_at_time = encoded_at_time.as_str();
            let arrival = *arrival;
            let _handle = scope.spawn(move || {
                let ehr_draw =
                    crate::perf_run::fnv1a(pack.seed ^ STREAM_EHR, &[phase_index, arrival.index]);
                let composition_draw = crate::perf_run::fnv1a(
                    pack.seed ^ STREAM_COMPOSITION,
                    &[phase_index, arrival.index],
                );
                let payload_draw = crate::perf_run::fnv1a(
                    pack.seed ^ STREAM_PAYLOAD,
                    &[phase_index, arrival.index],
                );
                let query_draw =
                    crate::perf_run::fnv1a(pack.seed ^ STREAM_QUERY, &[phase_index, arrival.index]);
                let ehr_slot = usize::try_from(
                    ehr_draw % u64::try_from(corpus.ehr_ids.len().max(1)).unwrap_or(1),
                )
                .unwrap_or(0);
                let composition_slot = usize::try_from(
                    composition_draw % u64::try_from(corpus.compositions.len().max(1)).unwrap_or(1),
                )
                .unwrap_or(0);
                let payload_slot = usize::try_from(payload_draw % PAYLOAD_VARIANTS).unwrap_or(0);
                let drawn = corpus.compositions.get(composition_slot);
                // A composition read addresses the EHR its composition was
                // committed into, so a read can never be a 404 the instrument
                // itself manufactured.
                let target = match drawn {
                    Some(seeded) if arrival.op.addresses_a_composition() => ArrivalTarget {
                        ehr_id: corpus
                            .ehr_ids
                            .get(seeded.ehr_index)
                            .map_or("", String::as_str),
                        object_uid: seeded.object_uid.as_str(),
                        version_uid: seeded.version_uid.as_str(),
                        version_at_time: encoded_at_time,
                        payload: payloads.get(payload_slot).map_or(&[][..], Vec::as_slice),
                        query_draw,
                    },
                    _ => ArrivalTarget {
                        ehr_id: corpus.ehr_ids.get(ehr_slot).map_or("", String::as_str),
                        object_uid: "",
                        version_uid: "",
                        version_at_time: encoded_at_time,
                        payload: payloads.get(payload_slot).map_or(&[][..], Vec::as_slice),
                        query_draw,
                    },
                };
                let (ok, class) = offer(client, arrival.op, target);
                let latency = planned.elapsed();
                let latency_us = u64::try_from(latency.as_micros().min(u128::from(HDR_MAX_US)))
                    .unwrap_or(HDR_MAX_US);
                let _closed = tx.send(Completion {
                    op: arrival.op,
                    latency_us,
                    class: if ok { None } else { class },
                    recorded: arrival.recorded,
                });
            });
        }
        drop(tx);
        start.elapsed()
    });

    let (recorders, warmup_arrivals, broken) = collector.join().map_err(|payload| {
        let detail = payload
            .downcast_ref::<&str>()
            .map(|message| (*message).to_owned())
            .or_else(|| payload.downcast_ref::<String>().cloned())
            .unwrap_or_else(|| "non-string panic payload".to_owned());
        fail(format!("collector thread panicked: {detail}"))
    })?;
    if broken > 0 {
        return Err(fail(format!(
            "{broken} arrival(s) could not be recorded into a histogram"
        )));
    }

    let planned_span_s = phase.warmup_s.saturating_add(phase.duration_s);
    #[expect(
        clippy::as_conversions,
        clippy::cast_precision_loss,
        reason = "spans and counts are far below 2^52"
    )]
    let (measured_span_s, offered_load_sustained_per_s, generator_bound) = {
        let actual_span = dispatch_span.as_secs_f64().max(planned_span_s as f64);
        let measured_span = (actual_span - phase.warmup_s as f64).max(0.0);
        let dispatched = dispatched_measured.load(Ordering::Relaxed) as f64;
        let offered = if measured_span > 0.0 {
            dispatched / measured_span
        } else {
            0.0
        };
        let lagged = dispatch_span.as_secs_f64() > planned_span_s as f64 * 1.02;
        (measured_span, offered, lagged)
    };

    let mut operations = BTreeMap::new();
    for (op, (histogram, classes)) in recorders {
        let _replaced = operations.insert(
            op.to_owned(),
            OperationStats::from_histogram(&histogram, classes, measured_span_s)?,
        );
    }
    progress(format!(
        "phase {} finished: {} measured arrival(s) over {} operation(s)",
        phase.name,
        dispatched_measured.load(Ordering::Relaxed),
        operations.len()
    ));
    Ok(MeasuredPhaseRecord {
        regime: LoopRegime::OpenLoop,
        rate_per_s: phase.rate_per_s,
        warmup_s: phase.warmup_s,
        duration_s: phase.duration_s,
        planned_measured_arrivals: u64::try_from(planned_measured).unwrap_or(u64::MAX),
        dispatched_measured_arrivals: dispatched_measured.load(Ordering::Relaxed),
        warmup_arrivals,
        offered_load_sustained_per_s,
        generator_bound,
        operations,
    })
}

#[cfg(test)]
#[expect(
    clippy::panic_in_result_fn,
    reason = "a Result-returning test in the Book ch11 shape that also asserts; \
              clippy offers no allow-in-tests knob for this lint"
)]
mod tests {
    use super::*;
    use crate::bench::pack;

    /// Credentials over plain http to a non-loopback host warn once and never
    /// refuse; loopback quickstarts, https targets and credential-less runs
    /// stay silent (#296).
    #[test]
    fn plain_http_credentials_warn_beyond_loopback() {
        let warns = |url: &str, auth: AuthKind| plain_http_credential_warning(url, auth);
        let warning = warns("http://cdr.example:8080/openehr/v1", AuthKind::Basic)
            .expect("a remote plain-http credential warns");
        assert!(warning.contains("cdr.example"), "{warning}");
        assert!(
            warns("http://[2001:db8::1]:8080/v1", AuthKind::Bearer).is_some(),
            "a bracketed remote IPv6 host warns"
        );
        for silent in [
            ("http://localhost:8080/openehr/v1", AuthKind::Basic),
            ("http://127.0.0.1:8080/v1", AuthKind::Bearer),
            ("http://[::1]:8080/v1", AuthKind::Basic),
            ("https://cdr.example/openehr/v1", AuthKind::Basic),
            ("http://cdr.example/openehr/v1", AuthKind::None),
        ] {
            assert!(warns(silent.0, silent.1).is_none(), "{silent:?}");
        }
    }

    /// The schedule is a pure function of the pack seed and the phase index,
    /// so two repetitions offer the same work in the same order.
    #[test]
    fn the_schedule_is_deterministic_in_the_seed() {
        let deck = pack::smoke();
        let Some(phase) = deck.measure_phases().first().copied().cloned() else {
            panic!("the smoke pack lost its measured phase");
        };
        let first = build_schedule(&deck, &phase, 0);
        let second = build_schedule(&deck, &phase, 0);
        let ops_first: Vec<BenchOp> = first.iter().map(|a| a.op).collect();
        let ops_second: Vec<BenchOp> = second.iter().map(|a| a.op).collect();
        assert_eq!(ops_first, ops_second);
        assert!(!first.is_empty());
    }

    /// A different phase index draws a different operation sequence, so two
    /// phases in one pack never correlate.
    #[test]
    fn separate_phases_draw_separate_streams() {
        let deck = pack::smoke();
        let Some(phase) = deck.measure_phases().first().copied().cloned() else {
            panic!("the smoke pack lost its measured phase");
        };
        let zero: Vec<BenchOp> = build_schedule(&deck, &phase, 0)
            .iter()
            .map(|a| a.op)
            .collect();
        let one: Vec<BenchOp> = build_schedule(&deck, &phase, 1)
            .iter()
            .map(|a| a.op)
            .collect();
        assert_ne!(zero, one);
    }

    /// The warmup split follows the planned instants: exactly the arrivals
    /// past the warmup boundary are recorded.
    #[test]
    fn warmup_arrivals_are_excluded_from_the_measured_span() {
        let deck = pack::smoke();
        let phase = MeasurePhase {
            name: "t".to_owned(),
            rate_per_s: 10.0,
            warmup_s: 2,
            duration_s: 3,
            mix: vec![pack::MixEntry::new(BenchOp::GetEhr, 1, "the EHR read")],
        };
        let schedule = build_schedule(&deck, &phase, 0);
        assert_eq!(schedule.len(), 50);
        assert_eq!(schedule.iter().filter(|a| a.recorded).count(), 30);
        assert!(schedule.iter().take(20).all(|a| !a.recorded));
    }

    /// A zero rate schedules nothing rather than dividing by zero.
    #[test]
    fn a_zero_rate_schedules_nothing() {
        let deck = pack::smoke();
        let phase = MeasurePhase {
            name: "t".to_owned(),
            rate_per_s: 0.0,
            warmup_s: 0,
            duration_s: 10,
            mix: vec![pack::MixEntry::new(BenchOp::GetEhr, 1, "the EHR read")],
        };
        assert!(build_schedule(&deck, &phase, 0).is_empty());
    }

    /// Payload variants are deterministic in the seed and genuinely differ,
    /// so a write arrival never offers the same bytes every time.
    #[test]
    fn payload_variants_are_deterministic_and_distinct() -> Result<(), BenchError> {
        let deck = pack::smoke();
        let fixture = deck
            .fixtures()
            .into_iter()
            .find(|fixture| fixture.kind == FixtureKind::Composition)
            .ok_or_else(|| BenchError::Histogram("no composition fixture".to_owned()))?;
        let first = payload_variants(&deck, &fixture)?;
        let second = payload_variants(&deck, &fixture)?;
        assert_eq!(first, second);
        assert_eq!(first.len(), 16);
        let distinct: std::collections::BTreeSet<&Vec<u8>> = first.iter().collect();
        assert!(distinct.len() > 1, "every variant carried the same bytes");
        Ok(())
    }

    /// The version-uid form is reduced to the versioned object the
    /// latest-version read addresses.
    #[test]
    fn a_version_uid_reduces_to_its_versioned_object() {
        assert_eq!(object_uid_of("abc::sys::3"), "abc");
        assert_eq!(object_uid_of("bare"), "bare");
    }

    /// A repetition count of zero is refused before any request.
    #[test]
    fn zero_repetitions_are_refused() {
        let deck = pack::smoke();
        let error = execute(
            &BenchRun {
                pack: &deck,
                base_url: "http://stub",
                profile: &crate::bench::posture::MINIMAL,
                auth: AuthKind::None,
                user: None,
                credential: None,
                repetitions: 0,
                label: None,
                scale: 1.0,
                seed_workers: None,
            },
            &|_message| {},
        )
        .unwrap_err();
        assert!(matches!(error, BenchError::Repetitions(0)), "{error}");
    }

    /// The scale factor multiplies the EHR count, never rounds to zero, and
    /// refuses a value that is not a positive finite number.
    #[test]
    fn the_scale_factor_shrinks_the_population_without_emptying_it()
    -> Result<(), Box<dyn std::error::Error>> {
        assert_eq!(scaled_ehrs(100, 1.0)?, 100);
        assert_eq!(scaled_ehrs(100, 0.1)?, 10);
        assert_eq!(scaled_ehrs(100, 2.5)?, 250);
        assert_eq!(scaled_ehrs(100, 0.0001)?, 1, "a scaled run still seeds one");
        for refused in [0.0, -1.0, f64::NAN, f64::INFINITY] {
            assert!(scaled_ehrs(100, refused).is_err(), "{refused} was accepted");
        }
        Ok(())
    }

    /// The captured instant is recorded only by a pack that actually drives a
    /// `version_at_time` read.
    #[test]
    fn only_a_pack_that_reads_at_an_instant_records_one() {
        assert!(!reads_a_version_at_time(&pack::smoke()));
        assert!(reads_a_version_at_time(&pack::community_vitals()));
    }

    /// One target for the query-body tests, with the draw the caller names.
    fn query_target(draw: u64) -> ArrivalTarget<'static> {
        ArrivalTarget {
            ehr_id: "EHR-7",
            object_uid: "c-1",
            version_uid: "c-1::sut::1",
            version_at_time: "2026-08-29T00%3A00%3A00Z",
            payload: &[],
            query_draw: draw,
        }
    }

    /// Every query class carries its own AQL statement, and every class that
    /// is not a query carries none.
    #[test]
    fn every_query_class_carries_its_own_statement() {
        let mut statements: std::collections::BTreeSet<String> = std::collections::BTreeSet::new();
        for op in BenchOp::ALL {
            let body = query_request(*op, query_target(1));
            assert_eq!(
                body.is_some(),
                op.is_adhoc_query(),
                "{op} disagrees about being a query"
            );
            let Some(body) = body else { continue };
            let statement = body
                .pointer("/q")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_owned();
            assert!(statement.starts_with("SELECT "), "{op}: {statement}");
            assert!(
                statements.insert(statement.clone()),
                "{op} repeats another class's statement: {statement}"
            );
        }
        assert_eq!(statements.len(), 7);
    }

    /// Each class's statement carries the shape its rationale claims: the
    /// point lookup addresses one uid, the scan is unbounded, the two
    /// predicated classes carry the systolic path, the aggregate counts, and
    /// the ordered page sorts.
    #[test]
    fn each_query_class_has_the_shape_its_rationale_claims() {
        assert!(AQL_POINT_LOOKUP.contains("WHERE c/uid/value = $uid"));
        assert!(AQL_POINT_LOOKUP.contains("EHR e[ehr_id/value=$ehr_id]"));

        assert!(AQL_EHR_SCAN.contains("EHR e[ehr_id/value=$ehr_id]"));
        assert!(!AQL_EHR_SCAN.contains("WHERE"), "the scan filters");

        assert!(AQL_FILTERED.contains("EHR e[ehr_id/value=$ehr_id]"));
        assert!(AQL_FILTERED.contains(SYSTOLIC_MAGNITUDE));
        assert!(AQL_FILTERED.ends_with(">= $systolic"));

        assert!(AQL_POPULATION.contains(SYSTOLIC_MAGNITUDE));
        assert!(
            !AQL_POPULATION.contains("$ehr_id"),
            "the population query is scoped to one EHR"
        );

        assert!(AQL_AGGREGATE.contains("COUNT(c/uid/value)"));
        assert!(AQL_AGGREGATE.contains(SYSTOLIC_MAGNITUDE));

        assert!(AQL_ORDERED_PAGE.contains("ORDER BY c/context/start_time/value DESC"));
        assert!(!AQL_ORDERED_PAGE.contains("$ehr_id"));
    }

    /// Only the two paged classes bound their result set, through the request
    /// members the ad-hoc execute operation defines.
    #[test]
    fn only_the_paged_classes_carry_a_fetch_bound() {
        let fetch = |op: BenchOp| {
            query_request(op, query_target(0x0102_0304))
                .and_then(|body| body.pointer("/fetch").and_then(Value::as_u64))
        };
        assert_eq!(fetch(BenchOp::AdhocQueryPopulation), Some(50));
        assert_eq!(fetch(BenchOp::AdhocQueryOrderedPage), Some(20));
        assert_eq!(fetch(BenchOp::AdhocQueryPointLookup), None);
        assert_eq!(fetch(BenchOp::AdhocQueryEhrScan), None);
        assert_eq!(fetch(BenchOp::AdhocQueryFiltered), None);
        assert_eq!(fetch(BenchOp::AdhocQueryAggregate), None);
    }

    /// Query parameters are a pure function of the arrival's draw: the same
    /// draw yields the same body, and the threshold stays inside the band the
    /// pack declares.
    #[test]
    fn query_parameters_are_deterministic_in_the_draw() {
        for draw in [0_u64, 1, 7, 0x0102_0304_0506_0708, u64::MAX] {
            assert_eq!(
                query_request(BenchOp::AdhocQueryFiltered, query_target(draw)),
                query_request(BenchOp::AdhocQueryFiltered, query_target(draw)),
                "draw {draw} is not deterministic"
            );
            let systolic = query_request(BenchOp::AdhocQueryFiltered, query_target(draw))
                .and_then(|body| {
                    body.pointer("/query_parameters/systolic")
                        .and_then(Value::as_u64)
                })
                .unwrap_or_default();
            assert!((90..150).contains(&systolic), "{systolic} is off the band");
            let offset = query_request(BenchOp::AdhocQueryOrderedPage, query_target(draw))
                .and_then(|body| body.pointer("/offset").and_then(Value::as_u64))
                .unwrap_or_default();
            assert!(
                offset < 200 && offset.is_multiple_of(20),
                "{offset} is off the grid"
            );
        }
    }

    /// Two different draws move the threshold and the page, which is what
    /// stops a server serving one memoized result set for a whole class.
    #[test]
    fn different_draws_move_the_threshold_and_the_page() {
        let systolic = |draw: u64| {
            query_request(BenchOp::AdhocQueryFiltered, query_target(draw)).and_then(|body| {
                body.pointer("/query_parameters/systolic")
                    .and_then(Value::as_u64)
            })
        };
        assert_ne!(systolic(0), systolic(1));
        let offset = |draw: u64| {
            query_request(BenchOp::AdhocQueryOrderedPage, query_target(draw))
                .and_then(|body| body.pointer("/offset").and_then(Value::as_u64))
        };
        assert_ne!(offset(0), offset(1 << 8));
    }

    /// The point lookup addresses the composition its EHR holds, so it can
    /// never be a lookup the instrument itself made miss.
    #[test]
    fn the_point_lookup_addresses_the_drawn_composition() {
        let Some(body) = query_request(BenchOp::AdhocQueryPointLookup, query_target(3)) else {
            panic!("the point lookup carries no body");
        };
        assert_eq!(
            body.pointer("/query_parameters/uid")
                .and_then(Value::as_str),
            Some("c-1::sut::1")
        );
        assert_eq!(
            body.pointer("/query_parameters/ehr_id")
                .and_then(Value::as_str),
            Some("EHR-7")
        );
        assert!(
            BenchOp::AdhocQueryPointLookup.addresses_a_composition(),
            "the point lookup would be given an EHR-only target"
        );
    }

    /// Every operation the community walk offers addresses a composition, so
    /// no arrival in the walk falls back to an EHR-only target.
    #[test]
    fn the_community_walk_addresses_only_compositions() {
        let deck = pack::community_vitals();
        let Some(sweep) = deck.sweep_phases().first().copied() else {
            panic!("the community pack lost its walk");
        };
        assert!(
            sweep
                .per_composition
                .iter()
                .all(|op| op.addresses_a_composition())
        );
    }
}
