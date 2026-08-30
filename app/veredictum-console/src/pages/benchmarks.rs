// SPDX-FileCopyrightText: Veredictum contributors
// SPDX-License-Identifier: Apache-2.0

//! S10 — the benchmark surface (#166).
//!
//! One route, three views, all of them addressed by the URL: the listing of
//! every record the console can see, one record in full, and the aligned
//! comparison over the selected ones. The selection lives in the query string,
//! so a comparison is shareable, refresh-safe and works before the WASM bundle
//! has loaded.
//!
//! The boundary statement is permanent furniture on every view, verbatim from
//! the records being shown: a table of speed numbers is exactly the artifact
//! somebody quotes out of context, and the sentence that says what it is not
//! travels with it.
//!
//! The upload is a plain `<form method="post" enctype="multipart/form-data">`
//! posting to a server-owned axum route. Uploaded records are transient and
//! swept; the console keeps no state of its own.

use leptos::prelude::{
    AddAnyAttr, ClassAttribute, CollectView, ElementChild, Get, GlobalAttributes, IntoAny,
    IntoView, Memo, Resource, Suspend, Transition, With, component, view,
};
use leptos_meta::Title;
use leptos_router::components::A;
use leptos_router::hooks::use_query_map;

use crate::bench_api::fns::{fetch_bench_comparison, fetch_bench_screen};
use crate::bench_api::{
    BenchComparison, BenchDetail, BenchListing, BenchScreen, CLI_COMPARE, CLI_DETAIL,
    CompareScreen, HISTOGRAM_NOTE, UPLOAD_PATH, ms, ops, ratio, us,
};
use crate::components::data_table::{TABLE, TABLE_WRAP, TD, TH};
use crate::components::empty_state::EmptyState;
use crate::components::field::BTN_PRIMARY;
use crate::components::format_view::inline_error;
use crate::components::page_header::PageHeader;
use crate::components::surface::{CARD_PAD, CARD_TITLE, WELL};

/// The whole surface's state, which lives in the URL and nowhere else.
#[derive(Debug, Clone, PartialEq, Eq)]
struct UrlParams {
    /// The address of the record being read in full.
    record: Option<String>,
    /// The comma-separated addresses selected for comparison.
    compare: String,
    /// How many records the upload route just accepted.
    uploaded: Option<String>,
    /// Why the upload route refused a batch.
    refused: Option<String>,
}

/// The four query parameters this surface reads.
///
/// A helper rather than the component body: reading the query map inside a
/// `#[component]` fn silences `clippy::must_use_candidate` there, turning the
/// crate's `#[expect]` idiom into an unfulfilled-expectation build failure.
fn params_from_url() -> Memo<UrlParams> {
    let query = use_query_map();
    Memo::new(move |_| {
        query.with(|map| UrlParams {
            record: map.get("record"),
            compare: map.get("compare").unwrap_or_default(),
            uploaded: map.get("uploaded"),
            refused: map.get("refused"),
        })
    })
}

/// The selection with `key` added when it is absent and removed when it is
/// present, so one anchor is the whole toggle.
#[must_use]
pub fn toggle(selection: &str, key: &str) -> String {
    let mut keys: Vec<&str> = selection
        .split(',')
        .map(str::trim)
        .filter(|entry| !entry.is_empty())
        .collect();
    match keys.iter().position(|entry| *entry == key) {
        Some(at) => {
            let _removed = keys.remove(at);
        }
        None => keys.push(key),
    }
    keys.join(",")
}

/// Whether `key` is in the comma-separated selection.
#[must_use]
pub fn selected(selection: &str, key: &str) -> bool {
    selection
        .split(',')
        .map(str::trim)
        .any(|entry| entry == key)
}

/// The benchmark surface.
#[expect(
    clippy::must_use_candidate,
    reason = "a Leptos component is mounted by the framework, never consumed as a value"
)]
#[component]
pub fn Benchmarks() -> impl IntoView {
    let params = params_from_url();
    let screen = Resource::new(
        move || params.get().record,
        |record| async move { fetch_bench_screen(record).await },
    );
    let comparison = Resource::new(
        move || params.get().compare,
        |selection| async move { fetch_bench_comparison(Some(selection)).await },
    );

    view! {
        <Title text="Benchmarks · Veredictum console" />
        <PageHeader
            title="Benchmarks"
            subtitle="Comparative speed records: what a pack drove, on which machine, under which posture — and what a bench number is never evidence of."
        />
        {move || {
            let state = params.get();
            banner(state.uploaded, state.refused)
        }}
        <Transition fallback=|| {
            view! { <p class="text-sm text-ink-muted">"Reading the records…"</p> }
        }>
            {move || Suspend::new(async move {
                let selection = params.get().compare;
                match screen.await {
                    Ok(BenchScreen::Listing(listing)) => {
                        listing_view(*listing, &selection).into_any()
                    }
                    Ok(BenchScreen::Record(detail)) => detail_view(*detail).into_any(),
                    Ok(BenchScreen::Unknown { reason }) => unknown_view(&reason).into_any(),
                    Err(e) => inline_error(&e.to_string()).into_any(),
                }
            })}
        </Transition>
        <Transition fallback=|| ()>
            {move || Suspend::new(async move {
                match comparison.await {
                    Ok(CompareScreen::Aligned(aligned)) => comparison_view(&aligned).into_any(),
                    Ok(CompareScreen::NeedsMore { selected }) => {
                        view! {
                            <p class="mt-4 text-sm text-ink-muted">
                                {format!(
                                    "{selected} record selected. Pick one more to align them side by side.",
                                )}
                            </p>
                        }
                            .into_any()
                    }
                    Ok(CompareScreen::Unknown { reason }) => inline_error(&reason).into_any(),
                    Ok(CompareScreen::Idle) => ().into_any(),
                    Err(e) => inline_error(&e.to_string()).into_any(),
                }
            })}
        </Transition>
    }
}

