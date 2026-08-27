// SPDX-FileCopyrightText: Veredictum contributors
// SPDX-License-Identifier: Apache-2.0

//! The console's integration suite: one binary, one module per topic
//! (`.claude/rules/testing.md` §Where tests live).

#[cfg(feature = "ssr")]
mod engine_gate;
#[cfg(feature = "ssr")]
mod read_surfaces;
#[cfg(feature = "ssr")]
mod run_scope;
