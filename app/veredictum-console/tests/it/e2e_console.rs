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

use leptos::server_fn::ServerFn;
use thirtyfour::ChromiumLikeCapabilities;
use thirtyfour::prelude::{By, DesiredCapabilities, ElementQueryable, WebDriver, WebElement};
use veredictum_console::run_api::StartOutcome;
use veredictum_console::run_api::fns::{CancelRun, ProbeAndSave, SaveScope, StartRun};

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

    /// Waits for an xpath under an explicit budget (a driven run's finish).
    ///
    /// # Panics
    /// When the element never appears — with the page and a failure
    /// screenshot in the message, exactly like the CSS waits, because a
    /// timeout with no picture is the one failure this suite cannot read.
    async fn wait_xpath_for(&self, xpath: &str, budget: Duration) -> WebElement {
        match self
            .driver
            .query(By::XPath(xpath))
            .wait(budget, POLL)
            .first()
            .await
        {
            Ok(element) => element,
            Err(e) => panic!(
                "waiting {budget:?} for xpath `{xpath}` {}: {e}",
                self.evidence("wait").await
            ),
        }
    }

    /// Waits for the first element matching `xpath`, within [`WAIT`].
    ///
    /// # Panics
    /// When the element never appears.
    async fn wait_xpath(&self, xpath: &str) -> WebElement {
        self.wait_xpath_for(xpath, WAIT).await
    }

    /// The path the browser is on, origin stripped, so a journey can navigate
    /// back to an address the console itself produced.
    ///
    /// # Panics
    /// When the current URL cannot be read.
    async fn current_path(&self) -> String {
        let url = self.driver.current_url().await.expect("current url");
        let mut path = String::from(url.path());
        if let Some(query) = url.query() {
            path.push('?');
            path.push_str(query);
        }
        path
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

/// An unrouted path renders the designed 404 inside the console's own chrome
/// (#84) rather than a blank document, a bare string, or a server error.
#[tokio::test]
async fn e2e_unknown_route_renders_the_fallback() {
    let Some(h) = Harness::start("not-found").await else {
        return;
    };
    h.goto_unhydrated("/no-such-surface").await;
    // The server-rendered heading, so the page is the assertion before any
    // WASM loads; the chrome and the route home prove it is the designed
    // surface rather than the old bare string.
    h.wait_xpath("//h1[normalize-space()='Page not found']")
        .await;
    h.wait_css("nav[aria-label='Primary'] a[href='/catalogue']")
        .await;
    h.wait_xpath("//a[normalize-space()='Back to the instrument']")
        .await;
    // The path that missed is named, so the reader knows what was asked for.
    h.wait_xpath("//body[contains(., '/no-such-surface')]")
        .await;
    h.wait_hydrated().await;
    h.capture("not-found-light").await;
    h.enable_dark().await;
    h.capture("not-found-dark").await;
    // The waived entry is the 404 this journey exists to provoke: the browser
    // logs a main-document 404 as a page error, and the server answering 404
    // for an unrouted path is the assertion.
    h.assert_console_clean(&["/no-such-surface - Failed to load resource"])
        .await;
    h.finish().await;
}

/// The wizard's first two steps render: /run redirects to Connect, the form
/// and the auth control exist, and Scope carries its own selection controls.
///
/// Scope is asserted by its OWN structure, never by the absence of a draft
/// (#135). The console now keeps a draft per submitter (#389), and every
/// journey drives the SAME browser, so they are one submitter sharing one
/// draft: an assertion on the empty-draft copy would still pass or fail by
/// which journey nextest happened to run first. The heading, the claim box,
/// the filter and the two controls are on the page whatever the draft holds.
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
    h.wait_xpath("//h1[contains(., 'Scope')]").await;
    h.wait_css("textarea#statement-json").await;
    h.wait_css("input#filter").await;
    h.wait_css("input#record-exchanges").await;
    h.wait_xpath("//button[contains(., 'Preview selection')]")
        .await;
    h.wait_xpath("//button[contains(., 'Save scope')]").await;
    // #100: the tier row and its counts, and the refusal an empty selection
    // earns. Both are draft-independent: the composer refuses a claim with no
    // profile before it ever reads the draft, so this journey stays order-free.
    for tier in ["core", "standard", "options", "sec-basic"] {
        h.wait_css(&format!("input#tier-{tier}")).await;
    }
    h.wait_xpath("//label[contains(., 'CORE') and contains(., 'cases')]")
        .await;
    h.wait_xpath("//button[contains(., 'Compose the claim')]")
        .await
        .click()
        .await
        .expect("compose with nothing checked");
    h.wait_xpath("//body[contains(., 'check at least one tier')]")
        .await;
    // The connection pane resolves either way — a draft summary labelled
    // `connection`, or the honest "No connection draft" answer — so what is
    // asserted is that the Suspense RESOLVED, never which of the two it
    // landed on.
    h.wait_xpath("//body[contains(., 'connection')]").await;
    // A refused server function answers 500, which the browser logs as a
    // failed resource: the refusal above is the assertion, and this ONE
    // endpoint's status line is what driving it deliberately costs.
    h.assert_console_clean(&["/api/compose_claim"]).await;
    h.finish().await;
}

/// A minimal fixture SUT for the driven-run journey: answers every request
/// `500` deterministically, in THIS test process — the console server on the
/// same host reaches it over loopback. The thread ends with the process.
fn fixture_sut() -> Result<u16, std::io::Error> {
    use std::io::{Read as _, Write as _};
    let listener = std::net::TcpListener::bind("127.0.0.1:0")?;
    let port = listener.local_addr()?.port();
    std::thread::spawn(move || {
        for stream in listener.incoming() {
            let Ok(mut stream) = stream else { continue };
            let mut scratch = [0_u8; 4096];
            let _bytes_read = stream.read(&mut scratch);
            let _write = stream.write_all(
                b"HTTP/1.1 500 Internal Server Error\r\ncontent-length: 2\r\nconnection: close\r\n\r\nno",
            );
        }
    });
    Ok(port)
}

