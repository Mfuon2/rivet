use rivet_core::browser_setup::{self as setup, Browser, Distribution, Platform};
use tempfile::TempDir;

fn helper(dir: &TempDir) -> std::path::PathBuf {
    let path = dir.path().join("rivet-native-host");
    std::fs::write(&path, b"test fixture; never executed").unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o700)).unwrap();
    }
    path
}
#[test]
fn default_browser_detection_is_conservative() {
    for (id, browser) in [
        ("/Applications/Google Chrome.app", Browser::Chrome),
        ("MSEdgeHTM", Browser::Edge),
        ("chromium.desktop", Browser::Chromium),
        ("FirefoxURL-ABC123", Browser::Firefox),
    ] {
        assert_eq!(setup::classify_default(id), Some(browser));
    }
    for id in [
        "/Applications/Safari.app",
        "google-chrome-beta.desktop",
        "untrusted-google-chrome.desktop",
        "",
    ] {
        assert!(setup::classify_default(id).is_none());
    }
}
#[test]
fn browser_paths_are_separate_on_every_platform() {
    let home = std::path::Path::new("/test-home");
    let data = std::path::Path::new("/test-data");
    assert!(setup::manifest_path(Platform::Mac,home,data,Browser::Edge).ends_with("Library/Application Support/Microsoft Edge/NativeMessagingHosts/org.rivet.downloads.json"));
    assert!(
        setup::manifest_path(Platform::Linux, home, data, Browser::Firefox)
            .ends_with(".mozilla/native-messaging-hosts/org.rivet.downloads.json")
    );
    assert!(
        setup::manifest_path(Platform::Windows, home, data, Browser::Chrome)
            .ends_with("browser-hosts/Chrome/org.rivet.downloads.json")
    );
}
#[test]
fn registration_repairs_itself_without_rewriting_correct_files() {
    let dir = TempDir::new().unwrap();
    let helper = helper(&dir);
    let home = dir.path().join("home");
    let data = dir.path().join("data");
    let distribution = Distribution::bundled().unwrap();
    let path = setup::register(
        Platform::Linux,
        &home,
        &data,
        Browser::Chrome,
        &helper,
        &distribution,
    )
    .unwrap();
    let first = std::fs::read(&path).unwrap();
    let modified = std::fs::metadata(&path).unwrap().modified().unwrap();
    setup::register(
        Platform::Linux,
        &home,
        &data,
        Browser::Chrome,
        &helper,
        &distribution,
    )
    .unwrap();
    assert_eq!(
        modified,
        std::fs::metadata(&path).unwrap().modified().unwrap()
    );
    let manifest: serde_json::Value = serde_json::from_slice(&first).unwrap();
    assert_eq!(
        manifest["allowed_origins"],
        serde_json::json!(
            distribution
                .extension_ids(Browser::Chrome)
                .iter()
                .map(|id| format!("chrome-extension://{id}/"))
                .collect::<Vec<_>>()
        )
    );
    assert!(manifest.get("allowed_extensions").is_none());
    std::fs::write(&path, b"broken json").unwrap();
    setup::register(
        Platform::Linux,
        &home,
        &data,
        Browser::Chrome,
        &helper,
        &distribution,
    )
    .unwrap();
    assert_eq!(std::fs::read(&path).unwrap(), first);
    let firefox = setup::register(
        Platform::Linux,
        &home,
        &data,
        Browser::Firefox,
        &helper,
        &distribution,
    )
    .unwrap();
    let manifest: serde_json::Value =
        serde_json::from_slice(&std::fs::read(firefox).unwrap()).unwrap();
    assert_eq!(
        manifest["allowed_extensions"],
        serde_json::json!([distribution.firefox_extension_id])
    );
    assert!(manifest.get("allowed_origins").is_none());
}
#[test]
fn missing_helpers_do_not_publish_a_working_registration() {
    let dir = TempDir::new().unwrap();
    let distribution = Distribution::bundled().unwrap();
    assert!(
        setup::register(
            Platform::Linux,
            dir.path(),
            dir.path(),
            Browser::Chrome,
            &dir.path().join("missing"),
            &distribution
        )
        .is_err()
    );
    assert!(
        !setup::manifest_path(Platform::Linux, dir.path(), dir.path(), Browser::Chrome).exists()
    );
}
#[test]
fn prepared_packages_use_fixed_ids_and_connection_records_are_bounded() {
    let dir = TempDir::new().unwrap();
    let distribution = Distribution::bundled().unwrap();
    let chromium = setup::prepare_extension(dir.path(), Browser::Edge).unwrap();
    let firefox = setup::prepare_extension(dir.path(), Browser::Firefox).unwrap();
    let chromium_manifest: serde_json::Value =
        serde_json::from_slice(&std::fs::read(chromium.join("manifest.json")).unwrap()).unwrap();
    assert!(chromium_manifest["key"].as_str().unwrap().len() > 100);
    assert!(firefox.join("background.js").is_file());
    for folder in [&chromium, &firefox] {
        for name in ["setup.html", "setup.js", "setup.css"] {
            assert!(
                folder.join(name).is_file(),
                "Automatic connection page missing from the prepared extension"
            );
        }
        assert_eq!(
            std::fs::read(folder.join("media-widget.js")).unwrap(),
            include_bytes!("../../../browser/shared/media-widget.js")
        );
        let manifest: serde_json::Value =
            serde_json::from_slice(&std::fs::read(folder.join("manifest.json")).unwrap()).unwrap();
        assert!(
            manifest["permissions"]
                .as_array()
                .unwrap()
                .iter()
                .any(|p| p == "scripting")
        );
    }
    let db = dir.path().join("queue.sqlite3");
    setup::record_connection(&db, &["https://untrusted.example".into()]).unwrap();
    assert!(setup::last_connection(dir.path(), Browser::Chrome, &distribution).is_none());
    setup::record_connection(
        &db,
        &[format!(
            "chrome-extension://{}/",
            distribution.chromium_extension_id
        )],
    )
    .unwrap();
    assert!(setup::last_connection(dir.path(), Browser::Chrome, &distribution).is_some());
    assert!(setup::last_connection(dir.path(), Browser::Firefox, &distribution).is_none());
    let record = std::fs::read_to_string(dir.path().join("browser-connection.json")).unwrap();
    assert!(!record.contains("untrusted"));
    let store_id = distribution.chrome_store_extension_id.as_deref().unwrap();
    setup::record_connection(&db, &[format!("chrome-extension://{store_id}/")]).unwrap();
    assert!(setup::last_connection(dir.path(), Browser::Chrome, &distribution).is_some());
    assert!(setup::last_connection(dir.path(), Browser::Firefox, &distribution).is_none());
    let record = std::fs::read_to_string(dir.path().join("browser-connection.json")).unwrap();
    assert!(record.contains(store_id));
}
