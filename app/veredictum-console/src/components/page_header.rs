// SPDX-FileCopyrightText: Veredictum contributors
// SPDX-License-Identifier: Apache-2.0

//! The shared page header: breadcrumbs, title, subtitle, and an action
//! slot — every routed screen opens with one so the page rhythm (header →
//! content) is identical across the console.

use leptos::prelude::{
    AddAnyAttr, AriaAttributes, Children, ClassAttribute, CollectView, ElementChild, Get, IntoAny,
    IntoView, Signal, component, view,
};
use leptos_router::components::A;

/// One breadcrumb ancestor (the current page renders as plain text after
/// the linked trail).
#[derive(Debug, Clone)]
pub struct Crumb {
    /// The link label.
    pub label: String,
    /// The route the crumb navigates to.
    pub href: String,
}

impl Crumb {
    /// Builds a crumb from anything string-like.
    pub fn new(label: impl Into<String>, href: impl Into<String>) -> Self {
        Self {
            label: label.into(),
            href: href.into(),
        }
    }
}

/// The standard page header.
///
/// `crumbs` are the ANCESTORS only — the component renders `title` as the
/// terminal crumb itself. `children` (optional) land right-aligned on the
/// title row: the screen's primary actions.
#[expect(
    clippy::must_use_candidate,
    reason = "a Leptos component is mounted by the framework, never consumed as a value"
)]
#[component]
pub fn PageHeader(
    /// The page title (also the terminal breadcrumb).
    #[prop(into)]
    title: Signal<String>,
    /// One-line description under the title.
    #[prop(optional, into)]
    subtitle: Option<String>,
    /// Ancestor breadcrumbs, root first.
    #[prop(optional)]
    crumbs: Vec<Crumb>,
    /// Right-aligned primary actions.
    #[prop(optional)]
    children: Option<Children>,
) -> impl IntoView {
    let trail = (!crumbs.is_empty()).then(|| {
        let links = crumbs
            .into_iter()
            .map(|crumb| {
                view! {
                    <li class="flex items-center gap-1.5">
                        <A
                            href=crumb.href
                            attr:class="text-ink-muted hover:text-accent hover:underline"
                        >
                            {crumb.label}
                        </A>
                        <span aria-hidden="true" class="text-ink-faint">
                            "/"
                        </span>
                    </li>
                }
            })
            .collect_view();
        view! {
            <nav aria-label="Breadcrumb" class="mb-1">
                <ol class="flex flex-wrap items-center gap-1.5 text-sm">
                    {links} <li aria-current="page" class="text-ink-muted">
                        {move || title.get()}
                    </li>
                </ol>
            </nav>
        }
        .into_any()
    });
    view! {
        <header class="mb-6">
            {trail} <div class="flex flex-wrap items-center justify-between gap-3">
                <h1 class="text-xl font-semibold text-ink-heading">{move || title.get()}</h1>
                <div class="flex items-center gap-2">{children.map(|c| c())}</div>
            </div>
            {subtitle
                .map(|s| {
                    view! { <p class="mt-1 text-sm text-ink-muted">{s}</p> }
                })}
        </header>
    }
}
