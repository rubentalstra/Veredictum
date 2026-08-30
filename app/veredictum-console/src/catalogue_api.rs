// SPDX-FileCopyrightText: Veredictum contributors
// SPDX-License-Identifier: Apache-2.0

//! The read seam for S1 and S2 (#64): typed rows the screens render, served
//! by thin `#[server]` fns over the startup catalogue state.
//!
//! Every number and every text here comes from the published lib's typed
//! model — the console re-parses nothing. Serialized types carry fixed-size
//! integers only: a pointer-sized `usize` crossing the 32-bit WASM to 64-bit
//! server boundary deserializes wrong
//! (<https://book.leptos.dev/server/25_server_functions.html>).

use serde::{Deserialize, Serialize};

/// What the instrument landing shows: the validate summary's own numbers.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct InstrumentSummary {
    /// The loaded case-core count.
    pub cases: u64,
    /// The loaded operation-binding count.
    pub bindings: u64,
    /// The committed party-statement count.
    pub parties: u64,
    /// The validation finding count; zero is the only passing result.
    pub findings: u64,
    /// The artifact root, for display.
    pub root: String,
    /// The vendored spec tree, for display.
    pub specs: String,
}

/// The catalogue-missing explanation: which mount was expected, and the
/// verbatim reason the load refused.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CatalogueMissing {
    /// The artifact root the console looked at.
    pub root: String,
    /// The spec tree the console looked at.
    pub specs: String,
    /// The load error, verbatim.
    pub reason: String,
}

/// Either the summary or the honest absence.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum InstrumentView {
    /// The catalogue loaded; the landing renders the numbers.
    Loaded(InstrumentSummary),
    /// The catalogue did not load; the landing renders the explanation.
    Missing(CatalogueMissing),
}

/// One schedule chapter and its case count.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ChapterRow {
    /// The chapter key: the directory under `schedule/`.
    pub key: String,
    /// How many case cores the chapter carries.
    pub cases: u64,
}

/// One case row in a chapter listing.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CaseRow {
    /// The case id.
    pub id: String,
    /// The executor kind (`functional` / `content`).
    pub kind: String,
    /// The ISO/IEC 9646 test purpose.
    pub purpose: String,
    /// The profile tiers the case carries (CORE / STANDARD / OPTIONS /
    /// SEC-BASIC), the CNF profile model's own vocabulary.
    pub tiers: Vec<String>,
    /// The presentation band within the chapter — the SAME two-level
    /// taxonomy the published chapter-bars SVG renders (`conf_assets`).
    pub band: String,
}

/// One band section of a chapter listing: the published SVG's second level.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BandRows {
    /// The band label from the instrument's own taxonomy.
    pub band: String,
    /// The band's cases, id-sorted.
    pub cases: Vec<CaseRow>,
}

/// One realizing binding in a case detail.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BindingRef {
    /// The binding's source file name.
    pub file: String,
    /// Whether the binding realizes the operation on this ITS or declares it
    /// unrealized.
    pub realized: bool,
}

/// Everything the case-detail screen shows.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CaseDetail {
    /// The case id.
    pub id: String,
    /// The chapter directory the case lives under.
    pub chapter: String,
    /// The executor kind.
    pub kind: String,
    /// The schedule component.
    pub component: String,
    /// The SM operation anchor, when the case has one.
    pub sm_operation: Option<String>,
    /// The ISO/IEC 9646 test purpose.
    pub test_purpose: String,
    /// The schedule's Description row.
    pub description: String,
    /// Every citation, verbatim.
    pub spec_refs: Vec<String>,
    /// The bindings realizing the case's SM operation.
    pub bindings: Vec<BindingRef>,
    /// The corpus keys the case's preconditions and commits reference.
    pub corpus_keys: Vec<String>,
    /// The profile tiers the case carries.
    pub tiers: Vec<String>,
    /// The verdict-bearing capability names (kept minimal by design).
    pub capabilities: Vec<String>,
    /// The informative coverage tags.
    pub exercises: Vec<String>,
    /// The spec-version windows, component and range, verbatim.
    pub applies: Vec<String>,
    /// The prose run conditions, each spec-cited.
    pub guards: Vec<String>,
    /// The case-level format axis.
    pub formats: Vec<String>,
    /// The register option tag realizing an implementation choice, if any.
    pub option: Option<String>,
    /// The flow step count (functional) or decision-table row count
    /// (content).
    pub size: String,
}

#[cfg(feature = "ssr")]
pub mod read {
    //! The ssr-side readers over the startup state — component-free, so the
    //! mapping from the typed model to the rows is plain testable code.

    use super::{
        BandRows, BindingRef, CaseDetail, CaseRow, CatalogueMissing, ChapterRow, InstrumentSummary,
        InstrumentView,
    };
    use crate::state::ConsoleState;

