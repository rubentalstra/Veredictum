// SPDX-FileCopyrightText: Veredictum contributors
// SPDX-License-Identifier: Apache-2.0

//! S3 Connect and S4 Scope (#61, #65): the run wizard's first half.
//!
//! Every step is a URL, so refresh and back never lose the user; the
//! wizard's memory is the server-side draft (`run_api`), and the secret
//! values the connect form collects reach only that draft.

use leptos::prelude::{
    Action, AddAnyAttr, ClassAttribute, CollectView, Effect, ElementChild, Get, GlobalAttributes,
    IntoAny, IntoView, OnAttribute, OnTargetAttribute, PropAttribute, Resource, RwSignal,
    ServerAction, Set, StyleAttribute, Suspend, Suspense, Transition, Update, With, component,
    view,
};
use leptos_meta::Title;
use leptos_router::components::{A, Redirect};

use crate::components::field::{BTN_PRIMARY, BTN_SECONDARY, INPUT, LABEL, TEXTAREA};
use crate::components::format_view::{Pane, inline_error};
use crate::components::page_header::{Crumb, PageHeader};
use crate::components::surface::{CARD_PAD, CARD_TITLE};
use crate::components::toast;
use crate::run_api::fns::{
    ProbeAndSave, cancel_run, compose_claim, fetch_draft, fetch_job, fetch_scope_preview,
    fetch_statement_body, fetch_statements, fetch_tier_counts, save_scope, start_run,
};
use crate::run_api::{AuthChoice, ClaimSummary, ProbeAnswer, ScopePreview, ScopeTier};
use crate::run_job::{JobStatus, JobView};

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
pub(crate) fn steps(active: &'static str) -> impl IntoView + use<> {
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
                        {move || {
                            if probe.pending().get() { "Probing…" } else { "Probe connection" }
                        }}
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

/// S4 — the scope: the tier row that builds a claim, the pasted claim, the
/// filter, and the honest preview.
#[expect(
    clippy::must_use_candidate,
    reason = "a Leptos component is mounted by the framework, never consumed as a value"
)]
#[expect(
    clippy::too_many_lines,
    reason = "the tier row, the statement pick, the filter, the preview and the save answer — one cohesive screen, its sections erased per rules §1"
)]
#[component]
pub fn Scope() -> impl IntoView {
    let draft = Resource::new(|| (), |()| fetch_draft());
    let statements = Resource::new(|| (), |()| fetch_statements());
    let statement_json = RwSignal::new(String::new());
    let example_note = RwSignal::new(None::<Result<String, String>>);
    // The sanctioned dispatch-continuation shape: the click is the event,
    // the answer lands in the action's own async block.
    let load_example = Action::new(move |path: &String| {
        let path = path.clone();
        async move {
            match fetch_statement_body(path.clone()).await {
                Ok(body) => {
                    statement_json.set(body);
                    example_note.set(Some(Ok(format!("Loaded {path}."))));
                }
                Err(e) => example_note.set(Some(Err(e.to_string()))),
            }
        }
    });
    let tier_counts = Resource::new(|| (), |()| fetch_tier_counts());
    let checked_tiers = RwSignal::new(Vec::<ScopeTier>::new());
    let toggle_tier = move |tier: ScopeTier, on: bool| {
        checked_tiers.update(|selection| {
            selection.retain(|held| *held != tier);
            if on {
                selection.push(tier);
            }
            // NOTE: no openEHR spec governs this — our own design: the
            // published judgement refuses a STANDARD claim that does not also
            // claim CORE, so the row keeps the two paired.
            match (tier, on) {
                (ScopeTier::Standard, true) => {
                    if !selection.contains(&ScopeTier::Core) {
                        selection.push(ScopeTier::Core);
                    }
                }
                (ScopeTier::Core, false) => selection.retain(|held| *held != ScopeTier::Standard),
                _ => {}
            }
        });
    };
    let composed_note = RwSignal::new(None::<Result<String, String>>);
    let compose = Action::new(move |selection: &Vec<ScopeTier>| {
        let selection = selection.clone();
        async move {
            match compose_claim(Some(selection)).await {
                Ok(document) => {
                    statement_json.set(document);
                    composed_note.set(Some(Ok(String::from(
                        "Composed from the checked tiers. Read it, edit it if you need to, then save the scope.",
                    ))));
                }
                Err(e) => composed_note.set(Some(Err(e.to_string()))),
            }
        }
    });
    let filter = RwSignal::new(String::new());
    let record_exchanges = RwSignal::new(false);
    // Every mutation below reports BOTH outcomes as a toast and keeps its
    // inline pane, where a diagnostic is worth reading line by line. The
    // answers land in each action's own async continuation: the dispatch is
    // the user event, so no Effect mediates (the Leptos book discourages
    // signal-writing effects — book/reactivity/working_with_signals §4).
    let previewed = RwSignal::new(None::<Result<ScopePreview, String>>);
    let preview = Action::new(move |filter: &String| {
        let filter = filter.clone();
        async move {
            match fetch_scope_preview(filter).await {
                Ok(scope) => {
                    toast::success(
                        "Selection previewed",
                        &format!(
                            "{} cases are in scope; every one of them lands as an outcome or a recorded exception.",
                            scope.total
                        ),
                    );
                    previewed.set(Some(Ok(scope)));
                }
                Err(e) => {
                    let body = e.to_string();
                    toast::error(
                        "The preview was refused",
                        &format!(
                            "The catalogue could not be counted: {body}. Check the mounted artifact root."
                        ),
                    );
                    previewed.set(Some(Err(body)));
                }
            }
        }
    });
    let saved = RwSignal::new(None::<Result<Option<ClaimSummary>, String>>);
    let save = Action::new(move |input: &ScopeInput| {
        let input = input.clone();
        async move {
            match save_scope(
                Some(input.statement_json),
                Some(input.filter),
                input.record_exchanges,
            )
            .await
            {
                Ok(claim) => {
                    toast::success("Scope saved", &saved_body(claim.as_ref()));
                    saved.set(Some(Ok(claim)));
                }
                Err(e) => {
                    let body = e.to_string();
                    toast::error(
                        "The scope was refused",
                        &format!("The claim was not stored: {body}. Correct it and save again."),
                    );
                    saved.set(Some(Err(body)));
                }
            }
        }
    });
    let started = RwSignal::new(None::<Result<u64, String>>);
    let start = Action::new(move |(): &()| async move {
        match start_run().await {
            Ok(id) => {
                toast::success(
                    "Run started",
                    &format!("Job {id} is driving; watch it on the Live screen."),
                );
                started.set(Some(Ok(id)));
            }
            Err(e) => {
                let body = e.to_string();
                toast::error(
                    "The run was refused",
                    &format!("Nothing was driven: {body}."),
                );
                started.set(Some(Err(body)));
            }
        }
    });

    view! {
        <Title text="Scope · Run · Veredictum console" />
        <PageHeader
            title="Scope"
            subtitle="Build the claim from tiers or paste the vendor's own, then preview what will process before anything starts."
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
                            if view_draft.probed_ok {
                                " · probed ✓"
                            } else {
                                " · probe not 2xx"
                            },
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
                    <span class=LABEL>"Build the claim from tiers"</span>
                    <p class="mt-1 text-sm text-ink-muted">
                        "Checking a tier claims the capabilities the matrix requires for it; the counts say how many catalogue cases those capabilities gate. Composing writes the claim into the box below, where you read it before anything runs. Option branches stay undeclared, because only the party running the server knows which branch it realizes."
                    </p>
                    <Suspense fallback=|| {
                        view! { <p class="text-sm text-ink-muted">"Counting the tiers…"</p> }
                    }>
                        {move || Suspend::new(async move {
                            match tier_counts.await {
                                Ok(rows) => {
                                    let boxes = rows
                                        .into_iter()
                                        .map(|row| {
                                            let tier = row.tier;
                                            let label = format!(
                                                "{} · {} capabilities · {} cases",
                                                tier.token(),
                                                row.capabilities,
                                                row.cases,
                                            );
                                            view! {
                                                <label
                                                    class="flex items-center gap-2 text-sm text-ink"
                                                    for=tier.control_id()
                                                >
                                                    <input
                                                        id=tier.control_id()
                                                        type="checkbox"
                                                        class="size-4 accent-accent"
                                                        prop:checked=move || {
                                                            checked_tiers.with(|held| held.contains(&tier))
                                                        }
                                                        on:change:target=move |ev| {
                                                            toggle_tier(tier, ev.target().checked());
                                                        }
                                                    />
                                                    {label}
                                                </label>
                                            }
                                        })
                                        .collect_view();
                                    view! {
                                        <div class="mt-2 grid gap-2 sm:grid-cols-2">{boxes}</div>
                                    }
                                        .into_any()
                                }
                                Err(e) => inline_error(&e.to_string()).into_any(),
                            }
                        })}
                    </Suspense>
                    <div class="mt-3 flex items-center gap-2">
                        <button
                            type="button"
                            class=BTN_SECONDARY
                            on:click=move |_| {
                                compose.dispatch(checked_tiers.get());
                            }
                        >
                            "Compose the claim"
                        </button>
                    </div>
                    {move || {
                        composed_note
                            .get()
                            .map(|note| match note {
                                Ok(line) => {
                                    view! { <p class="mt-2 text-sm text-ink-muted">{line}</p> }
                                        .into_any()
                                }
                                Err(e) => inline_error(&e).into_any(),
                            })
                    }}
                </div>
                <div>
                    <label class=LABEL for="statement-json">
                        "Party statement (ICS) — the claim this run grades"
                    </label>
                    <p class="mt-1 text-sm text-ink-muted">
                        "Paste the vendor's own statement.json, or load a committed example. Leave the box empty for an honest no-claim run: everything applicable drives, nothing is certified."
                    </p>
                    <Suspense fallback=|| {
                        view! { <p class="text-sm text-ink-muted">"Reading party/…"</p> }
                    }>
                        {move || Suspend::new(async move {
                            match statements.await {
                                Ok(rows) => {
                                    let buttons = rows
                                        .into_iter()
                                        .map(|row| {
                                            let label = format!("Load {}", row.product);
                                            let path = row.path;
                                            view! {
                                                <button
                                                    type="button"
                                                    class=BTN_SECONDARY
                                                    on:click=move |_| {
                                                        load_example.dispatch(path.clone());
                                                    }
                                                >
                                                    {label}
                                                </button>
                                            }
                                        })
                                        .collect_view();
                                    view! {
                                        <div class="mt-2 flex flex-wrap items-center gap-2">
                                            {buttons}
                                        </div>
                                    }
                                        .into_any()
                                }
                                Err(e) => inline_error(&e.to_string()).into_any(),
                            }
                        })}
                    </Suspense>
                    <textarea
                        id="statement-json"
                        class=format!("{TEXTAREA} mt-2 h-48")
                        placeholder="{ \"product\": { \"name\": …, \"version\": … }, \"claims\": { \"profiles\": [\"CORE\"], \"capabilities\": [s] }, … }"
                        prop:value=move || statement_json.get()
                        on:input:target=move |ev| statement_json.set(ev.target().value())
                    ></textarea>
                    {move || {
                        example_note
                            .get()
                            .map(|note| match note {
                                Ok(line) => {
                                    view! { <p class="mt-1 text-sm text-ink-muted">{line}</p> }
                                        .into_any()
                                }
                                Err(e) => inline_error(&e).into_any(),
                            })
                    }}
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
                <div>
                    <label class="flex items-center gap-2 text-sm text-ink" for="record-exchanges">
                        <input
                            id="record-exchanges"
                            type="checkbox"
                            class="size-4 accent-accent"
                            prop:checked=move || record_exchanges.get()
                            on:change:target=move |ev| record_exchanges.set(ev.target().checked())
                        />
                        "Record the wire exchanges"
                    </label>
                    <p class="mt-1 text-sm text-ink-muted">
                        "Off by default. The transcript keeps every request and response the run drove, so it can carry real patient data from the server you are grading. It lands beside results.json in the run's output directory, and the sealed record covers it."
                    </p>
                </div>
                <div class="flex items-center gap-2">
                    <button
                        type="button"
                        class=BTN_SECONDARY
                        on:click=move |_| {
                            preview.dispatch(filter.get());
                        }
                    >
                        "Preview selection"
                    </button>
                    <button
                        type="button"
                        class=BTN_PRIMARY
                        on:click=move |_| {
                            save.dispatch(ScopeInput {
                                statement_json: statement_json.get(),
                                filter: filter.get(),
                                record_exchanges: record_exchanges.get(),
                            });
                            preview.dispatch(filter.get());
                        }
                    >
                        "Save scope"
                    </button>
                </div>
                {move || {
                    previewed
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
                            Err(e) => inline_error(&e).into_any(),
                        })
                }}
                {move || {
                    saved
                        .get()
                        .map(|result| match result {
                            Ok(claim) => {
                                let line = saved_body(claim.as_ref());
                                view! {
                                    <div class="space-y-2">
                                        <p class="text-sm text-ink">{line}</p>
                                        <button
                                            type="button"
                                            class=BTN_PRIMARY
                                            on:click=move |_| {
                                                start.dispatch(());
                                            }
                                        >
                                            "Start the run"
                                        </button>
                                    </div>
                                }
                                    .into_any()
                            }
                            Err(e) => inline_error(&e).into_any(),
                        })
                }}
                {move || {
                    started
                        .get()
                        .map(|result| match result {
                            Ok(id) => {
                                view! {
                                    <div class="flex items-center gap-2">
                                        <p class="text-sm text-ink">
                                            {format!("Run started (job {id}).")}
                                        </p>
                                        <A href="/run/live" attr:class=BTN_SECONDARY>
                                            "Watch it live"
                                        </A>
                                    </div>
                                }
                                    .into_any()
                            }
                            Err(e) => inline_error(&e).into_any(),
                        })
                }}
            </div>
        </section>
    }
}

