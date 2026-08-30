// SPDX-FileCopyrightText: Veredictum contributors
// SPDX-License-Identifier: Apache-2.0

//! The persistent chrome (#61 §Look and feel).
//!
//! The static sidebar (seal, one entry per surface, the versions in the
//! footer) around the routed `<Outlet/>`. The chrome renders exactly once
//! outside any Suspense: a `Suspend` closure re-runs on every notification of
//! the resources it awaits and re-creates everything inside it, so a resource
//! owned there gets a different id on the server than on the client and
//! hydration reads the wrong serialized slot. Dark mode is re-applied after
//! hydration inside an `Effect` for the same reason
//! (<https://book.leptos.dev/ssr/24_hydration_bugs.html>).

use leptos::prelude::{
    AddAnyAttr, AriaAttributes, Children, ClassAttribute, CollectView, Effect, ElementChild, Get,
    IntoView, OnAttribute, RwSignal, Set, Update, component, view,
};
use leptos_icons::Icon;
use leptos_router::components::{A, Outlet};
use leptos_router::hooks::use_location;

use crate::components::toast::{self, ToastHost};
use crate::theme;

/// One sidebar entry.
struct NavEntry {
    key: &'static str,
    label: &'static str,
    icon: icondata_core::Icon,
}

/// The five surfaces (#61, #166): the run pipeline lives under one entry.
fn nav_entries() -> [NavEntry; 5] {
    [
        NavEntry {
            key: "/",
            label: "Instrument",
            icon: icondata_lu::LuGauge,
        },
        NavEntry {
            key: "/catalogue",
            label: "Catalogue",
            icon: icondata_lu::LuLibrary,
        },
        NavEntry {
            key: "/run",
            label: "Run",
            icon: icondata_lu::LuPlay,
        },
        NavEntry {
            key: "/benchmarks",
            label: "Benchmarks",
            icon: icondata_lu::LuActivity,
        },
        NavEntry {
            key: "/verify",
            label: "Verify",
            icon: icondata_lu::LuShieldCheck,
        },
    ]
}

/// Maps a full URL path to the top-level nav key it belongs under, so the
/// sidebar highlights the active surface.
fn nav_key(path: &str) -> &'static str {
    if path.starts_with("/catalogue") {
        "/catalogue"
    } else if path.starts_with("/run") {
        "/run"
    } else if path.starts_with("/benchmarks") {
        "/benchmarks"
    } else if path.starts_with("/verify") {
        "/verify"
    } else {
        "/"
    }
}

/// The application shell: the chrome around the routed `<Outlet/>`.
#[expect(
    clippy::must_use_candidate,
    reason = "a Leptos component is mounted by the framework, never consumed as a value"
)]
#[component]
pub fn Shell() -> impl IntoView {
    view! {
        <Chrome>
            <Outlet />
        </Chrome>
    }
}

