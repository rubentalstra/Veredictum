// SPDX-FileCopyrightText: Veredictum contributors
// SPDX-License-Identifier: Apache-2.0

//! S7 — the verdicts surface (#61, #67).
//!
//! The computed profile matrix with the coverage bounds first-class, and the
//! rendered documents verbatim — the same bodies the CLI writes, produced by
//! the same lib function.

use leptos::prelude::{
    Action, AddAnyAttr, ClassAttribute, CollectView, ElementChild, Get, IntoAny, IntoView,
    OnAttribute, PropAttribute, Resource, RwSignal, Set, Suspend, Suspense, Transition, component,
    view,
};
use leptos_meta::Title;
use leptos_router::components::A;

use crate::components::data_table::{TABLE, TABLE_WRAP, TD, TH};
use crate::components::empty_state::EmptyState;
use crate::components::field::{BTN_PRIMARY, BTN_SECONDARY};
use crate::components::format_view::{Pane, inline_error};
use crate::components::page_header::{Crumb, PageHeader};
use crate::components::surface::{CARD_PAD, CARD_TITLE, WELL};
use crate::components::toast::{self, Intent, MessageBar};
use crate::export_api::fns::{fetch_export, prepare_export};
use crate::export_api::{DOWNLOAD_PATH, ExportScreen, ExportSummary};
use crate::pages::run::steps;
use crate::record_api::VerdictsScreen;
use crate::record_api::fns::fetch_verdicts;

/// The evidence chip classes: verdict semantics, coverage bounds visible.
///
/// The tokens are the lib's own serde vocabulary — `pass`/`fail`/`not_claimed`
/// for a profile tier, `passed`/`failed`/`inconclusive`/`not_evidenced`/
/// `not_claimed` for a capability. An unclaimed row is neutral (no claim, no
/// alarm); the unevidenced and inconclusive bounds stay visibly amber.
fn evidence_chip(token: &str) -> &'static str {
    match token {
        "pass" | "passed" => {
            "rounded-control bg-ok-subtle px-1.5 py-0.5 text-xs font-medium text-ink"
        }
        "fail" | "failed" => {
            "rounded-control bg-danger-subtle px-1.5 py-0.5 text-xs font-medium text-ink"
        }
        "not_claimed" => "rounded-control bg-sunken px-1.5 py-0.5 text-xs text-ink-muted",
        _ => "rounded-control bg-warn-subtle px-1.5 py-0.5 text-xs text-ink",
    }
}

/// The verdicts surface.
#[expect(
    clippy::must_use_candidate,
    reason = "a Leptos component is mounted by the framework, never consumed as a value"
)]
#[component]
pub fn Verdicts() -> impl IntoView {
    let screen = Resource::new(|| (), |()| fetch_verdicts());

    view! {
        <Title text="Verdicts · Run · Veredictum console" />
        <PageHeader
            title="Verdicts"
            subtitle="A verdict is a pure function of the statement, the record, the catalogue and the capability matrix — computed, never asserted."
            crumbs=vec![Crumb::new("Run", "/run/connect")]
        >
            <div class="flex items-center gap-1">{steps("verdicts")}</div>
        </PageHeader>
        <Suspense fallback=|| {
            view! { <p class="text-sm text-ink-muted">"Computing the judgement…"</p> }
        }>
            {move || Suspend::new(async move {
                match screen.await {
                    Ok(VerdictsScreen::Judged { profiles, capabilities, documents }) => {
                        judged_view(profiles, capabilities, documents).into_any()
                    }
                    Ok(VerdictsScreen::NoStatement) => {
                        view! {
                            <EmptyState
                                icon=icondata_lu::LuFileQuestion
                                message="The run was driven without a statement"
                                hint="A verdict certifies a claim, and no claim was made: pick a statement at the Scope step and run again."
                            >
                                <A href="/run/scope" attr:class=BTN_SECONDARY>
                                    "Back to Scope"
                                </A>
                            </EmptyState>
                        }
                            .into_any()
                    }
                    Ok(VerdictsScreen::NoRun) => {
                        view! {
                            <EmptyState
                                icon=icondata_lu::LuHourglass
                                message="No finished run yet"
                                hint="Verdicts render here the moment a run completes."
                            >
                                <A href="/run/connect" attr:class=BTN_SECONDARY>
                                    "Grade a server"
                                </A>
                            </EmptyState>
                        }
                            .into_any()
                    }
                    Err(e) => inline_error(&e.to_string()).into_any(),
                }
            })}
        </Suspense>
        // OUTSIDE the Suspense on purpose: this section owns a resource, and a
        // Suspend closure re-creates everything inside it on every
        // notification, diverging the two sides' resource id spaces.
        <Export />
    }
}

