// SPDX-FileCopyrightText: Veredictum contributors
// SPDX-License-Identifier: Apache-2.0

//! The container-runtime-coupled half of the measured instruments, driven
//! against a real engine.
//!
//! Three code paths reach outside this process for their answer: the
//! resource sampler reads the Docker Engine API's per-container stats, the
//! disk anchor and the maintenance settling shell into the DB container, and
//! the AQL probe attributes its cost through `pg_stat_statements`. A fake
//! cannot establish that any of them works, because what is being checked IS
//! the reply of another program, so this module composes a real container
//! and reads the real answers.
//!
//! It SKIPS with a printed reason when no container runtime answers, so a
//! contributor without one is not blocked, and the CI job asserts the engine
//! and the pinned image are both present before the suite runs, so a skip can
//! never be the outcome that gates a merge. When the engine IS present and a
//! path is broken, every test here fails.

#![expect(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    reason = "test-support helpers (not `#[test]` fns, so the clippy.toml in-tests scoping does not reach them) are panic-idiomatic: a broken harness must abort the test loudly, Book ch11"
)]
#![expect(
    clippy::print_stderr,
    reason = "a skipped gate must say why on the run log, which is the standing shape of the GnuPG interop gate beside this one"
)]

use std::process::Command;
use std::time::Duration;

use veredictum::ixit::{Containers, Environment, Ixit};
use veredictum::perf_run::client::{PerfClient, PerfPrincipals};
use veredictum::perf_run::corpus::SeededCorpus;
use veredictum::perf_run::resources::{
    ResourceSampler, SAMPLE_INTERVAL, db_volume_bytes, settle_maintenance,
};
use veredictum::probe::{ProbeOptions, run_probe};

use crate::fake_sut::closed_port_url;

/// The database image the container-coupled paths are read against. The
/// upstream `postgres` image ships `du`, `vacuumdb`, `psql` and the
/// `pg_stat_statements` extension, which is exactly the set those paths
/// shell into, and it keeps its data under the prefix the disk anchor
/// probes.
const DB_IMAGE: &str = "postgres:18.6";

/// Whether a container runtime answers, with its server version as the
/// evidence.
fn engine_version() -> Option<String> {
    let out = Command::new("docker")
        .args(["version", "--format", "{{.Server.Version}}"])
        .output()
        .ok()?;
    if !out.status.success() {
        return None;
    }
    let version = String::from_utf8(out.stdout).ok()?.trim().to_owned();
    (!version.is_empty()).then_some(version)
}

/// Whether the pinned image is on this host, pulling it once if it is not.
fn image_present() -> bool {
    let inspect = Command::new("docker")
        .args(["image", "inspect", DB_IMAGE])
        .output();
    if inspect.is_ok_and(|out| out.status.success()) {
        return true;
    }
    Command::new("docker")
        .args(["pull", DB_IMAGE])
        .output()
        .is_ok_and(|out| out.status.success())
}

/// A running container, removed when the guard drops.
struct DbContainer {
    name: String,
}

impl Drop for DbContainer {
    fn drop(&mut self) {
        let _removed = Command::new("docker")
            .args(["rm", "-f", &self.name])
            .output();
    }
}

impl DbContainer {
    /// Start a fresh database container under a name unique to this test
    /// process, so the suite's own parallelism cannot collide.
    fn start(label: &str) -> Self {
        let name = format!("veredictum-cov-{label}-{}", std::process::id());
        let _stale = Command::new("docker").args(["rm", "-f", &name]).output();
        let run = Command::new("docker")
            .args([
                "run",
                "-d",
                "--name",
                &name,
                "-e",
                "POSTGRES_PASSWORD=postgres",
                DB_IMAGE,
                "-c",
                "shared_preload_libraries=pg_stat_statements",
            ])
            .output()
            .expect("the container runtime answered `docker version`, so `docker run` must run");
        assert!(
            run.status.success(),
            "starting {DB_IMAGE} failed: {}",
            String::from_utf8_lossy(&run.stderr)
        );
        Self { name }
    }

