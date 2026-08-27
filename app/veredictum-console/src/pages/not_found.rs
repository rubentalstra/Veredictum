// SPDX-FileCopyrightText: Veredictum contributors
// SPDX-License-Identifier: Apache-2.0

//! The routes' 404 answer (#84).
//!
//! The fallback renders OUTSIDE the route tree, so it wears the chrome by
//! taking it as a wrapper rather than by filling an outlet
//! (<https://book.leptos.dev/router/16_routes.html>). The axum layer sets the
//! status: `file_and_error_handler` answers `404` for any path the generated
//! route list does not carry, so this page is what that status looks like.

use leptos::prelude::{
    AddAnyAttr, ClassAttribute, ElementChild, GetUntracked, IntoView, component, view,
};
use leptos_meta::Title;
use leptos_router::components::A;
use leptos_router::hooks::use_location;
use leptos_router::location::Location;

use crate::components::empty_state::EmptyState;
use crate::components::field::{BTN_PRIMARY, BTN_SECONDARY};
use crate::components::page_header::PageHeader;
use crate::pages::shell::Chrome;

/// The 404 surface: the console's own chrome, the path that missed, and the
/// two routes out of it.
#[expect(
    clippy::must_use_candidate,
    reason = "a Leptos component is mounted by the framework, never consumed as a value"
)]
#[component]
pub fn NotFound() -> impl IntoView {
    let location = use_location();
    // A private helper rather than an inline read, so the component's
    // `must_use_candidate` expectation keeps firing (rules §2).
    let path = missed_path(&location);

    view! {
        <Chrome>
            <Title text="Page not found · Veredictum console" />
            <PageHeader
                title="Page not found"
                subtitle="This console has no surface at that address. Nothing was run, and nothing was judged."
            />
            <EmptyState
                icon=icondata_lu::LuCompass
                message=format!("No route answers {path}")
                hint="The console serves four surfaces: the instrument, the catalogue, the run wizard and the public record check."
            >
                <div class="flex flex-wrap items-center gap-2">
                    <A href="/" attr:class=BTN_PRIMARY>
                        "Back to the instrument"
                    </A>
                    <A href="/catalogue" attr:class=BTN_SECONDARY>
                        "Browse the catalogue"
                    </A>
                </div>
            </EmptyState>
        </Chrome>
    }
}

/// The path the browser asked for, read once at setup.
fn missed_path(location: &Location) -> String {
    let path = location.pathname.get_untracked();
    if path.is_empty() {
        String::from("that address")
    } else {
        path
    }
}