/// The upload route's own answer, when it redirected with one.
fn banner(uploaded: Option<String>, refused: Option<String>) -> impl IntoView {
    let accepted = uploaded.map(|count| {
        view! {
            <div class="mb-4 rounded-control border border-ok/40 bg-ok-subtle px-3 py-2 text-sm text-ink">
                {format!(
                    "{count} record(s) accepted. They are transient: this console keeps nothing of its own, and an uploaded batch is swept on a timer.",
                )}
            </div>
        }
    });
    let refusal = refused.map(|reason| {
        view! {
            <div
                role="alert"
                class="mb-4 rounded-control border border-danger/40 bg-danger-subtle px-3 py-2 text-sm text-ink"
            >
                <span class="font-medium">"The upload was refused: "</span>
                {reason}
            </div>
        }
    });
    view! {
        {accepted}
        {refusal}
    }
}

/// The boundary statement box: verbatim from the records being shown.
///
/// One box per distinct statement, so a set of records that disagree about
/// what they are says so rather than quietly showing the first one.
fn boundary_box(statements: &[String]) -> impl IntoView + use<> {
    let lines = statements
        .iter()
        .cloned()
        .map(|statement| {
            view! { <p class="text-sm text-ink">{statement}</p> }
        })
        .collect_view();
    view! {
        <section class=format!("{CARD_PAD} mb-4 border-warn/40")>
            <h2 class=CARD_TITLE>"What a bench record is"</h2>
            <div class="space-y-2">{lines}</div>
        </section>
    }
}

/// The upload control: one plain form, no script anywhere near it.
fn upload_form() -> impl IntoView {
    view! {
        <section class=format!("{CARD_PAD} mb-4")>
            <h2 class=CARD_TITLE>"Add records"</h2>
            <p class="mb-3 text-sm text-ink-muted">
                "Upload the JSON document a bench run writes ("
                <span class="font-mono text-xs">"bench-result*.json"</span>
                "). At most 8 records at a time, 8 MiB each. They are read, listed, and swept."
            </p>
            <form
                method="post"
                action=UPLOAD_PATH
                enctype="multipart/form-data"
                class="flex flex-wrap items-center gap-2"
            >
                <input
                    type="file"
                    id="records"
                    name="records"
                    multiple
                    accept=".json,application/json"
                    required
                    class="text-sm text-ink file:mr-3 file:rounded-control file:border file:border-edge-strong file:bg-raised file:px-3 file:py-1.5 file:text-sm file:font-medium file:text-ink hover:file:bg-sunken"
                />
                <button type="submit" class=BTN_PRIMARY>
                    "Read the records"
                </button>
            </form>
        </section>
    }
}

/// The command-line equivalent, so the console is never the only witness.
fn cli_box(command: &'static str, what: &'static str) -> impl IntoView {
    view! {
        <section class=format!("{CARD_PAD} mt-4")>
            <h2 class=CARD_TITLE>"The same thing without this page"</h2>
            <p class="mb-2 text-sm text-ink-muted">{what}</p>
            <pre class=format!("{WELL} overflow-x-auto font-mono text-xs text-ink")>{command}</pre>
        </section>
    }
}

/// The listing: every record, with a compare toggle and the mount it came
/// from.
fn listing_view(listing: BenchListing, selection: &str) -> impl IntoView + use<> {
    let boundary = (!listing.boundary_statements.is_empty())
        .then(|| boundary_box(&listing.boundary_statements).into_any());
    let unreadable = (!listing.unreadable.is_empty()).then(|| {
        let lines = listing
            .unreadable
            .iter()
            .cloned()
            .map(|line| view! { <li class="font-mono text-xs">{line}</li> })
            .collect_view();
        view! {
            <div class="mt-4 rounded-control border border-danger/40 bg-danger-subtle px-3 py-2">
                <p class="text-sm font-medium text-ink">
                    "Files that look like records and do not read"
                </p>
                <ul class="mt-1 list-disc space-y-0.5 pl-5 text-ink">{lines}</ul>
            </div>
        }
        .into_any()
    });
    let body = if listing.records.is_empty() {
        empty_listing(&listing.out).into_any()
    } else {
        record_table(listing, selection).into_any()
    };
    view! {
        {boundary}
        {upload_form()}
        {body}
        {unreadable}
        {cli_box(CLI_DETAIL, "A bench run writes the record this page reads:")}
    }
}

/// The honest empty state: the mount that was walked, named.
fn empty_listing(out: &str) -> impl IntoView + use<> {
    let hint = format!(
        "Nothing under {out} carries a bench-result document. Run `veredictum bench` into that directory, or upload a record above."
    );
    view! { <EmptyState icon=icondata_lu::LuGauge message="No bench records yet" hint=hint /> }
}

