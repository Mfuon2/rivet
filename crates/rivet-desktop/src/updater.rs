use crate::widgets;
use eframe::egui;
use rivet_core::{
    CancellationToken,
    update::{self, Installation, Prepared, Release},
};
use std::sync::mpsc;

enum Event {
    Checked(Result<Option<Release>, String>),
    Progress(u64, u64),
    Downloaded(Result<Prepared, String>),
}
#[derive(PartialEq)]
enum State {
    Checking,
    Available,
    Downloading,
    Ready,
    Current,
    Failed,
}
pub struct Updater {
    tx: mpsc::Sender<Event>,
    rx: mpsc::Receiver<Event>,
    state: State,
    release: Option<Release>,
    prepared: Option<Prepared>,
    cancel: CancellationToken,
    open: bool,
    error: String,
    downloaded: u64,
    total: u64,
    notice: String,
}
impl Updater {
    pub fn new() -> Self {
        let (tx, rx) = mpsc::channel();
        let mut updater = Self {
            tx,
            rx,
            state: State::Current,
            release: None,
            prepared: None,
            cancel: CancellationToken::new(),
            open: false,
            error: String::new(),
            downloaded: 0,
            total: 0,
            notice: String::new(),
        };
        if let Ok(cache) = update::cache_dir() {
            let receipt = cache.join("last-result.json");
            if let Ok(data) = std::fs::read(&receipt) {
                if let Ok(value) = serde_json::from_slice::<serde_json::Value>(&data) {
                    updater.notice = if value["ok"] == true {
                        format!(
                            "Update to Rivet {} completed.",
                            value["version"].as_str().unwrap_or("")
                        )
                    } else {
                        "The previous update was cancelled or could not finish. You can retry below.".into()
                    };
                }
                let _ = std::fs::remove_file(receipt);
            }
        }
        // Isolated UI tests/previews never contact GitHub or install software.
        if std::env::var_os("RIVET_SETUP_ROOT").is_none() {
            updater.check(false);
        }
        updater.open = !updater.notice.is_empty();
        updater
    }
    pub fn check(&mut self, show: bool) {
        if self.prepared.is_some() {
            self.state = State::Ready;
            self.open = true;
            return;
        }
        if self.state == State::Downloading || self.state == State::Checking {
            self.open = true;
            return;
        }
        self.state = State::Checking;
        self.open = show;
        self.error.clear();
        self.release = None;
        let tx = self.tx.clone();
        std::thread::spawn(move || {
            let result = tokio::runtime::Runtime::new()
                .map_err(|e| e.to_string())
                .and_then(|rt| {
                    rt.block_on(update::check(env!("CARGO_PKG_VERSION")))
                        .map_err(|e| e.to_string())
                });
            let _ = tx.send(Event::Checked(result));
        });
    }
    fn download(&mut self) {
        let Some(release) = self.release.clone() else {
            return;
        };
        let installation = std::env::current_exe()
            .map_err(anyhow::Error::from)
            .and_then(|p| Installation::detect(&p));
        let installation = match installation {
            Ok(v) => v,
            Err(e) => {
                self.error = e.to_string();
                self.state = State::Failed;
                return;
            }
        };
        self.state = State::Downloading;
        self.error.clear();
        self.cancel = CancellationToken::new();
        let cancel = self.cancel.clone();
        let tx = self.tx.clone();
        std::thread::spawn(move || {
            let result = tokio::runtime::Runtime::new()
                .map_err(|e| e.to_string())
                .and_then(|rt| {
                    rt.block_on(update::download(
                        release,
                        installation,
                        cancel,
                        |done, total| {
                            let _ = tx.send(Event::Progress(done, total));
                        },
                    ))
                    .map_err(|e| e.to_string())
                });
            let _ = tx.send(Event::Downloaded(result));
        });
    }
    pub fn render(&mut self, ctx: &egui::Context, can_install: bool) {
        while let Ok(event) = self.rx.try_recv() {
            match event {
                Event::Checked(Ok(Some(release))) => {
                    self.release = Some(release);
                    self.state = State::Available;
                    self.open = true;
                }
                Event::Checked(Ok(None)) => self.state = State::Current,
                Event::Checked(Err(error)) => {
                    self.state = State::Failed;
                    self.error = format!("Could not check for updates. {error}");
                }
                Event::Progress(done, total) => {
                    self.downloaded = done;
                    self.total = total;
                }
                Event::Downloaded(Ok(prepared)) => {
                    self.prepared = Some(prepared);
                    self.state = State::Ready;
                }
                Event::Downloaded(Err(error)) => {
                    if self.cancel.is_cancelled() {
                        self.state = State::Available;
                    } else {
                        self.state = State::Failed;
                        self.error = error;
                    }
                }
            }
        }
        if !self.open {
            return;
        }
        let mut open = true;
        let mut fetch = false;
        let mut install = false;
        let mut check = false;
        let mut later = false;
        egui::Window::new("Rivet updates").open(&mut open).collapsible(false).resizable(false)
            .anchor(egui::Align2::CENTER_CENTER,egui::Vec2::ZERO).default_width(490.).show(ctx,|ui|{
                ui.add_space(8.);
                ui.horizontal(|ui|{ui.add(widgets::logo(58.));ui.vertical(|ui|{
                    ui.heading(match self.state {State::Ready=>"Your update is ready",State::Available=>"A newer Rivet is available",State::Downloading=>"Getting your update ready",State::Checking=>"Checking for updates",State::Current=>"You're up to date",State::Failed=>"Update needs attention"});
                    ui.label(format!("Installed version {}",env!("CARGO_PKG_VERSION")));
                });});ui.add_space(12.);
                if !self.notice.is_empty(){ui.label(&self.notice);ui.add_space(6.);}
                if let Some(release)=&self.release {
                    ui.group(|ui|{ui.strong(format!("Rivet {}{}",release.version(),if release.prerelease{" · Preview"}else{""}));
                        ui.label("Updates the desktop app, download engine, native helper and bundled browser integration.");
                        ui.hyperlink_to("What's new in this release",release.page());
                    });ui.add_space(12.);
                }
                match self.state {
                    State::Checking=>{ui.horizontal(|ui|{ui.spinner();ui.label("Looking for published Rivet releases…");});},
                    State::Available=>{ui.label("Download now, then install and restart when you're ready. Your queue, settings and downloaded files are kept.");
                        ui.small("The release signature and package checksum are verified before installation.");
                        ui.add_space(10.);fetch=ui.add(egui::Button::new("Download update").fill(ui.visuals().selection.bg_fill)).clicked();later=ui.button("Not now").clicked();},
                    State::Downloading=>{let fraction=if self.total>0{self.downloaded as f32/self.total as f32}else{0.};
                        ui.add(egui::ProgressBar::new(fraction).show_percentage().animate(true));
                        ui.label(format!("{:.1} / {:.1} MB · Downloading and verifying",self.downloaded as f64/1e6,self.total as f64/1e6));
                        if ui.button("Cancel download").clicked(){self.cancel.cancel();}},
                    State::Ready=>{ui.horizontal(|ui|{ui.add(widgets::image("circle-check",18.,egui::Color32::from_rgb(48,151,105)));ui.label("Release signature and package checksum verified.");});
                        ui.label("Rivet will close, install the complete update and reopen automatically. macOS and system Linux packages may ask for administrator approval.");
                        if !can_install{ui.colored_label(ui.visuals().warn_fg_color,"Pause active downloads before installing this update.");}
                        ui.add_space(10.);install=ui.add_enabled(can_install,egui::Button::new("Install and restart").fill(ui.visuals().selection.bg_fill)).clicked();
                        later=ui.button("Install later").clicked();},
                    State::Current=>{ui.label("You're running the newest published version for this release channel.");ui.hyperlink_to("Browse releases","https://github.com/Mfuon2/rivet/releases");},
                    State::Failed=>{ui.label(&self.error);ui.label("You can continue using Rivet and try again later.");
                        check=ui.button("Retry update check").clicked();ui.hyperlink_to("Download from GitHub","https://github.com/Mfuon2/rivet/releases");},
                }
                ui.add_space(6.);
            });
        if !open || later {
            self.open = false;
        }
        if fetch {
            self.download();
        }
        if check {
            self.check(true);
        }
        if install && let Some(prepared) = &self.prepared {
            match update::launch(prepared) {
                Ok(()) => ctx.send_viewport_cmd(egui::ViewportCommand::Close),
                Err(e) => {
                    self.error = e.to_string();
                    self.state = State::Failed;
                }
            }
        }
    }
}
impl Drop for Updater {
    fn drop(&mut self) {
        self.cancel.cancel();
        // The held file lock releases only when Rivet exits; the detached installer waits on it.
        if let Some(prepared) = &self.prepared {
            let _ = &prepared.lock;
        }
    }
}
