// SPDX-FileCopyrightText: Veredictum contributors
// SPDX-License-Identifier: Apache-2.0

//! S8's presentation set (#68): the seal card, the badge and the
//! self-contained report a party publishes beside a sealed record.
//!
//! These three files are deliberately OUTSIDE the record manifest. The
//! manifest covers what the engine rendered and signed; these are renderings
//! ABOUT that bundle, and each carries the record digest prefix so a reader
//! can tie the artwork back to the bytes that were signed. Sealing itself is
//! never done here — the pinned CLI's `verdicts --sign-key` writes
//! `record-manifest.json` and its detached signature, and the console only
//! reads them back.
//!
//! Every renderer in this module is a pure function of the record, so the
//! same bundle bytes produce the same output bytes.

use serde::{Deserialize, Serialize};

/// The trademark acknowledgment and independence disclaimer every published
/// export surface carries visibly (the #94 ruling).
///
/// The same wording carries across every surface that publishes it; the seal
/// card renders it as [`INDEPENDENCE_LINES`].
pub const INDEPENDENCE_LINE: &str = "openEHR® is the registered trademark of the openEHR Foundation. Veredictum is an independent, community-driven conformance instrument, not an official openEHR Foundation product and not the Foundation's CNF program.";

/// [`INDEPENDENCE_LINE`] broken for the seal card's caption area, whose text
/// elements do not wrap. Joined with single spaces these ARE that line, which
/// the unit tests hold.
pub const INDEPENDENCE_LINES: [&str; 3] = [
    "openEHR® is the registered trademark of the openEHR Foundation. Veredictum is an",
    "independent, community-driven conformance instrument, not an official openEHR",
    "Foundation product and not the Foundation's CNF program.",
];

/// What a verified signature does and does not establish, rendered on every
/// S9 outcome and in the report footer.
///
/// One fact with the lib's own `veredictum::record::HONESTY_LINE`; the
/// `the_honesty_line_matches_the_engines` test holds the two together.
pub const HONESTY_LINE: &str =
    "A valid signature proves integrity and origin since signing — not the run's conditions.";

/// How many hex characters of the record digest the artwork carries.
///
/// Twelve is enough to name one bundle among a party's records while staying
/// readable in a badge; the full digest travels in the report footer and in
/// the manifest itself.
pub const DIGEST_PREFIX_CHARS: usize = 12;

/// Everything the artwork states about one sealed record.
///
/// Each field is read back from the bundle the engine wrote. Nothing is a
/// wall-clock reading: the signing time comes from the detached signature's
/// own creation subpacket, which is what makes the card reproducible.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SealFacts {
    /// The system under test, name and version, as the record spells it.
    pub sut: String,
    /// The profile verdicts summarized for the card's one line, with the
    /// performance class appended when a measured verdict exists.
    pub profile_summary: String,
    /// When the signature was made, as the detached signature carries it.
    pub signed_at: String,
    /// The full lowercase-hex SHA-256 of `record-manifest.json`.
    pub digest: String,
    /// The first [`DIGEST_PREFIX_CHARS`] of [`Self::digest`].
    pub digest_prefix: String,
    /// The fingerprint of the key component that signed the manifest.
    pub fingerprint: String,
    /// The top profile tier's verdict, for the badge's right segment.
    pub badge_label: String,
    /// Whether that verdict is a pass, which is the badge's only colour
    /// decision.
    pub badge_pass: bool,
}

/// Everything the self-contained report renders, all of it from the record.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReportFacts {
    /// The identity, tallies and red-first rows S6 shows.
    pub results: crate::record_api::ResultsScreen,
    /// The profile matrix S7 shows.
    pub profiles: Vec<(String, String)>,
    /// The capability evidence S7 shows.
    pub capabilities: Vec<(String, String)>,
    /// The seal facts the footer carries.
    pub seal: SealFacts,
}

/// What a renderer refuses.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum RenderError {
    /// The seal-card master no longer carries an anchor the fill needs, so a
    /// silent unfilled card would ship. Names the anchor.
    MissingAnchor {
        /// Which anchor could not be found.
        anchor: String,
    },
}

impl std::fmt::Display for RenderError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::MissingAnchor { anchor } => write!(
                f,
                "the seal-card master no longer carries the {anchor} anchor: the fill would ship an empty slot"
            ),
        }
    }
}

