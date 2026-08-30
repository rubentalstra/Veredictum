// SPDX-FileCopyrightText: Veredictum contributors
// SPDX-License-Identifier: Apache-2.0

//! S9 — the public record verification surface (#61, #68), over the engine's
//! signed-record machinery (#62).
//!
//! Public by design: no run, no CDR, no login. Upload a bundle, and the
//! published lib recomputes every digest the manifest names and checks the
//! detached signature over it. The honesty box renders on EVERY outcome, and
//! the command-line equivalent is printed beside it so nobody has to trust
//! the console to check the console.
//!
//! The upload is a plain `<form method="post" enctype="multipart/form-data">`
//! posting to a server-owned axum route, which redirects back here: zero
//! JavaScript, working before the WASM bundle has loaded.

use leptos::prelude::{
    ClassAttribute, CollectView, ElementChild, Get, GlobalAttributes, IntoAny, IntoView, Memo,
    Resource, Suspend, Transition, With, component, view,
};
use leptos_meta::Title;
use leptos_router::hooks::use_query_map;

use crate::components::data_table::{TABLE, TABLE_WRAP, TD, TH};
use crate::components::format_view::inline_error;
use crate::components::page_header::PageHeader;
use crate::components::surface::{CARD_PAD, CARD_TITLE, WELL};
use crate::verify_api::fns::fetch_verification;
use crate::verify_api::{
    BundleView, HONESTY_BOUNDS, HONESTY_LINE, NO_KEY_HINT, UPLOAD_PATH, VerifyScreen,
};

/// The bundle id and the refusal reason the upload route redirects with.
///
/// A helper rather than the component body: reading the query map inside a
/// `#[component]` fn silences `clippy::must_use_candidate` there, turning the
/// crate's `#[expect]` idiom into an unfulfilled-expectation build failure.
fn params_from_url() -> Memo<(Option<String>, Option<String>)> {
    let query = use_query_map();
    Memo::new(move |_| query.with(|map| (map.get("bundle"), map.get("refused"))))
}

/// The record verification surface.
#[expect(
    clippy::must_use_candidate,
    reason = "a Leptos component is mounted by the framework, never consumed as a value"
)]
#[component]
pub fn Verify() -> impl IntoView {
    let params = params_from_url();
    // The URL is the state (rules §9): the checked bundle is a query
    // parameter, so the outcome is shareable, refresh-safe and needs no WASM.
    let screen = Resource::new(
        move || params.get().0,
        |bundle| async move { fetch_verification(bundle).await },
    );

    view! {
        <Title text="Verify · Veredictum console" />
        <PageHeader
            title="Verify a record"
            subtitle="Recompute every digest a bundle's manifest names, and check the detached signature over that manifest. No run, no server, no account."
        />
        <div class="grid grid-cols-1 gap-4 lg:grid-cols-3">
            <div class="lg:col-span-2">
                {upload_form()}
                <Transition fallback=|| {
                    view! { <p class="mt-4 text-sm text-ink-muted">"Checking the bundle…"</p> }
                }>
                    {move || Suspend::new(async move {
                        let refused = params.get().1;
                        match screen.await {
                            Ok(screen) => outcome_view(screen, refused).into_any(),
                            Err(e) => inline_error(&e.to_string()).into_any(),
                        }
                    })}
                </Transition>
            </div>
            <div>{honesty_box()}</div>
        </div>
    }
}

/// The upload control: one plain form, no script anywhere near it.
fn upload_form() -> impl IntoView {
    view! {
        <section class=CARD_PAD>
            <h2 class=CARD_TITLE>"Check a bundle"</h2>
            <p class="mb-3 text-sm text-ink-muted">
                "Upload the archive a party published: the rendered documents, "
                <span class="font-mono text-xs">"record-manifest.json"</span>
                " and its detached signature. At most 16 MiB, and one flat directory of files."
            </p>
            // A server-owned route, so the form posts straight to it. No
            // `on:submit`, no FileReader, no script: the browser's own
            // multipart encoding is the upload mechanism.
            <form
                method="post"
                action=UPLOAD_PATH
                enctype="multipart/form-data"
                class="flex flex-wrap items-center gap-2"
            >
                <input
                    type="file"
                    id="bundle"
                    name="bundle"
                    accept=".zip,application/zip"
                    required
                    class="text-sm text-ink file:mr-3 file:rounded-control file:border file:border-edge-strong file:bg-raised file:px-3 file:py-1.5 file:text-sm file:font-medium file:text-ink hover:file:bg-sunken"
                />
                <button type="submit" class=crate::components::field::BTN_PRIMARY>
                    "Verify the bundle"
                </button>
            </form>
        </section>
    }
}

/// The honesty box — rendered on EVERY outcome, including none.
///
/// A signature that is overread is worse than no signature, so what
/// verification does NOT establish is permanent page furniture rather than a
/// footnote under a failure.
fn honesty_box() -> impl IntoView {
    let bounds = HONESTY_BOUNDS
        .iter()
        .map(|bound| {
            view! { <li>{*bound}</li> }
        })
        .collect_view();
    view! {
        <section class=format!("{CARD_PAD} border-warn/40")>
            <h2 class=CARD_TITLE>"What this proves"</h2>
            <p class="text-sm text-ink">{HONESTY_LINE}</p>
            <ul class="mt-2 list-disc space-y-1 pl-5 text-sm text-ink-muted">{bounds}</ul>
        </section>
    }
}

