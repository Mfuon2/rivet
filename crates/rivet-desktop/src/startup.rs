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
    package_source: String,
    package_warning: Option<String>,
}
enum SetupEvent {
    Progress(u8, &'static str),
    Finished(Box<Result<Report, String>>),
}
pub struct Startup {
    receiver: mpsc::Receiver<SetupEvent>,
    phase: u8,
    phase_text: String,
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
            let report = check(browser, !deferred || force, |phase, text| {
                let _ = tx.send(SetupEvent::Progress(phase, text));
            })
            .map_err(|e| e.to_string());
            let _ = tx.send(SetupEvent::Finished(Box::new(report)));
        });
        Self {
            receiver,
            phase: 0,
            phase_text: "Finding your default browser…".into(),
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
            loop {
                match self.receiver.try_recv() {
                    Ok(SetupEvent::Progress(phase, text)) => {
                        self.phase = phase;
                        self.phase_text = text.into();
                    }
                    Ok(SetupEvent::Finished(report)) => {
                        self.report = Some(*report);
                        self.phase = 3;
                        break;
                    }
                    Err(mpsc::TryRecvError::Disconnected) => {
                        self.report =
                            Some(Err("Setup stopped. Retry or continue to Rivet.".into()));
                        break;
                    }
                    Err(mpsc::TryRecvError::Empty) => break,
                }
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
                    Some(
                        report
                            .listing
                            .as_ref()
                            .map(|l| l.url.as_str())
                            .unwrap_or_else(|| report.browser.extensions_page()),
                    )
                } else {
                    None
                };
                if let Some(url) = url {
                    if report.listing.is_none()
                        && report.inspection.installed.is_empty()
                        && self.allow_request
                    {
                        let _ = browser_install::open_folder(&report.extension);
                    }
                    self.notice = match browser_install::open_browser(report.browser, url) {
                        Ok(()) => if disabled { "Browser extensions opened. Turn on Rivet; this screen will update automatically.".into() }
                            else if report.listing.is_some() { "Installation page opened. Approve Rivet in your browser to finish.".into() } else { "Browser setup and the prepared extension folder opened.".into() },
                        Err(_) => "Could not open the browser automatically. Use the button below to try again.".into(),
                    };
                }
            }
        }
        let mut retry = false;
        let mut continue_clicked = false;
        let mut restart_app = false;
        let palette = Palette::new(ui.visuals().dark_mode);
        egui::CentralPanel::default().frame(egui::Frame::new().fill(palette.background)).show(ui, |ui| {
            egui::ScrollArea::vertical().auto_shrink([false,false]).show(ui, |ui| {
                ui.add_space(20.);
                let width=(ui.available_width()-48.).clamp(300.,820.);
                let left=((ui.available_width()-width)*0.5).max(0.);
                ui.horizontal(|ui| {ui.add_space(left);ui.vertical(|ui| {
                    ui.set_width(width);
                    egui::Frame::new().fill(palette.surface).stroke(egui::Stroke::new(1.,palette.border)).corner_radius(20).inner_margin(24).show(ui,|ui| {
                        ui.horizontal(|ui| {
                            ui.add(widgets::logo(38.));
                            ui.label(egui::RichText::new("Rivet").font(semibold(18.)).color(palette.ink));
                            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center),|ui| {
                                let browser=self.report.as_ref().and_then(|r|r.as_ref().ok()).map(|r|r.browser.name()).unwrap_or("Detecting browser");
                                egui::Frame::new().fill(palette.tint).corner_radius(14).inner_margin(egui::Margin::symmetric(12,5)).show(ui,|ui|{ui.label(egui::RichText::new(browser).size(12.).color(palette.accent));});
                            });
                        });
                        ui.add_space(22.);
                        ui.label(egui::RichText::new(if self.confirmed {"Your browser is connected."} else {"Let’s connect your browser."}).font(semibold(27.)).color(palette.ink));
                        ui.label(egui::RichText::new(if self.confirmed {"You're ready to send links and media to your download queue."}else{"Rivet handles the preparation. Your browser controls the final approval."}).size(13.).color(palette.muted));
                        ui.add_space(18.);
                        ui.add(egui::ProgressBar::new(if self.confirmed {1.} else if self.report.is_some(){0.75}else{(self.phase as f32+0.4)/4.}).desired_height(4.).fill(if self.confirmed {palette.success}else{palette.accent}));
                        ui.add_space(14.);
                        let report=self.report.as_ref().and_then(|r|r.as_ref().ok());
                        step(ui,&palette,1,"Find your browser",report.map(|r|format!("{} detected for this connection",r.browser.name())).as_deref().unwrap_or("Checking your default browser"),report.is_some() || self.phase>0,self.report.is_none() && self.phase==0);
                        step(ui,&palette,2,"Prepare the desktop connection","Native helper registered for your account",report.is_some() || self.phase>1,self.report.is_none() && self.phase==1);
                        step(ui,&palette,3,"Prepare your extension",report.map(|r|r.package_source.as_str()).unwrap_or(self.phase_text.as_str()),report.is_some(),self.report.is_none() && self.phase==2);
                        step(ui,&palette,4,"Connect with your browser",if self.confirmed {"Live connection confirmed"}else{"Waiting for your browser to confirm"},self.confirmed,report.is_some() && !self.confirmed);
                        ui.add_space(14.);
                        match &self.report {
                            None=>{
                                ui.label(egui::RichText::new(&self.phase_text).color(palette.muted));
                                ui.add_space(12.);continue_clicked=secondary(ui,"Continue to Rivet").clicked();
                            },
                            Some(Err(error))=>{
                                callout(ui,&palette,"Setup could not finish",error);
                                ui.add_space(14.);ui.horizontal_wrapped(|ui|{retry=primary(ui,&palette,"Retry setup").clicked();continue_clicked=secondary(ui,"Continue to Rivet").clicked();});
                            },
                            Some(Ok(report))=>{
                                if self.confirmed {
                                    callout(ui,&palette,"You're all set","The browser extension and desktop app are connected. No restart is needed.");
                                    ui.add_space(16.);
                                    ui.horizontal_wrapped(|ui|{
                                        continue_clicked=primary(ui,&palette,"Continue to Rivet").clicked();
                                        restart_app=ui.add_enabled(can_restart,egui::Button::new("Restart Rivet").min_size(egui::vec2(130.,42.)).corner_radius(9)).clicked();
                                    });
                                    if !can_restart {ui.small("Pause active downloads before restarting.");}
                                } else {
                                    let disabled=report.inspection.installed.iter().any(|e|e.enabled==Some(false));
                                    let (title,description)=if disabled {
                                        (format!("Turn on Rivet in {}",report.browser.name()),"The extension is installed. Enable it in the browser window; Rivet will connect automatically.".to_owned())
                                    } else if !report.inspection.installed.is_empty() {
                                        (format!("Waiting for {}",report.browser.name()),"Keep your browser open. Rivet checks the connection automatically; this may take up to a minute.".to_owned())
                                    } else if report.listing.is_some() {
                                        (format!("Approve Rivet in {}",report.browser.name()),"The installation page is ready. Choose Add extension in your browser. Rivet will finish the connection automatically.".to_owned())
                                    } else if report.browser==Browser::Firefox {
                                        ("Files ready. Firefox approval needed.".into(),"Mozilla signing is required for permanent installation. For this unpublished build, choose Load Temporary Add-on and select manifest.json in the prepared folder.".into())
                                    } else {
                                        (format!("Files ready. Finish in {}.",report.browser.name()),"Chrome-based browsers block silent installation of unpublished extensions. In the opened browser, enable Developer mode, choose Load unpacked, then select the opened Rivet folder.".into())
                                    };
                                    callout(ui,&palette,&title,&description);
                                    if !self.notice.is_empty(){ui.add_space(8.);ui.label(egui::RichText::new(&self.notice).size(11.5).color(palette.muted));}
                                    ui.add_space(16.);
                                    ui.horizontal_wrapped(|ui|{
                                        if primary(ui,&palette,&format!("Continue in {}",report.browser.name())).clicked(){self.notice=open_setup(report);}
                                        continue_clicked=secondary(ui,"Continue to Rivet").clicked();
                                    });
                                    ui.add_space(8.);ui.label(egui::RichText::new("You can use Rivet while your browser connects.").size(11.5).color(palette.muted));
                                }
                                ui.add_space(16.);
                                ui.separator();ui.add_space(6.);
                                ui.collapsing("Connection details",|ui|{
                                    ui.label(report.inspection.summary());
                                    ui.label(format!("{} browser profiles checked",report.inspection.profiles_checked));
                                    ui.label(format!("Helper: {}",report.manifest.display()));
                                    if report.store_check_failed {ui.label("The browser store could not be reached. The bundled extension is ready to use.");}
                                    if let Some(warning)=&report.package_warning{ui.label(warning);}
                                    if report.listing.is_none(){ui.label(if report.store_check_failed { "Store availability could not be confirmed. The prepared development package can be loaded from browser setup." } else { "Store publication is pending. Browser confirmation is required for this development package." });}
                                    if report.install_requested{ui.label("A standard browser installation request has been registered.");}
                                    retry=ui.button("Check again").clicked();
                                    ui.collapsing("Development setup",|ui|{
                                        ui.label("The prepared folder is permanent; temporary download files are removed automatically.");
                                        ui.add(egui::Label::new(egui::RichText::new(report.extension.display().to_string()).monospace()).wrap());
                                        ui.horizontal_wrapped(|ui|{
                                            if ui.button("Open extension folder").clicked(){let _=browser_install::open_folder(&report.extension);}
                                            if ui.button("Copy extension folder").clicked(){ui.ctx().copy_text(report.extension.display().to_string());}
                                            if ui.button("Copy browser setup address").clicked(){ui.ctx().copy_text(report.browser.extensions_page().into());}
                                            if ui.button("Open GitHub releases").clicked(){self.notice=open_page(report.browser,"https://github.com/Mfuon2/rivet/releases");}
                                        });
                                    });
                                });
                            }
                        }
                    });
                    ui.add_space(16.);
                    ui.horizontal(|ui|{ui.add(widgets::image("circle-check",14.,palette.muted));ui.label(egui::RichText::new("Local connection. No account required. Your default browser stays yours.").size(11.5).color(palette.muted));});
                    ui.add_space(6.);
                });});
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
fn check(
    selected: Option<Browser>,
    fetch: bool,
    progress: impl Fn(u8, &'static str),
) -> anyhow::Result<Report> {
    progress(0, "Finding your default browser…");
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
    progress(1, "Preparing the desktop connection…");
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
    progress(2, "Checking the browser extension…");
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
    let mut package_source = "Prepared securely from this app".to_owned();
    let mut package_warning = None;
    if fetch && inspection.installed.is_empty() && listing.is_none() && !isolated {
        progress(2, "Downloading and verifying the extension…");
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()?;
        match runtime.block_on(browser_install::prepare_release_extension(&data, browser)) {
            Ok(downloaded) => {
                package_source = if downloaded {
                    "Downloaded, verified and unpacked automatically"
                } else {
                    "Verified release package already prepared"
                }
                .into()
            }
            Err(error) => {
                package_warning = Some(format!(
                    "Release download unavailable. Using the trusted files bundled with Rivet. {error}"
                ))
            }
        }
    }
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
        package_source,
        package_warning,
    })
}

struct Palette {
    background: egui::Color32,
    surface: egui::Color32,
    border: egui::Color32,
    ink: egui::Color32,
    muted: egui::Color32,
    accent: egui::Color32,
    tint: egui::Color32,
    success: egui::Color32,
}
impl Palette {
    fn new(dark: bool) -> Self {
        let color = |r, g, b| egui::Color32::from_rgb(r, g, b);
        if dark {
            Self {
                background: color(12, 20, 34),
                surface: color(20, 31, 49),
                border: color(40, 57, 78),
                ink: color(230, 238, 249),
                muted: color(154, 174, 198),
                accent: color(92, 153, 255),
                tint: color(28, 50, 82),
                success: color(70, 202, 156),
            }
        } else {
            Self {
                background: color(242, 246, 252),
                surface: egui::Color32::WHITE,
                border: color(223, 231, 242),
                ink: color(24, 42, 68),
                muted: color(99, 118, 143),
                accent: color(38, 105, 230),
                tint: color(237, 243, 255),
                success: color(27, 155, 116),
            }
        }
    }
}
fn semibold(size: f32) -> egui::FontId {
    egui::FontId::new(size, egui::FontFamily::Name("Rivet Strong".into()))
}
fn primary(ui: &mut egui::Ui, p: &Palette, label: &str) -> egui::Response {
    ui.add(
        egui::Button::new(
            egui::RichText::new(label)
                .color(if ui.visuals().dark_mode {
                    egui::Color32::from_rgb(12, 20, 34)
                } else {
                    egui::Color32::WHITE
                })
                .font(semibold(13.)),
        )
        .fill(p.accent)
        .min_size(egui::vec2(186., 42.))
        .corner_radius(9),
    )
}
fn secondary(ui: &mut egui::Ui, label: &str) -> egui::Response {
    ui.add(
        egui::Button::new(label)
            .min_size(egui::vec2(160., 42.))
            .corner_radius(9),
    )
}
fn step(
    ui: &mut egui::Ui,
    p: &Palette,
    number: u8,
    title: &str,
    description: &str,
    done: bool,
    active: bool,
) {
    ui.horizontal(|ui| {
        ui.allocate_ui(egui::vec2(34., 42.), |ui| {
            ui.add_space(6.);
            if done {
                ui.add(widgets::image("circle-check", 24., p.success));
            } else if active {
                ui.add(egui::Spinner::new().size(23.).color(p.accent));
            } else {
                ui.label(
                    egui::RichText::new(format!("{number:02}"))
                        .font(semibold(17.))
                        .color(p.muted),
                );
            }
        });
        ui.vertical(|ui| {
            ui.add_space(3.);
            ui.label(egui::RichText::new(title).font(semibold(13.5)).color(p.ink));
            ui.label(egui::RichText::new(description).size(11.5).color(p.muted));
        });
    });
}
fn callout(ui: &mut egui::Ui, p: &Palette, title: &str, description: &str) {
    egui::Frame::new()
        .fill(p.tint)
        .corner_radius(12)
        .inner_margin(16)
        .show(ui, |ui| {
            ui.set_min_width(ui.available_width());
            ui.label(egui::RichText::new(title).font(semibold(13.)).color(p.ink));
            ui.add_space(4.);
            ui.label(egui::RichText::new(description).size(12.).color(p.muted));
        });
}
fn open_setup(report: &Report) -> String {
    let url = report
        .listing
        .as_ref()
        .map(|l| l.url.as_str())
        .unwrap_or_else(|| management_page(report.browser));
    if report.listing.is_none()
        && report.inspection.installed.is_empty()
        && browser_install::open_folder(&report.extension).is_err()
    {
        return "The prepared folder could not be opened. Its path is under Connection details."
            .into();
    }
    match browser_install::open_browser(report.browser, url) {
        Ok(()) => "Browser setup opened. Rivet will detect the connection automatically.".into(),
        Err(_) => "Could not open the browser. Check that it is installed and try again.".into(),
    }
}
