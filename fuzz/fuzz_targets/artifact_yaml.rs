// SPDX-FileCopyrightText: Veredictum contributors
// SPDX-License-Identifier: Apache-2.0

#![no_main]
//! The artifact front-end, end to end over one case core: YAML text under the
//! anti-bomb budget, then the published JSON Schema, then the typed model.
//!
//! The YAML stage alone would mostly fuzz `serde-saphyr`, which is why the
//! harness carries the input all the way through. The stage that is ours is
//! the typed parse: `CaseCore` deserializes through every closed grammar in
//! the crate at once — the case-id family, the corpus keys, the `${…}`
//! templates, the capture sources, the SM operation anchors, the decision-table
//! literals — over a value the schema has already accepted, which is a shape
//! no unit test enumerates.
//!
//! The property is that a malformed artifact is REFUSED with a typed
//! `LoadError`, never a panic and never an abort. The budget is under test as
//! well: an alias bomb must be stopped by it rather than by the process
//! running out of memory.

use std::path::Path;
use std::sync::LazyLock;

use libfuzzer_sys::fuzz_target;

use veredictum::load::{compile_schema, validate_against, yaml_str_to_value};
use veredictum::model::case::CaseCore;
use veredictum::schema::case_core_schema;

/// The published case-core schema, compiled once. The schema is built in code
/// rather than read from disk, so the harness stays a pure function of its
/// input.
static CASE_CORE: LazyLock<jsonschema::Validator> = LazyLock::new(|| {
    #[expect(
        clippy::expect_used,
        reason = "a schema this crate emits itself; a failure to compile is a defect in \
                  veredictum::schema, and the validator tests already pin it"
    )]
    compile_schema(&case_core_schema(), "case_core").expect("the emitted case-core schema compiles")
});

fuzz_target!(|data: &[u8]| {
    let Ok(text) = std::str::from_utf8(data) else {
        return;
    };

    let name = Path::new("fuzz.yaml");
    let Ok(value) = yaml_str_to_value(text, name) else {
        return;
    };

    // Schema first, then the typed parse — the order `load_artifact` uses. The
    // typed parse runs whether or not the schema accepted the value, because
    // the schema is a third-party validator and the grammars underneath it are
    // ours: a value the schema wrongly admits must still be refused by type.
    let _ = validate_against(&CASE_CORE, &value, name);
    let _ = serde_json::from_value::<CaseCore>(value);
});
