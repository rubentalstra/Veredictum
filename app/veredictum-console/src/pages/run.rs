// SPDX-FileCopyrightText: Veredictum contributors
// SPDX-License-Identifier: Apache-2.0

//! S3 Connect and S4 Scope (#61, #65): the run wizard's first half.
//!
//! Every step is a URL, so refresh and back never lose the user; the
//! wizard's memory is the server-side draft (`run_api`), and the secret
//! values the connect form collects reach only that draft.

use leptos::prelude::{
    AddAnyAttr, ClassAttribute, CollectView, ElementChild, Get, GlobalAttributes, IntoAny,
    IntoView, OnAttribute, OnTargetAttribute, PropAttribute, Resource, RwSignal, ServerAction, Set,
    Suspend, Suspense, component, view,
};
use leptos_meta::Title;
use leptos_router::components::{A, Redirect};

use crate::components::field::{BTN_PRIMARY, BTN_SECONDARY, INPUT, LABEL, SELECT};
use crate::components::format_view::{Pane, inline_error};
use crate::components::page_header::{Crumb, PageHeader};
use crate::components::surface::{CARD_PAD, CARD_TITLE};
use crate::run_api::fns::{
    FetchScopePreview, ProbeAndSave, SaveScope, fetch_draft, fetch_statements,
};
use crate::run_api::{AuthChoice, ProbeAnswer};

/// `/run` — the wizard's entry: always the first step.
#[expect(
    clippy::must_use_candidate,
    reason = "a Leptos component is mounted by the framework, never consumed as a value"
)]
#[component]
pub fn Run() -> impl IntoView {
    view! { <Redirect path="/run/connect" /> }
}

/// The step indicator: one row, the active step accented — a URL per step.
fn steps(active: &'static str) -> impl IntoView + use<> {
    ["connect", "scope", "live", "results", "verdicts"]
        .into_iter()
        .map(|step| {
            let class = if step == active {
                "rounded-control bg-accent-subtle px-2 py-0.5 text-xs font-medium text-accent-ink"
            } else {
                "rounded-control px-2 py-0.5 text-xs text-ink-faint"
            };
            view! { <span class=class>{step}</span> }
        })
        .collect_view()
}

