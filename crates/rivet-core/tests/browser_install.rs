use rivet_core::{
    browser_install as install,
    browser_setup::{Browser, Distribution, Platform},
};
use serde_json::json;
use std::path::Path;
use tempfile::TempDir;

#[test]
fn browser_roots_are_platform_specific() {
    let home = Path::new("/home/test");
    assert!(
        install::user_data_root(Platform::Mac, home, Browser::Chrome)
            .ends_with("Library/Application Support/Google/Chrome")
    );
    assert!(
        install::user_data_root(Platform::Windows, home, Browser::Firefox)
            .ends_with("AppData/Roaming/Mozilla/Firefox")
    );
    assert!(
        install::user_data_root(Platform::Linux, home, Browser::Edge)
            .ends_with(".config/microsoft-edge")
    );
}

#[test]
fn chromium_discovery_distinguishes_enabled_disabled_absent_and_unknown_without_writes() {
    let temp = TempDir::new().unwrap();
    let root = temp.path();
    let d = Distribution::bundled().unwrap();
    assert_eq!(
        install::inspect_root(root, Browser::Chrome, &d).profiles_checked,
        0
    );
    let profile = root.join("Default");
    std::fs::create_dir(&profile).unwrap();
    let path = profile.join("Preferences");
    let data = serde_json::to_vec(
        &json!({"extensions":{"settings":{d.chromium_extension_id.clone():{"state":1}}}}),
    )
    .unwrap();
    std::fs::write(&path, &data).unwrap();
    let before = std::fs::metadata(&path).unwrap().modified().unwrap();
    let found = install::inspect_root(root, Browser::Chrome, &d);
    assert_eq!(found.profiles_checked, 1);
    assert_eq!(found.installed[0].enabled, Some(true));
    assert_eq!(std::fs::read(&path).unwrap(), data);
    assert_eq!(
        std::fs::metadata(&path).unwrap().modified().unwrap(),
        before
    );
    std::fs::write(profile.join("Secure Preferences"), serde_json::to_vec(&json!({"extensions":{"settings":{d.chromium_extension_id.clone():{"state":1,"disable_reasons":[1]}}}})).unwrap()).unwrap();
    assert_eq!(
        install::inspect_root(root, Browser::Chrome, &d).installed[0].enabled,
        Some(false)
    );
    std::fs::write(profile.join("Secure Preferences"), serde_json::to_vec(&json!({"extensions":{"settings":{d.chromium_extension_id.clone():{"disable_reasons":[]}}}})).unwrap()).unwrap();
    assert_eq!(
        install::inspect_root(root, Browser::Chrome, &d).installed[0].enabled,
        Some(true),
        "Modern Chromium records enabled state using an empty disable-reasons list"
    );
    std::fs::remove_file(profile.join("Secure Preferences")).unwrap();
    std::fs::write(&path, serde_json::to_vec(&json!({"extensions":{"settings":{},"external_uninstalls":[d.chrome_store_extension_id]}})).unwrap()).unwrap();
    let absent = install::inspect_root(root, Browser::Chrome, &d);
    assert!(absent.installed.is_empty());
    assert!(absent.user_removed);
    assert!(absent.summary().contains("not found"));
    std::fs::write(&path, b"invalid JSON").unwrap();
    let unknown = install::inspect_root(root, Browser::Chrome, &d);
    assert_eq!(unknown.unreadable_profiles, 1);
    assert!(!unknown.summary().contains("not found"));
}

#[test]
fn firefox_profiles_respect_active_state() {
    let temp = TempDir::new().unwrap();
    let d = Distribution::bundled().unwrap();
    let folder = temp.path().join("Profiles/example");
    std::fs::create_dir_all(&folder).unwrap();
    std::fs::write(
        temp.path().join("profiles.ini"),
        "[Profile0]\nIsRelative=1\nPath=Profiles/example\n",
    )
    .unwrap();
    for active in [true, false] {
        std::fs::write(
            folder.join("extensions.json"),
            serde_json::to_vec(&json!({"addons":[{"id":d.firefox_extension_id,"active":active}]}))
                .unwrap(),
        )
        .unwrap();
        let found = install::inspect_root(temp.path(), Browser::Firefox, &d);
        assert_eq!(found.profiles_checked, 1);
        assert_eq!(found.installed[0].enabled, Some(active));
    }
}

