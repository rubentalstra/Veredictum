// SPDX-FileCopyrightText: Veredictum contributors
// SPDX-License-Identifier: Apache-2.0

//! The browser journeys (#69), driven by `scripts/ui-e2e.sh`.
//!
//! Rust-native `WebDriver` over headless Chromium, never Playwright, because
//! the console admits no hand-written JavaScript and its test suite is part of
//! the console. Every journey waits on explicit element and URL conditions,
//! and ends by reading the browser console and failing on any `SEVERE` entry,
//! hydration error or panic. Each test skips with a printed reason when
//! `UI_E2E_BASE_URL` is unset, so a plain `cargo nextest run` without a
//! composed console stays green.
//!
//! With `UI_E2E_DOCS_SHOTS=1` the same journeys also write the book's
//! screenshots (light and dark, one fixed viewport) into
//! `website/book/src/console/img/`, which is what makes every console change
//! produce a reviewable visual diff.

#![allow(
    clippy::panic,
    clippy::expect_used,
    reason = "a browser journey asserts by panicking, and the harness panics when a configured console cannot be driven"
)]
#![allow(
    clippy::print_stdout,
    clippy::print_stderr,
    reason = "the skip-with-reason and capture lines ARE this suite's report"
)]

use std::path::{Path, PathBuf};
use std::time::Duration;

use thirtyfour::ChromiumLikeCapabilities;
use thirtyfour::prelude::{By, DesiredCapabilities, ElementQueryable, WebDriver, WebElement};

/// The budget every ordinary element or URL wait allows.
const WAIT: Duration = Duration::from_secs(15);

/// The interval between polls inside a wait.
const POLL: Duration = Duration::from_millis(200);

/// Polls per wait, so a loop-shaped condition gets the same budget as
/// [`WAIT`].
const POLLS: u32 = 75;

/// The budget [`Harness::wait_hydrated`] allows. Four times [`WAIT`], because
/// the harness serves a debug-profile WASM bundle the browser has to fetch,
/// compile and run before the marker appears — an order of magnitude slower
/// than any wait that only observes rendered HTML.
const HYDRATION_WAIT: Duration = Duration::from_mins(1);

/// The one capture viewport, so a screenshot diff shows a UI change and never
/// a window-size change.
const VIEWPORT: (u32, u32) = (1440, 900);

/// The settle a themed capture needs: the console's surfaces carry
/// `transition-colors`, and a screenshot racing the token switch freezes a
/// half-themed frame. An animation wait, and the only one in this file.
const THEME_SETTLE: Duration = Duration::from_millis(700);

/// Reads a harness variable, treating an empty value as unset.
fn env(name: &str) -> Option<String> {
    std::env::var(name).ok().filter(|value| !value.is_empty())
}

/// The book's console screenshot directory, resolved from this crate's
/// manifest directory.
fn book_img_dir() -> PathBuf {
    let root = concat!(env!("CARGO_MANIFEST_DIR"), "/../..");
    Path::new(root)
        .join("website")
        .join("book")
        .join("src")
        .join("console")
        .join("img")
}

/// One browser session plus everything a journey needs from it.
struct Harness {
    /// The `WebDriver` session.
    driver: WebDriver,
    /// The console origin the journeys navigate against.
    base: String,
    /// Where failure evidence is written.
    shots_dir: PathBuf,
    /// Where the book's captures are written, `None` outside capture mode.
    book_dir: Option<PathBuf>,
    /// The journey's slug, used in evidence file names.
    journey: &'static str,
}