/// S3 — the connection form and the probe.
#[expect(
    clippy::must_use_candidate,
    reason = "a Leptos component is mounted by the framework, never consumed as a value"
)]
#[expect(
    clippy::too_many_lines,
    reason = "the connect form's fields, the auth segmented control and the probe answer — one cohesive screen, its sections erased per rules §1"
)]
#[component]
pub fn Connect() -> impl IntoView {
    let base_url = RwSignal::new(String::new());
    let sut_name = RwSignal::new(String::from("my-cdr"));
    let sut_version = RwSignal::new(String::from("unknown"));
    let auth = RwSignal::new(AuthChoice::None);
    let user = RwSignal::new(String::new());
    let password = RwSignal::new(String::new());
    let token = RwSignal::new(String::new());
    let probe = ServerAction::<ProbeAndSave>::new();

    let auth_button = move |choice: AuthChoice, label: &'static str| {
        let active = move || auth.get() == choice;
        view! {
            <button
                type="button"
                class=move || {
                    if active() {
                        "rounded-control bg-accent px-3 py-1.5 text-sm font-medium text-on-accent"
                    } else {
                        "rounded-control border border-edge-strong bg-raised px-3 py-1.5 text-sm text-ink hover:bg-sunken"
                    }
                }
                on:click=move |_| auth.set(choice)
            >
                {label}
            </button>
        }
    };

    let dispatch_probe = move |_| {
        probe.dispatch(ProbeAndSave {
            base_url: base_url.get(),
            sut_name: sut_name.get(),
            sut_version: sut_version.get(),
            auth: auth.get(),
            user: user.get(),
            password: password.get(),
            token: token.get(),
        });
    };

    view! {
        <Title text="Connect · Run · Veredictum console" />
        <PageHeader
            title="Grade a server"
            subtitle="Point the instrument at a reachable CDR. Credential values stay in memory; only the spawned run's environment ever sees them."
            crumbs=vec![Crumb::new("Run", "/run/connect")]
        >
            <div class="flex items-center gap-1">{steps("connect")}</div>
        </PageHeader>
        <section class=format!("{CARD_PAD} max-w-2xl")>
            <div class="space-y-4">
                <div>
                    <label class=LABEL for="base-url">
                        "CDR base URL"
                    </label>
                    <input
                        id="base-url"
                        type="url"
                        class=format!("{INPUT} mt-1 w-full")
                        placeholder="https://cdr.example/ehrbase/rest/openehr/v1"
                        prop:value=move || base_url.get()
                        on:input:target=move |ev| base_url.set(ev.target().value())
                    />
                </div>
                <div class="grid grid-cols-2 gap-3">
                    <div>
                        <label class=LABEL for="sut-name">
                            "Display name"
                        </label>
                        <input
                            id="sut-name"
                            type="text"
                            class=format!("{INPUT} mt-1 w-full")
                            prop:value=move || sut_name.get()
                            on:input:target=move |ev| sut_name.set(ev.target().value())
                        />
                    </div>
                    <div>
                        <label class=LABEL for="sut-version">
                            "Version label"
                        </label>
                        <input
                            id="sut-version"
                            type="text"
                            class=format!("{INPUT} mt-1 w-full")
                            prop:value=move || sut_version.get()
                            on:input:target=move |ev| sut_version.set(ev.target().value())
                        />
                    </div>
                </div>
                <div>
                    <span class=LABEL>"Authentication"</span>
                    <div class="mt-1 flex items-center gap-2">
                        {auth_button(AuthChoice::None, "None")}
                        {auth_button(AuthChoice::Basic, "Basic")}
                        {auth_button(AuthChoice::Bearer, "Bearer")}
                    </div>
                </div>
                {move || {
                    match auth.get() {
                        AuthChoice::None => ().into_any(),
                        AuthChoice::Basic => {
                            view! {
                                <div class="grid grid-cols-2 gap-3">
                                    <div>
                                        <label class=LABEL for="sut-user">
                                            "User"
                                        </label>
                                        <input
                                            id="sut-user"
                                            type="text"
                                            autocomplete="off"
                                            class=format!("{INPUT} mt-1 w-full")
                                            prop:value=move || user.get()
                                            on:input:target=move |ev| user.set(ev.target().value())
                                        />
                                    </div>
                                    <div>
                                        <label class=LABEL for="sut-pass">
                                            "Password"
                                        </label>
                                        <input
                                            id="sut-pass"
                                            type="password"
                                            autocomplete="off"
                                            class=format!("{INPUT} mt-1 w-full")
                                            prop:value=move || password.get()
                                            on:input:target=move |ev| password.set(ev.target().value())
                                        />
                                    </div>
                                </div>
                            }
                                .into_any()
                        }
                        AuthChoice::Bearer => {
                            view! {
                                <div>
                                    <label class=LABEL for="sut-token">
                                        "Bearer token"
                                    </label>
                                    <input
                                        id="sut-token"
                                        type="password"
                                        autocomplete="off"
                                        class=format!("{INPUT} mt-1 w-full")
                                        prop:value=move || token.get()
                                        on:input:target=move |ev| token.set(ev.target().value())
                                    />
                                </div>
                            }
                                .into_any()
                        }
                    }
                }}
                <div class="flex items-center gap-2">
                    <button type="button" class=BTN_SECONDARY on:click=dispatch_probe>
                        {move || if probe.pending().get() { "Probing…" } else { "Probe connection" }}
                    </button>
                </div>
                {move || {
                    probe
                        .value()
                        .get()
                        .map(|result| {
                            match result {
                                Ok(ProbeAnswer::Answered { status, elapsed_ms, ok }) => {
                                    let line = format!(
                                        "{status} · {elapsed_ms} ms · GET …/definition/template/adl1.4",
                                    );
                                    let badge = if ok {
                                        view! {
                                            <p class="text-sm text-ink">
                                                "The server answered. Continue when ready."
                                            </p>
                                        }
                                            .into_any()
                                    } else {
                                        view! {
                                            <p class="text-sm text-ink">
                                                "The server answered, though not 2xx — a refusal of the unauthenticated probe is legitimate on locked-down deployments. Check the credentials, or continue anyway."
                                            </p>
                                        }
                                            .into_any()
                                    };
                                    view! {
                                        <div class="space-y-2">
                                            <Pane label="probe answer" body=line />
                                            {badge}
                                            <A href="/run/scope" attr:class=BTN_PRIMARY>
                                                {if ok { "Continue" } else { "Continue anyway" }}
                                            </A>
                                        </div>
                                    }
                                        .into_any()
                                }
                                Ok(ProbeAnswer::Unreachable { error }) => {
                                    view! {
                                        <div class="space-y-2">
                                            {inline_error(&error)}
                                            <p class="text-sm text-ink-muted">
                                                "Nothing answered: check the URL, the network path from this console to the server, and that the CDR is up."
                                            </p>
                                        </div>
                                    }
                                        .into_any()
                                }
                                Err(e) => inline_error(&e.to_string()).into_any(),
                            }
                        })
                }}
            </div>
        </section>
    }
}

