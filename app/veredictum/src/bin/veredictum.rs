// SPDX-FileCopyrightText: Veredictum contributors
// SPDX-License-Identifier: Apache-2.0

//! The CNF 2.0 reference runner CLI.
//!
//! ```text
//! veredictum emit-schemas --out DIR     write the published JSON-Schema set
//! veredictum validate --root DIR [--specs DIR] [--write-report]
//!                                       validate an artifact tree (all gates);
//!                                       --specs enables the SM/spec-ref
//!                                       resolution checks against the vendored
//!                                       spec tree (specs/openehr). The
//!                                       committed party statements beside the
//!                                       root (<root>/../party/*/statement.json)
//!                                       are swept in for the claim gates.
//! veredictum run --root DIR --ixit FILE --out DIR [--sut-name N] [--sut-version V] [--statement F]
//!                [--record-exchanges] [--sign-key FILE]
//!                                       drive the catalogue against a live SUT
//!                                       over its ixit topology and write
//!                                       results.json + the run report
//! veredictum verdicts --statement F --results F --root DIR --out DIR
//!                      [--sign-key FILE]
//!                                       compute the verdicts (pure pipeline)
//!                                       and write the report/statement/
//!                                       certificate + verdicts.json. With
//!                                       --sign-key, seal the bundle with
//!                                       record-manifest.json + .asc
//! veredictum verify-record --record DIR --key FILE
//!                                       recompute every digest a sealed
//!                                       bundle's manifest names and verify
//!                                       its detached signature against an
//!                                       armored public key
//! veredictum perf --root DIR --ixit FILE --results FILE --class POC|S|L|R
//!                 [--hours 1|2|4|6|8|12] [--seed-workers N]
//!                                       the measured class run (conformance-
//!                                       by-measurement): seed the scale
//!                                       corpus, hold the class's offered
//!                                       load for the sustained window, merge
//!                                       the record into results.json
//! veredictum stress --root DIR --ixit FILE --out FILE
//!                   [--corpus-class POC|S|L|R]
//!                   [--step-secs N] [--bisections N] [--max-rate R]
//!                                       the step-load stress ladder to the
//!                                       maximum sustainable throughput
//!                                       (exploration only — writes
//!                                       stress.json, never results.json)
//! veredictum stress-compare --left FILE --left-label L --right FILE
//!                           --right-label L --out FILE
//!                                       render the cross-SUT stress overlay
//!                                       FROM two committed stress reports,
//!                                       both directions on equal footing
//! veredictum bench --base-url URL [--auth none|basic|bearer] [--user U]
//!                  [--pack aql-mix|community-vitals|smoke] [--posture NAME]
//!                  [--repetitions N]
//!                  [--scale F] [--seed-workers N] [--with-baselines]
//!                  --out DIR [--label L]
//!                                       the universal speed benchmark: an
//!                                       embedded pack against any reachable
//!                                       CDR, seeded once and measured N times
//!                                       open-loop, under one declared posture
//!                                       profile whose canaries bracket the
//!                                       measured window, optionally anchored
//!                                       by the pinned reference CDRs on this
//!                                       host (comparative speed only — never a
//!                                       conformance record)
//! veredictum bench-compare --result FILE --result FILE [...] --out DIR
//!                                       align two or more committed bench
//!                                       results into one table, flagging every
//!                                       pack or host mismatch in the header
//! veredictum aql-probe --root DIR --ixit FILE --out FILE
//!                      [--corpus-class POC|S|L|R] [--requests N]
//!                                       the seeded-corpus AQL optimization
//!                                       probe: wire percentiles + DB
//!                                       statement attribution (exploration
//!                                       only — never a conformance record)
//! veredictum perf-assets --root DIR --results FILE --out DIR
//!                        [--summary FILE] [--stress FILE]
//!                                       render the published SVGs + summary
//!                                       FROM committed artifacts
//! veredictum conformance-assets --root DIR --results FILE --verdicts FILE
//!                               --out DIR [--suffix=-ehrbase]
//!                                       render the capability heat grid +
//!                                       per-chapter outcome bars FROM the
//!                                       committed party artifacts
//! ```
//!
//! Every subcommand is a clap front end over one seam of
//! [`veredictum::pipeline`]: this file parses arguments, renders the seam's
//! typed result to the console, and maps it to an exit code. Nothing here
//! decides anything a second consumer of the library would have to decide
//! again.
//!
//! Exit codes: `0` clean · `1` findings · `2` runner error.

// Verification CLI: progress/diagnostics on the console ARE this tool's user
// interface, so stdio is the right channel here; only library crates are
// restricted to `tracing`.
#![allow(
    clippy::print_stdout,
    clippy::print_stderr,
    reason = "this IS the CLI: stdout carries the run report and stderr the \
              diagnostics; only library crates are restricted to `tracing`"
)]

use std::path::{Path, PathBuf};
use std::process::ExitCode;

use clap::{Parser, Subcommand};

