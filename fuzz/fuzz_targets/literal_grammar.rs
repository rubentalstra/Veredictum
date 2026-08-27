// SPDX-FileCopyrightText: Veredictum contributors
// SPDX-License-Identifier: Apache-2.0

#![no_main]
//! The decision-table literal grammar and the violation-category grammar.
//!
//! Every cell of every content-chapter decision table runs through
//! `Literal::from_cell`, and every `violates` entry through
//! `ViolationRef::parse`. Both are hand-written splitters, and the literal
//! grammar is the only RECURSIVE reader in this crate: a list recurses into
//! `from_text` per item, and an ordinal or scale tuple recurses into its
//! symbol. Recursion with no depth bound over attacker-sized text is the
//! stack-overflow shape, and a Rust stack overflow ABORTS the process rather
//! than unwinding, so a validator run would die instead of reporting a
//! finding.
//!
//! The property is the absence of a panic, an abort or a hang. Malformed input
//! is the point: the grammar is expected to refuse most of what arrives, with
//! a typed `LiteralError`.

use libfuzzer_sys::fuzz_target;

use veredictum::literal::{Literal, ViolationRef};

fuzz_target!(|data: &[u8]| {
    let Ok(s) = std::str::from_utf8(data) else {
        return;
    };

    // The authored door: a YAML string cell.
    let _ = Literal::from_text(s);

    // The pipeline door: whatever JSON value the cell decoded to. A string
    // routes back into the grammar; the scalar arms are the cheap ones, and
    // the composite arms must refuse rather than descend.
    let _ = Literal::from_cell(&serde_json::Value::String(s.to_owned()));
    if let Ok(value) = serde_json::from_str::<serde_json::Value>(s) {
        let _ = Literal::from_cell(&value);
    }

    let _ = ViolationRef::parse(s);
});
