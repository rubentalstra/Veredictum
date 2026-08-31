// SPDX-FileCopyrightText: Veredictum contributors
// SPDX-License-Identifier: Apache-2.0

//! Whole-document equivalence for the two non-JSON forms a retrieval can
//! serve: an XML document entity and plain source text.
//!
//! Canonical JSON and canonical XML are the two bound document forms of the
//! same RM content (ITS-REST `specifications/docs/overview/Resources.md`
//! §Data representation), and an ADL2 artefact is neither: it is `text/plain`
//! source. The member-addressing assertion families read the JSON binding
//! only, so they refuse an unparsed body. The `equivalent` family addresses no
//! member — it compares a served DOCUMENT against a corpus fixture — so it is
//! judgeable on these forms once both sides are known to be in the same one.
//!
//! Both rules are stated where a catalogue author can predict them, because a
//! comparator nobody can predict is worse than one that refuses:
//!
//! - **XML.** Two document entities are equivalent when their canonical item
//!   streams are equal: element sequence in document order, each element's
//!   expanded name (namespace name plus local name, Namespaces in XML §2.1
//!   <https://www.w3.org/TR/xml-names/>) and its attribute SET keyed by
//!   expanded name, character data with CDATA sections replaced by their
//!   character content, and comments and processing instructions in place.
//!   Attribute ORDER is not compared, because "the order of attribute
//!   specifications in a start-tag or empty-element tag is not significant"
//!   (XML 1.0 §3.1 <https://www.w3.org/TR/xml/#sec-starttags>), which is also
//!   why Canonical XML 1.1 §2.2 sorts them; `<a/>` equals `<a></a>` by the
//!   same section's empty-element rule; the XML declaration and the document
//!   type declaration are ignored (Canonical XML 1.1 §2.3 removes both,
//!   <https://www.w3.org/TR/xml-c14n11/>); and line ends are normalized to LF
//!   (XML 1.0 §2.11). Character data is otherwise compared EXACTLY, because
//!   "an XML processor MUST always pass all characters in a document that are
//!   not markup through to the application" (XML 1.0 §2.10), so indentation
//!   inside an element's content is a difference. The one place this is weaker
//!   than the c14n byte form: a namespace PREFIX is not compared, only the
//!   namespace name it resolves to, since the prefix is not part of an
//!   expanded name.
//! - **Text.** Two source texts are equivalent when they are equal after line
//!   ends are normalized to LF, and not otherwise. MIME's canonical form
//!   "MUST always represent a line break as a CRLF sequence" (RFC 2046 §4.1.1
//!   <https://www.rfc-editor.org/rfc/rfc2046.html#section-4.1.1>) while
//!   "HTTP allows the transfer of text media with plain CR or LF alone
//!   representing a line break" (RFC 7231 Appendix A.2
//!   <https://www.rfc-editor.org/rfc/rfc7231.html#appendix-A.2>), so the
//!   spelling of a line break is a transfer choice and charging the SUT for it
//!   would be charging it for something HTTP grants. Everything else is
//!   byte-exact: trailing whitespace, blank lines and the final newline are
//!   content.
//!
//! Neither rule tolerates a re-serialization. The catalogue's own position on
//! retrieval fidelity is register entry `AMB-111` (disposition
//! `fixed_handling`): retrieval is VERBATIM, the ADL 1.4 OPT as the canonical
//! XML document the client sent and the ADL2 artefact as the source text it
//! sent. So a normalized whitespace run, a dropped element, an expanded
//! attribute default and a renumbered terminology code are all differences;
//! what is tolerated above is only what the cited specifications put outside
//! the document itself.
//!
//! A form this module cannot read stays out of the comparison entirely: the
//! caller keeps the inconclusive channel, never a pass.

use std::collections::BTreeMap;

use quick_xml::events::{BytesStart, Event};
use quick_xml::name::ResolveResult;

use crate::vocab::CorpusFormat;

/// The document form the two sides of a comparison must agree on.
///
/// The set is closed at two members on purpose: these are the forms this
/// module can read. An unrecognized media type or corpus format yields no
/// form at all rather than a default, so a typo cannot manufacture a
/// comparison.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DocumentForm {
    /// An XML document entity: canonical openEHR XML, or an ADL 1.4
    /// operational template in its XML form.
    Xml,
    /// Plain source text: an ADL2 or ADL 1.4 artefact source, or an AQL text.
    Text,
}