/// The console's own output root under the harness, where a driven run's job
/// directory (and its `export/` bundle) lands.
fn harness_out_dir() -> PathBuf {
    let root = concat!(env!("CARGO_MANIFEST_DIR"), "/../..");
    Path::new(root).join("target").join("ui-e2e").join("out")
}

/// The newest driven job's sealed bundle directory.
///
/// The journey reads the bundle off disk rather than driving a browser
/// download: a headless Chromium's download directory is harness-specific,
/// and what S9 must be given is the archive's BYTES, which the console has
/// already written where the operator can see them.
fn newest_export_dir() -> Option<PathBuf> {
    let mut jobs: Vec<PathBuf> = std::fs::read_dir(harness_out_dir())
        .ok()?
        .filter_map(|entry| {
            let entry = entry.ok()?;
            let name = entry.file_name().to_str()?.to_owned();
            name.starts_with("console-job-").then(|| entry.path())
        })
        .collect();
    jobs.sort();
    jobs.into_iter()
        .rev()
        .map(|job| job.join("export"))
        .find(|export| export.join("record-manifest.json").is_file())
}

/// Where the journey writes an upload, and what the BROWSER must be told to
/// read: a containerised browser has its own filesystem, so the harness
/// bind-mounts one directory and names it twice.
///
/// # Panics
/// When the harness set a base URL but not these — the S9 journey cannot be
/// driven at all then, and silently skipping its assertions would report a
/// green gate over an unexercised surface.
fn upload_paths() -> (PathBuf, String) {
    let host = env("UI_E2E_UPLOAD_DIR")
        .expect("UI_E2E_UPLOAD_DIR (run scripts/ui-e2e.sh, which mounts it)");
    let remote = env("UI_E2E_UPLOAD_REMOTE")
        .expect("UI_E2E_UPLOAD_REMOTE (run scripts/ui-e2e.sh, which mounts it)");
    (PathBuf::from(host), remote)
}

/// Zips a bundle directory into a file the upload control can be handed.
///
/// # Panics
/// On any archive or IO failure — a journey that silently uploaded nothing
/// would assert against the page's resting state and pass for the wrong
/// reason.
fn zip_bundle(dir: &Path, into: &Path) {
    let mut buffer = std::io::Cursor::new(Vec::new());
    let mut writer = zip::ZipWriter::new(&mut buffer);
    let options = zip::write::SimpleFileOptions::default();
    let mut names: Vec<String> = std::fs::read_dir(dir)
        .expect("read the bundle directory")
        .filter_map(|entry| entry.ok()?.file_name().to_str().map(ToOwned::to_owned))
        .collect();
    names.sort();
    for name in names {
        let body = std::fs::read(dir.join(&name)).expect("read a bundle file");
        writer.start_file(name, options).expect("start an entry");
        std::io::Write::write_all(&mut writer, &body).expect("write an entry");
    }
    writer.finish().expect("finish the archive");
    std::fs::write(into, buffer.into_inner()).expect("write the archive");
}

/// One system under test the wizard drives end to end.
struct DrivenSut<'a> {
    /// The openEHR REST base URL the connect form receives.
    base_url: &'a str,
    /// The SUT identity typed into the form (name, version).
    identity: (&'a str, &'a str),
    /// Basic credentials, when the SUT wants them.
    basic: Option<(&'a str, &'a str)>,
    /// The case-id filter for the run.
    filter: &'a str,
    /// Whether the journey ticks "Record the wire exchanges" on Scope (#96).
    record_exchanges: bool,
    /// The probe outcome the connect screen must show before continuing.
    probe_answer: &'a str,
    /// The continue control's label ("Continue" on a 2xx probe).
    continue_label: &'a str,
    /// The finish budget for the live screen's poll.
    finish_budget: Duration,
}

/// Checks CORE and composes the tier claim into the paste box (#100).
///
/// This IS the claim the journey grades. Since #465 the console offers no
/// committed declaration to load over it: the support columns of an ICS
/// proforma belong to the supplier of the implementation (ISO/IEC 9646-7), so
/// pasting or composing are the only ways one enters.
async fn compose_tier_claim(h: &Harness) {
    h.wait_css("input#tier-core")
        .await
        .click()
        .await
        .expect("check the CORE tier");
    h.wait_xpath("//button[contains(., 'Compose the claim')]")
        .await
        .click()
        .await
        .expect("compose the tier claim");
    h.wait_xpath("//p[contains(., 'Composed from the checked tiers')]")
        .await;
    // The box is driven by `prop:value`, so its DOM text never changes: the
    // live value is the property, which is what the operator reads.
    let composed = h
        .wait_css("textarea#statement-json")
        .await
        .prop("value")
        .await
        .expect("read the claim box");
    assert!(
        composed.is_some_and(|body| body.contains("urn:veredictum:console:")),
        "the composed claim must land in the paste box"
    );
}