impl Harness {
    /// Starts a journey, or returns `None` with a printed reason when the
    /// harness environment is absent.
    ///
    /// # Panics
    /// When the environment IS set but the browser session cannot start —
    /// that is a failure, never a skip.
    async fn start(journey: &'static str) -> Option<Self> {
        let (Some(base), Some(webdriver_url)) =
            (env("UI_E2E_BASE_URL"), env("UI_E2E_WEBDRIVER_URL"))
        else {
            eprintln!(
                "SKIP {journey}: UI_E2E_BASE_URL/UI_E2E_WEBDRIVER_URL unset (run scripts/ui-e2e.sh)"
            );
            return None;
        };
        let shots_dir = PathBuf::from(
            env("UI_E2E_SHOTS_DIR").unwrap_or_else(|| String::from("target/ui-e2e/screenshots")),
        );
        std::fs::create_dir_all(&shots_dir).expect("the evidence directory");
        let book_dir = env("UI_E2E_DOCS_SHOTS").map(|_| {
            let dir = book_img_dir();
            std::fs::create_dir_all(&dir).expect("the book screenshot directory");
            dir
        });

        let mut caps = DesiredCapabilities::chrome();
        caps.add_arg("--headless=new").expect("headless");
        caps.add_arg(&format!("--window-size={},{}", VIEWPORT.0, VIEWPORT.1))
            .expect("window size");
        // A retina host would otherwise capture at 2x and every committed
        // screenshot would change with the machine that took it.
        caps.add_arg("--force-device-scale-factor=1")
            .expect("device scale");
        // Without this the browser log endpoint answers empty and the console
        // gate below silently passes on every page.
        caps.set_logging_prefs("browser", thirtyfour::LoggingPrefsLogLevel::All)
            .expect("logging prefs");
        let driver = WebDriver::new(&webdriver_url, caps)
            .await
            .expect("a WebDriver session (is the browser endpoint up?)");
        Some(Self {
            driver,
            base,
            shots_dir,
            book_dir,
            journey,
        })
    }

    /// Navigates to a console path and waits until the page has hydrated.
    ///
    /// # Panics
    /// On navigation failure, or when the page never hydrates.
    async fn goto(&self, path: &str) {
        self.goto_unhydrated(path).await;
        self.wait_hydrated().await;
    }

    /// Navigates without waiting for hydration — for the routes' 404
    /// fallback, whose assertion is the server-rendered body itself.
    ///
    /// # Panics
    /// On navigation failure.
    async fn goto_unhydrated(&self, path: &str) {
        self.driver
            .goto(format!("{}{path}", self.base))
            .await
            .expect("navigate");
    }

    /// Waits until the client has taken over the page (`app.rs` stamps
    /// `data-hydrated` on `<body>` from a browser-only effect).
    ///
    /// # Panics
    /// When the marker never appears within [`HYDRATION_WAIT`].
    async fn wait_hydrated(&self) {
        self.wait_css_for("body[data-hydrated]", HYDRATION_WAIT)
            .await;
    }

    /// Waits for the first element matching `css`, within [`WAIT`].
    ///
    /// # Panics
    /// When the element never appears — with the selector and the page it was
    /// waiting on in the message.
    async fn wait_css(&self, css: &str) -> WebElement {
        self.wait_css_for(css, WAIT).await
    }

    /// [`Self::wait_css`] with an explicit budget, so the hydration wait can
    /// be long without lengthening every other wait.
    ///
    /// # Panics
    /// When the element never appears.
    async fn wait_css_for(&self, css: &str, budget: Duration) -> WebElement {
        match self
            .driver
            .query(By::Css(css))
            .wait(budget, POLL)
            .first()
            .await
        {
            Ok(element) => element,
            Err(e) => panic!("waiting for `{css}` {}: {e}", self.evidence("wait").await),
        }
    }

    /// Waits for the first element matching `xpath`, within [`WAIT`].
    ///
    /// # Panics
    /// When the element never appears.
    async fn wait_xpath(&self, xpath: &str) -> WebElement {
        match self
            .driver
            .query(By::XPath(xpath))
            .wait(WAIT, POLL)
            .first()
            .await
        {
            Ok(element) => element,
            Err(e) => panic!(
                "waiting for xpath `{xpath}` {}: {e}",
                self.evidence("wait").await
            ),
        }
    }

    /// Waits until the current URL contains `fragment`.
    ///
    /// # Panics
    /// When it never does within [`WAIT`].
    async fn wait_url_contains(&self, fragment: &str) {
        for _ in 0..POLLS {
            let url = self.driver.current_url().await.expect("current url");
            if url.as_str().contains(fragment) {
                return;
            }
            tokio::time::sleep(POLL).await;
        }
        let url = self.driver.current_url().await.expect("current url");
        panic!("the URL never contained `{fragment}` (last: {url})");
    }

    /// Clicks the shell's theme control and waits until the root class has
    /// caught up, returning once dark mode is on.
    ///
    /// # Panics
    /// When the control is absent or the class never appears.
    async fn enable_dark(&self) {
        self.wait_css("button[aria-label='Toggle dark mode']")
            .await
            .click()
            .await
            .expect("click the theme control");
        self.wait_css("html.dark").await;
        tokio::time::sleep(THEME_SETTLE).await;
    }

