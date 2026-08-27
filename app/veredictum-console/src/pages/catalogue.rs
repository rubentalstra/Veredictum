// SPDX-FileCopyrightText: Veredictum contributors
// SPDX-License-Identifier: Apache-2.0

//! S2 — the catalogue explorer (#61, #64).
//!
//! Chapters → cases → one case in full, read through the published lib's
//! typed model. Filter, search and page state live in the URL
//! (`.claude/rules/leptos-ui.md` §9).

use leptos::prelude::{
    AddAnyAttr, ClassAttribute, CollectView, ElementChild, Get, IntoAny, IntoView, Memo, Resource,
    Suspend, Suspense, Transition, With, component, view,
};
use leptos_meta::Title;
use leptos_router::components::{A, Form};
use leptos_router::hooks::{use_params_map, use_query_map};

use crate::catalogue_api::CaseDetail;
use crate::catalogue_api::fns::{fetch_case_detail, fetch_chapter_cases, fetch_chapters};
use crate::components::data_table::{
    TABLE, TABLE_WRAP, TD, TH, TableFooter, page_from_url, page_window,
};
use crate::components::empty_state::EmptyState;
use crate::components::field::INPUT;
use crate::components::format_view::inline_error;
use crate::components::page_header::{Crumb, PageHeader};
use crate::components::surface::{CARD_PAD, CARD_TITLE, WELL};

/// A router param by name, empty when absent — params are user input
/// (rules §9), and the URL reads live in helper fns (the `must_use` trap,
/// rules §2).
fn param(name: &'static str) -> Memo<String> {
    let params = use_params_map();
    Memo::new(move |_| params.with(|p| p.get(name).unwrap_or_default()))
}

/// The `?q=` search filter from the URL.
fn query_q() -> Memo<String> {
    let query = use_query_map();
    Memo::new(move |_| query.with(|q| q.get("q").unwrap_or_default()))
}

/// The chapter list.
#[expect(
    clippy::must_use_candidate,
    reason = "a Leptos component is mounted by the framework, never consumed as a value"
)]
#[component]
pub fn Catalogue() -> impl IntoView {
    let rows = Resource::new(|| (), |()| fetch_chapters());

    view! {
        <Title text="Catalogue · Veredictum console" />
        <PageHeader
            title="Catalogue"
            subtitle="Every chapter, every case, and the citations each expectation stands on."
        />
        <Suspense fallback=|| {
            view! { <p class="text-sm text-ink-muted">"Reading the catalogue…"</p> }
        }>
            {move || Suspend::new(async move {
                match rows.await {
                    Ok(chapters) => {
                        view! {
                            <div class="grid grid-cols-1 gap-3 sm:grid-cols-2 xl:grid-cols-3">
                                {chapters
                                    .into_iter()
                                    .map(|chapter| {
                                        let href = format!("/catalogue/{}", chapter.key);
                                        view! {
                                            <A
                                                href=href
                                                attr:class="flex items-center justify-between rounded-card border border-edge bg-raised p-4 shadow-card transition-colors hover:border-accent"
                                            >
                                                <span class="font-mono text-sm text-ink">
                                                    {chapter.key}
                                                </span>
                                                <span class="tabular-nums text-sm text-ink-muted">
                                                    {format!("{} cases", chapter.cases)}
                                                </span>
                                            </A>
                                        }
                                    })
                                    .collect_view()}
                            </div>
                        }
                            .into_any()
                    }
                    Err(e) => inline_error(&e.to_string()).into_any(),
                }
            })}
        </Suspense>
    }
}

/// One chapter's case listing: URL-state search and paging over the typed
/// rows.
#[expect(
    clippy::must_use_candidate,
    reason = "a Leptos component is mounted by the framework, never consumed as a value"
)]
#[component]
pub fn Chapter() -> impl IntoView {
    let chapter = param("chapter");
    let q = query_q();
    let page = page_from_url();
    // Reactive inputs in the SOURCE; the fetcher is untracked by design
    // (rules §6).
    let rows = Resource::new(
        move || (chapter.get(), q.get()),
        |(chapter, q)| fetch_chapter_cases(chapter, q),
    );

    view! {
        <Title text=move || format!("{} · Catalogue · Veredictum console", chapter.get()) />
        <PageHeader
            title=chapter
            subtitle="One small isolated case per behaviour, so a red row names one defect."
            crumbs=vec![Crumb::new("Catalogue", "/catalogue")]
        />
        <Form method="GET" action="">
            <div class="mb-4 flex items-center gap-2">
                <input
                    type="search"
                    name="q"
                    value=move || q.get()
                    placeholder="Filter by case id…"
                    class=INPUT
                />
                <button type="submit" class="text-sm text-accent hover:underline">
                    "filter"
                </button>
            </div>
        </Form>
        // Transition keeps the old rows visible while a filter reloads
        // (rules §6) — no fallback flash on every keystroke-submit.
        <Transition fallback=|| {
            view! { <p class="text-sm text-ink-muted">"Reading the chapter…"</p> }
        }>
            {move || Suspend::new(async move {
                let current_page = page.get();
                let chapter_key = chapter.get();
                match rows.await {
                    Ok(all) => {
                        let total = all.len();
                        if total == 0 {
                            return view! {
                                <EmptyState
                                    icon=icondata_lu::LuSearchX
                                    message="No case matches"
                                    hint="Loosen the filter, or check the chapter key in the URL."
                                />
                            }
                                .into_any();
                        }
                        let (start, end) = page_window(current_page, total);
                        let window = all.get(start..end).unwrap_or_default().to_vec();
                        let body = window
                            .into_iter()
                            .map(|row| {
                                let href = format!(
                                    "/catalogue/{chapter_key}/{}",
                                    row.id,
                                );
                                view! {
                                    <tr class="hover:bg-sunken">
                                        <td class=TD>
                                            <A
                                                href=href
                                                attr:class="font-mono text-xs text-accent hover:underline"
                                            >
                                                {row.id}
                                            </A>
                                        </td>
                                        <td class=TD>{row.kind}</td>
                                        <td class=TD>
                                            <span class="line-clamp-2 text-sm">{row.purpose}</span>
                                        </td>
                                    </tr>
                                }
                            })
                            .collect_view();
                        view! {
                            <div class=TABLE_WRAP>
                                <table class=TABLE>
                                    <thead>
                                        <tr>
                                            <th class=TH>"Case id"</th>
                                            <th class=TH>"Kind"</th>
                                            <th class=TH>"Test purpose"</th>
                                        </tr>
                                    </thead>
                                    <tbody>{body}</tbody>
                                </table>
                                <TableFooter
                                    base=format!("/catalogue/{chapter_key}")
                                    page=current_page
                                    total=total
                                />
                            </div>
                        }
                            .into_any()
                    }
                    Err(e) => inline_error(&e.to_string()).into_any(),
                }
            })}
        </Transition>
    }
}