/// Drives connect → probe → scope → start → live, returning the run's own
/// address with the run finished on the Live screen. Captures are the
/// caller's business.
async fn drive_wizard(h: &Harness, sut: &DrivenSut<'_>, scope_shot: Option<&str>) -> String {
    h.goto("/run/connect").await;
    let base = h.wait_css("input#base-url").await;
    base.clear().await.expect("clear the base URL");
    base.send_keys(sut.base_url)
        .await
        .expect("type the base URL");
    let (name, version) = sut.identity;
    let name_field = h.wait_css("input#sut-name").await;
    name_field.clear().await.expect("clear the name");
    name_field.send_keys(name).await.expect("type the name");
    let version_field = h.wait_css("input#sut-version").await;
    version_field.clear().await.expect("clear the version");
    version_field
        .send_keys(version)
        .await
        .expect("type the version");
    if let Some((user, password)) = sut.basic {
        h.wait_xpath("//button[contains(., 'Basic')]")
            .await
            .click()
            .await
            .expect("pick basic auth");
        h.wait_css("input#sut-user")
            .await
            .send_keys(user)
            .await
            .expect("type the user");
        h.wait_css("input#sut-pass")
            .await
            .send_keys(password)
            .await
            .expect("type the password");
    }
    h.wait_xpath("//button[contains(., 'Probe connection')]")
        .await
        .click()
        .await
        .expect("probe");
    h.wait_xpath(&format!("//body[contains(., '{}')]", sut.probe_answer))
        .await;
    h.wait_xpath(&format!("//a[contains(., '{}')]", sut.continue_label))
        .await
        .click()
        .await
        .expect("continue");

    h.wait_xpath("//h1[contains(., 'Scope')]").await;
    compose_tier_claim(h).await;
    h.wait_css("input#filter")
        .await
        .send_keys(sut.filter)
        .await
        .expect("type the filter");
    if sut.record_exchanges {
        h.wait_css("input#record-exchanges")
            .await
            .click()
            .await
            .expect("tick the wire-recording box");
    }
    if let Some(slug) = scope_shot {
        h.capture(slug).await;
    }
    h.wait_xpath("//button[contains(., 'Save scope')]")
        .await
        .click()
        .await
        .expect("save");
    h.wait_xpath("//p[contains(., 'Claim accepted')]").await;
    h.wait_xpath("//button[contains(., 'Start the run')]")
        .await
        .click()
        .await
        .expect("start");
    h.wait_xpath("//a[contains(., 'Watch it live')]")
        .await
        .click()
        .await
        .expect("to live");

    let address = follow_the_run(h).await;
    h.wait_xpath_for("//span[contains(., 'finished')]", sut.finish_budget)
        .await;
    address
}

/// Reads the run's own address off the live screen and reloads it mid-run,
/// returning that address.
///
/// A reload must rejoin the SAME run (#386): before the URL carried the id,
/// the page could only ask whether anything was in flight here, and answered
/// "no run is in flight" about a run that was still executing.
///
/// The permalink is asserted by its href rather than by its text, because
/// documentation capture mode pins the run id it DISPLAYS so an unchanged
/// console rewrites no screenshot; the address the page is actually serving
/// is the URL, and that is what a reload has to preserve.
///
/// # Panics
/// When the live link carries no id, when the reload lands on another run, or
/// when the screen offers no permalink at all.
async fn follow_the_run(h: &Harness) -> String {
    h.wait_xpath("//h1[contains(., 'Live run')]").await;
    let address = h.current_path().await;
    assert!(
        address.starts_with("/run/live/") && address.len() > "/run/live/".len(),
        "the live link must carry the run's id: {address}"
    );
    h.goto(&address).await;
    h.wait_xpath("//h1[contains(., 'Live run')]").await;
    h.wait_xpath("//a[starts-with(@href, '/run/live/')]").await;
    assert_eq!(
        h.current_path().await,
        address,
        "a reload mid-run follows the same run"
    );
    address
}

/// Walks the finished run's record: results with one URL-addressed detail,
/// then verdicts, capturing each under the given slugs. `wire` asserts the
/// drawer shows the recorded exchanges (#96) rather than the absence line.
async fn read_record(
    h: &Harness,
    case_needle: &str,
    wire: bool,
    results_shot: &str,
    verdicts_shot: &str,
) {
    h.goto("/run/results").await;
    h.wait_xpath("//h1[contains(., 'Results')]").await;
    h.wait_xpath(&format!("//td//a[contains(., '{case_needle}')]"))
        .await
        .click()
        .await
        .expect("open the detail");
    h.wait_xpath("//body[contains(., 'Spec citations')]").await;
    if wire {
        h.wait_xpath("//body[contains(., 'exchange 1')]").await;
        h.wait_xpath("//span[contains(., 'response body')]").await;
    }
    h.capture(results_shot).await;

    h.goto("/run/verdicts").await;
    h.wait_xpath("//h1[contains(., 'Verdicts')]").await;
    h.wait_xpath("//body[contains(., 'CONFORMANCE_REPORT.md')]")
        .await;
    h.capture(verdicts_shot).await;
}

/// The whole pipeline through the real UI: connect → probe → scope with a
/// statement → start → live to finished → results with a detail → verdicts —
/// capturing every record surface, light and dark, against the hermetic
/// in-process fixture (every request answered 500).
#[tokio::test(flavor = "multi_thread")]
async fn e2e_wizard_drives_a_run_to_its_verdicts() {
    let Some(h) = Harness::start("driven-run").await else {
        return;
    };
    let Ok(port) = fixture_sut() else {
        panic!("the fixture SUT could not bind");
    };
    let base_url = format!("http://127.0.0.1:{port}");
    let sut = DrivenSut {
        base_url: &base_url,
        identity: ("my-cdr", "unknown"),
        basic: None,
        filter: "I_EHR_SERVICE.create_ehr-main",
        record_exchanges: true,
        probe_answer: "HTTP 500",
        continue_label: "Continue anyway",
        finish_budget: Duration::from_mins(1),
    };
    let address = drive_wizard(&h, &sut, Some("scope-light")).await;
    h.capture("live-light").await;
    read_record(
        &h,
        "I_EHR_SERVICE.create_ehr-main",
        true,
        "results-light",
        "verdicts-light",
    )
    .await;

    // A link to the finished run's own id shows that run, after the reader
    // walked away to the record surfaces (#386). The permalink is asserted by
    // its href: capture mode pins the id the screen DISPLAYS, and the address
    // being served is the URL.
    h.goto(&address).await;
    h.wait_xpath("//span[contains(., 'finished')]").await;
    h.wait_xpath("//a[starts-with(@href, '/run/live/')]").await;

    // S8 and S9 ride this journey rather than a second driven one. The console
    // now holds a draft and a run per SUBMITTER (#389), and every journey
    // drives the same browser, which is one submitter: it has one draft, and
    // the per-submitter cap gives it one run in flight at a time.
    let clean_url = export_and_verify(&h).await;

    // S10 rides it too: the finished run states what it can submit, and asks
    // for the disclosure only its submitter knows (#391).
    read_submission(&h).await;

    // The dark pass re-walks the finished run's surfaces: the job state
    // persists, so each page renders the same record in the other theme.
    h.enable_dark().await;
    h.goto("/run/verdicts").await;
    h.capture("verdicts-dark").await;
    h.goto(&clean_url).await;
    h.wait_xpath("//h2[contains(., 'The check')]").await;
    h.wait_xpath("//h2[contains(., 'What this proves')]").await;
    h.capture("verify-dark").await;
    h.goto("/run/scope").await;
    h.wait_xpath("//h1[contains(., 'Scope')]").await;
    h.capture("scope-dark").await;
    h.goto(&address).await;
    h.wait_xpath("//span[contains(., 'finished')]").await;
    h.capture("live-dark").await;
    h.goto("/run/results").await;
    h.wait_xpath("//h1[contains(., 'Results')]").await;
    h.capture("results-dark").await;
    h.goto("/run/submit").await;
    h.wait_xpath("//h1[contains(., 'Submit to the registry')]")
        .await;
    h.capture("submit-dark").await;

    h.assert_console_clean(&[]).await;
    h.finish().await;
}

