// SPDX-FileCopyrightText: Veredictum contributors
// SPDX-License-Identifier: Apache-2.0

//! The shared form-control classes: ONE styled definition for inputs,
//! selects, textareas and buttons.
//!
//! Class constants rather than wrapper components on purpose: the kit
//! standardizes the LOOK, each screen keeps its own behaviour — controlled or
//! uncontrolled (<https://book.leptos.dev/view/05_forms.html>).

/// A single-line text input.
pub const INPUT: &str = "rounded-control border border-edge-strong bg-raised px-3 py-1.5 text-sm text-ink placeholder:text-ink-faint focus:outline-none focus:ring-2 focus:ring-accent focus:border-accent disabled:opacity-50 disabled:pointer-events-none";

/// A `<select>` control.
pub const SELECT: &str = "rounded-control border border-edge-strong bg-raised px-2 py-1.5 text-sm text-ink focus:outline-none focus:ring-2 focus:ring-accent focus:border-accent";

/// A multi-line `<textarea>`.
pub const TEXTAREA: &str = "w-full rounded-control border border-edge-strong bg-raised px-3 py-2 font-mono text-xs text-ink placeholder:text-ink-faint focus:outline-none focus:ring-2 focus:ring-accent focus:border-accent disabled:opacity-50 disabled:pointer-events-none";

/// A form label.
pub const LABEL: &str = "text-sm font-medium text-ink";

/// The primary (solid accent) button.
pub const BTN_PRIMARY: &str = "inline-flex items-center gap-1.5 rounded-control bg-accent px-3 py-1.5 text-sm font-medium text-on-accent hover:bg-accent-hover focus:outline-none focus:ring-2 focus:ring-accent focus:ring-offset-1 disabled:opacity-50 disabled:pointer-events-none";

/// The secondary (outlined) button.
pub const BTN_SECONDARY: &str = "inline-flex items-center gap-1.5 rounded-control border border-edge-strong bg-raised px-3 py-1.5 text-sm font-medium text-ink hover:bg-sunken focus:outline-none focus:ring-2 focus:ring-accent disabled:opacity-50 disabled:pointer-events-none";

/// The quiet/destructive text button (cancel-run, two-step confirms).
pub const BTN_DANGER: &str = "inline-flex items-center gap-1.5 rounded-control border border-danger/40 px-3 py-1.5 text-sm font-medium text-danger hover:bg-danger-subtle focus:outline-none focus:ring-2 focus:ring-danger disabled:opacity-50 disabled:pointer-events-none";
