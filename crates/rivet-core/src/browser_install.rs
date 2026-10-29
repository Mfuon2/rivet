//! Read-only extension discovery and browser-approved store installation requests.
use crate::browser_setup::{self, Browser, Distribution, Platform};
use anyhow::{Context, Result, ensure};
use serde_json::Value;
#[cfg(windows)]
use std::process::Stdio;
use std::{
    path::{Path, PathBuf},
    process::Command,
    time::Duration,
};

#[derive(Clone, Debug)]
pub struct InstalledExtension {
    pub id: String,
    pub enabled: Option<bool>,
}
#[derive(Default, Debug)]
pub struct Inspection {
    pub installed: Vec<InstalledExtension>,
    pub profiles_checked: usize,
    pub unreadable_profiles: usize,
    pub user_removed: bool,
}
impl Inspection {
    pub fn summary(&self) -> &'static str {
        if self.installed.iter().any(|e| e.enabled == Some(true)) {
            "Extension found in your browser."
        } else if self.installed.iter().any(|e| e.enabled == Some(false)) {
            "Extension found, but it is turned off in your browser."
        } else if !self.installed.is_empty() {
            "Extension files found. Waiting for the browser to confirm the connection."
        } else if self.profiles_checked > 0 && self.unreadable_profiles == 0 {
            "Rivet extension was not found in the checked browser profiles."
        } else {
            "Extension connection has not been confirmed. Custom browser profiles may need a connection check."
        }
    }
}
fn json_file(path: &Path) -> Option<Value> {
    if std::fs::metadata(path).ok()?.len() > 32 * 1024 * 1024 {
        return None;
    }
    serde_json::from_slice(&std::fs::read(path).ok()?).ok()
}
pub fn user_data_root(platform: Platform, home: &Path, browser: Browser) -> PathBuf {
    match platform {
        Platform::Mac => home
            .join("Library/Application Support")
            .join(match browser {
                Browser::Chrome => "Google/Chrome",
                Browser::Edge => "Microsoft Edge",
                Browser::Chromium => "Chromium",
                Browser::Firefox => "Firefox",
            }),
        Platform::Windows => home
            .join(if browser == Browser::Firefox {
                "AppData/Roaming"
            } else {
                "AppData/Local"
            })
            .join(match browser {
                Browser::Chrome => "Google/Chrome/User Data",
                Browser::Edge => "Microsoft/Edge/User Data",
                Browser::Chromium => "Chromium/User Data",
                Browser::Firefox => "Mozilla/Firefox",
            }),
        Platform::Linux => home.join(match browser {
            Browser::Chrome => ".config/google-chrome",
            Browser::Edge => ".config/microsoft-edge",
            Browser::Chromium => ".config/chromium",
            Browser::Firefox => ".mozilla/firefox",
        }),
    }
}
pub fn inspect(
    platform: Platform,
    home: &Path,
    browser: Browser,
    distribution: &Distribution,
) -> Inspection {
    inspect_root(
        &user_data_root(platform, home, browser),
        browser,
        distribution,
    )
}
/// Accepts an explicit root for isolated browser-profile checks and tests.
pub fn inspect_root(root: &Path, browser: Browser, distribution: &Distribution) -> Inspection {
    let mut report = Inspection::default();
    if browser == Browser::Firefox {
        let mut profiles = Vec::new();
        if let Ok(text) = std::fs::read_to_string(root.join("profiles.ini")) {
            let mut path = None;
            let mut relative = true;
            for line in text.lines().chain(["[end]"]) {
                let line = line.trim();
                if line.starts_with('[') {
                    if let Some(path) = path.take() {
                        profiles.push(if relative {
                            root.join(path)
                        } else {
                            PathBuf::from(path)
                        });
                    }
                    relative = true;
                } else if let Some(value) = line.strip_prefix("Path=") {
                    path = Some(value.to_owned());
                } else if let Some(value) = line.strip_prefix("IsRelative=") {
                    relative = value != "0";
                }
            }
        }
        for profile in profiles {
            let Some(value) = json_file(&profile.join("extensions.json")) else {
                report.unreadable_profiles += 1;
                continue;
            };
            report.profiles_checked += 1;
            if let Some(addons) = value["addons"].as_array() {
                for addon in addons
                    .iter()
                    .filter(|a| a["id"].as_str() == Some(&distribution.firefox_extension_id))
                {
                    report.installed.push(InstalledExtension {
                        id: distribution.firefox_extension_id.clone(),
                        enabled: addon["active"].as_bool(),
                    });
                }
            }
        }
        return report;
    }
    let Ok(entries) = std::fs::read_dir(root) else {
        return report;
    };
    let ids = distribution.extension_ids(browser);
    for entry in entries.flatten() {
        let profile = entry.path();
        if !profile.is_dir()
            || !(profile.join("Preferences").is_file()
                || profile.join("Secure Preferences").is_file())
        {
            continue;
        }
        let mut readable = false;
        let mut found = std::collections::BTreeMap::new();
        for name in ["Preferences", "Secure Preferences"] {
            let path = profile.join(name);
            if !path.exists() {
                continue;
            }
            let Some(value) = json_file(&path) else {
                report.unreadable_profiles += 1;
                continue;
            };
            readable = true;
            if let Some(removed) = value
                .pointer("/extensions/external_uninstalls")
                .and_then(Value::as_array)
            {
                report.user_removed |= removed
                    .iter()
                    .filter_map(Value::as_str)
                    .any(|id| ids.contains(&id));
            }
            for id in &ids {
                if let Some(settings) = value
                    .pointer("/extensions/settings")
                    .and_then(|v| v.get(*id))
                {
                    let disabled = settings["disable_reasons"].as_u64().is_some_and(|v| v > 0)
                        || settings["disable_reasons"]
                            .as_array()
                            .is_some_and(|v| !v.is_empty());
                    let enabled = if disabled {
                        Some(false)
                    } else {
                        match settings["state"].as_u64() {
                            Some(1) => Some(true),
                            Some(0 | 2 | 3 | 4) => Some(false),
                            None if settings["disable_reasons"]
                                .as_array()
                                .is_some_and(|v| v.is_empty())
                                || settings["disable_reasons"].as_u64() == Some(0) =>
                            {
                                Some(true)
                            }
                            _ => None,
                        }
                    };
                    found.insert((*id).to_owned(), enabled);
                }
            }
        }
        if readable {
            report.profiles_checked += 1;
        }
        for id in &ids {
            if !found.contains_key(*id) && profile.join("Extensions").join(id).is_dir() {
                found.insert((*id).to_owned(), None);
            }
        }
        report.installed.extend(
            found
                .into_iter()
                .map(|(id, enabled)| InstalledExtension { id, enabled }),
        );
    }
    report
}

