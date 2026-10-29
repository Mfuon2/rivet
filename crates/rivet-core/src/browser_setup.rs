//! Local, per-user browser setup. Never changes browser profiles or enterprise policy.
use anyhow::{Context, Result, bail, ensure};
use serde::{Deserialize, Serialize};
use std::{
    path::{Path, PathBuf},
    process::{Command, Stdio},
    time::{Duration, Instant},
};

const HOST: &str = "org.rivet.downloads";
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Browser {
    Chrome,
    Edge,
    Chromium,
    Firefox,
}
impl Browser {
    pub fn name(self) -> &'static str {
        match self {
            Self::Chrome => "Chrome",
            Self::Edge => "Edge",
            Self::Chromium => "Chromium",
            Self::Firefox => "Firefox",
        }
    }
    pub fn index(self) -> usize {
        match self {
            Self::Chrome => 0,
            Self::Edge => 1,
            Self::Chromium => 2,
            Self::Firefox => 3,
        }
    }
    pub fn extensions_page(self) -> &'static str {
        match self {
            Self::Edge => "edge://extensions",
            Self::Firefox => "about:debugging#/runtime/this-firefox",
            _ => "chrome://extensions",
        }
    }
}
#[derive(Deserialize)]
pub struct Distribution {
    pub chromium_extension_id: String,
    pub chrome_store_extension_id: Option<String>,
    pub edge_extension_id: Option<String>,
    pub firefox_extension_id: String,
    pub chrome_store_url: Option<String>,
    pub edge_store_url: Option<String>,
    pub firefox_install_url: Option<String>,
}
impl Distribution {
    pub fn bundled() -> Result<Self> {
        Ok(serde_json::from_str(include_str!(
            "../../../browser/distribution.json"
        ))?)
    }
    pub fn extension_id(&self, browser: Browser) -> &str {
        if browser == Browser::Firefox {
            &self.firefox_extension_id
        } else if browser == Browser::Edge {
            self.edge_extension_id
                .as_deref()
                .unwrap_or(&self.chromium_extension_id)
        } else {
            &self.chromium_extension_id
        }
    }
    pub fn install_url(&self, browser: Browser) -> Option<&str> {
        match browser {
            Browser::Chrome | Browser::Chromium => self.chrome_store_url.as_deref(),
            Browser::Edge => self.edge_store_url.as_deref(),
            Browser::Firefox => self.firefox_install_url.as_deref(),
        }
    }
    pub fn extension_ids(&self, browser: Browser) -> Vec<&str> {
        let mut ids = vec![self.extension_id(browser)];
        if browser != Browser::Firefox
            && !(browser == Browser::Edge && self.edge_extension_id.is_some())
            && let Some(id) = self.chrome_store_extension_id.as_deref()
            && !ids.contains(&id)
        {
            ids.push(id);
        }
        ids
    }
}
/// Only recognized stable-channel identifiers; unknown browsers are reported, never guessed.
pub fn classify_default(identifier: &str) -> Option<Browser> {
    let id = identifier.trim().to_ascii_lowercase();
    if id == "/applications/google chrome.app"
        || id == "chromehtml"
        || matches!(
            id.as_str(),
            "google-chrome.desktop" | "google-chrome-stable.desktop"
        )
    {
        Some(Browser::Chrome)
    } else if id == "/applications/microsoft edge.app"
        || id == "msedgehtm"
        || id == "microsoft-edge.desktop"
    {
        Some(Browser::Edge)
    } else if id == "/applications/chromium.app"
        || matches!(id.as_str(), "chromium.desktop" | "chromium-browser.desktop")
    {
        Some(Browser::Chromium)
    } else if id == "/applications/firefox.app"
        || id.starts_with("firefoxurl-")
        || id == "firefox.desktop"
    {
        Some(Browser::Firefox)
    } else {
        None
    }
}
fn output(command: &mut Command) -> Result<String> {
    let mut child = command
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()?;
    let start = Instant::now();
    loop {
        if child.try_wait()?.is_some() {
            break;
        }
        if start.elapsed() > Duration::from_secs(5) {
            let _ = child.kill();
            let _ = child.wait();
            bail!("Browser detection timed out");
        }
        std::thread::sleep(Duration::from_millis(20));
    }
    let result = child.wait_with_output()?;
    ensure!(
        result.status.success(),
        "Could not query the default browser"
    );
    Ok(String::from_utf8(result.stdout)?.trim().to_owned())
}
pub fn detect_default() -> Result<(Browser, String)> {
    #[cfg(target_os = "macos")]
    let identifier = output(Command::new("/usr/bin/osascript").args(["-l", "JavaScript", "-e", "ObjC.import('AppKit'); var u = $.NSWorkspace.sharedWorkspace.URLForApplicationToOpenURL($.NSURL.URLWithString('https://example.com')); u.isNil() ? '' : u.path.js"]))?;
    #[cfg(target_os = "windows")]
    let identifier = {
        let raw = output(Command::new("reg.exe").args([
            "query",
            r"HKCU\Software\Microsoft\Windows\Shell\Associations\UrlAssociations\https\UserChoice",
            "/v",
            "ProgId",
        ]))?;
        raw.lines()
            .find(|s| s.split_whitespace().next() == Some("ProgId"))
            .and_then(|s| s.split_whitespace().last())
            .context("No default HTTPS browser")?
            .to_owned()
    };
    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    let identifier = output(Command::new("xdg-settings").args(["get", "default-web-browser"]))?;
    let browser = classify_default(&identifier)
        .with_context(|| format!("Default browser is not supported yet: {identifier}"))?;
    Ok((browser, identifier))
}