impl DocumentForm {
    /// The form a corpus entry's declared format is in, when it is one of
    /// these two.
    ///
    /// The JSON formats answer [`None`]: a body that did not parse as JSON is
    /// not in the same form as a JSON fixture, whatever it is.
    #[must_use]
    pub const fn of_corpus_format(format: CorpusFormat) -> Option<Self> {
        match format {
            CorpusFormat::CanonicalXml | CorpusFormat::OptXml => Some(Self::Xml),
            CorpusFormat::Adl2Text | CorpusFormat::Adl14Text | CorpusFormat::AqlText => {
                Some(Self::Text)
            }
            CorpusFormat::CanonicalJson
            | CorpusFormat::WtFlat
            | CorpusFormat::WtStructured
            | CorpusFormat::RawJson => None,
        }
    }

    /// The form a served `Content-Type` declares, when it declares one of
    /// these two.
    ///
    /// The media type is read without its parameters and case-insensitively
    /// (RFC 9110 §8.3.1: "The type and subtype tokens are case-insensitive").
    /// `application/xml` and `text/xml` are the registered XML types
    /// (RFC 7303 §9.1 and §9.2) and the `+xml` suffix marks a media type whose
    /// content is XML (RFC 7303 §4.2); `text/plain` is the text type
    /// (RFC 2046 §4.1).
    #[must_use]
    pub fn of_media_type(media_type: &str) -> Option<Self> {
        let essence = media_type
            .split(';')
            .next()
            .unwrap_or(media_type)
            .trim()
            .to_ascii_lowercase();
        match essence.as_str() {
            "application/xml" | "text/xml" => Some(Self::Xml),
            "text/plain" => Some(Self::Text),
            other if other.ends_with("+xml") => Some(Self::Xml),
            _ => None,
        }
    }

    /// The token a diagnostic names this form by.
    #[must_use]
    pub const fn token(self) -> &'static str {
        match self {
            Self::Xml => "XML",
            Self::Text => "plain text",
        }
    }
}

/// Why a whole-document comparison did not come out equal.
///
/// The three members are separate because they belong to different channels:
/// a divergence and an ill-formed SERVED document are findings against the
/// SUT, while an ill-formed FIXTURE is a defect of this instrument's own
/// catalogue and proves nothing about the server. The caller branches on the
/// variant, never on the message.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum DocumentDivergence {
    /// Both documents were readable and they differ.
    #[error("{0}")]
    Divergent(String),
    /// The SUT's body is not a well-formed XML document entity.
    #[error("the served body is not a well-formed XML document entity: {0}")]
    ServedIllFormed(String),
    /// The corpus fixture is not a well-formed XML document entity.
    #[error("the corpus fixture is not a well-formed XML document entity: {0}")]
    FixtureIllFormed(String),
}

/// Compare a served document against a corpus fixture, in the form both are
/// declared to be in.
///
/// # Errors
/// [`DocumentDivergence`] naming where the two differ, or which side is not a
/// readable XML document entity.
pub fn compare(form: DocumentForm, served: &str, fixture: &str) -> Result<(), DocumentDivergence> {
    match form {
        DocumentForm::Xml => compare_xml(served, fixture),
        DocumentForm::Text => compare_text(served, fixture),
    }
}

/// An element's or attribute's expanded name: the namespace name it resolves
/// to, and its local name (Namespaces in XML §2.1).
///
/// The derived [`Ord`] takes the namespace name as primary key and the local
/// name as secondary, which is the order Canonical XML 1.1 §2.2 sorts an
/// element's attributes into — so a [`BTreeMap`] keyed by this type IS the
/// canonical attribute order, deterministically.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
struct Name {
    namespace: Option<String>,
    local: String,
}

impl std::fmt::Display for Name {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match &self.namespace {
            Some(namespace) => write!(f, "{{{namespace}}}{}", self.local),
            None => write!(f, "{}", self.local),
        }
    }
}

/// One item of a document's canonical stream.
#[derive(Debug, Clone, PartialEq, Eq)]
enum Item {
    /// An element's start: its expanded name and its attribute set.
    Start {
        name: Name,
        attributes: BTreeMap<Name, String>,
    },
    /// An element's end. The name is not carried: a well-formed document's
    /// end tag matches its start tag, which the reader already enforces.
    End,
    /// A run of character data, references resolved and CDATA sections
    /// replaced by their character content.
    Chars(String),
    /// A comment's content.
    Comment(String),
    /// A processing instruction's content.
    Instruction(String),
}

