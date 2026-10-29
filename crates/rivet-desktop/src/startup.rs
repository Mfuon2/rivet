use crate::{Preferences, widgets};
use eframe::egui;
use rivet_core::{
    browser_install::{self, Inspection, Listing},
    browser_setup::{self, Browser, Distribution, Platform},
    queue::{Queue, now},
};
use std::{
    path::PathBuf,
    sync::mpsc,
    time::{Duration, Instant},
};

pub struct Report {
    browser: Browser,
    identifier: String,
    manifest: PathBuf,
    extension: PathBuf,
    data: PathBuf,
    inspection: Inspection,
    listing: Option<Listing>,
    store_check_failed: bool,
    install_requested: bool,
    isolated: bool,
    started: i64,
}
pub struct Startup {
    receiver: mpsc::Receiver<Result<Report, String>>,
    report: Option<Result<Report, String>>,
    active: bool,
    force: bool,
    selected: Option<Browser>,
    allow_request: bool,
    notice: String,
    opened_browser: bool,
    confirmed: bool,
    last_poll: Instant,
}
impl Startup {
    pub fn new(force: bool, deferred: bool) -> Self {
        Self::for_browser(force, deferred, None)
    }
    fn for_browser(force: bool, deferred: bool, browser: Option<Browser>) -> Self {
        let (tx, receiver) = mpsc::channel();
        std::thread::spawn(move || {
            let _ = tx.send(check(browser).map_err(|e| e.to_string()));
        });
        Self {
            receiver,
            report: None,
            active: true,
            force,
            selected: browser,
            allow_request: !deferred,
            notice: String::new(),
            opened_browser: false,
            confirmed: false,
            last_poll: Instant::now() - Duration::from_secs(1),
        }
    }
    pub fn restart(&mut self) {
        *self = Self::new(true, false);
    }
    pub fn select_browser(&mut self, browser: Browser) {
        *self = Self::for_browser(true, false, Some(browser));
    }
    /// Setup watches fresh native-helper receipts; it never modifies browser preferences.
    pub fn render(
        &mut self,
        ui: &mut egui::Ui,
        prefs: &mut Preferences,
        can_restart: bool,
    ) -> bool {
        if !self.active {
            return false;
        }
        if self.report.is_none() {
            match self.receiver.try_recv() {
                Ok(report) => self.report = Some(report),
                Err(mpsc::TryRecvError::Disconnected) => {
                    self.report = Some(Err("Setup stopped. Retry or continue to Rivet.".into()))
                }
                Err(mpsc::TryRecvError::Empty) => {}
            }
            if let Some(Ok(report)) = &self.report {
                let receipt = Distribution::bundled()
                    .ok()
                    .and_then(|d| browser_setup::last_connection(&report.data, report.browser, &d));
                self.confirmed = browser_install::connection_confirmed(
                    &report.inspection,
                    receipt,
                    report.started,
                    now(),
                );
                let installed_enabled = report
                    .inspection
                    .installed
                    .iter()
                    .any(|e| e.enabled == Some(true));
                if !self.force
                    && (self.confirmed
                        || installed_enabled
                        || (prefs.browser_setup_deferred
                            && prefs.setup_browser == report.identifier))
                {
                    if self.confirmed || installed_enabled {
                        prefs.browser_setup_deferred = false;
                        prefs.setup_browser = report.identifier.clone();
                    }
                    self.active = false;
                    return false;
                }
            }
        }
        if self.last_poll.elapsed() >= Duration::from_millis(500) {
            if let Some(Ok(report)) = &self.report {
                let receipt = Distribution::bundled()
                    .ok()
                    .and_then(|d| browser_setup::last_connection(&report.data, report.browser, &d));
                self.confirmed |= browser_install::connection_confirmed(
                    &report.inspection,
                    receipt,
                    report.started,
                    now(),
                );
                if self.confirmed {
                    prefs.setup_browser = report.identifier.clone();
                    prefs.browser_setup_deferred = false;
                }
            }
            self.last_poll = Instant::now();
        }
        if !self.opened_browser
            && !self.confirmed
            && let Some(Ok(report)) = &mut self.report
        {
            self.opened_browser = true;
            let disabled = report
                .inspection
                .installed
                .iter()
                .any(|e| e.enabled == Some(false));
            if !report.isolated && !report.inspection.user_removed {
                if self.allow_request && report.inspection.installed.is_empty() {
                    report.install_requested = report.listing.as_ref().is_some_and(|listing| {
                        directories::UserDirs::new().is_some_and(|dirs| {
                            browser_install::request_install(
                                Platform::current(),
                                dirs.home_dir(),
                                report.browser,
                                listing,
                            )
                            .ok()
                            .is_some_and(|path| path.is_some() || cfg!(windows))
                        })
                    });
                }
                let url = if disabled {
                    Some(management_page(report.browser))
                } else if report.inspection.installed.is_empty() {
                    report.listing.as_ref().map(|l| l.url.as_str())
                } else {
                    None
                };
                if let Some(url) = url {
                    self.notice = match browser_install::open_browser(report.browser, url) {
                        Ok(()) => if disabled { "Browser extensions opened. Turn on Rivet; this screen will update automatically.".into() }
                            else { "Installation page opened. Approve Add extension in your browser; Rivet will finish connecting automatically.".into() },
                        Err(_) => "Could not open the browser automatically. Use the button below to try again.".into(),
                    };
                }
            }
        }
        let mut retry = false;
        let mut continue_clicked = false;
        let mut restart_app = false;
        egui::CentralPanel::default().show(ui, |ui| {
            egui::ScrollArea::vertical().show(ui, |ui| {
                ui.add_space(30.);
                ui.horizontal(|ui| {
                    ui.add(widgets::logo(48.));
                    ui.vertical(|ui| {
                        ui.heading(if self.confirmed { "Rivet is ready" } else { "Connect your browser" });
                        ui.label("Rivet prepares the desktop connection for you.");
                    });
                });
                ui.add_space(18.);
                match &self.report {
                    None => {
                        ui.horizontal(|ui| { ui.spinner(); ui.label("Detecting your browser, checking the extension and preparing setup…"); });
                        ui.add_space(12.);
                        continue_clicked = ui.button("Continue to Rivet").clicked();
                    }
                    Some(Err(error)) => {
                        ui.label(error);
                        ui.label("You can use Rivet now and return to Browsers whenever you need setup.");
                        retry = ui.button("Retry setup").clicked();
                        continue_clicked = ui.button("Continue to Rivet").clicked();
                    }
                    Some(Ok(report)) => {
                        ui.strong(format!("Browser: {}", report.browser.name()));
                        ui.horizontal(|ui| { ui.add(widgets::image("circle-check", 16., egui::Color32::from_rgb(48,151,105))); ui.label("Desktop helper prepared and registered automatically."); });
                        if self.confirmed {
                            ui.horizontal(|ui| { ui.add(widgets::image("circle-check", 16., egui::Color32::from_rgb(48,151,105))); ui.label("Extension connected. Browser setup is complete."); });
                            ui.label("You can continue now. A restart is normally unnecessary; restart Rivet if you prefer.");
                            ui.horizontal(|ui| {
                                continue_clicked = ui.button("Continue to Rivet").clicked();
                                restart_app = ui.add_enabled(can_restart, egui::Button::new("Restart Rivet")).clicked();
                            });
                            if !can_restart { ui.small("Pause active downloads before restarting Rivet."); }
                        } else {
                            ui.label(report.inspection.summary());
                            ui.separator();
                            if report.inspection.installed.iter().any(|e| e.enabled == Some(false)) {
                                ui.strong("Turn on Rivet in your browser");
                                ui.label("The extension is already installed. Enable it and Rivet will detect the connection automatically.");
                                if ui.button("Open browser extensions").clicked() {
                                    self.notice = open_page(report.browser, management_page(report.browser));
                                }
                            } else if !report.inspection.installed.is_empty() {
                                ui.strong("Waiting for your browser to connect…");
                                ui.label("Keep your browser open. Connection checks run automatically and can take up to a minute. The extension's Finish setup page can connect immediately.");
                                if ui.button("Open browser extensions").clicked() { self.notice = open_page(report.browser, management_page(report.browser)); }
                            } else if let Some(listing) = &report.listing {
                                ui.strong("Approve the extension in your browser");
                                ui.label("The browser downloads and installs the extension after you approve it. Rivet cannot approve browser permissions on your behalf.");
                                if report.install_requested { ui.label("A browser-approved installation request is prepared. If the browser does not pick it up, restart the browser when convenient."); }
                                if ui.button("Install Rivet extension").clicked() { self.notice = open_page(report.browser, &listing.url); }
                            } else {
                                ui.strong(if report.store_check_failed { "The extension store could not be reached" } else { "Browser-store approval is pending" });
                                ui.label(if report.store_check_failed { "Your desktop connection is ready. Check your connection and retry setup to open the installation page." } else { "Rivet's store item is not publicly available yet. Automatic store installation becomes available when it is published. You can use the desktop app now." });
                                retry = ui.button("Retry setup").clicked();
                            }
                            if report.inspection.installed.is_empty() {
                                ui.add_space(10.);
                                ui.group(|ui| {
                                    ui.strong("Install from GitHub");
                                    ui.label("Get the extension ZIP from this app's GitHub release, or use the matching files Rivet has already prepared for you. No terminal commands or extension-ID copying are needed.");
                                    ui.horizontal_wrapped(|ui| {
                                        if ui.button("Download extension ZIP").clicked() {
                                            let kind = if report.browser == Browser::Firefox { "firefox" } else { "chromium" };
                                            let version = env!("CARGO_PKG_VERSION");
                                            self.notice = open_page(report.browser, &format!("https://github.com/Mfuon2/rivet/releases/download/v{version}/rivet-{version}-extension-{kind}.zip"));
                                        }
                                        if ui.button("Open GitHub releases").clicked() { self.notice = open_page(report.browser, "https://github.com/Mfuon2/rivet/releases"); }
                                        if ui.button("Open prepared extension folder").clicked() { self.notice = match browser_install::open_folder(&report.extension) { Ok(()) => "Prepared extension folder opened. Select it in your browser's extension loader.".into(), Err(_) => "Could not open the folder. Its path is available under Development setup.".into() }; }
                                        if ui.button("Open extension installer").clicked() { self.notice = open_page(report.browser, report.browser.extensions_page()); }
                                    });
                                    ui.label(if report.browser == Browser::Firefox { "In Firefox, choose Load Temporary Add-on and select manifest.json in the prepared folder. Temporary add-ons disappear when Firefox restarts; a signed Mozilla release is needed for permanent installation." } else { "In your browser, enable Developer mode and choose Load unpacked. Select the prepared folder; Rivet has already extracted its files. Chromium browsers cannot install a raw ZIP directly." });
                                    ui.small("After the browser loads the extension, Rivet detects the connection and completes setup automatically.");
                                });
                            }
                            if !self.notice.is_empty() { ui.colored_label(ui.visuals().hyperlink_color, &self.notice); }
                            ui.add_space(10.);
                            ui.horizontal(|ui| { ui.spinner(); ui.label("Waiting for extension confirmation — this screen updates automatically."); });
                            ui.add_space(12.);
                            continue_clicked = ui.button("Continue to Rivet").clicked();
                            ui.small("Continuing defers this guide. You can reopen it from Browsers.");
                        }
                        ui.add_space(12.);
                        ui.collapsing("Development setup", |ui| {
                            ui.label("Only needed for an unpublished development build. Normal store installations do not require these steps.");
                            ui.label(if report.browser == Browser::Firefox { "Open about:debugging#/runtime/this-firefox, choose Load Temporary Add-on, and select manifest.json from the prepared folder. Temporary add-ons are removed on browser restart." } else { "Open the browser's extensions page, enable Developer mode, choose Load unpacked, and select the prepared folder." });
                            ui.horizontal(|ui| {
                                if ui.button("Open browser setup").clicked() { self.notice = open_page(report.browser, report.browser.extensions_page()); }
                                if ui.button("Open extension folder").clicked() { self.notice = match browser_install::open_folder(&report.extension) { Ok(()) => "Prepared extension folder opened.".into(), Err(_) => "Could not open the folder. Copy its path below.".into() }; }
                            });
                            if ui.button("Copy browser setup address").clicked() { ui.ctx().copy_text(report.browser.extensions_page().into()); self.notice = "Browser setup address copied.".into(); }
                            ui.add(egui::Label::new(egui::RichText::new(report.extension.display().to_string()).monospace()).wrap());
                            if ui.button("Copy extension folder").clicked() { ui.ctx().copy_text(report.extension.display().to_string()); self.notice = "Extension folder copied.".into(); }
                        });
                        ui.collapsing("Connection details", |ui| {
                            ui.label("Native helper registration:"); ui.code(report.manifest.display().to_string());
                            ui.label(format!("{} browser profiles checked. Custom profiles are confirmed by the extension's live connection.", report.inspection.profiles_checked));
                            if ui.button("Check again").clicked() { retry = true; }
                        });
                        ui.small("Rivet does not change your default browser or use enterprise force-install policies.");
                    }
                }
            });
        });
        if continue_clicked {
            if let Some(Ok(report)) = &self.report {
                prefs.setup_browser = report.identifier.clone();
            }
            prefs.browser_setup_deferred = !self.confirmed;
            self.active = false;
        }
        if restart_app {
            match std::env::current_exe().and_then(|exe| {
                std::process::Command::new(exe)
                    .args(std::env::args_os().skip(1))
                    .env("RIVET_SETUP_RECENT_RESTART", "1")
                    .stdin(std::process::Stdio::null())
                    .stdout(std::process::Stdio::null())
                    .stderr(std::process::Stdio::null())
                    .spawn()
            }) {
                Ok(_) => ui.ctx().send_viewport_cmd(egui::ViewportCommand::Close),
                Err(_) => {
                    self.notice =
                        "Could not restart Rivet. Continue now and reopen it when convenient."
                            .into()
                }
            }
        }
        if retry {
            *self = Self::for_browser(true, false, self.selected);
        }
        true
    }
}
fn management_page(browser: Browser) -> &'static str {
    if browser == Browser::Firefox {
        "about:addons"
    } else {
        browser.extensions_page()
    }
}
fn open_page(browser: Browser, url: &str) -> String {
    match browser_install::open_browser(browser, url) {
        Ok(()) => {
            "Browser opened. Approve or enable Rivet there; setup will finish automatically.".into()
        }
        Err(_) => "Could not open the browser. Check that it is installed, then retry.".into(),
    }
}
fn check(selected: Option<Browser>) -> anyhow::Result<Report> {
    use anyhow::{Context, ensure};
    // Preserve the just-confirmed handshake across a user-requested relaunch.
    let started = now()
        - if std::env::var("RIVET_SETUP_RECENT_RESTART").as_deref() == Ok("1") {
            90
        } else {
            0
        };
    let (browser, identifier) = if let Some(browser) = selected {
        (browser, format!("selected:{}", browser.name()))
    } else {
        browser_setup::detect_default()?
    };
    let database = Queue::default_path()?;
    let data = database
        .parent()
        .context("No Rivet data folder")?
        .to_path_buf();
    let isolated = std::env::var_os("RIVET_SETUP_ROOT").is_some();
    let home = match std::env::var_os("RIVET_SETUP_ROOT") {
        Some(path) => {
            let path = PathBuf::from(path);
            ensure!(path.is_absolute(), "RIVET_SETUP_ROOT must be absolute");
            path
        }
        None => directories::UserDirs::new()
            .context("Cannot locate user home")?
            .home_dir()
            .to_path_buf(),
    };
    let helper = std::env::current_exe()?.with_file_name(if cfg!(windows) {
        "rivet-native-host.exe"
    } else {
        "rivet-native-host"
    });
    let distribution = Distribution::bundled()?;
    let extension = browser_setup::prepare_extension(&data, browser)?;
    let platform = Platform::current();
    #[cfg(windows)]
    ensure!(
        !isolated,
        "Native-host registry setup is disabled in isolated preview mode"
    );
    let manifest =
        browser_setup::register(platform, &home, &data, browser, &helper, &distribution)?;
    let inspection = browser_install::inspect(platform, &home, browser, &distribution);
    let mut store_check_failed = false;
    let listing = if inspection.installed.is_empty() && !isolated {
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()?;
        match runtime.block_on(browser_install::resolve_listing(browser, &distribution)) {
            Ok(listing) => listing,
            Err(_) => {
                store_check_failed = true;
                None
            }
        }
    } else {
        browser_install::configured_listing(browser, &distribution)
    };
    let identifier = format!("{}:{}", identifier, distribution.extension_id(browser));
    Ok(Report {
        browser,
        identifier,
        manifest,
        extension,
        data,
        inspection,
        listing,
        store_check_failed,
        install_requested: false,
        isolated,
        started,
    })
}
