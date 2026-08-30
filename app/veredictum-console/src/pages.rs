// SPDX-FileCopyrightText: Veredictum contributors
// SPDX-License-Identifier: Apache-2.0

//! The routed surfaces (#61): the shell chrome, and one module per surface.
//! Screens still under construction render an honest placeholder naming
//! their tracker issue instead of pretending.

pub mod benchmarks;
pub mod catalogue;
pub mod instrument;
pub mod not_found;
pub mod results;
pub mod run;
pub mod shell;
pub mod verdicts;
pub mod verify;

use leptos::prelude::{IntoView, view};
use leptos_meta::Title;

use crate::components::empty_state::EmptyState;
use crate::components::page_header::PageHeader;

/// Renders a routed, titled placeholder naming the issue that builds the
/// surface, so an unfinished screen is never a blank pane.
#[must_use]
pub fn under_construction(title: &'static str, purpose: &'static str, issue: u32) -> impl IntoView {
    view! {
        <Title text=format!("{title} · Veredictum console") />
        <PageHeader title=title.to_owned() subtitle=purpose.to_owned() />
        <EmptyState
            icon=icondata_lu::LuHammer
            message=format!("{title} is under construction")
            hint=format!("Tracked as issue #{issue}; the design record is #61.")
        />
    }
}