/// What one Save-scope dispatch carries: the pasted claim, the filter, and
/// the wire-recording choice, read off the form at click time.
#[derive(Debug, Clone)]
struct ScopeInput {
    /// The pasted statement document, empty for a no-claim run.
    statement_json: String,
    /// The case-id filter, empty for the whole catalogue.
    filter: String,
    /// Whether the run persists its wire exchanges.
    record_exchanges: bool,
}

/// The one sentence a saved scope is reported by, in the toast and inline —
/// one reader for one claim.
fn saved_body(claim: Option<&ClaimSummary>) -> String {
    claim.map_or_else(
        || {
            String::from(
                "Scope saved without a claim: everything applicable drives, nothing is certified.",
            )
        },
        |summary| {
            let profiles = if summary.profiles.is_empty() {
                String::from("none")
            } else {
                summary.profiles.join(", ")
            };
            format!(
                "Claim accepted: {} — profiles {profiles} — {} capabilities.",
                summary.product, summary.capabilities,
            )
        },
    )
}

/// Renders milliseconds as `mm:ss` — display truncation is the intent of
/// each division here.
#[expect(
    clippy::integer_division,
    reason = "clock display truncates by definition; the remainder is shown in the seconds field"
)]
fn mmss(ms: u64) -> String {
    let seconds = ms / 1000;
    format!("{:02}:{:02}", seconds / 60, seconds % 60)
}

