// SPDX-FileCopyrightText: Veredictum contributors
// SPDX-License-Identifier: Apache-2.0

//! The status and the sentence a malformed server-function call gets back.
//!
//! Every `#[server]` fn here is a publicly reachable HTTP endpoint, so its
//! refusals are part of the interface. A call whose arguments cannot be
//! decoded never reaches its handler: `server_fn` builds that response itself
//! and documents the status it uses, which is 500 for every error it carries
//! (<https://docs.rs/leptos/latest/leptos/server_fn/response/trait.Res.html>:
//! "sets the HTTP status code to `500`"). A caller's mistake therefore reads
//! as the server breaking, and on a public instrument that is the difference
//! between "you sent the wrong thing" and "this thing is down".
//!
//! The framework offers no hook for it, so the rewrite happens in a layer this
//! crate owns, over the one thing the boundary hands back: the encoded error.

use std::fmt::Write as _;

/// The decoding failures that are the CALLER's mistake.
///
/// `server_fn` encodes an error as its variant name, a `|`, and the message
/// (`ServerFnError::ser`), and these three variants are the ones raised before
/// a handler runs: an argument that will not deserialize, one that is missing
/// outright, and a body that is not the expected encoding at all
/// (<https://docs.rs/leptos/latest/leptos/prelude/enum.ServerFnErrorErr.html>).
const CALLER_FAULT: [&str; 3] = ["Args", "MissingArg", "Deserialization"];

/// What a caller gets instead of the serializer's own words.
///
/// `Some` when the encoded error names a decoding failure, carrying the
/// sentence to answer with. `None` when the error is the server's own, which
/// keeps its status and its body untouched.
#[must_use]
pub fn caller_fault(encoded: &str) -> Option<String> {
    let (variant, detail) = encoded.split_once('|')?;
    if !CALLER_FAULT.contains(&variant) {
        return None;
    }

    let mut sentence = String::from("This call could not be read.");
    if let Some(argument) = missing_field(detail) {
        let _ = write!(
            sentence,
            " The argument `{argument}` is required and was not supplied."
        );
    } else {
        sentence.push_str(" One of its arguments could not be decoded.");
    }
    sentence.push_str(
        " Every argument of a console endpoint is required: a value that is \
         not being declared has its own spelling — an empty string, or the \
         `Undeclared` member of its vocabulary — so that an omitted argument \
         can never read as a declared absence.",
    );
    Some(sentence)
}

/// The field name out of serde's own phrasing, when it carries one.
///
/// serde writes the phrase "missing field" followed by the name in backticks
/// for an absent member, which is the only shape worth reading: anything else
/// is reported as an argument that could not be decoded, rather than guessed
/// at.
fn missing_field(detail: &str) -> Option<&str> {
    let rest = detail.split_once("missing field `")?.1;
    let name = rest.split_once('`')?.0;
    (!name.is_empty()).then_some(name)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_missing_argument_is_the_callers_fault_and_names_itself() {
        let answer = caller_fault("Args|missing field `postures`")
            .expect("a decoding failure is the caller's fault");
        assert!(answer.contains("`postures`"), "{answer}");
        assert!(answer.contains("required"), "{answer}");
        assert!(
            !answer.contains("Args|"),
            "the serializer's own phrasing does not travel: {answer}"
        );
    }

    #[test]
    fn an_undecodable_argument_says_so_without_inventing_a_name() {
        let answer = caller_fault("Deserialization|invalid type: string \"x\", expected u32")
            .expect("a decoding failure is the caller's fault");
        assert!(answer.contains("could not be decoded"), "{answer}");
        assert!(
            !answer.contains("The argument `"),
            "nothing is named when serde named nothing: {answer}"
        );
    }

    #[test]
    fn a_missing_arg_variant_is_read_the_same_way() {
        assert!(caller_fault("MissingArg|no argument named `filter`").is_some());
    }

    #[test]
    fn the_servers_own_errors_are_left_alone() {
        assert_eq!(caller_fault("ServerError|the engine exited 2"), None);
        assert_eq!(caller_fault("WrappedServerError|refused"), None);
        assert_eq!(caller_fault("no separator at all"), None);
    }
}
