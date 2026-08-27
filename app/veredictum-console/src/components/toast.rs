// SPDX-FileCopyrightText: Veredictum contributors
// SPDX-License-Identifier: Apache-2.0

//! Toast feedback: every mutation reports its outcome as a transient toast.
//!
//! Success AND failure both toast, with the
//! inline `MessageBar` beside a failure where a diagnostic is worth reading
//! line by line.
//!
//! Own machinery, deliberately small: a context queue plus one fixed host
//! region the shell mounts. A widget-kit toaster can replace the internals
//! later without touching a single call site, which is the point of the kit.

use leptos::prelude::{
    AriaAttributes, ClassAttribute, CollectView, ElementChild, Get, GlobalAttributes, IntoView,
    OnAttribute, RwSignal, Set, Update, component, expect_context, provide_context, set_timeout,
    view,
};

/// How long a toast stays before it dismisses itself.
const TOAST_MS: u64 = 6_000;

/// The outcome flavor a toast reports.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Intent {
    /// A mutation succeeded.
    Success,
    /// A mutation failed; the body carries the actionable copy.
    Error,
}

/// One queued toast.
#[derive(Debug, Clone)]
struct Item {
    id: u64,
    intent: Intent,
    title: String,
    body: String,
}

/// The toast queue, provided by the shell and consumed by [`push`].
#[derive(Debug, Clone, Copy)]
pub struct Toasts {
    items: RwSignal<Vec<Item>>,
    next_id: RwSignal<u64>,
}

/// Provides the toast queue to the subtree. The shell calls this once, in
/// setup, before mounting [`ToastHost`].
pub fn provide() {
    provide_context(Toasts {
        items: RwSignal::new(Vec::new()),
        next_id: RwSignal::new(0),
    });
}

/// Dispatches a toast; browser-only callers (event handlers, action
/// continuations), because the dismissal timer is a browser timer.
pub fn push(intent: Intent, title: &str, body: &str) {
    let toasts: Toasts = expect_context();
    let id = toasts.next_id.get();
    toasts.next_id.set(id.wrapping_add(1));
    toasts.items.update(|items| {
        items.push(Item {
            id,
            intent,
            title: title.to_owned(),
            body: body.to_owned(),
        });
    });
    set_timeout(
        move || {
            toasts
                .items
                .update(|items| items.retain(|item| item.id != id));
        },
        std::time::Duration::from_millis(TOAST_MS),
    );
}

/// Dispatches a success toast.
pub fn success(title: &str, body: &str) {
    push(Intent::Success, title, body);
}

/// Dispatches an error toast; the body is the actionable copy — name the
/// object, name what went wrong verbatim, name the next action.
pub fn error(title: &str, body: &str) {
    push(Intent::Error, title, body);
}

/// The fixed host region rendering the queue; the shell mounts exactly one.
#[expect(
    clippy::must_use_candidate,
    reason = "a Leptos component is mounted by the framework, never consumed as a value"
)]
#[component]
pub fn ToastHost() -> impl IntoView {
    let toasts: Toasts = expect_context();
    view! {
        <div class="pointer-events-none fixed right-4 top-4 z-50 flex w-80 flex-col gap-2">
            {move || {
                toasts
                    .items
                    .get()
                    .into_iter()
                    .map(|item| {
                        let accent = match item.intent {
                            Intent::Success => "border-l-4 border-l-ok",
                            Intent::Error => "border-l-4 border-l-danger",
                        };
                        let id = item.id;
                        view! {
                            <div
                                role="status"
                                class=format!(
                                    "pointer-events-auto rounded-card border border-edge bg-raised p-3 shadow-card {accent}",
                                )
                            >
                                <div class="flex items-start justify-between gap-2">
                                    <p class="text-sm font-semibold text-ink-heading">{item.title}</p>
                                    <button
                                        class="text-ink-faint hover:text-ink"
                                        aria-label="Dismiss"
                                        on:click=move |_| {
                                            toasts
                                                .items
                                                .update(|items| items.retain(|i| i.id != id));
                                        }
                                    >
                                        "×"
                                    </button>
                                </div>
                                <p class="mt-0.5 text-sm text-ink-muted">{item.body}</p>
                            </div>
                        }
                    })
                    .collect_view()
            }}
        </div>
    }
}

/// The inline message bar that sits BESIDE a failure toast where the
/// diagnostic is worth reading line by line — never inline-only for
/// mutations, never a toast for pure reads.
#[expect(
    clippy::must_use_candidate,
    reason = "a Leptos component is mounted by the framework, never consumed as a value"
)]
#[component]
pub fn MessageBar(
    /// The outcome flavor the bar reports.
    intent: Intent,
    /// The bar's content, verbatim where it quotes a diagnostic.
    #[prop(into)]
    message: String,
) -> impl IntoView {
    let class = match intent {
        Intent::Success => {
            "rounded-control border border-ok/40 bg-ok-subtle px-3 py-2 text-sm text-ink"
        }
        Intent::Error => {
            "rounded-control border border-danger/40 bg-danger-subtle px-3 py-2 text-sm text-ink"
        }
    };
    view! { <div role="alert" class=class>{message}</div> }
}
