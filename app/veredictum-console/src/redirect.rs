// SPDX-FileCopyrightText: Veredictum contributors
// SPDX-License-Identifier: Apache-2.0

//! The answer every server-owned upload route gives: a redirect back to the
//! page that posted to it.
//!
//! A plain `<form method="post">` is the whole zero-JavaScript upload
//! mechanism, and the POST-redirect-GET answer keeps a reload from re-posting
//! it. The outcome rides the query string, so the diagnostic a reader sees is
//! addressable and shareable.

/// Percent-encodes one diagnostic for a query-parameter value.
///
/// Everything outside the unreserved set (RFC 3986 §2.3,
/// <https://www.rfc-editor.org/rfc/rfc3986#section-2.3>) is escaped, which is
/// always valid if occasionally verbose.
#[must_use]
pub fn percent_encode(value: &str) -> String {
    use std::fmt::Write as _;
    value.bytes().fold(String::new(), |mut out, byte| {
        if byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'.' | b'_' | b'~') {
            out.push(char::from(byte));
        } else {
            let _written = write!(out, "%{byte:02X}");
        }
        out
    })
}

/// The `303 See Other` answer, so a reload never re-posts an upload.
#[cfg(feature = "ssr")]
#[must_use]
pub fn see_other(location: &str) -> axum::response::Response {
    use axum::response::IntoResponse as _;
    (
        axum::http::StatusCode::SEE_OTHER,
        [(axum::http::header::LOCATION, location.to_owned())],
    )
        .into_response()
}

#[cfg(test)]
mod tests {
    use super::percent_encode;

    /// A diagnostic reaches the page intact: the unreserved set passes
    /// through, and everything that could end the value or open another
    /// parameter is escaped.
    #[test]
    fn a_diagnostic_survives_the_query_string() {
        assert_eq!(percent_encode("plain-text_1.0~ok"), "plain-text_1.0~ok");
        assert_eq!(percent_encode("a b"), "a%20b");
        assert_eq!(percent_encode("a&b=c"), "a%26b%3Dc");
        assert_eq!(percent_encode("\"quoted\""), "%22quoted%22");
        assert_eq!(percent_encode(""), "");
        // Multi-byte text is escaped byte by byte, never sliced on a boundary.
        assert_eq!(percent_encode("é"), "%C3%A9");
    }
}