/// S10 — the submission screen states what the run knows and asks for the
/// disclosure the rules make mandatory.
///
/// The journey never opens a submission: the harness holds no real GitHub App,
/// and a screen that reached the API would be writing to a repository this test
/// does not own. What it proves is the browser-side half — the screen renders
/// the run's own facts and the whole disclosure form, and hydrates clean.
async fn read_submission(h: &Harness) {
    h.goto("/run/submit").await;
    h.wait_xpath("//h1[contains(., 'Submit to the registry')]")
        .await;
    h.wait_xpath("//h3[contains(., 'What the run knows')]")
        .await;
    h.wait_xpath("//body[contains(., 'registry/entries/conformance/')]")
        .await;
    h.wait_xpath("//body[contains(., 'no provenance block')]")
        .await;
    // The three fields only the submitter can answer, each present as a real
    // control rather than a sentence about one.
    for id in [
        "submitter-name",
        "sut-configuration",
        "conflict-of-interest",
    ] {
        h.wait_css(&format!("#{id}")).await;
    }
    h.wait_xpath("//button[contains(., 'Open the submission')]")
        .await;
    // The branch is the contract the re-derivation lane reads the run id out
    // of, so the screen states it rather than leaving it implied.
    let body = page_text(h).await;
    assert!(
        body.contains("console-run/"),
        "the submission screen does not name the branch it will open:\n{body}"
    );
    h.capture("submit-light").await;
}

/// A run id this console never drove says so about the RUN, and never "no run
/// is in flight", which is only true of a request that named none (#386). An
/// address that is not a run id at all lands in the same state with the parse
/// reason beside it, rather than a blank page or a panic.
#[tokio::test]
async fn e2e_an_unknown_run_id_says_so_in_its_own_words() {
    let Some(h) = Harness::start("unknown-run").await else {
        return;
    };
    // Addressed by id, so this journey is independent of whatever run another
    // journey may be driving through the same console.
    h.goto("/run/live/3f2504e0-4f89-41d3-9a0c-0305e82c3301")
        .await;
    h.wait_xpath("//body[contains(., 'knows nothing about run')]")
        .await;
    h.wait_xpath("//body[contains(., '3f2504e0-4f89-41d3-9a0c-0305e82c3301')]")
        .await;

    h.goto("/run/live/not-a-run-id").await;
    h.wait_xpath("//body[contains(., 'does not name a run')]")
        .await;
    h.wait_xpath("//h1[contains(., 'Live run')]").await;
    // The caveat is page furniture on every state of this screen (#388).
    h.wait_xpath("//body[contains(., 'keeps nothing durable')]")
        .await;

    h.assert_console_clean(&[]).await;
    h.finish().await;
}

/// Seals the finished run's record, carries the archive to the public page,
/// sees it verify clean — then tampers one document and watches the page NAME
/// the file that changed. Returns the clean result's own path.
async fn export_and_verify(h: &Harness) -> String {
    prepare_export(h).await;
    h.capture("verdicts-export-light").await;

    let bundle = newest_export_dir().expect("the console sealed a bundle");
    let (upload_dir, upload_remote) = upload_paths();
    std::fs::create_dir_all(&upload_dir).expect("the upload directory");
    zip_bundle(&bundle, &upload_dir.join("clean.zip"));

    let clean_url = upload_bundle(h, &format!("{upload_remote}/clean.zip")).await;
    h.wait_xpath("//body[contains(., 'The bundle verifies')]")
        .await;
    // The honesty box is page furniture on EVERY outcome, including a clean
    // one — that is the whole point of it.
    h.wait_xpath("//h2[contains(., 'What this proves')]").await;
    h.wait_xpath("//body[contains(., 'not the run')]").await;
    // The check's own provenance table, so a clean verdict names the origin
    // and the instrument it rests on and not merely that something verified.
    h.wait_xpath("//h2[contains(., 'The check')]").await;
    h.wait_xpath("//body[contains(., 'Every file the manifest names')]")
        .await;
    h.capture("verify-light").await;

    // The tamper: one document edited, everything else identical.
    let forged_dir = upload_dir.join("forged");
    std::fs::create_dir_all(&forged_dir).expect("the forged bundle directory");
    let mut tampered: Option<String> = None;
    for entry in std::fs::read_dir(&bundle).expect("read the bundle") {
        let entry = entry.expect("a bundle entry");
        let name = entry.file_name().to_string_lossy().into_owned();
        let mut body = std::fs::read(entry.path()).expect("read a bundle file");
        if Path::new(&name).extension() == Some(std::ffi::OsStr::new("md")) && tampered.is_none() {
            body.extend_from_slice(b"\nnot what was signed\n");
            tampered = Some(name.clone());
        }
        std::fs::write(forged_dir.join(&name), body).expect("write a forged file");
    }
    let tampered = tampered.expect("the judgement rendered a markdown document");
    zip_bundle(&forged_dir, &upload_dir.join("forged.zip"));

    upload_bundle(h, &format!("{upload_remote}/forged.zip")).await;
    h.wait_xpath("//body[contains(., 'does NOT verify')]").await;
    // The finding must NAME the file, which is what makes a tamper report
    // actionable rather than a shrug.
    h.wait_xpath(&format!("//body[contains(., '{tampered}')]"))
        .await;
    h.wait_xpath("//span[contains(., 'mismatched')]").await;

    clean_url
}

