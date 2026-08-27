// SPDX-FileCopyrightText: Veredictum contributors
// SPDX-License-Identifier: Apache-2.0

//! S6 — the results surface (#61, #67).
//!
//! The finished run's record, red rows first, with the detail joined to the
//! catalogue. The selected row lives in the URL (`?case=`, `?format=`), so a
//! finding is shareable.

use leptos::prelude::{
    AddAnyAttr, ClassAttribute, CollectView, ElementChild, Get, IntoAny, IntoView, Memo, Resource,
    Suspend, Suspense, With, component, view,
};
use leptos_meta::Title;
use leptos_router::components::A;
use leptos_router::hooks::use_query_map;

use crate::components::data_table::{TABLE, TABLE_WRAP, TD, TH};
use crate::components::empty_state::EmptyState;
use crate::components::field::BTN_SECONDARY;
use crate::components::format_view::{Pane, inline_error};
use crate::components::page_header::{Crumb, PageHeader};
use crate::components::stat_card::StatCard;
use crate::components::surface::{CARD_PAD, CARD_TITLE, WELL};
use crate::pages::run::steps;
use crate::record_api::fns::{fetch_result_detail, fetch_results};
use crate::record_api::{ExchangeView, ResultDetail, ResultsScreen, TranscriptView};

/// The `?case=` selection from the URL.
fn query_case() -> Memo<String> {
    let query = use_query_map();
    Memo::new(move |_| query.with(|q| q.get("case").unwrap_or_default()))
}

/// The `?format=` selection from the URL.
fn query_format() -> Memo<Option<String>> {
    let query = use_query_map();
    Memo::new(move |_| query.with(|q| q.get("format").filter(|f| !f.is_empty())))
}

/// The status chip classes, verdict semantics only.
fn status_chip(status: &str) -> &'static str {
    match status {
        "failed" => "rounded-control bg-danger-subtle px-1.5 py-0.5 text-xs font-medium text-ink",
        "errored" => "rounded-control bg-warn-subtle px-1.5 py-0.5 text-xs font-medium text-ink",
        "passed" => "rounded-control bg-ok-subtle px-1.5 py-0.5 text-xs font-medium text-ink",
        _ => "rounded-control bg-sunken px-1.5 py-0.5 text-xs text-ink-muted",
    }
}

