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
use crate::catalogue_api::fns::{fetch_case_detail, fetch_chapter_bands, fetch_chapters};
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

/// The `?tier=` profile filter from the URL (CORE / STANDARD / OPTIONS /
/// SEC-BASIC; empty = every tier).
fn query_tier() -> Memo<String> {
    let query = use_query_map();
    Memo::new(move |_| query.with(|q| q.get("tier").unwrap_or_default()))
}

/// One labeled fact list on the case card; an empty list says so instead of
/// vanishing.
fn fact_list(label: &'static str, values: Vec<String>) -> impl IntoView + use<> {
    let body = if values.is_empty() {
        view! { <p class="text-sm text-ink-faint">"none"</p> }.into_any()
    } else {
        values
            .into_iter()
            .map(|value| view! { <li class="font-mono text-xs text-ink">{value}</li> })
            .collect_view()
            .into_any()
    };
    view! {
        <div>
            <h3 class="mb-1 text-xs font-semibold uppercase tracking-wide text-ink-muted">
                {label}
            </h3>
            <ul class="space-y-0.5">{body}</ul>
        </div>
    }
}

/// The tier badge row for a case.
fn tier_badges(tiers: &[String]) -> impl IntoView + use<> {
    tiers
        .iter()
        .map(|tier| {
            let class = match tier.as_str() {
                "CORE" => "rounded-control bg-accent-subtle px-1.5 py-0.5 text-xs font-medium text-accent-ink",
                "SEC-BASIC" => "rounded-control bg-warn-subtle px-1.5 py-0.5 text-xs font-medium text-ink",
                _ => "rounded-control bg-sunken px-1.5 py-0.5 text-xs text-ink-muted",
            };
            view! { <span class=class>{tier.clone()}</span> }
        })
        .collect_view()
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

/// One chapter's case listing: band sections over URL-state search, tier
/// filter and paging — the same two-level taxonomy the published SVG renders.
#[expect(
    clippy::must_use_candidate,
    reason = "a Leptos component is mounted by the framework, never consumed as a value"
)]
#[expect(
    clippy::too_many_lines,
    reason = "the filter form and the band sections — one cohesive screen, its sections erased per the hydration rules"
)]
#[component]
pub fn Chapter() -> impl IntoView {
    let chapter = param("chapter");
    let q = query_q();
    let tier = query_tier();
    let page = page_from_url();
    // Reactive inputs in the SOURCE; the fetcher is untracked by design.
    let rows = Resource::new(
        move || (chapter.get(), q.get(), tier.get()),
        |(chapter, q, tier)| fetch_chapter_bands(chapter, q, tier),
    );

    let tier_links = move || {
        let chapter_key = chapter.get();
        let active = tier.get();
        let q_now = q.get();
        ["", "CORE", "STANDARD", "OPTIONS", "SEC-BASIC"]
            .into_iter()
            .map(|token| {
                let label = if token.is_empty() { "all" } else { token };
                let mut href = format!("/catalogue/{chapter_key}?tier={token}");
                if !q_now.is_empty() {
                    // Infallible on String; the idiomatic append the lint wants.
                    let _ = std::fmt::Write::write_fmt(&mut href, format_args!("&q={q_now}"));
                }
                let class = if active == token {
                    "rounded-control bg-accent px-2 py-1 text-xs font-medium text-on-accent"
                } else {
                    "rounded-control border border-edge-strong px-2 py-1 text-xs text-ink hover:bg-sunken"
                };
                view! {
                    <A href=href attr:class=class>
                        {label}
                    </A>
                }
            })
            .collect_view()
    };

    view! {
        <Title text=move || format!("{} · Catalogue · Veredictum console", chapter.get()) />
        <PageHeader
            title=chapter
            subtitle="One small isolated case per behaviour, grouped by the same bands the published conformance visuals render."
            crumbs=vec![Crumb::new("Catalogue", "/catalogue")]
        />
        <div class="mb-4 flex flex-wrap items-center gap-3">
            <Form method="GET" action="">
                <div class="flex items-center gap-2">
                    <input
                        type="search"
                        name="q"
                        value=move || q.get()
                        placeholder="Filter by case id…"
                        class=INPUT
                    />
                    <input type="hidden" name="tier" value=move || tier.get() />
                    <button type="submit" class="text-sm text-accent hover:underline">
                        "filter"
                    </button>
                </div>
            </Form>
            <div class="flex items-center gap-1.5">{tier_links}</div>
        </div>
        // Transition keeps the old rows visible while a filter reloads —
        // no fallback flash on every submit.
        <Transition fallback=|| {
            view! { <p class="text-sm text-ink-muted">"Reading the chapter…"</p> }
        }>
            {move || Suspend::new(async move {
                let current_page = page.get();
                let chapter_key = chapter.get();
                match rows.await {
                    Ok(bands) => {
                        let total: usize = bands.iter().map(|band| band.cases.len()).sum();
                        if total == 0 {
                            return view! {
                                <EmptyState
                                    icon=icondata_lu::LuSearchX
                                    message="No case matches"
                                    hint="Loosen the filter or the tier, or check the chapter key in the URL."
                                />
                            }
                                .into_any();
                        }
                        // Paging windows over the FLAT case sequence; band
                        // headers render wherever their first visible case
                        // lands, so the two-level reading survives paging.
                        let (start, end) = page_window(current_page, total);
                        let mut index = 0_usize;
                        let sections = bands
                            .into_iter()
                            .filter_map(|band| {
                                let band_len = band.cases.len();
                                let band_start = index;
                                index += band_len;
                                let visible_from = start.max(band_start);
                                let visible_to = end.min(band_start + band_len);
                                if visible_from >= visible_to {
                                    return None;
                                }
                                let rows = band
                                    .cases
                                    .get(visible_from - band_start..visible_to - band_start)
                                    .unwrap_or_default()
                                    .iter()
                                    .cloned()
                                    .map(|row| {
                                        let href = format!(
                                            "/catalogue/{chapter_key}/{}",
                                            row.id,
                                        );
                                        let badges = tier_badges(&row.tiers);
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
                                                <td class=TD>
                                                    <span class="flex flex-wrap gap-1">{badges}</span>
                                                </td>
                                                <td class=TD>{row.kind}</td>
                                                <td class=TD>
                                                    <span class="line-clamp-2 text-sm">{row.purpose}</span>
                                                </td>
                                            </tr>
                                        }
                                    })
                                    .collect_view();
                                Some(
                                    view! {
                                        <tbody>
                                            <tr>
                                                <th
                                                    colspan="4"
                                                    class="bg-sunken px-3 py-1.5 text-left text-xs font-semibold text-ink-heading"
                                                >
                                                    {format!("{} · {band_len} case(s)", band.band)}
                                                </th>
                                            </tr>
                                            {rows}
                                        </tbody>
                                    },
                                )
                            })
                            .collect_view();
                        view! {
                            <div class=TABLE_WRAP>
                                <table class=TABLE>
                                    <thead>
                                        <tr>
                                            <th class=TH>"Case id"</th>
                                            <th class=TH>"Tiers"</th>
                                            <th class=TH>"Kind"</th>
                                            <th class=TH>"Test purpose"</th>
                                        </tr>
                                    </thead>
                                    {sections}
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
#[expect(
    clippy::too_many_lines,
    reason = "the case card's five sections — one cohesive assembly, each section already erased"
)]
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
                <div class="mt-3 flex flex-wrap items-center gap-3 text-sm text-ink-muted">
                    <span>{format!("kind: {}", case.kind)}</span>
                    <span>{format!("component: {}", case.component)}</span>
                    <span>{case.size}</span>
                    <span class="flex flex-wrap gap-1">{tier_badges(&case.tiers)}</span>
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
            <section class=format!("{CARD_PAD} lg:col-span-2")>
                <h2 class=CARD_TITLE>"Selection facts"</h2>
                <div class="grid grid-cols-1 gap-3 text-sm sm:grid-cols-2">
                    {fact_list("Verdict-bearing capabilities", case.capabilities)}
                    {fact_list("Exercises (informative coverage)", case.exercises)}
                    {fact_list("Applies (spec-version windows)", case.applies)}
                    {fact_list("Guards (cited run conditions)", case.guards)}
                    {fact_list("Formats", case.formats)}
                    {fact_list(
                        "Register option",
                        case.option.map(|option| vec![option]).unwrap_or_default(),
                    )}
                </div>
            </section>
        </div>
    }
}