/// The matrix and the documents.
fn judged_view(
    profiles: Vec<(String, String)>,
    capabilities: Vec<(String, String)>,
    documents: Vec<crate::record_api::DocumentView>,
) -> impl IntoView + use<> {
    let profile_rows = profiles
        .into_iter()
        .map(|(tier, verdict)| {
            let chip = evidence_chip(&verdict);
            view! {
                <tr>
                    <td class=TD>
                        <span class="font-medium">{tier}</span>
                    </td>
                    <td class=TD>
                        <span class=chip>{verdict}</span>
                    </td>
                </tr>
            }
        })
        .collect_view();
    let capability_rows = capabilities
        .into_iter()
        .map(|(name, evidence)| {
            let chip = evidence_chip(&evidence);
            view! {
                <tr>
                    <td class=TD>
                        <span class="font-mono text-xs">{name}</span>
                    </td>
                    <td class=TD>
                        <span class=chip>{evidence}</span>
                    </td>
                </tr>
            }
        })
        .collect_view();
    let panes = documents
        .into_iter()
        .map(|document| {
            view! {
                <div class="mt-4">
                    <Pane label=document.name body=document.body />
                </div>
            }
        })
        .collect_view();
    view! {
        <div class="grid grid-cols-1 gap-4 lg:grid-cols-2">
            <section class=CARD_PAD>
                <h2 class=CARD_TITLE>"Profile verdicts"</h2>
                <div class=TABLE_WRAP>
                    <table class=TABLE>
                        <thead>
                            <tr>
                                <th class=TH>"Tier"</th>
                                <th class=TH>"Verdict"</th>
                            </tr>
                        </thead>
                        <tbody>{profile_rows}</tbody>
                    </table>
                </div>
            </section>
            <section class=CARD_PAD>
                <h2 class=CARD_TITLE>"Capability evidence"</h2>
                <p class="mb-2 text-sm text-ink-muted">
                    "not_evidenced and inconclusive are printed coverage bounds, never silent."
                </p>
                <div class=format!("{TABLE_WRAP} max-h-96 overflow-y-auto")>
                    <table class=TABLE>
                        <thead>
                            <tr>
                                <th class=TH>"Capability"</th>
                                <th class=TH>"Evidence"</th>
                            </tr>
                        </thead>
                        <tbody>{capability_rows}</tbody>
                    </table>
                </div>
            </section>
        </div>
        <section class="mt-2">
            <h2 class=format!("{CARD_TITLE} mt-4")>"The rendered documents"</h2>
            <p class="text-sm text-ink-muted">
                "Byte-for-byte the bodies the sealed record carries: one pure function renders them, here and in the export."
            </p>
            {panes}
        </section>
    }
}

/// S8 — the export section: one step that seals the record and renders what a
/// party publishes beside it.
///
/// Private, so the crate's `must_use_candidate` relaxation does not apply:
/// that lint reads public items only.
#[component]
fn Export() -> impl IntoView {
    let state = Resource::new(|| (), |()| fetch_export());
    // The inline bar sits BESIDE the toast, never instead of it: a transient
    // success with a silent failure below the fold reads as "nothing
    // happened".
    let note = RwSignal::new(None::<Result<String, String>>);
    let running = RwSignal::new(false);
    // The sanctioned dispatch-continuation shape: the click is the event, the
    // answer lands in the action's own async block. The sealed bundle is never
    // mirrored into a second signal; the refetched resource stays its one
    // reader, so nothing writes a render-visible signal inside a Suspend.
    let prepare = Action::new(move |(): &()| async move {
        running.set(true);
        match prepare_export().await {
            Ok(summary) => {
                let body = format!(
                    "{} sealed as record {}, signed {}.",
                    summary.sut, summary.digest_prefix, summary.signed_at
                );
                toast::success("Export prepared", &body);
                note.set(Some(Ok(body)));
                state.refetch();
            }
            Err(e) => {
                let body = e.to_string();
                toast::error("The export was refused", &body);
                note.set(Some(Err(body)));
            }
        }
        running.set(false);
    });

    view! {
        <section class=format!("{CARD_PAD} mt-4")>
            <h2 class=CARD_TITLE>"Export the signed record"</h2>
            <p class="mb-3 text-sm text-ink-muted">
                "The engine seals the rendered documents with a digest manifest and a detached signature. Beside them the console writes the seal card, the badge and a self-contained report, each carrying the record digest so the artwork names the bytes it certifies."
            </p>
            // A Transition rather than a Suspense: the refetch after a
            // successful seal keeps the section's content in place instead of
            // flashing the fallback (rules §6).
            <Transition fallback=|| {
                view! { <p class="text-sm text-ink-muted">"Reading the export state…"</p> }
            }>
                {move || Suspend::new(async move {
                    match state.await {
                        Ok(screen) => export_view(screen, prepare, running).into_any(),
                        Err(e) => inline_error(&e.to_string()).into_any(),
                    }
                })}
            </Transition>
            {move || {
                note.get()
                    .map(|outcome| {
                        let (intent, message) = match outcome {
                            Ok(body) => (Intent::Success, body),
                            Err(body) => (Intent::Error, body),
                        };
                        view! {
                            <div class="mt-3">
                                <MessageBar intent=intent message=message />
                            </div>
                        }
                    })
            }}
        </section>
    }
}