    /// Block until the database accepts connections, or fail the test.
    fn wait_ready(&self) {
        for _ in 0..60 {
            let ready = Command::new("docker")
                .args(["exec", &self.name, "pg_isready", "-U", "postgres"])
                .output();
            if ready.is_ok_and(|out| out.status.success()) {
                return;
            }
            std::thread::sleep(Duration::from_secs(1));
        }
        panic!("{} never accepted connections", self.name);
    }

    /// The ixit capability block naming this container in both roles.
    fn containers(&self) -> Containers {
        Containers {
            sut: self.name.clone(),
            db: self.name.clone(),
        }
    }
}

/// The environment block every measured artifact is bound to.
fn environment() -> Environment {
    serde_json::from_value(serde_json::json!({
        "hardware_class": "test-container", "cores": 1, "memory_gb": 1,
        "storage_class": "container", "topology": "one postgres container"
    }))
    .unwrap()
}

/// A corpus index the probe can address: the probe reads the first EHR id
/// to scope its ad-hoc query, and never writes.
fn corpus_index() -> SeededCorpus {
    SeededCorpus {
        corpus: "cnf.scale.10k".to_owned(),
        ehr_ids: vec!["ehr-0".to_owned()],
        compositions: Vec::new(),
        ward: Vec::new(),
    }
}

/// A client addressed at a port nothing listens on: the probe's WIRE half
/// is not what this module checks, and a recorded failure per request is
/// the probe's documented behaviour for an unreachable SUT.
fn unreachable_client() -> PerfClient {
    let ixit: Ixit = serde_json::from_value(serde_json::json!({
        "instances": { "sut": { "base_url": closed_port_url(), "auth": { "mode": "none" } } }
    }))
    .unwrap();
    PerfPrincipals::from_ixit(&ixit).unwrap().primary().clone()
}

/// Prints why a test is skipped and yields nothing, so the reason is in the
/// run log rather than inferred from a silence.
fn skip(test: &str) -> bool {
    match (engine_version(), image_present()) {
        (None, _) => {
            eprintln!("SKIP {test}: no container runtime answers `docker version`");
            true
        }
        (Some(_), false) => {
            eprintln!("SKIP {test}: the pinned image {DB_IMAGE} is neither present nor pullable");
            true
        }
        (Some(version), true) => {
            eprintln!("engine {version}, image {DB_IMAGE}");
            false
        }
    }
}

/// The sampler's whole point is the DELTA: CPU percentage needs two
/// readings, so a container's first tick only seeds the slot and the first
/// emitted sample lands one interval in. The window here is held past that
/// interval, which is the only way to read a real cpu/rss/blkio/network
/// sample off a real engine.
#[test]
fn the_resource_sampler_records_a_real_container_series() {
    if skip("the_resource_sampler_records_a_real_container_series") {
        return;
    }
    let container = DbContainer::start("sampler");
    let containers = container.containers();

    // The phase stamps derive from these bounds; the sample lands one
    // interval in, so it stamps as warmup.
    let sampler = ResourceSampler::start(&containers, 60, 60);
    std::thread::sleep(SAMPLE_INTERVAL + Duration::from_secs(3));
    let (series, notes) = sampler.stop();

    assert!(
        notes.is_empty(),
        "a reachable container still produced degradation notes: {notes:?}"
    );
    assert_eq!(series.len(), 2, "one series per declared container role");
    let recorded = series
        .iter()
        .find(|s| !s.samples.is_empty())
        .expect("no container produced a sample within one interval past the baseline");
    let sample = recorded
        .samples
        .first()
        .expect("the series carries a sample");
    assert!(
        sample.offset_s >= SAMPLE_INTERVAL.as_secs(),
        "the first emitted sample landed before one interval: {}s",
        sample.offset_s
    );
    assert!(sample.rss_bytes > 0, "a running postgres reported no RSS");
    assert!(
        sample.cpu_pct >= 0.0 && sample.cpu_pct.is_finite(),
        "cpu {} is not a percentage of one core",
        sample.cpu_pct
    );
    assert_eq!(sample.phase, veredictum::perf::ResourcePhase::Warmup);
    assert!(
        recorded.cpu_peak() >= sample.cpu_pct,
        "the peak is below a recorded sample"
    );
}

