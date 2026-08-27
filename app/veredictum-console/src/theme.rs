// SPDX-FileCopyrightText: Veredictum contributors
// SPDX-License-Identifier: Apache-2.0

//! Dark mode: the `dark` class on `<html>` drives every design token
//! (`style/tailwind.css`), and the choice persists to `localStorage`.
//!
//! Browser-only by construction: the persisted choice is re-applied after
//! hydration inside an `Effect`, keeping the initial render deterministic:
//! a server pass and a client pass that disagree are a hydration mismatch
//! (<https://book.leptos.dev/ssr/24_hydration_bugs.html>). The first paint is
//! light, and a dark preference flips within the hydration frame.

use leptos::prelude::document;

/// The `localStorage` key the preference persists under.
const STORAGE_KEY: &str = "veredictum-console-theme";

/// Applies dark mode to the document root. Browser-only callers (an
/// `Effect`, a click handler).
pub fn apply_dark(dark: bool) {
    if let Some(root) = document().document_element() {
        let list = root.class_list();
        let outcome = if dark {
            list.add_1("dark")
        } else {
            list.remove_1("dark")
        };
        // NOTE: no openEHR spec governs this — our own design; a classList
        // mutation has nothing to recover from.
        drop(outcome);
    }
}

/// Reads the persisted preference; `None` means never chosen (stay light).
#[must_use]
pub fn stored_dark() -> Option<bool> {
    let storage = leptos::web_sys::window()?.local_storage().ok()??;
    let value = storage.get_item(STORAGE_KEY).ok()??;
    Some(value == "dark")
}

/// Persists the preference. Browser-only callers.
pub fn persist_dark(dark: bool) {
    if let Some(storage) = leptos::web_sys::window().and_then(|w| w.local_storage().ok().flatten())
    {
        let outcome = storage.set_item(STORAGE_KEY, if dark { "dark" } else { "light" });
        // NOTE: no openEHR spec governs this — our own design; a full or
        // blocked storage loses only a convenience.
        drop(outcome);
    }
}
