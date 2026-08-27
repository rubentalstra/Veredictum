// SPDX-FileCopyrightText: Veredictum contributors
// SPDX-License-Identifier: Apache-2.0

//! S7 — the verdicts surface (#61, #67).
//!
//! The computed profile matrix with the coverage bounds first-class, and the
//! rendered documents verbatim — the same bodies the CLI writes, produced by
//! the same lib function.

use leptos::prelude::{
    AddAnyAttr, ClassAttribute, CollectView, ElementChild, IntoAny, IntoView, Resource, Suspend,
    Suspense, component, view,
};
use leptos_meta::Title;
use leptos_router::components::A;

use crate::components::data_table::{TABLE, TABLE_WRAP, TD, TH};
use crate::components::empty_state::EmptyState;
use crate::components::field::BTN_SECONDARY;
use crate::components::format_view::{Pane, inline_error};
use crate::components::page_header::{Crumb, PageHeader};
use crate::components::surface::{CARD_PAD, CARD_TITLE};
use crate::pages::run::steps;
use crate::record_api::VerdictsScreen;
use crate::record_api::fns::fetch_verdicts;

/// The evidence chip classes: verdict semantics, coverage bounds visible.
fn evidence_chip(token: &str) -> &'static str {
    match token {
        "Passed" | "earned" => {
            "rounded-control bg-ok-subtle px-1.5 py-0.5 text-xs font-medium text-ink"
        }
        "Failed" | "not-earned" => {
            "rounded-control bg-danger-subtle px-1.5 py-0.5 text-xs font-medium text-ink"
        }
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
                    "NotEvidenced and NoCases are printed coverage bounds, never silent."
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
                "Byte-for-byte the bodies the command line writes — the same pure function produced them. The signed export bundle is under construction (#68)."
            </p>
            {panes}
        </section>
    }
}
