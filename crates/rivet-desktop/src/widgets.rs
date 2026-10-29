use eframe::egui::{self, Color32, Response, Sense, Vec2};
#[derive(Clone, Copy)]
pub enum Icon {
    Add,
    Resume,
    Stop,
    StopAll,
    Delete,
    Clear,
    Options,
    Clock,
    StartQueue,
    StopQueue,
    Grabber,
    Browser,
}
impl Icon {
    fn name(self) -> &'static str {
        match self {
            Self::Add => "plus",
            Self::Resume => "play",
            Self::Stop => "square",
            Self::StopAll => "list-x",
            Self::Delete => "trash-2",
            Self::Clear => "list-checks",
            Self::Options => "settings",
            Self::Clock => "alarm-clock",
            Self::StartQueue => "list-start",
            Self::StopQueue => "list-end",
            Self::Grabber => "scan-search",
            Self::Browser => "panels-top-left",
        }
    }
}
// Embedded, versioned Lucide SVGs. No runtime filesystem or network dependency.
pub fn image(name: &str, size: f32, tint: Color32) -> egui::Image<'static> {
    let svg = match name {
        "plus" => include_str!("../assets/lucide/plus.svg"),
        "play" => include_str!("../assets/lucide/play.svg"),
        "square" => include_str!("../assets/lucide/square.svg"),
        "list-x" => include_str!("../assets/lucide/list-x.svg"),
        "trash-2" => include_str!("../assets/lucide/trash-2.svg"),
        "list-checks" => include_str!("../assets/lucide/list-checks.svg"),
        "settings" => include_str!("../assets/lucide/settings.svg"),
        "alarm-clock" => include_str!("../assets/lucide/alarm-clock.svg"),
        "list-start" => include_str!("../assets/lucide/list-start.svg"),
        "list-end" => include_str!("../assets/lucide/list-end.svg"),
        "scan-search" => include_str!("../assets/lucide/scan-search.svg"),
        "panels-top-left" => include_str!("../assets/lucide/panels-top-left.svg"),
        "folder" => include_str!("../assets/lucide/folder.svg"),
        "folder-archive" => include_str!("../assets/lucide/folder-archive.svg"),
        "file-text" => include_str!("../assets/lucide/file-text.svg"),
        "music" => include_str!("../assets/lucide/music.svg"),
        "app-window" => include_str!("../assets/lucide/app-window.svg"),
        "film" => include_str!("../assets/lucide/film.svg"),
        "file" => include_str!("../assets/lucide/file.svg"),
        "circle-check" => include_str!("../assets/lucide/circle-check.svg"),
        "circle-alert" => include_str!("../assets/lucide/circle-alert.svg"),
        "clock" => include_str!("../assets/lucide/clock.svg"),
        "download" => include_str!("../assets/lucide/download.svg"),
        "search" => include_str!("../assets/lucide/search.svg"),
        "chevron-right" => include_str!("../assets/lucide/chevron-right.svg"),
        _ => include_str!("../assets/lucide/file.svg"),
    };
    let uri = format!("bytes://lucide/{name}.svg");
    egui::Image::from_bytes(uri, svg.replace("currentColor", "#ffffff").into_bytes())
        .fit_to_exact_size(egui::vec2(size, size))
        .tint(tint)
}
pub fn category_icon(category: &str) -> (&'static str, Color32) {
    match category {
        "Compressed" => ("folder-archive", Color32::from_rgb(164, 119, 25)),
        "Documents" => ("file-text", Color32::from_rgb(47, 117, 189)),
        "Music" => ("music", Color32::from_rgb(151, 86, 180)),
        "Programs" => ("app-window", Color32::from_rgb(49, 139, 113)),
        "Video" => ("film", Color32::from_rgb(192, 98, 59)),
        "All downloads" => ("folder", Color32::from_rgb(164, 119, 25)),
        "Finished" => ("circle-check", Color32::from_rgb(49, 139, 113)),
        "Failed" => ("circle-alert", Color32::from_rgb(194, 76, 69)),
        "Unfinished" => ("clock", Color32::from_rgb(47, 117, 189)),
        "Main download queue" => ("list-start", Color32::from_rgb(47, 117, 189)),
        _ => ("file", Color32::from_rgb(116, 132, 150)),
    }
}
pub fn tool(ui: &mut egui::Ui, label: &str, icon: Icon, enabled: bool) -> Response {
    let width = match icon {
        Icon::Clear => 106.,
        Icon::StartQueue | Icon::StopQueue => 86.,
        _ => 70.,
    };
    let (rect, response) = ui.allocate_exact_size(
        Vec2::new(width, 60.),
        if enabled {
            Sense::click()
        } else {
            Sense::hover()
        },
    );
    response.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Button, enabled, label));
    let color = if !enabled {
        ui.visuals().weak_text_color().gamma_multiply(0.45)
    } else {
        match icon {
            Icon::Stop | Icon::StopAll | Icon::StopQueue | Icon::Delete => {
                Color32::from_rgb(201, 83, 73)
            }
            Icon::Resume | Icon::StartQueue | Icon::Add => Color32::from_rgb(48, 151, 105),
            Icon::Clock | Icon::Clear => Color32::from_rgb(192, 135, 37),
            _ => Color32::from_rgb(66, 130, 193),
        }
    };
    if response.hovered() && enabled {
        ui.painter().rect_filled(
            rect.shrink(1.),
            4.,
            ui.visuals().widgets.hovered.weak_bg_fill,
        );
    }
    image(icon.name(), 26., color).paint_at(
        ui,
        egui::Rect::from_center_size(
            rect.center_top() + egui::vec2(0., 20.),
            egui::vec2(26., 26.),
        ),
    );
    ui.painter().text(
        rect.center_bottom() - egui::vec2(0., 10.),
        egui::Align2::CENTER_CENTER,
        label,
        egui::FontId::proportional(11.5),
        if enabled {
            ui.visuals().text_color()
        } else {
            ui.visuals().weak_text_color().gamma_multiply(0.6)
        },
    );
    response.on_hover_text(label)
}
pub fn bytes(v: u64) -> String {
    if v >= 1 << 30 {
        format!("{:.2} GiB", v as f64 / (1u64 << 30) as f64)
    } else if v >= 1 << 20 {
        format!("{:.2} MiB", v as f64 / (1u64 << 20) as f64)
    } else if v >= 1 << 10 {
        format!("{:.1} KiB", v as f64 / 1024.)
    } else {
        format!("{v} B")
    }
}
pub fn duration(seconds: f64) -> String {
    if !seconds.is_finite() || seconds < 0. {
        return "—".into();
    }
    let s = seconds as u64;
    if s >= 3600 {
        format!("{}h {:02}m", s / 3600, s % 3600 / 60)
    } else if s >= 60 {
        format!("{}m {:02}s", s / 60, s % 60)
    } else {
        format!("{s}s")
    }
}
pub fn date(seconds: i64) -> String {
    chrono::DateTime::from_timestamp(seconds, 0)
        .filter(|_| seconds > 0)
        .map(|d| {
            d.with_timezone(&chrono::Local)
                .format("%b %d %H:%M")
                .to_string()
        })
        .unwrap_or_else(|| "—".into())
}