/// One outcome, plus the upload route's own refusal when it redirected with
/// one.
fn outcome_view(screen: VerifyScreen, refused: Option<String>) -> impl IntoView + use<> {
    let banner = refused.map(|reason| {
        view! {
            <div
                role="alert"
                class="mt-4 rounded-control border border-danger/40 bg-danger-subtle px-3 py-2 text-sm text-ink"
            >
                <span class="font-medium">"The upload was refused: "</span>
                {reason}
            </div>
        }
    });
    let body = match screen {
        VerifyScreen::Checked(view) => checked_view(*view).into_any(),
        VerifyScreen::Refused { reason } => {
            view! {
                <div
                    role="alert"
                    class="mt-4 rounded-control border border-danger/40 bg-danger-subtle px-3 py-2 text-sm text-ink"
                >
                    {reason}
                </div>
            }
                .into_any()
        }
        VerifyScreen::NoKey => {
            view! {
                <div class=format!("{WELL} mt-4")>
                    <p class="text-sm text-ink">
                        "No public key is mounted, so this console cannot check anything."
                    </p>
                    <p class="mt-1 text-sm text-ink-muted">{NO_KEY_HINT}</p>
                </div>
            }
                .into_any()
        }
        VerifyScreen::Idle => {
            view! {
                <p class="mt-4 text-sm text-ink-muted">
                    "Nothing checked yet. The result appears here, and its address carries the bundle so you can share exactly what you saw."
                </p>
            }
                .into_any()
        }
    };
    view! {
        {banner}
        {body}
    }
}

/// A checked bundle: the origin claim, then every file the manifest names.
fn checked_view(view: BundleView) -> impl IntoView + use<> {
    let verdict_class = if view.is_clean {
        "rounded-control bg-ok-subtle px-2 py-1 text-sm font-medium text-ink"
    } else {
        "rounded-control bg-danger-subtle px-2 py-1 text-sm font-medium text-ink"
    };
    let verdict_text = if view.is_clean {
        "The bundle verifies: the signature is good and every file reproduces its digest."
    } else {
        "The bundle does NOT verify."
    };
    let signature_row = if view.signature_accepted {
        format!(
            "signed by {} at {}",
            view.fingerprint.as_deref().unwrap_or("an unnamed key"),
            view.signed_at.as_deref().unwrap_or("an unstated time")
        )
    } else {
        String::from("the detached signature does not verify against the mounted public key")
    };
    let findings = view
        .findings
        .iter()
        .map(|finding| {
            view! { <li class="font-mono text-xs">{finding.clone()}</li> }
        })
        .collect_view();
    let findings_block = if view.findings.is_empty() {
        None
    } else {
        Some(view! {
            <div class="mt-3 rounded-control border border-danger/40 bg-danger-subtle px-3 py-2">
                <p class="text-sm font-medium text-ink">"Findings"</p>
                <ul class="mt-1 list-disc space-y-0.5 pl-5 text-ink">{findings}</ul>
            </div>
        })
    };
    // The key is the file name: a manifest never carries the same name twice
    // (the lib refuses a duplicate), so it is stable and data-derived —
    // never an index (rules §4).
    let rows = view
        .files
        .into_iter()
        .map(|file| {
            let chip = match file.outcome.as_str() {
                "matched" => "rounded-control bg-ok-subtle px-1.5 py-0.5 text-xs text-ink",
                _ => "rounded-control bg-danger-subtle px-1.5 py-0.5 text-xs font-medium text-ink",
            };
            view! {
                <tr>
                    <td class=TD>
                        <span class="font-mono text-xs">{file.name}</span>
                    </td>
                    <td class=TD>
                        <span class=chip>{file.outcome}</span>
                    </td>
                    <td class=TD>
                        <span class="break-all font-mono text-xs text-ink-muted">
                            {file.detail.unwrap_or(file.digest)}
                        </span>
                    </td>
                </tr>
            }
        })
        .collect_view();

    view! {
        <section class=format!("{CARD_PAD} mt-4")>
            <h2 class=CARD_TITLE>"The check"</h2>
            <p class=verdict_class>{verdict_text}</p>
            <dl class="mt-3 space-y-1 text-sm">
                <dt class="text-ink-muted">"Origin"</dt>
                <dd class="break-all font-mono text-xs text-ink">{signature_row}</dd>
                <dt class="text-ink-muted">"Instrument"</dt>
                <dd class="font-mono text-xs text-ink">{view.instrument}</dd>
            </dl>
            {findings_block}
            <h3 class="mt-4 text-xs font-medium uppercase tracking-wide text-ink-muted">
                "Every file the manifest names"
            </h3>
            <div class=format!("{TABLE_WRAP} mt-1")>
                <table class=TABLE>
                    <thead>
                        <tr>
                            <th class=TH>"File"</th>
                            <th class=TH>"Digest"</th>
                            <th class=TH>"Detail"</th>
                        </tr>
                    </thead>
                    <tbody>{rows}</tbody>
                </table>
            </div>
        </section>
    }
}