use veredictum::bench::client::AuthKind;
use veredictum::pipeline::assets::{
    conformance_assets, performance_assets, schema_files, stress_overlay,
};
use veredictum::pipeline::bench::{BenchRequest, compare_bench, run_bench};
use veredictum::pipeline::catalogue::{coverage_report_path, validate_tree, write_coverage_report};
use veredictum::pipeline::conformance::{RunRequest, RunWarning, execute_run};
use veredictum::pipeline::judgement::{JudgementRequest, judge};
use veredictum::pipeline::measured::{
    MeasuredEvent, MeasuredRequest, ProbeRequest, StressRequest, SustainedWindow, run_aql_probe,
    run_measured, run_stress,
};
use veredictum::pipeline::{RenderedFile, ensure_parent_dir, to_json_document, write_file};
use veredictum::record::{
    DigestOutcome, HONESTY_LINE, MANIFEST_FILE, RecordedFile, SIGNATURE_FILE, SignatureOutcome,
    seal, verify_bundle,
};
use veredictum::transcript::Recording;

#[derive(Parser)]
#[command(
    name = "veredictum",
    about = "The independent conformance instrument for openEHR clinical data repositories",
    version
)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Write the published JSON-Schema set (byte-deterministic).
    EmitSchemas {
        /// Output directory (created if missing).
        #[arg(long)]
        out: PathBuf,
    },
    /// Execute the catalogue against a live SUT (from its ixit topology)
    /// and emit results.json + the run report.
    Run {
        /// The artifact root.
        #[arg(long)]
        root: PathBuf,
        /// The ixit topology file (JSON).
        #[arg(long)]
        ixit: PathBuf,
        /// Output directory for results.json + the run summary.
        #[arg(long)]
        out: PathBuf,
        /// SUT display name.
        #[arg(long, default_value = "ferroehr")]
        sut_name: String,
        /// SUT version label.
        #[arg(long, default_value = "dev")]
        sut_version: String,
        /// Only run cases whose id contains this substring.
        #[arg(long)]
        filter: Option<String>,
        /// The party statement (ICS) — when supplied, option-gated cases
        /// whose option the ICS does not declare are recorded N/A at drive
        /// time (ISO/IEC 9646 test selection) instead of driven.
        #[arg(long)]
        statement: Option<PathBuf>,
        /// An armored OpenPGP secret key — when supplied, the emitted
        /// documents are sealed with `record-manifest.json` and its detached
        /// signature, which `verify-record` and `gpg --verify` both check.
        #[arg(long)]
        sign_key: Option<PathBuf>,
        /// The passphrase unlocking `--sign-key`. Supply it through the
        /// environment: a passphrase on the command line is visible to every
        /// process on the host.
        #[arg(long, env = "VEREDICTUM_SIGN_PASSPHRASE", hide_env_values = true)]
        sign_passphrase: Option<String>,
        /// Persist the wire exchanges beside results.json as transcript.json.
        ///
        /// OFF by default. The artifact can carry real patient data — a SUT's
        /// response body is recorded verbatim — so it is operator-controlled
        /// output, never a log: store it where the record itself is stored.
        /// The `authorization` request header's value is withheld; everything
        /// else lands exactly as it went out and came back.
        #[arg(long)]
        record_exchanges: bool,
        /// Print one machine-parseable line per processed case:
        /// `progress: <k>/<n> <case-id>` after `progress: 0/<n>` — a stable
        /// grammar a driver may parse. Off by default, so existing output is
        /// byte-identical without it.
        #[arg(long)]
        progress: bool,
    },
    /// Validate one artifact tree through every machine gate.
    Validate {
        /// The artifact root (schedule/, bindings/, vocab/, corpus/, registers/).
        #[arg(long)]
        root: PathBuf,
        /// The vendored openEHR spec tree; enables SM-operation and spec-ref
        /// resolution.
        #[arg(long)]
        specs: Option<PathBuf>,
        /// Also refresh `<ROOT>/coverage-report.md` from `--specs`.
        ///
        /// OFF by default: `validate` is a check verb, and a check that
        /// mutates the working tree is a trap for read-only and fenced
        /// invocations. The pipeline scripts that publish the report pass
        /// this explicitly.
        #[arg(long)]
        write_report: bool,
    },
    /// Execute the performance schedule's open-loop measured run(s) against
    /// a live SUT and merge the §8.10 measurement records into an existing
    /// results.json (conformance-by-measurement).
    Perf {
        /// The artifact root.
        #[arg(long)]
        root: PathBuf,
        /// The ixit topology file (JSON) — its environment block is
        /// mandatory for a measured run.
        #[arg(long)]
        ixit: PathBuf,
        /// The results.json to merge the measurement records into (written
        /// by a prior `run`).
        #[arg(long)]
        results: PathBuf,
        /// Select the performance case(s) of this class (POC | S | L | R).
        #[arg(long)]
        class: String,
        /// Parallel seeding workers.
        #[arg(long, default_value_t = 16)]
        seed_workers: usize,
        /// The sustained-window ladder: hours to hold the offered load —
        /// 1 (default, the case's normative window) | 2 | 4 | 6 | 8 | 12.
        /// A longer window is a STRICTER demonstration and persists like
        /// any measured run; nothing shorter than the case exists.
        #[arg(long, default_value_t = 1)]
        hours: u64,
    },
    /// Run the step-load STRESS instrument: geometric load steps to the
    /// maximum sustainable throughput — where the system breaks
    /// (exploration only; never a conformance record, and class-free by
    /// design).
    Stress {
        /// The artifact root.
        #[arg(long)]
        root: PathBuf,
        /// The ixit topology file (JSON) — its environment block is
        /// mandatory (a throughput number without the deployment described
        /// is meaningless).
        #[arg(long)]
        ixit: PathBuf,
        /// Where to write the stress report (stress.json).
        #[arg(long)]
        out: PathBuf,
        /// The class-scale corpus the stress runs on (POC | S | L | R —
        /// the standardized corpus selector): data volume + workload mix
        /// only; no class floor enters the stress report or chart.
        #[arg(long, default_value = "POC")]
        corpus_class: String,
        /// Parallel seeding workers.
        #[arg(long, default_value_t = 16)]
        seed_workers: usize,
        /// Each load step's recorded hold, seconds (short + intense by
        /// design).
        #[arg(long, default_value_t = 120)]
        step_secs: u64,
        /// Post-breach bisection refinements.
        #[arg(long, default_value_t = 3)]
        bisections: u32,
        /// The climb cap (arrivals/s).
        #[arg(long, default_value_t = 4096.0)]
        max_rate: f64,
    },
    /// Run the universal SPEED benchmark: an embedded pack against any
    /// reachable CDR over its base URL, seeded once and measured N times
    /// open-loop (comparative speed only; never a conformance record, a
    /// certificate, or a performance-class rating).
    Bench {
        /// The system's base URL, up to and including the openEHR REST base
        /// (for example `https://cdr.example/openehr/v1`).
        #[arg(long)]
        base_url: String,
        /// How the client presents itself: `none`, `basic` or `bearer`.
        ///
        /// The secret never rides argv: `basic` reads
        /// `VEREDICTUM_BENCH_PASSWORD` and `bearer` reads
        /// `VEREDICTUM_BENCH_TOKEN`.
        #[arg(long, default_value = "none")]
        auth: String,
        /// The user `--auth basic` presents.
        #[arg(long)]
        user: Option<String>,
        /// The embedded pack to drive.
        #[arg(long, default_value = "smoke")]
        pack: String,
        /// The posture profile to declare, out of the set the pack defines.
        /// Omit to take the pack's first, which is always `minimal`.
        ///
        /// The run's canaries check the declaration against the running system
        /// before and after the measured window, and a contradiction refuses
        /// the run.
        #[arg(long)]
        posture: Option<String>,
        /// How many times to repeat the measured phases. A result with fewer
        /// than three is recorded as not submittable, and names that as one of
        /// its unmet requirements.
        #[arg(long, default_value_t = 3)]
        repetitions: u32,
        /// Multiply the pack's EHR count by this factor, for a shorter run.
        /// Anything but `1.0` takes the run off the pack's pinned
        /// configuration, and the record says so.
        #[arg(long, default_value_t = 1.0)]
        scale: f64,
        /// Override the worker count every seed phase declares. Omit to run
        /// the pack's own value, which is what its reference figures describe.
        #[arg(long)]
        seed_workers: Option<usize>,
        /// After the target's run, compose each pinned reference CDR on this
        /// host from its own digest-pinned images, drive the same pack at the
        /// same seed against it, and record the relative index. Needs the
        /// docker CLI; a record without a baseline is not submittable.
        #[arg(long)]
        with_baselines: bool,
        /// Output directory for the result document and its summary.
        #[arg(long)]
        out: PathBuf,
        /// A label for this run, which names its column in a comparison and
        /// distinguishes its file name in a shared output directory.
        #[arg(long)]
        label: Option<String>,
    },
    /// Align two or more committed bench results into one table, one column
    /// per file, with every pack or host mismatch stated in the header.
    BenchCompare {
        /// A committed bench result. Repeat the flag once per file; at least
        /// two are needed.
        #[arg(long = "result", required = true)]
        results: Vec<PathBuf>,
        /// Output directory for the rendered comparison.
        #[arg(long)]
        out: PathBuf,
    },
    /// Run the AQL optimization probe: fire the measurement machinery's
    /// AQL vocabulary against a live, freshly seeded SUT, record wire
    /// percentiles, and attribute the DB-side cost per probe via
    /// `pg_stat_statements` (exploration evidence for the optimization
    /// loop — never a conformance record).
    AqlProbe {
        /// The artifact root.
        #[arg(long)]
        root: PathBuf,
        /// The ixit topology file (JSON) — the `containers` block enables
        /// DB-side attribution and maintenance settling.
        #[arg(long)]
        ixit: PathBuf,
        /// Where to write the probe report (aql-probe.json).
        #[arg(long)]
        out: PathBuf,
        /// The class-scale corpus the probes run against (POC | S | L | R).
        #[arg(long, default_value = "POC")]
        corpus_class: String,
        /// Parallel seeding workers.
        #[arg(long, default_value_t = 16)]
        seed_workers: usize,
        /// Requests fired per probe.
        #[arg(long, default_value_t = 20)]
        requests: u32,
    },
    /// Render the cross-SUT stress overlay (both systems' latency-throughput
    /// curves on one canvas) FROM two committed stress reports —
    /// deterministic, both directions on equal footing.
    StressCompare {
        /// The primary SUT's committed stress.json.
        #[arg(long)]
        left: PathBuf,
        /// The primary SUT's display label.
        #[arg(long)]
        left_label: String,
        /// The comparison SUT's committed stress.json.
        #[arg(long)]
        right: PathBuf,
        /// The comparison SUT's display label.
        #[arg(long)]
        right_label: String,
        /// Where to write the overlay SVG.
        #[arg(long)]
        out: PathBuf,
    },
    /// Render the published performance SVG assets FROM a committed
    /// results.json (deterministic; CI regenerates and diffs — hand-drawn
    /// numbers are a build failure).
    PerfAssets {
        /// The artifact root (for the class-ladder floors).
        #[arg(long)]
        root: PathBuf,
        /// The committed results.json carrying the measurement records.
        #[arg(long)]
        results: PathBuf,
        /// Output directory for the SVG files.
        #[arg(long)]
        out: PathBuf,
        /// Also write the generated Markdown summary (class ladder +
        /// measured detail) to this path — the book's build-time include.
        #[arg(long)]
        summary: Option<PathBuf>,
        /// A committed stress report (stress.json) to render the
        /// latency-throughput curve from, when one exists.
        #[arg(long)]
        stress: Option<PathBuf>,
    },
    /// Render the conformance visuals (the capability heat grid + the
    /// per-chapter outcome bars) deterministically FROM the committed party
    /// artifacts — the perf-assets pattern for functional conformance.
    ConformanceAssets {
        /// The artifact root (for the capability matrix).
        #[arg(long)]
        root: PathBuf,
        /// The committed results.json.
        #[arg(long)]
        results: PathBuf,
        /// The committed verdicts.json.
        #[arg(long)]
        verdicts: PathBuf,
        /// Output directory for the SVG files.
        #[arg(long)]
        out: PathBuf,
        /// A suffix appended to the SVG file stems (`-ehrbase` for the
        /// comparison SUT's copies).
        #[arg(long, default_value = "")]
        suffix: String,
    },
    /// Compute the verdicts from a statement + results against an artifact
    /// tree (the pure pipeline) and write the rendered submission documents.
    Verdicts {
        /// The party statement (`statement.json`).
        #[arg(long)]
        statement: PathBuf,
        /// The party results (`results.json`).
        #[arg(long)]
        results: PathBuf,
        /// The artifact root (schedule/, vocab/, registers/).
        #[arg(long)]
        root: PathBuf,
        /// Output directory for the rendered documents + verdicts.json.
        #[arg(long)]
        out: PathBuf,
        /// An armored OpenPGP secret key — when supplied, the rendered
        /// documents are sealed with `record-manifest.json` and its detached
        /// signature, which `verify-record` and `gpg --verify` both check.
        #[arg(long)]
        sign_key: Option<PathBuf>,
        /// The passphrase unlocking `--sign-key`. Supply it through the
        /// environment: a passphrase on the command line is visible to every
        /// process on the host.
        #[arg(long, env = "VEREDICTUM_SIGN_PASSPHRASE", hide_env_values = true)]
        sign_passphrase: Option<String>,
    },
    /// Verify a sealed bundle: recompute every digest its record manifest
    /// names, and check the detached signature over that manifest.
    VerifyRecord {
        /// The bundle directory holding the emitted documents, the record
        /// manifest and its detached signature.
        #[arg(long)]
        record: PathBuf,
        /// The armored OpenPGP public key the signature is checked against.
        #[arg(long)]
        key: PathBuf,
    },
}

