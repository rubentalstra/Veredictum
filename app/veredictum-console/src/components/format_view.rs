// SPDX-FileCopyrightText: Veredictum contributors
// SPDX-License-Identifier: Apache-2.0

//! The verbatim pane: the ONE way the console shows a raw body.
//!
//! Payloads, diagnostics and documents render monospace, scrollable, never
//! rewritten, with a copy affordance. Syntax highlighting arrives with the
//! exchange rendering (#67); this kit's contract is verbatimness.

use leptos::prelude::{
    ClassAttribute, ElementChild, Get, GlobalAttributes, IntoView, OnAttribute, component, view,
};
use leptos_use::{UseClipboardReturn, use_clipboard};

use crate::components::surface::WELL;

/// A labeled verbatim body with a copy button.
#[expect(
    clippy::must_use_candidate,
    reason = "a Leptos component is mounted by the framework, never consumed as a value"
)]
#[component]
pub fn Pane(
    /// What the body is ("probe answer", "results.json", …).
    #[prop(into)]
    label: String,
    /// The body, shown byte-for-byte.
    #[prop(into)]
    body: String,
) -> impl IntoView {
    let UseClipboardReturn { copy, copied, .. } = use_clipboard();
    let copy_source = body.clone();
    view! {
        <section>
            <div class="mb-1 flex items-center justify-between">
                <span class="text-xs font-medium uppercase tracking-wide text-ink-muted">
                    {label}
                </span>
                <button
                    class="text-xs text-accent hover:text-accent-hover hover:underline"
                    on:click=move |_| copy(&copy_source)
                >
                    {move || if copied.get() { "copied" } else { "copy" }}
                </button>
            </div>
            <pre class=format!(
                "{WELL} max-h-96 overflow-auto whitespace-pre-wrap break-all font-mono text-xs text-ink",
            )>{body}</pre>
        </section>
    }
}

/// The inline read-error shape: a screen says so where the data would be —
/// pure reads never toast.
#[must_use]
pub fn inline_error(diagnostic: &str) -> impl IntoView + use<> {
    view! {
        <div
            role="alert"
            class="rounded-control border border-danger/40 bg-danger-subtle px-3 py-2 font-mono text-xs text-ink"
        >
            {diagnostic.to_owned()}
        </div>
    }
}