impl Item {
    /// The short rendering a divergence message shows.
    fn preview(&self) -> String {
        let full = match self {
            Self::Start { name, attributes } => {
                let attrs: Vec<String> = attributes
                    .iter()
                    .map(|(k, v)| format!(" {k}=\"{v}\""))
                    .collect();
                format!("<{name}{}>", attrs.join(""))
            }
            Self::End => "</…>".to_owned(),
            Self::Chars(text) => format!("character data {text:?}"),
            Self::Comment(text) => format!("<!--{text}-->"),
            Self::Instruction(text) => format!("<?{text}?>"),
        };
        truncated(&full)
    }
}

/// At most 120 characters of a rendering, cut on a CHARACTER boundary.
///
/// `chars().take(..)` rather than a byte range: a byte slice panics on a
/// multi-byte boundary, and clinical text and archetype terminology are full
/// of multi-byte characters.
fn truncated(text: &str) -> String {
    let cut: String = text.chars().take(120).collect();
    if cut.chars().count() < text.chars().count() {
        format!("{cut}…")
    } else {
        cut
    }
}

/// Normalize line ends to LF.
///
/// For XML this is the processor's duty: a CRLF or a lone CR in the entity is
/// passed to the application as a single LF (XML 1.0 §2.11). For `text/plain`
/// it is what HTTP's own tolerance requires of a comparator (RFC 7231
/// Appendix A.2).
fn normalize_line_ends(text: &str) -> String {
    text.replace("\r\n", "\n").replace('\r', "\n")
}

/// Compare two source texts under the text rule.
fn compare_text(served: &str, fixture: &str) -> Result<(), DocumentDivergence> {
    let a = normalize_line_ends(served);
    let b = normalize_line_ends(fixture);
    if a == b {
        return Ok(());
    }
    let mut served_lines = a.lines();
    let mut fixture_lines = b.lines();
    let mut line = 0_usize;
    loop {
        line = line.saturating_add(1);
        match (served_lines.next(), fixture_lines.next()) {
            (None, None) => break,
            (Some(x), Some(y)) if x == y => {}
            (got, want) => {
                return Err(DocumentDivergence::Divergent(format!(
                    "the served text diverges from the corpus fixture at line {line}: got {}, want {}",
                    got.map_or_else(
                        || "end of document".to_owned(),
                        |t| format!("{:?}", truncated(t))
                    ),
                    want.map_or_else(
                        || "end of document".to_owned(),
                        |t| format!("{:?}", truncated(t))
                    ),
                )));
            }
        }
    }
    // Every line paired and the whole texts still differ: the trailing line
    // end is the only remaining difference, and it is content.
    Err(DocumentDivergence::Divergent(format!(
        "the served text diverges from the corpus fixture only in its trailing line end: it ends \
         {}, the fixture ends {}",
        ends_with_newline(&a),
        ends_with_newline(&b)
    )))
}

/// How a text ends, for the trailing-newline diagnostic.
fn ends_with_newline(text: &str) -> &'static str {
    if text.ends_with('\n') {
        "with a line end"
    } else {
        "without a line end"
    }
}

/// Compare two XML document entities under the XML rule.
fn compare_xml(served: &str, fixture: &str) -> Result<(), DocumentDivergence> {
    let a = canonical_items(served).map_err(DocumentDivergence::ServedIllFormed)?;
    let b = canonical_items(fixture).map_err(DocumentDivergence::FixtureIllFormed)?;
    let mut frames: Vec<Frame> = Vec::new();
    for (got, want) in a.iter().zip(&b) {
        if got != want {
            return Err(DocumentDivergence::Divergent(format!(
                "the served XML document diverges from the corpus fixture at {}: got {}, want {}",
                path_of(&frames),
                got.preview(),
                want.preview()
            )));
        }
        step(&mut frames, got);
    }
    if a.len() == b.len() {
        return Ok(());
    }
    let (longer, side) = if a.len() > b.len() {
        (a.len(), "the served document")
    } else {
        (b.len(), "the corpus fixture")
    };
    Err(DocumentDivergence::Divergent(format!(
        "the served XML document and the corpus fixture agree for {} item(s) and then {side} \
         continues to {longer}: one document is a prefix of the other",
        a.len().min(b.len())
    )))
}