/// Prepares the export on the verdicts screen and returns once the sealed
/// bundle's facts are on the page.
async fn prepare_export(h: &Harness) {
    h.goto("/run/verdicts").await;
    h.wait_xpath("//h2[contains(., 'Export the signed record')]")
        .await;
    h.wait_xpath("//button[contains(., 'Prepare the export')]")
        .await
        .click()
        .await
        .expect("prepare the export");
    // The sealed facts, not the button's own label: the seal is what the
    // journey is asserting happened.
    h.wait_xpath_for("//dt[contains(., 'Record digest')]", Duration::from_mins(2))
        .await;
    h.wait_xpath("//a[contains(., 'Download the bundle')]")
        .await;
}

/// Uploads one archive through the plain form and returns the resulting URL.
async fn upload_bundle(h: &Harness, archive: &str) -> String {
    h.goto("/verify").await;
    // WebDriver's own file-upload path: setting the value of a file input
    // sends the local file. No script, and nothing the page itself does.
    h.wait_css("input#bundle")
        .await
        .send_keys(archive)
        .await
        .expect("choose the bundle");
    h.wait_xpath("//button[contains(., 'Verify the bundle')]")
        .await
        .click()
        .await
        .expect("submit the bundle");
    h.wait_url_contains("bundle=").await;
    h.wait_xpath("//h2[contains(., 'The check')]").await;
    let url = h
        .driver
        .current_url()
        .await
        .map(|url| url.to_string())
        .expect("the verification URL");
    // The console-relative path, so a later navigation does not depend on how
    // the harness spells the origin.
    url.split_once("://")
        .and_then(|(_, rest)| rest.split_once('/'))
        .map_or_else(|| String::from("/verify"), |(_, tail)| format!("/{tail}"))
}

/// The committed bench fixtures, in the order the journey uploads them.
const BENCH_FIXTURES: [(&str, &str); 2] = [
    ("bench-result-alpha.json", "Alpha CDR 3.1"),
    ("bench-result-beta.json", "Beta CDR 2.0"),
];

