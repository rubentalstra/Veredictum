// SPDX-FileCopyrightText: Veredictum contributors
// SPDX-License-Identifier: Apache-2.0

#![no_main]
//! The citation reader: clause splitting, brace expansion of a path hint, and
//! the candidate forms a cited `§section` is matched by.
//!
//! Every expectation in the catalogue carries a citation, and citation
//! resolution is the gate that makes an expectation traceable to the released
//! spec text. The splitter has already produced one escape class: a fragment
//! that did not open with a component token was dropped unread, so the
//! citations inside it were never resolved and the field went unchecked.
//!
//! Two properties. Nothing panics on any citation text. And the work stays
//! bounded: `expand_braces` documents a 32-variant ceiling, so a citation with
//! many `{a,b}` groups must come back with the tokens unexpanded rather than
//! allocating the cartesian product.

use libfuzzer_sys::fuzz_target;

use veredictum::validate::{citation_clauses, expand_braces, section_candidates};

/// The expansion ceiling `expand_braces` documents; past it the function
/// returns the authored tokens as a single unexpanded variant.
const MAX_VARIANTS: usize = 32;

fuzz_target!(|data: &[u8]| {
    let Ok(s) = std::str::from_utf8(data) else {
        return;
    };

    for clause in citation_clauses(s) {
        let variants = expand_braces(&clause.tokens);
        assert!(
            variants.len() <= MAX_VARIANTS,
            "brace expansion produced {} variants for {:?}, past the documented ceiling",
            variants.len(),
            clause.tokens
        );
        for variant in &variants {
            assert_eq!(
                variant.len(),
                clause.tokens.len(),
                "an expansion variant must carry one token per authored token"
            );
        }
        for section in &clause.sections {
            let candidates = section_candidates(section);
            for candidate in &candidates {
                assert_eq!(
                    candidate.trim(),
                    candidate,
                    "a section candidate is normalized and never re-trimmable"
                );
            }
        }
    }
});
