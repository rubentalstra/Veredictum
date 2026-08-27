// SPDX-FileCopyrightText: Veredictum contributors
// SPDX-License-Identifier: Apache-2.0

#![no_main]
//! The three documents a party supplies: the IXIT declaration, the statement
//! of claims, and the results record.
//!
//! This is the instrument's least trusted input by a wide margin. A statement
//! and a results record are published by the organization whose product is
//! being judged, and re-checking them is the whole point of the verdict
//! pipeline: `veredictum verdicts` reads somebody else's JSON and re-derives
//! the verdict from it. An IXIT is operator-supplied and names endpoints,
//! credentials and file-system paths.
//!
//! The property is that a hostile or merely broken document is refused by
//! type, or reported by an invariant check, and never crashes the reader. The
//! invariant functions are asserted to be TOTAL: they either answer or return
//! their typed error for every value that deserialized.

use std::path::Path;

use libfuzzer_sys::fuzz_target;

use veredictum::ixit::Ixit;
use veredictum::party::{Results, Statement};

fuzz_target!(|data: &[u8]| {
    let Ok(value) = serde_json::from_slice::<serde_json::Value>(data) else {
        return;
    };

    if let Ok(mut ixit) = serde_json::from_value::<Ixit>(value.clone()) {
        // `rebase_paths` rewrites the declared relative paths against the
        // directory the document was read from. It touches no file system.
        ixit.rebase_paths(Path::new("/nonexistent/fuzz"));
        if let Ok(instance) = ixit.default_instance() {
            let _ = ixit.signing_of(instance);
            let _ = ixit.terminology_of(instance);
            let _ = ixit.spec_profile_of(instance);
        }
        for (name, _) in &ixit.instances {
            assert!(
                ixit.instance(name).is_some(),
                "an instance the document declares must be reachable by its own name"
            );
        }
    }

    let _ = serde_json::from_value::<Statement>(value.clone());

    if let Ok(results) = serde_json::from_value::<Results>(value) {
        let _ = results.check_invariants();
        for outcome in &results.outcomes {
            let _ = outcome.check_invariants();
        }
    }
});
