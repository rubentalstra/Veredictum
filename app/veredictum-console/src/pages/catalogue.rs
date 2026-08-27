// SPDX-FileCopyrightText: Veredictum contributors
// SPDX-License-Identifier: Apache-2.0

//! S2 — the catalogue explorer (#61). The real surface lands with #64.

use leptos::prelude::{IntoView, component};

/// The catalogue explorer surface.
#[expect(
    clippy::must_use_candidate,
    reason = "a Leptos component is mounted by the framework, never consumed as a value"
)]
#[component]
pub fn Catalogue() -> impl IntoView {
    crate::pages::under_construction(
        "Catalogue",
        "Every chapter, every case, and the citations each expectation stands on.",
        64,
    )
}