impl std::error::Error for RenderError {}

/// The first [`DIGEST_PREFIX_CHARS`] of a full hex digest.
///
/// A digest shorter than the prefix length is returned whole rather than
/// padded, so a malformed input is visible instead of disguised.
#[must_use]
pub fn prefix_of(digest: &str) -> String {
    digest.chars().take(DIGEST_PREFIX_CHARS).collect()
}

#[cfg(feature = "ssr")]
pub mod render {
    //! The renderers themselves, server-only.
    //!
    //! The presentation set is written to disk beside the sealed bundle and
    //! never shipped to a browser, so keeping these out of the `hydrate`
    //! build keeps the seal-card master and the report templates out of the
    //! WASM bundle (rules §1).

    use std::fmt::Write as _;

    use super::{
        HONESTY_LINE, INDEPENDENCE_LINE, INDEPENDENCE_LINES, RenderError, ReportFacts, SealFacts,
    };

    /// The seal-card master, compiled in.
    ///
    /// The card is filled from the ONE brand original rather than a copy, and
    /// reading it at runtime would make the export depend on a mount the
    /// container image does not carry.
    const SEAL_CARD_MASTER: &str = include_str!("../../../assets/brand/veredictum-seal-card.svg");

    /// Which fact of the record a dotted slot carries.
    ///
    /// The slot names its field, so a slot line and the value written on it
    /// travel together and no slot can be filled from a position that has no
    /// value behind it.
    #[derive(Debug, Clone, Copy)]
    enum SealSlot {
        /// The product under test.
        Sut,
        /// The conformance profile the verdict is spoken on.
        Profile,
        /// The instant the record was signed.
        SignedAt,
    }

    impl SealSlot {
        /// The record's fact this slot is filled from.
        fn value(self, facts: &SealFacts) -> &str {
            match self {
                Self::Sut => &facts.sut,
                Self::Profile => &facts.profile_summary,
                Self::SignedAt => &facts.signed_at,
            }
        }
    }

    /// The dotted slot lines the master ships empty, with the baseline each
    /// slot's value is written on and the fact it carries. Order is the
    /// card's own: product under test, conformance profile, verdict spoken
    /// on.
    const SLOT_ANCHORS: [(&str, u32, SealSlot); 3] = [
        (
            "<line x1=\"660\" y1=\"382\" x2=\"1180\" y2=\"382\" stroke=\"#258BB0\" stroke-width=\"1.6\" stroke-dasharray=\"2 4\"/>",
            375,
            SealSlot::Sut,
        ),
        (
            "<line x1=\"660\" y1=\"458\" x2=\"1180\" y2=\"458\" stroke=\"#258BB0\" stroke-width=\"1.6\" stroke-dasharray=\"2 4\"/>",
            451,
            SealSlot::Profile,
        ),
        (
            "<line x1=\"660\" y1=\"534\" x2=\"1180\" y2=\"534\" stroke=\"#258BB0\" stroke-width=\"1.6\" stroke-dasharray=\"2 4\"/>",
            527,
            SealSlot::SignedAt,
        ),
    ];

    /// The master's two grey caption lines, replaced wholesale by the
    /// disclaimer block and the digest line.
    const CAPTION_ANCHOR: &str = "<text x=\"660\" y=\"580\" font-size=\"16\" fill=\"#8a8a8a\">Every verdict re-derives from the recorded wire exchanges</text><text x=\"660\" y=\"602\" font-size=\"16\" fill=\"#8a8a8a\">and the released openEHR specifications.</text>";

    /// Escapes text for an XML or HTML text node or a quoted attribute.
    ///
    /// Both quote forms are escaped because the same helper fills SVG
    /// attributes, where an unescaped quote ends the value early. Public so a
    /// gate can assert that a rendered file carries a given sentence in the
    /// form it was actually written in.
    #[must_use]
    pub fn escape(text: &str) -> String {
        let mut out = String::with_capacity(text.len());
        for character in text.chars() {
            match character {
                '&' => out.push_str("&amp;"),
                '<' => out.push_str("&lt;"),
                '>' => out.push_str("&gt;"),
                '"' => out.push_str("&quot;"),
                '\'' => out.push_str("&#39;"),
                other => out.push(other),
            }
        }
        out
    }