/// Copies one committed bench fixture where the BROWSER can read it, and
/// returns the path to type into the file control.
///
/// # Panics
/// On any IO failure — a journey that silently uploaded nothing would assert
/// against the page's resting state and pass for the wrong reason.
fn stage_bench_fixture(name: &str) -> String {
    let (host, remote) = upload_paths();
    std::fs::create_dir_all(&host).expect("the upload directory");
    let source = Path::new(concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/bench")).join(name);
    let _copied = std::fs::copy(source, host.join(name)).expect("stage the bench fixture");
    format!("{remote}/{name}")
}

/// Uploads one bench record through the plain form and waits for the listing.
///
/// # Panics
/// When the control is absent or the upload never lands.
async fn upload_bench_record(h: &Harness, name: &str, label: &str) {
    let staged = stage_bench_fixture(name);
    h.goto("/benchmarks").await;
    h.wait_css("input#records")
        .await
        .send_keys(&staged)
        .await
        .expect("choose the bench record");
    h.wait_xpath("//button[contains(., 'Read the records')]")
        .await
        .click()
        .await
        .expect("submit the bench record");
    h.wait_url_contains("uploaded=").await;
    h.wait_xpath(&format!("//td//a[contains(., '{label}')]"))
        .await;
}

/// The benchmark surface (#166): upload a record, read it in full, then align
/// two of them and see every mismatch stated.
///
/// The fixtures are committed records, so the journey needs no bench run and
/// no CDR: what it exercises is the console's reading of the published
/// bench-result family.
#[tokio::test(flavor = "multi_thread")]
async fn e2e_bench_records_upload_render_and_compare() {
    let Some(h) = Harness::start("benchmarks").await else {
        return;
    };
    for (name, label) in BENCH_FIXTURES {
        upload_bench_record(&h, name, label).await;
    }
    // The boundary statement is furniture on every bench surface, verbatim
    // from the records: a table of speed numbers is exactly the artifact
    // somebody quotes out of context.
    h.wait_xpath("//h2[contains(., 'What a bench record is')]")
        .await;
    h.wait_xpath("//body[contains(., 'not a conformance record')]")
        .await;
    h.capture("benchmarks-light").await;

    // One record in full: the posture labels, the percentile table, the
    // failed-arrival readings, the baseline and the relative index.
    h.wait_xpath("//td//a[contains(., 'Alpha CDR 3.1')]")
        .await
        .click()
        .await
        .expect("open the record");
    h.wait_url_contains("record=").await;
    for heading in [
        "Posture `minimal`",
        "Cross-repetition percentiles",
        "Failed-arrival share",
        "Same-machine baselines",
        "Relative index",
    ] {
        h.wait_xpath(&format!("//h2[contains(., '{heading}')]"))
            .await;
    }
    // Every figure says which discipline produced it, and the histograms are
    // honestly named as tabulated rather than drawn.
    h.wait_xpath("//body[contains(., 'open-loop')]").await;
    h.wait_xpath("//body[contains(., 'closed-loop')]").await;
    h.wait_xpath("//body[contains(., 'declared-only')]").await;
    h.wait_xpath("//body[contains(., 'HdrHistogram V2')]").await;
    h.capture("benchmark-detail-light").await;

    // Two records aligned, each toggled from its own row.
    h.goto("/benchmarks").await;
    for (_, label) in BENCH_FIXTURES {
        h.wait_xpath(&format!(
            "//tr[.//a[contains(., '{label}')]]//a[normalize-space()='compare']"
        ))
        .await
        .click()
        .await
        .expect("select the record for comparison");
        h.wait_url_contains("compare=").await;
    }
    let aligned = h.wait_xpath("//h2[contains(., 'Side by side')]").await;
    // The mismatch warnings are the point of the view: two records taken on
    // different machines under different disclosures are not one ranking.
    h.wait_xpath("//body[contains(., 'DIFFERENT hosts')]").await;
    h.wait_xpath("//body[contains(., 'DIFFERENT postures')]")
        .await;
    h.wait_xpath("//body[contains(., 'not submittable')]").await;
    // The capture is of the ALIGNED table, which sits below the listing that
    // selected it; without this the book would show the listing twice.
    aligned
        .scroll_into_view()
        .await
        .expect("bring the aligned table into the viewport");
    tokio::time::sleep(THEME_SETTLE).await;
    h.capture("benchmark-compare-light").await;

    h.enable_dark().await;
    h.goto("/benchmarks").await;
    h.wait_css("html.dark").await;
    h.wait_xpath("//h2[contains(., 'What a bench record is')]")
        .await;
    tokio::time::sleep(THEME_SETTLE).await;
    h.capture("benchmarks-dark").await;

    h.assert_console_clean(&[]).await;
    h.finish().await;
}

/// The side-by-side grading (#99): the same wizard against two REAL CDRs —
/// FerroEHR's quickstart and EHRbase's official pairing, both latest — so the
/// captures show two records over one catalogue. Skips unless the harness
/// composed the SUTs (`UI_E2E_REAL_SUTS=1 scripts/ui-e2e.sh`).
#[tokio::test(flavor = "multi_thread")]
async fn e2e_wizard_grades_the_real_cdrs() {
    let ferroehr = std::env::var("UI_E2E_FERROEHR_URL").unwrap_or_default();
    let ehrbase = std::env::var("UI_E2E_EHRBASE_URL").unwrap_or_default();
    if ferroehr.is_empty() || ehrbase.is_empty() {
        println!(
            "skipping: UI_E2E_FERROEHR_URL / UI_E2E_EHRBASE_URL are unset (run with UI_E2E_REAL_SUTS=1)"
        );
        return;
    }
    let Some(h) = Harness::start("real-cdrs").await else {
        return;
    };
    let suts = [
        DrivenSut {
            base_url: &ferroehr,
            identity: ("FerroEHR", "latest"),
            basic: Some(("ferroehr", "ferroehr")),
            filter: "I_EHR_SERVICE.",
            record_exchanges: false,
            probe_answer: "The server answered",
            continue_label: "Continue",
            finish_budget: Duration::from_mins(5),
        },
        DrivenSut {
            base_url: &ehrbase,
            identity: ("EHRbase", "latest"),
            basic: Some(("ehrbase-user", "SuperSecretPassword")),
            filter: "I_EHR_SERVICE.",
            record_exchanges: false,
            probe_answer: "The server answered",
            continue_label: "Continue",
            finish_budget: Duration::from_mins(5),
        },
    ];
    for sut in &suts {
        let slug = sut.identity.0.to_lowercase();
        drive_wizard(&h, sut, None).await;
        h.capture(&format!("live-{slug}-light")).await;
        read_record(
            &h,
            "I_EHR_SERVICE.",
            false,
            &format!("results-{slug}-light"),
            &format!("verdicts-{slug}-light"),
        )
        .await;
    }

    h.assert_console_clean(&[]).await;
    h.finish().await;
}
/// A fixture SUT that answers `500` after `delay`, so a run driven against it
/// is still in flight while a journey navigates.
///
/// The concurrency journey asserts that two runs are executing AT THE SAME
/// TIME, and a run that ends before the next navigation cannot show that.
fn slow_fixture_sut(delay: Duration) -> Result<u16, std::io::Error> {
    use std::io::{Read as _, Write as _};
    let listener = std::net::TcpListener::bind("127.0.0.1:0")?;
    let port = listener.local_addr()?.port();
    std::thread::spawn(move || {
        for stream in listener.incoming() {
            let Ok(mut stream) = stream else { continue };
            std::thread::spawn(move || {
                let mut scratch = [0_u8; 4096];
                let _bytes_read = stream.read(&mut scratch);
                std::thread::sleep(delay);
                let _write = stream.write_all(
                    b"HTTP/1.1 500 Internal Server Error\r\ncontent-length: 2\r\nconnection: close\r\n\r\nno",
                );
            });
        }
    });
    Ok(port)
}

/// A visitor beside the browser: the same public server-function endpoints
/// the browser posts to, called under a client address of this journey's own.
///
/// The console reads a forwarded address only from the header its operator
/// named, so this identity exists only because `scripts/ui-e2e.sh` names one.
/// The browser sends no such header and keeps its socket peer, which is what
/// makes these calls a different submitter on one process.
struct ApiVisitor {
    /// The console's host-side origin (the browser's own origin may name a
    /// host only the browser container can resolve).
    origin: String,
    /// The header the console trusts for the client address.
    header: String,
    /// The address this visitor claims.
    address: &'static str,
    /// The HTTP client.
    client: reqwest::Client,
}

impl ApiVisitor {
    /// Builds a visitor claiming `address`, or returns `None` with a printed
    /// reason when the harness did not configure a trusted header.
    fn new(address: &'static str) -> Option<Self> {
        let (Some(origin), Some(header)) = (env("UI_E2E_HOST_URL"), env("UI_E2E_CLIENT_IP_HEADER"))
        else {
            eprintln!(
                "SKIP two-visitors: UI_E2E_HOST_URL/UI_E2E_CLIENT_IP_HEADER unset (run scripts/ui-e2e.sh)"
            );
            return None;
        };
        Some(Self {
            origin,
            header,
            address,
            client: reqwest::Client::new(),
        })
    }

    /// Posts one URL-encoded server-function call and returns its JSON body.
    ///
    /// # Panics
    /// On any transport failure or non-success status — a journey that
    /// silently skipped its second visitor would assert nothing.
    async fn call(&self, path: &str, form: &str) -> String {
        let url = format!("{}{path}", self.origin);
        let response = self
            .client
            .post(&url)
            .header(self.header.as_str(), self.address)
            .header("accept", "application/json")
            .header("content-type", "application/x-www-form-urlencoded")
            .body(form.to_owned())
            .send()
            .await
            .unwrap_or_else(|e| panic!("POST {url}: {e}"));
        let status = response.status();
        let body = response.text().await.unwrap_or_default();
        assert!(status.is_success(), "POST {url} answered {status}: {body}");
        body
    }

    /// Posts `form` and returns the status with the body, refusing nothing.
    ///
    /// The sibling `call` asserts success, which is right for every step of a
    /// journey; a refusal is the subject here rather than a failure.
    async fn call_expecting_a_refusal(
        &self,
        path: &str,
        form: &str,
    ) -> (reqwest::StatusCode, String) {
        let url = format!("{}{path}", self.origin);
        let response = self
            .client
            .post(&url)
            .header(self.header.as_str(), self.address)
            .header("accept", "application/json")
            .header("content-type", "application/x-www-form-urlencoded")
            .body(form.to_owned())
            .send()
            .await
            .unwrap_or_else(|e| panic!("POST {url}: {e}"));
        let status = response.status();
        (status, response.text().await.unwrap_or_default())
    }

    /// Drives connect, scope and start as this visitor, returning the run's
    /// own address.
    ///
    /// # Panics
    /// When the console does not accept the run — the second visitor has no
    /// run in flight, so anything but an acceptance is a real failure.
    async fn start_a_run(&self, sut_base_url: &str, sut_name: &str, filter: &str) -> String {
        self.call(
            <ProbeAndSave as ServerFn>::PATH,
            &format!(
                "base_url={}&sut_name={sut_name}&sut_version=0.0.0-gate&auth=None&user=&password=&token=",
                encode(sut_base_url)
            ),
        )
        .await;
        // `postures` is a REQUIRED argument, so this visitor declares one like
        // any other caller: an undeclared posture is a stated absence rather
        // than an omitted field. A form that leaves it out is refused, which is
        // the behaviour the console wants and the reason this is spelled here
        // instead of the argument being made optional.
        self.call(
            <SaveScope as ServerFn>::PATH,
            &format!(
                "filter={}&record_exchanges=false\
                 &postures[system_id]=&postures[dump_location]=\
                 &postures[signing]=Undeclared&postures[digest_encoding]=Base64\
                 &postures[digest_prefix]=&postures[pgp_public_key]=\
                 &postures[spec_profile]=Undeclared",
                encode(filter)
            ),
        )
        .await;
        let answer = self.call(<StartRun as ServerFn>::PATH, "").await;
        let outcome: StartOutcome = serde_json::from_str(&answer)
            .unwrap_or_else(|e| panic!("start answered `{answer}`: {e}"));
        match outcome {
            StartOutcome::Accepted(id) => format!("/run/live/{id}"),
            StartOutcome::AlreadyInFlight(id) => {
                panic!("the second visitor already had run {id} in flight")
            }
        }
    }

    /// Cancels the run at `address`, so the journey leaves no slot occupied.
    ///
    /// Cancel addresses a run by id and is not scoped to whoever started it,
    /// which is the same property that lets anyone holding a run's URL read
    /// it — so this ends both visitors' runs.
    async fn stop(&self, address: &str) {
        let Some(id) = address.rsplit('/').next() else {
            return;
        };
        drop(
            self.client
                .post(format!("{}{}", self.origin, <CancelRun as ServerFn>::PATH))
                .header(self.header.as_str(), self.address)
                .header("accept", "application/json")
                .header("content-type", "application/x-www-form-urlencoded")
                .body(format!("id={id}"))
                .send()
                .await,
        );
    }
}

/// Percent-encodes a form value's reserved characters.
///
/// Only what these calls actually carry: a URL and a case-id filter. Anything
/// outside the unreserved set becomes `%XX`, which is always correct even
/// where it was not required.
fn encode(value: &str) -> String {
    use std::fmt::Write as _;
    let mut out = String::with_capacity(value.len());
    for byte in value.bytes() {
        if byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.' | b'~') {
            out.push(char::from(byte));
        } else {
            let _ = write!(out, "%{byte:02X}");
        }
    }
    out
}

