// SPDX-FileCopyrightText: Veredictum contributors
// SPDX-License-Identifier: Apache-2.0

//! The listing-table kit: the ONE table shell every listing screen uses.
//!
//! Table classes, the loading skeleton, the page size and the pagination
//! footer. Page state lives in the URL (`?page=`), so a listing is shareable,
//! refresh-safe and works without WASM loaded
//! (<https://book.leptos.dev/router/20_form.html>).

use leptos::prelude::{
    AddAnyAttr, ClassAttribute, CollectView, ElementChild, IntoView, Memo, StyleAttribute, With,
    component, view,
};
use leptos_router::components::A;
use leptos_router::hooks::use_query_map;

/// The console-wide page size.
pub const PAGE_SIZE: usize = 25;

/// The table wrapper (horizontal scroll containment).
pub const TABLE_WRAP: &str =
    "overflow-x-auto rounded-card border border-edge bg-raised shadow-card";

/// The `<table>` element.
pub const TABLE: &str = "w-full text-sm";

/// A header row cell.
pub const TH: &str =
    "bg-sunken px-3 py-2 text-left text-xs font-semibold uppercase tracking-wide text-ink-muted";

/// A body row cell.
pub const TD: &str = "border-t border-edge px-3 py-2 text-ink";

/// Reads the 1-based page number from the URL's `?page=`.
///
/// A helper on purpose: `use_query_map()` called directly in a `#[component]`
/// body stops `clippy::must_use_candidate` firing on that fn, turning the
/// component's own `#[expect]` of it into an `unfulfilled_lint_expectations`
/// build failure.
#[must_use]
pub fn page_from_url() -> Memo<usize> {
    let query = use_query_map();
    Memo::new(move |_| {
        query.with(|q| {
            q.get("page")
                .and_then(|p| p.parse::<usize>().ok())
                .filter(|&p| p >= 1)
                .unwrap_or(1)
        })
    })
}

/// How many pages `total` rows occupy at [`PAGE_SIZE`].
#[must_use]
pub fn page_count(total: usize) -> usize {
    total.div_ceil(PAGE_SIZE).max(1)
}

/// The half-open row range page `page` (1-based, clamped) shows of `total`.
#[must_use]
pub fn page_window(page: usize, total: usize) -> (usize, usize) {
    let page = page.clamp(1, page_count(total));
    let start = (page - 1).saturating_mul(PAGE_SIZE);
    (start.min(total), start.saturating_add(PAGE_SIZE).min(total))
}

/// The loading skeleton: shimmering placeholder rows in the table shell,
/// never a blank region or a spinner-only pane.
#[must_use]
pub fn table_skeleton(columns: usize) -> impl IntoView {
    let rows = (0..5_usize)
        .map(|row| {
            let cells = (0..columns)
                .map(|column| {
                    view! {
                        <td class=TD>
                            <div
                                class="h-4 animate-pulse rounded bg-sunken"
                                style=format!("width: {}%", 90 - ((row + column) % 3) * 20)
                            ></div>
                        </td>
                    }
                })
                .collect_view();
            view! { <tr>{cells}</tr> }
        })
        .collect_view();
    view! { <tbody>{rows}</tbody> }
}

/// The pagination footer: the visible window, and previous/next links that
/// carry the page in the URL. `base` is the listing's path (filters intact
/// is the CALLER's duty when it builds `base` from its own query state).
#[expect(
    clippy::must_use_candidate,
    reason = "a Leptos component is mounted by the framework, never consumed as a value"
)]
#[component]
pub fn TableFooter(
    /// The listing's path the page links target, without `?page=`.
    #[prop(into)]
    base: String,
    /// The current 1-based page.
    page: usize,
    /// The total row count.
    total: usize,
) -> impl IntoView {
    let (start, end) = page_window(page, total);
    let pages = page_count(total);
    let previous_href = format!("{base}?page={}", page.saturating_sub(1));
    let next_href = format!("{base}?page={}", page + 1);
    // Both hrefs are built, so the by-value prop is spent here.
    drop(base);
    let previous = (page > 1).then(|| {
        let href = previous_href;
        view! {
            <A href=href attr:class="text-accent hover:text-accent-hover hover:underline">
                "‹ previous"
            </A>
        }
    });
    let next = (page < pages).then(|| {
        let href = next_href;
        view! {
            <A href=href attr:class="text-accent hover:text-accent-hover hover:underline">
                "next ›"
            </A>
        }
    });
    view! {
        <div class="flex items-center justify-between border-t border-edge px-3 py-2 text-sm text-ink-muted">
            <span class="tabular-nums">
                {format!("{}–{} of {total}", if total == 0 { 0 } else { start + 1 }, end)}
            </span>
            <span class="flex items-center gap-3">{previous} {next}</span>
        </div>
    }
}

#[cfg(test)]
mod tests {
    use super::{PAGE_SIZE, page_count, page_window};

    #[test]
    fn the_window_math_is_total_and_clamped() {
        assert_eq!(page_window(1, 0), (0, 0));
        assert_eq!(page_count(0), 1);
        assert_eq!(page_window(1, 10), (0, 10));
        assert_eq!(page_window(1, PAGE_SIZE + 1), (0, PAGE_SIZE));
        assert_eq!(page_window(2, PAGE_SIZE + 1), (PAGE_SIZE, PAGE_SIZE + 1));
        // A page past the end clamps, so a stale link still shows data.
        assert_eq!(page_window(99, PAGE_SIZE + 1), (PAGE_SIZE, PAGE_SIZE + 1));
        assert_eq!(page_count(PAGE_SIZE * 3), 3);
    }
}