/// The two paths that shell into the DB container: the disk anchor reads
/// the volume's byte total, and the maintenance settling pays the seeding's
/// vacuum debt outside the window. Both are read against a real database,
/// and both name the container when it is not there.
#[test]
fn the_disk_anchor_and_the_maintenance_settling_run_through_the_container() {
    if skip("the_disk_anchor_and_the_maintenance_settling_run_through_the_container") {
        return;
    }
    let container = DbContainer::start("anchors");
    container.wait_ready();

    let bytes = db_volume_bytes(&container.name).expect("the volume has a byte total");
    assert!(bytes > 0, "an initialized cluster occupies no bytes");
    settle_maintenance(&container.name).expect("vacuumdb --all --analyze settles a live cluster");

    // The absent-container side: an anchor that cannot be probed is an
    // absent anchor with a reason, never a fabricated number.
    let missing = "veredictum-cov-no-such-container";
    let error = db_volume_bytes(missing).expect_err("no such container has no volume");
    assert!(error.contains("no byte total"), "{error}");
    let error = settle_maintenance(missing).expect_err("no such container to settle");
    assert!(error.contains("vacuumdb failed"), "{error}");
}

/// The probe's DB-side attribution, end to end: the extension is armed,
/// planner time is separated from executor time, the counters are reset per
/// probe, and the statements are read back as typed costs. The background
/// query loop stands in for the SUT's own database work — with the wire half
/// pointed at nothing, it is the only traffic there is to attribute, and
/// attribution over an idle database would prove only that the read parses.
#[test]
fn the_probe_attributes_its_cost_through_pg_stat_statements() {
    if skip("the_probe_attributes_its_cost_through_pg_stat_statements") {
        return;
    }
    let container = DbContainer::start("probe");
    container.wait_ready();
    let containers = container.containers();

    let name = container.name.clone();
    let stop = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
    let running = std::sync::Arc::clone(&stop);
    let traffic = std::thread::spawn(move || {
        while !running.load(std::sync::atomic::Ordering::Relaxed) {
            let _query = Command::new("docker")
                .args([
                    "exec",
                    &name,
                    "psql",
                    "-U",
                    "postgres",
                    "-d",
                    "postgres",
                    "-Atc",
                    "SELECT count(*) FROM pg_class WHERE relname LIKE 'pg_%';",
                ])
                .output();
        }
    });

    let notes = std::sync::Mutex::new(Vec::new());
    let report = run_probe(
        &unreachable_client(),
        &corpus_index(),
        &environment(),
        Some(&containers),
        &ProbeOptions { requests: 2 },
        &|message| notes.lock().unwrap().push(message),
    )
    .expect("a probe over a reachable database returns its report");
    stop.store(true, std::sync::atomic::Ordering::Relaxed);
    traffic.join().expect("the traffic thread finished");

    assert!(
        report.maintenance_settled,
        "settling failed: {:?}",
        notes.lock().unwrap()
    );
    assert_eq!(report.attribution, "pg_stat_statements");
    let seen = notes.lock().unwrap();
    assert!(
        !seen.iter().any(|m| m.contains("attribution read failed")),
        "the statement read failed: {seen:?}"
    );
    // The wire half is deliberately unreachable, so every request is a
    // recorded finding rather than an instrument error.
    assert!(report.probes.iter().all(|p| p.failures == 2));
    let attributed: usize = report.probes.iter().map(|p| p.statements.len()).sum();
    assert!(
        attributed > 0,
        "no statement was attributed across {} probes: {seen:?}",
        report.probes.len()
    );
    let statement = report
        .probes
        .iter()
        .flat_map(|p| &p.statements)
        .find(|s| s.sql.contains("pg_class"))
        .expect("the background query is among the attributed statements");
    assert!(statement.calls > 0);
    assert!(statement.total_ms >= statement.mean_ms - f64::EPSILON);
    assert!(
        statement.mean_plan_ms > 0.0,
        "track_planning was armed but the plan share reads zero"
    );
    assert!(
        statement.shared_blks_hit > 0,
        "a catalogue scan hit no buffer"
    );
}