/// Reads the whole rendered page as text.
///
/// # Panics
/// When the body cannot be read.
async fn page_text(h: &Harness) -> String {
    h.wait_css("body")
        .await
        .text()
        .await
        .expect("the page's text")
}

/// Several cases against a two-second SUT: both runs of the concurrency
/// journey stay in flight for its whole length, and both are cancelled before
/// it ends.
const TWO_VISITOR_FILTER: &str = "I_EHR_SERVICE.create_ehr";

/// The #389 journey: two runs drive at once on ONE console, each followed by
/// its own URL, neither screen showing the other's progress or outcome, and a
/// third start waiting in a queue that states its place.
///
/// The three visitors are the browser (its socket peer) and two sets of HTTP
/// calls under forwarded addresses the harness told the console to trust. The
/// screens are compared by the SUT display name and by the served URL, never
/// by the run id the page prints: documentation capture mode pins that id, so
/// every live screen displays the same one.
#[tokio::test(flavor = "multi_thread")]
async fn e2e_two_visitors_drive_two_runs_at_once() {
    let Some(h) = Harness::start("two-visitors").await else {
        return;
    };
    let (Some(second), Some(third)) = (
        ApiVisitor::new("203.0.113.77"),
        ApiVisitor::new("203.0.113.78"),
    ) else {
        h.finish().await;
        return;
    };
    let Ok(port) = slow_fixture_sut(Duration::from_secs(2)) else {
        panic!("the fixture SUT could not bind");
    };
    let base_url = format!("http://127.0.0.1:{port}");

    // Visitor one is the browser, driving the wizard.
    let first = drive_to_live(&h, &base_url, "first-visitor", TWO_VISITOR_FILTER).await;
    h.wait_xpath("//span[contains(., 'running')]").await;

    // A malformed call is the CALLER's mistake and answers as one (#484).
    // Every `#[server]` fn is a public endpoint, so this is part of the
    // interface: the same form with `postures` left out, which server_fn
    // answers with 500 and its own serializer's phrasing unless something
    // rewrites it.
    let (status, body) = second
        .call_expecting_a_refusal(
            <SaveScope as ServerFn>::PATH,
            "filter=&record_exchanges=false",
        )
        .await;
    assert_eq!(
        status,
        reqwest::StatusCode::BAD_REQUEST,
        "a caller's mistake is 4xx, not 5xx: {body}"
    );
    assert!(
        body.contains("`postures`"),
        "the refusal names the argument: {body}"
    );
    assert!(
        !body.contains("Args|"),
        "the serializer's own phrasing does not reach a caller: {body}"
    );

    // Visitor two is this journey, over the same public endpoints.
    let other = second
        .start_a_run(&base_url, "second-visitor", TWO_VISITOR_FILTER)
        .await;
    assert_ne!(other, first, "two runs, two addresses");

    // Each address answers about its own run and says nothing of the other.
    h.goto(&other).await;
    assert_eq!(h.current_path().await, other, "the URL is the run served");
    h.wait_xpath("//h2[contains(., 'second-visitor')]").await;
    h.wait_xpath("//span[contains(., 'running')]").await;
    let second_page = page_text(&h).await;
    assert!(
        !second_page.contains("first-visitor"),
        "the second run's screen showed the first run:\n{second_page}"
    );

    h.goto(&first).await;
    assert_eq!(h.current_path().await, first, "the URL is the run served");
    h.wait_xpath("//h2[contains(., 'first-visitor')]").await;
    h.wait_xpath("//span[contains(., 'running')]").await;
    let first_page = page_text(&h).await;
    assert!(
        !first_page.contains("second-visitor"),
        "the first run's screen showed the second run:\n{first_page}"
    );
    // The permalink is asserted by its href SHAPE and the served address by
    // the URL: capture mode pins the id the page displays, and it pins the
    // same one for both runs.
    h.wait_xpath("//a[starts-with(@href, '/run/live/')]").await;

    // Visitor three arrives at a full instance: accepted, addressable at once,
    // and told its place rather than left on a spinner.
    let waiting = third
        .start_a_run(&base_url, "third-visitor", TWO_VISITOR_FILTER)
        .await;
    h.goto(&waiting).await;
    assert_eq!(h.current_path().await, waiting, "the URL is the run served");
    h.wait_xpath("//h2[contains(., 'third-visitor')]").await;
    h.wait_xpath("//span[contains(., 'queued') and contains(., 'position 1')]")
        .await;
    h.wait_xpath("//button[contains(., 'Leave the queue')]")
        .await;

    // No run may outlive the journey: the instance has two slots, and the
    // queued one is dropped first so freeing a slot promotes nothing. Every
    // run is ended by ADDRESS rather than through the screen's own control,
    // because documentation capture mode pins the id the live view carries
    // and that control would dispatch the pinned one.
    h.wait_xpath("//h1[contains(., 'Live run')]").await;
    third.stop(&waiting).await;
    second.stop(&first).await;
    second.stop(&other).await;

    h.assert_console_clean(&[]).await;
    h.finish().await;
}