/// The optional signing posture a document-emitting command was invoked with.
///
/// The passphrase never leaves this struct: no `Debug`, and nothing prints it.
struct Signing {
    /// The armored secret key file, when one was supplied.
    key: Option<PathBuf>,
    /// The passphrase unlocking that key, when it needs one.
    passphrase: Option<String>,
}

/// Report a seam's failure and stop with the runner-error code.
fn fail<E: std::fmt::Display>(e: &E) -> ExitCode {
    eprintln!("{e}");
    ExitCode::from(2)
}

/// Write finished artifacts into `out`, one console line per file.
fn emit(out: &Path, files: &[RenderedFile]) -> Result<(), ExitCode> {
    if let Err(e) = std::fs::create_dir_all(out) {
        eprintln!("cannot create {}: {e}", out.display());
        return Err(ExitCode::from(2));
    }
    for file in files {
        let path = out.join(&file.name);
        if let Err(e) = std::fs::write(&path, &file.body) {
            eprintln!("cannot write {}: {e}", path.display());
            return Err(ExitCode::from(2));
        }
        println!("wrote {}", path.display());
    }
    Ok(())
}

/// Write one finished document to `path`, creating its parent directory.
fn emit_one(path: &Path, body: &str) -> Result<(), ExitCode> {
    if let Err(e) = ensure_parent_dir(path) {
        eprintln!("{e}");
        return Err(ExitCode::from(2));
    }
    if let Err(e) = write_file(path, body) {
        eprintln!("{e}");
        return Err(ExitCode::from(2));
    }
    Ok(())
}