    /// Fills the seal-card master's three dotted slots and its caption area.
    ///
    /// The result is the certificate a party publishes: the record's own
    /// facts in the artwork, the digest prefix tying it to the signed bundle,
    /// and the independence line rendered visibly.
    ///
    /// # Errors
    /// [`RenderError::MissingAnchor`] when the master no longer carries a
    /// slot line or the caption block the fill targets — a loud refusal,
    /// because a card that silently lost a slot would certify nothing.
    pub fn seal_card(facts: &SealFacts) -> Result<String, RenderError> {
        let mut svg = String::from(SEAL_CARD_MASTER);
        for (index, (anchor, baseline, slot)) in SLOT_ANCHORS.iter().enumerate() {
            if !svg.contains(anchor) {
                return Err(RenderError::MissingAnchor {
                    anchor: format!("slot {}", index + 1),
                });
            }
            let filled = format!(
                "{anchor}<text x=\"660\" y=\"{baseline}\" font-family=\"Georgia,serif\" font-size=\"24\" fill=\"#0b2530\">{}</text>",
                escape(slot.value(facts))
            );
            svg = svg.replace(anchor, &filled);
        }
        if !svg.contains(CAPTION_ANCHOR) {
            return Err(RenderError::MissingAnchor {
                anchor: String::from("caption"),
            });
        }
        Ok(svg.replace(CAPTION_ANCHOR, &caption(facts)))
    }

    /// The card's caption area: the digest line, then the disclaimer.
    fn caption(facts: &SealFacts) -> String {
        let mut caption = format!(
            "<text x=\"1180\" y=\"558\" font-family=\"ui-monospace,Menlo,Consolas,monospace\" font-size=\"15\" fill=\"#1B6E92\" text-anchor=\"end\">record {}</text>",
            escape(&facts.digest_prefix)
        );
        for (index, line) in INDEPENDENCE_LINES.iter().enumerate() {
            // 566, 586, 606 — three baselines inside the 614 inner border.
            let baseline = 566_u32.saturating_add(20 * u32::try_from(index).unwrap_or(0));
            let _ = write!(
                caption,
                "<text x=\"660\" y=\"{baseline}\" font-size=\"12\" fill=\"#8a8a8a\">{}</text>",
                escape(line)
            );
        }
        caption
    }

    /// The compact badge a party embeds beside its published record.
    ///
    /// Own design, shields-like, with no external fetch of any kind: the
    /// whole mark is drawn here. The digest prefix rides the accessible name
    /// and the `<title>`, and the verify path is embedded as a comment so a
    /// reader of the raw file finds the check without the surrounding page.
    #[must_use]
    pub fn badge(facts: &SealFacts) -> String {
        // Fixed advance widths keep the geometry a pure function of the label
        // length: a font metric read at render time would not be reproducible.
        let left_text = "VEREDICTUM";
        let left_width = 22 + 7 * u32::try_from(left_text.len()).unwrap_or(0);
        let right_width = 20 + 7 * u32::try_from(facts.badge_label.chars().count()).unwrap_or(0);
        let total = left_width.saturating_add(right_width);
        let right_fill = if facts.badge_pass {
            "#1f7a4d"
        } else {
            "#a3282d"
        };
        let label = escape(&format!(
            "Veredictum: {} (record {})",
            facts.badge_label, facts.digest_prefix
        ));
        format!(
            "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n\
             <!-- Veredictum record {prefix}. Verify this bundle at /verify, or run \
             `veredictum verify-record --record <dir> --key <public-key>`. -->\n\
             <svg xmlns=\"http://www.w3.org/2000/svg\" width=\"{total}\" height=\"20\" \
             viewBox=\"0 0 {total} 20\" role=\"img\" aria-label=\"{label}\">\n\
             <title>{label}</title>\n\
             <rect width=\"{left_width}\" height=\"20\" rx=\"3\" fill=\"#14566F\"/>\
             <rect x=\"{left_width}\" width=\"{right_width}\" height=\"20\" rx=\"3\" fill=\"{right_fill}\"/>\
             <rect x=\"{shim}\" width=\"6\" height=\"20\" fill=\"{right_fill}\"/>\n\
             <circle cx=\"12\" cy=\"10\" r=\"6\" fill=\"none\" stroke=\"#FF861C\" stroke-width=\"1.6\"/>\
             <path d=\"M 9,10 L 11,12.5 L 15,7.5\" fill=\"none\" stroke=\"#FFF\" stroke-width=\"1.8\" \
             stroke-linecap=\"round\" stroke-linejoin=\"round\"/>\n\
             <g font-family=\"Verdana,DejaVu Sans,Geneva,sans-serif\" font-size=\"11\" fill=\"#FFF\">\
             <text x=\"22\" y=\"14\">{left}</text>\
             <text x=\"{right_x}\" y=\"14\">{right}</text></g>\n\
             </svg>\n",
            prefix = escape(&facts.digest_prefix),
            shim = left_width.saturating_sub(3),
            right_x = left_width.saturating_add(10),
            left = escape(left_text),
            right = escape(&facts.badge_label),
        )
    }

