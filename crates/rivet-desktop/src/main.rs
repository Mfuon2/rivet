#![cfg_attr(target_os = "windows", windows_subsystem = "windows")]
mod browser;
mod startup;
mod view;
mod widgets;
use eframe::egui::{self, Color32};
use rivet_core::{
    CancellationToken, DownloadRequest, Engine, Progress, native,
    queue::{Job, Queue, now},
    worker::{run_job, safe_error},
};
use serde::{Deserialize, Serialize};
use std::{
    collections::{HashMap, HashSet},
    path::PathBuf,
    sync::mpsc,
    time::{Duration, Instant},
};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
enum Field {
    Name,
    Queue,
    Size,
    Status,
    Remaining,
    Rate,
    LastTry,
    Added,
    Description,
    Destination,
}
impl Field {
    const ALL: [Self; 10] = [
        Self::Name,
        Self::Queue,
        Self::Size,
        Self::Status,
        Self::Remaining,
        Self::Rate,
        Self::LastTry,
        Self::Added,
        Self::Description,
        Self::Destination,
    ];
    fn label(self) -> &'static str {
        match self {
            Self::Name => "File name",
            Self::Queue => "Q",
            Self::Size => "Size",
            Self::Status => "Status",
            Self::Remaining => "Time left",
            Self::Rate => "Transfer rate",
            Self::LastTry => "Last try",
            Self::Added => "Date added",
            Self::Description => "Description",
            Self::Destination => "Save to",
        }
    }
    fn width(self) -> f32 {
        match self {
            Self::Name => 250.,
            Self::Queue => 24.,
            Self::Size => 85.,
            Self::Status => 140.,
            Self::Remaining => 75.,
            Self::Rate => 95.,
            Self::LastTry | Self::Added => 126.,
            Self::Description => 180.,
            Self::Destination => 260.,
        }
    }
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(default)]
struct Preferences {
    dark: bool,
    row_height: f32,
    categories: bool,
    details: bool,
    columns: Vec<Field>,
    connections: u8,
    setup_browser: String,
}
impl Default for Preferences {
    fn default() -> Self {
        Self {
            dark: false,
            row_height: 22.,
            categories: true,
            details: true,
            columns: vec![
                Field::Name,
                Field::Queue,
                Field::Size,
                Field::Status,
                Field::Remaining,
                Field::Rate,
                Field::LastTry,
                Field::Description,
            ],
            connections: 4,
            setup_browser: String::new(),
        }
    }
}
#[derive(Clone, PartialEq)]
enum Filter {
    All,
    Unfinished,
    Finished,
    Failed,
    Queued,
    Category(String),
}
impl Filter {
    fn label(&self) -> &str {
        match self {
            Self::All => "All downloads",
            Self::Unfinished => "Unfinished",
            Self::Finished => "Finished",
            Self::Failed => "Failed",
            Self::Queued => "Main download queue",
            Self::Category(s) => s,
        }
    }
    fn includes(&self, j: &Job) -> bool {
        match self {
            Self::All => true,
            Self::Unfinished => j.status != "complete",
            Self::Finished => j.status == "complete",
            Self::Failed => j.status == "failed",
            Self::Queued => j.status == "queued",
            Self::Category(s) => category(j) == s,
        }
    }
}
#[derive(Clone, Copy)]
enum Action {
    Add,
    Resume,
    Stop,
    StopAll,
    Delete,
    DeleteCompleted,
    Options,
    Scheduler,
    StartQueue,
    StopQueue,
    Properties,
    Browser,
    SelectAll,
}
struct Active {
    cancel: CancellationToken,
    progress: Progress,
    sampled: Instant,
    last_bytes: u64,
    rate: f64,
}
struct App {
    startup: startup::Startup,
    queue: Queue,
    runtime: tokio::runtime::Runtime,
    engine: Engine,
    jobs: Vec<Job>,
    active: HashMap<String, Active>,
    selected: HashSet<String>,
    anchor: Option<usize>,
    tx: mpsc::Sender<(String, Option<String>)>,
    rx: mpsc::Receiver<(String, Option<String>)>,
    prefs: Preferences,
    filter: Filter,
    sort: Field,
    descending: bool,
    search: String,
    message: String,
    auto_start: bool,
    last_refresh: Instant,
    last_queue_scan: Instant,
    show_add: bool,
    show_options: bool,
    show_scheduler: bool,
    show_browser: bool,
    browser_choice: usize,
    properties: Option<String>,
    url: String,
    output: String,
    expected_hash: String,
    description: String,
    new_category: String,
    start_immediately: bool,
    delay_minutes: u32,
    edit_description: String,
    edit_category: String,
}
fn filename(j: &Job) -> String {
    j.request
        .destination
        .file_name()
        .unwrap_or_default()
        .to_string_lossy()
        .into_owned()
}
fn category(j: &Job) -> &str {
    if j.category != "auto" {
        return &j.category;
    }
    match j
        .request
        .destination
        .extension()
        .and_then(|s| s.to_str())
        .unwrap_or("")
        .to_ascii_lowercase()
        .as_str()
    {
        "zip" | "7z" | "rar" | "gz" | "bz2" | "xz" | "tar" => "Compressed",
        "pdf" | "txt" | "doc" | "docx" | "xls" | "xlsx" | "csv" | "ppt" | "pptx" | "epub" => {
            "Documents"
        }
        "mp3" | "flac" | "wav" | "m4a" | "ogg" | "aac" => "Music",
        "exe" | "msi" | "dmg" | "pkg" | "deb" | "rpm" | "appimage" => "Programs",
        "mp4" | "mkv" | "webm" | "mov" | "avi" | "ts" | "m3u8" | "mpd" => "Video",
        _ => "Other",
    }
}