/// Drives connect → probe → scope → start → live in the browser, returning
/// the run's own address with the run still driving.
///
/// A lean sibling of [`drive_wizard`]: no claim is composed and nothing is
/// waited on to finish, because what this journey needs is a run that is
/// still executing.
///
/// # Panics
/// On any control the wizard does not offer, or an address carrying no id.
async fn drive_to_live(h: &Harness, base_url: &str, sut_name: &str, filter: &str) -> String {
    h.goto("/run/connect").await;
    let base = h.wait_css("input#base-url").await;
    base.clear().await.expect("clear the base URL");
    base.send_keys(base_url).await.expect("type the base URL");
    let name_field = h.wait_css("input#sut-name").await;
    name_field.clear().await.expect("clear the name");
    name_field.send_keys(sut_name).await.expect("type the name");
    h.wait_xpath("//button[contains(., 'Probe connection')]")
        .await
        .click()
        .await
        .expect("probe");
    h.wait_xpath("//body[contains(., 'HTTP 500')]").await;
    h.wait_xpath("//a[contains(., 'Continue anyway')]")
        .await
        .click()
        .await
        .expect("continue");

    h.wait_xpath("//h1[contains(., 'Scope')]").await;
    let filter_field = h.wait_css("input#filter").await;
    filter_field.clear().await.expect("clear the filter");
    filter_field
        .send_keys(filter)
        .await
        .expect("type the filter");
    h.wait_xpath("//button[contains(., 'Save scope')]")
        .await
        .click()
        .await
        .expect("save");
    h.wait_xpath("//body[contains(., 'Scope saved')]").await;
    h.wait_xpath("//button[contains(., 'Start the run')]")
        .await
        .click()
        .await
        .expect("start");
    h.wait_xpath("//a[contains(., 'Watch it live')]")
        .await
        .click()
        .await
        .expect("to live");
    h.wait_xpath("//h1[contains(., 'Live run')]").await;
    let address = h.current_path().await;
    assert!(
        address.starts_with("/run/live/") && address.len() > "/run/live/".len(),
        "the live link must carry the run's id: {address}"
    );
    address
}