/// One case in full: purpose, citations, bindings, corpus references.
///
/// No `must_use_candidate` expect here: the URL-reading helpers keep that
/// lint from firing on this fn (the rules §2 toolchain trap, inverted).
#[component]
pub fn Case() -> impl IntoView {
    let chapter = param("chapter");
    let id = param("case");
    let detail = Resource::new(move || id.get(), fetch_case_detail);

    view! {
        <Title text=move || format!("{} · Catalogue · Veredictum console", id.get()) />
        <Suspense fallback=|| {
            view! { <p class="text-sm text-ink-muted">"Reading the case…"</p> }
        }>
            {move || Suspend::new(async move {
                let chapter_key = chapter.get();
                match detail.await {
                    Ok(Some(case)) => case_view(&chapter_key, case).into_any(),
                    Ok(None) => {
                        view! {
                            <EmptyState
                                icon=icondata_lu::LuFileX
                                message="No case carries this id"
                                hint="The catalogue never reuses an id, even after retirement — check the URL."
                            />
                        }
                            .into_any()
                    }
                    Err(e) => inline_error(&e.to_string()).into_any(),
                }
            })}
        </Suspense>
    }
}

/// The loaded case's sections — plain assembly, erased per section
/// (rules §1).
fn case_view(chapter_key: &str, case: CaseDetail) -> impl IntoView + use<> {
    let refs = case
        .spec_refs
        .into_iter()
        .map(|citation| {
            view! { <li class="font-mono text-xs text-ink">{citation}</li> }
        })
        .collect_view();
    let bindings = if case.bindings.is_empty() {
        view! {
            <p class="text-sm text-ink-muted">
                "No operation binding: a content case reaches the wire through its committing flow."
            </p>
        }
        .into_any()
    } else {
        case.bindings
            .into_iter()
            .map(|binding| {
                let badge = if binding.realized {
                    view! { <span class="rounded-control bg-ok-subtle px-1.5 py-0.5 text-xs text-ink">"realized"</span> }
                        .into_any()
                } else {
                    view! { <span class="rounded-control bg-warn-subtle px-1.5 py-0.5 text-xs text-ink">"unrealized"</span> }
                        .into_any()
                };
                view! {
                    <li class="flex items-center gap-2">
                        <span class="font-mono text-xs text-ink">{binding.file}</span>
                        {badge}
                    </li>
                }
            })
            .collect_view()
            .into_any()
    };
    let corpus = if case.corpus_keys.is_empty() {
        view! { <p class="text-sm text-ink-muted">"No corpus references."</p> }.into_any()
    } else {
        case.corpus_keys
            .into_iter()
            .map(|key| view! { <li class="font-mono text-xs text-ink">{key}</li> })
            .collect_view()
            .into_any()
    };
    let anchor = case.sm_operation.map(|op| {
        view! {
            <p class="text-sm text-ink-muted">
                "SM anchor: " <span class="font-mono text-xs text-ink">{op}</span>
            </p>
        }
    });
    view! {
        <PageHeader
            title=case.id
            subtitle=case.test_purpose
            crumbs=vec![
                Crumb::new("Catalogue", "/catalogue"),
                Crumb::new(chapter_key.to_owned(), format!("/catalogue/{chapter_key}")),
            ]
        />
        <div class="grid grid-cols-1 gap-4 lg:grid-cols-2">
            <section class=format!("{CARD_PAD} lg:col-span-2")>
                <h2 class=CARD_TITLE>"Description"</h2>
                <p class="whitespace-pre-wrap text-sm text-ink">{case.description}</p>
                <div class="mt-3 flex flex-wrap gap-3 text-sm text-ink-muted">
                    <span>{format!("kind: {}", case.kind)}</span>
                    <span>{format!("component: {}", case.component)}</span>
                </div>
                {anchor}
            </section>
            <section class=CARD_PAD>
                <h2 class=CARD_TITLE>"Spec citations"</h2>
                <ul class=format!("{WELL} space-y-1")>{refs}</ul>
                <p class="mt-2 text-sm text-ink-muted">
                    "An expectation is refuted by a better reading of the cited text, and by nothing else."
                </p>
            </section>
            <section class=CARD_PAD>
                <h2 class=CARD_TITLE>"Wire realization"</h2>
                <ul class="space-y-1.5">{bindings}</ul>
                <h2 class=format!("{CARD_TITLE} mt-4")>"Corpus references"</h2>
                <ul class="space-y-1">{corpus}</ul>
            </section>
        </div>
    }
}
