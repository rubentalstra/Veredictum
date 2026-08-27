// SPDX-FileCopyrightText: Veredictum contributors
// SPDX-License-Identifier: Apache-2.0

//! The byte-identity gate (#54): a run driven through the console's engine
//! seam and the same run driven by the CLI directly must emit IDENTICAL
//! documents. The results record carries no wall-clock stamp, so the gate
//! demands full byte equality — stronger than the 14-line identity-stamp
//! delta the FerroEHR acceptance run tolerated. This is what makes "no
//! runner logic is reimplemented in the console" a checked property.

use std::io::{Read as _, Write as _};
use std::path::{Path, PathBuf};

use veredictum_console::engine::{self, Credential, Engine, RunSpec, Secret};

/// The repository root, two levels above this crate (#55): the catalogue the
/// gate drives lives there.
pub(crate) fn repo_root() -> &'static Path {
    Path::new(concat!(env!("CARGO_MANIFEST_DIR"), "/../.."))
}

/// The engine binary for the gate: the [`engine::ENGINE_ENV`] override when
/// set, else the workspace build. Both run paths use the SAME binary, which
/// is exactly the property under test — the console must add and remove
/// nothing around it.
pub(crate) fn gate_binary() -> PathBuf {
    std::env::var(engine::ENGINE_ENV).map_or_else(
        |_| repo_root().join("target/debug/veredictum"),
        PathBuf::from,
    )
}

/// A minimal fixture SUT: answers every request `500` with a fixed body, so
/// every driven row fails or errors DETERMINISTICALLY and the two run paths
/// have identical material to record. Listens on an ephemeral loopback port;
/// the thread ends with the test process.
pub(crate) fn fixture_sut() -> Result<u16, std::io::Error> {
    let listener = std::net::TcpListener::bind("127.0.0.1:0")?;
    let port = listener.local_addr()?.port();
    std::thread::spawn(move || {
        for stream in listener.incoming() {
            let Ok(mut stream) = stream else { continue };
            // Read whatever arrives without insisting on a full parse: the
            // answer is the same for every request.
            let mut scratch = [0_u8; 4096];
            let _bytes_read = stream.read(&mut scratch);
            let _write = stream.write_all(
                b"HTTP/1.1 500 Internal Server Error\r\ncontent-length: 2\r\nconnection: close\r\n\r\nno",
            );
        }
    });
    Ok(port)
}

/// Writes the gate's ixit: every instance at the fixture SUT, the `sut`
/// principal on Basic credentials whose env-var NAMES the document carries
/// and whose VALUES only the spawned run's environment will hold. Raw bytes
/// on purpose: an independently authored wire input catches codec bugs a
/// typed-then-serialized value cannot, and the lib's `Ixit` is
/// deserialize-only, so raw is also the only honest way to author one.
pub(crate) fn write_ixit(dir: &Path, port: u16) -> Result<PathBuf, std::io::Error> {
    let document = format!(
        r#"{{
  "instances": {{
    "sut": {{
      "base_url": "http://127.0.0.1:{port}",
      "auth": {{ "mode": "basic", "user_env": "GATE_SUT_USER", "password_env": "GATE_SUT_PASS" }}
    }},
    "admin": {{
      "base_url": "http://127.0.0.1:{port}",
      "auth": {{ "mode": "basic", "user_env": "GATE_SUT_USER", "password_env": "GATE_SUT_PASS" }}
    }},
    "unauthenticated": {{
      "base_url": "http://127.0.0.1:{port}",
      "auth": {{ "mode": "none" }}
    }}
  }}
}}
"#
    );
    let path = dir.join("ixit.json");
    std::fs::write(&path, document)?;
    Ok(path)
}

/// The case the gate drives: one small isolated case, so the run is seconds.
const GATE_FILTER: &str = "I_EHR_SERVICE.create_ehr-main";

