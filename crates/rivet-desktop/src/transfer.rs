use super::*;
use rivet_core::TransferState;

pub(super) struct TransferView {
    pub id: String,
    pub progress: Progress,
    expanded: bool,
}

impl App {
    pub(super) fn open_transfer(&mut self, id: String) {
        if self.transfer.as_ref().is_some_and(|view| view.id == id) {
            return;
        }
        if let Some(progress) = self.transfer_progress.get(&id) {
            self.transfer = Some(TransferView {
                id,
                progress: progress.clone(),
                expanded: true,
            });
        }
    }

    pub(super) fn transfer_dialog(&mut self, ctx: &egui::Context) {
        let Some(mut view) = self.transfer.take() else {
            return;
        };
        let Some(job) = self.jobs.iter().find(|job| job.id == view.id).cloned() else {
            return;
        };
        let snapshot = view.progress.transfer_snapshot();
        let (bytes, total) = view.progress.snapshot();
        let active = self.active.get(&job.id);
        let rate = active.map_or(0., |a| a.rate);
        let pausing = active.is_some_and(|a| a.cancel.is_cancelled());
        let running = active.is_some();
        let mut open = true;
        let mut close = false;
        let mut pause = false;
        let mut resume = false;
        egui::Window::new("Download progress")
            .id(egui::Id::unique("transfer-progress"))
            .open(&mut open)
            .collapsible(false)
            .default_width(740.)
            .default_height(620.)
            .max_width((ctx.content_rect().width() - 32.).max(300.))
            .max_height((ctx.content_rect().height() - 48.).max(240.))
            .vscroll(true)
            .default_pos([260., 135.])
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    ui.add(widgets::logo(36.));
                    ui.vertical(|ui| {
                        ui.heading(filename(&job));
                        ui.label(if pausing {
                            "Pausing…"
                        } else {
                            snapshot.state.label()
                        });
                    });
                });
                // Signed query parameters never appear in the visible source address.
                ui.add(
                    egui::Label::new(job.request.url.split(['?', '#']).next().unwrap_or(""))
                        .truncate(),
                );
                ui.add_space(10.);
                egui::Grid::new("transfer-summary")
                    .num_columns(2)
                    .spacing([24., 7.])
                    .show(ui, |ui| {
                        ui.label("File size");
                        ui.strong(if total > 0 || snapshot.state == TransferState::Complete {
                            widgets::bytes(total)
                        } else {
                            "Not yet known".into()
                        });
                        ui.end_row();
                        ui.label("Downloaded");
                        ui.strong(widgets::bytes(bytes));
                        ui.end_row();
                        ui.label("Transfer rate");
                        ui.strong(format!("{}/s", widgets::bytes(rate as u64)));
                        ui.end_row();
                        ui.label("Time remaining");
                        ui.strong(if total > bytes && rate > 0. {
                            widgets::duration((total - bytes) as f64 / rate)
                        } else {
                            "—".into()
                        });
                        ui.end_row();
                        ui.label("Resume capability");
                        ui.label(if snapshot.segments.is_empty() {
                            "Checking server…"
                        } else if snapshot.resumable {
                            "Supported — validated byte ranges"
                        } else {
                            "Single stream — restarts when resumed"
                        });
                        ui.end_row();
                    });
                ui.add_space(10.);
                let fraction = if total > 0 {
                    (bytes as f64 / total as f64).min(1.) as f32
                } else {
                    0.
                };
                let bar = egui::ProgressBar::new(fraction).desired_height(19.);
                ui.add(if total > 0 {
                    bar.text(format!(
                        "{:.1}% · {} / {}",
                        fraction * 100.,
                        widgets::bytes(bytes),
                        widgets::bytes(total)
                    ))
                } else {
                    bar.text(widgets::bytes(bytes)).animate(running)
                });
                ui.add_space(8.);
                ui.horizontal(|ui| {
                    if ui
                        .button(if view.expanded {
                            "Hide segments"
                        } else {
                            "Show segments"
                        })
                        .clicked()
                    {
                        view.expanded = !view.expanded;
                    }
                    pause = ui
                        .add_enabled(
                            running && !pausing,
                            egui::Button::new(if pausing { "Pausing…" } else { "Pause" }),
                        )
                        .clicked();
                    resume = ui
                        .add_enabled(
                            !running
                                && matches!(
                                    snapshot.state,
                                    TransferState::Paused | TransferState::Failed
                                ),
                            egui::Button::new("Resume download"),
                        )
                        .clicked();
                    if ui.button("Close").clicked() {
                        close = true;
                    }
                });
                if view.expanded {
                    ui.separator();
                    let receiving = snapshot
                        .segments
                        .iter()
                        .filter(|s| {
                            matches!(
                                s.state,
                                TransferState::Receiving | TransferState::Connecting
                            )
                        })
                        .count();
                    ui.strong(format!(
                        "{} segments · {} active · up to {} connections",
                        snapshot.segments.len(),
                        receiving,
                        job.request.connections
                    ));
                    if snapshot.segments.is_empty() {
                        ui.label("Waiting for the server to confirm the download plan…");
                    } else {
                        segment_map(ui, &snapshot.segments);
                        egui::ScrollArea::both()
                            .id_salt("transfer-segments-scroll")
                            .max_height(230.)
                            .show(ui, |ui| {
                                egui::Grid::new("transfer-segments")
                                    .striped(true)
                                    .spacing([18., 7.])
                                    .show(ui, |ui| {
                                        for title in [
                                            "Segment",
                                            "Byte range",
                                            "Downloaded",
                                            "Status",
                                            "Progress",
                                        ] {
                                            ui.strong(title);
                                        }
                                        ui.end_row();
                                        for segment in &snapshot.segments {
                                            ui.label(segment.id.to_string());
                                            ui.monospace(segment.end.map_or_else(
                                                || format!("{}–…", segment.start),
                                                |end| format!("{}–{end}", segment.start),
                                            ));
                                            ui.label(widgets::bytes(segment.downloaded));
                                            ui.label(segment.state.label());
                                            if let Some(size) = segment.total {
                                                let value = if size == 0 {
                                                    1.
                                                } else {
                                                    (segment.downloaded as f64 / size as f64)
                                                        .min(1.)
                                                        as f32
                                                };
                                                ui.add(
                                                    egui::ProgressBar::new(value)
                                                        .desired_width(100.)
                                                        .show_percentage(),
                                                );
                                            } else {
                                                ui.label("Size unknown");
                                            }
                                            ui.end_row();
                                        }
                                    });
                            });
                    }
                }
                if let Some(error) = &job.error
                    && job.status == "failed"
                {
                    ui.colored_label(Color32::from_rgb(194, 76, 69), error);
                }
                if let Some(hash) = &job.sha256 {
                    ui.small(format!("SHA-256: {hash}"));
                }
                ui.small(
                    "Closing this window keeps the download running. Pause keeps partial data.",
                );
            });
        if pause && let Some(active) = self.active.get(&view.id) {
            active.cancel.cancel();
        }
        let id = view.id.clone();
        if open && !close {
            self.transfer = Some(view);
        }
        if resume {
            match self.queue.retry(&id) {
                Ok(()) => {
                    self.refresh();
                    self.start(id, ctx.clone());
                }
                Err(error) => self.message = error.to_string(),
            }
        }
        if running {
            ctx.request_repaint_after(Duration::from_millis(100));
        }
    }
}

