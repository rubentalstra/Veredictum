// SPDX-FileCopyrightText: Veredictum contributors
// SPDX-License-Identifier: Apache-2.0

#![no_main]
//! The closed grammars a case core, a binding and a vocabulary file are read
//! through: the `${…}` variable-reference grammar, the capture-source grammar,
//! the identifier newtypes, and the binding's wire-source and header-matcher
//! grammars.
//!
//! These are the readers a third-party catalogue reaches first. `validate
//! --root <their artifacts>` is an advertised use of the instrument, so every
//! one of them takes text nobody here authored, and each is a hand-written
//! splitter over separators that carry different meanings at different depths
//! (`${ds:<key>#<view>}`, `<outcome>.<field>[]`, `I_<INTERFACE>.<operation>`).
//! A mis-slice there is how `${row.a}` and `${row.a}b}` become the same value.
//!
//! Two properties are asserted. Nothing may panic on any input. And a
//! reference the grammar ACCEPTS must render back to text that parses to the
//! same reference, because the runner resolves against the rendered form: a
//! value that cannot survive its own `Display` addresses something else on the
//! second read.

use libfuzzer_sys::fuzz_target;

use veredictum::ids::{
    AmbiguityId, CapabilityName, CaptureName, CaseId, CorpusKey, InstanceName, OptionTag,
    RecipeName, SmOperationRef, ViewName,
};
use veredictum::model::binding::{HeaderMatcher, WireFrom};
use veredictum::refgrammar::{CaptureValueSource, Template, ValueRef};

fuzz_target!(|data: &[u8]| {
    // Every one of these readers is handed a `&str` by serde, so non-UTF-8 is
    // refused a stage earlier and feeding it here would only burn executions.
    let Ok(s) = std::str::from_utf8(data) else {
        return;
    };

    // The template is the real entry point: a body reaches `ValueRef::parse`
    // only after `Template::parse` has cut it at the first `}`, so a reference
    // examined here can never contain the terminator. That is what makes the
    // render/re-parse identity below well defined.
    if let Ok(template) = Template::parse(s) {
        assert_eq!(template.raw(), s, "a template must keep its authored text");
        for reference in template.refs() {
            let rendered = reference.to_string();
            let Ok(reparsed) = Template::parse(&rendered) else {
                panic!("an accepted reference must re-parse from its own rendering: {rendered:?}");
            };
            assert_eq!(
                reparsed.as_single_ref(),
                Some(reference),
                "a reference must render to itself: {rendered:?}"
            );
        }
    }

    // Reached directly by the binding reader for a request-template body, and
    // the one door where a `}` can appear inside the reference body.
    let _ = ValueRef::parse(s);

    if let Ok(source) = CaptureValueSource::parse(s) {
        let rendered = source.to_string();
        let Ok(reparsed) = CaptureValueSource::parse(&rendered) else {
            panic!("an accepted capture source must re-parse from its rendering: {rendered:?}");
        };
        assert_eq!(reparsed, source, "a capture source must render to itself");
    }

    // The identifier newtypes. Each stores the text it validated, so the
    // interesting one is the composite: `SmOperationRef` SPLITS its input and
    // reassembles it for `Display`, which is the shape that loses a boundary.
    if let Ok(op) = SmOperationRef::parse(s) {
        assert_eq!(
            op.to_string(),
            s,
            "an SM operation reference must recompose to the text it parsed"
        );
        assert!(op.interface().starts_with("I_"));
        assert!(!op.operation().is_empty());
    }
    for accepted in [
        CaseId::parse(s).map(|v| v.to_string()),
        CapabilityName::parse(s).map(|v| v.to_string()),
        CorpusKey::parse(s).map(|v| v.to_string()),
        AmbiguityId::parse(s).map(|v| v.to_string()),
        OptionTag::parse(s).map(|v| v.to_string()),
        ViewName::parse(s).map(|v| v.to_string()),
        RecipeName::parse(s).map(|v| v.to_string()),
        InstanceName::parse(s).map(|v| v.to_string()),
        CaptureName::parse(s).map(|v| v.to_string()),
    ]
    .into_iter()
    .flatten()
    {
        assert_eq!(accepted, s, "an accepted identifier must render to itself");
    }

    // The binding grammars. `HeaderMatcher` compiles a caller-supplied regular
    // expression for its `pattern:` form, so it is the one reader here that
    // hands untrusted text to a compiler rather than to a splitter.
    let _ = WireFrom::parse(s);
    let _ = HeaderMatcher::parse(s);
});