#[expect(
    clippy::too_many_lines,
    reason = "the dispatch table: one arm per subcommand, each binding its own flags"
)]
fn main() -> ExitCode {
    match Cli::parse().command {
        Command::EmitSchemas { out } => emit_schemas_command(&out),
        Command::Run {
            root,
            ixit,
            out,
            sut_name,
            sut_version,
            filter,
            statement,
            sign_key,
            sign_passphrase,
            record_exchanges,
            progress,
        } => run_command(
            &RunRequest {
                root: &root,
                ixit: &ixit,
                out_dir: &out,
                sut_name: &sut_name,
                sut_version: &sut_version,
                filter: filter.as_deref(),
                statement: statement.as_deref(),
                recording: Recording::from(record_exchanges),
            },
            &Signing {
                key: sign_key,
                passphrase: sign_passphrase,
            },
            progress,
        ),
        Command::Validate {
            root,
            specs,
            write_report,
        } => validate_command(&root, specs.as_deref(), write_report),
        Command::Perf {
            root,
            ixit,
            results,
            class,
            seed_workers,
            hours,
        } => perf_command(&root, &ixit, &results, &class, seed_workers, hours),
        Command::Stress {
            root,
            ixit,
            out,
            corpus_class,
            seed_workers,
            step_secs,
            bisections,
            max_rate,
        } => stress_command(
            &StressRequest {
                root: &root,
                ixit: &ixit,
                corpus_class: &corpus_class,
                seed_workers,
                step_secs,
                bisections,
                max_rate,
            },
            &out,
        ),
        Command::Bench {
            base_url,
            auth,
            user,
            pack,
            posture,
            repetitions,
            scale,
            seed_workers,
            with_baselines,
            out,
            label,
        } => bench_command(
            &BenchInvocation {
                base_url: &base_url,
                auth_token: &auth,
                user: user.as_deref(),
                pack_token: &pack,
                posture_token: posture.as_deref(),
                repetitions,
                scale,
                seed_workers,
                with_baselines,
                label: label.as_deref(),
            },
            &out,
        ),
        Command::BenchCompare { results, out } => bench_compare_command(&results, &out),
        Command::AqlProbe {
            root,
            ixit,
            out,
            corpus_class,
            seed_workers,
            requests,
        } => probe_command(
            &ProbeRequest {
                root: &root,
                ixit: &ixit,
                corpus_class: &corpus_class,
                seed_workers,
                requests,
            },
            &out,
        ),
        Command::StressCompare {
            left,
            left_label,
            right,
            right_label,
            out,
        } => stress_compare_command(&left, &left_label, &right, &right_label, &out),
        Command::PerfAssets {
            root,
            results,
            out,
            summary,
            stress,
        } => perf_assets_command(&root, &results, &out, summary.as_deref(), stress.as_deref()),
        Command::ConformanceAssets {
            root,
            results,
            verdicts,
            out,
            suffix,
        } => conformance_assets_command(&root, &results, &verdicts, &out, &suffix),
        Command::Verdicts {
            statement,
            results,
            root,
            out,
            sign_key,
            sign_passphrase,
        } => verdicts_command(
            &JudgementRequest {
                statement: &statement,
                results: &results,
                root: &root,
            },
            &out,
            &Signing {
                key: sign_key,
                passphrase: sign_passphrase,
            },
        ),
        Command::VerifyRecord { record, key } => verify_record_command(&record, &key),
    }
}

