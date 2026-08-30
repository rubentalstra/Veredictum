// SPDX-FileCopyrightText: Veredictum contributors
// SPDX-License-Identifier: Apache-2.0

//! S10 — the submission screen (#61, #391).
//!
//! The run already knows what it measured, when, and against which catalogue
//! revision. What it cannot know is the machine the graded server runs on, how
//! that server is configured, and what interest the person publishing the
//! result holds in it. This screen asks for exactly that, refuses an empty
//! value by name before anything is opened, and then hands the submission to
//! the instrument's own App identity.

use leptos::prelude::{
    Action, AddAnyAttr, ClassAttribute, CollectView, ElementChild, Get, GlobalAttributes, IntoAny,
    IntoView, OnAttribute, OnTargetAttribute, PropAttribute, Resource, RwSignal, Set, Suspend,
    Transition, component, view,
};
use leptos_meta::Title;
use leptos_router::components::A;

use crate::components::empty_state::EmptyState;
use crate::components::field::{BTN_PRIMARY, BTN_SECONDARY, INPUT, LABEL, SELECT, TEXTAREA};
use crate::components::format_view::inline_error;
use crate::components::page_header::{Crumb, PageHeader};
use crate::components::surface::{CARD_PAD, CARD_TITLE, WELL};
use crate::components::toast::{self, Intent, MessageBar};
use crate::pages::run::steps;
use crate::submit_api::fns::{fetch_submission, open_submission};
use crate::submit_api::{DisclosureForm, SubmissionFacts, SubmitOutcome, SubmitScreen};

/// The four relationship tokens the rules allow, with the label each reads as.
const RELATIONSHIPS: [(&str, &str); 4] = [
    ("independent", "independent"),
    ("vendor", "vendor"),
    ("integrator", "integrator"),
    ("maintainer", "maintainer"),
];

