// SPDX-FileCopyrightText: Veredictum contributors
// SPDX-License-Identifier: Apache-2.0

//! S3–S8 — the run pipeline (#61): connect, scope, live run, results,
//! verdicts, export. The wizard lands across #65–#68.

use leptos::prelude::{IntoView, component};

/// The run pipeline surface.
#[expect(
    clippy::must_use_candidate,
    reason = "a Leptos component is mounted by the framework, never consumed as a value"
)]
#[component]
pub fn Run() -> impl IntoView {
    crate::pages::under_construction(
        "Run",
        "Point the instrument at a reachable CDR and read the verdict.",
        65,
    )
}