#[test]
fn unpublished_and_redirected_store_pages_are_not_installable() {
    let d = Distribution::bundled().unwrap();
    let id = d.chrome_store_extension_id.unwrap();
    let url = format!("https://chromewebstore.google.com/detail/rivet/{id}");
    assert!(install::published_chrome_response(
        200,
        &url,
        "<title>Rivet — Download Manager - Chrome Web Store</title>",
        &id
    ));
    for (status, page, html) in [
        (404, url.clone(), "<title>Rivet</title>"),
        (200, url.clone(), "<title>Chrome Web Store</title>"),
        (
            200,
            format!("https://chromewebstore.google.com/detail/empty-title/{id}"),
            "<title>Rivet</title>",
        ),
        (
            200,
            format!("https://chromewebstore.google.com/detail/{id}/error"),
            "<title>Rivet</title>",
        ),
        (
            200,
            format!("https://example.com/{id}"),
            "<title>Rivet</title>",
        ),
    ] {
        assert!(!install::published_chrome_response(
            status, &page, html, &id
        ));
    }
}

#[test]
fn configured_store_links_are_tied_to_the_known_identity() {
    let mut d = Distribution::bundled().unwrap();
    let id = d.chrome_store_extension_id.clone().unwrap();
    d.chrome_store_url = Some(format!(
        "https://chromewebstore.google.com/detail/rivet/{id}"
    ));
    assert!(install::configured_listing(Browser::Chrome, &d).is_some());
    for url in [
        format!("https://example.com/{id}"),
        "https://chromewebstore.google.com/detail/aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa".into(),
        format!("http://chromewebstore.google.com/detail/{id}"),
    ] {
        d.chrome_store_url = Some(url);
        assert!(install::configured_listing(Browser::Chrome, &d).is_none());
    }
}

#[test]
fn install_requests_use_official_store_metadata_and_are_idempotent() {
    let temp = TempDir::new().unwrap();
    let d = Distribution::bundled().unwrap();
    let id = d.chrome_store_extension_id.unwrap();
    let listing = install::Listing {
        url: format!("https://chromewebstore.google.com/detail/{id}"),
        id: Some(id),
        edge_store: false,
    };
    let path = install::request_install(Platform::Mac, temp.path(), Browser::Chrome, &listing)
        .unwrap()
        .unwrap();
    let content = std::fs::read(&path).unwrap();
    assert_eq!(
        serde_json::from_slice::<serde_json::Value>(&content).unwrap(),
        json!({"external_update_url":"https://clients2.google.com/service/update2/crx"})
    );
    let before = std::fs::metadata(&path).unwrap().modified().unwrap();
    install::request_install(Platform::Mac, temp.path(), Browser::Chrome, &listing).unwrap();
    assert_eq!(
        std::fs::metadata(&path).unwrap().modified().unwrap(),
        before
    );
    assert!(
        install::request_install(Platform::Linux, temp.path(), Browser::Chrome, &listing)
            .unwrap()
            .is_none()
    );
    assert!(
        install::request_install(Platform::Mac, temp.path(), Browser::Firefox, &listing)
            .unwrap()
            .is_none()
    );
}

#[test]
fn stale_receipts_do_not_hide_missing_or_disabled_extensions() {
    let mut report = install::Inspection::default();
    assert!(!install::connection_confirmed(
        &report,
        Some(995),
        1000,
        1005
    ));
    assert!(install::connection_confirmed(
        &report,
        Some(1001),
        1000,
        1005
    ));
    report.installed.push(install::InstalledExtension {
        id: "known".into(),
        enabled: Some(false),
    });
    assert!(!install::connection_confirmed(
        &report,
        Some(995),
        1000,
        1005
    ));
    report.installed[0].enabled = Some(true);
    assert!(install::connection_confirmed(
        &report,
        Some(995),
        1000,
        1005
    ));
    assert!(!install::connection_confirmed(
        &report,
        Some(900),
        1000,
        1005
    ));
    assert!(!install::connection_confirmed(
        &report,
        Some(2000),
        1000,
        1005
    ));
}