/// The record table, one row per record, each row carrying its compare toggle.
fn record_table(listing: BenchListing, selection: &str) -> impl IntoView + use<> {
    // The key is the record's address, which is a digest of its own path: a
    // stable, data-derived, unique row key — never an index (rules §4).
    let rows = listing
        .records
        .into_iter()
        .map(|record| {
            let open = format!("/benchmarks?record={}", record.key);
            let toggled = toggle(selection, &record.key);
            let compare_href = if toggled.is_empty() {
                String::from("/benchmarks")
            } else {
                format!("/benchmarks?compare={toggled}")
            };
            let picked = selected(selection, &record.key);
            let compare_label = if picked { "deselect" } else { "compare" };
            let compare_class = if picked {
                "rounded-control bg-accent-subtle px-1.5 py-0.5 text-xs font-medium text-accent-ink"
            } else {
                "rounded-control border border-edge-strong px-1.5 py-0.5 text-xs text-ink-muted hover:bg-sunken"
            };
            let submittable = if record.submittable {
                view! {
                    <span class="rounded-control bg-ok-subtle px-1.5 py-0.5 text-xs text-ink">
                        "submittable"
                    </span>
                }
                    .into_any()
            } else {
                let unmet = record.unmet.join(", ");
                view! {
                    <span class="rounded-control bg-warn-subtle px-1.5 py-0.5 text-xs text-ink">
                        {format!("not submittable ({unmet})")}
                    </span>
                }
                .into_any()
            };
            view! {
                <tr class="hover:bg-sunken">
                    <td class=format!("{TD} whitespace-nowrap")>
                        <A href=open attr:class="font-medium text-sm text-accent hover:underline">
                            {record.label}
                        </A>
                        <div class="font-mono text-xs text-ink-faint">{record.file}</div>
                    </td>
                    <td class=TD>
                        <span class="rounded-control bg-sunken px-1.5 py-0.5 text-xs text-ink-muted">
                            {record.source.as_str()}
                        </span>
                    </td>
                    <td class=TD>
                        <span class="font-mono text-xs">{record.pack}</span>
                    </td>
                    <td class=TD>
                        <span class="font-mono text-xs">{record.target}</span>
                    </td>
                    <td class=TD>
                        <span class="font-mono text-xs text-ink-muted">{record.machine}</span>
                    </td>
                    <td class=TD>
                        <span class="font-mono text-xs">{record.posture_profile}</span>
                    </td>
                    <td class=TD>
                        <span class="tabular-nums">{record.repetitions}</span>
                    </td>
                    <td class=TD>{submittable}</td>
                    <td class=TD>
                        <A href=compare_href attr:class=compare_class>
                            {compare_label}
                        </A>
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
                        <th class=TH>"Record"</th>
                        <th class=TH>"Source"</th>
                        <th class=TH>"Pack"</th>
                        <th class=TH>"Target"</th>
                        <th class=TH>"Machine"</th>
                        <th class=TH>"Posture"</th>
                        <th class=TH>"Repetitions"</th>
                        <th class=TH>"Submittable"</th>
                        <th class=TH>"Compare"</th>
                    </tr>
                </thead>
                <tbody>{rows}</tbody>
            </table>
        </div>
    }
}

/// An address that resolves to nothing, said in one sentence with a way back.
fn unknown_view(reason: &str) -> impl IntoView + use<> {
    let reason = reason.to_owned();
    view! {
        <div
            role="alert"
            class="rounded-control border border-danger/40 bg-danger-subtle px-3 py-2 text-sm text-ink"
        >
            {reason}
        </div>
        <p class="mt-3 text-sm">
            <A href="/benchmarks" attr:class="text-accent hover:underline">
                "Back to the record list"
            </A>
        </p>
    }
}

/// One record in full.
fn detail_view(detail: BenchDetail) -> impl IntoView + use<> {
    let boundary = boundary_box(std::slice::from_ref(&detail.boundary_statement)).into_any();
    let header = detail_header(&detail).into_any();
    let posture = posture_section(&detail).into_any();
    let seeds = seed_section(&detail).into_any();
    let phases = phase_section(&detail).into_any();
    let shares = failed_share_section(&detail).into_any();
    let baselines = baseline_section(&detail).into_any();
    let relative = relative_section(&detail).into_any();
    view! {
        <p class="mb-3 text-sm">
            <A href="/benchmarks" attr:class="text-accent hover:underline">
                "‹ every record"
            </A>
        </p>
        {boundary}
        {header}
        {posture}
        {seeds}
        {phases}
        {shares}
        {baselines}
        {relative}
        <section class=format!("{CARD_PAD} mt-4")>
            <h2 class=CARD_TITLE>"Methodology"</h2>
            <p class="text-sm text-ink">{detail.methodology_statement}</p>
        </section>
        {cli_box(CLI_DETAIL, "A bench run writes this record:")}
    }
}

/// What the header says beyond the facts table: why the record is not
/// submittable, that it left the pack's pinned configuration, and which
/// instant its `version_at_time` reads addressed.
fn header_notes(detail: &BenchDetail) -> impl IntoView + use<> {
    let unmet = (!detail.unmet.is_empty()).then(|| {
        let lines = detail
            .unmet
            .iter()
            .map(|(token, statement)| {
                view! {
                    <li class="text-sm text-ink">
                        <span class="font-mono text-xs">{token.clone()}</span>
                        {format!(" — {statement}")}
                    </li>
                }
            })
            .collect_view();
        view! {
            <div class="mt-3 rounded-control border border-warn/40 bg-warn-subtle px-3 py-2">
                <p class="text-sm font-medium text-ink">"Not submittable, and why"</p>
                <ul class="mt-1 list-disc space-y-0.5 pl-5">{lines}</ul>
            </div>
        }
    });
    let configuration = (!detail.reference_configuration).then(|| {
        view! {
            <p class="mt-2 text-sm text-ink">
                "This run is off the pack's pinned configuration, so its numbers are not comparable with the reference figures the pack describes."
            </p>
        }
    });
    let version_at_time = detail.version_at_time.clone().map(|instant| {
        view! {
            <p class="mt-2 text-sm text-ink-muted">
                {format!(
                    "Every version_at_time read addressed {instant}, captured after the seed phases finished.",
                )}
            </p>
        }
    });
    view! {
        {configuration}
        {version_at_time}
        {unmet}
    }
}

/// The summary header: what was driven, against what, on which machine, and
/// whether the record may be offered for ranking.
fn detail_header(detail: &BenchDetail) -> impl IntoView + use<> {
    let notes = header_notes(detail);
    let submittable_chip = if detail.submittable {
        "rounded-control bg-ok-subtle px-2 py-1 text-sm font-medium text-ink"
    } else {
        "rounded-control bg-warn-subtle px-2 py-1 text-sm font-medium text-ink"
    };
    let submittable_text = if detail.submittable {
        "Submittable: the record carries what a ranked figure needs."
    } else {
        "Not submittable."
    };
    view! {
        <section class=CARD_PAD>
            <h2 class=CARD_TITLE>{detail.label.clone()}</h2>
            <p class=submittable_chip>{submittable_text}</p>
            <dl class="mt-3 grid grid-cols-1 gap-x-6 gap-y-1 text-sm sm:grid-cols-2">
                <div>
                    <dt class="text-ink-muted">"Pack"</dt>
                    <dd class="font-mono text-xs text-ink">{detail.pack.clone()}</dd>
                </div>
                <div>
                    <dt class="text-ink-muted">"Target"</dt>
                    <dd class="break-all font-mono text-xs text-ink">
                        {detail.target.clone()}
                        {detail
                            .sut_version
                            .clone()
                            .map(|version| format!(" (reports {version})"))
                            .unwrap_or_default()}
                    </dd>
                </div>
                <div>
                    <dt class="text-ink-muted">"Machine (the load generator)"</dt>
                    <dd class="break-all font-mono text-xs text-ink">{detail.machine.clone()}</dd>
                </div>
                <div>
                    <dt class="text-ink-muted">"Seed"</dt>
                    <dd class="font-mono text-xs text-ink">{detail.seed.clone()}</dd>
                </div>
                <div>
                    <dt class="text-ink-muted">"Window"</dt>
                    <dd class="font-mono text-xs text-ink">
                        {format!("{} → {}", detail.started_at, detail.finished_at)}
                    </dd>
                </div>
                <div>
                    <dt class="text-ink-muted">"Scale"</dt>
                    <dd class="font-mono text-xs text-ink">
                        {format!(
                            "factor {:.3}, declared workers {}, reference configuration {}",
                            detail.scale_factor,
                            detail.declared_workers,
                            detail.reference_configuration,
                        )}
                    </dd>
                </div>
                <div>
                    <dt class="text-ink-muted">"Repetitions"</dt>
                    <dd class="font-mono text-xs text-ink">{detail.repetitions}</dd>
                </div>
                <div>
                    <dt class="text-ink-muted">"Failed-arrival ceiling"</dt>
                    <dd class="font-mono text-xs text-ink">{ratio(detail.max_failed_share)}</dd>
                </div>
            </dl>
            <p class="mt-3 text-sm text-ink-muted">{detail.pack_description.clone()}</p>
            {notes}
        </section>
    }
}

/// The posture block: what was switched on behind the numbers, and how far
/// each item is stood behind.
fn posture_section(detail: &BenchDetail) -> impl IntoView + use<> {
    let rows = detail
        .posture
        .iter()
        .map(|line| {
            let chip = if line.verified {
                "rounded-control bg-ok-subtle px-1.5 py-0.5 text-xs text-ink"
            } else {
                "rounded-control bg-sunken px-1.5 py-0.5 text-xs text-ink-muted"
            };
            view! {
                <tr>
                    <td class=TD>
                        <span class="font-mono text-xs">{line.item.clone()}</span>
                    </td>
                    <td class=TD>
                        <span class="font-mono text-xs">{line.declared.clone()}</span>
                    </td>
                    <td class=TD>
                        <span class=chip>{line.assurance.clone()}</span>
                    </td>
                    <td class=TD>
                        <span class="text-xs text-ink-muted">{line.evidence.join("; ")}</span>
                    </td>
                </tr>
            }
        })
        .collect_view();
    let comparability = (!detail.comparability.is_empty()).then(|| {
        let lines = detail
            .comparability
            .iter()
            .cloned()
            .map(|line| view! { <li class="text-sm text-ink">{line}</li> })
            .collect_view();
        view! {
            <div class="mt-3">
                <p class="text-sm font-medium text-ink">"Comparability"</p>
                <ul class="mt-1 list-disc space-y-0.5 pl-5">{lines}</ul>
            </div>
        }
    });
    view! {
        <section class=format!("{CARD_PAD} mt-4")>
            <h2 class=CARD_TITLE>{format!("Posture `{}`", detail.posture_profile)}</h2>
            <p class="mb-3 text-sm text-ink">{detail.posture_summary.clone()}</p>
            <div class=TABLE_WRAP>
                <table class=TABLE>
                    <thead>
                        <tr>
                            <th class=TH>"Item"</th>
                            <th class=TH>"Declared"</th>
                            <th class=TH>"Assurance"</th>
                            <th class=TH>"Canary evidence"</th>
                        </tr>
                    </thead>
                    <tbody>{rows}</tbody>
                </table>
            </div>
            <p class="mt-2 text-sm text-ink-muted">
                "A verified item was observed black-box at BOTH ends of the measured window; a declared-only item is a claim this record carries because nothing on the wire discloses it."
            </p>
            {comparability}
        </section>
    }
}

/// The closed-loop work: the bulk loads and the sweeps, each labelled by the
/// discipline that produced it.
fn seed_section(detail: &BenchDetail) -> impl IntoView + use<> {
    if detail.seed_phases.is_empty() && detail.sweeps.is_empty() {
        return ().into_any();
    }
    let seeds = detail
        .seed_phases
        .iter()
        .map(|seed| {
            view! {
                <li class="text-sm text-ink">
                    {format!(
                        "Seed phase `{}` ({}): {} EHRs × {} compositions on {} worker(s) in {:.1}s, {} writes/s, {:.2} ms/composition whole-loop.",
                        seed.name,
                        seed.regime,
                        seed.ehrs,
                        seed.compositions_per_ehr,
                        seed.workers,
                        seed.elapsed_s,
                        ops(seed.writes_per_s),
                        seed.ms_per_composition,
                    )}
                </li>
            }
        })
        .collect_view();
    let sweeps = detail
        .sweeps
        .iter()
        .map(|sweep| {
            view! {
                <li class="text-sm text-ink">
                    {format!(
                        "Sweep `{}` ({}) repetition {}: {} request(s) over {} composition(s) on {} worker(s) in {:.1}s, {} us/request whole-loop.",
                        sweep.name,
                        sweep.regime,
                        sweep.repetition,
                        sweep.requests,
                        sweep.compositions,
                        sweep.workers,
                        sweep.elapsed_s,
                        us(sweep.us_per_request),
                    )}
                </li>
            }
        })
        .collect_view();
    view! {
        <section class=format!("{CARD_PAD} mt-4")>
            <h2 class=CARD_TITLE>"Closed-loop work"</h2>
            <p class="mb-2 text-sm text-ink-muted">
                "A closed-loop figure is bounded by its own worker pool, so it is a throughput or an average and never a latency claim."
            </p>
            <ul class="list-disc space-y-1 pl-5">{seeds} {sweeps}</ul>
        </section>
    }
    .into_any()
}

/// The per-phase percentile tables, in the command line's own column order.
fn phase_section(detail: &BenchDetail) -> impl IntoView + use<> {
    let tables = detail
        .phases
        .iter()
        .map(|table| {
            let rows = table
                .rows
                .iter()
                .map(|row| {
                    view! {
                        <tr>
                            <td class=TD>
                                <span class="font-mono text-xs">{row.operation.clone()}</span>
                            </td>
                            {latency_cell(row.p50_us)}
                            {latency_cell(row.p90_us)}
                            {latency_cell(row.p99_us)}
                            {latency_cell(row.p999_us)}
                            <td class=TD>
                                <span class="tabular-nums">{ops(row.throughput_ops_s)}</span>
                            </td>
                            {latency_cell(row.p99_iqr_us)}
                            <td class=TD>
                                <span class="tabular-nums">{row.repetitions}</span>
                            </td>
                        </tr>
                    }
                })
                .collect_view();
            view! {
                <div class="mt-4">
                    <h3 class="mb-1 text-sm font-semibold text-ink-heading">
                        {format!("Phase `{}`", table.phase)}
                        <span class="ml-2 rounded-control bg-sunken px-1.5 py-0.5 font-mono text-xs font-normal text-ink-muted">
                            {table.regime.clone()}
                        </span>
                    </h3>
                    <div class=TABLE_WRAP>
                        <table class=TABLE>
                            <thead>
                                <tr>
                                    <th class=TH>"Operation"</th>
                                    <th class=TH>"p50"</th>
                                    <th class=TH>"p90"</th>
                                    <th class=TH>"p99"</th>
                                    <th class=TH>"p99.9"</th>
                                    <th class=TH>"ops/s"</th>
                                    <th class=TH>"IQR of p99"</th>
                                    <th class=TH>"Reps"</th>
                                </tr>
                            </thead>
                            <tbody>{rows}</tbody>
                        </table>
                    </div>
                </div>
            }
        })
        .collect_view();
    view! {
        <section class=format!("{CARD_PAD} mt-4")>
            <h2 class=CARD_TITLE>"Cross-repetition percentiles"</h2>
            <p class="text-sm text-ink-muted">
                "Each figure is the median across the run's repetitions, in microseconds with the millisecond reading beside it. The discipline label on every phase says which question its numbers answer."
            </p>
            <p class="mt-2 text-sm text-ink-muted">{HISTOGRAM_NOTE}</p>
            {tables}
        </section>
    }
}

/// One latency cell: the microseconds the command line prints, and the
/// millisecond reading beside it.
fn latency_cell(value: f64) -> impl IntoView {
    view! {
        <td class=TD>
            <span class="tabular-nums">{us(value)}</span>
            <span class="ml-1 text-xs text-ink-faint">{format!("{} ms", ms(value))}</span>
        </td>
    }
}

/// The failed-arrival readings, target first and then each baseline.
fn failed_share_section(detail: &BenchDetail) -> impl IntoView + use<> {
    if detail.failed_shares.is_empty() {
        return ().into_any();
    }
    let rows = detail
        .failed_shares
        .iter()
        .map(|reading| {
            let class = if reading.breaches {
                "rounded-control bg-danger-subtle px-1.5 py-0.5 text-xs font-medium text-ink"
            } else {
                "tabular-nums"
            };
            view! {
                <tr>
                    <td class=TD>{reading.side.clone()}</td>
                    <td class=TD>
                        <span class="tabular-nums">{reading.repetition}</span>
                    </td>
                    <td class=TD>
                        <span class="font-mono text-xs">{reading.phase.clone()}</span>
                    </td>
                    <td class=TD>
                        <span class="font-mono text-xs text-ink-muted">
                            {reading.regime.clone()}
                        </span>
                    </td>
                    <td class=TD>
                        <span class="tabular-nums">{reading.count}</span>
                    </td>
                    <td class=TD>
                        <span class="tabular-nums">{reading.errors}</span>
                    </td>
                    <td class=TD>
                        <span class="tabular-nums">{ratio(reading.share)}</span>
                    </td>
                    <td class=TD>
                        <span class="font-mono text-xs">
                            {reading
                                .worst_operation
                                .clone()
                                .unwrap_or_else(|| String::from("(none recorded)"))}
                        </span>
                    </td>
                    <td class=TD>
                        <span class=class>{ratio(reading.worst_share)}</span>
                    </td>
                </tr>
            }
        })
        .collect_view();
    view! {
        <section class=format!("{CARD_PAD} mt-4")>
            <h2 class=CARD_TITLE>"Failed-arrival share"</h2>
            <p class="mb-2 text-sm text-ink-muted">
                {format!(
                    "The pack pins a ceiling of {} per repetition, phase and operation. Percentiles taken over failed arrivals measure the failure rather than the system.",
                    ratio(detail.max_failed_share),
                )}
            </p>
            <div class=TABLE_WRAP>
                <table class=TABLE>
                    <thead>
                        <tr>
                            <th class=TH>"Side"</th>
                            <th class=TH>"Repetition"</th>
                            <th class=TH>"Phase"</th>
                            <th class=TH>"Discipline"</th>
                            <th class=TH>"Arrivals"</th>
                            <th class=TH>"Failed"</th>
                            <th class=TH>"Share"</th>
                            <th class=TH>"Worst operation"</th>
                            <th class=TH>"Worst share"</th>
                        </tr>
                    </thead>
                    <tbody>{rows}</tbody>
                </table>
            </div>
        </section>
    }
    .into_any()
}

/// The same-machine references, so a reader can recompose them.
fn baseline_section(detail: &BenchDetail) -> impl IntoView + use<> {
    if detail.baselines.is_empty() {
        return ().into_any();
    }
    let cards = detail
        .baselines
        .iter()
        .map(|baseline| {
            let images = baseline
                .images
                .iter()
                .map(|(role, image)| {
                    view! { <li class="font-mono text-xs text-ink-muted">{format!("{role}: {image}")}</li> }
                })
                .collect_view();
            view! {
                <li class="border-t border-edge pt-2 first:border-0 first:pt-0">
                    <p class="text-sm font-medium text-ink">
                        {format!("{} at {}", baseline.display_name, baseline.base_url)}
                        <span class="ml-2 rounded-control bg-sunken px-1.5 py-0.5 font-mono text-xs font-normal text-ink-muted">
                            {baseline.posture_profile.clone()}
                        </span>
                    </p>
                    <p class="font-mono text-xs text-ink-muted">
                        {format!("recipe {}", baseline.recipe)}
                    </p>
                    <p class="font-mono text-xs text-ink-muted">
                        {format!("ceilings {}", baseline.resources)}
                    </p>
                    {baseline
                        .sut_version
                        .clone()
                        .map(|version| {
                            view! {
                                <p class="font-mono text-xs text-ink-muted">
                                    {format!("reported version: {version}")}
                                </p>
                            }
                        })}
                    <ul class="mt-1 space-y-0.5">{images}</ul>
                </li>
            }
        })
        .collect_view();
    view! {
        <section class=format!("{CARD_PAD} mt-4")>
            <h2 class=CARD_TITLE>"Same-machine baselines"</h2>
            <p class="mb-2 text-sm text-ink-muted">
                "Each baseline ran the same pack at the same seed and the same repetition count, on this machine, in this session, from fresh volumes."
            </p>
            <ul class="space-y-3">{cards}</ul>
        </section>
    }
    .into_any()
}

/// The relative index the record derived, one table per reference.
fn relative_section(detail: &BenchDetail) -> impl IntoView + use<> {
    if detail.relative.is_empty() {
        return ().into_any();
    }
    let tables = detail
        .relative
        .iter()
        .map(|table| {
            let rows = table
                .rows
                .iter()
                .map(|row| {
                    view! {
                        <tr>
                            <td class=TD>
                                <span class="font-mono text-xs">{row.phase.clone()}</span>
                            </td>
                            <td class=TD>
                                <span class="font-mono text-xs text-ink-muted">
                                    {row.regime.clone()}
                                </span>
                            </td>
                            <td class=TD>
                                <span class="font-mono text-xs">{row.operation.clone()}</span>
                            </td>
                            <td class=TD>
                                <span class="font-mono text-xs">{row.metric.clone()}</span>
                            </td>
                            <td class=TD>
                                <span class="tabular-nums">{ops(row.target_median)}</span>
                            </td>
                            <td class=TD>
                                <span class="tabular-nums">{ops(row.baseline_median)}</span>
                            </td>
                            <td class=TD>
                                <span class="font-medium tabular-nums">{ratio(row.index)}</span>
                            </td>
                        </tr>
                    }
                })
                .collect_view();
            let gaps = (!table.gaps.is_empty()).then(|| {
                let lines = table
                    .gaps
                    .iter()
                    .cloned()
                    .map(|gap| view! { <li class="font-mono text-xs text-ink-muted">{gap}</li> })
                    .collect_view();
                view! {
                    <div class="mt-2">
                        <p class="text-sm text-ink">"No index exists for:"</p>
                        <ul class="mt-1 list-disc space-y-0.5 pl-5">{lines}</ul>
                    </div>
                }
            });
            view! {
                <div class="mt-4">
                    <h3 class="mb-1 text-sm font-semibold text-ink-heading">
                        {format!("vs {}", table.display_name)}
                    </h3>
                    <p class="mb-2 text-sm text-ink-muted">{table.derivation.clone()}</p>
                    <div class=TABLE_WRAP>
                        <table class=TABLE>
                            <thead>
                                <tr>
                                    <th class=TH>"Phase"</th>
                                    <th class=TH>"Discipline"</th>
                                    <th class=TH>"Operation"</th>
                                    <th class=TH>"Metric"</th>
                                    <th class=TH>"Target"</th>
                                    <th class=TH>"Baseline"</th>
                                    <th class=TH>"Index"</th>
                                </tr>
                            </thead>
                            <tbody>{rows}</tbody>
                        </table>
                    </div>
                    {gaps}
                </div>
            }
        })
        .collect_view();
    view! {
        <section class=format!("{CARD_PAD} mt-4")>
            <h2 class=CARD_TITLE>"Relative index"</h2>
            <p class="text-sm text-ink-muted">
                "The one figure that travels between machines, read out of the record as the run derived it."
            </p>
            {tables}
        </section>
    }
    .into_any()
}

/// The aligned comparison over the selected records.
fn comparison_view(comparison: &BenchComparison) -> impl IntoView + use<> {
    let boundary = boundary_box(&comparison.boundary_statements).into_any();
    let warnings = comparison_warnings(comparison).into_any();
    let columns = comparison_columns(comparison).into_any();
    let body = comparison_body(comparison).into_any();
    view! {
        <section class="mt-6">
            <h2 class="mb-3 text-lg font-semibold text-ink-heading">"Side by side"</h2>
            {boundary}
            {warnings}
            {columns}
            {body}
            {cli_box(CLI_COMPARE, "The same alignment, from a terminal:")}
        </section>
    }
}

/// Everything that makes the columns less than directly comparable.
fn comparison_warnings(comparison: &BenchComparison) -> impl IntoView + use<> {
    if comparison.warnings.is_empty() {
        return view! {
            <p class="mb-3 text-sm text-ink">
                "Every column ran the same pack version, under the same posture, from the same generator host."
            </p>
        }
        .into_any();
    }
    let lines = comparison
        .warnings
        .iter()
        .cloned()
        .map(|warning| view! { <li class="text-sm text-ink">{warning}</li> })
        .collect_view();
    view! {
        <div
            role="alert"
            class="mb-3 rounded-control border border-warn/40 bg-warn-subtle px-3 py-2"
        >
            <p class="text-sm font-medium text-ink">"Read these before reading the numbers."</p>
            <ul class="mt-1 list-disc space-y-0.5 pl-5">{lines}</ul>
        </div>
    }
    .into_any()
}

/// The column header table: the machine, the posture and the submittability of
/// each record being compared.
fn comparison_columns(comparison: &BenchComparison) -> impl IntoView + use<> {
    let rows = comparison
        .columns
        .iter()
        .map(|column| {
            let submittable = if column.submittable {
                String::from("yes")
            } else {
                format!("no ({})", column.unmet.join(", "))
            };
            view! {
                <tr>
                    <td class=TD>
                        <span class="font-medium">{column.label.clone()}</span>
                    </td>
                    <td class=TD>
                        <span class="font-mono text-xs">{column.pack.clone()}</span>
                    </td>
                    <td class=TD>
                        <span class="break-all font-mono text-xs text-ink-muted">
                            {column.machine.clone()}
                        </span>
                    </td>
                    <td class=TD>
                        <span class="font-mono text-xs">{column.posture_profile.clone()}</span>
                        <div class="break-all font-mono text-xs text-ink-faint">
                            {column.posture_signature.clone()}
                        </div>
                    </td>
                    <td class=TD>
                        <span class="font-mono text-xs">
                            {column
                                .sut_version
                                .clone()
                                .unwrap_or_else(|| String::from("(undisclosed)"))}
                        </span>
                    </td>
                    <td class=TD>
                        <span class="tabular-nums">{column.repetitions}</span>
                    </td>
                    <td class=TD>{submittable}</td>
                    <td class=TD>
                        <span class="tabular-nums">
                            {format!(
                                "{} of {}",
                                ratio(column.worst_failed_share),
                                ratio(column.max_failed_share),
                            )}
                        </span>
                    </td>
                    <td class=TD>
                        <span class="tabular-nums">
                            {format!(
                                "{:.3}{}",
                                column.scale_factor,
                                if column.reference_configuration { "" } else { " (off reference)" },
                            )}
                        </span>
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
                        <th class=TH>"Column"</th>
                        <th class=TH>"Pack"</th>
                        <th class=TH>"Machine"</th>
                        <th class=TH>"Posture"</th>
                        <th class=TH>"SUT version"</th>
                        <th class=TH>"Repetitions"</th>
                        <th class=TH>"Submittable"</th>
                        <th class=TH>"Worst failed share"</th>
                        <th class=TH>"Scale"</th>
                    </tr>
                </thead>
                <tbody>{rows}</tbody>
            </table>
        </div>
    }
}

/// The aligned body: one row per phase, operation and metric, one cell per
/// column, every row carrying the discipline that produced it.
fn comparison_body(comparison: &BenchComparison) -> impl IntoView + use<> {
    let headers = comparison
        .columns
        .iter()
        .map(|column| view! { <th class=TH>{column.label.clone()}</th> })
        .collect_view();
    let rows = comparison
        .rows
        .iter()
        .map(|row| {
            let cells = row
                .cells
                .iter()
                .map(|cell| match (cell.median, cell.iqr) {
                    (Some(median), Some(iqr)) => view! {
                        <td class=TD>
                            <span class="tabular-nums">{ops(median)}</span>
                            <span class="ml-1 text-xs text-ink-faint">
                                {format!("({})", ops(iqr))}
                            </span>
                        </td>
                    }
                    .into_any(),
                    _ => view! { <td class=TD>"—"</td> }.into_any(),
                })
                .collect_view();
            view! {
                <tr>
                    <td class=TD>
                        <span class="font-mono text-xs">{row.phase.clone()}</span>
                    </td>
                    <td class=TD>
                        <span class="font-mono text-xs text-ink-muted">{row.regime.clone()}</span>
                    </td>
                    <td class=TD>
                        <span class="font-mono text-xs">{row.operation.clone()}</span>
                    </td>
                    <td class=TD>
                        <span class="font-mono text-xs">{row.metric.clone()}</span>
                    </td>
                    {cells}
                </tr>
            }
        })
        .collect_view();
    view! {
        <p class="mt-3 text-sm text-ink-muted">
            "Each cell is the cross-repetition median with the inter-quartile range in parentheses. A closed-loop average and an open-loop percentile answer different questions and are never read against one another, so the discipline column says which produced the row."
        </p>
        <div class=format!("{TABLE_WRAP} mt-1")>
            <table class=TABLE>
                <thead>
                    <tr>
                        <th class=TH>"Phase"</th>
                        <th class=TH>"Discipline"</th>
                        <th class=TH>"Operation"</th>
                        <th class=TH>"Metric"</th>
                        {headers}
                    </tr>
                </thead>
                <tbody>{rows}</tbody>
            </table>
        </div>
    }
}

#[cfg(test)]
mod tests {
    use super::{selected, toggle};

    /// One anchor is the whole selection control: a key not in the selection
    /// joins it, and a key already there leaves.
    #[test]
    fn the_compare_anchor_toggles_one_key() {
        assert_eq!(toggle("", "aa"), "aa");
        assert_eq!(toggle("aa", "bb"), "aa,bb");
        assert_eq!(toggle("aa,bb", "aa"), "bb");
        assert_eq!(toggle("aa,bb", "bb"), "aa");
        assert_eq!(toggle("aa", "aa"), "");
        // A selection carrying stray whitespace still reads: the URL is user
        // input, and a hand-edited one must not silently duplicate a key.
        assert_eq!(toggle(" aa , bb ", "aa"), "bb");
    }

    /// The row's own state is read from the same selection the anchor writes.
    #[test]
    fn a_selected_key_reads_as_selected() {
        assert!(selected("aa,bb", "aa"));
        assert!(selected(" aa , bb ", "bb"));
        assert!(!selected("aa,bb", "cc"));
        assert!(!selected("", "aa"));
    }
}
