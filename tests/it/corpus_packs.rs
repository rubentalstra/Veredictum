// SPDX-FileCopyrightText: Veredictum contributors
// SPDX-License-Identifier: Apache-2.0

//! The vendored CKM ADL 1.4 archetype pack, exercised as bytes.
//!
//! A vendored tree is 100% exercised with adjudicated skips only
//! (`.claude/rules/testing.md`). This instrument reads a corpus as a wire
//! payload and ships no ADL parser, so the exercise the pack gets here is the
//! one the instrument can perform first-hand: every file is read and decoded,
//! and checked against the dialect and the identity its `PROVENANCE.md`
//! records. The counts are pinned, so a re-vendor that silently returns fewer
//! files, a 404 body, or ADL 2 text fails instead of shrinking the pack
//! unnoticed.
//!
//! The ADL 2 pair pack and the CKM template breadth pack are not covered here:
//! each carries its own dialect and its own adjudication, tracked as their own
//! issues.

/// The pack root, under the repository root.
const PACK: &str = "artifacts/corpus/archetypes/ckm";

/// The ADL 1.4 exports the pack's `PROVENANCE.md` records as vendored. CKM
/// published 945 at vendoring time and one is held in a private incubator that
/// answers 404, which that record carries as its adjudicated refusal. Bumping
/// this number is a deliberate re-vendor, never a way to quiet a pack that
/// shrank.
const VENDORED: usize = 944;

/// The namespace the AM 1.4 archetype XML twins bind, as CKM exports them.
const XML_NAMESPACE: &str = "http://schemas.openehr.org/v1";

/// The repository root. The one package sits at it, so the manifest directory
/// IS the root.
fn pack_dir() -> std::path::PathBuf {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join(PACK)
}

/// Every file with `extension` directly under `dir`, keyed by file stem and
/// ordered by it, so two halves of the pack compare as sequences.
///
/// # Errors
/// A message when the directory cannot be read, an entry cannot be resolved, or
/// a file name is not UTF-8.
fn files_by_stem(
    dir: &std::path::Path,
    extension: &str,
) -> Result<Vec<(String, std::path::PathBuf)>, String> {
    let entries = std::fs::read_dir(dir).map_err(|e| format!("reading {}: {e}", dir.display()))?;
    let mut out: Vec<(String, std::path::PathBuf)> = Vec::new();
    for entry in entries {
        let path = entry
            .map_err(|e| format!("reading an entry of {}: {e}", dir.display()))?
            .path();
        if path.extension().is_none_or(|e| e != extension) {
            continue;
        }
        let stem = path
            .file_stem()
            .and_then(std::ffi::OsStr::to_str)
            .ok_or_else(|| format!("non-UTF-8 file name at {}", path.display()))?
            .to_owned();
        out.push((stem, path));
    }
    out.sort();
    Ok(out)
}

/// The archetype id a pack file name carries.
///
/// `ckm-archetypes.sh` suffixes a repeated CKM `resourceMainId` with `__2`,
/// `__3`, … so two rows cannot write one file name. The id inside the file
/// carries no such suffix. An archetype id may itself contain `__`
/// (`openEHR-EHR-ADMIN_ENTRY.covid__outcomes.v0`), so only an all-digit tail
/// is treated as the counter.
fn archetype_id_of(stem: &str) -> &str {
    match stem.rsplit_once("__") {
        Some((head, tail)) if !tail.is_empty() && tail.bytes().all(|b| b.is_ascii_digit()) => head,
        _ => stem,
    }
}

#[test]
fn the_ckm_pack_is_completely_vendored_and_its_record_agrees() {
    let pack = pack_dir();
    let adl = files_by_stem(&pack.join("adl14"), "adl").expect("the ADL 1.4 half");
    let xml = files_by_stem(&pack.join("xml"), "xml").expect("the XML twin half");

    assert_eq!(
        adl.len(),
        VENDORED,
        "the ADL 1.4 half holds {} files, the pin says {VENDORED} — re-vendor deliberately or \
         restore the pack, never lower the pin",
        adl.len()
    );
    assert_eq!(
        xml.len(),
        VENDORED,
        "the AM 1.4 XML twin holds {} files, the pin says {VENDORED}",
        xml.len()
    );

    // The two halves are the SAME 944 archetypes, so a twin that failed to
    // fetch cannot hide behind a matching count.
    let adl_stems: Vec<&str> = adl.iter().map(|(stem, _)| stem.as_str()).collect();
    let xml_stems: Vec<&str> = xml.iter().map(|(stem, _)| stem.as_str()).collect();
    assert_eq!(
        adl_stems, xml_stems,
        "the ADL 1.4 exports and their XML twins name different archetypes"
    );

    // The provenance record is generated from the same fetch, so a drifted
    // inventory line means the record no longer describes the tree.
    let provenance = std::fs::read_to_string(pack.join("PROVENANCE.md")).expect("PROVENANCE.md");
    let claimed = provenance
        .lines()
        .find_map(|line| line.strip_prefix("- vendored: **"))
        .and_then(|rest| rest.strip_suffix("**"))
        .expect("PROVENANCE.md carries a `- vendored: **N**` inventory line");
    assert_eq!(
        claimed,
        VENDORED.to_string(),
        "PROVENANCE.md claims {claimed} vendored files, the tree holds {VENDORED}"
    );
}