    /// The chapter directory a loaded case file sits under.
    pub fn chapter_of(path: &std::path::Path) -> String {
        let mut components = path.components();
        for component in components.by_ref() {
            if component.as_os_str() == "schedule" {
                break;
            }
        }
        components.next().map_or_else(String::new, |c| {
            c.as_os_str().to_string_lossy().into_owned()
        })
    }

    /// A collection length as the wire's fixed-size integer.
    ///
    /// NOTE: no openEHR spec governs this — our own design; usize → u64 is
    /// lossless on every supported target, so the saturation arm is
    /// unreachable and exists only because `as` is banned.
    fn count(n: usize) -> u64 {
        u64::try_from(n).unwrap_or(u64::MAX)
    }

    /// Maps the startup state to the landing view.
    ///
    /// The counts are the SAME expressions the CLI's validate summary prints
    /// (`app/veredictum/src/bin/veredictum.rs`), so the two cannot disagree.
    #[must_use]
    pub fn instrument_view(state: &ConsoleState) -> InstrumentView {
        match state.catalogue.as_ref() {
            Ok(validation) => InstrumentView::Loaded(InstrumentSummary {
                cases: count(validation.loaded.set.cases.len()),
                bindings: count(validation.loaded.set.bindings.len()),
                parties: count(validation.loaded.set.parties.len()),
                findings: count(validation.findings.len()),
                root: state.root.display().to_string(),
                specs: state.specs.display().to_string(),
            }),
            Err(reason) => InstrumentView::Missing(CatalogueMissing {
                root: state.root.display().to_string(),
                specs: state.specs.display().to_string(),
                reason: reason.clone(),
            }),
        }
    }

    /// The chapters, sorted by key, each with its case count.
    ///
    /// # Errors
    /// The verbatim load failure when the catalogue is absent.
    pub fn chapter_rows(state: &ConsoleState) -> Result<Vec<ChapterRow>, String> {
        let validation = state.catalogue.as_ref().as_ref().map_err(Clone::clone)?;
        let mut counts: std::collections::BTreeMap<String, u64> = std::collections::BTreeMap::new();
        for (path, _case) in &validation.loaded.set.cases {
            *counts.entry(chapter_of(path)).or_insert(0) += 1;
        }
        Ok(counts
            .into_iter()
            .map(|(key, cases)| ChapterRow { key, cases })
            .collect())
    }

    /// The tier tokens a case carries, in vocabulary order: the lib's own
    /// serde names, never a mirrored vocabulary.
    fn tier_tokens(case: &veredictum::model::case::CaseCore) -> Vec<String> {
        case.profiles
            .iter()
            .filter_map(|tier| serde_json::to_string(tier).ok())
            .map(|token| token.trim_matches('"').to_owned())
            .collect()
    }

    /// The chapter's cases grouped by the published SVG's own bands.
    ///
    /// The taxonomy is `conf_assets::band_of` — never a console-side
    /// re-taxonomy. Id-sorted within a band, filtered by the id substring
    /// `q` and the tier token `tier` (empty = every tier).
    ///
    /// # Errors
    /// The verbatim load failure when the catalogue is absent, or the
    /// taxonomy's own refusal for an unmapped id.
    pub fn band_rows(
        state: &ConsoleState,
        chapter: &str,
        q: &str,
        tier: &str,
    ) -> Result<Vec<BandRows>, String> {
        let validation = state.catalogue.as_ref().as_ref().map_err(Clone::clone)?;
        let mut bands: std::collections::BTreeMap<String, Vec<CaseRow>> =
            std::collections::BTreeMap::new();
        for (path, case) in &validation.loaded.set.cases {
            if chapter_of(path) != chapter {
                continue;
            }
            let id = case.id.to_string();
            if !q.is_empty() && !id.contains(q) {
                continue;
            }
            let tiers = tier_tokens(case);
            if !tier.is_empty() && !tiers.iter().any(|t| t == tier) {
                continue;
            }
            let (_, band) = veredictum::conf_assets::band_of(&id).map_err(|e| e.to_string())?;
            bands.entry(band.to_owned()).or_default().push(CaseRow {
                id,
                kind: format!("{:?}", case.kind).to_lowercase(),
                purpose: case.test_purpose.clone(),
                tiers,
                band: band.to_owned(),
            });
        }
        Ok(bands
            .into_iter()
            .map(|(band, mut cases)| {
                cases.sort_by(|a, b| a.id.cmp(&b.id));
                BandRows { band, cases }
            })
            .collect())
    }