/// One open element while the two streams are walked in lockstep: its local
/// name, its position among its own siblings, and how many children it has
/// opened so far.
#[derive(Debug)]
struct Frame {
    local: String,
    ordinal: usize,
    children: usize,
}

/// Advance the element path by one compared item.
fn step(frames: &mut Vec<Frame>, item: &Item) {
    match item {
        Item::Start { name, .. } => {
            let ordinal = match frames.last_mut() {
                Some(parent) => {
                    let ordinal = parent.children;
                    parent.children = parent.children.saturating_add(1);
                    ordinal
                }
                None => 0,
            };
            frames.push(Frame {
                local: name.local.clone(),
                ordinal,
                children: 0,
            });
        }
        Item::End => {
            frames.pop();
        }
        Item::Chars(_) | Item::Comment(_) | Item::Instruction(_) => {}
    }
}

/// The element path of the position two streams diverged at.
fn path_of(frames: &[Frame]) -> String {
    if frames.is_empty() {
        return "the document root".to_owned();
    }
    let mut path = String::new();
    for frame in frames {
        path.push('/');
        path.push_str(&frame.local);
        path.push('[');
        path.push_str(&frame.ordinal.to_string());
        path.push(']');
    }
    path
}

/// Read an XML document entity into its canonical item stream.
///
/// The whole document is read to end of input, not just its first tag: a
/// truncated or unbalanced document is not the document the fixture declares,
/// whatever its opening looks like.
fn canonical_items(text: &str) -> Result<Vec<Item>, String> {
    let mut reader = quick_xml::NsReader::from_str(text);
    let mut items: Vec<Item> = Vec::new();
    let mut chars = String::new();
    let mut depth: i64 = 0;
    let mut root_seen = false;
    loop {
        let (resolved, event) = reader.read_resolved_event().map_err(|e| e.to_string())?;
        let namespace = element_namespace(resolved);
        match event {
            Event::Eof => break,
            Event::Start(element) => {
                flush(&mut chars, &mut items);
                depth = depth.saturating_add(1);
                root_seen = true;
                items.push(start_item(&mut reader, &element, namespace?)?);
            }
            Event::Empty(element) => {
                flush(&mut chars, &mut items);
                root_seen = true;
                items.push(start_item(&mut reader, &element, namespace?)?);
                items.push(Item::End);
            }
            Event::End(_) => {
                flush(&mut chars, &mut items);
                depth = depth.saturating_sub(1);
                items.push(Item::End);
            }
            Event::Text(content) => {
                let text = content.xml10_content().map_err(|e| e.to_string())?;
                if depth > 0 {
                    chars.push_str(&text);
                } else if !text.trim().is_empty() {
                    return Err(
                        "character data outside the document element (XML 1.0 §2.1)".to_owned()
                    );
                }
            }
            Event::CData(content) => {
                let text = content.decode().map_err(|e| e.to_string())?;
                if depth > 0 {
                    chars.push_str(&normalize_line_ends(&text));
                }
            }
            Event::GeneralRef(reference) => {
                chars.push_str(&resolved_reference(&reference)?);
            }
            Event::Comment(content) => {
                flush(&mut chars, &mut items);
                items.push(Item::Comment(
                    content.decode().map_err(|e| e.to_string())?.into_owned(),
                ));
            }
            Event::PI(content) => {
                flush(&mut chars, &mut items);
                items.push(Item::Instruction(format!(
                    "{} {}",
                    String::from_utf8_lossy(content.target()),
                    String::from_utf8_lossy(content.content())
                )));
            }
            // Canonical XML 1.1 §2.3: the XML declaration and the document
            // type declaration are removed from the canonical form.
            Event::Decl(_) | Event::DocType(_) => {}
        }
    }
    if depth != 0 {
        return Err("the document ends with unclosed elements".to_owned());
    }
    if !root_seen {
        return Err("the document carries no XML element at all".to_owned());
    }
    Ok(items)
}