/// S4 — the scope: statement pick, filter, and the honest preview.
#[expect(
    clippy::must_use_candidate,
    reason = "a Leptos component is mounted by the framework, never consumed as a value"
)]
#[expect(
    clippy::too_many_lines,
    reason = "the statement pick, the filter, the preview and the save answer — one cohesive screen, its sections erased per rules §1"
)]
#[component]
pub fn Scope() -> impl IntoView {
    let draft = Resource::new(|| (), |()| fetch_draft());
    let statements = Resource::new(|| (), |()| fetch_statements());
    let statement = RwSignal::new(String::new());
    let filter = RwSignal::new(String::new());
    let preview = ServerAction::<FetchScopePreview>::new();
    let save = ServerAction::<SaveScope>::new();

    view! {
        <Title text="Scope · Run · Veredictum console" />
        <PageHeader
            title="Scope"
            subtitle="Pick the statement the selection applies, and preview what the run will process."
            crumbs=vec![Crumb::new("Run", "/run/connect")]
        >
            <div class="flex items-center gap-1">{steps("scope")}</div>
        </PageHeader>
        <Suspense fallback=|| {
            view! { <p class="text-sm text-ink-muted">"Reading the draft…"</p> }
        }>
            {move || Suspend::new(async move {
                match draft.await {
                    Ok(Some(view_draft)) => {
                        let connection = format!(
                            "{} · {} {} · auth {}{}",
                            view_draft.base_url,
                            view_draft.sut_name,
                            view_draft.sut_version,
                            view_draft.auth,
                            if view_draft.probed_ok { " · probed ✓" } else { " · probe not 2xx" },
                        );
                        view! { <Pane label="connection" body=connection /> }.into_any()
                    }
                    Ok(None) => {
                        view! {
                            <div class="space-y-2">
                                <p class="text-sm text-ink">
                                    "No connection draft: the wizard starts at Connect."
                                </p>
                                <A href="/run/connect" attr:class=BTN_SECONDARY>
                                    "Back to Connect"
                                </A>
                            </div>
                        }
                            .into_any()
                    }
                    Err(e) => inline_error(&e.to_string()).into_any(),
                }
            })}
        </Suspense>
        <section class=format!("{CARD_PAD} mt-4 max-w-2xl")>
            <h2 class=CARD_TITLE>"Selection"</h2>
            <div class="space-y-4">
                <div>
                    <label class=LABEL for="statement">
                        "Party statement (ICS)"
                    </label>
                    <Suspense fallback=|| {
                        view! { <p class="text-sm text-ink-muted">"Reading party/…"</p> }
                    }>
                        {move || Suspend::new(async move {
                            match statements.await {
                                Ok(rows) => {
                                    let options = rows
                                        .into_iter()
                                        .map(|row| {
                                            view! {
                                                <option value=row.path.clone()>
                                                    {format!("{} — {}", row.product, row.path)}
                                                </option>
                                            }
                                        })
                                        .collect_view();
                                    view! {
                                        <select
                                            id="statement"
                                            class=format!("{SELECT} mt-1 w-full")
                                            prop:value=move || statement.get()
                                            on:change:target=move |ev| statement.set(ev.target().value())
                                        >
                                            <option value="">
                                                "No statement — drive everything applicable"
                                            </option>
                                            {options}
                                        </select>
                                    }
                                        .into_any()
                                }
                                Err(e) => inline_error(&e.to_string()).into_any(),
                            }
                        })}
                    </Suspense>
                </div>
                <div>
                    <label class=LABEL for="filter">
                        "Case-id filter (optional)"
                    </label>
                    <input
                        id="filter"
                        type="text"
                        class=format!("{INPUT} mt-1 w-full font-mono")
                        placeholder="I_EHR_SERVICE"
                        prop:value=move || filter.get()
                        on:input:target=move |ev| filter.set(ev.target().value())
                    />
                </div>
                <div class="flex items-center gap-2">
                    <button
                        type="button"
                        class=BTN_SECONDARY
                        on:click=move |_| {
                            preview.dispatch(FetchScopePreview {
                                filter: filter.get(),
                            });
                        }
                    >
                        "Preview selection"
                    </button>
                    <button
                        type="button"
                        class=BTN_PRIMARY
                        on:click=move |_| {
                            save.dispatch(SaveScope {
                                statement: Some(statement.get()),
                                filter: Some(filter.get()),
                            });
                        }
                    >
                        "Save scope"
                    </button>
                </div>
                {move || {
                    preview
                        .value()
                        .get()
                        .map(|result| match result {
                            Ok(scope) => {
                                let chapters = scope
                                    .chapters
                                    .iter()
                                    .map(|(chapter, cases)| format!("{chapter} {cases}"))
                                    .collect::<Vec<_>>()
                                    .join(" · ");
                                view! {
                                    <div class="space-y-1">
                                        <p class="text-sm font-medium text-ink">
                                            {format!("{} cases in scope", scope.total)}
                                        </p>
                                        <p class="text-sm text-ink-muted">{chapters}</p>
                                        <p class="text-sm text-ink-muted">
                                            "Every case in scope lands as an outcome or a recorded exception; the statement excuses out-of-claim cases at drive time."
                                        </p>
                                    </div>
                                }
                                    .into_any()
                            }
                            Err(e) => inline_error(&e.to_string()).into_any(),
                        })
                }}
                {move || {
                    save.value()
                        .get()
                        .map(|result| match result {
                            Ok(()) => {
                                view! {
                                    <p class="text-sm text-ink">
                                        "Scope saved. The live run screen is under construction (#66)."
                                    </p>
                                }
                                    .into_any()
                            }
                            Err(e) => inline_error(&e.to_string()).into_any(),
                        })
                }}
            </div>
        </section>
    }
}
