use rivet_core::{
    browser_install,
    browser_setup::{self, Browser},
};
use std::{collections::BTreeMap, io::Write, path::Path};
use zip::write::SimpleFileOptions;
fn files(root: &Path) -> BTreeMap<String, Vec<u8>> {
    let prepared = browser_setup::prepare_extension(root, Browser::Chrome).unwrap();
    let mut entries = BTreeMap::new();
    for path in [prepared.clone(), prepared.join("icons")] {
        for entry in std::fs::read_dir(path).unwrap() {
            let p = entry.unwrap().path();
            if p.is_file() && p.file_name().is_some_and(|n| n != "segoeui.woff") {
                entries.insert(
                    p.strip_prefix(&prepared)
                        .unwrap()
                        .to_string_lossy()
                        .replace('\\', "/"),
                    std::fs::read(p).unwrap(),
                );
            }
        }
    }
    entries
}
fn archive(path: &Path, entries: &BTreeMap<String, Vec<u8>>) {
    let mut zip = zip::ZipWriter::new(std::fs::File::create(path).unwrap());
    for (name, bytes) in entries {
        zip.start_file(
            name,
            SimpleFileOptions::default().compression_method(zip::CompressionMethod::Stored),
        )
        .unwrap();
        zip.write_all(bytes).unwrap();
    }
    zip.finish().unwrap();
}
#[test]
fn complete_archive_preserves_manifest_identity_and_bundled_fonts() {
    let temp = tempfile::TempDir::new().unwrap();
    let entries = files(temp.path());
    let path = temp.path().join("package.zip");
    archive(&path, &entries);
    let verified = browser_install::validated_extension_archive(
        &path,
        Browser::Chrome,
        env!("CARGO_PKG_VERSION"),
    )
    .unwrap();
    assert_eq!(verified, entries);
    assert!(verified["Poppins-Regular.ttf"].len() > 100_000);
    assert!(verified.contains_key("POPPINS-LICENSE.txt"));
    assert!(
        browser_install::validated_extension_archive(&path, Browser::Chrome, "99.99.99").is_err()
    );
    assert!(
        browser_install::validated_extension_archive(
            &path,
            Browser::Firefox,
            env!("CARGO_PKG_VERSION")
        )
        .is_err()
    );
}
#[test]
fn unsafe_paths_missing_files_and_replaced_identities_are_rejected_before_activation() {
    let temp = tempfile::TempDir::new().unwrap();
    let original = files(temp.path());
    let path = temp.path().join("package.zip");
    for name in ["../outside.js", "/outside.js", "unrelated.js"] {
        let mut entries = original.clone();
        let bytes = entries.remove("background.js").unwrap();
        entries.insert(name.into(), bytes);
        archive(&path, &entries);
        assert!(
            browser_install::validated_extension_archive(
                &path,
                Browser::Chrome,
                env!("CARGO_PKG_VERSION")
            )
            .is_err()
        );
    }
    let mut entries = original.clone();
    entries.remove("setup.js");
    archive(&path, &entries);
    assert!(
        browser_install::validated_extension_archive(
            &path,
            Browser::Chrome,
            env!("CARGO_PKG_VERSION")
        )
        .is_err()
    );
    let mut entries = original;
    let mut m: serde_json::Value = serde_json::from_slice(&entries["manifest.json"]).unwrap();
    m["key"] = "different identity".into();
    entries.insert("manifest.json".into(), serde_json::to_vec(&m).unwrap());
    archive(&path, &entries);
    assert!(
        browser_install::validated_extension_archive(
            &path,
            Browser::Chrome,
            env!("CARGO_PKG_VERSION")
        )
        .is_err()
    );
    assert!(!temp.path().join("outside.js").exists());
}
#[test]
fn symbolic_links_are_never_activated() {
    let temp = tempfile::TempDir::new().unwrap();
    let entries = files(temp.path());
    let path = temp.path().join("package.zip");
    let mut zip = zip::ZipWriter::new(std::fs::File::create(&path).unwrap());
    for (name, bytes) in entries {
        if name == "background.js" {
            zip.add_symlink(name, "../../outside.js", SimpleFileOptions::default())
                .unwrap();
        } else {
            zip.start_file(name, SimpleFileOptions::default()).unwrap();
            zip.write_all(&bytes).unwrap();
        }
    }
    zip.finish().unwrap();
    assert!(
        browser_install::validated_extension_archive(
            &path,
            Browser::Chrome,
            env!("CARGO_PKG_VERSION")
        )
        .is_err()
    );
}
