// SPDX-FileCopyrightText: Veredictum contributors
// SPDX-License-Identifier: Apache-2.0

//! The shared component kits (#63): one kit per repeated surface.
//!
//! A surface never gets a second, slightly different realization. Every kit
//! draws its colors from the design tokens in `style/tailwind.css`.

pub mod data_table;
pub mod empty_state;
pub mod field;
pub mod format_view;
pub mod page_header;
pub mod stat_card;
pub mod surface;
pub mod toast;