    /// The copy-paste markdown snippet for the badge.
    ///
    /// `badge_url` is where the party hosts its own copy of the badge; the
    /// console cannot know that, so the caller supplies a placeholder and the
    /// surface says so.
    #[must_use]
    pub fn badge_markdown(badge_url: &str, verify_url: &str) -> String {
        format!("[![Veredictum conformance record]({badge_url})]({verify_url})")
    }

    /// The copy-paste HTML snippet for the badge.
    #[must_use]
    pub fn badge_html(badge_url: &str, verify_url: &str) -> String {
        format!(
            "<a href=\"{}\"><img src=\"{}\" alt=\"Veredictum conformance record\"></a>",
            escape(verify_url),
            escape(badge_url)
        )
    }

    /// The inline chip style for a verdict or evidence token, mirroring the
    /// console's own semantics in styles the report carries alone.
    fn chip_style(token: &str) -> &'static str {
        match token {
            "pass" | "passed" => "background:#dcf3e7;color:#0f3d28",
            "fail" | "failed" => "background:#fadcdd;color:#5a1417",
            "not_claimed" => "background:#eceae6;color:#5c5952",
            _ => "background:#fbeacf;color:#5a3b0f",
        }
    }

    /// The document head, with every style inline.
    fn report_head(sut: &str) -> String {
        format!(
            "<!DOCTYPE html>\n<html lang=\"en\"><head><meta charset=\"utf-8\">\
             <meta name=\"viewport\" content=\"width=device-width, initial-scale=1\">\
             <title>Veredictum conformance record — {sut}</title>\
             <style>\
             :root{{color-scheme:light}}\
             body{{margin:0;padding:2rem 1.25rem;background:#FFF9F2;color:#363636;\
             font-family:Georgia,serif;line-height:1.5}}\
             main{{max-width:60rem;margin:0 auto}}\
             h1{{font-size:1.9rem;color:#14566F;letter-spacing:.04em;margin:0 0 .25rem}}\
             h2{{font-size:1.2rem;color:#1B6E92;margin:2rem 0 .5rem}}\
             .sub{{color:#6b6b6b;margin:0 0 1.5rem}}\
             .tallies{{display:flex;flex-wrap:wrap;gap:.75rem;margin:0 0 1rem;padding:0;\
             list-style:none}}\
             .tallies li{{border:1px solid #e3ddd4;border-radius:.5rem;padding:.6rem 1rem;\
             background:#fff}}\
             .tallies b{{display:block;font-size:1.5rem;color:#14566F}}\
             table{{border-collapse:collapse;width:100%;\
             font-family:ui-monospace,Menlo,Consolas,monospace;font-size:.8rem}}\
             th,td{{border-bottom:1px solid #e3ddd4;padding:.4rem .5rem;text-align:left;\
             vertical-align:top}}\
             th{{color:#1B6E92;font-family:Georgia,serif;font-size:.85rem}}\
             .chip{{border-radius:.35rem;padding:.1rem .45rem;font-size:.75rem}}\
             .reason{{color:#5a1417;white-space:pre-wrap;word-break:break-word}}\
             .wrap{{overflow-x:auto}}\
             footer{{margin-top:2.5rem;border-top:2px solid #FF861C;padding-top:1rem;\
             font-size:.8rem;color:#6b6b6b}}\
             footer dt{{color:#1B6E92;margin-top:.5rem}}\
             footer dd{{margin:0;font-family:ui-monospace,Menlo,Consolas,monospace;\
             word-break:break-all}}\
             </style></head><body><main>",
            sut = escape(sut)
        )
    }

    /// One two-column table of token rows, chipped by their own vocabulary.
    fn token_table(caption: &str, head: (&str, &str), rows: &[(String, String)]) -> String {
        let mut table = format!(
            "<h2>{caption}</h2><div class=\"wrap\"><table><thead><tr><th>{}</th><th>{}</th>\
             </tr></thead><tbody>",
            escape(head.0),
            escape(head.1)
        );
        for (name, token) in rows {
            let _ = write!(
                table,
                "<tr><td>{}</td><td><span class=\"chip\" style=\"{}\">{}</span></td></tr>",
                escape(name),
                chip_style(token),
                escape(token)
            );
        }
        table.push_str("</tbody></table></div>");
        table
    }

    /// The outcome rows, red first, each reason verbatim and escaped.
    fn outcomes_table(rows: &[crate::record_api::ResultRow]) -> String {
        let mut table = String::from(
            "<h2>Outcomes</h2><p class=\"sub\">Failures and errors first; a red row names one \
             behaviour, and its reason is the recorded evidence verbatim.</p>\
             <div class=\"wrap\"><table><thead><tr><th>Case</th><th>Format</th><th>Status</th>\
             <th>Rows</th><th>Reason</th></tr></thead><tbody>",
        );
        for row in rows {
            let _ = write!(
                table,
                "<tr><td>{case}</td><td>{format}</td>\
                 <td><span class=\"chip\" style=\"{style}\">{status}</span></td><td>{rows}</td>\
                 <td class=\"reason\">{reason}</td></tr>",
                case = escape(&row.case),
                format = escape(row.format.as_deref().unwrap_or("—")),
                style = chip_style(&row.status),
                status = escape(&row.status),
                rows = escape(&row.rows),
                reason = escape(row.reason.as_deref().unwrap_or(""))
            );
        }
        table.push_str("</tbody></table></div>");
        table
    }

    /// The footer: the record's identity, and both standing lines.
    fn report_footer(seal: &SealFacts) -> String {
        format!(
            "<footer><dl>\
             <dt>Record digest (SHA-256 of record-manifest.json)</dt><dd>{digest}</dd>\
             <dt>Signer fingerprint</dt><dd>{fingerprint}</dd>\
             <dt>Signing time</dt><dd>{signed}</dd>\
             </dl><p>{honesty}</p><p>{independence}</p>\
             <p>Check this bundle yourself: <code>veredictum verify-record --record &lt;dir&gt; \
             --key &lt;public-key&gt;</code></p></footer></main></body></html>\n",
            digest = escape(&seal.digest),
            fingerprint = escape(&seal.fingerprint),
            signed = escape(&seal.signed_at),
            honesty = escape(HONESTY_LINE),
            independence = escape(INDEPENDENCE_LINE)
        )
    }

    /// Renders the one-file HTML report of what S6 and S7 show.
    ///
    /// Self-contained by construction: every style is inline, there is no
    /// script, and nothing is fetched — so the file renders identically from
    /// a filesystem, an email attachment, or a static host. The footer
    /// carries the full record digest, the signer fingerprint, the signing
    /// time, the honesty line and the independence line.
    #[must_use]
    pub fn html_report(facts: &ReportFacts) -> String {
        let (passed, failed, errored, excused) = facts.results.tallies;
        let mut body = report_head(&facts.results.sut);
        let _ = write!(
            body,
            "<h1>Veredictum conformance record</h1>\
             <p class=\"sub\">{sut} · {profile} · verdict spoken on {signed}</p>\
             <ul class=\"tallies\"><li><b>{passed}</b>passed</li><li><b>{failed}</b>failed</li>\
             <li><b>{errored}</b>errored</li><li><b>{excused}</b>excused</li></ul>",
            sut = escape(&facts.results.sut),
            profile = escape(&facts.seal.profile_summary),
            signed = escape(&facts.seal.signed_at)
        );
        body.push_str(&token_table(
            "Profile verdicts",
            ("Tier", "Verdict"),
            &facts.profiles,
        ));
        body.push_str(&outcomes_table(&facts.results.rows));
        body.push_str(&token_table(
            "Capability evidence",
            ("Capability", "Evidence"),
            &facts.capabilities,
        ));
        body.push_str(&report_footer(&facts.seal));
        body
    }

    #[cfg(test)]
    mod tests {
        use super::escape;

        #[test]
        fn escaping_covers_every_metacharacter() {
            assert_eq!(escape("<&>\"'"), "&lt;&amp;&gt;&quot;&#39;");
        }
    }
}