    /// The full case detail, or `Ok(None)` for an id the catalogue does not
    /// carry — a legitimately absent page, not an error.
    ///
    /// # Errors
    /// The verbatim load failure when the catalogue is absent.
    pub fn case_detail(state: &ConsoleState, id: &str) -> Result<Option<CaseDetail>, String> {
        let validation = state.catalogue.as_ref().as_ref().map_err(Clone::clone)?;
        let Some((path, case)) = validation
            .loaded
            .set
            .cases
            .iter()
            .find(|(_, case)| case.id.to_string() == id)
        else {
            return Ok(None);
        };
        let sm_operation = case.sm_operation.as_ref().map(ToString::to_string);
        let bindings = sm_operation.as_ref().map_or_else(Vec::new, |anchor| {
            validation
                .loaded
                .set
                .bindings
                .iter()
                .filter(|(_, binding)| binding.sm_operation.to_string() == *anchor)
                .map(|(binding_path, binding)| BindingRef {
                    file: binding_path
                        .file_name()
                        .map_or_else(String::new, |n| n.to_string_lossy().into_owned()),
                    realized: binding.unrealized.is_none(),
                })
                .collect()
        });
        let mut corpus_keys: Vec<String> = case
            .requires
            .templates
            .iter()
            .chain(case.requires.commit.iter())
            .map(ToString::to_string)
            .collect();
        corpus_keys.sort();
        corpus_keys.dedup();
        let applies = case
            .applies
            .entries()
            .into_iter()
            .map(|(component, range)| format!("{} {}", component.token(), range.raw()))
            .collect();
        let size = case.decision_table.as_ref().map_or_else(
            || format!("{} flow step(s)", case.flow.len()),
            |table| format!("{} decision-table row(s)", table.rows.len()),
        );
        Ok(Some(CaseDetail {
            id: case.id.to_string(),
            chapter: chapter_of(path),
            kind: format!("{:?}", case.kind).to_lowercase(),
            component: format!("{:?}", case.component),
            sm_operation,
            test_purpose: case.test_purpose.clone(),
            description: case.description.clone(),
            spec_refs: case.spec_refs.clone(),
            bindings,
            corpus_keys,
            tiers: tier_tokens(case),
            capabilities: case.capabilities.iter().map(ToString::to_string).collect(),
            exercises: case.exercises.iter().map(ToString::to_string).collect(),
            applies,
            guards: case.guards.clone(),
            formats: case
                .formats
                .iter()
                .map(|f| format!("{f:?}").to_lowercase())
                .collect(),
            option: case.option.as_ref().map(ToString::to_string),
            size,
        }))
    }
}

pub mod fns {
    //! The `#[server]` endpoints, one module for one inner suppression.
    //!
    //! The suppression covers what the macro expands: `unused_async`, and
    //! `missing_docs` on the argument struct it mints. Both fire only in SOME
    //! expansions, so `#[expect]` would itself warn through
    //! `unfulfilled_lint_expectations` in the quiet ones
    //! (<https://doc.rust-lang.org/rustc/lints/levels.html#expect>).
    #![allow(
        clippy::unused_async,
        missing_docs,
        reason = "fires only in some #[server] expansions; see the module doc"
    )]

    use super::{BandRows, CaseDetail, ChapterRow, InstrumentView};
    use leptos::prelude::{ServerFnError, server};

    #[cfg(feature = "ssr")]
    use super::read;

    /// The landing view: the counts, or the named-mount explanation.
    ///
    /// # Errors
    /// The server-fn transport only; the catalogue's absence is data, not an
    /// error.
    #[server]
    pub async fn fetch_instrument() -> Result<InstrumentView, ServerFnError> {
        let state: crate::state::ConsoleState = leptos::prelude::expect_context();
        Ok(read::instrument_view(&state))
    }

    /// The chapter list.
    ///
    /// # Errors
    /// The verbatim load failure when the catalogue is absent.
    #[server]
    pub async fn fetch_chapters() -> Result<Vec<ChapterRow>, ServerFnError> {
        let state: crate::state::ConsoleState = leptos::prelude::expect_context();
        read::chapter_rows(&state).map_err(ServerFnError::new)
    }

    /// One chapter's cases, grouped by band, filtered by the id substring
    /// `q` and the tier token `tier`.
    ///
    /// # Errors
    /// The verbatim load failure when the catalogue is absent, or the
    /// taxonomy's refusal for an unmapped id.
    #[server]
    pub async fn fetch_chapter_bands(
        chapter: String,
        q: String,
        tier: String,
    ) -> Result<Vec<BandRows>, ServerFnError> {
        let state: crate::state::ConsoleState = leptos::prelude::expect_context();
        read::band_rows(&state, &chapter, &q, &tier).map_err(ServerFnError::new)
    }

    /// One case in full; `None` for an id the catalogue does not carry.
    ///
    /// # Errors
    /// The verbatim load failure when the catalogue is absent.
    #[server]
    pub async fn fetch_case_detail(id: String) -> Result<Option<CaseDetail>, ServerFnError> {
        let state: crate::state::ConsoleState = leptos::prelude::expect_context();
        read::case_detail(&state, &id).map_err(ServerFnError::new)
    }
}