/// S5 — the live run: progress from the engine's own stream, the honest
/// estimate, the output tail, and cancel. The page polls and therefore
/// rejoins the running job on refresh.
#[expect(
    clippy::must_use_candidate,
    reason = "a Leptos component is mounted by the framework, never consumed as a value"
)]
#[component]
pub fn Live() -> impl IntoView {
    // A browser-side tick drives the poll; on the server the resource loads
    // once and the timer never runs (pause/resume are browser-only).
    let tick = RwSignal::new(0_u64);
    let job = Resource::new(move || tick.get(), |_| fetch_job());
    // Cancel is a mutation, so both outcomes are notifications: the live
    // screen's own status chip is the standing record, and a silent refusal
    // below the fold would read as "nothing happened".
    let cancel = Action::new(move |(): &()| async move {
        match cancel_run().await {
            Ok(()) => toast::success(
                "Cancel requested",
                "The engine process was signalled; the job reports cancelled once it exits.",
            ),
            Err(e) => toast::error(
                "The cancel was refused",
                &format!("The run is still driving: {e}."),
            ),
        }
    });
    Effect::new(move |_| {
        let _pausable =
            leptos_use::use_interval_fn(move || tick.update(|t| *t = t.wrapping_add(1)), 1_000);
    });

    view! {
        <Title text="Live run · Veredictum console" />
        <PageHeader
            title="Live run"
            subtitle="Progress is the engine's own stream; the estimate is a moving median, labelled as such."
            crumbs=vec![Crumb::new("Run", "/run/connect")]
        >
            <div class="flex items-center gap-1">{steps("live")}</div>
        </PageHeader>
        <Transition fallback=|| {
            view! { <p class="text-sm text-ink-muted">"Reading the job…"</p> }
        }>
            {move || Suspend::new(async move {
                match job.await {
                    Ok(Some(view_job)) => live_view(&view_job, cancel).into_any(),
                    Ok(None) => idle_view().into_any(),
                    Err(e) => inline_error(&e.to_string()).into_any(),
                }
            })}
        </Transition>
    }
}

