use super::*;
use rivet_core::browser_setup::Browser;

impl App {
    pub(super) fn browser_dialog(&mut self, ctx: &egui::Context) {
        let mut open = self.show_browser;
        if !open {
            return;
        }
        let mut configure = None;
        egui::Window::new("Browser integration").open(&mut open).default_width(590.).show(ctx, |ui| {
            ui.horizontal(|ui| {
                ui.add(widgets::logo(40.));
                ui.vertical(|ui| { ui.heading("Connect Rivet to your browser"); ui.label("Rivet checks the extension and prepares the desktop connection automatically."); });
            });
            ui.add_space(12.);
            if ui.button("Check and configure my default browser automatically").clicked() { configure = Some(None); }
            ui.separator();
            ui.label("Set up an additional browser");
            ui.horizontal_wrapped(|ui| {
                for (index, name) in ["Chrome", "Edge", "Chromium", "Firefox"].iter().enumerate() { ui.selectable_value(&mut self.browser_choice, index, *name); }
            });
            if ui.button("Set up selected browser").clicked() { configure = Some(Some([Browser::Chrome, Browser::Edge, Browser::Chromium, Browser::Firefox][self.browser_choice])); }
            ui.add_space(10.);
            ui.label("Your browser asks you to approve extension installation. After approval, Rivet confirms the connection automatically. Store approval is required before normal installation is available.");
            ui.hyperlink_to("Browser setup help", "https://github.com/Mfuon2/rivet/blob/main/docs/BROWSER.md");
        });
        self.show_browser = open;
        if let Some(browser) = configure {
            self.prefs.browser_setup_deferred = false;
            if let Some(browser) = browser {
                self.startup.select_browser(browser);
            } else {
                self.startup.restart();
            }
            self.show_browser = false;
        }
    }
}