/// The namespace name an element event resolved to, or the unbound prefix
/// that stopped it resolving.
fn element_namespace(resolved: ResolveResult<'_>) -> Result<Option<String>, String> {
    match resolved {
        ResolveResult::Bound(namespace) => Ok(Some(
            String::from_utf8_lossy(namespace.as_ref()).into_owned(),
        )),
        ResolveResult::Unbound => Ok(None),
        ResolveResult::Unknown(prefix) => Err(format!(
            "the prefix `{}` is not bound to any namespace",
            String::from_utf8_lossy(&prefix)
        )),
    }
}

/// A general reference's replacement text: a character reference resolves to
/// its character, and the five predefined entities to theirs.
///
/// Any other entity name needs a declaration this reader never processed (the
/// document type declaration is not part of the canonical form), so it is
/// refused rather than dropped.
fn resolved_reference(reference: &quick_xml::events::BytesRef<'_>) -> Result<String, String> {
    if let Some(character) = reference.resolve_char_ref().map_err(|e| e.to_string())? {
        return Ok(character.to_string());
    }
    let name = reference.decode().map_err(|e| e.to_string())?;
    quick_xml::escape::resolve_predefined_entity(&name)
        .map(str::to_owned)
        .ok_or_else(|| format!("the entity `&{name};` is declared by no readable declaration"))
}

/// Move the pending character data into the stream, if there is any.
fn flush(chars: &mut String, items: &mut Vec<Item>) {
    if !chars.is_empty() {
        items.push(Item::Chars(std::mem::take(chars)));
    }
}

/// One element start as a canonical item: its expanded name plus its
/// attribute set.
fn start_item(
    reader: &mut quick_xml::NsReader<&[u8]>,
    element: &BytesStart<'_>,
    namespace: Option<String>,
) -> Result<Item, String> {
    let name = Name {
        namespace,
        local: String::from_utf8_lossy(element.local_name().as_ref()).into_owned(),
    };
    let mut attributes: BTreeMap<Name, String> = BTreeMap::new();
    for attribute in element.attributes() {
        let attribute = attribute.map_err(|e| e.to_string())?;
        // Namespaces in XML §3: `xmlns` and every name beginning `xmlns:` is a
        // namespace DECLARATION, not an attribute, and what it declares is
        // already carried by the expanded names it resolves.
        let key = attribute.key.as_ref();
        if key == b"xmlns" || key.starts_with(b"xmlns:") {
            continue;
        }
        let (attribute_namespace, local) = reader.resolver_mut().resolve_attribute(attribute.key);
        let attribute_name = Name {
            namespace: element_namespace(attribute_namespace)?,
            local: String::from_utf8_lossy(local.as_ref()).into_owned(),
        };
        // Attribute-value normalization per XML 1.0 §3.3.3: references
        // resolved, tab/CR/LF folded to spaces, before the value is compared.
        let value = attribute
            .normalized_value(quick_xml::XmlVersion::Implicit1_0)
            .map_err(|e| e.to_string())?
            .into_owned();
        if attributes.insert(attribute_name.clone(), value).is_some() {
            return Err(format!(
                "the element carries two attributes with the expanded name {attribute_name} \
                 (Namespaces in XML §6.3)"
            ));
        }
    }
    Ok(Item::Start { name, attributes })
}

#[cfg(test)]
#[expect(
    clippy::panic_in_result_fn,
    reason = "the Book's Result-test shape: assertions panic, plumbing propagates with `?`"
)]
mod tests {
    use super::*;

    /// The negotiated media types of the three retrieval bindings resolve to
    /// the forms their corpus fixtures declare.
    #[test]
    fn the_negotiated_media_types_name_their_forms() {
        assert_eq!(
            DocumentForm::of_media_type("application/xml"),
            Some(DocumentForm::Xml)
        );
        assert_eq!(
            DocumentForm::of_media_type("application/xml; charset=UTF-8"),
            Some(DocumentForm::Xml)
        );
        assert_eq!(
            DocumentForm::of_media_type("TEXT/XML"),
            Some(DocumentForm::Xml)
        );
        assert_eq!(
            DocumentForm::of_media_type("application/openehr.opt+xml"),
            Some(DocumentForm::Xml)
        );
        assert_eq!(
            DocumentForm::of_media_type("text/plain; charset=utf-8"),
            Some(DocumentForm::Text)
        );
        assert_eq!(
            DocumentForm::of_corpus_format(CorpusFormat::OptXml),
            Some(DocumentForm::Xml)
        );
        assert_eq!(
            DocumentForm::of_corpus_format(CorpusFormat::Adl2Text),
            Some(DocumentForm::Text)
        );
    }