    /// Writes one book screenshot under `slug`, or does nothing outside
    /// capture mode.
    ///
    /// # Panics
    /// On capture or IO failure — a capture pass that silently wrote nothing
    /// would leave the book showing a stale UI.
    async fn capture(&self, slug: &str) {
        let Some(dir) = self.book_dir.as_ref() else {
            return;
        };
        let out = dir.join(format!("{slug}.png"));
        self.driver
            .screenshot(&out)
            .await
            .expect("write the book screenshot");
        println!("captured {slug} -> {}", out.display());
    }

    /// Where the browser is, plus a failure screenshot, as one line for a
    /// panic message.
    async fn evidence(&self, slug: &str) -> String {
        let url = self
            .driver
            .current_url()
            .await
            .map(|u| u.to_string())
            .unwrap_or_default();
        let path = self.shots_dir.join(format!("{}-{slug}.png", self.journey));
        drop(self.driver.screenshot(&path).await);
        format!("at {url} (evidence: {})", path.display())
    }

    /// The standing console gate: reads the browser log and fails on any
    /// `SEVERE` entry, and on any entry naming a hydration failure or a
    /// panic at any level.
    ///
    /// `allowed` waives an entry by substring, and exists only for a journey
    /// whose own deliberate negative step produces it — a browser logs a
    /// main-document 404 as an error, and the 404 is what that journey
    /// asserts. Every other journey passes an empty slice.
    ///
    /// # Panics
    /// When the log carries such an entry.
    async fn assert_console_clean(&self, allowed: &[&str]) {
        let entries = self
            .driver
            .get_log("browser")
            .await
            .expect("the browser log (the driver's legacy log endpoint)");
        let mut findings: Vec<String> = Vec::new();
        for entry in entries {
            if allowed.iter().any(|a| entry.message.contains(a)) {
                continue;
            }
            let lowered = entry.message.to_lowercase();
            let named = lowered.contains("hydrat") || lowered.contains("panic");
            if entry.level == "SEVERE" || named {
                findings.push(format!("[{}] {}", entry.level, entry.message));
            }
        }
        let at = self
            .driver
            .current_url()
            .await
            .map(|u| u.to_string())
            .unwrap_or_default();
        assert!(
            findings.is_empty(),
            "the browser console is not clean (last page: {at}):\n{}",
            findings.join("\n")
        );
    }

    /// Ends the session.
    ///
    /// # Panics
    /// When the session cannot be closed.
    async fn finish(self) {
        self.driver.quit().await.expect("quit the session");
    }
}

/// The landing renders the four catalogue counts as filled stat cards, and
/// the numbers are the catalogue's own rather than placeholders.
#[tokio::test]
async fn e2e_landing_shows_the_catalogue_counts() {
    let Some(h) = Harness::start("landing").await else {
        return;
    };
    h.goto("/").await;
    h.wait_css("#instrument-stats").await;
    let cards = h
        .driver
        .find_all(By::Css("#instrument-stats > *"))
        .await
        .expect("the stat cards");
    assert_eq!(cards.len(), 4, "the landing renders four stat cards");
    for card in &cards {
        let text = card.text().await.expect("a stat card's text");
        let value = text.lines().next().unwrap_or_default().trim().to_owned();
        assert!(
            value.chars().all(|c| c.is_ascii_digit()) && !value.is_empty(),
            "a stat card must lead with its count, not `{value}`"
        );
    }
    // The case count is the catalogue's, so a placeholder zero is a failure.
    let cases = h
        .wait_xpath("//*[@id='instrument-stats']//a[@href='/catalogue']")
        .await
        .text()
        .await
        .expect("the case-core card");
    assert!(
        !cases.starts_with('0'),
        "the case-core count must come from the mounted catalogue (read `{cases}`)"
    );
    h.capture("landing-light").await;
    h.enable_dark().await;
    h.capture("landing-dark").await;
    h.assert_console_clean(&[]).await;
    h.finish().await;
}

