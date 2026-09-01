// SPDX-FileCopyrightText: Veredictum contributors
// SPDX-License-Identifier: Apache-2.0

//! Emits the engine pin from this package's own manifest.
//!
//! The console names the engine by an exact crates.io version and shows that
//! version in its chrome. The manifest line is the one place it is written:
//! this script reads it and hands it to the crate as `VEREDICTUM_ENGINE_PIN`,
//! so `ENGINE_PIN` is a substitution instead of a hand-typed second copy.
//! It runs on the host for every target, the wasm32 client bundle included
//! (<https://doc.rust-lang.org/cargo/reference/build-scripts.html>).

// A build script speaks to cargo over stdout: that is its protocol, not
// console output (the Cargo book, "Build Scripts").
#![allow(
    clippy::print_stdout,
    reason = "the build-script protocol is line-oriented stdout, per the Cargo book"
)]

// The dependency line the pin is read out of:
// `veredictum = { version = "=0.1.4", optional = true }`.
const DEPENDENCY: &str = "veredictum = ";
const VERSION_KEY: &str = "version = \"=";
const MANIFEST: &str = "Cargo.toml";

fn main() -> Result<(), String> {
    // A build script's working directory is its package's source directory
    // (the Cargo book, "Build Scripts"), so the manifest is a bare name.
    println!("cargo::rerun-if-changed={MANIFEST}");
    let manifest = std::fs::read_to_string(MANIFEST)
        .map_err(|source| format!("cannot read app/veredictum-console/{MANIFEST}: {source}"))?;
    let pin = engine_pin(&manifest).ok_or_else(|| {
        format!(
            "no exact engine pin in app/veredictum-console/{MANIFEST}: the line this script \
             reads starts with `{DEPENDENCY}` and carries `{VERSION_KEY}<version>\"`, and \
             ENGINE_PIN is emitted from it"
        )
    })?;
    println!("cargo::rustc-env=VEREDICTUM_ENGINE_PIN={pin}");
    Ok(())
}

fn engine_pin(manifest: &str) -> Option<&str> {
    let line = manifest.lines().find(|line| line.starts_with(DEPENDENCY))?;
    let pin = line.split_once(VERSION_KEY)?.1.split_once('"')?.0;
    (!pin.is_empty()).then_some(pin)
}
