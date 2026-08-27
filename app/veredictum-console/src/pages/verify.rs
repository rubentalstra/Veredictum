// SPDX-FileCopyrightText: Veredictum contributors
// SPDX-License-Identifier: Apache-2.0

//! S9 — the public record verification surface (#61). Lands with #68 over
//! the engine's signed-record machinery (#62).

use leptos::prelude::{IntoView, component};

/// The record verification surface.
#[expect(
    clippy::must_use_candidate,
    reason = "a Leptos component is mounted by the framework, never consumed as a value"
)]
#[component]
pub fn Verify() -> impl IntoView {
    crate::pages::under_construction(
        "Verify",
        "Check a published record's signature and digests — trust is good, verification is better.",
        68,
    )
}