fn emit_schemas_command(out: &Path) -> ExitCode {
    match emit(out, &schema_files()) {
        Ok(()) => ExitCode::SUCCESS,
        Err(code) => code,
    }
}

fn validate_command(root: &Path, specs: Option<&Path>, write_report: bool) -> ExitCode {
    let validation = match validate_tree(root, specs) {
        Ok(validation) => validation,
        Err(e) => return fail(&e),
    };
    for finding in &validation.findings {
        println!("{finding}");
    }
    if write_report && let Some(specs) = specs {
        let path = coverage_report_path(root);
        match write_coverage_report(&validation.loaded.set, specs, &path) {
            Ok(()) => println!("wrote {}", path.display()),
            Err(e) => eprintln!("warning: {e}"),
        }
    }
    println!(
        "{} case(s), {} binding(s), {} party statement(s), {} finding(s)",
        validation.loaded.set.cases.len(),
        validation.loaded.set.bindings.len(),
        validation.loaded.set.parties.len(),
        validation.findings.len()
    );
    if validation.is_clean() {
        ExitCode::SUCCESS
    } else {
        ExitCode::from(1)
    }
}

/// Seal the documents a command just emitted, when a signing key was supplied.
///
/// The manifest and its detached signature are written beside the documents
/// they cover, so the bundle stays ordinary files.
fn seal_emitted(out: &Path, files: &[RecordedFile<'_>], signing: &Signing) -> Result<(), ExitCode> {
    let Some(key_path) = signing.key.as_deref() else {
        return Ok(());
    };
    let sealed = match seal(files, key_path, signing.passphrase.as_deref()) {
        Ok(sealed) => sealed,
        Err(e) => return Err(fail(&e)),
    };
    emit(
        out,
        &[
            RenderedFile {
                name: MANIFEST_FILE.to_owned(),
                body: sealed.manifest,
            },
            RenderedFile {
                name: SIGNATURE_FILE.to_owned(),
                body: sealed.signature,
            },
        ],
    )
}

/// The bundle-relative name a pipeline-produced path carries.
fn record_name(path: &Path) -> Result<&str, ExitCode> {
    let Some(name) = path.file_name().and_then(std::ffi::OsStr::to_str) else {
        eprintln!("cannot name {} in the record manifest", path.display());
        return Err(ExitCode::from(2));
    };
    Ok(name)
}

fn verify_record_command(record: &Path, key: &Path) -> ExitCode {
    let verification = match verify_bundle(record, key) {
        Ok(verification) => verification,
        Err(e) => return fail(&e),
    };
    match &verification.signature {
        SignatureOutcome::Accepted(signed) => {
            println!("signer fingerprint {}", signed.signer_fingerprint);
            println!("signed at          {}", signed.signed_at);
        }
        SignatureOutcome::Rejected => println!(
            "signature          REJECTED — no component of {} verified {MANIFEST_FILE}",
            key.display()
        ),
    }
    println!(
        "instrument         {} {}",
        verification.instrument.name, verification.instrument.version
    );
    for file in &verification.files {
        match &file.outcome {
            DigestOutcome::Matches => println!("  ok         {} {}", file.digest, file.name),
            DigestOutcome::Mismatch { recomputed } => println!(
                "  MISMATCH   {} {} (recomputed {recomputed})",
                file.digest, file.name
            ),
            DigestOutcome::Missing => println!("  MISSING    {} {}", file.digest, file.name),
            DigestOutcome::Unreadable { message } => {
                println!("  UNREADABLE {} {} ({message})", file.digest, file.name);
            }
        }
    }
    println!("{HONESTY_LINE}");
    if verification.is_clean() {
        ExitCode::SUCCESS
    } else {
        for finding in verification.findings() {
            eprintln!("record: {finding}");
        }
        ExitCode::from(1)
    }
}

fn verdicts_command(request: &JudgementRequest<'_>, out: &Path, signing: &Signing) -> ExitCode {
    let judgement = match judge(request) {
        Ok(judgement) => judgement,
        Err(e) => return fail(&e),
    };
    if let Err(code) = emit(out, &judgement.documents) {
        return code;
    }
    let sealed: Vec<RecordedFile<'_>> = judgement
        .documents
        .iter()
        .map(|file| RecordedFile {
            name: &file.name,
            body: file.body.as_bytes(),
        })
        .collect();
    if let Err(code) = seal_emitted(out, &sealed, signing) {
        return code;
    }
    for finding in &judgement.report.review {
        println!("static-review: {}", finding.message);
    }
    println!(
        "{} capability verdict(s), {} of {} cases driven, {} review finding(s)",
        judgement.report.capabilities.len(),
        judgement.report.coverage.driven,
        judgement.report.coverage.selected,
        judgement.report.review.len(),
    );
    if judgement.is_clean() {
        ExitCode::SUCCESS
    } else {
        ExitCode::from(1)
    }
}

fn conformance_assets_command(
    root: &Path,
    results: &Path,
    verdicts: &Path,
    out: &Path,
    suffix: &str,
) -> ExitCode {
    let files = match conformance_assets(root, results, verdicts, suffix) {
        Ok(files) => files,
        Err(e) => return fail(&e),
    };
    match emit(out, &files) {
        Ok(()) => ExitCode::SUCCESS,
        Err(code) => code,
    }
}

fn perf_assets_command(
    root: &Path,
    results: &Path,
    out: &Path,
    summary: Option<&Path>,
    stress: Option<&Path>,
) -> ExitCode {
    let assets = match performance_assets(root, results, stress) {
        Ok(assets) => assets,
        Err(e) => return fail(&e),
    };
    if let Err(code) = emit(out, &assets.files) {
        return code;
    }
    if let Some(path) = summary {
        let body = match assets.summary_markdown() {
            Ok(body) => body,
            Err(e) => return fail(&e),
        };
        if let Err(code) = emit_one(path, &body) {
            return code;
        }
        println!("wrote {}", path.display());
    }
    ExitCode::SUCCESS
}

fn stress_compare_command(
    left: &Path,
    left_label: &str,
    right: &Path,
    right_label: &str,
    out: &Path,
) -> ExitCode {
    let svg = match stress_overlay((left_label, left), (right_label, right)) {
        Ok(svg) => svg,
        Err(e) => return fail(&e),
    };
    if let Err(code) = emit_one(out, &svg) {
        return code;
    }
    println!("wrote {}", out.display());
    ExitCode::SUCCESS
}

fn stress_command(request: &StressRequest<'_>, out: &Path) -> ExitCode {
    let progress = |message: String| eprintln!("[stress] {message}");
    let report = match run_stress(request, &progress) {
        Ok(report) => report,
        Err(e) => return fail(&e),
    };
    let document = match to_json_document(&report, "serialize") {
        Ok(document) => document,
        Err(e) => return fail(&e),
    };
    if let Err(code) = emit_one(out, &document) {
        return code;
    }
    println!("{}", report.remark);
    println!(
        "wrote {} ({} steps, max sustainable {:.1}/s)",
        out.display(),
        report.steps.len(),
        report.max_sustainable_throughput_per_s
    );
    ExitCode::SUCCESS
}

/// What one `bench` invocation asked for, so the command takes one argument
/// rather than eight positional ones that a swap would silently reorder.
struct BenchInvocation<'a> {
    base_url: &'a str,
    auth_token: &'a str,
    user: Option<&'a str>,
    pack_token: &'a str,
    posture_token: Option<&'a str>,
    repetitions: u32,
    scale: f64,
    seed_workers: Option<usize>,
    with_baselines: bool,
    label: Option<&'a str>,
}