/// The no-job state: the wizard starts at Connect.
fn idle_view() -> impl IntoView + use<> {
    view! {
        <div class="space-y-2">
            <p class="text-sm text-ink">"No run is in flight. Start one from Connect."</p>
            <A href="/run/connect" attr:class=BTN_SECONDARY>
                "Go to Connect"
            </A>
        </div>
    }
}

/// The loaded job's sections — plain assembly, erased per section.
#[expect(
    clippy::too_many_lines,
    reason = "the progress header, the tail pane and the finished summary — one cohesive assembly, each section already erased"
)]
fn live_view(job: &JobView, cancel: Action<(), ()>) -> impl IntoView + use<> {
    // Display truncation is intended: a bar at 99.9% shows 99.
    let percent = job
        .completed
        .saturating_mul(100)
        .checked_div(job.total)
        .unwrap_or(0);
    let counter = if job.total > 0 {
        format!("{} / {} cases", job.completed, job.total)
    } else {
        String::from("progress stream not available from this engine build")
    };
    let eta = job
        .eta_ms
        .map(|ms| format!("~{} remaining (estimate)", mmss(ms)));
    let current = job.current_case.clone().map(
        |case| view! { <p class="font-mono text-xs text-ink-muted">{format!("now: {case}")}</p> },
    );
    let status_line = match &job.status {
        JobStatus::Running => view! {
            <span class="rounded-control bg-run-subtle px-2 py-0.5 text-xs font-medium text-run-ink">
                "running"
            </span>
        }
        .into_any(),
        JobStatus::Finished => view! {
            <span class="rounded-control bg-ok-subtle px-2 py-0.5 text-xs font-medium text-ink">
                "finished"
            </span>
        }
        .into_any(),
        JobStatus::Cancelled => view! {
            <span class="rounded-control bg-warn-subtle px-2 py-0.5 text-xs font-medium text-ink">
                "cancelled"
            </span>
        }
        .into_any(),
        JobStatus::Failed(reason) => view! {
            <span class="rounded-control bg-danger-subtle px-2 py-0.5 text-xs font-medium text-ink">
                {format!("failed: {reason}")}
            </span>
        }
        .into_any(),
    };
    let running = job.status == JobStatus::Running;
    let tail = job.tail.join("\n");
    let finished = job.finished.clone().map(|summary| {
        view! {
            <section class=format!("{CARD_PAD} mt-4")>
                <h2 class=CARD_TITLE>"Outcome"</h2>
                <p class="tabular-nums text-sm text-ink">
                    {format!(
                        "{} passed · {} failed · {} errored · {} not applicable",
                        summary.passed,
                        summary.failed,
                        summary.errored,
                        summary.not_applicable,
                    )}
                </p>
                <p class="mt-1 font-mono text-xs text-ink-muted">
                    {format!("results: {}", summary.results_path)}
                </p>
                <div class="mt-2 flex items-center gap-2">
                    <A href="/run/results" attr:class=BTN_PRIMARY>
                        "Read the results"
                    </A>
                    <A href="/run/verdicts" attr:class=BTN_SECONDARY>
                        "Compute the verdicts"
                    </A>
                </div>
            </section>
        }
    });
    view! {
        <section class=CARD_PAD>
            <div class="flex flex-wrap items-center justify-between gap-3">
                <div class="flex items-center gap-2">
                    <h2 class="text-sm font-semibold text-ink-heading">
                        {format!("job {} · {}", job.id, job.sut_name)}
                    </h2>
                    {status_line}
                </div>
                {running
                    .then(|| {
                        view! {
                            <button
                                type="button"
                                class=crate::components::field::BTN_DANGER
                                on:click=move |_| {
                                    cancel.dispatch(());
                                }
                            >
                                "Cancel run"
                            </button>
                        }
                    })}
            </div>
            <div class="mt-3 h-2 w-full overflow-hidden rounded-control bg-sunken">
                <div
                    class="h-full rounded-control bg-run transition-all"
                    style=format!("width: {percent}%")
                ></div>
            </div>
            <div class="mt-2 flex flex-wrap items-center gap-3 text-sm text-ink-muted">
                <span class="tabular-nums">{counter}</span>
                <span class="tabular-nums">{format!("elapsed {}", mmss(job.elapsed_ms))}</span>
                {eta.map(|eta| view! { <span class="tabular-nums">{eta}</span> })}
            </div>
            {current}
        </section>
        <div class="mt-4">
            <Pane label="engine output (tail)" body=tail />
        </div>
        {finished}
    }
}
