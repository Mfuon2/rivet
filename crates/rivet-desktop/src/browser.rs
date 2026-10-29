use super::*;

impl App {
    pub(super) fn browser_dialog(&mut self, ctx: &egui::Context) {
        let mut open = self.show_browser;
        if !open {
            return;
        }
        egui::Window::new("Browser integration")
            .open(&mut open).default_width(650.).resizable(true).show(ctx, |ui| {
                ui.horizontal(|ui| {
                    ui.add(widgets::logo(36.));
                    ui.vertical(|ui| {
                        ui.strong("Send browser downloads to Rivet");
                        ui.label("Install the Rivet extension, then connect its native helper.");
                    });
                });
                ui.separator();
                if ui.button("Check and configure my default browser automatically").clicked() {
                    self.startup.restart();
                    self.show_browser = false;
                }
                ui.small("The guide below is for advanced or additional-browser setup.");
                ui.horizontal_wrapped(|ui| {
                    for (index, name) in ["Chrome", "Edge", "Chromium", "Firefox"].iter().enumerate() {
                        ui.selectable_value(&mut self.browser_choice, index, *name);
                    }
                });
                let firefox = self.browser_choice == 3;
                let page = ["chrome://extensions", "edge://extensions", "chrome://extensions", "about:debugging#/runtime/this-firefox"][self.browser_choice];
                egui::ScrollArea::vertical().max_height(490.).show(ui, |ui| {
                    ui.add_space(8.);
                    ui.strong("1. Prepare the desktop helper and extension");
                    ui.label("From the Rivet project folder, build the native helper and package the extension:");
                    let build = "cargo build --release -p rivet-cli --bins\npython3 scripts/package_browser.py";
                    ui.code(build);
                    if ui.small_button("Copy build commands").clicked() { ctx.copy_text(build.into()); }
                    ui.add_space(8.);
                    ui.strong("2. Load the browser extension");
                    ui.horizontal(|ui| {
                        ui.code(page);
                        if ui.small_button("Copy browser address").clicked() { ctx.copy_text(page.into()); }
                    });
                    ui.label(if firefox {
                        "Choose Load Temporary Add-on and select dist/firefox/manifest.json. Development ID: rivet@rivet.local. Temporary add-ons must be reloaded after Firefox restarts."
                    } else {
                        "Enable Developer mode, choose Load unpacked, and select dist/chromium. Copy the installed Rivet extension ID from its card."
                    });
                    ui.add_space(8.);
                    ui.strong("3. Register the native helper");
                    ui.label("Generate org.rivet.downloads.json using the CLI. Replace the helper path and extension ID with your installed values:");
                    let cli = if cfg!(windows) { ".\\target\\release\\rivet.exe" } else { "target/release/rivet" };
                    let helper = if cfg!(windows) { "C:\\absolute\\path\\rivet-native-host.exe" } else { "/absolute/path/to/rivet-native-host" };
                    let command = format!("{cli} host-manifest --executable \"{helper}\" --extension-id {}{}",
                        if firefox { "rivet@rivet.local" } else { "YOUR_RIVET_EXTENSION_ID" },
                        if firefox { " --firefox" } else { "" });
                    ui.code(&command);
                    if ui.small_button("Copy manifest command").clicked() { ctx.copy_text(command.clone()); }
                    ui.label("Save the command's JSON output as UTF-8 at the location below. The helper path must point to the dedicated rivet-native-host binary.");
                    let location = registration(self.browser_choice);
                    ui.add(egui::Label::new(&location).wrap());
                    if ui.small_button("Copy registration location").clicked() { ctx.copy_text(location); }
                    ui.add_space(8.);
                    ui.strong("4. Check the connection and send a link");
                    ui.label("Reload the extension, open its popup and choose Check connection. Then right-click a download link → Send link to Rivet. Select the queued file here and choose Resume, or Start queue.");
                    ui.separator();
                    ui.label("Chrome / Edge / Chromium use the same extension package, with each installed extension ID registered separately. Safari requires a separate adapter and is not supported yet.");
                    ui.small("Rivet extensions are development packages; store publication and signed installers are pending. Session-authenticated downloads, browser takeover and assembled HLS/DASH video are pending.");
                    ui.collapsing("Troubleshooting", |ui| {
                        ui.label("Host not found: check the manifest location, host name org.rivet.downloads and the browser profile/channel. Host failed to start: verify the absolute helper path and execute permissions. Access forbidden: match the allowlisted ID to this installed Rivet extension. Firefox needs allowed_extensions; Chromium browsers need allowed_origins.");
                        ui.label("The IDM Integration Module connects to IDM. Install Rivet's own extension for Rivet.");
                        ui.label("Full per-OS instructions are in docs/BROWSER.md.");
                    });
                });
            });
        self.show_browser = open;
    }
}
fn registration(browser: usize) -> String {
    let name = "org.rivet.downloads.json";
    if cfg!(windows) {
        let vendor = ["Google\\Chrome", "Microsoft\\Edge", "Chromium", "Mozilla"][browser];
        format!(
            "Save JSON at a permanent absolute path, then set the default value of HKCU\\Software\\{vendor}\\NativeMessagingHosts\\org.rivet.downloads to that JSON path."
        )
    } else if cfg!(target_os = "macos") {
        let folder = ["Google/Chrome", "Microsoft Edge", "Chromium", "Mozilla"][browser];
        format!("~/Library/Application Support/{folder}/NativeMessagingHosts/{name}")
    } else {
        let folder = [
            ".config/google-chrome/NativeMessagingHosts",
            ".config/microsoft-edge/NativeMessagingHosts",
            ".config/chromium/NativeMessagingHosts",
            ".mozilla/native-messaging-hosts",
        ][browser];
        format!("~/{folder}/{name}")
    }
}