fn bench_command(invocation: &BenchInvocation<'_>, out: &Path) -> ExitCode {
    let auth = match AuthKind::parse(invocation.auth_token) {
        Ok(auth) => auth,
        Err(e) => return fail(&e),
    };
    let pack = match veredictum::bench::pack::load(invocation.pack_token) {
        Ok(pack) => pack,
        Err(e) => return fail(&e),
    };
    let profile = match pack.resolve_profile(invocation.posture_token) {
        Ok(profile) => profile,
        Err(e) => return fail(&e),
    };
    let progress = |message: String| eprintln!("[bench] {message}");
    let outcome = match run_bench(
        &BenchRequest {
            pack: &pack,
            profile,
            base_url: invocation.base_url,
            auth,
            user: invocation.user,
            repetitions: invocation.repetitions,
            label: invocation.label,
            scale: invocation.scale,
            seed_workers: invocation.seed_workers,
            with_baselines: invocation.with_baselines,
            docker: None,
        },
        &progress,
    ) {
        Ok(outcome) => outcome,
        Err(e) => return fail(&e),
    };
    if let Err(code) = emit(out, &outcome.documents) {
        return code;
    }
    println!("{}", veredictum::bench::BOUNDARY_STATEMENT);
    println!(
        "machine: {}",
        veredictum::bench::render::machine_line(&outcome.result.environment)
    );
    println!(
        "{} repetition(s) over pack {}@{}; submittable: {}",
        outcome.result.repetitions.len(),
        outcome.result.pack.id,
        outcome.result.pack.version,
        outcome.result.submittable
    );
    println!("posture `{}`:", outcome.result.posture.profile);
    for line in &outcome.result.posture.items {
        println!("  {} = {} ({})", line.item, line.declared, line.assurance);
    }
    for requirement in &outcome.result.submittable_unmet {
        println!(
            "not submittable, unmet `{requirement}`: {}",
            requirement.statement()
        );
    }
    for index in &outcome.result.relative {
        println!(
            "vs {}: {} indexed operation(s), {} gap(s)",
            index.display_name,
            index
                .phases
                .values()
                .map(|phase| phase.operations.len())
                .sum::<usize>(),
            index.gaps.len()
        );
    }
    if !outcome.result.scale.reference_configuration {
        println!(
            "scale factor {:.3}: this run is off the pack's pinned configuration, so its numbers are not comparable with the reference figures the pack describes",
            outcome.result.scale.factor
        );
    }
    ExitCode::SUCCESS
}