#[derive(Clone, Debug)]
pub struct Listing {
    pub url: String,
    pub id: Option<String>,
    pub edge_store: bool,
}
fn chromium_id(id: &str) -> bool {
    id.len() == 32 && id.bytes().all(|c| (b'a'..=b'p').contains(&c))
}
pub fn configured_listing(browser: Browser, distribution: &Distribution) -> Option<Listing> {
    let raw = distribution.install_url(browser)?;
    let url = reqwest::Url::parse(raw).ok()?;
    if url.scheme() != "https" {
        return None;
    }
    let host = url.host_str()?;
    let edge_store = host == "microsoftedge.microsoft.com" || host == "edge.microsoft.com";
    let id = match browser {
        Browser::Firefox if host == "addons.mozilla.org" => None,
        Browser::Edge if edge_store => distribution.edge_extension_id.clone(),
        Browser::Chrome | Browser::Chromium | Browser::Edge
            if host == "chromewebstore.google.com" =>
        {
            distribution.chrome_store_extension_id.clone()
        }
        _ => return None,
    };
    if browser != Browser::Firefox
        && !id.as_deref().is_some_and(|id| {
            chromium_id(id)
                && url
                    .path_segments()
                    .is_some_and(|mut parts| parts.any(|p| p == id))
        })
    {
        return None;
    }
    Some(Listing {
        url: raw.to_owned(),
        id,
        edge_store,
    })
}
pub fn published_chrome_response(status: u16, url: &str, html: &str, id: &str) -> bool {
    let Ok(url) = reqwest::Url::parse(url) else {
        return false;
    };
    status == 200
        && chromium_id(id)
        && url.scheme() == "https"
        && url.host_str() == Some("chromewebstore.google.com")
        && url
            .path_segments()
            .is_some_and(|mut parts| parts.any(|p| p == id))
        && !url.path().contains("/empty-title/")
        && !url.path().ends_with("/error")
        && html
            .split("<title>")
            .nth(1)
            .and_then(|s| s.split("</title>").next())
            .is_some_and(|title| title.contains("Rivet"))
}
/// Probes only the known public store item, never browsing history or download URLs.
pub async fn resolve_listing(
    browser: Browser,
    distribution: &Distribution,
) -> Result<Option<Listing>> {
    if let Some(listing) = configured_listing(browser, distribution) {
        return Ok(Some(listing));
    }
    if browser == Browser::Firefox
        || (browser == Browser::Edge && distribution.edge_extension_id.is_some())
    {
        return Ok(None);
    }
    let Some(id) = &distribution.chrome_store_extension_id else {
        return Ok(None);
    };
    ensure!(chromium_id(id), "Invalid store item ID");
    let url = format!("https://chromewebstore.google.com/detail/{id}");
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(6))
        .redirect(reqwest::redirect::Policy::limited(4))
        .build()?;
    let mut response = client.get(&url).send().await?;
    let status = response.status().as_u16();
    let final_url = response.url().to_string();
    let mut data = Vec::new();
    while let Some(chunk) = response.chunk().await? {
        let remaining = (128 * 1024usize).saturating_sub(data.len());
        data.extend_from_slice(&chunk[..chunk.len().min(remaining)]);
        if data.len() == 128 * 1024 {
            break;
        }
    }
    Ok(
        published_chrome_response(status, &final_url, &String::from_utf8_lossy(&data), id).then(
            || Listing {
                url,
                id: Some(id.clone()),
                edge_store: false,
            },
        ),
    )
}