#[test]
fn every_ckm_export_is_adl14_text_named_by_the_archetype_id_inside_it() {
    let dir = pack_dir().join("adl14");
    let files = files_by_stem(&dir, "adl").expect("the ADL 1.4 half");
    assert_eq!(files.len(), VENDORED, "pack size changed");

    for (stem, path) in &files {
        let bytes = std::fs::read(path).unwrap_or_else(|e| panic!("reading {stem}: {e}"));
        let text = String::from_utf8(bytes)
            .unwrap_or_else(|e| panic!("{stem} is not valid UTF-8: {e}"))
            // CKM's export opens with a byte-order mark.
            .trim_start_matches('\u{feff}')
            .to_owned();

        let mut lines = text.lines();
        let header = lines.next().unwrap_or_default();
        assert!(
            header.starts_with("archetype ("),
            "{stem} does not open with an ADL archetype header, found `{header}`"
        );
        // The dialect claim the pack's provenance rests on: CKM publishes ADL
        // 1.4 only, and an export that ever came back as anything else would
        // silently relabel the corpus.
        assert!(
            header.contains("adl_version=1.4"),
            "{stem} does not declare adl_version=1.4, header is `{header}`"
        );

        let declared = lines.next().unwrap_or_default().trim();
        assert_eq!(
            declared,
            archetype_id_of(stem),
            "{stem} declares the archetype id `{declared}`, so the file name and its content \
             disagree"
        );
    }
}

#[test]
fn every_ckm_xml_twin_is_a_well_formed_archetype_document() {
    let dir = pack_dir().join("xml");
    let files = files_by_stem(&dir, "xml").expect("the XML twin half");
    assert_eq!(files.len(), VENDORED, "twin pack size changed");

    for (stem, path) in &files {
        let text = std::fs::read_to_string(path)
            .unwrap_or_else(|e| panic!("reading {stem} as UTF-8 text: {e}"));
        let (root, namespace, archetype_id) = read_archetype_xml(&text)
            .unwrap_or_else(|e| panic!("{stem} is not a well-formed XML document: {e}"));

        assert_eq!(
            root, "archetype",
            "{stem} has root element `{root}`, so it is not an AM archetype document"
        );
        assert_eq!(
            namespace.as_deref(),
            Some(XML_NAMESPACE),
            "{stem} binds its root to {namespace:?} rather than the exported namespace"
        );
        assert_eq!(
            archetype_id.as_deref(),
            Some(archetype_id_of(stem)),
            "{stem} carries archetype_id/value {archetype_id:?}, so the file name and its content \
             disagree"
        );
    }
}

/// Read an exported archetype document to end of input, returning its root
/// element's local name, the namespace bound to it, and the text of
/// `archetype_id/value`.
///
/// The whole document is read rather than only its first tag: a truncated or
/// ill-formed export is exactly the failure a byte-level gate exists to catch,
/// and it is invisible if reading stops at the root. Namespace resolution is
/// delegated to `quick_xml::NsReader` because a conforming document may bind
/// the namespace with any prefix.
fn read_archetype_xml(text: &str) -> Result<(String, Option<String>, Option<String>), String> {
    let mut reader = quick_xml::NsReader::from_str(text);
    let mut root: Option<(String, Option<String>)> = None;
    let mut archetype_id: Option<String> = None;
    // The element path below the root, so `archetype_id/value` is read at its
    // own position and not from a namesake elsewhere in the document.
    let mut path: Vec<String> = Vec::new();
    loop {
        let (resolved, event) = reader.read_resolved_event().map_err(|e| e.to_string())?;
        match event {
            quick_xml::events::Event::Eof => break,
            quick_xml::events::Event::Start(e) => {
                let local = String::from_utf8_lossy(e.local_name().as_ref()).into_owned();
                if root.is_none() {
                    let namespace = match resolved {
                        quick_xml::name::ResolveResult::Bound(ns) => {
                            Some(String::from_utf8_lossy(ns.as_ref()).into_owned())
                        }
                        quick_xml::name::ResolveResult::Unbound => None,
                        quick_xml::name::ResolveResult::Unknown(prefix) => {
                            return Err(format!(
                                "the root element's prefix `{}` is bound to no namespace",
                                String::from_utf8_lossy(&prefix)
                            ));
                        }
                    };
                    root = Some((local, namespace));
                } else {
                    path.push(local);
                }
            }
            quick_xml::events::Event::End(_) => {
                path.pop();
            }
            quick_xml::events::Event::Text(e) if path == ["archetype_id", "value"] => {
                archetype_id = Some(e.decode().map_err(|err| err.to_string())?.trim().to_owned());
            }
            _ => {}
        }
    }
    match root {
        Some((local, namespace)) => Ok((local, namespace, archetype_id)),
        None => Err("the document carries no element".to_owned()),
    }
}
