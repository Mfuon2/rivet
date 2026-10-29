use crate::{Preferences, widgets};
use eframe::egui;
use rivet_core::{
    browser_setup::{self, Browser, Distribution, Platform},
    queue::Queue,
};
use std::{path::PathBuf, sync::mpsc};

pub struct Report {
    pub browser: Browser,
    pub identifier: String,
    pub manifest: PathBuf,
    pub extension: PathBuf,
    pub data: PathBuf,
    pub install_url: Option<String>,
}
pub struct Startup {
    receiver: mpsc::Receiver<Result<Report, String>>,
    report: Option<Result<Report, String>>,
    active: bool,
    force: bool,
    notice: String,
}
impl Startup {
    pub fn new(force: bool) -> Self {
        let (tx, receiver) = mpsc::channel();
        std::thread::spawn(move || {
            let _ = tx.send(check().map_err(|e| e.to_string()));
        });
        Self {
            receiver,
            report: None,
            active: true,
            force,
            notice: String::new(),
        }
    }
    pub fn restart(&mut self) {
        *self = Self::new(true);
    }
    /// True while the startup screen owns the window. Browser detection runs off the UI thread.
    pub fn render(&mut self, ui: &mut egui::Ui, prefs: &mut Preferences) -> bool {
        if !self.active {
            return false;
        }
        if self.report.is_none() {
            match self.receiver.try_recv() {
                Ok(report) => self.report = Some(report),
                Err(mpsc::TryRecvError::Disconnected) => {
                    self.report = Some(Err(
                        "Setup worker stopped. Retry or continue to Rivet.".into()
                    ))
                }
                Err(mpsc::TryRecvError::Empty) => {}
            }
            if let Some(Ok(report)) = &self.report
                && !self.force
                && prefs.setup_browser == report.identifier
            {
                self.active = false;
                return false;
            }
        }
        let mut retry = false;
        let mut continue_clicked = false;
        egui::CentralPanel::default().show(ui, |ui| {
            egui::ScrollArea::vertical().show(ui, |ui| {
            ui.add_space(35.);
            ui.horizontal(|ui| {
                ui.add(widgets::image("panels-top-left", 36., egui::Color32::from_rgb(66,130,193)));
                ui.vertical(|ui| { ui.heading("Getting Rivet ready"); ui.label("Set up downloads for your default browser."); });
            });
            ui.add_space(18.);
            match &self.report {
                None => { ui.horizontal(|ui| { ui.spinner(); ui.label("Checking your default browser and preparing its connection…"); }); }
                Some(Err(error)) => {
                    ui.label(error);
                    ui.label("You can still use Rivet directly. Browser setup is available from Browsers in the toolbar.");
                    ui.add_space(12.);
                    retry = ui.button("Retry setup").clicked();
                    continue_clicked = ui.button("Continue to Rivet").clicked();
                }
                Some(Ok(report)) => {
                    ui.strong(format!("Default browser: {}", report.browser.name()));
                    ui.label("Native helper: found and registered automatically for this user.");
                    ui.label("Extension package: prepared locally.");
                    let connected = Distribution::bundled().ok().and_then(|d| browser_setup::last_connection(&report.data, report.browser, &d));
                    if let Some(time) = connected {
                        ui.label(format!("Extension last contacted Rivet: {}. This does not verify that it is currently enabled in this browser profile.", widgets::date(time)));
                    } else { ui.label("Extension connection has not been confirmed yet."); }
                    ui.separator();
                    if let Some(url) = &report.install_url {
                        ui.strong("Approve the Rivet extension in your browser");
                        ui.label("Your desktop connection is ready. The browser will ask you to approve the extension.");
                        if ui.button("Install Rivet extension").clicked() { ui.ctx().open_url(egui::OpenUrl::new_tab(url)); }
                    } else {
                        ui.strong("Development build — load the extension once");
                        ui.label("Rivet's extension has not been published yet. No terminal commands or extension-ID copying are needed.");
                        ui.add_space(8.);
                        ui.label(format!("1. Open {} in {}.", report.browser.extensions_page(), report.browser.name()));
                        if ui.button("Copy browser setup address").clicked() {
                            ui.ctx().copy_text(report.browser.extensions_page().into());
                            self.notice = "Browser setup address copied. Paste it into your browser's address bar.".into();
                        }
                        ui.label(if report.browser == Browser::Firefox {
                            "2. Choose Load Temporary Add-on and select manifest.json from the folder below. Firefox removes temporary add-ons when it exits."
                        } else { "2. Enable Developer mode, choose Load unpacked, then select the prepared folder below." });
                        ui.add(egui::Label::new(egui::RichText::new(report.extension.display().to_string()).monospace()).wrap());
                        if ui.button("Copy extension folder").clicked() {
                            ui.ctx().copy_text(report.extension.display().to_string());
                            self.notice = "Extension folder copied. Paste it into the browser's Load unpacked folder picker.".into();
                        }
                    }
                    if !self.notice.is_empty() { ui.colored_label(ui.visuals().hyperlink_color, &self.notice); }
                    ui.add_space(8.);
                    ui.label("Open the extension popup and choose Check connection. Captured links appear in your Rivet queue.");
                    ui.collapsing("Connection details", |ui| { ui.label("Native-host registration:"); ui.code(report.manifest.display().to_string()); });
                    ui.add_space(12.);
                    ui.horizontal(|ui| {
                        continue_clicked = ui.button("Continue to Rivet").clicked();
                        retry = ui.button("Check again").clicked();
                    });
                    ui.small("You can return to setup from Browsers. Rivet never changes your default browser.");
                }
            }
            });
        });
        if continue_clicked {
            if let Some(Ok(report)) = &self.report {
                prefs.setup_browser = report.identifier.clone();
            }
            self.active = false;
        }
        if retry {
            self.restart();
        }
        true
    }
}
fn check() -> anyhow::Result<Report> {
    use anyhow::{Context, ensure};
    let (browser, identifier) = browser_setup::detect_default()?;
    let database = Queue::default_path()?;
    let data = database
        .parent()
        .context("No Rivet data folder")?
        .to_path_buf();
    // Smoke tests redirect registration into a temporary home rather than touching real browsers.
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
    let exe = std::env::current_exe()?;
    let helper = exe.with_file_name(if cfg!(windows) {
        "rivet-native-host.exe"
    } else {
        "rivet-native-host"
    });
    let distribution = Distribution::bundled()?;
    let extension = browser_setup::prepare_extension(&data, browser)?;
    let platform = Platform::current();
    // On Windows the registry is the browser discovery mechanism. Test overrides must not write it.
    #[cfg(windows)]
    ensure!(
        std::env::var_os("RIVET_SETUP_ROOT").is_none(),
        "Native-host registry setup is disabled in isolated preview mode"
    );
    let manifest =
        browser_setup::register(platform, &home, &data, browser, &helper, &distribution)?;
    let install_url = distribution.install_url(browser).map(str::to_owned);
    let identifier = format!("{}:{}", identifier, distribution.extension_id(browser));
    Ok(Report {
        browser,
        identifier,
        manifest,
        extension,
        data,
        install_url,
    })
}
