// SPDX-FileCopyrightText: Veredictum contributors
// SPDX-License-Identifier: Apache-2.0

//! The application shell and route tree.
//!
//! Every rule in `.claude/rules/leptos-ui.md` governs this module: identical
//! view structure on both targets, valid HTML, every routed page sets a
//! `<Title>`, and the chrome renders exactly once around the `<Outlet/>`.

use leptos::prelude::{
    AutoReload, ElementChild, GlobalAttributes, HydrationScripts, IntoView, LeptosOptions,
    component, view,
};
use leptos_meta::{MetaTags, Stylesheet, Title, provide_meta_context};
use leptos_router::{
    ParamSegment, StaticSegment,
    components::{ParentRoute, Route, Router, Routes},
};

use crate::pages::catalogue::{Case, Catalogue, Chapter};
use crate::pages::instrument::Instrument;
use crate::pages::run::{Connect, Run, Scope};
use crate::pages::shell::Shell;
use crate::pages::verify::Verify;

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

/// The root component: meta context, the stylesheet, and the route tree —
/// every surface nested under the one [`Shell`].
#[expect(
    clippy::must_use_candidate,
    reason = "a Leptos component is mounted by the framework, never consumed as a value"
)]
#[component]
pub fn App() -> impl IntoView {
    provide_meta_context();

    view! {
        <Stylesheet id="leptos" href="/pkg/veredictum-console.css" />
        <Title text="Veredictum console" />
        <Router>
            <Routes fallback=|| "Page not found.">
                <ParentRoute path=StaticSegment("") view=Shell>
                    <Route path=StaticSegment("") view=Instrument />
                    <Route path=StaticSegment("catalogue") view=Catalogue />
                    <Route
                        path=(StaticSegment("catalogue"), ParamSegment("chapter"))
                        view=Chapter
                    />
                    <Route
                        path=(
                            StaticSegment("catalogue"),
                            ParamSegment("chapter"),
                            ParamSegment("case"),
                        )
                        view=Case
                    />
                    <Route path=StaticSegment("run") view=Run />
                    <Route
                        path=(StaticSegment("run"), StaticSegment("connect"))
                        view=Connect
                    />
                    <Route path=(StaticSegment("run"), StaticSegment("scope")) view=Scope />
                    <Route path=StaticSegment("verify") view=Verify />
                </ParentRoute>
            </Routes>
        </Router>
    }
}