fn bench_compare_command(results: &[PathBuf], out: &Path) -> ExitCode {
    let outcome = match compare_bench(results) {
        Ok(outcome) => outcome,
        Err(e) => return fail(&e),
    };
    println!("{}", outcome.document.body);
    if let Err(code) = emit(out, std::slice::from_ref(&outcome.document)) {
        return code;
    }
    if outcome.comparison.warnings.is_empty() {
        ExitCode::SUCCESS
    } else {
        for warning in &outcome.comparison.warnings {
            eprintln!("bench-compare: {warning}");
        }
        ExitCode::from(1)
    }
}

fn probe_command(request: &ProbeRequest<'_>, out: &Path) -> ExitCode {
    let progress = |message: String| eprintln!("[probe] {message}");
    let report = match run_aql_probe(request, &progress) {
        Ok(report) => report,
        Err(e) => return fail(&e),
    };
    let document = match to_json_document(&report, "serialize") {
        Ok(document) => document,
        Err(e) => return fail(&e),
    };
    if let Err(code) = emit_one(out, &document) {
        return code;
    }
    println!("wrote {} ({} probes)", out.display(), report.probes.len());
    ExitCode::SUCCESS
}

fn perf_command(
    root: &Path,
    ixit: &Path,
    results: &Path,
    class_token: &str,
    seed_workers: usize,
    hours: u64,
) -> ExitCode {
    let Some(window) = SustainedWindow::hours(hours) else {
        eprintln!("--hours must be one of 1 | 2 | 4 | 6 | 8 | 12 (got {hours})");
        return ExitCode::from(2);
    };
    let request = MeasuredRequest {
        root,
        ixit,
        results,
        class: class_token,
        seed_workers,
        window,
    };
    let observe = |event: MeasuredEvent<'_>| match event {
        MeasuredEvent::Progress(message) => eprintln!("[perf] {message}"),
        MeasuredEvent::CaseStarted { case, source } => println!(
            "case {} (class {class_token}) from {}",
            case.id,
            source.display()
        ),
        MeasuredEvent::Measured(measurement) => {
            for op in &measurement.operations {
                println!(
                    "  {}: {} requests, {} errors, p50 {:.1}ms p90 {:.1}ms p99 {:.1}ms",
                    op.operation,
                    op.requests,
                    op.errors,
                    op.latency_ms_p50,
                    op.latency_ms_p90,
                    op.latency_ms_p99
                );
            }
            println!("  {}", veredictum::perf::verdict_evidence(measurement));
        }
        MeasuredEvent::PrunedOrphan(case) => {
            println!("  pruned orphaned measurement for retired case {case}");
        }
        MeasuredEvent::Merged(path) => {
            println!("  measurement merged into {}", path.display());
        }
    };
    match run_measured(&request, &observe) {
        Ok(run) if run.earned_all => ExitCode::SUCCESS,
        Ok(_) => ExitCode::from(1),
        Err(e) => fail(&e),
    }
}