/// The results surface.
#[expect(
    clippy::must_use_candidate,
    reason = "a Leptos component is mounted by the framework, never consumed as a value"
)]
#[component]
pub fn Results() -> impl IntoView {
    let case = query_case();
    let format = query_format();
    let screen = Resource::new(|| (), |()| fetch_results());
    let detail = Resource::new(
        move || (case.get(), format.get()),
        |(case, format)| async move {
            if case.is_empty() {
                Ok(None)
            } else {
                fetch_result_detail(case, format).await
            }
        },
    );

    view! {
        <Title text="Results · Run · Veredictum console" />
        <PageHeader
            title="Results"
            subtitle="The record, red rows first — a red row names a defect in exactly one of three suspects."
            crumbs=vec![Crumb::new("Run", "/run/connect")]
        >
            <div class="flex items-center gap-1">{steps("results")}</div>
        </PageHeader>
        <Suspense fallback=|| {
            view! { <p class="text-sm text-ink-muted">"Reading the record…"</p> }
        }>
            {move || Suspend::new(async move {
                match screen.await {
                    Ok(Some(results)) => results_view(&results).into_any(),
                    Ok(None) => {
                        view! {
                            <EmptyState
                                icon=icondata_lu::LuHourglass
                                message="No finished run yet"
                                hint="Results render here the moment a run completes."
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
        <Suspense fallback=|| ()>
            {move || Suspend::new(async move {
                match detail.await {
                    Ok(Some(found)) => detail_view(&found).into_any(),
                    Ok(None) => ().into_any(),
                    Err(e) => inline_error(&e.to_string()).into_any(),
                }
            })}
        </Suspense>
    }
}

/// The tallies and the red-first table.
fn results_view(results: &ResultsScreen) -> impl IntoView + use<> {
    let (passed, failed, errored, excused) = results.tallies;
    let rows = results
        .rows
        .iter()
        .cloned()
        .map(|row| {
            let href = match &row.format {
                Some(format) => format!("/run/results?case={}&format={format}", row.case),
                None => format!("/run/results?case={}", row.case),
            };
            let chip = status_chip(&row.status);
            view! {
                <tr class="hover:bg-sunken">
                    <td class=TD>
                        <A href=href attr:class="font-mono text-xs text-accent hover:underline">
                            {row.case}
                        </A>
                    </td>
                    <td class=TD>
                        <span class=chip>{row.status}</span>
                    </td>
                    <td class=TD>{row.format.unwrap_or_default()}</td>
                    <td class=TD>
                        <span class="tabular-nums">{row.rows}</span>
                    </td>
                    <td class=TD>
                        <span class="line-clamp-2 font-mono text-xs">
                            {row.reason.unwrap_or_default()}
                        </span>
                    </td>
                </tr>
            }
        })
        .collect_view();
    view! {
        <div class="mb-4 grid grid-cols-2 gap-4 xl:grid-cols-4">
            <StatCard label="Passed" value=passed.to_string() icon=icondata_lu::LuCheck />
            <StatCard label="Failed" value=failed.to_string() icon=icondata_lu::LuX />
            <StatCard
                label="Errored (inconclusive)"
                value=errored.to_string()
                icon=icondata_lu::LuCircleHelp
            />
            <StatCard
                label="Excused with citation"
                value=excused.to_string()
                icon=icondata_lu::LuFileText
            />
        </div>
        <p class="mb-2 text-sm text-ink-muted">{format!("System under test: {}", results.sut)}</p>
        <div class=TABLE_WRAP>
            <table class=TABLE>
                <thead>
                    <tr>
                        <th class=TH>"Case"</th>
                        <th class=TH>"Status"</th>
                        <th class=TH>"Format"</th>
                        <th class=TH>"Rows"</th>
                        <th class=TH>"Reason"</th>
                    </tr>
                </thead>
                <tbody>{rows}</tbody>
            </table>
        </div>
    }
}

/// One exchange as its request and response panes.
fn exchange_view(exchange: &ExchangeView) -> impl IntoView + use<> {
    let seq = exchange.seq;
    let request = format!("{}\n{}", exchange.request_line, exchange.request_headers);
    let response = format!("{}\n{}", exchange.status_line, exchange.response_headers);
    let request_body = exchange
        .request_body
        .clone()
        .map(|body| view! { <Pane label="request body" body=body /> });
    let response_body = exchange
        .response_body
        .clone()
        .map(|body| view! { <Pane label="response body" body=body /> });
    view! {
        <div class="space-y-2 border-t border-line pt-3">
            <p class="text-xs font-medium uppercase tracking-wide text-ink-muted">
                {format!("exchange {seq}")}
            </p>
            <Pane label="request" body=request />
            {request_body}
            <Pane label="response" body=response />
            {response_body}
        </div>
    }
}

/// The wire section: the recorded exchanges, or the honest line saying why
/// there are none.
fn transcript_view(transcript: &TranscriptView) -> impl IntoView + use<> {
    match transcript {
        TranscriptView::NotRecorded => {
            view! {
                <p class="text-sm text-ink-muted">
                    "This run was driven without --record-exchanges, so the record carries no wire transcript. Re-run with the box ticked on Scope to keep the exchanges."
                </p>
            }
                .into_any()
        }
        TranscriptView::Unreadable(diagnostic) => inline_error(diagnostic).into_any(),
        TranscriptView::Recorded(exchanges) if exchanges.is_empty() => {
            view! {
                <p class="text-sm text-ink-muted">
                    "The run recorded its wire, and this case drove no exchange: it was excused or refused before any request went out."
                </p>
            }
                .into_any()
        }
        TranscriptView::Recorded(exchanges) => {
            let panes = exchanges.iter().map(exchange_view).collect_view();
            view! { <div class="space-y-3">{panes}</div> }.into_any()
        }
    }
}

/// The selected outcome's detail: the record's evidence beside the case's
/// citations, and the attribution law stated where a red row is read.
fn detail_view(detail: &ResultDetail) -> impl IntoView + use<> {
    let refs = detail
        .spec_refs
        .iter()
        .cloned()
        .map(|citation| view! { <li class="font-mono text-xs text-ink">{citation}</li> })
        .collect_view();
    let failed_rows = (!detail.failed_rows.is_empty()).then(|| {
        let rows = detail
            .failed_rows
            .iter()
            .map(|failed| {
                view! {
                    <li class="font-mono text-xs text-ink">
                        {format!("{} — {}", failed.row, failed.evidence)}
                    </li>
                }
            })
            .collect_view();
        view! {
            <h3 class=CARD_TITLE>"Failing rows"</h3>
            <ul class=format!("{WELL} space-y-1")>{rows}</ul>
        }
    });
    let reason = detail.row.reason.clone().map(|reason| {
        view! { <Pane label="recorded reason" body=reason /> }
    });
    let wire = transcript_view(&detail.transcript);
    let citation = detail.citation.clone().map(|citation| {
        view! {
            <p class="text-sm text-ink">
                "Excusing citation: " <span class="font-mono text-xs">{citation}</span>
            </p>
        }
    });
    view! {
        <section class=format!("{CARD_PAD} mt-4")>
            <h2 class=CARD_TITLE>
                {format!(
                    "{}{}",
                    detail.row.case,
                    detail
                        .row
                        .format
                        .as_ref()
                        .map(|format| format!(" · {format}"))
                        .unwrap_or_default(),
                )}
            </h2>
            {detail
                .test_purpose
                .clone()
                .map(|purpose| {
                    view! { <p class="mb-3 text-sm text-ink-muted">{purpose}</p> }
                })}
            <div class="space-y-3">
                {reason} {citation}
                {detail
                    .failing_step
                    .map(|step| {
                        view! {
                            <p class="text-sm text-ink-muted">
                                {format!("first failing step: {step}")}
                            </p>
                        }
                    })} {failed_rows} <h3 class=CARD_TITLE>"Spec citations"</h3>
                <ul class=format!("{WELL} space-y-1")>{refs}</ul>
                <h3 class=CARD_TITLE>"The wire"</h3> {wire}
                <p class="text-sm text-ink-muted">
                    "A red row names a defect in exactly one of three suspects — the server, the runner, or the catalogue — and the cited text is the reference, never any side's confidence."
                </p>
            </div>
        </section>
    }
}