fn segment_map(ui: &mut egui::Ui, segments: &[rivet_core::SegmentSnapshot]) {
    let (rect, _) =
        ui.allocate_exact_size(egui::vec2(ui.available_width(), 15.), egui::Sense::hover());
    let total: u64 = segments.iter().filter_map(|s| s.total).sum();
    for (index, segment) in segments.iter().enumerate() {
        let (start, width) = if total > 0 {
            (
                segment.start as f32 / total as f32,
                segment.total.unwrap_or(0) as f32 / total as f32,
            )
        } else {
            (
                index as f32 / segments.len() as f32,
                1. / segments.len() as f32,
            )
        };
        let cell = egui::Rect::from_min_size(
            egui::pos2(rect.left() + rect.width() * start, rect.top()),
            egui::vec2((rect.width() * width - 1.).max(1.), rect.height()),
        );
        ui.painter()
            .rect_filled(cell, 1., ui.visuals().extreme_bg_color);
        let fraction = segment
            .total
            .filter(|n| *n > 0)
            .map_or(0., |n| (segment.downloaded as f32 / n as f32).min(1.));
        let color = match segment.state {
            TransferState::Complete => Color32::from_rgb(48, 151, 105),
            TransferState::Failed => Color32::from_rgb(194, 76, 69),
            _ => Color32::from_rgb(42, 143, 194),
        };
        ui.painter().rect_filled(
            egui::Rect::from_min_size(cell.min, egui::vec2(cell.width() * fraction, cell.height())),
            1.,
            color,
        );
        ui.interact(cell, ui.scope_id().with(segment.id), egui::Sense::hover())
            .on_hover_text(format!(
                "Segment {}: {} — {}",
                segment.id,
                widgets::bytes(segment.downloaded),
                segment.state.label()
            ));
    }
}
