// SPDX-FileCopyrightText: Veredictum contributors
// SPDX-License-Identifier: Apache-2.0

//! The card surface classes: one look — token colors, hairline border, the
//! single soft shadow level. Every panel a screen builds starts here, so
//! elevation and borders cannot drift per screen.

use leptos::prelude::{AnyView, ClassAttribute, ElementChild, IntoAny, view};

/// The card surface (no padding — content decides).
pub const CARD: &str = "rounded-card border border-edge bg-raised shadow-card";

/// The card surface with the standard padding.
pub const CARD_PAD: &str = "rounded-card border border-edge bg-raised shadow-card p-4";

/// A sunken well (code panes, read-only documents).
pub const WELL: &str = "rounded-card border border-edge bg-sunken p-3";

/// The standard section heading inside a card.
pub const CARD_TITLE: &str = "text-sm font-semibold text-ink-heading mb-3";

/// A titled panel card wrapping an already-erased body: the uniform section
/// every panel screen builds from. `full_width` spans both columns of a
/// two-column grid.
#[must_use]
pub fn titled_card(title: &'static str, full_width: bool, body: AnyView) -> AnyView {
    let class = if full_width {
        format!("{CARD_PAD} lg:col-span-2")
    } else {
        CARD_PAD.to_owned()
    };
    view! {
        <section class=class>
            <h2 class=CARD_TITLE>{title}</h2>
            {body}
        </section>
    }
    .into_any()
}