#[cfg(all(test, feature = "ssr"))]
#[expect(
    clippy::panic_in_result_fn,
    reason = "Result-returning tests in the Book ch11 shape that also assert; clippy offers no allow-in-tests knob for this lint"
)]
mod tests {
    use super::render::{badge, badge_html, badge_markdown, html_report, seal_card};
    use super::{
        DIGEST_PREFIX_CHARS, HONESTY_LINE, INDEPENDENCE_LINE, INDEPENDENCE_LINES, RenderError,
        ReportFacts, SealFacts, prefix_of,
    };
    use crate::record_api::{ResultRow, ResultsScreen};

    fn facts() -> SealFacts {
        SealFacts {
            sut: String::from("my-cdr 1.2.3"),
            profile_summary: String::from("CORE pass · STANDARD pass"),
            signed_at: String::from("2026-08-27T10:11:12Z"),
            digest: String::from(
                "a1b2c3d4e5f60718293a4b5c6d7e8f900112233445566778899aabbccddeeff0",
            ),
            digest_prefix: String::from("a1b2c3d4e5f6"),
            fingerprint: String::from("deadbeefdeadbeefdeadbeefdeadbeefdeadbeef"),
            badge_label: String::from("CORE pass"),
            badge_pass: true,
        }
    }

    fn report_facts() -> ReportFacts {
        ReportFacts {
            results: ResultsScreen {
                sut: String::from("my-cdr 1.2.3"),
                tallies: (12, 1, 0, 2),
                rows: vec![
                    ResultRow {
                        case: String::from("I_EHR_SERVICE.create_ehr-main"),
                        format: Some(String::from("json")),
                        status: String::from("failed"),
                        rows: String::from("1/1"),
                        reason: Some(String::from("expected 201, observed 500 <script>")),
                    },
                    ResultRow {
                        case: String::from("I_EHR_SERVICE.get_ehr-main"),
                        format: None,
                        status: String::from("passed"),
                        rows: String::from("1/1"),
                        reason: None,
                    },
                ],
            },
            profiles: vec![
                (String::from("CORE"), String::from("pass")),
                (String::from("STANDARD"), String::from("fail")),
            ],
            capabilities: vec![
                (String::from("ehr.create"), String::from("passed")),
                (String::from("ehr.delete"), String::from("not_evidenced")),
            ],
            seal: facts(),
        }
    }

