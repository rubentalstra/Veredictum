// SPDX-FileCopyrightText: Veredictum contributors
// SPDX-License-Identifier: Apache-2.0

//! The application shell and route tree.
//!
//! Every rule in `.claude/rules/leptos-ui.md` governs this module: identical
//! view structure on both targets, valid HTML, every routed page sets a
//! `<Title>`, and screens grow as `.into_any()`-erased sections rather than
//! monolithic `view!` trees.

use leptos::prelude::{
    AutoReload, ElementChild, GlobalAttributes, HydrationScripts, IntoView, LeptosOptions,
    component, view,
};
use leptos_meta::{MetaTags, Stylesheet, Title, provide_meta_context};
use leptos_router::{
    StaticSegment,
    components::{Route, Router, Routes},
};

/// The HTML document the server renders around the application: the head with
/// the hydration bootstrap, and the body the client takes over.
#[must_use]
pub fn shell(options: LeptosOptions) -> impl IntoView {
    view! {
        <!DOCTYPE html>
        <html lang="en">
            <head>
                <meta charset="utf-8" />
                <meta name="viewport" content="width=device-width, initial-scale=1" />
                <AutoReload options=options.clone() />
                <HydrationScripts options />
                <MetaTags />
            </head>
            <body>
                <App />
            </body>
        </html>
    }
}

/// The root component: meta context, the stylesheet, and the route tree.
#[component]
#[expect(
    clippy::must_use_candidate,
    reason = "a Leptos component is mounted by the framework, never consumed as a value"
)]
pub fn App() -> impl IntoView {
    provide_meta_context();

    view! {
        <Stylesheet id="leptos" href="/pkg/veredictum-console.css" />
        <Title text="Veredictum console" />
        <Router>
            <main>
                <Routes fallback=|| "Page not found.">
                    <Route path=StaticSegment("") view=Landing />
                </Routes>
            </main>
        </Router>
    }
}

/// The landing page: what the console is, until the real screens land (#6).
#[component]
fn Landing() -> impl IntoView {
    view! {
        <section>
            <h1>"Veredictum console"</h1>
            <p>
                "The web frontend of the independent conformance instrument "
                "for openEHR clinical data repositories."
            </p>
        </section>
    }
}