    /// A form outside the two this module reads answers NO form, so no
    /// comparison is attempted on it.
    #[test]
    fn an_unreadable_form_names_no_document_form() {
        assert_eq!(DocumentForm::of_media_type("application/json"), None);
        assert_eq!(
            DocumentForm::of_media_type("application/openehr.wt.flat+json"),
            None
        );
        assert_eq!(DocumentForm::of_media_type(""), None);
        assert_eq!(
            DocumentForm::of_corpus_format(CorpusFormat::CanonicalJson),
            None
        );
        assert_eq!(DocumentForm::of_corpus_format(CorpusFormat::RawJson), None);
    }

    /// The same document, served back unchanged, is equivalent.
    #[test]
    fn an_identical_xml_document_is_equivalent() -> Result<(), DocumentDivergence> {
        let document = concat!(
            "<?xml version=\"1.0\" encoding=\"UTF-8\"?>",
            "<template xmlns=\"http://schemas.openehr.org/v1\">",
            "<template_id><value>obs_act.en.v1</value></template_id>",
            "</template>"
        );
        compare(DocumentForm::Xml, document, document)
    }

    /// Attribute ORDER is not a difference: "the order of attribute
    /// specifications in a start-tag or empty-element tag is not significant"
    /// (XML 1.0 §3.1).
    #[test]
    fn attribute_order_is_not_a_difference() -> Result<(), DocumentDivergence> {
        compare(
            DocumentForm::Xml,
            "<e a=\"1\" b=\"2\" c=\"3\"/>",
            "<e c=\"3\" a=\"1\" b=\"2\"/>",
        )
    }

    /// An empty-element tag equals a start tag immediately followed by its
    /// end tag: "Empty-element tags may be used for any element which has no
    /// content" (XML 1.0 §3.1).
    #[test]
    fn an_empty_element_tag_equals_an_empty_element() -> Result<(), DocumentDivergence> {
        compare(DocumentForm::Xml, "<r><e/></r>", "<r><e></e></r>")
    }

    /// Whitespace INSIDE a tag is markup, not character data, so it is not a
    /// difference (XML 1.0 §3.1 start-tag production); the XML declaration is
    /// removed from the canonical form (Canonical XML 1.1 §2.3).
    #[test]
    fn markup_whitespace_and_the_declaration_are_not_differences() -> Result<(), DocumentDivergence>
    {
        compare(
            DocumentForm::Xml,
            "<?xml version=\"1.0\"?><e   a=\"1\"\n   b=\"2\" />",
            "<e a=\"1\" b=\"2\"/>",
        )
    }

    /// A namespace bound to another PREFIX resolves to the same expanded
    /// names, so it is not a difference (Namespaces in XML §2.1).
    #[test]
    fn a_reprefixed_namespace_is_not_a_difference() -> Result<(), DocumentDivergence> {
        compare(
            DocumentForm::Xml,
            "<o:e xmlns:o=\"http://schemas.openehr.org/v1\"><o:c>x</o:c></o:e>",
            "<e xmlns=\"http://schemas.openehr.org/v1\"><c>x</c></e>",
        )
    }

    /// A CDATA section is replaced by its character content (Canonical XML 1.1
    /// §2.3), and a predefined entity by the character it stands for, so the
    /// three spellings of one character sequence agree.
    #[test]
    fn cdata_and_references_resolve_to_their_characters() -> Result<(), DocumentDivergence> {
        compare(
            DocumentForm::Xml,
            "<e>a&amp;b</e>",
            "<e>a<![CDATA[&]]>b</e>",
        )?;
        compare(DocumentForm::Xml, "<e>a&#38;b</e>", "<e>a&amp;b</e>")
    }

    /// A DIFFERENT namespace is load-bearing: the same local name in another
    /// namespace is another element (Namespaces in XML §2.1).
    #[test]
    fn a_different_namespace_is_a_difference() {
        let divergence = compare(
            DocumentForm::Xml,
            "<e xmlns=\"http://schemas.openehr.org/v2\"/>",
            "<e xmlns=\"http://schemas.openehr.org/v1\"/>",
        )
        .expect_err("two namespaces are one document");
        assert!(
            matches!(divergence, DocumentDivergence::Divergent(_)),
            "{divergence:?}"
        );
        assert!(
            divergence.to_string().contains("v2"),
            "the divergence hides what was served: {divergence}"
        );
    }