    /// The card's whole promise: same bundle in, same bytes out.
    #[test]
    fn the_seal_card_is_byte_deterministic() -> Result<(), RenderError> {
        assert_eq!(seal_card(&facts())?, seal_card(&facts())?);
        Ok(())
    }

    #[test]
    fn the_seal_card_fills_every_slot_with_the_records_own_facts()
    -> Result<(), Box<dyn std::error::Error>> {
        let card = seal_card(&facts())?;
        assert!(card.contains("my-cdr 1.2.3"), "the product under test");
        assert!(card.contains("CORE pass · STANDARD pass"), "the profile");
        assert!(card.contains("2026-08-27T10:11:12Z"), "the signing time");
        assert!(card.contains("record a1b2c3d4e5f6"), "the digest prefix");
        // The master's three dotted lines survive: the values sit ABOVE them.
        assert_eq!(card.matches("stroke-dasharray=\"2 4\"").count(), 3);
        Ok(())
    }

    /// The #94 ruling: what a party publishes says so on its face.
    #[test]
    fn the_seal_card_renders_the_independence_line_visibly()
    -> Result<(), Box<dyn std::error::Error>> {
        let card = seal_card(&facts())?;
        for line in INDEPENDENCE_LINES {
            let written = super::render::escape(line);
            assert!(card.contains(&written), "missing caption line: {line}");
        }
        // The master's own decorative caption is what the disclaimer replaced.
        assert!(!card.contains("Every verdict re-derives"));
        Ok(())
    }

    /// The broken caption lines and the canonical sentence are one fact.
    #[test]
    fn the_caption_lines_rejoin_into_the_canonical_disclaimer() {
        assert_eq!(INDEPENDENCE_LINES.join(" "), INDEPENDENCE_LINE);
    }

