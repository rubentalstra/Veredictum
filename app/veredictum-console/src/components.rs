// SPDX-FileCopyrightText: Veredictum contributors
// SPDX-License-Identifier: Apache-2.0

//! The shared component kits (#63): one kit per repeated surface.
//!
//! A surface never gets a second, slightly different realization. The shapes
//! are the FerroEHR console's proven kits, re-grounded on the Veredictum
//! design tokens (`style/tailwind.css`).

pub mod data_table;
pub mod empty_state;
pub mod field;
pub mod format_view;
pub mod page_header;
pub mod stat_card;
pub mod surface;
pub mod toast;
