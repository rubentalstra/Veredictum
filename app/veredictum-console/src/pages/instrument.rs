// SPDX-FileCopyrightText: Veredictum contributors
// SPDX-License-Identifier: Apache-2.0

//! S1 — the instrument landing (#61, #64): the catalogue's own numbers, the
//! mounts they were read from, and where to start. Every count comes from
//! the same expressions the CLI's validate summary prints.

use leptos::prelude::{
    AddAnyAttr, ClassAttribute, ElementChild, IntoAny, IntoView, Resource, Suspend, Suspense,
    component, view,
};
use leptos_meta::Title;
use leptos_router::components::A;

use crate::catalogue_api::InstrumentView;
use crate::catalogue_api::fns::fetch_instrument;
use crate::components::field::{BTN_PRIMARY, BTN_SECONDARY};
use crate::components::format_view::inline_error;
use crate::components::page_header::PageHeader;
use crate::components::stat_card::StatCard;
use crate::components::surface::{CARD_PAD, CARD_TITLE};

/// The landing surface.
#[expect(
    clippy::must_use_candidate,
    reason = "a Leptos component is mounted by the framework, never consumed as a value"
)]
#[component]
pub fn Instrument() -> impl IntoView {
    // Created in setup, never inside a Suspend (rules §4).
    let summary = Resource::new(|| (), |()| fetch_instrument());

    view! {
        <Title text="Instrument · Veredictum console" />
        <PageHeader
            title="Instrument"
            subtitle="The catalogue's own numbers, read from the mounts at startup — the same counts the validate summary prints."
        />
        <Suspense fallback=|| {
            view! { <p class="text-sm text-ink-muted">"Reading the catalogue…"</p> }
        }>
            {move || Suspend::new(async move {
                match summary.await {
                    Ok(InstrumentView::Loaded(s)) => {
                        let findings_ok = s.findings == 0;
                        view! {
                            <div class="grid grid-cols-1 gap-4 sm:grid-cols-2 xl:grid-cols-4">
                                <StatCard
                                    label="Case cores"
                                    value=s.cases.to_string()
                                    icon=icondata_lu::LuFileCheck
                                    href="/catalogue"
                                />
                                <StatCard
                                    label="Operation bindings"
                                    value=s.bindings.to_string()
                                    icon=icondata_lu::LuCable
                                />
                                <StatCard
                                    label="Party statements"
                                    value=s.parties.to_string()
                                    icon=icondata_lu::LuUsers
                                />
                                <StatCard
                                    label="Validate findings"
                                    value=s.findings.to_string()
                                    icon=icondata_lu::LuShieldCheck
                                />
                            </div>
                            <div class="mt-4 grid grid-cols-1 gap-4 lg:grid-cols-2">
                                <section class=CARD_PAD>
                                    <h2 class=CARD_TITLE>"The mounts"</h2>
                                    <dl class="space-y-1 font-mono text-xs text-ink">
                                        <div class="flex gap-2">
                                            <dt class="text-ink-muted">"root"</dt>
                                            <dd>{s.root}</dd>
                                        </div>
                                        <div class="flex gap-2">
                                            <dt class="text-ink-muted">"specs"</dt>
                                            <dd>{s.specs}</dd>
                                        </div>
                                    </dl>
                                    <p class="mt-3 text-sm text-ink-muted">
                                        {if findings_ok {
                                            "Zero findings: the catalogue passes every machine gate."
                                        } else {
                                            "The catalogue carries findings — a red number here means the tree itself needs attention before any server is graded."
                                        }}
                                    </p>
                                </section>
                                <section class=CARD_PAD>
                                    <h2 class=CARD_TITLE>"Start"</h2>
                                    <p class="mb-3 text-sm text-ink-muted">
                                        "Point the instrument at a reachable CDR and read the verdict, or check a published record."
                                    </p>
                                    <div class="flex flex-wrap items-center gap-2">
                                        <A href="/run" attr:class=BTN_PRIMARY>
                                            "Grade a server"
                                        </A>
                                        <A href="/verify" attr:class=BTN_SECONDARY>
                                            "Verify a record"
                                        </A>
                                    </div>
                                </section>
                            </div>
                        }
                            .into_any()
                    }
                    Ok(InstrumentView::Missing(missing)) => {
                        view! {
                            <section class=CARD_PAD>
                                <h2 class=CARD_TITLE>"No catalogue is mounted"</h2>
                                <p class="text-sm text-ink">
                                    "The console reads the catalogue and the vendored specifications as paths. It looked for the artifact root at "
                                    <code class="font-mono text-xs">{missing.root}</code>
                                    " and the spec tree at "
                                    <code class="font-mono text-xs">{missing.specs}</code>
                                    ". In the container, mount the repository at /work; locally, start the console from the repository root or set "
                                    <code class="font-mono text-xs">"VEREDICTUM_ROOT"</code> " and "
                                    <code class="font-mono text-xs">"VEREDICTUM_SPECS"</code> "."
                                </p>
                                <div class="mt-3">{inline_error(&missing.reason)}</div>
                            </section>
                        }
                            .into_any()
                    }
                    Err(e) => inline_error(&e.to_string()).into_any(),
                }
            })}
        </Suspense>
    }
}