#[derive(Clone, Copy)]
pub enum Platform {
    Mac,
    Windows,
    Linux,
}
impl Platform {
    pub fn current() -> Self {
        if cfg!(windows) {
            Self::Windows
        } else if cfg!(target_os = "macos") {
            Self::Mac
        } else {
            Self::Linux
        }
    }
}
pub fn manifest_path(platform: Platform, home: &Path, data: &Path, browser: Browser) -> PathBuf {
    let folder = match platform {
        Platform::Mac => home
            .join("Library/Application Support")
            .join(match browser {
                Browser::Chrome => "Google/Chrome",
                Browser::Edge => "Microsoft Edge",
                Browser::Chromium => "Chromium",
                Browser::Firefox => "Mozilla",
            })
            .join("NativeMessagingHosts"),
        Platform::Linux => home.join(match browser {
            Browser::Chrome => ".config/google-chrome/NativeMessagingHosts",
            Browser::Edge => ".config/microsoft-edge/NativeMessagingHosts",
            Browser::Chromium => ".config/chromium/NativeMessagingHosts",
            Browser::Firefox => ".mozilla/native-messaging-hosts",
        }),
        Platform::Windows => data.join("browser-hosts").join(browser.name()),
    };
    folder.join(format!("{HOST}.json"))
}
/// Idempotent publication: matching files are not rewritten; interrupted writes never publish partial JSON.
fn write_if_changed(path: &Path, bytes: &[u8]) -> Result<bool> {
    if std::fs::read(path).ok().as_deref() == Some(bytes) {
        return Ok(false);
    }
    let parent = path.parent().context("No destination directory")?;
    std::fs::create_dir_all(parent)?;
    let temp = parent.join(format!(".rivet-{}.tmp", uuid::Uuid::new_v4()));
    let result = (|| {
        use std::io::Write;
        let mut options = std::fs::OpenOptions::new();
        options.create_new(true).write(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        let mut file = options.open(&temp)?;
        file.write_all(bytes)?;
        file.sync_all()?;
        #[cfg(windows)]
        if path.exists() {
            std::fs::remove_file(path)?;
        }
        std::fs::rename(&temp, path)?;
        Ok(true)
    })();
    if result.is_err() {
        let _ = std::fs::remove_file(temp);
    }
    result
}
/// Writes only Rivet's native-host registration. Extension installation remains browser-controlled.
pub fn register(
    platform: Platform,
    home: &Path,
    data: &Path,
    browser: Browser,
    helper: &Path,
    distribution: &Distribution,
) -> Result<PathBuf> {
    ensure!(
        helper.is_file(),
        "Rivet native helper is missing. Reinstall Rivet or build the complete workspace."
    );
    let helper = helper.canonicalize()?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        ensure!(
            std::fs::metadata(&helper)?.permissions().mode() & 0o111 != 0,
            "Native helper is not executable"
        );
    }
    let ids = distribution.extension_ids(browser);
    if browser != Browser::Firefox {
        ensure!(
            ids.iter()
                .all(|id| id.len() == 32 && id.bytes().all(|c| (b'a'..=b'p').contains(&c))),
            "Invalid Chromium extension ID"
        );
    } else {
        ensure!(
            ids.iter().all(|id| !id.is_empty() && id.len() <= 255),
            "Invalid Firefox extension ID"
        );
    }
    let mut manifest = serde_json::json!({"name":HOST,"description":"Rivet download queue","path":helper,"type":"stdio"});
    manifest[if browser == Browser::Firefox {
        "allowed_extensions"
    } else {
        "allowed_origins"
    }] = if browser == Browser::Firefox {
        serde_json::json!(ids)
    } else {
        serde_json::json!(
            ids.iter()
                .map(|id| format!("chrome-extension://{id}/"))
                .collect::<Vec<_>>()
        )
    };
    let path = manifest_path(platform, home, data, browser);
    write_if_changed(&path, &serde_json::to_vec_pretty(&manifest)?)?;
    #[cfg(windows)]
    if matches!(platform, Platform::Windows) {
        let vendor = match browser {
            Browser::Chrome => r"Google\Chrome",
            Browser::Edge => r"Microsoft\Edge",
            Browser::Chromium => "Chromium",
            Browser::Firefox => "Mozilla",
        };
        output(
            Command::new("reg.exe")
                .args([
                    "add",
                    &format!(r"HKCU\Software\{vendor}\NativeMessagingHosts\{HOST}"),
                    "/ve",
                    "/t",
                    "REG_SZ",
                    "/d",
                ])
                .arg(&path)
                .arg("/f"),
        )?;
    }
    Ok(path)
}
const ASSETS: [(&str, &[u8]); 11] = [
    (
        "media-widget.js",
        include_bytes!("../../../browser/shared/media-widget.js"),
    ),
    (
        "background.js",
        include_bytes!("../../../browser/shared/background.js"),
    ),
    (
        "popup.html",
        include_bytes!("../../../browser/shared/popup.html"),
    ),
    (
        "popup.js",
        include_bytes!("../../../browser/shared/popup.js"),
    ),
    (
        "popup.css",
        include_bytes!("../../../browser/shared/popup.css"),
    ),
    ("manifest.json", b""),
    (
        "icons/icon-16.png",
        include_bytes!("../../../assets/branding/icons/icon-16.png"),
    ),
    (
        "icons/icon-32.png",
        include_bytes!("../../../assets/branding/icons/icon-32.png"),
    ),
    (
        "icons/icon-48.png",
        include_bytes!("../../../assets/branding/icons/icon-48.png"),
    ),
    (
        "icons/icon-64.png",
        include_bytes!("../../../assets/branding/icons/icon-64.png"),
    ),
    (
        "icons/icon-128.png",
        include_bytes!("../../../assets/branding/icons/icon-128.png"),
    ),
];
pub fn prepare_extension(data: &Path, browser: Browser) -> Result<PathBuf> {
    let firefox = browser == Browser::Firefox;
    let path = data
        .join("browser-extensions")
        .join(if firefox { "firefox" } else { "chromium" });
    for (name, bytes) in ASSETS {
        let bytes = if name == "manifest.json" {
            if firefox {
                include_bytes!("../../../browser/firefox/manifest.json").as_slice()
            } else {
                include_bytes!("../../../browser/chromium/manifest.json").as_slice()
            }
        } else {
            bytes
        };
        write_if_changed(&path.join(name), bytes)?;
    }
    Ok(path)
}
#[derive(Serialize, Deserialize)]
pub struct ConnectionReceipt {
    pub extension_id: String,
    pub time: i64,
}
pub fn record_connection(database: &Path, arguments: &[String]) -> Result<()> {
    let distribution = Distribution::bundled()?;
    let id = [Browser::Chrome, Browser::Edge, Browser::Firefox]
        .into_iter()
        .find_map(|browser| {
            distribution.extension_ids(browser).into_iter().find(|id| {
                let expected = if browser == Browser::Firefox {
                    id.to_string()
                } else {
                    format!("chrome-extension://{id}/")
                };
                arguments.iter().any(|v| v == &expected)
            })
        });
    let Some(id) = id else {
        return Ok(());
    };
    let parent = database.parent().context("Missing data folder")?;
    write_if_changed(
        &parent.join("browser-connection.json"),
        &serde_json::to_vec(&ConnectionReceipt {
            extension_id: id.to_owned(),
            time: crate::queue::now(),
        })?,
    )?;
    Ok(())
}
pub fn last_connection(data: &Path, browser: Browser, distribution: &Distribution) -> Option<i64> {
    let receipt: ConnectionReceipt =
        serde_json::from_slice(&std::fs::read(data.join("browser-connection.json")).ok()?).ok()?;
    distribution
        .extension_ids(browser)
        .contains(&receipt.extension_id.as_str())
        .then_some(receipt.time)
}