/// The chrome itself: sidebar, toast host, and whatever content it frames.
///
/// Taking the content as `children` rather than rendering `<Outlet/>` itself
/// is what lets the routes' 404 fallback wear the same chrome (#84): the
/// fallback renders OUTSIDE the route tree, so it has no outlet to fill.
#[expect(
    clippy::must_use_candidate,
    reason = "a Leptos component is mounted by the framework, never consumed as a value"
)]
#[component]
pub fn Chrome(
    /// The framed content — the routed `<Outlet/>`, or the 404 answer.
    children: Children,
) -> impl IntoView {
    toast::provide();
    let location = use_location();
    let nav_open = RwSignal::new(false);
    let dark = RwSignal::new(false);

    // Browser-only: re-apply the persisted theme after hydration. Reads the
    // outside world (localStorage), so an Effect is the correct home
    // (rules §2); the server pass and the first client paint stay light.
    Effect::new(move |_| {
        if let Some(stored) = theme::stored_dark() {
            dark.set(stored);
            theme::apply_dark(stored);
        }
    });

    let sidebar = move || {
        let active = nav_key(&location.pathname.get());
        nav_entries()
            .into_iter()
            .map(|entry| {
                let class = if entry.key == active {
                    "flex items-center gap-2.5 rounded-control bg-accent-subtle px-3 py-2 text-sm font-medium text-accent-ink"
                } else {
                    "flex items-center gap-2.5 rounded-control px-3 py-2 text-sm font-medium text-ink-muted hover:bg-sunken hover:text-ink"
                };
                view! {
                    <li>
                        <A href=entry.key attr:class=class on:click=move |_| nav_open.set(false)>
                            <Icon icon=entry.icon width="16" height="16" />
                            {entry.label}
                        </A>
                    </li>
                }
            })
            .collect_view()
    };

    view! {
        <div class="flex min-h-screen bg-surface">
            // The scrim behind the mobile drawer; clicking it closes.
            <div
                class="fixed inset-0 z-30 bg-scrim lg:hidden"
                class:hidden=move || !nav_open.get()
                on:click=move |_| nav_open.set(false)
            ></div>
            <aside
                class="fixed inset-y-0 left-0 z-40 flex w-60 -translate-x-full flex-col border-r border-edge bg-raised transition-transform lg:static lg:translate-x-0"
                class:translate-x-0=move || nav_open.get()
            >
                <div class="flex items-center gap-2.5 px-4 py-4">
                    <img src="/seal.svg" alt="" class="h-8 w-8" />
                    <span class="text-base font-semibold tracking-tight text-ink-heading">
                        "Veredictum"
                    </span>
                </div>
                <nav aria-label="Primary" class="flex-1 overflow-y-auto px-3">
                    <ul class="flex flex-col gap-1">{sidebar}</ul>
                </nav>
                <div class="border-t border-edge px-4 py-3 text-xs text-ink-faint">
                    <div class="flex items-center justify-between">
                        <span class="tabular-nums">{format!("engine {}", crate::ENGINE_PIN)}</span>
                        <button
                            class="rounded-control px-2 py-1 text-ink-muted hover:bg-sunken hover:text-ink"
                            aria-label="Toggle dark mode"
                            on:click=move |_| {
                                let next = !dark.get();
                                dark.set(next);
                                theme::apply_dark(next);
                                theme::persist_dark(next);
                            }
                        >
                            {move || if dark.get() { "light" } else { "dark" }}
                        </button>
                    </div>
                </div>
            </aside>
            <div class="flex min-w-0 flex-1 flex-col">
                <header class="flex items-center gap-3 border-b border-edge bg-raised px-4 py-2 lg:hidden">
                    <button
                        class="rounded-control border border-edge-strong px-2 py-1 text-sm text-ink"
                        aria-label="Open navigation"
                        on:click=move |_| nav_open.update(|open| *open = !*open)
                    >
                        "☰"
                    </button>
                    <span class="text-sm font-semibold text-ink-heading">"Veredictum"</span>
                </header>
                <main class="min-w-0 flex-1 px-6 py-6 lg:px-8">{children()}</main>
            </div>
            <ToastHost />
        </div>
    }
}

#[cfg(test)]
mod tests {
    use super::nav_key;

    #[test]
    fn every_path_maps_to_exactly_one_surface() {
        assert_eq!(nav_key("/"), "/");
        assert_eq!(nav_key("/catalogue"), "/catalogue");
        assert_eq!(
            nav_key("/catalogue/ehr/I_EHR_SERVICE.create_ehr-main"),
            "/catalogue"
        );
        assert_eq!(nav_key("/run/connect"), "/run");
        assert_eq!(nav_key("/run/results"), "/run");
        assert_eq!(nav_key("/verify"), "/verify");
        assert_eq!(nav_key("/benchmarks"), "/benchmarks");
        assert_eq!(nav_key("/benchmarks?record=abc"), "/benchmarks");
        assert_eq!(nav_key("/nonsense"), "/");
    }
}