impl App {
    fn new(cc: &eframe::CreationContext<'_>) -> anyhow::Result<Self> {
        let mut prefs: Preferences = cc
            .storage
            .and_then(|s| s.get_string("rivet.preferences"))
            .and_then(|s| serde_json::from_str(&s).ok())
            .unwrap_or_default();
        prefs.row_height = prefs.row_height.clamp(18., 30.);
        prefs.connections = prefs.connections.clamp(1, 16);
        let mut seen = Vec::new();
        prefs.columns.retain(|c| {
            if seen.contains(c) {
                false
            } else {
                seen.push(*c);
                true
            }
        });
        if !prefs.columns.contains(&Field::Name) {
            prefs.columns.insert(0, Field::Name);
        }
        egui_extras::install_image_loaders(&cc.egui_ctx);
        theme(&cc.egui_ctx, prefs.dark);
        let queue = Queue::open(Queue::default_path()?)?;
        let jobs = queue.list()?;
        let runtime = tokio::runtime::Runtime::new()?;
        let engine = {
            let _guard = runtime.enter();
            Engine::new(None)?
        };
        let (tx, rx) = mpsc::channel();
        Ok(Self {
            startup: startup::Startup::new(false),
            queue,
            runtime,
            engine,
            jobs,
            active: HashMap::new(),
            selected: HashSet::new(),
            anchor: None,
            tx,
            rx,
            prefs,
            filter: Filter::All,
            sort: Field::Added,
            descending: true,
            search: String::new(),
            message: String::new(),
            auto_start: false,
            last_refresh: Instant::now(),
            last_queue_scan: Instant::now(),
            show_add: false,
            show_options: false,
            show_scheduler: false,
            show_browser: false,
            browser_choice: 0,
            properties: None,
            url: String::new(),
            output: String::new(),
            expected_hash: String::new(),
            description: String::new(),
            new_category: "auto".into(),
            start_immediately: false,
            delay_minutes: 0,
            edit_description: String::new(),
            edit_category: "auto".into(),
        })
    }
    fn refresh(&mut self) {
        match self.queue.list() {
            Ok(jobs) => {
                self.jobs = jobs;
                self.selected
                    .retain(|id| self.jobs.iter().any(|j| &j.id == id));
            }
            Err(e) => self.message = e.to_string(),
        }
        self.last_refresh = Instant::now();
    }
    fn tick(&mut self, ctx: &egui::Context) {
        while let Ok((id, error)) = self.rx.try_recv() {
            self.active.remove(&id);
            if let Some(e) = error {
                self.message = e;
            }
            self.refresh();
        }
        for a in self.active.values_mut() {
            let elapsed = a.sampled.elapsed().as_secs_f64();
            if elapsed >= 0.5 {
                let bytes = a.progress.snapshot().0;
                let instant = bytes.saturating_sub(a.last_bytes) as f64 / elapsed;
                a.rate = if a.last_bytes == 0 {
                    0.
                } else {
                    a.rate * 0.35 + instant * 0.65
                };
                a.last_bytes = bytes;
                a.sampled = Instant::now();
            }
        }
        if self.last_refresh.elapsed() > Duration::from_secs(1) {
            self.refresh();
        }
        if self.auto_start
            && self.active.is_empty()
            && self.last_queue_scan.elapsed() > Duration::from_millis(500)
        {
            self.last_queue_scan = Instant::now();
            match self.queue.next_queued() {
                Ok(Some(id)) => self.start(id, ctx.clone()),
                Err(e) => self.message = e.to_string(),
                _ => {}
            }
        }
    }
    fn start(&mut self, id: String, ctx: egui::Context) {
        if self.active.contains_key(&id) {
            return;
        }
        let active = Active {
            cancel: CancellationToken::new(),
            progress: Progress::default(),
            sampled: Instant::now(),
            last_bytes: 0,
            rate: 0.,
        };
        let (cancel, progress, queue, engine, tx, task_id) = (
            active.cancel.clone(),
            active.progress.clone(),
            self.queue.clone(),
            self.engine.clone(),
            self.tx.clone(),
            id.clone(),
        );
        self.active.insert(id, active);
        self.runtime.spawn(async move {
            let error = run_job(queue, &task_id, engine, cancel, progress)
                .await
                .err()
                .map(|e| safe_error(&e));
            let _ = tx.send((task_id, error));
            ctx.request_repaint();
        });
    }
    fn action(&mut self, action: Action, ctx: &egui::Context) {
        match action {
            Action::Add => self.show_add = true,
            Action::Resume => {
                for id in self.selected.clone() {
                    if self.active.contains_key(&id) {
                        continue;
                    }
                    if let Some(job) = self.jobs.iter().find(|j| j.id == id) {
                        if job.status == "complete" {
                            continue;
                        }
                        if matches!(job.status.as_str(), "paused" | "failed" | "running")
                            && let Err(e) = self.queue.retry(&id)
                        {
                            self.message = e.to_string();
                            continue;
                        }
                        self.start(id, ctx.clone());
                    }
                }
            }
            Action::Stop => {
                for id in &self.selected {
                    if let Some(a) = self.active.get(id) {
                        a.cancel.cancel();
                    }
                }
            }
            Action::StopAll | Action::StopQueue => {
                self.auto_start = false;
                for a in self.active.values() {
                    a.cancel.cancel();
                }
            }
            Action::Delete => {
                for id in &self.selected {
                    if let Err(e) = self.queue.remove(id) {
                        self.message = e.to_string();
                    }
                }
                self.refresh();
            }
            Action::DeleteCompleted => {
                for j in &self.jobs {
                    if j.status == "complete"
                        && let Err(e) = self.queue.remove(&j.id)
                    {
                        self.message = e.to_string();
                    }
                }
                self.refresh();
            }
            Action::Options => self.show_options = true,
            Action::Scheduler => self.show_scheduler = true,
            Action::StartQueue => self.auto_start = true,
            Action::Properties => {
                if let Some(id) = self.selected.iter().next() {
                    self.open_properties(id.clone());
                }
            }
            Action::Browser => self.show_browser = true,
            Action::SelectAll => {
                self.selected = self.visible().iter().map(|j| j.id.clone()).collect()
            }
        }
    }
    fn open_properties(&mut self, id: String) {
        if let Some(j) = self.jobs.iter().find(|j| j.id == id) {
            self.edit_description = j.description.clone();
            self.edit_category = j.category.clone();
            self.properties = Some(id);
        }
    }
    fn add(&mut self, ctx: &egui::Context) -> bool {
        if self.output.trim().is_empty() {
            self.suggest_destination();
        }
        let destination = PathBuf::from(self.output.trim());
        if !destination.is_absolute() {
            self.message = "Choose an absolute destination path, including the filename.".into();
            return false;
        }
        if let Some(parent) = destination.parent()
            && let Err(e) = std::fs::create_dir_all(parent)
        {
            self.message = e.to_string();
            return false;
        }
        let request = DownloadRequest {
            url: self.url.trim().into(),
            destination,
            connections: self.prefs.connections,
            expected_sha256: (!self.expected_hash.trim().is_empty())
                .then(|| self.expected_hash.trim().into()),
        };
        match self.queue.enqueue(
            request,
            now().saturating_add(i64::from(self.delay_minutes) * 60),
        ) {
            Ok(id) => {
                if let Err(e) = self
                    .queue
                    .set_metadata(&id, &self.new_category, &self.description)
                {
                    self.message = e.to_string();
                }
                self.selected = HashSet::from([id.clone()]);
                if self.start_immediately && self.delay_minutes == 0 {
                    self.start(id, ctx.clone());
                }
                self.url.clear();
                self.output.clear();
                self.expected_hash.clear();
                self.description.clear();
                self.message = "Download added.".into();
                self.refresh();
                true
            }
            Err(e) => {
                self.message = safe_error(&e);
                false
            }
        }
    }
    fn suggest_destination(&mut self) {
        if !self.output.trim().is_empty() {
            return;
        }
        let path = self.url.split(['?', '#']).next().unwrap_or("");
        if let Ok(folder) = native::default_download_dir() {
            let name = path
                .rsplit('/')
                .next()
                .filter(|s| !s.is_empty())
                .unwrap_or("download.bin");
            let safe: String = name
                .chars()
                .take(100)
                .map(|c| {
                    if c.is_ascii_alphanumeric() || matches!(c, '.' | '-' | '_') {
                        c
                    } else {
                        '_'
                    }
                })
                .collect();
            let safe = safe.trim_start_matches('.');
            self.output = folder
                .join(if safe.is_empty() {
                    "download.bin"
                } else {
                    safe
                })
                .display()
                .to_string();
        }
    }
    fn visible(&self) -> Vec<Job> {
        let search = self.search.to_lowercase();
        let mut jobs: Vec<_> = self
            .jobs
            .iter()
            .filter(|j| {
                self.filter.includes(j)
                    && (search.is_empty()
                        || format!("{} {} {}", filename(j), j.description, j.status)
                            .to_lowercase()
                            .contains(&search))
            })
            .cloned()
            .collect();
        jobs.sort_by(|a, b| {
            let order = match self.sort {
                Field::Name => filename(a).to_lowercase().cmp(&filename(b).to_lowercase()),
                Field::Size => self.metrics(a).1.cmp(&self.metrics(b).1),
                Field::Status => a.status.cmp(&b.status),
                Field::LastTry => a.last_try.cmp(&b.last_try),
                Field::Added => a.created.cmp(&b.created),
                Field::Description => a.description.cmp(&b.description),
                Field::Destination => a.request.destination.cmp(&b.request.destination),
                Field::Rate => self.metrics(a).2.total_cmp(&self.metrics(b).2),
                Field::Queue => (a.status == "queued").cmp(&(b.status == "queued")),
                Field::Remaining => {
                    let eta = |job: &Job| {
                        let (bytes, total, rate) = self.metrics(job);
                        if rate > 0. && total > 0 {
                            total.saturating_sub(bytes) as f64 / rate
                        } else {
                            f64::INFINITY
                        }
                    };
                    eta(a).total_cmp(&eta(b))
                }
            };
            if self.descending {
                order.reverse()
            } else {
                order
            }
        });
        jobs
    }
    fn metrics(&self, j: &Job) -> (u64, u64, f64) {
        self.active
            .get(&j.id)
            .map(|a| {
                let (b, t) = a.progress.snapshot();
                (b, t, a.rate)
            })
            .unwrap_or((j.bytes, j.total_bytes, 0.))
    }
    fn select(&mut self, index: usize, jobs: &[Job], m: egui::Modifiers) {
        let id = jobs[index].id.clone();
        if m.shift {
            if let Some(anchor) = self.anchor.filter(|a| *a < jobs.len()) {
                if !m.command {
                    self.selected.clear();
                }
                for j in &jobs[anchor.min(index)..=anchor.max(index).min(jobs.len() - 1)] {
                    self.selected.insert(j.id.clone());
                }
            }
        } else if m.command || m.ctrl {
            if !self.selected.remove(&id) {
                self.selected.insert(id);
            }
            self.anchor = Some(index);
        } else {
            self.selected = HashSet::from([id]);
            self.anchor = Some(index);
        }
    }
}