/// Standard external-install metadata. The browser still enforces installation approval.
/// Linux uses the store page because Chrome's supported external directories are system-wide.
pub fn request_install(
    platform: Platform,
    home: &Path,
    browser: Browser,
    listing: &Listing,
) -> Result<Option<PathBuf>> {
    if browser == Browser::Firefox {
        return Ok(None);
    }
    let Some(id) = listing.id.as_deref() else {
        return Ok(None);
    };
    ensure!(chromium_id(id), "Invalid store extension ID");
    let update = if listing.edge_store {
        "https://edge.microsoft.com/extensionwebstorebase/v1/crx"
    } else {
        "https://clients2.google.com/service/update2/crx"
    };
    match platform {
        Platform::Mac => {
            let path = user_data_root(platform, home, browser)
                .join("External Extensions")
                .join(format!("{id}.json"));
            browser_setup::write_if_changed(
                &path,
                &serde_json::to_vec_pretty(&serde_json::json!({"external_update_url":update}))?,
            )?;
            Ok(Some(path))
        }
        Platform::Windows => {
            #[cfg(windows)]
            {
                let vendor = match browser {
                    Browser::Chrome => "Google\\Chrome",
                    Browser::Edge => "Microsoft\\Edge",
                    Browser::Chromium => "Chromium",
                    Browser::Firefox => return Ok(None),
                };
                let key = format!("HKCU\\Software\\{vendor}\\Extensions\\{id}");
                let status = Command::new("reg.exe")
                    .args([
                        "add",
                        &key,
                        "/v",
                        "update_url",
                        "/t",
                        "REG_SZ",
                        "/d",
                        update,
                        "/f",
                    ])
                    .stdout(Stdio::null())
                    .stderr(Stdio::null())
                    .status()?;
                ensure!(
                    status.success(),
                    "Could not register the browser install request"
                );
            }
            Ok(None)
        }
        Platform::Linux => Ok(None),
    }
}