/// Says where the transcript landed, with the caution its content earns.
fn report_transcript(outcome: &veredictum::pipeline::conformance::RunOutcome) {
    let Some(path) = &outcome.transcript_path else {
        return;
    };
    let exchanges: usize = outcome
        .report
        .transcripts
        .iter()
        .map(|case| case.exchanges.len())
        .sum();
    println!(
        "wrote {} ({exchanges} recorded exchange(s)) — it can carry real patient data; store it as you store the record",
        path.display()
    );
}

/// Builds and writes every document a run emits, paired with the name each
/// one carries in the record manifest.
///
/// The transcript is written before the manifest is built, so a sealed bundle
/// covers every document the run emitted.
///
/// # Errors
///
/// Returns the process exit code the failure earns: a rendering failure or an
/// unwritable path fails through [`fail`], and a path with no file name is a
/// usage error.
fn emit_documents(
    outcome: &veredictum::pipeline::conformance::RunOutcome,
) -> Result<Vec<(&str, String)>, ExitCode> {
    let document = match outcome.results_document() {
        Ok(document) => document,
        Err(e) => return Err(fail(&e)),
    };
    if let Err(e) = write_file(&outcome.results_path, &document) {
        return Err(fail(&e));
    }
    let exceptions = match outcome.exceptions_document() {
        Ok(document) => document,
        Err(e) => return Err(fail(&e)),
    };
    if let Err(e) = write_file(&outcome.exceptions_path, &exceptions) {
        return Err(fail(&e));
    }
    let transcript = match outcome.transcript_document() {
        Ok(transcript) => transcript,
        Err(e) => return Err(fail(&e)),
    };
    if let (Some(body), Some(path)) = (transcript.as_ref(), outcome.transcript_path.as_ref())
        && let Err(e) = write_file(path, body)
    {
        return Err(fail(&e));
    }
    let names = match (
        record_name(&outcome.results_path),
        record_name(&outcome.exceptions_path),
    ) {
        (Ok(results), Ok(exceptions)) => [results, exceptions],
        (Err(code), _) | (_, Err(code)) => return Err(code),
    };
    let [results_name, exceptions_name] = names;
    let mut emitted = vec![(results_name, document), (exceptions_name, exceptions)];
    if let (Some(body), Some(path)) = (transcript, outcome.transcript_path.as_ref()) {
        emitted.push((record_name(path)?, body));
    }
    Ok(emitted)
}

fn run_command(request: &RunRequest<'_>, signing: &Signing, progress: bool) -> ExitCode {
    let warn = |warning: RunWarning<'_>| match warning {
        RunWarning::CarriedMeasurements {
            count,
            measured_at,
            running_at,
        } => eprintln!(
            "warning: carrying {count} measurement record(s) taken at SUT version {measured_at} into a run at {running_at} — re-measure or attest the surface unchanged"
        ),
    };
    // The progress stream is line-flushed on purpose: a driver reads this
    // through a pipe, where stdout is block-buffered and an unflushed line
    // arrives only in bursts.
    let mut report_progress = |event: veredictum::run::Progress<'_>| {
        if progress {
            use std::io::Write as _;
            println!("{}", event.render_line());
            let _flush = std::io::stdout().flush();
        }
    };
    let outcome = match execute_run(request, &warn, &mut report_progress) {
        Ok(outcome) => outcome,
        Err(e) => return fail(&e),
    };
    if let Err(e) = std::fs::create_dir_all(request.out_dir) {
        eprintln!("cannot create {}: {e}", request.out_dir.display());
        return ExitCode::from(2);
    }
    let emitted = match emit_documents(&outcome) {
        Ok(emitted) => emitted,
        Err(code) => return code,
    };
    let sealed: Vec<RecordedFile<'_>> = emitted
        .iter()
        .map(|(name, body)| RecordedFile {
            name,
            body: body.as_bytes(),
        })
        .collect();
    if let Err(code) = seal_emitted(request.out_dir, &sealed, signing) {
        return code;
    }
    println!(
        "{} case-records: {} passed / {} failed / {} errored / {} n-a; interpreter coverage {:.1}% ({} exceptions); wrote {}",
        outcome.report.records.len(),
        outcome.counts.passed,
        outcome.counts.failed,
        outcome.counts.errored,
        outcome.counts.not_applicable,
        outcome.report.interpreter_coverage() * 100.0,
        outcome.report.exceptions.len(),
        outcome.results_path.display()
    );
    report_transcript(&outcome);
    if outcome.is_clean() {
        ExitCode::SUCCESS
    } else {
        ExitCode::from(1)
    }
}