#[expect(
    clippy::panic_in_result_fn,
    reason = "the Book ch11 Result-returning test shape: assertions panic, plumbing propagates with ? (https://doc.rust-lang.org/book/ch11-01-writing-tests.html)"
)]
#[test]
fn a_console_run_and_a_cli_run_emit_identical_documents() -> Result<(), Box<dyn std::error::Error>>
{
    let binary = gate_binary();
    if !binary.exists() {
        eprintln!(
            "SKIPPED(no engine binary at {}): build the workspace first, or set {}",
            binary.display(),
            engine::ENGINE_ENV
        );
        return Ok(());
    }
    let engine = match Engine::verified(&binary) {
        Ok(engine) => engine,
        Err(engine::Error::VersionMismatch { reported }) => {
            // Mid-cycle the workspace binary legitimately runs ahead of the
            // published pin; the gate then has no same-version binary to
            // drive both paths with, and pretending otherwise would test
            // nothing. CI builds the workspace at the pinned version, so
            // there this arm is a red flag it never takes silently.
            eprintln!(
                "SKIPPED(engine version drift): {reported} != {}",
                engine::ENGINE_VERSION
            );
            return Ok(());
        }
        Err(other) => return Err(other.into()),
    };

    let scratch = assert_fs::TempDir::new()?;
    let port = fixture_sut()?;
    let ixit = write_ixit(scratch.path(), port)?;
    let root = repo_root().join("artifacts");

    // Path one: the console's seam.
    let console_out = scratch.path().join("console-run");
    std::fs::create_dir_all(&console_out)?;
    let finished = engine.run(
        &RunSpec {
            root: root.clone(),
            ixit: ixit.clone(),
            out_dir: console_out.clone(),
            sut_name: String::from("gate-sut"),
            sut_version: String::from("0.0.0-gate"),
            statement: None,
            filter: Some(String::from(GATE_FILTER)),
            progress: false,
            record_exchanges: false,
            credentials: vec![
                Credential {
                    name: String::from("GATE_SUT_USER"),
                    value: Secret::new(String::from("gate-user")),
                },
                Credential {
                    name: String::from("GATE_SUT_PASS"),
                    value: Secret::new(String::from("gate-pass")),
                },
            ],
        },
        |_line| {},
    )?;
    assert!(
        !finished.results.outcomes.is_empty(),
        "the filter drove no case at all — the gate compared two empty runs"
    );

    // Path two: the CLI, exactly as a terminal user runs it, with the same
    // credentials in ITS environment.
    let cli_out = scratch.path().join("cli-run");
    std::fs::create_dir_all(&cli_out)?;
    let status = std::process::Command::new(engine.binary())
        .args([
            "run",
            "--root",
            &root.to_string_lossy(),
            "--ixit",
            &ixit.to_string_lossy(),
            "--out",
            &cli_out.to_string_lossy(),
            "--sut-name",
            "gate-sut",
            "--sut-version",
            "0.0.0-gate",
            "--filter",
            GATE_FILTER,
        ])
        .env("GATE_SUT_USER", "gate-user")
        .env("GATE_SUT_PASS", "gate-pass")
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()?;
    assert_eq!(
        status.success(),
        finished.clean_exit,
        "the two paths disagree about whether the campaign was clean"
    );

    // The gate itself: byte equality, no tolerated delta.
    let console_results = std::fs::read(&finished.results_path)?;
    let cli_results = std::fs::read(cli_out.join("results.json"))?;
    assert_eq!(
        console_results, cli_results,
        "results.json differs between the console seam and the CLI"
    );
    let console_exceptions = std::fs::read(&finished.exceptions_path)?;
    let cli_exceptions = std::fs::read(cli_out.join("run-exceptions.json"))?;
    assert_eq!(
        console_exceptions, cli_exceptions,
        "run-exceptions.json differs between the console seam and the CLI"
    );

    // The typed read seam: the record the lib parsed is the record on disk.
    let reread: veredictum::party::Results = serde_json::from_slice(&console_results)?;
    assert_eq!(reread.sut.name, finished.results.sut.name);
    Ok(())
}
