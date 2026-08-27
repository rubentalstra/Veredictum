// SPDX-FileCopyrightText: Veredictum contributors
// SPDX-License-Identifier: Apache-2.0

//! Judging a campaign: the pure verdict pipeline over a statement and a
//! results record, plus the submission documents it renders.
//!
//! Nothing here touches a network or a system under test. The inputs are two
//! committed party artifacts and one catalogue; the output is the verdict
//! report and the documents derived from it, which makes a judgement
//! reproducible by anyone holding the same three files.

use std::path::Path;

use crate::party::{Results, Statement};
use crate::pipeline::{Error, RenderedFile, load_clean_root, load_party_json, to_json_document};
use crate::render::{render_certificate, render_report, render_statement};
use crate::schema::{results_schema, statement_schema};
use crate::verdict::VerdictReport;

/// Which campaign to judge, against which catalogue.
#[derive(Debug)]
pub struct JudgementRequest<'a> {
    /// The party statement, the claim being certified.
    pub statement: &'a Path,
    /// The party results, the campaign being judged.
    pub results: &'a Path,
    /// The artifact root the claim is judged against.
    pub root: &'a Path,
}

/// One completed judgement.
#[derive(Debug)]
pub struct Judgement {
    /// The claim that was judged.
    pub statement: Statement,
    /// The campaign that was judged.
    pub results: Results,
    /// The verdicts, the coverage accounting and the static-review findings.
    pub report: VerdictReport,
    /// The submission set: the verdict record, the three rendered documents
    /// and the badge endpoints, in publication order.
    pub documents: Vec<RenderedFile>,
}

impl Judgement {
    /// Returns whether the judgement is clean, which means the static review
    /// found nothing.
    #[must_use]
    pub fn is_clean(&self) -> bool {
        self.report.review.is_empty()
    }
}

/// Computes the verdicts for one campaign and renders its submission set.
///
/// # Errors
/// [`Error::Party`] when either party artifact fails its schema or model
/// stage, [`Error::ResultsInvariants`] when the results violate their own
/// invariants, [`Error::Catalogue`] or [`Error::Artifacts`] when the
/// catalogue does not load, [`Error::Missing`] when the tree carries no
/// capability matrix or ambiguity register, and [`Error::Serialize`] or
/// [`Error::Instrument`] when a document cannot be produced.
pub fn judge(request: &JudgementRequest<'_>) -> Result<Judgement, Error> {
    let statement: Statement = load_party_json(
        request.statement,
        &statement_schema(),
        "statement.schema.json",
    )?;
    let results: Results =
        load_party_json(request.results, &results_schema(), "results.schema.json")?;
    results
        .check_invariants()
        .map_err(Error::ResultsInvariants)?;

    let loaded = load_clean_root(request.root)?;
    let Some((_, matrix)) = &loaded.set.matrix else {
        return Err(Error::Missing(
            "artifact tree carries no capability matrix".to_owned(),
        ));
    };
    let Some((_, register)) = &loaded.set.register else {
        return Err(Error::Missing(
            "artifact tree carries no ambiguity register".to_owned(),
        ));
    };
    let cases: Vec<_> = loaded.set.cases.iter().map(|(_, c)| c.clone()).collect();
    let perf_cases: Vec<_> = loaded
        .set
        .performance
        .iter()
        .map(|(_, c)| c.clone())
        .collect();

    let report =
        crate::verdict::compute(&statement, &results, &cases, &perf_cases, matrix, register);

    // The outward wire-surface axis (`vocab/wire_surface.yaml`
    // `served_extensions`): rendered into the statement as a declaration of
    // the non-openEHR surface, never an input to any verdict.
    let served_extensions = match &loaded.set.wire_surface {
        Some((_, wire_surface)) => wire_surface.served_extensions.as_slice(),
        None => &[],
    };

    let mut documents = vec![
        RenderedFile {
            name: "verdicts.json".to_owned(),
            body: to_json_document(&report, "cannot serialize verdicts")?,
        },
        RenderedFile {
            name: "CONFORMANCE_REPORT.md".to_owned(),
            body: render_report(&results, &report, &statement)
                .map_err(|e| Error::Instrument(format!("cannot render the report: {e}")))?,
        },
        RenderedFile {
            name: "CONFORMANCE_STATEMENT.md".to_owned(),
            body: render_statement(&statement, &report, served_extensions),
        },
        RenderedFile {
            name: "CONFORMANCE_CERTIFICATE.md".to_owned(),
            body: render_certificate(&statement, &results, &report, matrix),
        },
    ];
    // The shields.io endpoints, derived here rather than downstream so a
    // published count and the verdict beside it come from one rule.
    for named in crate::badges::badges(&report, matrix, crate::badges::CaseCounts::of(&results)) {
        let body = to_json_document(
            &named.badge,
            &format!("cannot serialize the {} badge", named.file),
        )?;
        documents.push(RenderedFile {
            name: named.file,
            body,
        });
    }

    Ok(Judgement {
        statement,
        results,
        report,
        documents,
    })
}