/// The submission surface.
#[expect(
    clippy::must_use_candidate,
    reason = "a Leptos component is mounted by the framework, never consumed as a value"
)]
#[component]
pub fn Submit() -> impl IntoView {
    let screen = Resource::new(|| (), |()| fetch_submission());
    // The inline bar sits BESIDE the toast, never instead of it: a transient
    // success with a silent failure below the fold reads as "nothing
    // happened".
    let note = RwSignal::new(None::<Result<String, String>>);
    let opened = RwSignal::new(None::<SubmitOutcome>);
    let running = RwSignal::new(false);
    // The sanctioned dispatch-continuation shape: the click is the user event,
    // and the answer lands in the action's own async block.
    let submit = Action::new(move |form: &DisclosureForm| {
        let form = form.clone();
        async move {
            running.set(true);
            match open_submission(form).await {
                Ok(outcome) => {
                    let body = format!(
                        "{} opened on {} as pull request #{}.",
                        outcome.entry_id, outcome.branch, outcome.pull_request
                    );
                    toast::success("Submission opened", &body);
                    note.set(Some(Ok(body)));
                    opened.set(Some(outcome));
                }
                Err(e) => {
                    let body = e.to_string();
                    toast::error("The submission was refused", &body);
                    note.set(Some(Err(body)));
                }
            }
            running.set(false);
        }
    });

    view! {
        <Title text="Submit · Run · Veredictum console" />
        <PageHeader
            title="Submit to the registry"
            subtitle="A finished run becomes a published record by being committed to the registry. The instrument opens the pull request; CI recomputes the judgement from the recorded exchanges and signs it."
            crumbs=vec![Crumb::new("Run", "/run/connect")]
        >
            <div class="flex items-center gap-1">{steps("submit")}</div>
        </PageHeader>
        <Transition fallback=|| {
            view! { <p class="text-sm text-ink-muted">"Reading the finished run…"</p> }
        }>
            {move || Suspend::new(async move {
                match screen.await {
                    Ok(state) => state_view(state, submit, running, opened).into_any(),
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
    }
}

/// Where the section stands, one erased branch per state.
fn state_view(
    state: SubmitScreen,
    submit: Action<DisclosureForm, ()>,
    running: RwSignal<bool>,
    opened: RwSignal<Option<SubmitOutcome>>,
) -> impl IntoView + use<> {
    match state {
        SubmitScreen::Ready(facts) => ready_view(&facts, submit, running, opened).into_any(),
        SubmitScreen::NotConfigured { missing } => not_configured_view(&missing).into_any(),
        SubmitScreen::NoRun => {
            view! {
                <EmptyState
                    icon=icondata_lu::LuHourglass
                    message="No finished run yet"
                    hint="A submission publishes a run that finished here."
                >
                    <A href="/run/connect" attr:class=BTN_SECONDARY>
                        "Grade a server"
                    </A>
                </EmptyState>
            }
                .into_any()
        }
        SubmitScreen::NoStatement => {
            view! {
                <EmptyState
                    icon=icondata_lu::LuFileQuestion
                    message="The run was driven without a statement"
                    hint="A registry entry publishes a verdict about a claim, and no claim was made: pick a statement at the Scope step and run again."
                >
                    <A href="/run/scope" attr:class=BTN_SECONDARY>
                        "Back to Scope"
                    </A>
                </EmptyState>
            }
                .into_any()
        }
        SubmitScreen::NoTranscript => {
            view! {
                <EmptyState
                    icon=icondata_lu::LuFileQuestion
                    message="The run recorded no wire exchanges"
                    hint="A registry entry from this instrument is only worth reading because CI recomputes its judgement from the recorded exchanges. Switch exchange recording on at the Scope step and run again."
                >
                    <A href="/run/scope" attr:class=BTN_SECONDARY>
                        "Back to Scope"
                    </A>
                </EmptyState>
            }
                .into_any()
        }
    }
}

/// The unconfigured posture: what to set, and no button at all.
fn not_configured_view(missing: &[String]) -> impl IntoView + use<> {
    let names = missing.join(", ");
    view! {
        <div class=WELL>
            <p class="text-sm text-ink">
                "This instrument carries no registry identity, so nothing here can be submitted."
            </p>
            <p class="mt-1 text-sm text-ink-muted">
                "A submission is opened by a GitHub App whose installation token is short-lived and revocable, and that identity is the only one permitted to open an official console entry. Configure it: "
                <span class="font-mono text-xs">{names}</span>
                ". The App key stays a file the instrument reads at the moment it mints a token; it reaches no page, no log line and no artifact."
            </p>
        </div>
    }
}

/// One labelled text input bound to a signal.
fn text_field(
    id: &'static str,
    label: &'static str,
    placeholder: &'static str,
    value: RwSignal<String>,
) -> impl IntoView + use<> {
    view! {
        <div>
            <label class=LABEL for=id>
                {label}
            </label>
            <input
                id=id
                type="text"
                class=format!("{INPUT} mt-1 w-full")
                placeholder=placeholder
                prop:value=move || value.get()
                on:input:target=move |ev| value.set(ev.target().value())
            />
        </div>
    }
}

/// One labelled textarea bound to a signal.
fn text_area(
    id: &'static str,
    label: &'static str,
    placeholder: &'static str,
    value: RwSignal<String>,
) -> impl IntoView + use<> {
    view! {
        <div>
            <label class=LABEL for=id>
                {label}
            </label>
            <textarea
                id=id
                rows="3"
                class=format!("{TEXTAREA} mt-1")
                placeholder=placeholder
                prop:value=move || value.get()
                on:input:target=move |ev| value.set(ev.target().value())
            >
                {value.get()}
            </textarea>
        </div>
    }
}

/// What the run already knows, stated rather than asked for.
fn facts_view(facts: &SubmissionFacts) -> impl IntoView + use<> {
    let rows = [
        ("Run", facts.run_id.clone()),
        ("Entry id", facts.entry_id.clone()),
        ("Branch", facts.branch.clone()),
        ("Registry", facts.repo.clone()),
        ("Endpoint driven", facts.endpoint.clone()),
        ("Run started", facts.run_started_at.clone()),
        ("Instrument", facts.instrument_version.clone()),
        ("Catalogue revision", facts.catalogue_revision.clone()),
    ]
    .into_iter()
    .map(|(name, value)| {
        view! {
            <div>
                <dt class="text-ink-muted">{name}</dt>
                <dd class="break-all font-mono text-xs text-ink">{value}</dd>
            </div>
        }
    })
    .collect_view();
    let files = facts
        .files
        .iter()
        .map(|path| {
            view! { <li class="break-all font-mono text-xs text-ink">{path.clone()}</li> }
        })
        .collect_view();
    view! {
        <div class=WELL>
            <h3 class="text-xs font-medium uppercase tracking-wide text-ink-muted">
                "What the run knows"
            </h3>
            <dl class="mt-2 grid grid-cols-1 gap-2 text-sm sm:grid-cols-2">{rows}</dl>
            <h3 class="mt-3 text-xs font-medium uppercase tracking-wide text-ink-muted">
                "What the submission adds"
            </h3>
            <ul class="mt-1 space-y-0.5">{files}</ul>
            <p class="mt-2 text-sm text-ink-muted">
                "The entry carries no provenance block. A performer does not state its own provenance, so CI writes that block after it has recomputed the verdicts from the submitted transcript and signed the record with a key this instrument never holds."
            </p>
        </div>
    }
}

/// The opened submission: where it went.
fn opened_view(outcome: &SubmitOutcome) -> impl IntoView + use<> {
    view! {
        <div class=WELL>
            <h3 class="text-xs font-medium uppercase tracking-wide text-ink-muted">"Submitted"</h3>
            <p class="mt-2 text-sm text-ink">
                "Entry "<span class="font-mono text-xs">{outcome.entry_id.clone()}</span>
                " arrived on "<span class="font-mono text-xs">{outcome.branch.clone()}</span>"."
            </p>
            <a
                href=outcome.pull_request_url.clone()
                rel="external noopener"
                target="_blank"
                class=format!("{BTN_SECONDARY} mt-2")
            >
                {format!("Open pull request #{}", outcome.pull_request)}
            </a>
        </div>
    }
}

/// Every control the disclosure form is made of.
///
/// One value per field the submission rules name, so the sections below take
/// the whole set rather than a parameter list that has to be kept in step with
/// it in three places.
#[derive(Debug, Clone, Copy)]
struct Fields {
    /// Who is publishing.
    submitter_name: RwSignal<String>,
    /// Where the entry can be discussed.
    submitter_contact: RwSignal<String>,
    /// What the submitter is to the system.
    relationship: RwSignal<String>,
    /// The lowercase system id.
    system: RwSignal<String>,
    /// The name a board prints.
    display_name: RwSignal<String>,
    /// The version measured.
    version: RwSignal<String>,
    /// Whether a reproduction run is authorized.
    authorized: RwSignal<String>,
    /// The operating system the graded server runs on.
    os: RwSignal<String>,
    /// Its architecture.
    arch: RwSignal<String>,
    /// How the submitter describes the host.
    host_class: RwSignal<String>,
    /// The CPU model, when the platform discloses one.
    cpu_model: RwSignal<String>,
    /// Cores available, when the platform discloses them.
    cores: RwSignal<String>,
    /// Memory in bytes, when the platform discloses it.
    memory_bytes: RwSignal<String>,
    /// What was switched on behind the result.
    sut_configuration: RwSignal<String>,
    /// Any interest the submitter holds in the outcome.
    conflict_of_interest: RwSignal<String>,
}

impl Fields {
    /// The controls, seeded with what the run already recorded.
    fn seeded(facts: &SubmissionFacts) -> Self {
        Self {
            submitter_name: RwSignal::new(String::new()),
            submitter_contact: RwSignal::new(String::new()),
            relationship: RwSignal::new(String::from("independent")),
            system: RwSignal::new(facts.system.clone()),
            display_name: RwSignal::new(facts.display_name.clone()),
            version: RwSignal::new(facts.version.clone()),
            authorized: RwSignal::new(String::from("no")),
            os: RwSignal::new(String::new()),
            arch: RwSignal::new(String::new()),
            host_class: RwSignal::new(String::new()),
            cpu_model: RwSignal::new(String::new()),
            cores: RwSignal::new(String::new()),
            memory_bytes: RwSignal::new(String::new()),
            sut_configuration: RwSignal::new(String::new()),
            conflict_of_interest: RwSignal::new(String::new()),
        }
    }

    /// The disclosure as the form stands, for one dispatch.
    fn read(self) -> DisclosureForm {
        DisclosureForm {
            submitter_name: self.submitter_name.get(),
            submitter_contact: self.submitter_contact.get(),
            relationship: self.relationship.get(),
            system: self.system.get(),
            display_name: self.display_name.get(),
            version: self.version.get(),
            reproduction_authorized: self.authorized.get(),
            environment_os: self.os.get(),
            environment_arch: self.arch.get(),
            environment_host_class: self.host_class.get(),
            environment_cpu_model: self.cpu_model.get(),
            environment_cores: self.cores.get(),
            environment_memory_bytes: self.memory_bytes.get(),
            sut_configuration: self.sut_configuration.get(),
            conflict_of_interest: self.conflict_of_interest.get(),
        }
    }
}

/// Who is publishing, and what they are to the system.
fn submitter_section(fields: Fields) -> impl IntoView + use<> {
    let relationship = fields.relationship;
    let options = RELATIONSHIPS
        .into_iter()
        .map(|(token, label)| {
            view! { <option value=token>{label}</option> }
        })
        .collect_view();
    view! {
        <section class=CARD_PAD>
            <h2 class=CARD_TITLE>"Who is publishing"</h2>
            <div class="space-y-3">
                {text_field(
                    "submitter-name",
                    "Name",
                    "The person or organization publishing this entry",
                    fields.submitter_name,
                )}
                {text_field(
                    "submitter-contact",
                    "Contact",
                    "A URL or mailto: address the entry can be discussed at",
                    fields.submitter_contact,
                )} <div>
                    <label class=LABEL for="relationship">
                        "Relationship to the system"
                    </label>
                    <select
                        id="relationship"
                        class=format!("{SELECT} mt-1 w-full")
                        prop:value=move || relationship.get()
                        on:change:target=move |ev| relationship.set(ev.target().value())
                    >
                        {options}
                    </select>
                </div>
            </div>
        </section>
    }
}

/// What was measured, and whether this repository may drive it again.
fn subject_section(fields: Fields) -> impl IntoView + use<> {
    let authorized = fields.authorized;
    view! {
        <section class=CARD_PAD>
            <h2 class=CARD_TITLE>"What was measured"</h2>
            <div class="space-y-3">
                {text_field("system", "System id", "lowercase, hyphen separated", fields.system)}
                {text_field(
                    "display-name",
                    "Display name",
                    "The name a board prints",
                    fields.display_name,
                )}
                {text_field("version", "Version", "The version that was measured", fields.version)}
                <div>
                    <label class=LABEL for="reproduction-authorized">
                        "May this repository drive that deployment again?"
                    </label>
                    <select
                        id="reproduction-authorized"
                        class=format!("{SELECT} mt-1 w-full")
                        prop:value=move || authorized.get()
                        on:change:target=move |ev| authorized.set(ev.target().value())
                    >
                        <option value="no">"no"</option>
                        <option value="yes">
                            "yes — a reproduction run may create and delete data"
                        </option>
                    </select>
                </div>
            </div>
        </section>
    }
}

/// The machine and the configuration only the submitter knows.
fn environment_section(fields: Fields) -> impl IntoView + use<> {
    view! {
        <section class=CARD_PAD>
            <h2 class=CARD_TITLE>"The machine the graded server runs on"</h2>
            <p class="mb-3 text-sm text-ink-muted">
                "The instrument drove an endpoint you named, so it measured what that server answered and knows nothing about the host behind it. Only you can say."
            </p>
            <div class="space-y-3">
                <div class="grid grid-cols-1 gap-3 sm:grid-cols-3">
                    {text_field("environment-os", "Operating system", "Linux 6.8", fields.os)}
                    {text_field("environment-arch", "Architecture", "x86_64", fields.arch)}
                    {text_field(
                        "environment-host-class",
                        "Host",
                        "8 vCPU cloud VM",
                        fields.host_class,
                    )}
                </div>
                <div class="grid grid-cols-1 gap-3 sm:grid-cols-3">
                    {text_field(
                        "environment-cpu-model",
                        "CPU model (optional)",
                        "",
                        fields.cpu_model,
                    )} {text_field("environment-cores", "Cores (optional)", "8", fields.cores)}
                    {text_field(
                        "environment-memory-bytes",
                        "Memory in bytes (optional)",
                        "17179869184",
                        fields.memory_bytes,
                    )}
                </div>
                {text_area(
                    "sut-configuration",
                    "What was switched on behind the result",
                    "Authentication, validation depth, signing, audit, tenancy",
                    fields.sut_configuration,
                )}
                {text_area(
                    "conflict-of-interest",
                    "Any interest you hold in the outcome",
                    "There is no \"not applicable\": write the sentence that is true.",
                    fields.conflict_of_interest,
                )}
            </div>
        </section>
    }
}

/// The ready state: the facts, the disclosure the submitter owes, the button.
fn ready_view(
    facts: &SubmissionFacts,
    submit: Action<DisclosureForm, ()>,
    running: RwSignal<bool>,
    opened: RwSignal<Option<SubmitOutcome>>,
) -> impl IntoView + use<> {
    let fields = Fields::seeded(facts);
    let dispatch = move |_| {
        submit.dispatch(fields.read());
    };
    let facts_panel = facts_view(facts).into_any();
    let who = submitter_section(fields).into_any();
    let subject = subject_section(fields).into_any();
    let environment = environment_section(fields).into_any();
    view! {
        <div class="space-y-4">
            {facts_panel} <div class="grid grid-cols-1 gap-4 lg:grid-cols-2">{who} {subject}</div>
            {environment} <div class="flex items-center gap-3">
                <button
                    type="button"
                    class=BTN_PRIMARY
                    prop:disabled=move || running.get()
                    on:click=dispatch
                >
                    {move || if running.get() { "Opening…" } else { "Open the submission" }}
                </button>
                <span class="text-sm text-ink-muted">
                    "Every field the rules make mandatory is checked here first: an empty one is refused by name, before a single request leaves this process."
                </span>
            </div> {move || opened.get().map(|outcome| opened_view(&outcome))}
        </div>
    }
}
