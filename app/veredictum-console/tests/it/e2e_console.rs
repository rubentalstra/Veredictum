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
    /// Waits for an xpath under an explicit budget (a driven run's finish).
    async fn wait_xpath_for(&self, xpath: &str, budget: Duration) -> WebElement {
        match self
            .driver
            .query(By::XPath(xpath))
            .wait(budget, POLL)
            .first()
            .await
        {
            Ok(element) => element,
            Err(e) => panic!("waiting {budget:?} for xpath `{xpath}`: {e}"),
        }
    }

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

/// True during a release-cut window, when no published engine speaks the
/// console's current flags — the driven journeys skip with the reason
/// printed, mirroring the integration gates' version-drift skip.
fn engine_drift() -> bool {
    let drift = !std::env::var("UI_E2E_ENGINE_DRIFT")
        .unwrap_or_default()
        .is_empty();
    if drift {
        println!(
            "skipping: release-cut window — the workspace engine is ahead of the console's pin"
        );
    }
    drift
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
    /// The substring locating the party statement option at Scope.
    statement: &'a str,
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

/// Drives connect → probe → scope → start → live, returning with the run
/// finished on the Live screen. Captures are the caller's business.
async fn drive_wizard(h: &Harness, sut: &DrivenSut<'_>, scope_shot: Option<&str>) {
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
    // The claim goes in as the document itself (#101): the example button
    // fills the paste box the way a vendor pastes their own statement.json.
    h.wait_xpath(&format!(
        "//button[contains(., 'Load') and contains(., '{}')]",
        sut.statement
    ))
    .await
    .click()
    .await
    .expect("load the example claim");
    h.wait_xpath("//p[contains(., 'Loaded ')]").await;
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

    h.wait_xpath("//h1[contains(., 'Live run')]").await;
    h.wait_xpath_for("//span[contains(., 'finished')]", sut.finish_budget)
        .await;
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
    if engine_drift() {
        return;
    }
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
        statement: "EHRbase",
        filter: "I_EHR_SERVICE.create_ehr-main",
        record_exchanges: true,
        probe_answer: "HTTP 500",
        continue_label: "Continue anyway",
        finish_budget: Duration::from_mins(1),
    };
    drive_wizard(&h, &sut, Some("scope-light")).await;
    h.capture("live-light").await;
    read_record(
        &h,
        "I_EHR_SERVICE.create_ehr-main",
        true,
        "results-light",
        "verdicts-light",
    )
    .await;

    // S8 and S9 ride this journey rather than a second driven one: the console
    // holds ONE run draft and ONE job slot, so a second wizard-driving journey
    // would leave a draft behind for whichever journey nextest runs next.
    let clean_url = export_and_verify(&h).await;

    h.enable_dark().await;
    h.goto("/run/verdicts").await;
    h.capture("verdicts-dark").await;
    h.goto(&clean_url).await;
    h.wait_xpath("//h2[contains(., 'The check')]").await;
    h.wait_xpath("//h2[contains(., 'What this proves')]").await;
    h.capture("verify-dark").await;

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
    // And the command-line equivalent, so the console is never the only
    // witness to its own verdict.
    h.wait_xpath("//body[contains(., 'veredictum verify-record')]")
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
    if engine_drift() {
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
            statement: "FerroEHR",
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
            statement: "EHRbase",
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