pub fn open_browser(browser: Browser, url: &str) -> Result<()> {
    ensure!(
        url.starts_with("https://") || url == browser.extensions_page() || url == "about:addons",
        "Unsupported browser setup address"
    );
    #[cfg(target_os = "macos")]
    {
        let app = match browser {
            Browser::Chrome => "Google Chrome",
            Browser::Edge => "Microsoft Edge",
            Browser::Chromium => "Chromium",
            Browser::Firefox => "Firefox",
        };
        let status = Command::new("/usr/bin/open")
            .args(["-a", app, url])
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .status()
            .context("Could not open the browser")?;
        ensure!(status.success(), "Could not open this browser");
    }
    #[cfg(windows)]
    {
        let suffix = match browser {
            Browser::Chrome => "Google/Chrome/Application/chrome.exe",
            Browser::Edge => "Microsoft/Edge/Application/msedge.exe",
            Browser::Chromium => "Chromium/Application/chrome.exe",
            Browser::Firefox => "Mozilla Firefox/firefox.exe",
        };
        let path = ["LOCALAPPDATA", "ProgramFiles", "ProgramFiles(x86)"]
            .into_iter()
            .filter_map(std::env::var_os)
            .map(|base| PathBuf::from(base).join(suffix))
            .find(|p| p.is_file())
            .context("Could not locate this browser")?;
        Command::new(path)
            .args([
                if browser == Browser::Firefox {
                    "-new-tab"
                } else {
                    "--new-tab"
                },
                url,
            ])
            .spawn()?;
    }
    #[cfg(not(any(target_os = "macos", windows)))]
    {
        let names: &[&str] = match browser {
            Browser::Chrome => &["google-chrome", "google-chrome-stable"],
            Browser::Edge => &["microsoft-edge", "microsoft-edge-stable"],
            Browser::Chromium => &["chromium", "chromium-browser"],
            Browser::Firefox => &["firefox"],
        };
        let mut opened = false;
        for name in names {
            if Command::new(name)
                .args([
                    if browser == Browser::Firefox {
                        "-new-tab"
                    } else {
                        "--new-tab"
                    },
                    url,
                ])
                .spawn()
                .is_ok()
            {
                opened = true;
                break;
            }
        }
        ensure!(opened, "Could not open this browser");
    }
    Ok(())
}
pub fn open_folder(path: &Path) -> Result<()> {
    ensure!(path.is_dir(), "The prepared extension folder is missing");
    let program = if cfg!(target_os = "macos") {
        "open"
    } else if cfg!(windows) {
        "explorer.exe"
    } else {
        "xdg-open"
    };
    Command::new(program).arg(path).spawn()?;
    Ok(())
}
pub fn recent_connection(
    data: &Path,
    browser: Browser,
    distribution: &Distribution,
    now: i64,
) -> Option<i64> {
    browser_setup::last_connection(data, browser, distribution)
        .filter(|time| *time <= now + 5 && now.saturating_sub(*time) <= 90)
}
pub fn connection_confirmed(
    inspection: &Inspection,
    receipt: Option<i64>,
    started: i64,
    now: i64,
) -> bool {
    receipt.is_some_and(|time| {
        time <= now + 5
            && now.saturating_sub(time) <= 90
            && (time >= started || inspection.installed.iter().any(|e| e.enabled == Some(true)))
    })
}