/// The sidebar reaches the catalogue, a chapter listing, and one case in
/// full — the case detail naming the citations its expectation stands on.
#[tokio::test]
async fn e2e_sidebar_reaches_a_case_detail() {
    let Some(h) = Harness::start("catalogue").await else {
        return;
    };
    h.goto("/").await;
    h.wait_css("nav[aria-label='Primary'] a[href='/catalogue']")
        .await
        .click()
        .await
        .expect("click the Catalogue entry");
    h.wait_url_contains("/catalogue").await;
    let chapter = h.wait_css("a[href^='/catalogue/']").await;
    let chapter_href = chapter
        .attr("href")
        .await
        .expect("the chapter link's href")
        .expect("a chapter link carries an href");
    h.capture("catalogue-light").await;

    chapter.click().await.expect("open the chapter");
    h.wait_url_contains(&chapter_href).await;
    h.wait_css("table tbody tr").await;
    h.capture("chapter-light").await;

    h.wait_css("table tbody tr td a")
        .await
        .click()
        .await
        .expect("open the case");
    h.wait_xpath("//h2[normalize-space()='Spec citations']")
        .await;
    let citations = h
        .wait_css("h2 + ul li")
        .await
        .text()
        .await
        .expect("the first citation");
    assert!(
        !citations.trim().is_empty(),
        "a case detail lists the citations its expectation stands on"
    );
    h.capture("case-light").await;

    // Dark is captured walking the same trail backwards: the preference is
    // stored, so each navigation re-applies it after hydration.
    h.enable_dark().await;
    h.capture("case-dark").await;
    h.goto(&chapter_href).await;
    h.wait_css("html.dark").await;
    h.wait_css("table tbody tr").await;
    tokio::time::sleep(THEME_SETTLE).await;
    h.capture("chapter-dark").await;
    h.goto("/catalogue").await;
    h.wait_css("html.dark").await;
    h.wait_css("a[href^='/catalogue/']").await;
    tokio::time::sleep(THEME_SETTLE).await;
    h.capture("catalogue-dark").await;

    h.assert_console_clean(&[]).await;
    h.finish().await;
}

/// The theme control flips the root class, and the choice survives a full
/// reload — the persisted preference is re-applied after hydration.
#[tokio::test]
async fn e2e_dark_mode_persists_across_a_reload() {
    let Some(h) = Harness::start("dark-mode").await else {
        return;
    };
    h.goto("/").await;
    let roots = h
        .driver
        .find_all(By::Css("html.dark"))
        .await
        .expect("the root element");
    assert!(roots.is_empty(), "the first paint is light");
    h.enable_dark().await;
    h.goto("/").await;
    h.wait_css("html.dark").await;
    h.wait_css("button[aria-label='Toggle dark mode']")
        .await
        .click()
        .await
        .expect("click the theme control");
    // The mirror direction: turning it off must persist as well, or the
    // console would come back dark for a reader who chose light.
    for _ in 0..POLLS {
        if h.driver
            .find_all(By::Css("html.dark"))
            .await
            .unwrap_or_default()
            .is_empty()
        {
            break;
        }
        tokio::time::sleep(POLL).await;
    }
    h.goto("/").await;
    h.wait_css("#instrument-stats").await;
    let still_dark = h
        .driver
        .find_all(By::Css("html.dark"))
        .await
        .expect("the root element");
    assert!(
        still_dark.is_empty(),
        "turning dark mode off must survive a reload too"
    );
    h.assert_console_clean(&[]).await;
    h.finish().await;
}

/// An unrouted path renders the router's own 404 answer rather than a blank
/// document or a server error.
#[tokio::test]
async fn e2e_unknown_route_renders_the_fallback() {
    let Some(h) = Harness::start("not-found").await else {
        return;
    };
    h.goto_unhydrated("/no-such-surface").await;
    h.wait_xpath("//body[contains(., 'Page not found.')]").await;
    h.wait_hydrated().await;
    // The waived entry is the 404 this journey exists to provoke: the browser
    // logs a main-document 404 as a page error, and the server answering 404
    // for an unrouted path is the assertion.
    h.assert_console_clean(&["/no-such-surface - Failed to load resource"])
        .await;
    h.finish().await;
}

/// The wizard's first step renders: /run redirects to Connect, the form and
/// the auth control exist, and Scope without a draft answers honestly.
#[tokio::test(flavor = "multi_thread")]
async fn e2e_run_wizard_reaches_connect_and_scope() {
    let Some(h) = Harness::start("run-wizard").await else {
        return;
    };
    h.goto("/run/connect").await;
    h.wait_xpath("//h1[contains(., 'Grade a server')]").await;
    h.wait_xpath("//button[contains(., 'Probe connection')]")
        .await;
    h.wait_xpath("//button[contains(., 'Basic')]").await;
    h.capture("connect-light").await;
    h.enable_dark().await;
    h.capture("connect-dark").await;
    h.goto("/run/scope").await;
    h.wait_xpath("//body[contains(., 'No connection draft')]")
        .await;
    h.assert_console_clean(&[]).await;
    h.finish().await;
}

// NOTE: no openEHR spec governs the journeys — our own design; a probe/run
// journey against a composed SUT joins the record surfaces' work (#67),
// where a finished run is what the screens under test consume.