    /// A differing attribute VALUE is a difference, and the message names the
    /// element path it was found at.
    #[test]
    fn a_differing_attribute_value_is_a_difference() {
        let divergence = compare(
            DocumentForm::Xml,
            "<t><d><a k=\"1\"/><a k=\"9\"/></d></t>",
            "<t><d><a k=\"1\"/><a k=\"2\"/></d></t>",
        )
        .expect_err("a changed attribute value passed");
        let reason = divergence.to_string();
        assert!(reason.contains("/t[0]/d[0]"), "no element path: {reason}");
        assert!(reason.contains("k=\"9\""), "no served value: {reason}");
    }

    /// Whitespace in CHARACTER DATA is a difference: "an XML processor MUST
    /// always pass all characters in a document that are not markup through to
    /// the application" (XML 1.0 §2.10), and AMB-111 pins retrieval as
    /// verbatim.
    #[test]
    fn character_data_whitespace_is_a_difference() {
        let divergence = compare(DocumentForm::Xml, "<e> x </e>", "<e>x</e>")
            .expect_err("content whitespace was normalized away");
        assert!(
            matches!(divergence, DocumentDivergence::Divergent(_)),
            "{divergence:?}"
        );
    }

    /// A dropped element is a difference, and the message names the parent it
    /// went missing under.
    #[test]
    fn a_dropped_element_is_a_difference() {
        let divergence = compare(DocumentForm::Xml, "<t><a/></t>", "<t><a/><b/></t>")
            .expect_err("a dropped element passed");
        let reason = divergence.to_string();
        assert!(reason.contains("/t[0]"), "no element path: {reason}");
        assert!(reason.contains("want <b>"), "no missing element: {reason}");
    }

    /// A document that agrees item for item and then STOPS is a difference:
    /// one stream is a prefix of the other, which no per-item comparison
    /// reaches.
    #[test]
    fn a_document_that_is_a_prefix_of_the_other_is_a_difference() {
        let divergence = compare(DocumentForm::Xml, "<t/>", "<t/><!--c-->")
            .expect_err("a dropped trailing comment passed");
        assert!(divergence.to_string().contains("prefix"), "{divergence}");
    }

    /// An ill-formed SERVED document is a finding against the SUT; an
    /// ill-formed FIXTURE is this instrument's own defect. The two are
    /// separate variants because they belong to separate channels.
    #[test]
    fn each_ill_formed_side_names_itself() {
        let served = compare(DocumentForm::Xml, "<e>", "<e/>")
            .expect_err("an unclosed served document passed");
        assert!(
            matches!(served, DocumentDivergence::ServedIllFormed(_)),
            "{served:?}"
        );
        let fixture =
            compare(DocumentForm::Xml, "<e/>", "<e>").expect_err("an unclosed fixture passed");
        assert!(
            matches!(fixture, DocumentDivergence::FixtureIllFormed(_)),
            "{fixture:?}"
        );
    }

    /// Text equivalence tolerates the line-break spelling HTTP grants
    /// (RFC 7231 Appendix A.2) and nothing else.
    #[test]
    fn text_tolerates_only_the_line_break_spelling() -> Result<(), DocumentDivergence> {
        compare(
            DocumentForm::Text,
            "operational_template (adl_version=2.0.6)\r\n\tlanguage\r\n",
            "operational_template (adl_version=2.0.6)\n\tlanguage\n",
        )?;
        let divergence = compare(
            DocumentForm::Text,
            "operational_template\n  language\n",
            "operational_template\n\tlanguage\n",
        )
        .expect_err("indentation was normalized away");
        let reason = divergence.to_string();
        assert!(reason.contains("line 2"), "no line number: {reason}");
        Ok(())
    }

    /// A missing trailing line end is content, and the diagnostic says so
    /// rather than reporting a line nobody can see.
    #[test]
    fn a_missing_trailing_line_end_is_a_difference() {
        let divergence = compare(DocumentForm::Text, "archetype\n", "archetype")
            .expect_err("a trailing line end was normalized away");
        assert!(
            divergence.to_string().contains("trailing line end"),
            "{divergence}"
        );
    }
}