/// The section's state before, or without, a prepared bundle.
fn export_view(
    screen: ExportScreen,
    prepare: Action<(), ()>,
    running: RwSignal<bool>,
) -> impl IntoView + use<> {
    match screen {
        ExportScreen::Prepared(summary) => prepared_view(&summary).into_any(),
        ExportScreen::Ready => {
            view! {
                <button
                    type="button"
                    class=BTN_PRIMARY
                    prop:disabled=move || running.get()
                    on:click=move |_| {
                        prepare.dispatch(());
                    }
                >
                    {move || if running.get() { "Sealing…" } else { "Prepare the export" }}
                </button>
            }
                .into_any()
        }
        ExportScreen::NoKey { missing } => {
            view! {
                <div class=WELL>
                    <p class="text-sm text-ink">
                        "No signing posture is configured, so nothing here can be sealed."
                    </p>
                    <p class="mt-1 text-sm text-ink-muted">
                        "Mount an armored OpenPGP key pair and name it: "
                        <span class="font-mono text-xs">{missing.join(", ")}</span>
                        ". The secret key seals the bundle; the public key is what the console checks its own seal against, because it never prints a signing time it has not verified."
                    </p>
                </div>
            }
                .into_any()
        }
        ExportScreen::NoStatement => {
            view! {
                <p class="text-sm text-ink-muted">
                    "The run was driven without a statement, so there is no claim to certify. Pick one at the Scope step and run again."
                </p>
            }
                .into_any()
        }
        ExportScreen::NoRun => {
            view! {
                <p class="text-sm text-ink-muted">
                    "An export certifies a finished run; there is none yet."
                </p>
            }
                .into_any()
        }
    }
}

/// The prepared bundle: what was sealed, the download, and the snippets.
fn prepared_view(summary: &ExportSummary) -> impl IntoView + use<> {
    let sealed_rows = summary
        .sealed_files
        .iter()
        .map(|name| {
            view! { <li class="font-mono text-xs text-ink">{name.clone()}</li> }
        })
        .collect_view();
    let presentation_rows = summary
        .presentation_files
        .iter()
        .map(|name| {
            view! { <li class="font-mono text-xs text-ink">{name.clone()}</li> }
        })
        .collect_view();
    view! {
        <div class="mt-4 grid grid-cols-1 gap-4 lg:grid-cols-2">
            <div class=WELL>
                <h3 class="text-xs font-medium uppercase tracking-wide text-ink-muted">
                    "What was sealed"
                </h3>
                <dl class="mt-2 space-y-1 text-sm">
                    <dt class="text-ink-muted">"Record digest"</dt>
                    <dd class="break-all font-mono text-xs text-ink">{summary.digest.clone()}</dd>
                    <dt class="text-ink-muted">"Signer fingerprint"</dt>
                    <dd class="break-all font-mono text-xs text-ink">
                        {summary.fingerprint.clone()}
                    </dd>
                    <dt class="text-ink-muted">"Signing time"</dt>
                    <dd class="font-mono text-xs text-ink">{summary.signed_at.clone()}</dd>
                </dl>
                <h3 class="mt-3 text-xs font-medium uppercase tracking-wide text-ink-muted">
                    "In the manifest"
                </h3>
                <ul class="mt-1 space-y-0.5">{sealed_rows}</ul>
                <h3 class="mt-3 text-xs font-medium uppercase tracking-wide text-ink-muted">
                    "Beside it, outside the manifest"
                </h3>
                <ul class="mt-1 space-y-0.5">{presentation_rows}</ul>
            </div>
            <div class=WELL>
                <h3 class="text-xs font-medium uppercase tracking-wide text-ink-muted">
                    "Publish it"
                </h3>
                // A server-owned axum route, so the anchor is external: after
                // hydration the client router would otherwise intercept it
                // and 404 a route it does not own (rules §4).
                <a href=DOWNLOAD_PATH rel="external" class=format!("{BTN_PRIMARY} mt-2")>
                    "Download the bundle"
                </a>
                <p class="mt-3 text-sm text-ink-muted">
                    "The snippets point at "
                    <span class="font-mono text-xs">"record-badge.svg"</span>
                    " beside wherever you publish the record; change the path to your own hosting."
                </p>
                <div class="mt-2 space-y-2">
                    <Pane label="badge · markdown" body=summary.badge_markdown.clone() />
                    <Pane label="badge · html" body=summary.badge_html.clone() />
                </div>
            </div>
        </div>
    }
}
