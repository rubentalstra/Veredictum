// SPDX-FileCopyrightText: Veredictum contributors
// SPDX-License-Identifier: Apache-2.0

//! The console's deployment posture (#390): whose network the instance sits
//! in, and therefore what it may be pointed at.
//!
//! An operator running the console on their laptop drives a CDR at
//! `localhost` all day, and that is the normal case. A public instance drives
//! whatever endpoint a visitor names, so the same request reaches addresses
//! only that instance can see. One posture keeps both honest: the local one
//! refuses nothing, and the hosted one refuses the private families
//! [`crate::target_safety`] enumerates.
//!
//! The posture is read once at startup beside the mounts
//! ([`crate::state::ConsoleState::load`]), never per request.

/// The environment variable naming the posture (`local` or `hosted`).
///
/// Unset is `local`, which is what a laptop run and every gate already are.
pub const POSTURE_ENV: &str = "VEREDICTUM_POSTURE";

/// The token selecting [`Posture::Local`].
pub const LOCAL_TOKEN: &str = "local";

/// The token selecting [`Posture::Hosted`].
pub const HOSTED_TOKEN: &str = "hosted";

/// Where this instance runs, and therefore what it may be pointed at.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Posture {
    /// An operator's own machine. The instance reaches only what its operator
    /// already reaches, so it refuses no target at all.
    #[default]
    Local,
    /// A public instance anyone may drive. A visitor-named target that is
    /// only reachable from inside this instance's network is refused before a
    /// socket opens.
    Hosted,
}

impl Posture {
    /// Whether this posture refuses the private address families.
    #[must_use]
    pub fn guards_targets(self) -> bool {
        matches!(self, Self::Hosted)
    }

    /// The token this posture is named by.
    #[must_use]
    pub fn token(self) -> &'static str {
        match self {
            Self::Local => LOCAL_TOKEN,
            Self::Hosted => HOSTED_TOKEN,
        }
    }
}

/// A [`POSTURE_ENV`] value that names no posture.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error(
    "{POSTURE_ENV}={value:?} names no posture: set it to \"{LOCAL_TOKEN}\" or \"{HOSTED_TOKEN}\", or leave it unset for \"{LOCAL_TOKEN}\""
)]
pub struct UnknownPosture {
    /// The value the environment carried, verbatim.
    pub value: String,
}

impl std::str::FromStr for Posture {
    type Err = UnknownPosture;

    /// Reads a posture token, ignoring surrounding whitespace and case.
    ///
    /// # Errors
    /// [`UnknownPosture`] for anything but the two tokens. An empty or
    /// whitespace-only value reads as unset, which is [`Posture::Local`]:
    /// a container that exports the variable with no value has named nothing,
    /// and that is not a misspelling of `hosted`.
    fn from_str(value: &str) -> Result<Self, Self::Err> {
        let token = value.trim().to_ascii_lowercase();
        match token.as_str() {
            "" | LOCAL_TOKEN => Ok(Self::Local),
            HOSTED_TOKEN => Ok(Self::Hosted),
            _ => Err(UnknownPosture {
                value: value.to_owned(),
            }),
        }
    }
}

/// The posture [`POSTURE_ENV`] names, or [`Posture::Local`] when it is unset.
///
/// # Errors
/// [`UnknownPosture`] when the variable carries anything else. Every other
/// missing or unreadable mount in this console is a first-class state the
/// screens explain, and this one is the exception on purpose: a public
/// instance that fell back to `local` on a typo would drive whatever address
/// a visitor named, so the value is refused at startup instead of at the
/// first run.
pub fn from_env() -> Result<Posture, UnknownPosture> {
    match std::env::var(POSTURE_ENV) {
        Ok(value) => value.parse(),
        Err(_) => Ok(Posture::Local),
    }
}

#[cfg(test)]
#[expect(
    clippy::panic_in_result_fn,
    reason = "the Book ch11 Result-returning test shape: assertions panic, plumbing propagates with ? (https://doc.rust-lang.org/book/ch11-01-writing-tests.html)"
)]
mod tests {
    use super::{HOSTED_TOKEN, LOCAL_TOKEN, POSTURE_ENV, Posture, UnknownPosture};

    /// Both tokens read, in any case and with any surrounding whitespace, and
    /// an absent value is the local posture.
    #[test]
    fn the_two_tokens_read() -> Result<(), UnknownPosture> {
        assert_eq!(LOCAL_TOKEN.parse::<Posture>()?, Posture::Local);
        assert_eq!(HOSTED_TOKEN.parse::<Posture>()?, Posture::Hosted);
        assert_eq!(" Hosted \n".parse::<Posture>()?, Posture::Hosted);
        assert_eq!("  LOCAL ".parse::<Posture>()?, Posture::Local);
        assert_eq!("".parse::<Posture>()?, Posture::Local);
        assert_eq!("   ".parse::<Posture>()?, Posture::Local);
        assert_eq!(Posture::default(), Posture::Local);
        Ok(())
    }

    /// Only the hosted posture guards targets, and each posture names itself.
    #[test]
    fn only_the_hosted_posture_guards() {
        assert!(Posture::Hosted.guards_targets());
        assert!(!Posture::Local.guards_targets());
        assert_eq!(Posture::Hosted.token(), HOSTED_TOKEN);
        assert_eq!(Posture::Local.token(), LOCAL_TOKEN);
    }

    /// A misspelling is refused rather than read as the weaker posture, and
    /// the refusal names both accepted tokens so the operator can fix it from
    /// the message alone.
    #[test]
    fn a_misspelling_is_refused_by_name() {
        for value in ["hostedd", "host ed", "public", "0", "loc al"] {
            let outcome = value.parse::<Posture>();
            assert_eq!(
                outcome,
                Err(UnknownPosture {
                    value: value.to_owned()
                }),
                "{value:?}"
            );
            let said =
                outcome.map_or_else(|refusal| refusal.to_string(), |ok| ok.token().to_owned());
            assert!(said.contains(POSTURE_ENV), "{said}");
            assert!(said.contains(LOCAL_TOKEN), "{said}");
            assert!(said.contains(HOSTED_TOKEN), "{said}");
        }
    }
}
