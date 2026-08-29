// SPDX-FileCopyrightText: Veredictum contributors
// SPDX-License-Identifier: Apache-2.0

//! The application shell and route tree.
//!
//! The hydration contract governs this module: identical view structure on
//! both targets, valid HTML, every routed page setting a `<Title>`, and the
//! chrome rendering exactly once around the `<Outlet/>`.

use leptos::prelude::{
    AutoReload, Effect, ElementChild, GlobalAttributes, HydrationScripts, IntoView, LeptosOptions,
    component, document, view,
};
use leptos_meta::{Link, Meta, MetaTags, Stylesheet, Title, provide_meta_context};
use leptos_router::{
    ParamSegment, StaticSegment,
    components::{ParentRoute, Route, Router, Routes},
};

use crate::pages::benchmarks::Benchmarks;
use crate::pages::catalogue::{Case, Catalogue, Chapter};
use crate::pages::instrument::Instrument;
use crate::pages::not_found::NotFound;
use crate::pages::results::Results;
use crate::pages::run::{Connect, Live, Run, Scope};
use crate::pages::shell::Shell;
use crate::pages::verdicts::Verdicts;
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

    // The hydration marker every browser journey waits on before driving a
    // control: a click that lands before hydration attaches its listener is
    // silently lost. An Effect never runs on the server, so the server pass
    // and the first paint stay deterministic, and the marker sits on the root
    // component so the routes' 404 fallback carries it too.
    Effect::new(|_| {
        if let Some(body) = document().body() {
            let outcome = body.set_attribute("data-hydrated", "");
            // NOTE: no openEHR spec governs this — our own design; a failed
            // attribute write on a live document has nothing to recover from.
            drop(outcome);
        }
    });

    view! {
        <Stylesheet id="leptos" href="/pkg/veredictum-console.css" />
        // The icon set (#84), every file rendered from one of the two brand
        // originals by `scripts/render/brand-icons.sh`. `favicon.ico` exists
        // whether or not it is declared: a browser probes that path on every
        // load, and the 404 it logs is a page error the journeys' console gate
        // fails on.
        <Link rel="icon" type_="image/svg+xml" href="/seal.svg" />
        <Link rel="icon" sizes="32x32" href="/favicon.ico" />
        <Link rel="apple-touch-icon" sizes="180x180" href="/apple-touch-icon.png" />
        <Link rel="manifest" href="/site.webmanifest" />
        <Meta name="theme-color" content="#1B6E92" />
        <Title text="Veredictum console" />
        <Router>
            <Routes fallback=|| view! { <NotFound /> }>
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
                    <Route path=(StaticSegment("run"), StaticSegment("connect")) view=Connect />
                    <Route path=(StaticSegment("run"), StaticSegment("scope")) view=Scope />
                    <Route path=(StaticSegment("run"), StaticSegment("live")) view=Live />
                    <Route path=(StaticSegment("run"), StaticSegment("results")) view=Results />
                    <Route path=(StaticSegment("run"), StaticSegment("verdicts")) view=Verdicts />
                    <Route path=StaticSegment("benchmarks") view=Benchmarks />
                    <Route path=StaticSegment("verify") view=Verify />
                </ParentRoute>
            </Routes>
        </Router>
    }
}
