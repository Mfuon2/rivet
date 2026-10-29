use super::*;
use crate::widgets::{self, Icon};
use egui::{RichText, Sense};
use egui_extras::{Column, TableBuilder};

impl App {
    pub(super) fn render(&mut self, ui: &mut egui::Ui) {
        let ctx = ui.ctx().clone();
        let mut action = None;
        if !ctx.egui_wants_keyboard_input() {
            ctx.input(|i| {
                if i.modifiers.command && i.key_pressed(egui::Key::N) {
                    action = Some(Action::Add);
                }
                if i.modifiers.command && i.key_pressed(egui::Key::A) {
                    action = Some(Action::SelectAll);
                }
                if i.key_pressed(egui::Key::Delete) {
                    action = Some(Action::Delete);
                }
                if i.key_pressed(egui::Key::Enter) && self.selected.len() == 1 {
                    action = Some(Action::Properties);
                }
            });
        }
        egui::Panel::top("menu")
            .frame(
                egui::Frame::new()
                    .fill(ui.visuals().panel_fill)
                    .inner_margin(egui::Margin::symmetric(8, 3)),
            )
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    for (title, items) in [
                        (
                            "Tasks",
                            vec![
                                ("Add URL…", Action::Add),
                                ("Select all", Action::SelectAll),
                                ("Properties…", Action::Properties),
                            ],
                        ),
                        (
                            "Downloads",
                            vec![
                                ("Resume selected", Action::Resume),
                                ("Stop selected", Action::Stop),
                                ("Stop all", Action::StopAll),
                                ("Delete entries", Action::Delete),
                                ("Delete completed", Action::DeleteCompleted),
                            ],
                        ),
                        (
                            "Queue",
                            vec![
                                ("Start queue", Action::StartQueue),
                                ("Stop queue", Action::StopQueue),
                                ("Scheduler…", Action::Scheduler),
                            ],
                        ),
                    ] {
                        ui.menu_button(title, |ui| {
                            for (label, value) in items {
                                if ui.button(label).clicked() {
                                    action = Some(value);
                                    ui.close();
                                }
                            }
                        });
                    }
                    ui.menu_button("View", |ui| {
                        ui.checkbox(&mut self.prefs.categories, "Categories");
                        ui.checkbox(&mut self.prefs.details, "Details pane");
                        if ui.checkbox(&mut self.prefs.dark, "Dark theme").changed() {
                            theme(&ctx, self.prefs.dark);
                        }
                        if ui.button("Columns and density…").clicked() {
                            action = Some(Action::Options);
                            ui.close();
                        }
                    });
                    ui.menu_button("Help", |ui| {
                        if ui.button("Browser integration…").clicked() {
                            action = Some(Action::Browser);
                            ui.close();
                        }
                        ui.add(widgets::logo(64.));
                        ui.label("Rivet 0.1 · MIT open source");
                        ui.label("No activation. No telemetry.");
                        ui.collapsing("Lucide icon license", |ui| {
                            egui::ScrollArea::vertical()
                                .max_height(240.)
                                .show(ui, |ui| {
                                    ui.label(include_str!("../assets/lucide/LICENSE"));
                                });
                        });
                    });
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        ui.add(widgets::logo(22.));
                        ui.label(
                            RichText::new("RIVET")
                                .strong()
                                .color(Color32::from_rgb(92, 187, 212)),
                        );
                    });
                });
            });
        let resumable = self.jobs.iter().any(|j| {
            self.selected.contains(&j.id)
                && j.status != "complete"
                && !self.active.contains_key(&j.id)
        });
        let stoppable = self.selected.iter().any(|id| self.active.contains_key(id));
        let deletable = self
            .jobs
            .iter()
            .any(|j| self.selected.contains(&j.id) && j.status != "running");
        egui::Panel::top("tools")
            .frame(
                egui::Frame::new()
                    .fill(ui.visuals().panel_fill)
                    .inner_margin(egui::Margin::symmetric(6, 3)),
            )
            .show(ui, |ui| {
                egui::ScrollArea::horizontal()
                    .id_salt("toolbar-scroll")
                    .show(ui, |ui| {
                        ui.horizontal(|ui| {
                            ui.spacing_mut().item_spacing.x = 1.;
                            for (label, icon, enabled, value) in [
                                ("Add URL", Icon::Add, true, Action::Add),
                                ("Resume", Icon::Resume, resumable, Action::Resume),
                                ("Stop", Icon::Stop, stoppable, Action::Stop),
                                (
                                    "Stop all",
                                    Icon::StopAll,
                                    !self.active.is_empty(),
                                    Action::StopAll,
                                ),
                                ("Delete", Icon::Delete, deletable, Action::Delete),
                                (
                                    "Delete completed",
                                    Icon::Clear,
                                    self.jobs.iter().any(|j| j.status == "complete"),
                                    Action::DeleteCompleted,
                                ),
                                ("Options", Icon::Options, true, Action::Options),
                                ("Scheduler", Icon::Clock, true, Action::Scheduler),
                                (
                                    "Start queue",
                                    Icon::StartQueue,
                                    !self.auto_start,
                                    Action::StartQueue,
                                ),
                                (
                                    "Stop queue",
                                    Icon::StopQueue,
                                    self.auto_start,
                                    Action::StopQueue,
                                ),
                            ] {
                                let response = widgets::tool(ui, label, icon, enabled);
                                if response.clicked() {
                                    action = Some(value);
                                }
                            }
                            widgets::tool(ui, "Grabber", Icon::Grabber, false)
                                .on_hover_text("Website grabbing is planned; see the roadmap.");
                            if widgets::tool(ui, "Browsers", Icon::Browser, true).clicked() {
                                action = Some(Action::Browser);
                            }
                        });
                    });
            });
        let total_rate: f64 = self.active.values().map(|a| a.rate).sum();
        egui::Panel::bottom("status")
            .exact_size(25.)
            .frame(
                egui::Frame::new()
                    .fill(ui.visuals().panel_fill)
                    .inner_margin(egui::Margin::symmetric(8, 3)),
            )
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.label(format!("{} items", self.jobs.len()));
                    ui.separator();
                    ui.label(format!("{} selected", self.selected.len()));
                    ui.separator();
                    ui.label(format!("{} active", self.active.len()));
                    ui.separator();
                    ui.label(format!("{}/s", widgets::bytes(total_rate as u64)));
                    ui.separator();
                    ui.label(if self.auto_start {
                        "Queue running"
                    } else {
                        "Queue stopped"
                    });
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        ui.small("Open source · No activation");
                    });
                });
            });
        if !self.message.is_empty() {
            egui::Panel::bottom("notice")
                .frame(
                    egui::Frame::new()
                        .fill(ui.visuals().panel_fill)
                        .inner_margin(egui::Margin::symmetric(8, 4)),
                )
                .show(ui, |ui| {
                    ui.horizontal(|ui| {
                        ui.label(RichText::new(&self.message).color(if self.prefs.dark {
                            Color32::from_rgb(236, 187, 112)
                        } else {
                            Color32::from_rgb(137, 81, 17)
                        }));
                        if ui.small_button("Dismiss").clicked() {
                            self.message.clear();
                        }
                    });
                });
        }
        if self.prefs.details {
            egui::Panel::bottom("details")
                .resizable(true)
                .default_size(108.)
                .min_size(75.)
                .max_size(240.)
                .show(ui, |ui| {
                    self.details(ui);
                });
        }
        if self.prefs.categories {
            egui::Panel::left("categories")
                .default_size(180.)
                .min_size(140.)
                .max_size(280.)
                .show(ui, |ui| {
                    ui.horizontal(|ui| {
                        ui.strong("Categories");
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            if ui.small_button("×").clicked() {
                                self.prefs.categories = false;
                            }
                        });
                    });
                    ui.separator();
                    let mut entry = |ui: &mut egui::Ui, filter: Filter, indent: bool| {
                        let n = self.jobs.iter().filter(|j| filter.includes(j)).count();
                        ui.horizontal(|ui| {
                            if indent {
                                ui.add_space(15.);
                            }
                            let (icon, color) = widgets::category_icon(filter.label());
                            ui.add(widgets::image(icon, 15., color));
                            if ui
                                .selectable_label(
                                    self.filter == filter,
                                    format!("{}  ({n})", filter.label()),
                                )
                                .clicked()
                            {
                                self.filter = filter;
                                self.selected.clear();
                                self.anchor = None;
                            }
                        });
                    };
                    entry(ui, Filter::All, false);
                    for name in [
                        "Compressed",
                        "Documents",
                        "Music",
                        "Programs",
                        "Video",
                        "Other",
                    ] {
                        entry(ui, Filter::Category(name.into()), true);
                    }
                    let mut custom: Vec<_> = self
                        .jobs
                        .iter()
                        .filter(|j| j.category != "auto")
                        .map(|j| j.category.clone())
                        .collect();
                    custom.sort();
                    custom.dedup();
                    for name in custom {
                        if ![
                            "Compressed",
                            "Documents",
                            "Music",
                            "Programs",
                            "Video",
                            "Other",
                        ]
                        .contains(&name.as_str())
                        {
                            entry(ui, Filter::Category(name), true);
                        }
                    }
                    ui.add_space(8.);
                    entry(ui, Filter::Unfinished, false);
                    entry(ui, Filter::Finished, false);
                    entry(ui, Filter::Failed, false);
                    ui.separator();
                    ui.strong("Queues");
                    entry(ui, Filter::Queued, true);
                    ui.add_space(8.);
                    ui.add_enabled(false, egui::Label::new("Grabber projects"))
                        .on_hover_text("Website grabbing is not implemented yet.");
                });
        }
        egui::CentralPanel::default()
            .frame(
                egui::Frame::new()
                    .fill(ui.visuals().extreme_bg_color)
                    .inner_margin(6),
            )
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.strong(self.filter.label());
                    ui.weak(format!("{} files", self.visible().len()));
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        let search_response = ui.add(
                            egui::TextEdit::singleline(&mut self.search)
                                .hint_text("Filter downloads…")
                                .desired_width(200.),
                        );
                        if search_response.changed() {
                            self.anchor = None;
                        }
                    });
                });
                ui.add_space(3.);
                self.table(ui, &mut action);
            });
        if let Some(action) = action {
            self.action(action, &ctx);
        }
    }

    fn details(&self, ui: &mut egui::Ui) {
        if let Some(job) = self.jobs.iter().find(|j| self.selected.contains(&j.id)) {
            let (bytes, total, rate) = self.metrics(job);
            ui.horizontal(|ui| {
                ui.strong(filename(job));
                ui.label(format!(
                    "· {} · {} connections",
                    category(job),
                    job.request.connections
                ));
            });
            ui.horizontal(|ui| {
                ui.label(format!("Downloaded: {}", widgets::bytes(bytes)));
                ui.separator();
                ui.label(format!("Rate: {}/s", widgets::bytes(rate as u64)));
                ui.separator();
                ui.label(if total > 0 && rate > 0. {
                    format!(
                        "Time left: {}",
                        widgets::duration(total.saturating_sub(bytes) as f64 / rate)
                    )
                } else {
                    "Time left: —".into()
                });
            });
            if total > 0 {
                ui.add(
                    egui::ProgressBar::new((bytes as f64 / total as f64).min(1.) as f32)
                        .text(format!(
                            "{} / {}",
                            widgets::bytes(bytes),
                            widgets::bytes(total)
                        ))
                        .desired_height(16.),
                );
            }
            ui.label(egui::RichText::new(job.request.destination.display().to_string()).small());
            if let Some(hash) = &job.sha256 {
                ui.small(format!("SHA-256: {hash}"));
            } else if let Some(error) = &job.error {
                ui.colored_label(Color32::LIGHT_RED, error);
            } else {
                ui.small(&job.description);
            }
        } else {
            ui.strong("Download details");
            ui.small("Select a row to inspect transfer status, destination and integrity. Double-click for properties.");
        }
    }

    fn table(&mut self, ui: &mut egui::Ui, action: &mut Option<Action>) {
        let ctx = ui.ctx().clone();
        let jobs = self.visible();
        let columns = self.prefs.columns.clone();
        let height = ui.available_height();
        egui::ScrollArea::horizontal()
            .id_salt("horizontal-table")
            .auto_shrink([false, false])
            .show(ui, |ui| {
                let minimum: f32 = columns.iter().map(|f| f.width()).sum();
                ui.set_min_width(minimum.max(ui.available_width()));
                let mut table = TableBuilder::new(ui)
                    .id_salt("transfers")
                    .striped(true)
                    .resizable(true)
                    .sense(Sense::click())
                    .cell_layout(egui::Layout::left_to_right(egui::Align::Center))
                    .max_scroll_height((height - 29.).max(50.));
                for (index, field) in columns.iter().enumerate() {
                    table = table.column(if index == columns.len() - 1 {
                        Column::remainder().at_least(field.width())
                    } else {
                        Column::initial(field.width())
                            .at_least(if *field == Field::Queue { 22. } else { 55. })
                            .clip(true)
                    });
                }
                table
                    .header(25., |mut header| {
                        for field in &columns {
                            header.col(|ui| {
                                let label = if self.sort == *field {
                                    format!(
                                        "{} {}",
                                        field.label(),
                                        if self.descending { "↓" } else { "↑" }
                                    )
                                } else {
                                    field.label().into()
                                };
                                let response = ui.add(
                                    egui::Button::new(RichText::new(label).strong()).frame(false),
                                );
                                if response.clicked() {
                                    if self.sort == *field {
                                        self.descending = !self.descending;
                                    } else {
                                        self.sort = *field;
                                        self.descending = false;
                                    }
                                    self.anchor = None;
                                }
                                response.context_menu(|ui| {
                                    if ui.button("Choose / reorder columns…").clicked() {
                                        *action = Some(Action::Options);
                                        ui.close();
                                    }
                                });
                            });
                        }
                    })
                    .body(|body| {
                        body.rows(self.prefs.row_height, jobs.len(), |mut row| {
                            let index = row.index();
                            let job = &jobs[index];
                            row.set_selected(self.selected.contains(&job.id));
                            let (bytes, total, rate) = self.metrics(job);
                            for field in &columns {
                                row.col(|ui| match field {
                                    Field::Name => {
                                        let (icon, color) = widgets::category_icon(category(job));
                                        ui.add(widgets::image(icon, 15., color));
                                        ui.add(egui::Label::new(filename(job)).truncate())
                                            .on_hover_text(
                                                job.request.destination.display().to_string(),
                                            );
                                    }
                                    Field::Queue => {
                                        ui.label(if job.status == "queued" { "Q" } else { "" });
                                    }
                                    Field::Size => {
                                        ui.label(if total > 0 || job.status == "complete" {
                                            widgets::bytes(total)
                                        } else {
                                            "—".into()
                                        });
                                    }
                                    Field::Status => {
                                        if self.active.contains_key(&job.id) && total > 0 {
                                            ui.add(
                                                egui::ProgressBar::new(
                                                    (bytes as f64 / total as f64).min(1.) as f32,
                                                )
                                                .text(format!(
                                                    "{:.1}%",
                                                    bytes as f64 * 100. / total as f64
                                                ))
                                                .desired_height(14.)
                                                .desired_width(ui.available_width()),
                                            );
                                        } else {
                                            let label = if self.active.contains_key(&job.id) {
                                                "Connecting"
                                            } else {
                                                match job.status.as_str() {
                                                    "complete" => "Complete",
                                                    "queued" if job.not_before > now() => {
                                                        "Scheduled"
                                                    }
                                                    "queued" => "Queued",
                                                    "running" => "Running",
                                                    "paused" => "Paused",
                                                    "failed" => "Failed",
                                                    s => s,
                                                }
                                            };
                                            let color = match job.status.as_str() {
                                                "complete" => Color32::from_rgb(48, 151, 105),
                                                "failed" => Color32::from_rgb(201, 83, 73),
                                                _ => ui.visuals().text_color(),
                                            };
                                            ui.colored_label(color, label);
                                        }
                                    }
                                    Field::Remaining => {
                                        ui.label(if rate > 0. && total > 0 {
                                            widgets::duration(
                                                total.saturating_sub(bytes) as f64 / rate,
                                            )
                                        } else {
                                            "—".into()
                                        });
                                    }
                                    Field::Rate => {
                                        ui.label(if rate > 0. {
                                            format!("{}/s", widgets::bytes(rate as u64))
                                        } else {
                                            "—".into()
                                        });
                                    }
                                    Field::LastTry => {
                                        ui.label(widgets::date(job.last_try));
                                    }
                                    Field::Added => {
                                        ui.label(widgets::date(job.created));
                                    }
                                    Field::Description => {
                                        ui.add(egui::Label::new(&job.description).truncate());
                                    }
                                    Field::Destination => {
                                        ui.add(
                                            egui::Label::new(
                                                job.request.destination.display().to_string(),
                                            )
                                            .truncate(),
                                        );
                                    }
                                });
                            }
                            let response = row.response();
                            if response.clicked() {
                                self.select(index, &jobs, ctx.input(|i| i.modifiers));
                            }
                            if response.double_clicked() {
                                self.selected = HashSet::from([job.id.clone()]);
                                self.open_properties(job.id.clone());
                            }
                            if response.secondary_clicked() && !self.selected.contains(&job.id) {
                                self.selected = HashSet::from([job.id.clone()]);
                            }
                            response.context_menu(|ui| {
                                for (label, value) in [
                                    ("Resume", Action::Resume),
                                    ("Stop", Action::Stop),
                                    ("Properties…", Action::Properties),
                                    ("Schedule…", Action::Scheduler),
                                    ("Delete entry", Action::Delete),
                                ] {
                                    if ui.button(label).clicked() {
                                        *action = Some(value);
                                        ui.close();
                                    }
                                }
                            });
                        });
                    });
                if jobs.is_empty() {
                    ui.add_space(9.);
                    ui.weak(
                        "No downloads in this view. Use Add URL, or send a link from your browser.",
                    );
                }
            });
    }

    pub(super) fn dialogs(&mut self, ctx: &egui::Context) {
        let mut open = self.show_add;
        let mut close = false;
        if open {
            egui::Window::new("Add URL")
                .id(egui::Id::unique("add-url"))
                .open(&mut open)
                .collapsible(false)
                .resizable(false)
                .default_width(570.)
                .show(ctx, |ui| {
                    egui::Grid::new("add-grid")
                        .num_columns(2)
                        .spacing([12., 9.])
                        .show(ui, |ui| {
                            ui.label("Address");
                            let response = ui.add(
                                egui::TextEdit::singleline(&mut self.url)
                                    .desired_width(445.)
                                    .hint_text("https://…"),
                            );
                            if response.lost_focus() {
                                self.suggest_destination();
                            }
                            ui.end_row();
                            ui.label("Save to");
                            ui.add(
                                egui::TextEdit::singleline(&mut self.output).desired_width(445.),
                            );
                            ui.end_row();
                            ui.label("Category");
                            category_picker(ui, "new-category", &mut self.new_category);
                            ui.end_row();
                            ui.label("Description");
                            ui.add(
                                egui::TextEdit::singleline(&mut self.description)
                                    .desired_width(445.),
                            );
                            ui.end_row();
                            ui.label("Connections");
                            ui.add(egui::Slider::new(&mut self.prefs.connections, 1..=16));
                            ui.end_row();
                            ui.label("Start after");
                            ui.horizontal(|ui| {
                                ui.add(
                                    egui::DragValue::new(&mut self.delay_minutes).range(0..=10080),
                                );
                                ui.label("minutes (0 = now)");
                            });
                            ui.end_row();
                        });
                    ui.collapsing("Integrity check", |ui| {
                        ui.add(
                            egui::TextEdit::singleline(&mut self.expected_hash)
                                .hint_text("Publisher SHA-256 (optional)")
                                .desired_width(f32::INFINITY),
                        );
                    });
                    ui.checkbox(
                        &mut self.start_immediately,
                        "Start immediately when no delay is set",
                    );
                    ui.separator();
                    ui.horizontal(|ui| {
                        if ui.button("Add download").clicked() {
                            close = self.add(ctx);
                        }
                        if ui.button("Cancel").clicked() {
                            close = true;
                        }
                    });
                    if !self.message.is_empty() {
                        ui.small(&self.message);
                    }
                });
        }
        self.show_add = open && !close;
        self.options(ctx);
        self.scheduler(ctx);
        self.properties_dialog(ctx);
        self.browser_dialog(ctx);
    }

    fn options(&mut self, ctx: &egui::Context) {
        let mut open = self.show_options;
        if !open {
            return;
        }
        egui::Window::new("Options").open(&mut open).default_width(520.).show(ctx,|ui|{
            ui.horizontal(|ui|{ui.strong("Appearance");if ui.checkbox(&mut self.prefs.dark,"Dark theme").changed(){theme(ctx,self.prefs.dark);}});
            ui.add(egui::Slider::new(&mut self.prefs.row_height,18.0..=30.0).text("Row height"));ui.checkbox(&mut self.prefs.categories,"Show category tree");ui.checkbox(&mut self.prefs.details,"Show download details");
            ui.add(egui::Slider::new(&mut self.prefs.connections,1..=16).text("Default connections"));ui.small("More connections can improve throughput when the server and network allow it. One connection is the baseline.");
            ui.separator();ui.strong("Columns");ui.small("Drag table dividers to resize. Use arrows here to change order.");
            let mut fields=self.prefs.columns.clone();for f in Field::ALL{if !fields.contains(&f){fields.push(f);}}
            for field in fields{ui.horizontal(|ui|{let mut visible=self.prefs.columns.contains(&field);let r=ui.add_enabled(field!=Field::Name,egui::Checkbox::new(&mut visible,field.label()));if r.changed(){if visible{self.prefs.columns.push(field);}else{self.prefs.columns.retain(|f|*f!=field);}}
                if let Some(i)=self.prefs.columns.iter().position(|f|*f==field){if ui.add_enabled(i>0,egui::Button::new("↑")).clicked(){self.prefs.columns.swap(i,i-1);}
if ui.add_enabled(i+1<self.prefs.columns.len(),egui::Button::new("↓")).clicked(){self.prefs.columns.swap(i,i+1);}}});}
            ui.separator();ui.small("Preferences persist across restarts. Recurring schedules, bandwidth budgets and authenticated profiles remain on the roadmap.");
        });
        self.show_options = open;
    }
    fn scheduler(&mut self, ctx: &egui::Context) {
        let mut open = self.show_scheduler;
        if !open {
            return;
        }
        egui::Window::new("Scheduler — Main download queue").open(&mut open).default_width(620.).show(ctx,|ui|{
            ui.horizontal(|ui|{ui.label("Start selected downloads after");ui.add(egui::DragValue::new(&mut self.delay_minutes).range(0..=10080));ui.label("minutes");});
            if ui.add_enabled(!self.selected.is_empty(),egui::Button::new("Schedule selected")).clicked(){for id in &self.selected{if let Err(e)=self.queue.schedule(id,now()+i64::from(self.delay_minutes)*60){self.message=e.to_string();}}self.refresh();}
            ui.checkbox(&mut self.auto_start,"Process due queued jobs automatically");ui.separator();
            egui::ScrollArea::vertical().max_height(220.).show(ui,|ui|{for job in &self.jobs{if job.status=="queued"{ui.horizontal(|ui|{ui.label(filename(job));ui.label(if job.not_before>now(){format!("Due {}",widgets::date(job.not_before))}else{"Ready".into()});});}}});
            ui.separator();ui.small("One-time scheduling is available. Named queues, recurring synchronization and completion power actions are planned.");
        });
        self.show_scheduler = open;
    }
    fn properties_dialog(&mut self, ctx: &egui::Context) {
        let Some(id) = self.properties.clone() else {
            return;
        };
        let Some(job) = self.jobs.iter().find(|j| j.id == id).cloned() else {
            self.properties = None;
            return;
        };
        let mut open = true;
        egui::Window::new("Download properties").id(egui::Id::unique("properties")).open(&mut open).default_width(640.).show(ctx,|ui|{
            ui.strong(filename(&job));ui.label(job.request.destination.display().to_string());
            ui.horizontal(|ui|{ui.label(format!("Status: {}",job.status));ui.separator();ui.label(format!("Added: {}",widgets::date(job.created)));});
            if ui.button("Copy source URL").clicked(){ctx.copy_text(job.request.url.clone());}
            ui.small("Source query parameters are hidden here; copied URLs may contain private access tokens.");ui.separator();
            ui.horizontal(|ui|{ui.label("Category");category_picker(ui,"property-category",&mut self.edit_category);});ui.label("Description");ui.add(egui::TextEdit::multiline(&mut self.edit_description).desired_rows(2).desired_width(f32::INFINITY));
            if let Some(hash)=&job.sha256{ui.label("SHA-256");ui.monospace(hash);}
            if let Some(error)=&job.error{ui.colored_label(Color32::LIGHT_RED,error);}
            if ui.button("Save metadata").clicked(){match self.queue.set_metadata(&id,&self.edit_category,&self.edit_description){Ok(())=>{self.message="Properties saved.".into();self.refresh();},Err(e)=>self.message=e.to_string()}}
        });
        if !open {
            self.properties = None;
        }
    }
}

fn category_picker(ui: &mut egui::Ui, id: &str, value: &mut String) {
    egui::ComboBox::from_id_salt(id)
        .selected_text(if value == "auto" {
            "Automatic"
        } else {
            value.as_str()
        })
        .show_ui(ui, |ui| {
            for category in [
                "auto",
                "Compressed",
                "Documents",
                "Music",
                "Programs",
                "Video",
                "Other",
            ] {
                ui.selectable_value(
                    value,
                    category.into(),
                    if category == "auto" {
                        "Automatic"
                    } else {
                        category
                    },
                );
            }
        });
}
