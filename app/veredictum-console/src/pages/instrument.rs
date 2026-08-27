// SPDX-FileCopyrightText: Veredictum contributors
// SPDX-License-Identifier: Apache-2.0

//! S1 — the instrument landing (#61). The real surface (validate counts,
//! spec pins) lands with #64; until then the honest placeholder.

use leptos::prelude::{IntoView, component};

/// The landing surface.
#[expect(
    clippy::must_use_candidate,
    reason = "a Leptos component is mounted by the framework, never consumed as a value"
)]
#[component]
pub fn Instrument() -> impl IntoView {
    crate::pages::under_construction(
        "Instrument",
        "The catalogue's own numbers, the spec pins, and where to start.",
        64,
    )
}