    /// One fact with the engine's own constant, so the console cannot soften
    /// what verification claims.
    #[test]
    fn the_honesty_line_matches_the_engines() {
        assert_eq!(HONESTY_LINE, veredictum::record::HONESTY_LINE);
    }

    /// A value carrying SVG metacharacters must not close a text element.
    #[test]
    fn the_seal_card_escapes_a_hostile_sut_name() -> Result<(), Box<dyn std::error::Error>> {
        let mut hostile = facts();
        hostile.sut = String::from("</text><script>alert(1)</script>");
        let card = seal_card(&hostile)?;
        assert!(!card.contains("<script>"), "the name escaped its element");
        assert!(card.contains("&lt;script&gt;"));
        Ok(())
    }

    #[test]
    fn the_badge_is_byte_deterministic_and_names_the_record() {
        let first = badge(&facts());
        assert_eq!(first, badge(&facts()));
        assert!(first.contains("VEREDICTUM"));
        assert!(first.contains("CORE pass"));
        assert!(first.contains("a1b2c3d4e5f6"), "the digest prefix");
        assert!(first.contains("/verify"), "the verify path comment");
        assert!(!first.contains("<script"), "no script in a badge");
    }

    /// The badge's one colour decision is the verdict, and a failing record
    /// must not be able to render as a passing green.
    #[test]
    fn the_badge_colours_a_failure_differently() {
        let mut failing = facts();
        failing.badge_label = String::from("CORE fail");
        failing.badge_pass = false;
        let red = badge(&failing);
        assert!(red.contains("#a3282d"), "the failing fill");
        assert!(!red.contains("#1f7a4d"), "no passing fill on a failure");
    }

    #[test]
    fn the_report_is_byte_deterministic() {
        assert_eq!(html_report(&report_facts()), html_report(&report_facts()));
    }

    #[test]
    fn the_report_carries_the_record_and_both_standing_lines() {
        let html = html_report(&report_facts());
        assert!(html.contains(&facts().digest), "the full digest");
        assert!(html.contains(&facts().fingerprint), "the fingerprint");
        assert!(html.contains("2026-08-27T10:11:12Z"), "the signing time");
        assert!(html.contains(&super::render::escape(HONESTY_LINE)));
        assert!(html.contains(&super::render::escape(INDEPENDENCE_LINE)));
        assert!(html.contains("veredictum verify-record"));
    }

    /// Self-contained means self-contained: no fetch of any kind, and no
    /// script, so the file renders the same from a disk as from a host.
    #[test]
    fn the_report_makes_no_external_request() {
        let html = html_report(&report_facts());
        assert!(!html.contains("<script"), "no script");
        assert!(!html.contains("http://"), "no external http reference");
        assert!(!html.contains("https://"), "no external https reference");
        assert!(!html.contains("<link"), "no external stylesheet");
    }

    #[test]
    fn the_report_renders_both_screens_content() {
        let html = html_report(&report_facts());
        assert!(html.contains("I_EHR_SERVICE.create_ehr-main"), "a row");
        assert!(html.contains("STANDARD"), "the profile matrix");
        assert!(html.contains("not_evidenced"), "the coverage bound");
        assert!(html.contains(">12<"), "the passed tally");
    }

    /// A recorded reason is SUT-supplied text; it must never become markup.
    #[test]
    fn the_report_escapes_a_recorded_reason() {
        let html = html_report(&report_facts());
        assert!(html.contains("&lt;script&gt;"), "the reason was escaped");
        assert!(!html.contains("observed 500 <script>"));
    }

    #[test]
    fn the_prefix_is_the_first_twelve_hex_characters() {
        assert_eq!(prefix_of("0123456789abcdef"), "0123456789ab");
        assert_eq!(prefix_of("0123456789abcdef").len(), DIGEST_PREFIX_CHARS);
        // A short digest comes back whole rather than padded into a lie.
        assert_eq!(prefix_of("abc"), "abc");
    }

    #[test]
    fn the_snippets_point_at_the_hosted_badge_and_the_verify_page() {
        assert_eq!(
            badge_markdown("record-badge.svg", "https://example.test/verify"),
            "[![Veredictum conformance record](record-badge.svg)](https://example.test/verify)"
        );
        assert!(badge_html("record-badge.svg", "/verify").contains("href=\"/verify\""));
    }
}