fn theme(ctx: &egui::Context, dark: bool) {
    let t = if dark {
        egui::Theme::Dark
    } else {
        egui::Theme::Light
    };
    ctx.set_theme(t);
    let mut v = if dark {
        egui::Visuals::dark()
    } else {
        egui::Visuals::light()
    };
    if dark {
        v.override_text_color = Some(Color32::from_rgb(216, 225, 234));
        v.panel_fill = Color32::from_rgb(24, 29, 36);
        v.window_fill = Color32::from_rgb(30, 36, 44);
        v.extreme_bg_color = Color32::from_rgb(19, 24, 30);
        v.text_edit_bg_color = Some(v.extreme_bg_color);
        v.widgets.inactive.bg_fill = Color32::from_rgb(40, 48, 59);
        v.widgets.inactive.weak_bg_fill = Color32::from_rgb(34, 42, 52);
        v.selection.bg_fill = Color32::from_rgb(31, 83, 118);
    } else {
        v.override_text_color = Some(Color32::from_rgb(36, 49, 66));
        v.panel_fill = Color32::from_rgb(242, 245, 249);
        v.window_fill = Color32::from_rgb(250, 251, 253);
        v.extreme_bg_color = Color32::WHITE;
        v.text_edit_bg_color = Some(Color32::WHITE);
        v.selection.bg_fill = Color32::from_rgb(211, 230, 250);
        v.selection.stroke.color = Color32::from_rgb(26, 73, 125);
        v.faint_bg_color = Color32::from_rgb(247, 249, 252);
    }
    ctx.set_visuals(v);
    ctx.style_mut_of(t, |s| {
        s.spacing.item_spacing = egui::vec2(6., 4.);
        s.spacing.interact_size.y = 22.;
        s.spacing.button_padding = egui::vec2(9., 4.);
        s.spacing.window_margin = egui::Margin::same(12);
        for t in [egui::TextStyle::Body, egui::TextStyle::Button] {
            s.text_styles.insert(t, egui::FontId::proportional(12.5));
        }
        s.text_styles
            .insert(egui::TextStyle::Heading, egui::FontId::proportional(16.));
    });
}
impl eframe::App for App {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();
        if self.startup.render(ui, &mut self.prefs) {
            ctx.request_repaint_after(Duration::from_millis(200));
            return;
        }
        self.tick(&ctx);
        self.render(ui);
        self.dialogs(&ctx);
        ctx.request_repaint_after(Duration::from_millis(200));
    }
    fn save(&mut self, storage: &mut dyn eframe::Storage) {
        if let Ok(value) = serde_json::to_string(&self.prefs) {
            storage.set_string("rivet.preferences", value);
        }
    }
}
impl Drop for App {
    fn drop(&mut self) {
        for a in self.active.values() {
            a.cancel.cancel();
        }
        self.runtime.block_on(async {
            let deadline = Instant::now() + Duration::from_secs(5);
            while !self.active.is_empty() && Instant::now() < deadline {
                while let Ok((id, _)) = self.rx.try_recv() {
                    self.active.remove(&id);
                }
                tokio::time::sleep(Duration::from_millis(20)).await;
            }
        });
    }
}
fn main() -> eframe::Result {
    eframe::run_native(
        "org.rivet.downloads",
        eframe::NativeOptions {
            persistence_path: std::env::var_os("RIVET_DATABASE")
                .map(PathBuf::from)
                .map(|p| p.with_file_name("ui-state.ron")),
            viewport: egui::ViewportBuilder::default()
                .with_title("Rivet — Download Manager")
                .with_inner_size([1320., 780.])
                .with_min_inner_size([980., 580.]),
            ..Default::default()
        },
        Box::new(|cc| Ok(Box::new(App::new(cc)?))),
    )
}
